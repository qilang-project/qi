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
include!(concat!(env!("OUT_DIR"), "/qi_stdlib_builtin.rs"));

/// 虚拟路径前缀。嵌进二进制的模块没有真实文件，但整条模块收集流程都以
/// PathBuf 为 key（visited / compiled_modules / 私有函数按文件消歧），
/// 所以给它们编一个稳定且不可能与真实文件相撞的路径。
pub const VIRTUAL_PREFIX: &str = "<标准库>";

/// 这个路径是不是嵌入式标准库模块。
pub fn is_virtual_path(p: &Path) -> bool {
    p.components()
        .next()
        .map(|c| c.as_os_str() == VIRTUAL_PREFIX)
        == Some(true)
}

/// 模块名 → 虚拟路径（`<标准库>/JSON.qi`）。
pub fn virtual_path(name: &str) -> PathBuf {
    PathBuf::from(VIRTUAL_PREFIX).join(format!("{}.qi", name))
}

fn forced_to_ffi(name: &str) -> bool {
    match std::env::var("QI_STDLIB_FFI") {
        Ok(s) => s.split(',').any(|x| x.trim() == name),
        Err(_) => false,
    }
}

/// 开发覆盖目录：`QI_STDLIB_QI=/path/to/标准库`。
fn override_dir() -> Option<PathBuf> {
    std::env::var("QI_STDLIB_QI").ok().map(PathBuf::from)
}

/// 取模块的 qi 源码。没有 qi 实现（或被 QI_STDLIB_FFI 挡下）返回 None，
/// 调用方据此落回 FFI 注册表。
pub fn source(name: &str) -> Option<String> {
    if forced_to_ffi(name) {
        return None;
    }
    if let Some(dir) = override_dir() {
        let p = dir.join(format!("{}.qi", name));
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
    BUILTIN_SOURCES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, src)| (*src).to_string())
}

/// 从虚拟路径反查源码。
pub fn virtual_path_source(p: &Path) -> Option<String> {
    if !is_virtual_path(p) {
        return None;
    }
    let n = p.file_stem()?.to_str()?;
    source(n)
}

/// 有 qi 实现的模块名（已扣掉 QI_STDLIB_FFI 挡下的）。
pub fn modules_with_qi_impl() -> Vec<String> {
    BUILTIN_SOURCES
        .iter()
        .map(|(n, _)| (*n).to_string())
        .filter(|n| source(n).is_some())
        .collect()
}

/// 模块名 → 源码，给需要一次拿全的调用方（如 doctor / 测试）。
pub fn all_sources() -> HashMap<String, String> {
    modules_with_qi_impl()
        .into_iter()
        .filter_map(|n| source(&n).map(|src| (n, src)))
        .collect()
}
