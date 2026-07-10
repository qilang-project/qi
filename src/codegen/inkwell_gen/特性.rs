//! 特性（trait）注册 —— `特性 X { ... }` 声明 + `实现 X 对于 Y { ... }` 实现关系。
//!
//! 跑在类型名字登记之后、函数登记之前（mod.rs）：
//!   - 特性声明进 符号.特性（方法签名 + 默认体 AST）；
//!   - 特性实现进 符号.特性实现（(特性名, 结构体索引)）—— 泛型约束 `<T: X>`
//!     单态化时据此校验实参类型（见 泛型.rs）；
//!   - 实现块完整性校验：特性里没有默认体的方法，实现块必须逐个提供。
//!
//! v1 边界：特性名全局共享（跨包不消歧，与泛型模板同款）；只有结构体能实现特性。

use super::后端;
use super::类型检查::特性信息;
use crate::parser::ast::{AstNode, Program};

impl<'ctx> 后端<'ctx> {
    /// 登记一个模块的所有特性声明与特性实现（须已 设当前包）。
    pub(super) fn 登记特性(&mut self, program: &Program) -> Result<(), String> {
        // 先收声明（同一模块内 实现 可以写在 特性 前面）
        for stmt in &program.statements {
            if let AstNode::特性声明(td) = stmt {
                if self.符号.特性.contains_key(&td.name) {
                    return Err(format!("特性 {} 重复声明", td.name));
                }
                // 方法重名校验
                for (i, m) in td.methods.iter().enumerate() {
                    if td.methods[..i].iter().any(|p| p.name == m.name) {
                        return Err(format!("特性 {} 里方法 {} 重复声明", td.name, m.name));
                    }
                }
                self.符号.特性.insert(
                    td.name.clone(),
                    特性信息 {
                        方法: td.methods.clone(),
                    },
                );
            }
        }
        Ok(())
    }

    /// 登记一个模块的特性实现关系 + 完整性校验。
    /// 所有模块的 登记特性 跑完后再跑（特性可能声明在别的模块）。
    pub(super) fn 登记特性实现(&mut self, program: &Program) -> Result<(), String> {
        for stmt in &program.statements {
            let AstNode::实现块(imp) = stmt else { continue };
            let Some(特性名) = &imp.trait_name else {
                continue; // 固有实现 `实现 类型 { ... }`：与特性无关
            };
            let info = self.符号.特性.get(特性名).cloned().ok_or_else(|| {
                format!(
                    "实现块 `实现 {} 对于 {}` 引用了未声明的特性「{}」",
                    特性名, imp.target_type, 特性名
                )
            })?;
            let idx = self.符号.结构体索引(&imp.target_type).ok_or_else(|| {
                format!(
                    "实现块 `实现 {} 对于 {}` 的目标类型解析失败：{}",
                    特性名,
                    imp.target_type,
                    self.符号.结构体解析错误(&imp.target_type)
                )
            })?;
            // 完整性：无默认体的特性方法，实现块必须提供
            for tm in &info.方法 {
                if tm.default_body.is_some() {
                    continue;
                }
                if !imp.methods.iter().any(|m| m.method_name == tm.name) {
                    return Err(format!(
                        "类型「{}」实现特性「{}」缺少方法 {}（该方法没有默认实现，必须在实现块里提供）",
                        imp.target_type, 特性名, tm.name
                    ));
                }
            }
            self.符号.特性实现.insert((特性名.clone(), idx));
        }
        Ok(())
    }
}
