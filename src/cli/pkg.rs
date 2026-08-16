//! `qi 包` 子命令族 —— 注册中心包管理客户端。
//!
//! - `qi 包 安装`               读 [依赖]（或 qi.lock）→ 下载解压到 ./qi_packages/ → 写 qi.lock
//! - `qi 包 添加 <名称> <版本>` 写进 qi.toml [依赖] 并立即安装
//! - `qi 包 发布`               打包当前目录 PUT 上去；token 读 QI_REGISTRY_TOKEN
//! - `qi 包 搜索 <关键词>`      按名称/描述过滤包列表
//!
//! 协议合同见 `docs/包管理设计.md`。

use std::path::Path;

use crate::cli::commands::CliError;
use crate::package::registry::{注册中心, TOKEN_ENV};
use crate::package::{archive, install, is_exact_version, read_registry_lock};
use crate::package::{DependencySource, ResolvedPackageManifest};

fn 包错(消息: impl Into<String>) -> CliError {
    CliError::Package(消息.into())
}

/// 从当前目录向上找项目 qi.toml。
fn 找项目清单() -> Result<ResolvedPackageManifest, CliError> {
    let cwd = std::env::current_dir().map_err(CliError::Io)?;
    ResolvedPackageManifest::discover(&cwd)
        .map_err(包错)?
        .ok_or_else(|| {
            包错("当前目录及其上级没有找到 qi.toml。\n请在 Qi 项目目录内运行，或先创建 qi.toml。")
        })
}

// ───────────────────────── 安装 ─────────────────────────

/// `qi 包 安装`
pub fn 安装(verbose: bool) -> Result<(), CliError> {
    let 项目 = 找项目清单()?;
    安装到(&项目, verbose)
}

/// 把项目清单里全部注册中心依赖装齐，并刷新 qi.lock。
fn 安装到(项目: &ResolvedPackageManifest, verbose: bool) -> Result<(), CliError> {
    let 依赖表 = 项目.registry_dependencies();
    if 依赖表.is_empty() {
        // 有别的类型的依赖时提示一句，免得用户以为 qi.toml 没被读到
        let 其他 = 项目.manifest.dependencies.len();
        if 其他 > 0 {
            println!("qi.toml 的 [依赖] 里没有注册中心依赖（{} 条是本地路径或 git 依赖，用 qi get 拉取）", 其他);
        } else {
            println!("qi.toml 的 [依赖] 是空的，没有要装的包");
        }
        return Ok(());
    }

    let 中心 = 注册中心::新建().map_err(包错)?;
    // lock 优先：有 lock 就按 lock 里钉死的 sha256 校验，没有才纯按 qi.toml 装
    let 锁定表 = read_registry_lock(&项目.root_dir);
    if verbose && !锁定表.is_empty() {
        println!("按 qi.lock 校验（{} 条锁定记录）", 锁定表.len());
    }

    let mut 装了 = 0usize;
    let mut 跳过 = 0usize;
    for (别名, 版本) in &依赖表 {
        if !is_exact_version(版本) {
            return Err(包错(format!(
                "依赖 `{}` 的版本 \"{}\" 不是「主.次.补」三段数字。\n  v1 只支持精确版本（不做 ^1.2 这类范围解析），请写成如 0.1.0",
                别名, 版本
            )));
        }

        // lock 里同名同版本才用它的 sha256；版本不一样说明 qi.toml 刚被改过，lock 已过期
        let 锁定sha256 = 锁定表
            .get(别名)
            .filter(|锁| &锁.版本 == 版本)
            .map(|锁| 锁.sha256.as_str());

        let 包目录 = 项目.registry_package_dir(别名);
        let 结果 = install::安装一个包(&中心, 别名, 版本, &包目录, 锁定sha256)
            .map_err(|e| 包错(format!("安装 {} {} 失败: {}", 别名, 版本, e)))?;

        match 结果 {
            install::安装结果::已是(v) => {
                println!("{} 已是 {}", 别名, v);
                跳过 += 1;
            }
            install::安装结果::已装 {
                版本: v,
                sha256,
                文件数,
            } => {
                println!(
                    "已安装 {} {} ({} 个文件) → {}",
                    别名,
                    v,
                    文件数,
                    包目录.display()
                );
                if verbose {
                    println!("  sha256 {}", sha256);
                }
                装了 += 1;
            }
        }
    }

    // 重新加载清单：装好的包各自带 qi.toml，lock 要把它们的元信息写全
    let 刷新后 = ResolvedPackageManifest::load_dir(&项目.root_dir)
        .map_err(包错)?
        .unwrap_or_else(|| 项目.clone());
    let lock路径 = ResolvedPackageManifest::write_lock_file_for_manifest(&刷新后).map_err(包错)?;

    println!(
        "共 {} 个依赖：新装 {}，跳过 {}；已更新 {}",
        依赖表.len(),
        装了,
        跳过,
        lock路径.display()
    );
    Ok(())
}

// ───────────────────────── 添加 ─────────────────────────

/// `qi 包 添加 <名称> [版本]`
///
/// 省略版本 = 取注册中心的最新版（`go get` 不写版本时的行为）。解析出来的
/// **具体版本号**才写进 qi.toml —— 清单里始终是精确版本，重跑安装必然装到同一份，
/// 「最新」只发生在你敲命令的那一刻，不会随时间漂移。
pub fn 添加(名称: String, 版本: Option<String>, verbose: bool) -> Result<(), CliError> {
    let 版本 = match 版本 {
        Some(v) => v,
        None => 解析最新版(&名称)?,
    };
    if !is_exact_version(&版本) {
        return Err(包错(format!(
            "版本 \"{}\" 不是「主.次.补」三段数字。\n  v1 只支持精确版本，请写成如 0.1.0",
            版本
        )));
    }

    let 项目 = 找项目清单()?;
    写依赖(&项目.manifest_path, &名称, &版本).map_err(包错)?;
    println!(
        "已写入依赖: {} = \"{}\" → {}",
        名称,
        版本,
        项目.manifest_path.display()
    );

    // 重新读清单（刚被改写过），再走一遍完整安装
    let 刷新后 = ResolvedPackageManifest::load_dir(&项目.root_dir)
        .map_err(包错)?
        .ok_or_else(|| 包错("改写 qi.toml 后反而读不出来了，请检查文件内容"))?;
    安装到(&刷新后, verbose)
}

/// 问注册中心要某个包的最新版本号。找不到包时把「搜索」指出来，别只甩一个 404。
fn 解析最新版(名称: &str) -> Result<String, CliError> {
    let 中心 = 注册中心::新建().map_err(包错)?;
    let 全部 = 中心.列出包().map_err(包错)?;

    let 命中 = 全部.iter().find(|包| 包.name == 名称).ok_or_else(|| {
        包错(format!(
            "注册中心 {} 上没有名为 \"{}\" 的包。\n  用 qi 包 搜索 {} 看看有没有相近的",
            中心.地址(),
            名称,
            名称
        ))
    })?;

    let 版本 = 命中.latest.clone().ok_or_else(|| {
        包错(format!(
            "包 \"{}\" 在注册中心存在但还没有任何已发布版本",
            名称
        ))
    })?;

    println!("解析 {} 最新版 → {}", 名称, 版本);
    Ok(版本)
}

/// 保守地把 `名称 = "版本"` 写进 qi.toml 的 [依赖] 表。
///
/// 逐行编辑而不是 toml 反序列化再写回：后者会重排键、丢注释、把中文表头
/// 加上引号 —— 用户手写的 qi.toml 被工具改得面目全非是最招人烦的事。
/// 这份逻辑与 `qi get` 的 `update_manifest_dependency` 同源，行为保持一致。
fn 写依赖(清单路径: &Path, 名称: &str, 版本: &str) -> Result<(), String> {
    let 内容 = std::fs::read_to_string(清单路径)
        .map_err(|e| format!("读取 {} 失败: {}", 清单路径.display(), e))?;
    let mut 行: Vec<String> = 内容.lines().map(String::from).collect();
    let 新行 = format!("{} = \"{}\"", 名称, 版本);

    let 表头 = 行.iter().position(|line| {
        let t = line.trim();
        t == "[依赖]" || t == "[dependencies]" || t == "[\"依赖\"]"
    });

    match 表头 {
        Some(头) => {
            // 表范围：表头之后到下一个表头（或文件尾）
            let 表尾 = 行[头 + 1..]
                .iter()
                .position(|line| line.trim_start().starts_with('['))
                .map(|偏移| 头 + 1 + 偏移)
                .unwrap_or(行.len());

            // 已有同名条目 → 原地替换（等价于「改版本」）
            for line in 行.iter_mut().take(表尾).skip(头 + 1) {
                let t = line.trim_start();
                if t.starts_with('#') || !t.contains('=') {
                    continue;
                }
                let 键 = t.split('=').next().unwrap_or("").trim().trim_matches('"');
                if 键 == 名称 {
                    *line = 新行;
                    return 写回(清单路径, 行);
                }
            }

            // 追加到表末尾（跳过表尾空行，让新条目紧跟已有的）
            let mut 插入点 = 表尾;
            while 插入点 > 头 + 1 && 行[插入点 - 1].trim().is_empty() {
                插入点 -= 1;
            }
            行.insert(插入点, 新行);
        }
        None => {
            if !行.is_empty() && !行.last().map(|l| l.trim().is_empty()).unwrap_or(true) {
                行.push(String::new());
            }
            行.push("[依赖]".to_string());
            行.push(新行);
        }
    }

    写回(清单路径, 行)
}

fn 写回(清单路径: &Path, 行: Vec<String>) -> Result<(), String> {
    let mut 输出 = 行.join("\n");
    输出.push('\n');
    std::fs::write(清单路径, 输出).map_err(|e| format!("写入 {} 失败: {}", 清单路径.display(), e))
}

// ───────────────────────── 发布 ─────────────────────────

/// `qi 包 发布`
pub fn 发布(打包后不发: bool, verbose: bool) -> Result<(), CliError> {
    let 项目 = 找项目清单()?;

    let 名称 = 项目.package_name().ok_or_else(|| {
        包错(format!(
            "{} 里没有 [包] 名称，无法发布",
            项目.manifest_path.display()
        ))
    })?;
    let 版本 = 项目
        .manifest
        .package
        .as_ref()
        .and_then(|p| p.version.clone())
        .ok_or_else(|| {
            包错(format!(
                "{} 里没有 [包] 版本，无法发布",
                项目.manifest_path.display()
            ))
        })?;

    if !is_exact_version(&版本) {
        return Err(包错(format!(
            "[包] 版本 \"{}\" 不是「主.次.补」三段数字，注册中心只收这种形式",
            版本
        )));
    }

    let 包体 = archive::打包(&项目.root_dir).map_err(包错)?;
    let sha256 = archive::算sha256(&包体);
    println!(
        "已打包 {} {}：{} 字节，sha256 {}",
        名称,
        版本,
        包体.len(),
        sha256
    );
    if verbose {
        println!("  排除: {}", archive::排除目录.join(" "));
    }

    // 自检：包里那份 qi.toml 必须跟发布地址一致，不然服务端会 400，
    // 与其等一个远程往返再报错，不如本地当场说清楚。
    match archive::读包内清单(&包体).map_err(包错)? {
        Some(_) => {}
        None => {
            return Err(包错(
                "打出来的包里没有包根 qi.toml —— 这不该发生，请报 bug",
            ))
        }
    }

    if 打包后不发 {
        println!("（--只打包）没有发布，包体未上传");
        return Ok(());
    }

    let 令牌 = std::env::var(TOKEN_ENV).unwrap_or_default();
    if 令牌.trim().is_empty() {
        return Err(包错(format!(
            "没有发布 token：请设置环境变量 {}\n  token 由注册中心管理员签发（v1 不做自助注册）",
            TOKEN_ENV
        )));
    }

    let 中心 = 注册中心::新建().map_err(包错)?;
    println!("发布到 {} …", 中心.地址());
    中心.发布(名称, &版本, 包体, 令牌.trim()).map_err(包错)?;

    println!("已发布 {} {} → {}", 名称, 版本, 中心.地址());
    Ok(())
}

// ───────────────────────── 搜索 ─────────────────────────

/// `qi 包 搜索 <关键词>`
pub fn 搜索(关键词: String) -> Result<(), CliError> {
    let 中心 = 注册中心::新建().map_err(包错)?;
    let 全部 = 中心.列出包().map_err(包错)?;

    let 词 = 关键词.to_lowercase();
    let mut 命中: Vec<_> = 全部
        .into_iter()
        .filter(|包| {
            包.name.to_lowercase().contains(&词)
                || 包
                    .description
                    .as_deref()
                    .map(|d| d.to_lowercase().contains(&词))
                    .unwrap_or(false)
        })
        .collect();
    命中.sort_by(|a, b| a.name.cmp(&b.name));

    if 命中.is_empty() {
        println!("在 {} 上没有匹配「{}」的包", 中心.地址(), 关键词);
        return Ok(());
    }

    // 列宽按**显示宽度**算：中文包名在终端占两格，按字符数对齐会歪一片
    let 名称宽 = 命中
        .iter()
        .map(|包| 显示宽度(&包.name))
        .chain(std::iter::once(显示宽度("名称")))
        .max()
        .unwrap_or(4);
    let 版本宽 = 命中
        .iter()
        .map(|包| 显示宽度(包.latest.as_deref().unwrap_or("-")))
        .chain(std::iter::once(显示宽度("最新版本")))
        .max()
        .unwrap_or(8);

    println!(
        "{}  {}  说明",
        补齐("名称", 名称宽),
        补齐("最新版本", 版本宽)
    );
    for 包 in &命中 {
        println!(
            "{}  {}  {}",
            补齐(&包.name, 名称宽),
            补齐(包.latest.as_deref().unwrap_or("-"), 版本宽),
            包.description.as_deref().unwrap_or("")
        );
    }
    println!("\n共 {} 个包（来自 {}）", 命中.len(), 中心.地址());
    println!("安装: qi 包 添加 <名称> <版本>");
    Ok(())
}

/// 终端显示宽度：CJK / 全角标点算 2 格，其余算 1 格。
fn 显示宽度(文本: &str) -> usize {
    文本.chars().map(|c| if 是宽字符(c) { 2 } else { 1 }).sum()
}

fn 是宽字符(c: char) -> bool {
    let n = c as u32;
    (0x1100..=0x115F).contains(&n)      // 韩文字母
        || (0x2E80..=0xA4CF).contains(&n)   // CJK 部首 / 汉字 / 假名
        || (0xAC00..=0xD7A3).contains(&n)   // 韩文音节
        || (0xF900..=0xFAFF).contains(&n)   // CJK 兼容汉字
        || (0xFE30..=0xFE6F).contains(&n)   // CJK 兼容标点
        || (0xFF00..=0xFF60).contains(&n)   // 全角 ASCII
        || (0xFFE0..=0xFFE6).contains(&n)
}

fn 补齐(文本: &str, 宽度: usize) -> String {
    let 当前 = 显示宽度(文本);
    if 当前 >= 宽度 {
        文本.to_string()
    } else {
        format!("{}{}", 文本, " ".repeat(宽度 - 当前))
    }
}

// ───────────────────────── 列出（本地已装）─────────────────────────

/// `qi 包 列出` —— 看本项目 qi_packages 里都装了什么。
pub fn 列出() -> Result<(), CliError> {
    let 项目 = 找项目清单()?;
    let 依赖表 = 项目.registry_dependencies();
    if 依赖表.is_empty() {
        println!("qi.toml 里没有注册中心依赖");
        return Ok(());
    }
    for (别名, 版本) in &依赖表 {
        let 目录 = 项目.registry_package_dir(别名);
        match install::读标记(&目录) {
            Some(标记) if &标记.版本 == 版本 => {
                println!("{} {}  已装  {}", 别名, 版本, 标记.来源)
            }
            Some(标记) => println!(
                "{} {}  版本不符（已装 {}），跑 qi 包 安装",
                别名, 版本, 标记.版本
            ),
            None => println!("{} {}  未安装，跑 qi 包 安装", 别名, 版本),
        }
    }
    Ok(())
}

/// 项目里非注册中心依赖的条数（给 `安装` 的提示用，也便于测试）。
#[allow(dead_code)]
pub(crate) fn 非注册中心依赖数(项目: &ResolvedPackageManifest) -> usize {
    项目
        .manifest
        .dependencies
        .values()
        .filter(|dep| !matches!(dep.source(), Ok(DependencySource::Registry { .. })))
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn 改(内容: &str, 名称: &str, 版本: &str) -> String {
        let 临时 = tempfile::TempDir::new().unwrap();
        let 路径 = 临时.path().join("qi.toml");
        std::fs::write(&路径, 内容).unwrap();
        写依赖(&路径, 名称, 版本).unwrap();
        std::fs::read_to_string(&路径).unwrap()
    }

    #[test]
    fn test_写依赖新建表() {
        let 后 = 改("[包]\n名称 = \"主程序\"\n", "海龟", "0.1.0");
        assert!(后.contains("[依赖]\n海龟 = \"0.1.0\"\n"), "实际:\n{}", 后);
    }

    #[test]
    fn test_写依赖保留注释与其他表() {
        let 前 = "# 我的项目\n[包]\n名称 = \"主程序\" # 别动\n\n[依赖]\n本地 = \"../本地\"\n\n[源码]\n目录 = [\".\"]\n";
        let 后 = 改(前, "海龟", "0.2.0");
        assert!(后.contains("# 我的项目"));
        assert!(后.contains("名称 = \"主程序\" # 别动"));
        assert!(后.contains("本地 = \"../本地\"\n海龟 = \"0.2.0\""));
        assert!(后.contains("[源码]"));
    }

    #[test]
    fn test_写依赖替换同名() {
        let 后 = 改("[依赖]\n海龟 = \"0.1.0\"\n", "海龟", "0.3.0");
        assert!(后.contains("海龟 = \"0.3.0\""));
        assert!(!后.contains("0.1.0"));
    }

    #[test]
    fn test_显示宽度与补齐() {
        assert_eq!(显示宽度("海龟"), 4);
        assert_eq!(显示宽度("qi-web"), 6);
        assert_eq!(显示宽度("试验包"), 6);
        // 补齐到同一显示宽度
        assert_eq!(显示宽度(&补齐("海龟", 8)), 8);
        assert_eq!(显示宽度(&补齐("qi-web", 8)), 8);
        // 超宽不截断
        assert_eq!(补齐("海龟", 2), "海龟");
    }
}
