//! 模块顶层全局变量 / 常量降级。
//!
//! Qi 的全局是「每文件」的，但合并成单 LLVM 模块后同名会撞 —— 三项目里全局名各文件
//! 唯一（_连接/_基址/端口/主机…），故简单幂等（同名跳过重复登记）即可。
//!
//! 表示：每个全局 → LLVM global（先零初始化）。初值在 main 序言里 `生成表达式` 后 store
//! （字符串字面量也走序言 store，省去 const ptr initializer 的麻烦）。
//! 读写：`表达式.rs` 的标识符 / 赋值分支在 变量表(局部) miss 后 fallback 到 全局变量表。

use super::后端;
use super::类型::Qi类型;
use super::类型检查::推断表达式类型;
use crate::parser::ast::{AstNode, Program};

impl<'ctx> 后端<'ctx> {
    /// 登记所有模块顶层的全局变量 / 常量（零初始化 + 进符号表全局作用域）。
    /// 必须在函数体生成之前跑，否则函数里引用全局会定型不到。
    pub(super) fn 登记全局变量(&mut self, program: &Program) -> Result<(), String> {
        for stmt in &program.statements {
            if let AstNode::变量声明(vd) = stmt {
                // 幂等：同名全局已登记则跳过（跨文件合并）
                if self.全局变量表.contains_key(&vd.name) {
                    continue;
                }
                // 定型：注解优先（结构体感知），否则由初值推断
                let 注解类型 = vd.type_annotation.as_ref().map(|t| self.符号.解析类型(t));
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
                    .ok_or_else(|| format!("全局 {} 类型无效", vd.name))?;
                let g = self.module.add_global(llvmt, None, &vd.name);
                g.set_initializer(&llvmt.const_zero());
                self.全局变量表.insert(vd.name.clone(), (g, 类型));
                // 进符号表全局作用域（作用域栈底），供类型检查器在任意函数里定型
                self.符号.声明全局(&vd.name, 类型);
            }
        }
        Ok(())
    }

    /// 在 main 序言里初始化所有带初值的全局：`生成表达式(init)` → store 到 global。
    /// 调用点：`生成入口` 里、进入 body 之前。
    pub(super) fn 生成全局初始化(&mut self, programs: &[Program]) -> Result<(), String> {
        for program in programs {
            for stmt in &program.statements {
                if let AstNode::变量声明(vd) = stmt {
                    let init = match &vd.initializer {
                        Some(i) => i,
                        None => continue,
                    };
                    let (g, gt) = match self.全局变量表.get(&vd.name) {
                        Some(x) => (x.0, x.1),
                        None => continue,
                    };
                    // 幂等：同名全局只初始化一次（用一个已见集合防重复 store）
                    if self.已初始化全局.contains(&vd.name) {
                        continue;
                    }
                    self.已初始化全局.insert(vd.name.clone());

                    let (mut v, vt) = self
                        .生成表达式(init)?
                        .ok_or_else(|| format!("全局 {} 初值无值", vd.name))?;
                    if gt.是浮点() && !vt.是浮点() {
                        v = self
                            .builder
                            .build_signed_int_to_float(
                                v.into_int_value(),
                                self.ctx.f64_type(),
                                "sitofp",
                            )
                            .map_err(|e| e.to_string())?
                            .into();
                    }
                    // ARC：RC 值（字符串/结构体/数组）存进全局（程序生命周期，
                    // 永不释放）—— BORROWED retain / OWNED 转移；无需释放旧值
                    // （zeroinit，只初始化一次）
                    if self.弧开() && v.is_pointer_value() {
                        self.弧存入槽2(v, init, gt);
                    }
                    self.builder
                        .build_store(g.as_pointer_value(), v)
                        .map_err(|e| e.to_string())?;
                }
            }
        }
        Ok(())
    }
}
