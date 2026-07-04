//! 语句 / 控制流降级 —— 变量声明、赋值、如果/否则、当（while）、对于（for）、返回。
//!
//! 控制流用 basic block + 条件跳转搭建；每个块生成完后检查是否已有终结指令，
//! 避免重复 br/ret 触发 LLVM 校验失败。

use super::后端;
use super::类型::Qi类型;
use super::类型检查::推断表达式类型;
use crate::parser::ast::AstNode;
use inkwell::values::FunctionValue;
use inkwell::IntPredicate;

impl<'ctx> 后端<'ctx> {
    /// 生成一条语句。`func` 为当前所在函数，用于插入新块。
    pub(super) fn 生成语句(
        &mut self,
        node: &AstNode,
        func: FunctionValue<'ctx>,
    ) -> Result<(), String> {
        match node {
            AstNode::变量声明(vd) => {
                // 类型：注解优先（结构体感知），否则由初值推断。
                // 注解解析出结构体/基础类型即用；解析不出（未知）再退回初值推断。
                let 注解类型 = vd
                    .type_annotation
                    .as_ref()
                    .map(|ann| self.符号.解析类型(ann));
                let 初值类型 = vd
                    .initializer
                    .as_ref()
                    .map(|init| 推断表达式类型(init, &self.符号));
                let 类型 = match (注解类型, 初值类型) {
                    (Some(t), _) if t != Qi类型::未知 => t,
                    (_, Some(t)) if t != Qi类型::未知 => t,
                    _ => Qi类型::整数,
                };

                let llvmt = self
                    .llvm基础类型(类型)
                    .ok_or_else(|| format!("变量 {} 类型无效", vd.name))?;
                // ARC：RC 槽（字符串/结构体/数组）统一在 entry 块 alloca + null
                // 初始化（支配所有 return 点，出口统一释放合法；重入声明可安全
                // 释放旧值）。非 ARC 路径也统一 entry 块 alloca（循环不吃栈）。
                let 弧rc = self.弧开() && super::所有权::是RC类型(类型);
                let ptr = if 弧rc {
                    self.弧字符串槽(func, &vd.name)?
                } else {
                    self.入口块alloca(llvmt, &vd.name)?
                };

                if let Some(init) = &vd.initializer {
                    let (mut v, vt) = self
                        .生成表达式(init)?
                        .ok_or_else(|| format!("变量 {} 初值无值", vd.name))?;
                    // 隐式协调：整数→浮点提升、布尔(i1)→整数(i64) 加宽等
                    v = self.协调到类型(v, vt, 类型)?;
                    if 弧rc && v.is_pointer_value() {
                        // 先 retain 新值（BORROWED 时），再释放旧值（首轮为 null，
                        // 循环里重入声明时是上一轮的值），最后 store。
                        self.弧存入槽2(v, init, 类型);
                        self.弧释放槽旧值2(ptr, 类型)?;
                    }
                    self.builder
                        .build_store(ptr, v)
                        .map_err(|e| e.to_string())?;
                }
                self.变量表.insert(vd.name.clone(), (ptr, 类型));
                self.符号.声明变量(&vd.name, 类型);
                Ok(())
            }

            AstNode::表达式语句(es) => {
                if let Some((v, t)) = self.生成表达式(&es.expression)? {
                    // ARC：被丢弃的 OWNED RC 值（FFI 返回串没人接 / 结构体
                    // 数组临时没人存）→ release
                    self.弧消费后释放2(v, t, &es.expression);
                }
                Ok(())
            }

            AstNode::返回语句(ret) => {
                // 入口→main 返回 i32：无论 bare `返回` 还是带值，都 ret i32 0
                if self.在入口中 {
                    if let Some(expr) = &ret.value {
                        // 求值副作用，丢弃 —— OWNED RC 值按丢弃释放
                        if let Some((v, t)) = self.生成表达式(expr)? {
                            self.弧消费后释放2(v, t, expr);
                        }
                    }
                    self.弧释放局部()?; // ARC：main 出口释放字符串局部
                    self.弹try帧()?; // E4：try 体内 return 先弹异常 frame
                    let z = self.ctx.i32_type().const_int(0, false);
                    self.builder
                        .build_return(Some(&z))
                        .map_err(|e| e.to_string())?;
                    return Ok(());
                }
                match &ret.value {
                    Some(expr) => {
                        let (v, vt) = self
                            .生成表达式(expr)?
                            .ok_or_else(|| "返回表达式无值".to_string())?;
                        let rt = self.当前返回类型;
                        // 声明返回 void：忽略返回值，emit ret void
                        if rt == Qi类型::空 {
                            self.弧消费后释放2(v, vt, expr); // 值被丢弃
                            self.弧释放局部()?;
                            self.弹try帧()?;
                            self.builder.build_return(None).map_err(|e| e.to_string())?;
                        } else if rt.未来内部().is_some() && vt.未来内部().is_none() {
                            // 函数返回 未来<T> 而值不是 future：包成 ready future
                            // ARC：结构体/数组经 ready_ptr **存指针不拷贝** ——
                            // 发送即转移：BORROWED 先 retain（future 持有一份，
                            // future 本体不回收 → 该对象泄漏，宁泄漏不悬垂）。
                            if self.弧开()
                                && matches!(vt, Qi类型::结构体(_) | Qi类型::数组(_))
                                && !self.表达式拥有RC(expr, vt)
                            {
                                self.弧retain任意(v, vt);
                            }
                            let fut = self.包装ready(v, vt)?;
                            // ready_string 已把字节拷进 future；OWNED 源释放
                            // （仅字符串 —— 结构体/数组已转移进 future，不释放）
                            self.弧消费后释放(v, vt, expr);
                            self.弧释放局部()?;
                            self.弹try帧()?;
                            self.builder
                                .build_return(Some(&fut))
                                .map_err(|e| e.to_string())?;
                        } else {
                            let cv = self.协调返回值(v, vt, rt)?;
                            // 返回约定 +1：RC 返回值 BORROWED → retain 再 ret；
                            // OWNED → 直接 ret（所有权移交调用方）。
                            // 结构体/数组按声明返回类型 rt retain（值可能经 FFI
                            // 透传 vt 标注不准 —— qi_obj_retain 跨 magic 安全）。
                            let 该retain = if rt == Qi类型::字符串 {
                                vt == Qi类型::字符串 && !self.表达式拥有字符串(expr)
                            } else {
                                super::所有权::是RC类型(rt) && !self.表达式拥有RC(expr, rt)
                            };
                            if self.弧开() && 该retain && cv.is_pointer_value() {
                                self.弧retain任意(cv, rt);
                            }
                            // 出口释放全部字符串局部（返回值若来自某局部：上面已
                            // retain +1，槽 release -1，净 +1 交调用方，正确）。
                            self.弧释放局部()?;
                            self.弹try帧()?;
                            self.builder
                                .build_return(Some(&cv))
                                .map_err(|e| e.to_string())?;
                        }
                    }
                    None => {
                        self.弧释放局部()?;
                        self.弹try帧()?;
                        self.builder.build_return(None).map_err(|e| e.to_string())?;
                    }
                }
                Ok(())
            }

            AstNode::如果语句(if_stmt) => {
                let cond = self.生成条件(&if_stmt.condition)?;
                let then_bb = self.ctx.append_basic_block(func, "then");
                let else_bb = self.ctx.append_basic_block(func, "else");
                let merge_bb = self.ctx.append_basic_block(func, "ifcont");

                self.builder
                    .build_conditional_branch(cond, then_bb, else_bb)
                    .map_err(|e| e.to_string())?;

                // then
                self.builder.position_at_end(then_bb);
                self.生成块(&if_stmt.then_branch, func)?;
                self.跳转若未终结(merge_bb)?;

                // else
                self.builder.position_at_end(else_bb);
                if let Some(else_branch) = &if_stmt.else_branch {
                    match else_branch.as_ref() {
                        AstNode::块语句(bs) => self.生成块(&bs.statements, func)?,
                        // else if：else 分支是单条如果语句
                        other => self.生成语句(other, func)?,
                    }
                }
                self.跳转若未终结(merge_bb)?;

                self.builder.position_at_end(merge_bb);
                Ok(())
            }

            AstNode::当语句(w) => {
                let cond_bb = self.ctx.append_basic_block(func, "while.cond");
                let body_bb = self.ctx.append_basic_block(func, "while.body");
                let end_bb = self.ctx.append_basic_block(func, "while.end");

                self.builder
                    .build_unconditional_branch(cond_bb)
                    .map_err(|e| e.to_string())?;

                self.builder.position_at_end(cond_bb);
                let cond = self.生成条件(&w.condition)?;
                self.builder
                    .build_conditional_branch(cond, body_bb, end_bb)
                    .map_err(|e| e.to_string())?;

                self.builder.position_at_end(body_bb);
                // 继续 → 重测条件；跳出 → 循环后
                self.循环栈.push((cond_bb, end_bb));
                let r = self.生成块(&w.body, func);
                self.循环栈.pop();
                r?;
                self.跳转若未终结(cond_bb)?;

                self.builder.position_at_end(end_bb);
                Ok(())
            }

            AstNode::对于语句(f) => self.生成对于(f, func),

            AstNode::跳出语句(_) => {
                let (_, 出口) = self
                    .循环栈
                    .last()
                    .copied()
                    .ok_or_else(|| "跳出 只能出现在 当 / 对于 循环体内".to_string())?;
                // RC 局部槽在函数出口统一释放（entry 块 alloca + null 初始化），
                // 穿作用域跳转无需额外插释放。
                self.builder
                    .build_unconditional_branch(出口)
                    .map_err(|e| e.to_string())?;
                Ok(())
            }

            AstNode::继续语句(_) => {
                let (继续目标, _) = self
                    .循环栈
                    .last()
                    .copied()
                    .ok_or_else(|| "继续 只能出现在 当 / 对于 循环体内".to_string())?;
                self.builder
                    .build_unconditional_branch(继续目标)
                    .map_err(|e| e.to_string())?;
                Ok(())
            }

            AstNode::选择表达式(s) => self.生成选择(s, func),

            AstNode::匹配表达式(m) => self.生成匹配(m).map(|_| ()),

            AstNode::块语句(bs) => self.生成块(&bs.statements, func),

            AstNode::尝试语句(t) => self.生成尝试(t, func),

            AstNode::抛出语句(t) => self.生成抛出(t),

            _ => Ok(()),
        }
    }

    /// 生成一个语句块（带作用域），保留 self 的作用域栈平衡。
    pub(super) fn 生成块(
        &mut self,
        stmts: &[AstNode],
        func: FunctionValue<'ctx>,
    ) -> Result<(), String> {
        self.符号.进入作用域();
        for s in stmts {
            self.生成语句(s, func)?;
            // 已终结（return/break）后不再生成同块后续指令
            if self.当前块已终结() {
                break;
            }
        }
        self.符号.退出作用域();
        Ok(())
    }

    /// `对于 变量 在 起..止` 形式的整数区间循环。range 期望为二元「..」表达式；
    /// 前端把范围建成什么节点不确定，这里保守支持 数组访问/范围模式暂略，
    /// 仅覆盖最常见的 `对于 i 在 0..n`（若 range 非区间则跳过循环体）。
    fn 生成对于(
        &mut self,
        f: &crate::parser::ast::ForStatement,
        func: FunctionValue<'ctx>,
    ) -> Result<(), String> {
        // range 若不是可识别的区间，先尝试「对于 x 在 数组」遍历；
        // 都不是则退化为不执行（保守，不误生成）。
        let (起, 止, 含端点) = match self.拆区间(&f.range)? {
            Some(x) => x,
            None => return self.生成对于数组(f, func),
        };

        let i64t = self.ctx.i64_type();
        // entry 块 alloca：嵌套循环里的 for 不再每次外层迭代吃栈
        let ivar = self.入口块alloca(i64t.into(), &f.variable)?;
        self.builder
            .build_store(ivar, 起)
            .map_err(|e| e.to_string())?;
        self.变量表.insert(f.variable.clone(), (ivar, Qi类型::整数));
        self.符号.声明变量(&f.variable, Qi类型::整数);

        let cond_bb = self.ctx.append_basic_block(func, "for.cond");
        let body_bb = self.ctx.append_basic_block(func, "for.body");
        let step_bb = self.ctx.append_basic_block(func, "for.step");
        let end_bb = self.ctx.append_basic_block(func, "for.end");

        self.builder
            .build_unconditional_branch(cond_bb)
            .map_err(|e| e.to_string())?;
        self.builder.position_at_end(cond_bb);
        let cur = self
            .builder
            .build_load(i64t, ivar, "i")
            .map_err(|e| e.to_string())?
            .into_int_value();
        // `..` 半开（SLT，不含 止）；`到` 闭区间（SLE，含 止 —— 用 SLE 而非
        // 止+1，避免 止 = i64::MAX 时加一溢出）。
        let 谓词 = if 含端点 {
            IntPredicate::SLE
        } else {
            IntPredicate::SLT
        };
        let cond = self
            .builder
            .build_int_compare(谓词, cur, 止, "for.cmp")
            .map_err(|e| e.to_string())?;
        self.builder
            .build_conditional_branch(cond, body_bb, end_bb)
            .map_err(|e| e.to_string())?;

        self.builder.position_at_end(body_bb);
        // 继续 → step（先自增再重测，不跳过步进）；跳出 → 循环后
        self.循环栈.push((step_bb, end_bb));
        let r = self.生成块(&f.body, func);
        self.循环栈.pop();
        r?;
        self.跳转若未终结(step_bb)?;

        // step：自增 + 回测（体内全路径 return/跳出 时不可达，仍合法）
        self.builder.position_at_end(step_bb);
        let cur2 = self
            .builder
            .build_load(i64t, ivar, "i")
            .map_err(|e| e.to_string())?
            .into_int_value();
        let next = self
            .builder
            .build_int_add(cur2, i64t.const_int(1, false), "inc")
            .map_err(|e| e.to_string())?;
        self.builder
            .build_store(ivar, next)
            .map_err(|e| e.to_string())?;
        self.builder
            .build_unconditional_branch(cond_bb)
            .map_err(|e| e.to_string())?;

        self.builder.position_at_end(end_bb);
        Ok(())
    }

    /// `对于 元素 在 数组` —— 按数组长度头逐槽遍历，元素拷入循环变量。
    /// range 表达式类型不是数组时保守跳过循环体（与旧行为一致，不误生成）。
    fn 生成对于数组(
        &mut self,
        f: &crate::parser::ast::ForStatement,
        func: FunctionValue<'ctx>,
    ) -> Result<(), String> {
        let Some((av, at)) = self.生成表达式(&f.range)? else {
            return Ok(());
        };
        let Some(元素) = at.数组元素() else {
            return Ok(());
        };
        let base = av.into_pointer_value();
        let i64t = self.ctx.i64_type();

        // 数组长度（头槽）
        let head = self.槽指针(base, i64t.into(), 0)?;
        let len = self
            .builder
            .build_load(i64t, head, "each.len")
            .map_err(|e| e.to_string())?
            .into_int_value();

        // 循环变量（元素副本）+ 隐藏下标 —— entry 块 alloca，嵌套循环不吃栈。
        // ARC：RC 元素槽走 entry null 初始化（空数组不进循环体时出口统一释放
        // 读到 null 才安全；每迭代覆写也需释放上一轮旧值）。
        let 元素qi = 元素.标量();
        let 元素llvm = self.元素llvm类型(元素);
        let evar = if self.弧开() && super::所有权::是RC类型(元素qi) {
            self.弧字符串槽(func, &f.variable)?
        } else {
            self.入口块alloca(元素llvm, &f.variable)?
        };
        let ivar = self.入口块alloca(i64t.into(), "each.idx")?;
        self.builder
            .build_store(ivar, i64t.const_zero())
            .map_err(|e| e.to_string())?;
        self.变量表.insert(f.variable.clone(), (evar, 元素qi));
        self.符号.声明变量(&f.variable, 元素qi);

        let cond_bb = self.ctx.append_basic_block(func, "each.cond");
        let body_bb = self.ctx.append_basic_block(func, "each.body");
        let step_bb = self.ctx.append_basic_block(func, "each.step");
        let end_bb = self.ctx.append_basic_block(func, "each.end");

        self.builder
            .build_unconditional_branch(cond_bb)
            .map_err(|e| e.to_string())?;
        self.builder.position_at_end(cond_bb);
        let cur = self
            .builder
            .build_load(i64t, ivar, "each.i")
            .map_err(|e| e.to_string())?
            .into_int_value();
        let cond = self
            .builder
            .build_int_compare(IntPredicate::SLT, cur, len, "each.cmp")
            .map_err(|e| e.to_string())?;
        self.builder
            .build_conditional_branch(cond, body_bb, end_bb)
            .map_err(|e| e.to_string())?;

        self.builder.position_at_end(body_bb);
        // 元素槽 = i + 1（跳过长度头）→ 载入循环变量
        let 偏移 = self
            .builder
            .build_int_add(cur, i64t.const_int(1, false), "each.idx1")
            .map_err(|e| e.to_string())?;
        let slot = self.槽指针动态(base, 元素llvm, 偏移)?;
        let ev = self
            .builder
            .build_load(元素llvm, slot, "each.elem")
            .map_err(|e| e.to_string())?;
        // ARC：RC 元素拷入局部槽是借用 → 先 retain 新值，再释放上一轮旧值
        // （首轮为 null），最后 store；与出口「释放 RC 局部」平衡。
        if self.弧开() && super::所有权::是RC类型(元素qi) {
            self.弧retain任意(ev, 元素qi);
            self.弧释放槽旧值2(evar, 元素qi)?;
        }
        self.builder
            .build_store(evar, ev)
            .map_err(|e| e.to_string())?;

        // 继续 → step（先步进再重测）；跳出 → 循环后
        self.循环栈.push((step_bb, end_bb));
        let r = self.生成块(&f.body, func);
        self.循环栈.pop();
        r?;
        self.跳转若未终结(step_bb)?;

        self.builder.position_at_end(step_bb);
        let cur2 = self
            .builder
            .build_load(i64t, ivar, "each.i2")
            .map_err(|e| e.to_string())?
            .into_int_value();
        let next = self
            .builder
            .build_int_add(cur2, i64t.const_int(1, false), "each.inc")
            .map_err(|e| e.to_string())?;
        self.builder
            .build_store(ivar, next)
            .map_err(|e| e.to_string())?;
        self.builder
            .build_unconditional_branch(cond_bb)
            .map_err(|e| e.to_string())?;

        self.builder.position_at_end(end_bb);
        Ok(())
    }

    /// 尝试把 range 节点拆成 (起始 i64 值, 终止 i64 值, 是否含端点)。
    /// 前端把 `起..止`（半开）/ `起 到 止`（闭）都建成 区间表达式 节点；
    /// 起止在循环块之外求值（各一次）。非区间节点返回 None（走数组遍历）。
    fn 拆区间(
        &mut self,
        range: &AstNode,
    ) -> Result<
        Option<(
            inkwell::values::IntValue<'ctx>,
            inkwell::values::IntValue<'ctx>,
            bool,
        )>,
        String,
    > {
        let AstNode::区间表达式(r) = range else {
            return Ok(None);
        };
        let 关键字 = if r.inclusive { "到" } else { ".." };
        let (sv, st) = self
            .生成表达式(&r.start)?
            .ok_or_else(|| "区间起点无值".to_string())?;
        let (ev, et) = self
            .生成表达式(&r.end)?
            .ok_or_else(|| "区间终点无值".to_string())?;
        for (t, 侧) in [(st, "起点"), (et, "终点")] {
            if t.是浮点() || t == Qi类型::字符串 {
                return Err(format!(
                    "区间循环（{}）的{}必须是整数，不能是{}",
                    关键字,
                    侧,
                    if t.是浮点() {
                        "浮点数"
                    } else {
                        "字符串"
                    }
                ));
            }
        }
        if !sv.is_int_value() || !ev.is_int_value() {
            return Err(format!("区间循环（{}）的起止必须是整数", 关键字));
        }
        let s = self.整数加宽到i64(sv.into_int_value())?;
        let e = self.整数加宽到i64(ev.into_int_value())?;
        Ok(Some((s, e, r.inclusive)))
    }

    /// 生成条件表达式并归一到 i1。
    pub(super) fn 生成条件(
        &mut self,
        node: &AstNode,
    ) -> Result<inkwell::values::IntValue<'ctx>, String> {
        let (v, _t) = self
            .生成表达式(node)?
            .ok_or_else(|| "条件表达式无值".to_string())?;
        let iv = v.into_int_value();
        if iv.get_type().get_bit_width() == 1 {
            Ok(iv)
        } else {
            self.builder
                .build_int_compare(IntPredicate::NE, iv, iv.get_type().const_zero(), "tobool")
                .map_err(|e| e.to_string())
        }
    }

    /// 把整数值 sitofp 成 double。
    fn 转为浮点(
        &mut self,
        v: inkwell::values::BasicValueEnum<'ctx>,
    ) -> Result<inkwell::values::BasicValueEnum<'ctx>, String> {
        Ok(self
            .builder
            .build_signed_int_to_float(v.into_int_value(), self.ctx.f64_type(), "sitofp")
            .map_err(|e| e.to_string())?
            .into())
    }

    /// 把返回值协调到函数声明的返回类型对应的 LLVM 表示。
    /// 处理：int→float 提升；ptr(字符串/结构体/函数值/句柄) ↔ i64 句柄；bool↔i64。
    fn 协调返回值(
        &mut self,
        v: inkwell::values::BasicValueEnum<'ctx>,
        实际: Qi类型,
        期望: Qi类型,
    ) -> Result<inkwell::values::BasicValueEnum<'ctx>, String> {
        use inkwell::values::BasicValueEnum::*;
        // 目标是否 ptr 语义
        let 目标ptr = matches!(
            期望,
            Qi类型::字符串
                | Qi类型::结构体(_)
                | Qi类型::函数值(_)
                | Qi类型::数组(_)
                | Qi类型::未来(_)
        );
        // 浮点提升
        if 期望.是浮点() && !实际.是浮点() {
            return self.转为浮点(v);
        }
        match v {
            PointerValue(pv) => {
                if 目标ptr {
                    Ok(v)
                } else {
                    // ptr → i64 句柄
                    Ok(self
                        .builder
                        .build_ptr_to_int(pv, self.ctx.i64_type(), "p2i")
                        .map_err(|e| e.to_string())?
                        .into())
                }
            }
            IntValue(iv) => {
                if 目标ptr {
                    // i64 句柄 → ptr
                    Ok(self
                        .builder
                        .build_int_to_ptr(
                            iv,
                            self.ctx.ptr_type(inkwell::AddressSpace::default()),
                            "i2p",
                        )
                        .map_err(|e| e.to_string())?
                        .into())
                } else {
                    // 整数位宽协调（布尔 i1 ↔ i64 等）
                    let want_bits = if 期望 == Qi类型::布尔 { 1 } else { 64 };
                    let cur = iv.get_type().get_bit_width();
                    if cur == want_bits {
                        Ok(v)
                    } else if cur < want_bits {
                        Ok(self
                            .builder
                            .build_int_z_extend(iv, self.ctx.i64_type(), "zext")
                            .map_err(|e| e.to_string())?
                            .into())
                    } else {
                        Ok(self
                            .builder
                            .build_int_truncate(iv, self.ctx.bool_type(), "trunc")
                            .map_err(|e| e.to_string())?
                            .into())
                    }
                }
            }
            _ => Ok(v),
        }
    }

    /// 当前基本块是否已有终结指令（ret/br 等）。
    pub(super) fn 当前块已终结(&self) -> bool {
        self.builder
            .get_insert_block()
            .and_then(|b| b.get_terminator())
            .is_some()
    }

    /// 若当前块未终结，则无条件跳到目标块。
    pub(super) fn 跳转若未终结(
        &self,
        target: inkwell::basic_block::BasicBlock<'ctx>,
    ) -> Result<(), String> {
        if !self.当前块已终结() {
            self.builder
                .build_unconditional_branch(target)
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}
