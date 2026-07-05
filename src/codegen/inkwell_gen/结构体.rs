//! 结构体降级 —— 声明登记、字面量堆分配、字段读写。
//!
//! 内存表示：结构体实例一律**堆分配**（`qi_runtime_alloc`），按指针传递，
//! 因此可跨函数返回（构造函数、方法返回自身、链式调用都依赖这点）。
//! LLVM 侧用具名 struct 类型 + typed GEP 定位字段，读写用 load/store，
//! 类型完全由类型检查器提供 —— 不再有旧后端的字节偏移 + i64 猜测。

use super::后端;
use super::类型::Qi类型;
use super::类型检查::结构体信息;
use crate::parser::ast::{AstNode, Program, StructLiteralExpression};
use inkwell::types::BasicTypeEnum;
use inkwell::values::{BasicValueEnum, PointerValue};
use inkwell::AddressSpace;

impl<'ctx> 后端<'ctx> {
    /// 登记所有结构体：填符号表布局 + 建 LLVM 具名 struct 类型。
    /// 第一趟：只登记结构体名字（占索引），字段类型暂空。
    /// 必须先跑完所有模块的名字登记，字段里跨模块/前向引用的结构体才能解析成 结构体(idx)。
    /// 按 (当前包, 名字) 登记 —— 跨包同名结构体各占独立索引（调用前须 设当前包）。
    pub(super) fn 登记结构体名字(&mut self, program: &Program) -> Result<(), String> {
        for stmt in &program.statements {
            if let AstNode::结构体声明(sd) = stmt {
                // 泛型结构体（带 <T>）：模板入注册表（字段 TypeNode 原样存），
                // 不占具体结构体索引 —— 按 (模板, 实参) 单态实例化。
                if !sd.type_params.is_empty() {
                    if sd.type_params.len() > 2 {
                        return Err(format!(
                            "泛型结构体 {} 声明了 {} 个类型参数，最多支持 2 个（<T> 或 <T, E>）",
                            sd.name,
                            sd.type_params.len()
                        ));
                    }
                    self.符号.泛型结构体模板.insert(
                        sd.name.clone(),
                        super::类型检查::泛型结构体模板 {
                            类型参数: sd.type_params.clone(),
                            字段: sd
                                .fields
                                .iter()
                                .map(|f| (f.name.clone(), f.type_annotation.clone()))
                                .collect(),
                        },
                    );
                    continue;
                }
                let 字段名: Vec<String> = sd.fields.iter().map(|f| f.name.clone()).collect();
                // 字段类型先占位（整数），第二趟再解析
                let 占位: Vec<Qi类型> = sd.fields.iter().map(|_| Qi类型::整数).collect();
                // 登记结构体 按 (包, 名字) 幂等
                self.符号.登记结构体(结构体信息 {
                    名字: sd.name.clone(),
                    包: self.当前包.clone(),
                    字段名,
                    字段类型: 占位,
                });
            }
        }
        Ok(())
    }

    /// 第二趟：所有名字登记后，解析字段真实类型（跨模块结构体已可解析）。
    /// 只写**本包**登记的那条（绝不覆写别包同名结构体的布局）；同包重复定义且
    /// 字段不一致 → 编译报错（否则 字段名/字段类型 错位 → 字段读错内存）。
    pub(super) fn 解析结构体字段(&mut self, program: &Program) -> Result<(), String> {
        for stmt in &program.statements {
            if let AstNode::结构体声明(sd) = stmt {
                if !sd.type_params.is_empty() {
                    continue; // 泛型模板：实例化时才解析字段
                }
                let idx = self
                    .符号
                    .本包结构体索引(&sd.name)
                    .ok_or_else(|| format!("内部错误：结构体 {} 未在第一趟登记", sd.name))?;
                let 名字们: Vec<String> = sd.fields.iter().map(|f| f.name.clone()).collect();
                if self.符号.结构体[idx as usize].字段名 != 名字们 {
                    return Err(format!(
                        "结构体 {} 在包 {} 中重复定义且字段不一致",
                        sd.name,
                        self.当前包.as_deref().unwrap_or("(无包)")
                    ));
                }
                let 字段类型: Vec<Qi类型> = sd
                    .fields
                    .iter()
                    .map(|f| self.符号.解析类型(&f.type_annotation))
                    .collect();
                self.符号.结构体[idx as usize].字段类型 = 字段类型;
            }
        }
        Ok(())
    }

    /// 第三趟：据最终字段类型建所有 LLVM struct 类型（顺序与索引一致）。
    pub(super) fn 建结构体llvm类型(&mut self) -> Result<(), String> {
        self.结构体llvm.clear();
        self.确保结构体llvm齐全();
        Ok(())
    }

    /// 补建 结构体llvm 缺口（泛型结构体实例在函数体生成期间迟到登记，
    /// LLVM 类型在首次使用时惰性补齐）。幂等。
    pub(super) fn 确保结构体llvm齐全(&mut self) {
        for i in self.结构体llvm.len()..self.符号.结构体.len() {
            let 字段类型 = self.符号.结构体[i].字段类型.clone();
            let 名字 = self.符号.结构体[i].名字.clone();
            let llvm字段: Vec<BasicTypeEnum> = 字段类型
                .iter()
                .map(|t| {
                    self.llvm基础类型(*t)
                        .unwrap_or_else(|| self.ctx.i64_type().into())
                })
                .collect();
            // LLVM 类型名带包名 —— 跨包同名结构体不靠 LLVM 自动改名（.0 后缀）区分
            let 类型名 = match &self.符号.结构体[i].包 {
                Some(p) => format!("struct.{}.{}", p, 名字),
                None => format!("struct.{}", 名字),
            };
            let st = self.ctx.opaque_struct_type(&类型名);
            st.set_body(&llvm字段, false);
            self.结构体llvm.push(st);
        }
    }

    /// 按索引取结构体 LLVM 类型（缺则惰性补建 —— 泛型实例迟到登记的关键）。
    pub(super) fn 取结构体llvm(
        &mut self,
        idx: u32,
    ) -> Result<inkwell::types::StructType<'ctx>, String> {
        if idx as usize >= self.结构体llvm.len() {
            self.确保结构体llvm齐全();
        }
        self.结构体llvm
            .get(idx as usize)
            .copied()
            .ok_or_else(|| format!("结构体索引 {} 越界", idx))
    }

    /// 结构体字面量 → 堆分配指针 + 逐字段 store。
    /// 泛型字面量（对<整数> { … }）先按 (模板, 实参) 单态实例化取具体索引。
    pub(super) fn 生成结构体字面量(
        &mut self,
        lit: &StructLiteralExpression,
    ) -> Result<(BasicValueEnum<'ctx>, Qi类型), String> {
        let idx = if !lit.type_arguments.is_empty() {
            let 实参: Vec<Qi类型> = lit
                .type_arguments
                .iter()
                .map(|t| self.符号.解析类型(t))
                .collect();
            if 实参.contains(&Qi类型::未知) {
                return Err(format!(
                    "泛型结构体字面量 {} 的类型实参无法解析（未定义的类型？）",
                    lit.struct_name
                ));
            }
            self.符号.实例化参数结构体(&lit.struct_name, &实参)?
        } else if let Some(idx) = self.符号.结构体索引(&lit.struct_name) {
            idx
        } else if self.符号.泛型结构体模板.contains_key(&lit.struct_name) {
            return Err(format!(
                "{} 是泛型结构体，字面量需要带类型实参，例如 新建 {}<整数> {{ … }}",
                lit.struct_name, lit.struct_name
            ));
        } else {
            return Err(self.符号.结构体解析错误(&lit.struct_name));
        };
        let st = self.取结构体llvm(idx)?;

        // 堆分配：size = LLVM 结构体大小（用 target-independent 常量占位不可靠，
        // 直接用 store 之前的 GEP 定位；分配大小取字段数*8 的保守上界，够放所有字段，
        // 与旧后端一致，指针可 GC 追踪）。这里用精确布局大小更稳：字段数*8。
        let 字段数 = self
            .符号
            .结构体信息(idx)
            .map(|s| s.字段名.len())
            .unwrap_or(0);
        let size = self.ctx.i64_type().const_int((字段数 as u64) * 8, false);
        // QI_ARC=1：走 RC 对象分配器（ptr-24 header + refcount=1 + 零初始化，
        // 且绕开 memory_manager 的全局 RwLock）；关闭时保持 qi_runtime_alloc 不变。
        let alloc名 = if self.弧开() {
            "qi_obj_alloc"
        } else {
            "qi_runtime_alloc"
        };
        let alloc = self
            .module
            .get_function(alloc名)
            .ok_or_else(|| format!("运行时函数未声明: {}", alloc名))?;
        let cs = self
            .builder
            .build_call(alloc, &[size.into()], "structmem")
            .map_err(|e| e.to_string())?;
        let base = cs
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| "qi_runtime_alloc 未返回指针".to_string())?
            .into_pointer_value();

        // ARC：先把「字面量没覆盖到的字符串字段」置 null（qi_runtime_alloc 不清零，
        // 否则后续字段赋值的旧值 release 会读到垃圾指针）。
        if self.弧开() {
            let 信息 = self
                .符号
                .结构体信息(idx)
                .map(|s| (s.字段名.clone(), s.字段类型.clone()));
            if let Some((字段名, 字段类型)) = 信息 {
                let ptrt = self.ctx.ptr_type(AddressSpace::default());
                // zip 迭代：字段名/字段类型 结构上不可能越界（长度不一致只会少迭代，
                // 且第二趟已强制两者一致）
                for (fi, (名, ft)) in 字段名.iter().zip(字段类型.iter()).enumerate() {
                    if *ft == Qi类型::字符串 && !lit.fields.iter().any(|fv| &fv.name == 名) {
                        let fptr = self.字段指针(base, st, fi as u32, 名)?;
                        self.builder
                            .build_store(fptr, ptrt.const_null())
                            .map_err(|e| e.to_string())?;
                    }
                }
            }
        }

        // 逐字段初始化（按字面量出现顺序，用字段名定索引）
        for fv in &lit.fields {
            let (fidx, ftype) = self
                .符号
                .结构体信息(idx)
                .and_then(|s| s.查字段(&fv.name))
                .ok_or_else(|| format!("结构体 {} 无字段 {}", lit.struct_name, fv.name))?;
            let (mut v, vt) = self
                .生成表达式(&fv.value)?
                .ok_or_else(|| format!("字段 {} 初值无值", fv.name))?;
            if ftype.是浮点() && !vt.是浮点() {
                v = self.整数转浮点值(v)?;
            }
            // ARC：RC 值（字符串/结构体/数组）存进字段 —— BORROWED retain /
            // OWNED 转移（字段持有一份）。本体归零时释放函数逐字段回收。
            if self.弧开() && v.is_pointer_value() {
                self.弧存入槽2(v, &fv.value, ftype);
            }
            let fptr = self.字段指针(base, st, fidx, &fv.name)?;
            self.builder
                .build_store(fptr, v)
                .map_err(|e| e.to_string())?;
        }

        Ok((base.into(), Qi类型::结构体(idx)))
    }

    /// 字段访问 x.字段 → GEP + load，返回 (值, 字段类型)。
    pub(super) fn 生成字段访问(
        &mut self,
        object: &AstNode,
        field: &str,
    ) -> Result<(BasicValueEnum<'ctx>, Qi类型), String> {
        let (base, idx) = self.求结构体指针(object)?;
        let st = self.取结构体llvm(idx)?;
        let (fidx, ftype) = self
            .符号
            .结构体信息(idx)
            .and_then(|s| s.查字段(field))
            .ok_or_else(|| format!("结构体无字段 {}", field))?;
        let fptr = self.字段指针(base, st, fidx, field)?;
        let llvmt = self
            .llvm基础类型(ftype)
            .ok_or_else(|| format!("字段 {} 类型无效", field))?;
        let v = self
            .builder
            .build_load(llvmt, fptr, field)
            .map_err(|e| e.to_string())?;
        Ok((v, ftype))
    }

    /// 字段赋值 x.字段 = 值 → GEP + store。
    pub(super) fn 生成字段赋值(
        &mut self,
        object: &AstNode,
        field: &str,
        value: &AstNode,
    ) -> Result<(BasicValueEnum<'ctx>, Qi类型), String> {
        let (base, idx) = self.求结构体指针(object)?;
        let st = self.取结构体llvm(idx)?;
        let (fidx, ftype) = self
            .符号
            .结构体信息(idx)
            .and_then(|s| s.查字段(field))
            .ok_or_else(|| format!("结构体无字段 {}", field))?;
        let (mut v, vt) = self
            .生成表达式(value)?
            .ok_or_else(|| "字段赋值右值无值".to_string())?;
        if ftype.是浮点() && !vt.是浮点() {
            v = self.整数转浮点值(v)?;
        }
        let fptr = self.字段指针(base, st, fidx, field)?;
        // ARC：RC 字段覆写 —— 先 retain 新值（BORROWED 时；自赋值安全的关键
        // 顺序），再释放旧值（qi_obj_alloc 零初始化保证旧值是合法值或 null）。
        if self.弧开() && v.is_pointer_value() {
            self.弧存入槽2(v, value, ftype);
            self.弧释放槽旧值2(fptr, ftype)?;
        }
        self.builder
            .build_store(fptr, v)
            .map_err(|e| e.to_string())?;
        Ok((v, ftype))
    }

    /// 求某表达式的结构体指针 + 结构体索引。表达式类型必须是结构体。
    pub(super) fn 求结构体指针(
        &mut self,
        object: &AstNode,
    ) -> Result<(PointerValue<'ctx>, u32), String> {
        let (v, t) = self
            .生成表达式(object)?
            .ok_or_else(|| "结构体表达式无值".to_string())?;
        let idx = t
            .结构体索引()
            .ok_or_else(|| "该表达式不是结构体，无法访问字段/方法".to_string())?;
        Ok((v.into_pointer_value(), idx))
    }

    /// 计算字段的 typed GEP 指针。
    fn 字段指针(
        &self,
        base: PointerValue<'ctx>,
        st: inkwell::types::StructType<'ctx>,
        fidx: u32,
        name: &str,
    ) -> Result<PointerValue<'ctx>, String> {
        self.builder
            .build_struct_gep(st, base, fidx, name)
            .map_err(|_| format!("字段 {} GEP 失败", name))
    }

    /// 整数值 sitofp → double（结构体字段/字面量隐式提升用）。
    fn 整数转浮点值(&self, v: BasicValueEnum<'ctx>) -> Result<BasicValueEnum<'ctx>, String> {
        Ok(self
            .builder
            .build_signed_int_to_float(v.into_int_value(), self.ctx.f64_type(), "sitofp")
            .map_err(|e| e.to_string())?
            .into())
    }

    /// 便捷：不透明指针类型。
    #[allow(dead_code)]
    fn ptr类型(&self) -> inkwell::types::PointerType<'ctx> {
        self.ctx.ptr_type(AddressSpace::default())
    }
}
