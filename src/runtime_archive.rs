//! 选运行时归档：普通版 `libqi_runtime.a` / GUI 版 `libqi_runtime_gui.a`。
//!
//! 为什么要两份：GUI 版把 qi-gui（winit / softbuffer / rodio）编了进去，
//! 在 Linux 上它要链 ALSA（libasound）。服务器上的命令行程序不该因为
//! 运行时**能**画窗口，就被迫依赖音频库 —— 所以只有程序真用到
//! `标准库.图形化` 时才换成 GUI 版，并补上 GUI 那几个系统库。
//!
//! 两份归档导出同一套 `qi_gui_*` 符号：普通版里是桩（运行时报
//! 「GUI 库未安装」），GUI 版里是真实现。所以选错了不会链接失败，
//! 只会退回到老行为 —— 找不到 GUI 版时就是这么兜底的。
//!
//! 查找顺序（GUI 程序）：
//!   1. `QI_RUNTIME_GUI_LIB`
//!   2. `QI_RUNTIME_LIB`（显式指定的归档一律尊重 —— 老脚本拿它指过 GUI 版）
//!   3. 发布包 / 安装布局 `<prefix>/lib/qi/libqi_runtime_gui.a`
//!   4. 开发布局 `qi-runtime/target/gui/release/libqi_runtime.a`
//!      （`make runtime-gui` 编在独立 target 目录，不覆盖普通版）
//!   5. 都没有：退回普通版的查找
//!
//! 非 GUI 程序的查找顺序与以前完全相同。

use crate::{CompilerError, QiCompiler};
use std::path::{Path, PathBuf};

/// 程序（含所有被导入的用户模块 / 包）用没用到 GUI。
///
/// 两条路都算：`导入 标准库.图形化`（或短名 `图形化`），以及
/// `外部 "" { 函数 qi_gui_xxx(...) }` 直接声明 GUI 符号。
/// 构建缓存的键和链接用的是同一个判断，二者不会选出不同的归档。
pub fn uses_gui(programs: &[crate::parser::ast::Program]) -> bool {
    programs.iter().any(|p| {
        let imports_gui = p.imports.iter().any(|imp| {
            let path = imp.module_path.join(".");
            path == "标准库.图形化" || path == "图形化"
        });
        let declares_gui = p.statements.iter().any(|stmt| match stmt {
            crate::parser::ast::AstNode::外部声明(blk) => {
                blk.functions.iter().any(|f| f.name.starts_with("qi_gui_"))
            }
            _ => false,
        });
        imports_gui || declares_gui
    })
}

/// 归档文件名：unix `.a` / windows msvc `.lib`。
fn archive_name(gui: bool) -> &'static str {
    match (cfg!(windows), gui) {
        (true, false) => "qi_runtime.lib",
        (true, true) => "qi_runtime_gui.lib",
        (false, false) => "libqi_runtime.a",
        (false, true) => "libqi_runtime_gui.a",
    }
}

/// GUI 版运行时额外要的系统库，追加在运行时归档之后。
///
/// - macOS：AppKit / CoreAudio 那批 framework 一直都无条件链着（普通程序也带）；
///   这里补 `cargo rustc -- --print native-static-libs` 列出、而那批里没有的。
/// - Linux：winit / softbuffer 的 X11、Wayland、xkbcommon 都是运行时 dlopen，
///   链接期只剩 rodio → cpal → alsa-sys 硬链的 libasound。写成
///   `-l:libasound.so.2` 而不是 `-lasound`：后者要 `libasound.so` 这个
///   开发包里的软链，前者只要运行时包（libasound2 / libasound2t64），
///   用户机器上不用为了编 GUI 程序去装 -dev。
/// - Windows：winit / softbuffer / cpal 用到的 Win32 导入库。普通程序那份
///   清单之外的部分。
pub fn push_gui_system_libs(cmd: &mut std::process::Command) {
    if cfg!(target_os = "macos") {
        cmd.args([
            "-framework",
            "Foundation",
            "-framework",
            "ApplicationServices",
            "-lobjc",
        ]);
    } else if cfg!(target_os = "linux") {
        cmd.arg("-l:libasound.so.2");
    } else if cfg!(windows) {
        for lib in [
            "-lgdi32",
            "-ldwmapi",
            "-luxtheme",
            "-limm32",
            "-loleaut32",
            "-lcomctl32",
            "-lwinmm",
            "-lpropsys",
            "-lmsimg32",
        ] {
            cmd.arg(lib);
        }
    }
}

impl QiCompiler {
    /// 找宿主平台的 qi-runtime 归档（无 LLVM）。`gui` 为真时优先 GUI 版，见模块文档。
    pub(crate) fn find_host_runtime_library(&self, gui: bool) -> Result<PathBuf, CompilerError> {
        if gui {
            if let Some(p) = Self::find_gui_runtime_library() {
                Self::提醒归档过期(&p);
                return Ok(p);
            }
        }
        if let Ok(p) = std::env::var("QI_RUNTIME_LIB") {
            let path = PathBuf::from(p);
            if path.exists() {
                Self::提醒归档过期(&path);
                return Ok(path);
            }
        }
        let exe = std::env::current_exe().map_err(CompilerError::Io)?;
        let 归档名 = archive_name(false);
        // 安装后/发布包布局：<prefix>/bin/qi → <prefix>/lib/qi/<归档>
        // （installer 装到 /usr/local;release tar/zip 解压即用同构）
        if let Some(prefix) = exe.parent().and_then(|p| p.parent()) {
            let p = prefix.join("lib/qi").join(归档名);
            if p.exists() {
                Self::提醒归档过期(&p);
                return Ok(p);
            }
        }
        // 开发布局：qilang/target/debug/qi → 上溯到 qilang
        if let Some(ws) = workspace_root(&exe) {
            for profile in ["debug", "release"] {
                let p = ws.join("qi-runtime/target").join(profile).join(归档名);
                if p.exists() {
                    Self::提醒归档过期(&p);
                    return Ok(p);
                }
            }
        }
        Err(CompilerError::Codegen(
            "找不到 qi-runtime 归档。先在 qi-runtime/ 跑 cargo build，或设 QI_RUNTIME_LIB。"
                .to_string(),
        ))
    }

    /// GUI 版归档（查找顺序 1–4）。找不到返回 None，调用方退回普通版。
    fn find_gui_runtime_library() -> Option<PathBuf> {
        for var in ["QI_RUNTIME_GUI_LIB", "QI_RUNTIME_LIB"] {
            if let Ok(p) = std::env::var(var) {
                let path = PathBuf::from(p);
                if path.exists() {
                    return Some(path);
                }
            }
        }
        let exe = std::env::current_exe().ok()?;
        if let Some(prefix) = exe.parent().and_then(|p| p.parent()) {
            let p = prefix.join("lib/qi").join(archive_name(true));
            if p.exists() {
                return Some(p);
            }
        }
        let ws = workspace_root(&exe)?;
        let p = ws
            .join("qi-runtime/target/gui/release")
            .join(archive_name(false));
        p.exists().then_some(p)
    }
}

/// qilang/target/<profile>/qi → qilang
fn workspace_root(exe: &Path) -> Option<PathBuf> {
    exe.parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .map(Path::to_path_buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> crate::parser::ast::Program {
        crate::parser::Parser::new()
            .parse_source(src)
            .expect("测试源码应能解析")
    }

    #[test]
    fn plain_program_does_not_use_gui() {
        let p = parse("导入 标准库.输入输出;\n函数 入口() { 打印(\"hi\"); }\n");
        assert!(!uses_gui(&[p]));
    }

    #[test]
    fn importing_gui_module_uses_gui() {
        let p = parse("导入 标准库.图形化 作为 图形;\n函数 入口() { }\n");
        assert!(uses_gui(&[p]));
    }

    #[test]
    fn gui_import_in_any_module_counts() {
        let entry = parse("函数 入口() { }\n");
        let lib = parse("导入 标准库.图形化;\n函数 画() { }\n");
        assert!(uses_gui(&[entry, lib]));
    }
}
