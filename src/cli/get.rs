//! `qi get`（拉取）—— Go 风格远程包依赖拉取。
//!
//! - `qi get`：把项目 qi.toml 中所有远程依赖拉取进本地缓存，并写 qi.lock。
//! - `qi get github.com/x/y@ref [--名 别名]`：先拉取，再把依赖写进项目 qi.toml，
//!   然后同 `qi get`。qi.toml 编辑采用保守的逐行插入，不重排、不丢已有内容与注释。

use std::path::Path;

use crate::cli::commands::CliError;
use crate::package::remote::{self, FetchOutcome, RemoteSpec};
use crate::package::{DependencySource, ResolvedPackageManifest};

/// `qi get` 入口。
pub fn run(
    spec: Option<String>,
    alias_flag: Option<String>,
    verbose: bool,
) -> Result<(), CliError> {
    if let Some(spec_str) = spec.as_deref() {
        let parsed = remote::parse_remote_string(spec_str).ok_or_else(|| {
            package_err(format!(
                "无法识别的远程包地址: {}\n支持形式:\n  github.com/用户/仓库[@标签]\n  https://主机/用户/仓库[.git][@标签]\n  git@主机:用户/仓库[.git]\n  file:///本地/裸仓库.git[@标签]",
                spec_str
            ))
        })?;

        let outcome = remote::fetch(&parsed).map_err(package_err)?;
        report_outcome(&parsed, &outcome);

        let alias = decide_alias(&parsed, alias_flag);
        let project = discover_project_manifest()?;
        update_manifest_dependency(&project.manifest_path, &alias, spec_str)
            .map_err(package_err)?;
        println!(
            "已写入依赖: {} = \"{}\" → {}",
            alias,
            spec_str,
            project.manifest_path.display()
        );
    }

    // 统一收尾：拉取项目全部远程依赖 + 写 qi.lock（重新读清单，可能刚被改写）
    let project = discover_project_manifest()?;

    let mut items: Vec<(&String, &crate::package::ManifestDependency)> =
        project.manifest.dependencies.iter().collect();
    items.sort_by(|(a, _), (b, _)| a.cmp(b));

    let mut remote_count = 0usize;
    for (alias, dependency) in items {
        match dependency.source() {
            Ok(DependencySource::Remote(dep_spec)) => {
                remote_count += 1;
                let outcome = remote::fetch(&dep_spec)
                    .map_err(|e| package_err(format!("拉取依赖 `{}` 失败: {}", alias, e)))?;
                print!("依赖 {} → ", alias);
                report_outcome(&dep_spec, &outcome);
            }
            Ok(DependencySource::LocalPath(path)) => {
                if verbose {
                    println!("依赖 {} → 本地路径 {}（跳过）", alias, path);
                }
            }
            // 注册中心依赖归 `qi 包 安装` 管，qi get 只负责 git 那条线
            Ok(DependencySource::Registry { version }) => {
                if verbose {
                    println!(
                        "依赖 {} → 注册中心 {}（跳过；用 qi 包 安装）",
                        alias, version
                    );
                }
            }
            Err(message) => {
                return Err(package_err(format!(
                    "qi.toml 中依赖 `{}` 配置有误: {}",
                    alias, message
                )));
            }
        }
    }

    if remote_count == 0 && spec.is_none() {
        println!("qi.toml 中没有远程依赖，无需拉取");
    }

    let lock_path =
        ResolvedPackageManifest::write_lock_file_for_manifest(&project).map_err(package_err)?;
    println!("已更新 {}", lock_path.display());

    Ok(())
}

fn package_err(message: impl Into<String>) -> CliError {
    CliError::Package(message.into())
}

fn report_outcome(spec: &RemoteSpec, outcome: &FetchOutcome) {
    match outcome {
        FetchOutcome::Cached(dir) => {
            println!("已缓存: {} ({})", spec.coordinate(), dir.display());
        }
        FetchOutcome::Fetched { dir, commit } => {
            println!(
                "已拉取: {} (commit {}) → {}",
                spec.coordinate(),
                &commit[..commit.len().min(12)],
                dir.display()
            );
        }
    }
}

/// 别名优先级：--名 > 被拉包 qi.toml 的 包.名称 > 仓库名。
fn decide_alias(spec: &RemoteSpec, alias_flag: Option<String>) -> String {
    if let Some(alias) = alias_flag {
        return alias;
    }
    if let Ok(Some(manifest)) = ResolvedPackageManifest::load_dir(&spec.package_root()) {
        if let Some(name) = manifest.package_name() {
            return name.to_string();
        }
    }
    spec.repo.clone()
}

/// 从当前目录向上找项目 qi.toml。
fn discover_project_manifest() -> Result<ResolvedPackageManifest, CliError> {
    let cwd = std::env::current_dir().map_err(CliError::Io)?;
    ResolvedPackageManifest::discover(&cwd)
        .map_err(package_err)?
        .ok_or_else(|| {
            package_err(
                "当前目录及其上级没有找到 qi.toml。\n请在 Qi 项目目录内运行 qi get，或先创建 qi.toml。",
            )
        })
}

/// 保守地把 `别名 = "值"` 写进 qi.toml 的 [依赖] 表：
/// - 已有同名条目 → 原地替换该行
/// - 有 [依赖] 表 → 追加到表末尾
/// - 没有 [依赖] 表 → 文件末尾新建
///
/// 逐行操作，不重排、不丢用户已有内容与注释。
fn update_manifest_dependency(
    manifest_path: &Path,
    alias: &str,
    value: &str,
) -> Result<(), String> {
    let content = std::fs::read_to_string(manifest_path)
        .map_err(|e| format!("读取 {} 失败: {}", manifest_path.display(), e))?;
    let had_trailing_newline = content.ends_with('\n');
    let mut lines: Vec<String> = content.lines().map(String::from).collect();
    let dep_line = format!("{} = \"{}\"", alias, value);

    let header_index = lines.iter().position(|line| {
        let trimmed = line.trim();
        trimmed == "[依赖]" || trimmed == "[dependencies]" || trimmed == "[\"依赖\"]"
    });

    match header_index {
        Some(header) => {
            // 表范围：表头之后到下一个表头（或文件尾）
            let table_end = lines[header + 1..]
                .iter()
                .position(|line| line.trim_start().starts_with('['))
                .map(|offset| header + 1 + offset)
                .unwrap_or(lines.len());

            // 已有同名条目 → 替换
            for line in lines.iter_mut().take(table_end).skip(header + 1) {
                let trimmed = line.trim_start();
                if trimmed.starts_with('#') || !trimmed.contains('=') {
                    continue;
                }
                let key = trimmed
                    .split('=')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .trim_matches('"');
                if key == alias {
                    *line = dep_line;
                    return write_lines(manifest_path, lines, had_trailing_newline);
                }
            }

            // 追加到表末尾（跳过表尾空行，让新行紧跟已有条目）
            let mut insert_at = table_end;
            while insert_at > header + 1 && lines[insert_at - 1].trim().is_empty() {
                insert_at -= 1;
            }
            lines.insert(insert_at, dep_line);
        }
        None => {
            if !lines.is_empty() && !lines.last().map(|l| l.trim().is_empty()).unwrap_or(true) {
                lines.push(String::new());
            }
            lines.push("[依赖]".to_string());
            lines.push(dep_line);
        }
    }

    write_lines(manifest_path, lines, true)
}

fn write_lines(
    manifest_path: &Path,
    lines: Vec<String>,
    trailing_newline: bool,
) -> Result<(), String> {
    let mut output = lines.join("\n");
    if trailing_newline {
        output.push('\n');
    }
    std::fs::write(manifest_path, output)
        .map_err(|e| format!("写入 {} 失败: {}", manifest_path.display(), e))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edit(content: &str, alias: &str, value: &str) -> String {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("qi.toml");
        std::fs::write(&path, content).unwrap();
        update_manifest_dependency(&path, alias, value).unwrap();
        std::fs::read_to_string(&path).unwrap()
    }

    #[test]
    fn test_insert_into_existing_table_preserves_content() {
        let before = "# 项目清单\n[包]\n名称 = \"主程序\" # 注释保留\n\n[依赖]\n本地 = \"../本地\"\n\n[源码]\n目录 = [\".\"]\n";
        let after = edit(before, "Web", "github.com/x/y@v1.0");
        assert!(after.contains("# 项目清单"));
        assert!(after.contains("名称 = \"主程序\" # 注释保留"));
        assert!(after.contains("本地 = \"../本地\"\nWeb = \"github.com/x/y@v1.0\""));
        assert!(after.contains("[源码]"));
    }

    #[test]
    fn test_replace_existing_alias() {
        let before = "[依赖]\nWeb = \"github.com/x/y@v0.9\"\n";
        let after = edit(before, "Web", "github.com/x/y@v1.0");
        assert!(after.contains("Web = \"github.com/x/y@v1.0\""));
        assert!(!after.contains("v0.9"));
    }

    #[test]
    fn test_create_table_when_missing() {
        let before = "[包]\n名称 = \"主程序\"\n";
        let after = edit(before, "Web", "github.com/x/y@v1.0");
        assert!(
            after.contains("[包]\n名称 = \"主程序\"\n\n[依赖]\nWeb = \"github.com/x/y@v1.0\"\n")
        );
    }
}
