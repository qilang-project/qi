//! 可增长数组：`数组<T>()` 空数组 + `甲.添加(x)` / `甲.弹出()`。
//!
//! 布局不变（`[len, e0, e1, …]`，每槽 8 字节）。对象 header 的 size 就是容量，
//! 所以不用给数组加字段，现有的下标、`.长度`、`对于`、释放函数全都照旧。
//!
//! 「能不能原地改」交给运行时 `qi_array_prepare_mut`：唯一持有就原地（不够就
//! realloc 扩容，翻倍增长），被共享就写时复制。它可能换掉数组指针，所以
//! **接收者必须是能写回的位置**（变量 / 结构体字段），返回的新指针原样写回 ——
//! 旧指针的那份引用已经被运行时转给了新指针（realloc 搬家 / 复制后减一），
//! 这里不再 retain/release 它。
//!
//! 元素本身的读写和 ARC 仍在这里做：添加 = 新值按「存入槽」规矩 retain 后写进
//! 第 len 号元素槽；弹出 = 读出末槽、把槽清零（释放函数不会再放它），值的那份
//! 引用随返回值移交（所有权判定侧把 `甲.弹出()` 判成 OWNED）。

use super::后端;
use super::所有权::是RC类型;
use super::类型::Qi类型;
use crate::parser::ast::{AstNode, LiteralExpression, LiteralValue, ThrowStatement};
use inkwell::values::{BasicValueEnum, PointerValue};
use inkwell::AddressSpace;

impl<'ctx> 后端<'ctx> {
    /// 接收者是数组时的内建方法分发。不是 添加/弹出 → Ok(None)，交回调用方继续。
    pub(super) fn 尝试数组方法(
        &mut self,
        recv: &AstNode,
        method: &str,
        args: &[AstNode],
    ) -> Result<Option<Option<(BasicValueEnum<'ctx>, Qi类型)>>, String> {
        if method != "添加" && method != "弹出" {
            return Ok(None);
        }
        let rt = super::类型检查::推断表达式类型(recv, &self.符号);
        let Some(元素) = rt.数组元素() else {
            return Ok(None);
        };
        let 元素qi = 元素.标量();
        if method == "添加" {
            if args.len() != 1 {
                return Err(format!(
                    "数组的 添加 要 1 个参数（要追加的元素），这里给了 {} 个",
                    args.len()
                ));
            }
            self.生成数组添加(recv, 元素qi, &args[0])?;
            Ok(Some(None))
        } else {
            if !args.is_empty() {
                return Err(format!(
                    "数组的 弹出 不带参数（总是取走最后一个），这里给了 {} 个",
                    args.len()
                ));
            }
            Ok(Some(Some(self.生成数组弹出(recv, 元素qi)?)))
        }
    }

    /// 接收者存放数组指针的那个位置（alloca / 全局 / 结构体字段）。
    fn 数组接收者槽(&mut self, recv: &AstNode) -> Result<PointerValue<'ctx>, String> {
        match recv {
            AstNode::标识符表达式(id) => {
                if let Some((p, _)) = self.变量表.get(&id.name) {
                    return Ok(*p);
                }
                if let Some((g, _)) = self.全局变量表.get(&id.name) {
                    return Ok(g.as_pointer_value());
                }
                Err(format!("未声明的变量 {}", id.name))
            }
            AstNode::字段访问表达式(fa) => {
                if self.字段基是临时(&fa.object) {
                    return Err(format!(
                        "对临时值的字段 `{}` 调 添加/弹出 没有意义（改完就丢了）。先存进变量再改",
                        fa.field
                    ));
                }
                let (base, idx) = self.求结构体指针(&fa.object)?;
                let st = self.取结构体llvm(idx)?;
                let (fidx, _) = self
                    .符号
                    .结构体信息(idx)
                    .and_then(|s| s.查字段(&fa.field))
                    .ok_or_else(|| format!("结构体无字段 {}", fa.field))?;
                self.字段指针(base, st, fidx, &fa.field)
            }
            _ => Err(
                "添加 / 弹出 可能要搬动数组存储，接收者必须是变量或结构体字段\
                 （如 `列表.添加(x)`、`自己.点们.添加(p)`）。\
                 数组元素里的数组请先取出：`变量 行 = 表[i]; 行.添加(x); 表[i] = 行;`"
                    .to_string(),
            ),
        }
    }

    /// `qi_array_prepare_mut(arr, extra, elem_rc, arc) -> arr'`，并把结果写回槽。
    fn 数组准备修改(
        &mut self,
        槽: PointerValue<'ctx>,
        extra: u64,
        元素qi: Qi类型,
    ) -> Result<PointerValue<'ctx>, String> {
        let ptrt = self.ctx.ptr_type(AddressSpace::default());
        let i64t = self.ctx.i64_type();
        let f = match self.module.get_function("qi_array_prepare_mut") {
            Some(f) => f,
            None => self.module.add_function(
                "qi_array_prepare_mut",
                ptrt.fn_type(&[ptrt.into(), i64t.into(), i64t.into(), i64t.into()], false),
                None,
            ),
        };
        let old = self
            .builder
            .build_load(ptrt, 槽, "arr.old")
            .map_err(|e| e.to_string())?;
        let 元素rc = u64::from(self.弧开() && 是RC类型(元素qi));
        let arc = u64::from(self.弧开());
        let new = self
            .builder
            .build_call(
                f,
                &[
                    old.into(),
                    i64t.const_int(extra, false).into(),
                    i64t.const_int(元素rc, false).into(),
                    i64t.const_int(arc, false).into(),
                ],
                "arr.mut",
            )
            .map_err(|e| e.to_string())?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| "qi_array_prepare_mut 未返回".to_string())?
            .into_pointer_value();
        self.builder
            .build_store(槽, new)
            .map_err(|e| e.to_string())?;
        Ok(new)
    }

    /// `甲.添加(x)`：先求值（从左到右的直觉，也让 x 里读 甲 时看到的是旧样子），
    /// 再准备存储、写进第 len 号元素槽、长度 +1。
    fn 生成数组添加(
        &mut self,
        recv: &AstNode,
        元素qi: Qi类型,
        值: &AstNode,
    ) -> Result<(), String> {
        let (mut v, vt) = self
            .生成表达式(值)?
            .ok_or_else(|| "添加 的参数没有值".to_string())?;
        Self::检查元素类型(元素qi, vt)?;
        v = self.协调到类型(v, vt, 元素qi)?;
        if 元素qi == Qi类型::布尔 && v.is_int_value() {
            // 数组里的布尔按 i1 存（元素llvm类型）；整数/比较结果收窄成 i1
            let iv = v.into_int_value();
            if iv.get_type().get_bit_width() != 1 {
                let zero = iv.get_type().const_zero();
                v = self
                    .builder
                    .build_int_compare(inkwell::IntPredicate::NE, iv, zero, "tobool")
                    .map_err(|e| e.to_string())?
                    .into();
            }
        }
        // ARC：BORROWED 的 RC 值进槽前 retain 一次；OWNED 直接转移
        if self.弧开() && v.is_pointer_value() {
            self.弧存入槽2(v, 值, 元素qi);
        }
        let 槽 = self.数组接收者槽(recv)?;
        let arr = self.数组准备修改(槽, 1, 元素qi)?;
        let i64t = self.ctx.i64_type();
        let head = self.槽指针(arr, i64t.into(), 0)?;
        let len = self
            .builder
            .build_load(i64t, head, "arr.len")
            .map_err(|e| e.to_string())?
            .into_int_value();
        let 新长 = self
            .builder
            .build_int_add(len, i64t.const_int(1, false), "arr.len1")
            .map_err(|e| e.to_string())?;
        // 第 len 个元素在第 len+1 号槽
        let 元素llvm = self.元素llvm类型(super::类型::元素类型::从标量(元素qi));
        let slot = self.槽指针动态(arr, 元素llvm, 新长)?;
        self.builder
            .build_store(slot, v)
            .map_err(|e| e.to_string())?;
        self.builder
            .build_store(head, 新长)
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// `甲.弹出()`：空数组抛异常（可被 尝试/捕获 接住）；否则取末槽、清槽、长度 -1。
    fn 生成数组弹出(
        &mut self,
        recv: &AstNode,
        元素qi: Qi类型,
    ) -> Result<(BasicValueEnum<'ctx>, Qi类型), String> {
        let 槽 = self.数组接收者槽(recv)?;
        let arr = self.数组准备修改(槽, 0, 元素qi)?;
        let i64t = self.ctx.i64_type();
        let head = self.槽指针(arr, i64t.into(), 0)?;
        let len = self
            .builder
            .build_load(i64t, head, "arr.len")
            .map_err(|e| e.to_string())?
            .into_int_value();
        let func = self
            .builder
            .get_insert_block()
            .and_then(|bb| bb.get_parent())
            .ok_or_else(|| "弹出：不在函数体内".to_string())?;
        let 空块 = self.ctx.append_basic_block(func, "pop.empty");
        let 取块 = self.ctx.append_basic_block(func, "pop.take");
        let 是空 = self
            .builder
            .build_int_compare(
                inkwell::IntPredicate::SLE,
                len,
                i64t.const_zero(),
                "pop.isempty",
            )
            .map_err(|e| e.to_string())?;
        self.builder
            .build_conditional_branch(是空, 空块, 取块)
            .map_err(|e| e.to_string())?;

        self.builder.position_at_end(空块);
        let 消息 = ThrowStatement {
            expression: Box::new(AstNode::字面量表达式(LiteralExpression {
                value: LiteralValue::字符串("弹出：数组是空的".to_string()),
                span: Default::default(),
            })),
            span: Default::default(),
        };
        self.生成抛出(&消息)?;

        self.builder.position_at_end(取块);
        let 元素llvm = self.元素llvm类型(super::类型::元素类型::从标量(元素qi));
        // 末元素在第 len 号槽
        let slot = self.槽指针动态(arr, 元素llvm, len)?;
        let v = self
            .builder
            .build_load(元素llvm, slot, "arr.pop")
            .map_err(|e| e.to_string())?;
        if self.弧开() && 是RC类型(元素qi) {
            // 引用随返回值移交：槽清零，数组释放时不会再放它一次
            let ptrt = self.ctx.ptr_type(AddressSpace::default());
            self.builder
                .build_store(slot, ptrt.const_null())
                .map_err(|e| e.to_string())?;
        }
        let 新长 = self
            .builder
            .build_int_sub(len, i64t.const_int(1, false), "arr.len0")
            .map_err(|e| e.to_string())?;
        self.builder
            .build_store(head, 新长)
            .map_err(|e| e.to_string())?;
        Ok((v, 元素qi))
    }

    /// 明显放错类型的元素直接报错（例：往 数组<点> 里添加 整数）。
    /// 只拦有把握的组合；未知类型放行，交给类型检查器。
    fn 检查元素类型(元素qi: Qi类型, 实际: Qi类型) -> Result<(), String> {
        let 名 = |t: Qi类型| match t {
            Qi类型::整数 => "整数",
            Qi类型::浮点数 => "浮点数",
            Qi类型::布尔 => "布尔",
            Qi类型::字符串 => "字符串",
            Qi类型::结构体(_) => "结构体",
            _ => "其它类型",
        };
        let 冲突 = match (元素qi, 实际) {
            (_, Qi类型::未知) => false,
            (Qi类型::结构体(a), Qi类型::结构体(b)) => a != b,
            (Qi类型::结构体(_), Qi类型::整数 | Qi类型::浮点数 | Qi类型::字符串 | Qi类型::布尔) => {
                true
            }
            (Qi类型::整数 | Qi类型::浮点数 | Qi类型::布尔, Qi类型::字符串 | Qi类型::结构体(_)) => {
                true
            }
            (Qi类型::整数, Qi类型::浮点数) => true,
            (Qi类型::字符串, Qi类型::整数 | Qi类型::浮点数 | Qi类型::结构体(_)) => {
                true
            }
            _ => false,
        };
        if 冲突 {
            let 说明 = if matches!((元素qi, 实际), (Qi类型::结构体(a), Qi类型::结构体(b)) if a != b)
            {
                "（另一种结构体）"
            } else {
                ""
            };
            return Err(format!(
                "添加 的元素类型不对：数组元素是 {}，这里给的是 {}{}",
                名(元素qi),
                名(实际),
                说明
            ));
        }
        Ok(())
    }
}
