//! qi.toml `[依赖]` 里声明的本地路径依赖必须真正参与导入解析。
//!
//! 这批用例的关键是**包放在祖先目录扫描够不着的地方**：依赖方在 甲/应用/，
//! 被依赖包在 乙/工具库/（是 乙 的子目录，不是任何一级祖先的直接子目录）。
//! 曾经的 bug 是 `[依赖]` 那行完全没参与解析，兄弟目录能跑通只是祖先扫描的
//! 巧合；这里的布局让巧合失效，声明失效就一定挂。

use std::path::{Path, PathBuf};

use qi_compiler::QiCompiler;
use tempfile::TempDir;

fn 写文件(path: &Path, content: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

/// 造出 甲/应用（含 `manifest` 指定的 qi.toml 内容）+ 乙/工具库 两棵子树，
/// 返回入口文件路径。TempDir 必须由调用方持有，否则会被提前删掉。
fn 搭建工程(root: &Path, manifest: &str) -> PathBuf {
    写文件(
        &root.join("乙/工具库/qi.toml"),
        "[包]\n名称 = \"工具\"\n入口 = \"工具.qi\"\n",
    );
    写文件(
        &root.join("乙/工具库/工具.qi"),
        "包 工具;\n\n公开 函数 平方(x: 整数): 整数 {\n    返回 x * x;\n}\n",
    );
    写文件(&root.join("甲/应用/qi.toml"), manifest);

    let entry = root.join("甲/应用/主程序.qi");
    写文件(
        &entry,
        "导入 工具::{平方};\n\n函数 入口() {\n    打印(平方(12));\n}\n",
    );
    entry
}

const 声明依赖: &str =
    "[包]\n名称 = \"应用\"\n入口 = \"主程序.qi\"\n\n[依赖]\n工具 = \"../../乙/工具库\"\n";

#[test]
fn 相对路径本地依赖能跨目录解析() {
    let temp = TempDir::new().unwrap();
    let entry = 搭建工程(temp.path(), 声明依赖);

    let programs = QiCompiler::new()
        .collect_programs(&entry)
        .expect("声明了 [依赖] 就该解析得到 工具 包");
    assert!(
        programs
            .iter()
            .any(|p| p.package_name.as_deref() == Some("工具")),
        "编译单元里应包含被依赖的 工具 包，实际: {:?}",
        programs.iter().map(|p| &p.package_name).collect::<Vec<_>>()
    );
}

#[test]
fn 绝对路径本地依赖能跨目录解析() {
    let temp = TempDir::new().unwrap();
    let 依赖目录 = temp.path().join("乙/工具库");
    let manifest = format!(
        "[包]\n名称 = \"应用\"\n入口 = \"主程序.qi\"\n\n[依赖]\n工具 = \"{}\"\n",
        依赖目录.display()
    );
    let entry = 搭建工程(temp.path(), &manifest);

    assert!(QiCompiler::new().collect_programs(&entry).is_ok());
}

/// 反向验证：没有这行声明就该失败。防止「把祖先扫描范围扩大」这种蒙混过关的改法
/// —— 那样上面两条也会绿，但声明依旧没起作用。
#[test]
fn 删掉依赖声明后跨目录解析必须失败() {
    let temp = TempDir::new().unwrap();
    let entry = 搭建工程(temp.path(), "[包]\n名称 = \"应用\"\n入口 = \"主程序.qi\"\n");

    let err = QiCompiler::new()
        .collect_programs(&entry)
        .expect_err("没有 [依赖] 声明时，祖先扫描够不着 乙/工具库，必须报找不到模块");
    assert!(
        err.to_string().contains("工具"),
        "错误信息应指出是哪个模块找不到: {}",
        err
    );
}

#[test]
fn 路径写错时报的是路径不存在() {
    let temp = TempDir::new().unwrap();
    let entry = 搭建工程(
        temp.path(),
        "[包]\n名称 = \"应用\"\n入口 = \"主程序.qi\"\n\n[依赖]\n工具 = \"../../乙/写错了\"\n",
    );

    let err = QiCompiler::new().collect_programs(&entry).unwrap_err();
    let text = err.to_string();
    assert!(
        text.contains("声明的本地路径不存在") && text.contains("写错了"),
        "应指明是声明的路径不存在: {}",
        text
    );
}

#[test]
fn 路径对但包不对时报的是包名不符() {
    let temp = TempDir::new().unwrap();
    let entry = 搭建工程(
        temp.path(),
        "[包]\n名称 = \"应用\"\n入口 = \"主程序.qi\"\n\n[依赖]\n工具 = \"../../丙/别的包\"\n",
    );
    写文件(
        &temp.path().join("丙/别的包/qi.toml"),
        "[包]\n名称 = \"别的\"\n入口 = \"别的.qi\"\n",
    );
    写文件(&temp.path().join("丙/别的包/别的.qi"), "包 别的;\n");

    let err = QiCompiler::new().collect_programs(&entry).unwrap_err();
    let text = err.to_string();
    assert!(
        text.contains("里是包 `别的`") && text.contains("没有名为 `工具` 的包"),
        "应指明该目录下的包名对不上: {}",
        text
    );
}

/// 兜底：路径写歪但包恰好躺在某级祖先目录下时，仍按老办法解析得到，
/// 免得历史工程升级即炸。
#[test]
fn 路径写歪时祖先目录扫描仍兜底() {
    let temp = TempDir::new().unwrap();
    let entry = 搭建工程(
        temp.path(),
        "[包]\n名称 = \"应用\"\n入口 = \"主程序.qi\"\n\n[依赖]\n工具 = \"./并不存在\"\n",
    );
    // 工具库同时放一份到依赖方的兄弟位置，构成祖先扫描能命中的老布局
    写文件(
        &entry.parent().unwrap().join("../工具库/qi.toml"),
        "[包]\n名称 = \"工具\"\n入口 = \"工具.qi\"\n",
    );
    写文件(
        &entry.parent().unwrap().join("../工具库/工具.qi"),
        "包 工具;\n\n公开 函数 平方(x: 整数): 整数 {\n    返回 x * x;\n}\n",
    );

    assert!(QiCompiler::new().collect_programs(&entry).is_ok());
}
