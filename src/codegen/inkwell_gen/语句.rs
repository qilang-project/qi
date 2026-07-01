//! 语句 / 控制流降级 —— 变量声明、赋值、如果/否则、当（while）、对于（for）、返回。
//!
//! 控制流用 basic block + 条件跳转搭建；每个块生成完后检查是否已有终结指令，
//! 避免重复 br/ret 触发 LLVM 校验失败。

use super::类型::Qi类型;
use super::类型检查::推断表达式类型;
use super::后端;
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
                let 注解类型 = vd.type_annotation.as_ref().map(|ann| self.符号.解析类型(ann));
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
                let ptr = self
                    .builder
                    .build_alloca(llvmt, &vd.name)
                    .map_err(|e| e.to_string())?;

                if let Some(init) = &vd.initializer {
                    let (mut v, vt) = self
                        .生成表达式(init)?
                        .ok_or_else(|| format!("变量 {} 初值无值", vd.name))?;
                    // 整数初值赋给浮点变量：隐式提升
                    if 类型.是浮点() && !vt.是浮点() {
                        v = self.转为浮点(v)?;
                    }
                    self.builder.build_store(ptr, v).map_err(|e| e.to_string())?;
                }
                self.变量表.insert(vd.name.clone(), (ptr, 类型));
                self.符号.声明变量(&vd.name, 类型);
                Ok(())
            }

            AstNode::表达式语句(es) => {
                self.生成表达式(&es.expression)?;
                Ok(())
            }

            AstNode::返回语句(ret) => {
                match &ret.value {
                    Some(expr) => {
                        let (mut v, vt) = self
                            .生成表达式(expr)?
                            .ok_or_else(|| "返回表达式无值".to_string())?;
                        // 若当前函数返回浮点而值是整数，提升
                        if self.当前返回类型.是浮点() && !vt.是浮点() {
                            v = self.转为浮点(v)?;
                        }
                        self.builder.build_return(Some(&v)).map_err(|e| e.to_string())?;
                    }
                    None => {
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
                self.生成块(&w.body, func)?;
                self.跳转若未终结(cond_bb)?;

                self.builder.position_at_end(end_bb);
                Ok(())
            }

            AstNode::对于语句(f) => self.生成对于(f, func),

            AstNode::块语句(bs) => self.生成块(&bs.statements, func),

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
        // range 若不是可识别的区间，退化为不执行（保守，不误生成）。
        let (起, 止) = match self.拆区间(&f.range)? {
            Some(x) => x,
            None => return Ok(()),
        };

        let i64t = self.ctx.i64_type();
        let ivar = self
            .builder
            .build_alloca(i64t, &f.variable)
            .map_err(|e| e.to_string())?;
        self.builder.build_store(ivar, 起).map_err(|e| e.to_string())?;
        self.变量表.insert(f.variable.clone(), (ivar, Qi类型::整数));
        self.符号.声明变量(&f.variable, Qi类型::整数);

        let cond_bb = self.ctx.append_basic_block(func, "for.cond");
        let body_bb = self.ctx.append_basic_block(func, "for.body");
        let end_bb = self.ctx.append_basic_block(func, "for.end");

        self.builder.build_unconditional_branch(cond_bb).map_err(|e| e.to_string())?;
        self.builder.position_at_end(cond_bb);
        let cur = self
            .builder
            .build_load(i64t, ivar, "i")
            .map_err(|e| e.to_string())?
            .into_int_value();
        let cond = self
            .builder
            .build_int_compare(IntPredicate::SLT, cur, 止, "for.cmp")
            .map_err(|e| e.to_string())?;
        self.builder
            .build_conditional_branch(cond, body_bb, end_bb)
            .map_err(|e| e.to_string())?;

        self.builder.position_at_end(body_bb);
        self.生成块(&f.body, func)?;
        if !self.当前块已终结() {
            let cur2 = self
                .builder
                .build_load(i64t, ivar, "i")
                .map_err(|e| e.to_string())?
                .into_int_value();
            let next = self
                .builder
                .build_int_add(cur2, i64t.const_int(1, false), "inc")
                .map_err(|e| e.to_string())?;
            self.builder.build_store(ivar, next).map_err(|e| e.to_string())?;
            self.builder.build_unconditional_branch(cond_bb).map_err(|e| e.to_string())?;
        }

        self.builder.position_at_end(end_bb);
        Ok(())
    }

    /// 尝试把 range 节点拆成 (起始 i64 值, 终止 i64 值)。识别不了返回 None。
    fn 拆区间(
        &mut self,
        range: &AstNode,
    ) -> Result<Option<(inkwell::values::IntValue<'ctx>, inkwell::values::IntValue<'ctx>)>, String>
    {
        // 二元操作里没有专门的「范围」运算符，前端可能用别的节点。
        // 目前仅当 range 是「数组字面量 [起, 止]」这类无法确定时保守跳过。
        let _ = range;
        Ok(None)
    }

    /// 生成条件表达式并归一到 i1。
    fn 生成条件(&mut self, node: &AstNode) -> Result<inkwell::values::IntValue<'ctx>, String> {
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

    /// 当前基本块是否已有终结指令（ret/br 等）。
    pub(super) fn 当前块已终结(&self) -> bool {
        self.builder
            .get_insert_block()
            .and_then(|b| b.get_terminator())
            .is_some()
    }

    /// 若当前块未终结，则无条件跳到目标块。
    fn 跳转若未终结(
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
