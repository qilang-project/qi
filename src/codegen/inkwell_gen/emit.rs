//! 模块收尾：死代码裁剪 + 优化管线 + 目标文件发射。
//!
//! ## 为什么要裁剪（2026-09-25 编译速度实测）
//!
//! qi 把 entry 和它导入的**整个包**（qi-web + qi-harness ≈ 3.7 万行）合并成
//! 一个 LLVM 模块编译。所有函数原先都是默认的 external 链接 —— 对 LLVM 来说
//! 「外面可能有人调」，于是一个也删不掉：家庭待办 2531 个函数定义，从 main
//! 可达的只有 258 个（IR 字节 19%），剩下八成照样跑完 O1 优化 + 指令选择 +
//! 寄存器分配。那 6.2 秒里 5.2 秒花在 LLVM 上，大头就是这些死代码。
//!
//! 可执行文件只有 `main` 一个对外入口：运行时归档不按名字引用任何 qi 生成的
//! 符号（nm 核对过：libqi_runtime.a 的未定义符号与 qi .o 的定义交集为空），
//! 插件热加载 dlopen 的是**库模式**产物（那条路径不走这里）。所以可执行模式
//! 下把除 main 以外的函数定义全部改成 internal，再跑一遍 globaldce，
//! 不可达的函数整片消失；被取地址的（函数值 / C 回调 / goroutine 蹦床）
//! 仍有引用，保留。
//!
//! 库模式（导出 C ABI）不裁剪：导出包装与构造器都要对外可见。

use inkwell::module::{Linkage, Module};
use inkwell::passes::PassBuilderOptions;
use inkwell::targets::TargetMachine;

/// 可执行模式：除 `main` 外的函数定义一律 internal，然后 globaldce 删掉不可达的。
///
/// 只动**有函数体**的定义（声明是外部符号 / 运行时符号，不能碰），
/// 也不动已经是 internal / private / appending 等非 external 链接的。
pub(super) fn prune_unreachable(module: &Module<'_>, tm: &TargetMachine) -> Result<(), String> {
    let mut f = module.get_first_function();
    while let Some(func) = f {
        f = func.get_next_function();
        if func.count_basic_blocks() == 0 {
            continue; // 声明
        }
        if func.get_linkage() != Linkage::External {
            continue;
        }
        let name = func.get_name().to_bytes();
        if name == b"main" {
            continue;
        }
        func.set_linkage(Linkage::Internal);
    }
    module
        .run_passes("globaldce", tm, PassBuilderOptions::create())
        .map_err(|e| format!("死代码裁剪失败(globaldce): {}", e))
}
