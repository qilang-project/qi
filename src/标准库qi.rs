//! 用 qi 写的标准库模块。
//!
//! `导入 标准库.X` 一直是纯 FFI：编译器在 module_registry 的 724 条表里查 X::函数，
//! 直接发射对 libqi_runtime.a 里某个 C 符号的调用，**从不解析任何文件**。
//! 这意味着标准库里每一行都必须是 Rust，哪怕那个函数只是在拼字符串。
//!
//! 这里开的是另一条路：先在本表里找 X 的 qi 实现，找到就当普通 qi 模块编译进去，
//! 找不到才落回 FFI 表。两条路互不干扰，可以一个模块一个模块地搬。
//!
//! ## 为什么把源码嵌进二进制，而不是装到 /usr/local/lib/qi/ 下
//!
//! 装出去就得在运行时探路径，而这条路上已经踩过好几次：装的 `qi` 是拷贝、
//! 发布包忘了钉运行时 tag、交叉编译出来的产物找不到同版本资产。标准库跟编译器
//! 是同一件东西的两半，版本必须严格一致 —— 嵌进去就不可能对不上，交叉编译、
//! docker distroless、`qi` 二进制单文件分发也全都不用额外带资产。
//!
//! 开发时想改了立刻见效，用 `QI_STDLIB_QI=<目录>` 覆盖（见 源码）。
//!
//! ## 逃生口
//!
//! `QI_STDLIB_FFI=模块1,模块2` 强制这几个模块走回 FFI 实现。留这个口子是因为
//! qi 版和 Rust 版的性能不会一样（JSON 尤其 —— 它在 qi-web 的热路径上），
//! 要能当场 A/B 对照，而不是只能靠改代码重编来比。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

// build.rs 扫 `标准库/` 目录生成，内容形如：
// `pub static 内置源码: &[(&str, &str)] = &[("JSON", include_str!(...)), ...];`
include!(concat!(env!("OUT_DIR"), "/标准库qi内置.rs"));

/// 虚拟路径前缀。嵌进二进制的模块没有真实文件，但整条模块收集流程都以
/// PathBuf 为 key（visited / compiled_modules / 私有函数按文件消歧），
/// 所以给它们编一个稳定且不可能与真实文件相撞的路径。
pub const 虚拟前缀: &str = "<标准库>";

/// 这个路径是不是嵌入式标准库模块。
pub fn 是虚拟路径(p: &Path) -> bool {
    p.components().next().map(|c| c.as_os_str() == 虚拟前缀) == Some(true)
}

/// 模块名 → 虚拟路径（`<标准库>/JSON.qi`）。
pub fn 虚拟路径(模块名: &str) -> PathBuf {
    PathBuf::from(虚拟前缀).join(format!("{}.qi", 模块名))
}

fn 强制走ffi(模块名: &str) -> bool {
    match std::env::var("QI_STDLIB_FFI") {
        Ok(s) => s.split(',').any(|x| x.trim() == 模块名),
        Err(_) => false,
    }
}

/// 开发覆盖目录：`QI_STDLIB_QI=/path/to/标准库`。
fn 覆盖目录() -> Option<PathBuf> {
    std::env::var("QI_STDLIB_QI").ok().map(PathBuf::from)
}

/// 取模块的 qi 源码。没有 qi 实现（或被 QI_STDLIB_FFI 挡下）返回 None，
/// 调用方据此落回 FFI 注册表。
pub fn 源码(模块名: &str) -> Option<String> {
    if 强制走ffi(模块名) {
        return None;
    }
    if let Some(目录) = 覆盖目录() {
        let p = 目录.join(format!("{}.qi", 模块名));
        if p.is_file() {
            // 覆盖目录读不出来是配置错误，不该静默退回内置版本装作没事 ——
            // 那会让人对着改过的源码百思不得其解。
            return Some(
                std::fs::read_to_string(&p).unwrap_or_else(|e| {
                    panic!("QI_STDLIB_QI 指定的 {} 读不出来: {e}", p.display())
                }),
            );
        }
        // 覆盖目录里没有这个模块 → 照常用内置的，允许只覆盖一部分。
    }
    内置源码
        .iter()
        .find(|(名, _)| *名 == 模块名)
        .map(|(_, 源)| (*源).to_string())
}

/// 从虚拟路径反查源码。
pub fn 虚拟路径源码(p: &Path) -> Option<String> {
    if !是虚拟路径(p) {
        return None;
    }
    let 名 = p.file_stem()?.to_str()?;
    源码(名)
}

/// 有 qi 实现的模块名（已扣掉 QI_STDLIB_FFI 挡下的）。
pub fn 有qi实现的模块() -> Vec<String> {
    内置源码
        .iter()
        .map(|(名, _)| (*名).to_string())
        .filter(|名| 源码(名).is_some())
        .collect()
}

/// 模块名 → 源码，给需要一次拿全的调用方（如 doctor / 测试）。
pub fn 全部() -> HashMap<String, String> {
    有qi实现的模块()
        .into_iter()
        .filter_map(|名| 源码(&名).map(|源| (名, 源)))
        .collect()
}
