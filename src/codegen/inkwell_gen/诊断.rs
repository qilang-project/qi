//! `qi doctor` 静态段地基 —— 复用 codegen 的类型登记前几趟 + 环检测收集版，
//! 产出一份「内存拓扑 + 工程概况」的静态诊断，**不**做代码生成、不写任何产物。
//!
//! 只跑到「类型全部登记 + 建 LLVM 结构体类型」为止（这是环检测所需的最小前提），
//! 然后调 `环检测::收集循环引用环` 拿引用环列表。工程概况（结构体/枚举/函数/外部/
//! 导出计数）直接扫 AST，避免再跑函数登记趟（更少的失败面）。

use super::后端;
use crate::parser::ast::{AstNode, Program};
use inkwell::context::Context;

/// 静态诊断结果：给 doctor 报告的「静态拓扑」段用。
pub struct 静态诊断 {
    /// 检测到的引用环链（如 `"账户 → 交易 → 账户"`）。空 = 无环。
    pub 环列表: Vec<String>,
    /// 结构体类型数（含单态化实例）。
    pub 结构体数: usize,
    /// 枚举类型数（含选项/结果/单态化实例）。
    pub 枚举数: usize,
    /// 顶层函数声明数（不含方法 / 导出）。
    pub 函数数: usize,
    /// 外部 C 函数声明数（`外部 "库" { ... }` 内的签名总数）。
    pub 外部函数数: usize,
    /// 反向 FFI 导出函数数（`导出 函数 ...`）。
    pub 导出函数数: usize,
}

/// 对合并后的程序集跑静态分析。`programs[0]` 是 entry，其余为被导入的用户模块
/// （与真实编译同一口径 —— 跨模块引用环也能报）。
pub fn 静态分析(programs: &[Program]) -> Result<静态诊断, String> {
    if programs.is_empty() {
        return Err("没有可分析的模块".to_string());
    }

    // 只跑类型登记的前几趟（够环检测用），不生成函数体。
    let ctx = Context::create();
    let mut 后端值 = 后端::new(&ctx);
    后端值.声明运行时();

    for p in programs {
        后端值.收集导入别名(p);
        后端值.收集符号导入(p);
    }
    for p in programs {
        后端值.设当前包(p.package_name.clone());
        后端值.登记结构体名字(p)?;
    }
    for p in programs {
        后端值.设当前包(p.package_name.clone());
        后端值.登记枚举名字(p)?;
    }
    for p in programs {
        后端值.设当前包(p.package_name.clone());
        后端值.解析结构体字段(p)?;
    }
    for p in programs {
        后端值.设当前包(p.package_name.clone());
        后端值.解析枚举变体(p)?;
    }
    后端值.建结构体llvm类型()?;

    let 环列表 = super::环检测::收集循环引用环(&后端值.符号);

    let 结构体数 = 后端值.符号.结构体.len();
    let 枚举数 = 后端值.符号.枚举.len();

    // 函数 / 外部 / 导出计数直接扫 AST（顶层语句），无需再跑函数登记趟。
    let mut 函数数 = 0usize;
    let mut 外部函数数 = 0usize;
    let mut 导出函数数 = 0usize;
    for p in programs {
        for 语句 in &p.statements {
            match 语句 {
                AstNode::函数声明(_) => 函数数 += 1,
                AstNode::外部声明(blk) => 外部函数数 += blk.functions.len(),
                AstNode::导出函数(_) => 导出函数数 += 1,
                _ => {}
            }
        }
    }

    Ok(静态诊断 {
        环列表,
        结构体数,
        枚举数,
        函数数,
        外部函数数,
        导出函数数,
    })
}
