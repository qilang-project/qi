//! 项目根导入：项目子目录（`测试/`、`工具/`…）里的文件 `导入 标记;` 能拿到
//! 项目根（最近一个 qi.toml 所在目录）下的 `标记.qi`。
//!
//! 一半用例是**反例**：它必须有边界。2.0 删掉祖先目录扫描，就是因为祖先路径上
//! 任何一份残留副本都能静默盖掉真依赖。这里钉住三条边界 ——
//! 只认最近的一个 qi.toml、不越过它往上找、不出仓库（.git / qi工作区.toml）。

use std::path::{Path, PathBuf};

use qi_compiler::QiCompiler;
use tempfile::TempDir;

fn 写文件(path: &Path, content: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

fn 写模块(path: &Path, 包名: &str) {
    写文件(
        path,
        &format!("包 {包名};\n\n公开 函数 加粗(s: 字符串): 字符串 {{\n    返回 s;\n}}\n"),
    );
}

fn 写入口(path: &Path, 导入: &str) -> PathBuf {
    let 名 = 导入.rsplit(['/', '.']).next().unwrap();
    写文件(
        path,
        &format!("包 主程序;\n导入 {导入};\n\n函数 入口() {{\n    打印行({名}.加粗(\"x\"));\n}}\n"),
    );
    path.to_path_buf()
}

/// 解析成功时返回被导入模块实际读的文件。
fn 解析到(entry: &Path, 包名: &str) -> Result<PathBuf, String> {
    let programs = QiCompiler::new()
        .collect_programs(&entry.to_path_buf())
        .map_err(|e| e.to_string())?;
    programs
        .iter()
        .find(|p| p.package_name.as_deref() == Some(包名))
        .and_then(|p| p.source_path.clone())
        .map(PathBuf::from)
        .ok_or_else(|| format!("编译单元里没有包 {包名}"))
}

fn 同一文件(a: &Path, b: &Path) -> bool {
    a.canonicalize().unwrap() == b.canonicalize().unwrap()
}

#[test]
fn 子目录能导入项目根的模块() {
    let t = TempDir::new().unwrap();
    let 根 = t.path().join("文字编辑");
    写文件(&根.join("qi.toml"), "[包]\n名称 = \"文字编辑\"\n");
    写模块(&根.join("标记.qi"), "标记");
    let entry = 写入口(&根.join("测试/标记测试.qi"), "标记");

    let 读的 = 解析到(&entry, "标记").expect("项目根下的 标记.qi 应该能直接导入");
    assert!(同一文件(&读的, &根.join("标记.qi")));
}

#[test]
fn 多级子目录也能导入项目根的模块() {
    let t = TempDir::new().unwrap();
    let 根 = t.path().join("应用");
    写文件(&根.join("qi.toml"), "[包]\n名称 = \"应用\"\n");
    写模块(&根.join("工具/字符.qi"), "字符");
    let entry = 写入口(&根.join("测试/单元/深/t.qi"), "工具.字符");

    let 读的 = 解析到(&entry, "字符").expect("点分路径也按项目根展开");
    assert!(同一文件(&读的, &根.join("工具/字符.qi")));
}

#[test]
fn 同目录的模块优先于项目根() {
    let t = TempDir::new().unwrap();
    let 根 = t.path().join("应用");
    写文件(&根.join("qi.toml"), "[包]\n名称 = \"应用\"\n");
    写模块(&根.join("标记.qi"), "标记");
    写模块(&根.join("测试/标记.qi"), "标记");
    let entry = 写入口(&根.join("测试/t.qi"), "标记");

    let 读的 = 解析到(&entry, "标记").unwrap();
    assert!(
        同一文件(&读的, &根.join("测试/标记.qi")),
        "离得近的赢，实际读了 {}",
        读的.display()
    );
}

#[test]
fn 项目根的源码目录也算() {
    let t = TempDir::new().unwrap();
    let 根 = t.path().join("应用");
    写文件(
        &根.join("qi.toml"),
        "[包]\n名称 = \"应用\"\n\n[源码]\n目录 = [\"源码\"]\n",
    );
    写模块(&根.join("源码/标记.qi"), "标记");
    let entry = 写入口(&根.join("测试/t.qi"), "标记");

    let 读的 = 解析到(&entry, "标记").expect("[源码] 目录 下的模块也是项目根的模块");
    assert!(同一文件(&读的, &根.join("源码/标记.qi")));
}

// ───────────────────────────── 反例：边界 ─────────────────────────────

#[test]
fn 不越过项目根往上找() {
    let t = TempDir::new().unwrap();
    写模块(&t.path().join("外层/外面.qi"), "外面");
    let 根 = t.path().join("外层/项目");
    写文件(&根.join("qi.toml"), "[包]\n名称 = \"项目\"\n");
    let entry = 写入口(&根.join("测试/越界.qi"), "外面");

    let err = 解析到(&entry, "外面").expect_err("项目根外面的模块不许被找到");
    assert!(err.contains("无法找到导入模块: 外面"), "{err}");
    // 报错要说清楚为什么没找到、怎么写才对
    assert!(err.contains("项目根查找不会越过"), "{err}");
    assert!(err.contains("导入 ../../外面;"), "{err}");
}

#[test]
fn 只认最近的一个_qi_toml() {
    let t = TempDir::new().unwrap();
    // 外层自己也是个项目，根下有 标记.qi —— 子项目里不该看见它
    写文件(&t.path().join("外层/qi.toml"), "[包]\n名称 = \"外层\"\n");
    写模块(&t.path().join("外层/标记.qi"), "标记");
    let 子 = t.path().join("外层/子项目");
    写文件(&子.join("qi.toml"), "[包]\n名称 = \"子项目\"\n");
    let entry = 写入口(&子.join("测试/t.qi"), "标记");

    let err = 解析到(&entry, "标记").expect_err("不能跳过最近的 qi.toml 去找第二个");
    assert!(err.contains("无法找到导入模块: 标记"), "{err}");
}

#[test]
fn 不出仓库() {
    let t = TempDir::new().unwrap();
    // 仓库外面有 qi.toml + 同名模块；仓库里没有 qi.toml
    写文件(&t.path().join("qi.toml"), "[包]\n名称 = \"外面\"\n");
    写模块(&t.path().join("标记.qi"), "标记");
    let 仓库 = t.path().join("仓库");
    std::fs::create_dir_all(仓库.join(".git")).unwrap();
    let entry = 写入口(&仓库.join("测试/t.qi"), "标记");

    let err = 解析到(&entry, "标记").expect_err("仓库根（.git）以外的 qi.toml 不算项目根");
    assert!(err.contains("无法找到导入模块: 标记"), "{err}");
}

#[test]
fn 没有_qi_toml_时报错给出相对导入写法() {
    let t = TempDir::new().unwrap();
    写模块(&t.path().join("编辑器/标记.qi"), "标记");
    let entry = 写入口(&t.path().join("编辑器/测试/t.qi"), "标记");

    let err = 解析到(&entry, "标记").expect_err("没有 qi.toml 就没有项目根");
    assert!(err.contains("导入 ../标记;"), "{err}");
    assert!(err.contains("qi.toml"), "{err}");
}

#[test]
fn 相对导入一直可用() {
    let t = TempDir::new().unwrap();
    写模块(&t.path().join("编辑器/标记.qi"), "标记");
    let entry = 写入口(&t.path().join("编辑器/测试/t.qi"), "../标记");

    let 读的 = 解析到(&entry, "标记").expect("`导入 ../标记;` 不需要 qi.toml");
    assert!(同一文件(&读的, &t.path().join("编辑器/标记.qi")));
}
