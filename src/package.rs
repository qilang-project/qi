//! Package manifest, identifiers, and lockfile support.

pub mod archive;
pub mod install;
pub mod registry;
pub mod remote;

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};

use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PackageManifest {
    #[serde(default, rename = "包", alias = "package")]
    pub package: Option<ManifestPackageInfo>,
    #[serde(default, rename = "源码", alias = "source")]
    pub source: Option<ManifestSourceInfo>,
    #[serde(default, rename = "依赖", alias = "dependencies")]
    pub dependencies: HashMap<String, ManifestDependency>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ManifestPackageInfo {
    #[serde(default, rename = "名称", alias = "name")]
    pub name: Option<String>,
    #[serde(default, rename = "版本", alias = "version")]
    pub version: Option<String>,
    #[serde(default, rename = "入口", alias = "entry")]
    pub entry: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ManifestSourceInfo {
    #[serde(default, rename = "目录", alias = "dirs")]
    pub dirs: Vec<String>,
    #[serde(default, rename = "入口", alias = "entry")]
    pub entry: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ManifestDependency {
    PathString(String),
    Detail(ManifestDependencyDetail),
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ManifestDependencyDetail {
    #[serde(default, rename = "路径", alias = "path")]
    pub path: Option<String>,
    #[serde(default, rename = "版本", alias = "version")]
    pub version: Option<String>,
    /// 远程 git 依赖的仓库地址（与 `路径` 互斥）
    #[serde(default, rename = "git", alias = "仓库")]
    pub git: Option<String>,
    /// 远程依赖的 tag / branch / commit
    #[serde(
        default,
        rename = "引用",
        alias = "ref",
        alias = "tag",
        alias = "branch"
    )]
    pub reference: Option<String>,
    /// 包在仓库中的子目录（monorepo 场景）
    #[serde(default, rename = "子目录", alias = "subdir")]
    pub subdir: Option<String>,
}

/// 依赖来源：本地路径、远程 git 仓库，或注册中心。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DependencySource {
    /// 本地路径依赖（相对于清单所在目录）
    LocalPath(String),
    /// 远程 git 依赖
    Remote(remote::RemoteSpec),
    /// 注册中心依赖：`名称 = "0.1.0"`，装在 `<项目>/qi_packages/<别名>/`
    Registry { version: String },
}

/// 是不是「主.次.补」三段纯数字版本号。
///
/// 这个判定是 `[依赖] 海龟 = "0.1.0"` 与 `[依赖] 本地 = "../本地"` 的分水岭。
/// 卡得这么死是故意的：v1 只做精确版本相等比较（见 docs/包管理设计.md），
/// 放宽成「像版本就算」会让 `1.0`、`v2` 这类写法悄悄变成注册中心依赖，
/// 而它们更可能是用户写歪了的路径。
pub fn is_exact_version(raw: &str) -> bool {
    let mut 段数 = 0;
    for 段 in raw.split('.') {
        if 段.is_empty() || !段.chars().all(|c| c.is_ascii_digit()) {
            return false;
        }
        段数 += 1;
    }
    段数 == 3
}

impl ManifestDependency {
    /// 本地路径（远程依赖返回 None）。
    pub fn path(&self) -> Option<&str> {
        match self {
            ManifestDependency::PathString(path) => {
                if remote::parse_remote_string(path).is_some() {
                    None
                } else {
                    Some(path.as_str())
                }
            }
            ManifestDependency::Detail(detail) => detail.path.as_deref(),
        }
    }

    #[allow(dead_code)]
    pub fn version(&self) -> Option<&str> {
        match self {
            ManifestDependency::PathString(_) => None,
            ManifestDependency::Detail(detail) => detail.version.as_deref(),
        }
    }

    /// 判定依赖来源：
    /// - 裸字符串是「主.次.补」三段数字 → 注册中心依赖
    /// - 裸字符串形如 `host.tld/路径[@ref]` 或 URL/ssh 形式 → 远程
    /// - 其余裸字符串 → 本地路径
    /// - 详细表：`git` / `路径` / 纯 `版本` 三选一
    pub fn source(&self) -> Result<DependencySource, String> {
        match self {
            ManifestDependency::PathString(raw) => Ok(if is_exact_version(raw) {
                DependencySource::Registry {
                    version: raw.clone(),
                }
            } else {
                match remote::parse_remote_string(raw) {
                    Some(spec) => DependencySource::Remote(spec),
                    None => DependencySource::LocalPath(raw.clone()),
                }
            }),
            ManifestDependency::Detail(detail) => {
                if detail.git.is_some() && detail.path.is_some() {
                    return Err("同一个依赖不能同时指定 `路径` 和 `git`".to_string());
                }
                if let Some(git) = &detail.git {
                    Ok(DependencySource::Remote(remote::spec_from_detail(
                        git,
                        detail.reference.clone(),
                        detail.subdir.clone(),
                    )))
                } else if let Some(path) = &detail.path {
                    Ok(DependencySource::LocalPath(path.clone()))
                } else if let Some(version) = &detail.version {
                    // `包 = { 版本 = "0.1.0" }` —— 光有版本没有路径/git，只能是注册中心。
                    // 改动前这里报「依赖需要指定 路径 或 git」，现在它有了归宿。
                    Ok(DependencySource::Registry {
                        version: version.clone(),
                    })
                } else {
                    Err(
                        "依赖需要指定 `路径`（本地）、`git`（远程）或 `版本`（注册中心）"
                            .to_string(),
                    )
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedPackageManifest {
    pub manifest_path: PathBuf,
    pub root_dir: PathBuf,
    pub manifest: PackageManifest,
}

/// 本地路径依赖解析失败的原因。
///
/// 分三种是因为排查手法完全不同：路径不存在要去改 qi.toml 那一行；包名不符是
/// 路径指错了地方；模块找不到要去看被依赖包的 `[包] 入口` / `[源码] 目录`
/// 和实际文件名。笼统一句「哪儿都没找到」会让这三种都变成撞运气。
#[derive(Debug, Clone)]
pub enum LocalDependencyError {
    /// `[依赖]` 里写的路径展开后不是一个目录
    PathMissing { declared: String, expanded: PathBuf },
    /// 目录里确实是个包，但包名跟依赖别名对不上
    PackageNameMismatch { root_dir: PathBuf, found: String },
    /// 目录存在、包名也对（或该目录没有 qi.toml），但没有能对上的模块文件
    ModuleNotFound { root_dir: PathBuf },
}

impl LocalDependencyError {
    /// 拼出带上下文（别名、清单位置、导入路径）的完整报错。
    pub fn message(&self, alias: &str, manifest_path: &Path, module_path: &[String]) -> String {
        match self {
            LocalDependencyError::PathMissing { declared, expanded } => format!(
                "qi.toml 中依赖 `{}` 声明的本地路径不存在: {}\n  声明位置: {}（原文: {} = \"{}\"）",
                alias,
                expanded.display(),
                manifest_path.display(),
                alias,
                declared
            ),
            LocalDependencyError::PackageNameMismatch { root_dir, found } => format!(
                "qi.toml 中依赖 `{}` 指向的目录 {} 里是包 `{}`，没有名为 `{}` 的包\n  声明位置: {}\n  要么把依赖别名改成 `{}`，要么把路径指向真正的 `{}` 包",
                alias,
                root_dir.display(),
                found,
                alias,
                manifest_path.display(),
                found,
                alias
            ),
            LocalDependencyError::ModuleNotFound { root_dir } => format!(
                "qi.toml 中依赖 `{}` 指向的目录 {} 存在，但找不到模块 {}\n  声明位置: {}\n  检查该目录 qi.toml 的 [包] 入口 / [源码] 目录，以及包内实际文件名",
                alias,
                root_dir.display(),
                module_path.join("."),
                manifest_path.display()
            ),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedDependency {
    pub alias: String,
    pub root_dir: PathBuf,
    pub manifest: Option<ResolvedPackageManifest>,
    /// 远程依赖时携带其 spec（缓存路径 / lockfile 用）
    pub remote: Option<remote::RemoteSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PackageName {
    pub parts: Vec<String>,
}

impl PackageName {
    pub fn parse(raw: &str) -> Self {
        let parts = raw
            .split('.')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(|part| part.to_string())
            .collect();
        Self { parts }
    }

    pub fn as_string(&self) -> String {
        self.parts.join(".")
    }
}

impl fmt::Display for PackageName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PackageSourceKind {
    Path,
    Workspace,
    Stdlib,
    Anonymous,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PackageSource {
    pub kind: PackageSourceKind,
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PackageId {
    pub name: PackageName,
    pub version: Option<String>,
    pub source: PackageSource,
}

impl PackageId {
    pub fn anonymous(name: &str) -> Self {
        Self {
            name: PackageName::parse(name),
            version: None,
            source: PackageSource {
                kind: PackageSourceKind::Anonymous,
                path: None,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct LockFile {
    #[serde(rename = "格式版本")]
    format_version: u32,
    #[serde(rename = "根包")]
    root_package: LockPackageEntry,
    #[serde(rename = "包", default, skip_serializing_if = "Vec::is_empty")]
    packages: Vec<LockPackageEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct LockPackageEntry {
    #[serde(rename = "名称")]
    name: String,
    #[serde(rename = "版本", default, skip_serializing_if = "Option::is_none")]
    version: Option<String>,
    #[serde(rename = "来源")]
    source: String,
    #[serde(rename = "路径", default, skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    /// 远程包解析出的 commit hash
    #[serde(rename = "提交", default, skip_serializing_if = "Option::is_none")]
    commit: Option<String>,
    /// 注册中心包体的 sha256（协议里的版本指纹，安装时逐字节校验）
    #[serde(rename = "sha256", default, skip_serializing_if = "Option::is_none")]
    sha256: Option<String>,
    #[serde(rename = "依赖", default, skip_serializing_if = "Vec::is_empty")]
    dependencies: Vec<LockDependencyEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct LockDependencyEntry {
    #[serde(rename = "别名")]
    alias: String,
    #[serde(rename = "名称")]
    name: String,
    #[serde(rename = "版本", default, skip_serializing_if = "Option::is_none")]
    version: Option<String>,
    #[serde(rename = "路径", default, skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    /// 远程依赖来源坐标：`git+<url>@<ref>`
    #[serde(rename = "来源", default, skip_serializing_if = "Option::is_none")]
    source: Option<String>,
    /// 远程依赖解析出的 commit hash
    #[serde(rename = "提交", default, skip_serializing_if = "Option::is_none")]
    commit: Option<String>,
    /// 注册中心依赖的包体 sha256
    #[serde(rename = "sha256", default, skip_serializing_if = "Option::is_none")]
    sha256: Option<String>,
}

/// 从 qi.lock 里读出的一条注册中心包锁定信息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct 锁定包 {
    pub 名称: String,
    pub 版本: String,
    pub sha256: String,
    pub 来源: String,
}

/// 读取项目 qi.lock 里锁定的注册中心包（按名称索引）。
///
/// 只挑出「来源不是 workspace/path/git+…」且带 sha256 的条目 —— 那些才是
/// 注册中心装下来的。lock 不存在 / 解析不动一律返回空表：lock 是**加速与钉死**
/// 的手段，不是必需品，坏 lock 不该让 `qi 包 安装` 整个走不下去。
pub fn read_registry_lock(project_root: &Path) -> HashMap<String, 锁定包> {
    let mut 结果 = HashMap::new();
    let Ok(内容) = std::fs::read_to_string(project_root.join("qi.lock")) else {
        return 结果;
    };
    // 跟读 qi.toml 走同一条归一化：中文裸键在 TOML 1.0 里非法，
    // 工具自己写出来的 lock 是带引号的，但 qi.lock 是要提交进 git、
    // 也可能被人手改的，手写的 `名称 = "…"` 不该直接被当成坏文件丢掉。
    let Ok(lock) = toml::from_str::<LockFile>(&normalize_manifest_text(&内容)) else {
        return 结果;
    };
    for 条目 in lock.packages {
        let (Some(sha256), Some(版本)) = (条目.sha256.clone(), 条目.version.clone()) else {
            continue;
        };
        if 条目.source == "workspace" || 条目.source == "path" || 条目.source.starts_with("git+")
        {
            continue;
        }
        结果.insert(
            条目.name.clone(),
            锁定包 {
                名称: 条目.name,
                版本,
                sha256,
                来源: 条目.source,
            },
        );
    }
    结果
}

impl ResolvedPackageManifest {
    pub fn discover(start_path: &Path) -> Result<Option<Self>, String> {
        let start_dir = if start_path.is_dir() {
            start_path.to_path_buf()
        } else {
            start_path
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .to_path_buf()
        };

        let mut current = start_dir;
        loop {
            if let Some(found) = Self::load_dir(&current)? {
                return Ok(Some(found));
            }

            if !current.pop() {
                break;
            }
        }

        Ok(None)
    }

    /// 只读取 `dir/qi.toml`（不向上查找）。
    pub fn load_dir(dir: &Path) -> Result<Option<Self>, String> {
        let manifest_path = dir.join("qi.toml");
        if !manifest_path.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(&manifest_path)
            .map_err(|e| format!("读取 qi.toml 失败 {}: {}", manifest_path.display(), e))?;
        let normalized = normalize_manifest_text(&content);
        let manifest: PackageManifest = toml::from_str(&normalized)
            .map_err(|e| format!("解析 qi.toml 失败 {}: {}", manifest_path.display(), e))?;
        let root_dir = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
        let manifest_path = manifest_path.canonicalize().unwrap_or(manifest_path);
        Ok(Some(Self {
            manifest_path,
            root_dir,
            manifest,
        }))
    }

    pub fn package_name(&self) -> Option<&str> {
        self.manifest
            .package
            .as_ref()
            .and_then(|pkg| pkg.name.as_deref())
    }

    pub fn package_id(&self, source_kind: PackageSourceKind) -> Option<PackageId> {
        let name = self.package_name()?;
        Some(PackageId {
            name: PackageName::parse(name),
            version: self
                .manifest
                .package
                .as_ref()
                .and_then(|pkg| pkg.version.clone()),
            source: PackageSource {
                kind: source_kind,
                path: Some(self.root_dir.clone()),
            },
        })
    }

    pub fn entry_file_name(&self, fallback_name: &str) -> String {
        self.manifest
            .source
            .as_ref()
            .and_then(|src| src.entry.clone())
            .or_else(|| {
                self.manifest
                    .package
                    .as_ref()
                    .and_then(|pkg| pkg.entry.clone())
            })
            .unwrap_or_else(|| format!("{}.qi", fallback_name))
    }

    pub fn source_dirs(&self) -> Vec<PathBuf> {
        let dirs = self
            .manifest
            .source
            .as_ref()
            .map(|src| src.dirs.clone())
            .unwrap_or_default();

        if dirs.is_empty() {
            vec![self.root_dir.clone()]
        } else {
            dirs.into_iter()
                .map(|dir| self.root_dir.join(dir))
                .collect()
        }
    }

    pub fn resolve_module_path(
        &self,
        package_alias: &str,
        module_path: &[String],
    ) -> Option<PathBuf> {
        if module_path.is_empty() {
            return None;
        }

        let submodule_parts = if module_path.first().map(|s| s.as_str()) == Some(package_alias) {
            &module_path[1..]
        } else {
            module_path
        };

        if submodule_parts.is_empty() {
            let entry_name = self.package_name().unwrap_or(package_alias);
            let entry_file_name = self.entry_file_name(entry_name);
            for source_dir in self.source_dirs() {
                let candidate = source_dir.join(&entry_file_name);
                if candidate.exists() {
                    return Some(candidate);
                }
            }
            return None;
        }

        for source_dir in self.source_dirs() {
            let mut flat_path = source_dir.clone();
            for part in submodule_parts {
                flat_path.push(part);
            }
            flat_path.set_extension("qi");
            if flat_path.exists() {
                return Some(flat_path);
            }

            if submodule_parts.len() == 1 {
                let nested_name = &submodule_parts[0];
                let nested_entry = source_dir
                    .join(nested_name)
                    .join(format!("{}.qi", nested_name));
                if nested_entry.exists() {
                    return Some(nested_entry);
                }
            }
        }

        None
    }

    /// 按 `[依赖]` 里**声明的那条路径**解析模块文件。
    ///
    /// 声明才是权威来源：相对路径以 qi.toml 所在目录为基准展开，绝对路径直接用。
    /// 这里刻意不向上查找被依赖包的 qi.toml（`load_dir` 而非 `discover`）——
    /// 向上找会把依赖方自己或更上层的清单当成它的，`[源码] 目录` 全解释错。
    pub fn resolve_local_dependency_module(
        &self,
        alias: &str,
        declared_path: &str,
        module_path: &[String],
    ) -> Result<PathBuf, LocalDependencyError> {
        let raw = Path::new(declared_path);
        let expanded = if raw.is_absolute() {
            raw.to_path_buf()
        } else {
            self.root_dir.join(raw)
        };

        if !expanded.is_dir() {
            return Err(LocalDependencyError::PathMissing {
                declared: declared_path.to_string(),
                expanded,
            });
        }
        let root_dir = expanded.canonicalize().unwrap_or(expanded);

        let dep_manifest = Self::load_dir(&root_dir).ok().flatten();

        // 包名跟别名对不上就当场停：qi 的模块系统按包名索引符号，硬着头皮解析
        // 出那个包的入口，只会把错误推迟成 codegen 阶段的「符号不存在」。
        if let Some(found) = dep_manifest.as_ref().and_then(|m| m.package_name()) {
            if found != alias {
                return Err(LocalDependencyError::PackageNameMismatch {
                    found: found.to_string(),
                    root_dir,
                });
            }
        }

        // 没有 qi.toml 的目录退化为默认布局（<别名>.qi / 子模块 .qi）
        let effective = dep_manifest.unwrap_or_else(|| Self {
            manifest_path: root_dir.join("qi.toml"),
            root_dir: root_dir.clone(),
            manifest: PackageManifest::default(),
        });

        if let Some(path) = effective.resolve_module_path(alias, module_path) {
            return Ok(path);
        }

        // 兜底：入口文件跟目录同名（包目录 工具库/ 里放 工具库.qi）
        if module_path.len() == 1 {
            if let Some(dir_name) = root_dir.file_name().and_then(|name| name.to_str()) {
                let candidate = root_dir.join(format!("{}.qi", dir_name));
                if candidate.exists() {
                    return Ok(candidate);
                }
            }
        }

        Err(LocalDependencyError::ModuleNotFound { root_dir })
    }

    pub fn resolve_dependency(&self, alias: &str) -> Option<ResolvedDependency> {
        let dependency = self.manifest.dependencies.get(alias)?;
        match dependency.source().ok()? {
            DependencySource::LocalPath(rel_path) => {
                let root_dir = self
                    .root_dir
                    .join(&rel_path)
                    .canonicalize()
                    .unwrap_or_else(|_| self.root_dir.join(&rel_path));
                let manifest = Self::discover(&root_dir).ok().flatten();
                Some(ResolvedDependency {
                    alias: alias.to_string(),
                    root_dir,
                    manifest,
                    remote: None,
                })
            }
            DependencySource::Remote(spec) => {
                let root_dir = spec.package_root();
                // 远程包只认缓存目录自己的 qi.toml，不向上查找
                let manifest = Self::load_dir(&root_dir).ok().flatten();
                Some(ResolvedDependency {
                    alias: alias.to_string(),
                    root_dir,
                    manifest,
                    remote: Some(spec),
                })
            }
            DependencySource::Registry { .. } => {
                let root_dir = self.registry_package_dir(alias);
                // 同远程：只认包自己那份 qi.toml，向上查找会把项目清单当成它的
                let manifest = Self::load_dir(&root_dir).ok().flatten();
                Some(ResolvedDependency {
                    alias: alias.to_string(),
                    root_dir,
                    manifest,
                    remote: None,
                })
            }
        }
    }

    /// 注册中心依赖的安装位置：`<项目根>/qi_packages/<别名>/`（扁平安装）。
    pub fn registry_package_dir(&self, alias: &str) -> PathBuf {
        self.root_dir.join("qi_packages").join(alias)
    }

    /// 项目清单里声明的全部注册中心依赖（别名 → 版本），已按别名排序。
    pub fn registry_dependencies(&self) -> Vec<(String, String)> {
        let mut 结果: Vec<(String, String)> = self
            .manifest
            .dependencies
            .iter()
            .filter_map(|(alias, dep)| match dep.source() {
                Ok(DependencySource::Registry { version }) => Some((alias.clone(), version)),
                _ => None,
            })
            .collect();
        结果.sort_by(|a, b| a.0.cmp(&b.0));
        结果
    }

    pub fn write_lock_file_for_entry(entry_file: &Path) -> Result<Option<PathBuf>, String> {
        let Some(root_manifest) = Self::discover(entry_file)? else {
            return Ok(None);
        };
        Self::write_lock_file_for_manifest(&root_manifest).map(Some)
    }

    /// 为给定项目清单生成并写入 qi.lock，返回 lock 文件路径。
    pub fn write_lock_file_for_manifest(root_manifest: &Self) -> Result<PathBuf, String> {
        let lock = build_lock_file(root_manifest)?;
        let lock_path = root_manifest.root_dir.join("qi.lock");
        let content = toml::to_string_pretty(&lock)
            .map_err(|e| format!("生成 qi.lock 失败 {}: {}", lock_path.display(), e))?;
        std::fs::write(&lock_path, content)
            .map_err(|e| format!("写入 qi.lock 失败 {}: {}", lock_path.display(), e))?;
        Ok(lock_path)
    }
}

/// lock 条目里包的来源标记。
enum LockPackageOrigin {
    Root,
    Path,
    Remote {
        source: String,
        commit: Option<String>,
    },
    /// 注册中心包：来源是注册中心地址，指纹是包体 sha256
    Registry {
        source: String,
        sha256: String,
    },
}

fn build_lock_file(root_manifest: &ResolvedPackageManifest) -> Result<LockFile, String> {
    let mut visited = HashSet::new();
    let mut packages = Vec::new();
    let root_package = collect_lock_package(
        root_manifest,
        LockPackageOrigin::Root,
        &mut visited,
        &mut packages,
    )?;
    packages.sort_by(|a, b| a.name.cmp(&b.name).then(a.path.cmp(&b.path)));
    Ok(LockFile {
        format_version: 1,
        root_package,
        packages,
    })
}

fn collect_lock_package(
    manifest: &ResolvedPackageManifest,
    origin: LockPackageOrigin,
    visited: &mut HashSet<PathBuf>,
    packages: &mut Vec<LockPackageEntry>,
) -> Result<LockPackageEntry, String> {
    let root_key = manifest
        .root_dir
        .canonicalize()
        .unwrap_or_else(|_| manifest.root_dir.clone());
    let was_new = visited.insert(root_key.clone());

    let mut dependency_items: Vec<_> = manifest.manifest.dependencies.iter().collect();
    dependency_items.sort_by(|(left, _), (right, _)| left.cmp(right));

    let mut dependencies = Vec::new();
    for (alias, dep) in dependency_items {
        let resolved = manifest.resolve_dependency(alias);
        let (
            dep_name,
            dep_version,
            dep_path,
            dep_source,
            dep_commit,
            dep_sha256,
            dep_manifest,
            dep_remote,
        ) = if let Some(resolved) = resolved {
            let dep_manifest = resolved.manifest.clone();
            let dep_name = dep_manifest
                .as_ref()
                .and_then(|manifest| manifest.package_name().map(|name| name.to_string()))
                .unwrap_or_else(|| alias.clone());
            // 注册中心装下来的包，安装标记里有权威的 版本/sha256/来源
            let dep_marker = install::读标记(&resolved.root_dir);
            let dep_version = dep_manifest
                .as_ref()
                .and_then(|manifest| manifest.manifest.package.as_ref())
                .and_then(|pkg| pkg.version.clone())
                .or_else(|| dep_marker.as_ref().map(|m| m.版本.clone()))
                .or_else(|| dep.version().map(|v| v.to_string()));
            let dep_path = Some(resolved.root_dir.display().to_string());
            let dep_source = resolved
                .remote
                .as_ref()
                .map(|spec| format!("git+{}", spec.coordinate()))
                .or_else(|| dep_marker.as_ref().map(|m| m.来源.clone()));
            let dep_commit = resolved
                .remote
                .as_ref()
                .and_then(|spec| remote::read_meta(&spec.cache_dir()))
                .map(|meta| meta.commit);
            let dep_sha256 = dep_marker.as_ref().map(|m| m.sha256.clone());
            (
                dep_name,
                dep_version,
                dep_path,
                dep_source,
                dep_commit,
                dep_sha256,
                dep_manifest,
                resolved.remote,
            )
        } else {
            let dep_path = dep
                .path()
                .map(|path| manifest.root_dir.join(path).display().to_string());
            (
                alias.clone(),
                dep.version().map(|v| v.to_string()),
                dep_path,
                None,
                None,
                None,
                None,
                None,
            )
        };

        dependencies.push(LockDependencyEntry {
            alias: alias.clone(),
            name: dep_name,
            version: dep_version,
            path: dep_path,
            source: dep_source.clone(),
            commit: dep_commit.clone(),
            sha256: dep_sha256.clone(),
        });

        if let Some(dep_manifest) = dep_manifest {
            let dep_key = dep_manifest
                .root_dir
                .canonicalize()
                .unwrap_or_else(|_| dep_manifest.root_dir.clone());
            if !visited.contains(&dep_key) {
                let dep_origin = match (dep_source, dep_remote, dep_sha256) {
                    (Some(source), Some(_), _) => LockPackageOrigin::Remote {
                        source,
                        commit: dep_commit,
                    },
                    // 无 remote spec 但有 sha256 → 注册中心包，来源就是注册中心地址
                    (Some(source), None, Some(sha256)) => {
                        LockPackageOrigin::Registry { source, sha256 }
                    }
                    _ => LockPackageOrigin::Path,
                };
                let entry = collect_lock_package(&dep_manifest, dep_origin, visited, packages)?;
                packages.push(entry);
            }
        }
    }

    dependencies.sort_by(|a, b| a.alias.cmp(&b.alias));

    let (source, commit, sha256) = match origin {
        LockPackageOrigin::Root => ("workspace".to_string(), None, None),
        LockPackageOrigin::Path => ("path".to_string(), None, None),
        LockPackageOrigin::Remote { source, commit } => (source, commit, None),
        LockPackageOrigin::Registry { source, sha256 } => (source, None, Some(sha256)),
    };

    Ok(LockPackageEntry {
        name: manifest.package_name().unwrap_or("未命名包").to_string(),
        version: manifest
            .manifest
            .package
            .as_ref()
            .and_then(|pkg| pkg.version.clone()),
        source,
        path: Some(root_key.display().to_string()),
        commit,
        sha256,
        dependencies: if was_new { dependencies } else { Vec::new() },
    })
}

pub(crate) fn normalize_manifest_text(raw: &str) -> String {
    // 数组表头 `[[包]]` 要先处理：下面那条单表头正则的 `\]\s*$` 匹配不到它，
    // 漏掉的话中文数组表头会以裸键形式留在文本里，TOML 1.0 直接判非法。
    let array_table_regex = Regex::new(r#"(?m)^\s*\[\[([^\]\n]+)\]\]\s*$"#).unwrap();
    let normalized_array_tables = array_table_regex.replace_all(raw, |caps: &regex::Captures| {
        let key = caps.get(1).map(|m| m.as_str().trim()).unwrap_or_default();
        if needs_quoted_manifest_key(key) {
            format!(r#"[["{}"]]"#, key)
        } else {
            caps.get(0).unwrap().as_str().to_string()
        }
    });

    let table_regex = Regex::new(r#"(?m)^\s*\[([^\]\n]+)\]\s*$"#).unwrap();
    let normalized_tables =
        table_regex.replace_all(&normalized_array_tables, |caps: &regex::Captures| {
            let key = caps.get(1).map(|m| m.as_str().trim()).unwrap_or_default();
            if needs_quoted_manifest_key(key) {
                format!(r#"["{}"]"#, key)
            } else {
                caps.get(0).unwrap().as_str().to_string()
            }
        });

    let top_level_key_regex = Regex::new(r#"(?m)^(\s*)([^"\s=\[\]#][^=]*?)(\s*=)"#).unwrap();
    let normalized_keys =
        top_level_key_regex.replace_all(&normalized_tables, |caps: &regex::Captures| {
            let prefix = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
            let key = caps.get(2).map(|m| m.as_str().trim()).unwrap_or_default();
            let suffix = caps.get(3).map(|m| m.as_str()).unwrap_or_default();
            if needs_quoted_manifest_key(key) {
                format!(r#"{prefix}"{key}"{suffix}"#)
            } else {
                caps.get(0).unwrap().as_str().to_string()
            }
        });

    let inline_key_regex = Regex::new(r#"([\{,]\s*)([^"\s=\{\},][^=]*?)(\s*=)"#).unwrap();
    inline_key_regex
        .replace_all(&normalized_keys, |caps: &regex::Captures| {
            let prefix = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
            let key = caps.get(2).map(|m| m.as_str().trim()).unwrap_or_default();
            let suffix = caps.get(3).map(|m| m.as_str()).unwrap_or_default();
            if needs_quoted_manifest_key(key) {
                format!(r#"{prefix}"{key}"{suffix}"#)
            } else {
                caps.get(0).unwrap().as_str().to_string()
            }
        })
        .into_owned()
}

fn needs_quoted_manifest_key(key: &str) -> bool {
    // 已经是带引号的写法就别再包一层。qi.lock 是 `toml::to_string_pretty` 写出来的，
    // 中文键本来就带引号（还有 `["根包"."依赖"]` 这种带点的），再包一次会变成
    // `[""根包""]` —— 解析必挂，而 read_registry_lock 对解析失败是静默跳过的，
    // 表现就是「lock 明明在那儿，sha256 校验却没生效」。
    if key.contains('"') || key.contains('\'') {
        return false;
    }
    key.chars().any(|ch| !ch.is_ascii())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_manifest_resolves_module_in_source_dirs() {
        let temp_dir = TempDir::new().unwrap();
        let pkg_dir = temp_dir.path().join("pkg");
        std::fs::create_dir_all(pkg_dir.join("lib")).unwrap();
        std::fs::write(
            pkg_dir.join("qi.toml"),
            r#"
[包]
名称 = "Web"

[源码]
目录 = [".", "lib"]
"#,
        )
        .unwrap();
        std::fs::write(pkg_dir.join("Web.qi"), "包 Web;").unwrap();
        std::fs::write(pkg_dir.join("lib").join("中间件.qi"), "包 Web.中间件;").unwrap();

        let manifest = ResolvedPackageManifest::discover(&pkg_dir.join("Web.qi"))
            .unwrap()
            .unwrap();

        let path = manifest
            .resolve_module_path("Web", &["Web".to_string(), "中间件".to_string()])
            .unwrap();

        assert!(path.ends_with("lib/中间件.qi"));
    }

    fn parse_manifest(text: &str) -> PackageManifest {
        toml::from_str(&normalize_manifest_text(text)).unwrap()
    }

    #[test]
    fn test_版本字符串依赖识别为注册中心() {
        let manifest = parse_manifest(
            r#"
[依赖]
海龟 = "0.1.0"
Web = "github.com/liliang-cn/qi-web@v1.0"
本地包 = "../本地包"
详细 = { 版本 = "1.2.3" }
"#,
        );

        assert_eq!(
            manifest.dependencies["海龟"].source().unwrap(),
            DependencySource::Registry {
                version: "0.1.0".to_string()
            }
        );
        assert_eq!(
            manifest.dependencies["详细"].source().unwrap(),
            DependencySource::Registry {
                version: "1.2.3".to_string()
            }
        );
        // 版本形状之外的裸串，判定跟改动前一模一样
        assert!(matches!(
            manifest.dependencies["Web"].source().unwrap(),
            DependencySource::Remote(_)
        ));
        assert_eq!(
            manifest.dependencies["本地包"].source().unwrap(),
            DependencySource::LocalPath("../本地包".to_string())
        );
    }

    #[test]
    fn test_精确版本判定() {
        assert!(is_exact_version("0.1.0"));
        assert!(is_exact_version("10.20.30"));
        // 范围 / 两段 / 带 v / 路径 一律不算，免得把写歪的路径当成版本
        assert!(!is_exact_version("^0.1"));
        assert!(!is_exact_version("0.1"));
        assert!(!is_exact_version("v1.0.0"));
        assert!(!is_exact_version("1.0.0-beta"));
        assert!(!is_exact_version("../本地"));
        assert!(!is_exact_version(""));
        assert!(!is_exact_version("1.0.0.0"));
    }

    /// 工具写出的 qi.lock 必须能被自己读回来。
    ///
    /// 这条曾经真的挂过：归一化把已带引号的中文键又包了一层（`[["包"]]` →
    /// `[[""包""]]`），解析失败被静默吞掉，表现是「lock 在那儿但 sha256 校验没生效」
    /// —— 供应链校验假绿是最不能接受的一类 bug，所以钉一条回归。
    #[test]
    fn test_lock文件写完能读回_含sha256() {
        let 临时 = TempDir::new().unwrap();
        let 项目 = 临时.path().join("工程");
        let 包 = 项目.join("qi_packages").join("海龟");
        std::fs::create_dir_all(&包).unwrap();
        std::fs::write(
            项目.join("qi.toml"),
            "[包]\n名称 = \"应用\"\n\n[依赖]\n海龟 = \"0.1.0\"\n",
        )
        .unwrap();
        std::fs::write(
            包.join("qi.toml"),
            "[包]\n名称 = \"海龟\"\n版本 = \"0.1.0\"\n",
        )
        .unwrap();
        std::fs::write(包.join("海龟.qi"), "包 海龟;\n").unwrap();
        // 用 toml 序列化写标记（中文键会被自动加引号），跟 install::写标记 同形；
        // 手写裸键在 TOML 1.0 里非法，那样这条测试验的就不是 lock 而是归一化了。
        std::fs::write(
            包.join(install::标记文件名),
            toml::to_string_pretty(&install::安装标记 {
                名称: "海龟".to_string(),
                版本: "0.1.0".to_string(),
                sha256: "deadbeef".to_string(),
                来源: "https://pkg.qilang.org".to_string(),
            })
            .unwrap(),
        )
        .unwrap();

        let 清单 = ResolvedPackageManifest::load_dir(&项目).unwrap().unwrap();
        ResolvedPackageManifest::write_lock_file_for_manifest(&清单).unwrap();

        let 读回 = read_registry_lock(&清单.root_dir);
        let 条目 = 读回.get("海龟").expect("lock 里应有海龟这条注册中心记录");
        assert_eq!(条目.版本, "0.1.0");
        assert_eq!(条目.sha256, "deadbeef");
        assert_eq!(条目.来源, "https://pkg.qilang.org");
    }

    #[test]
    fn test_归一化不动已带引号的键() {
        let 原文 = "\"格式版本\" = 1\n\n[[\"包\"]]\n\"名称\" = \"海龟\"\n";
        assert_eq!(normalize_manifest_text(原文), 原文);
        // 裸中文键仍然要补引号
        assert!(normalize_manifest_text("[[包]]\n名称 = \"x\"\n").contains("[[\"包\"]]"));
    }

    #[test]
    fn test_manifest_bare_string_remote_vs_local() {
        let manifest = parse_manifest(
            r#"
[依赖]
Web = "github.com/liliang-cn/qi-web@v1.0"
本地包 = "../本地包"
"#,
        );

        match manifest.dependencies["Web"].source().unwrap() {
            DependencySource::Remote(spec) => {
                assert_eq!(spec.url, "https://github.com/liliang-cn/qi-web.git");
                assert_eq!(spec.reference.as_deref(), Some("v1.0"));
            }
            other => panic!("应判定为远程依赖: {:?}", other),
        }
        // 远程裸串的 path() 应为 None（不再当本地路径）
        assert!(manifest.dependencies["Web"].path().is_none());

        match manifest.dependencies["本地包"].source().unwrap() {
            DependencySource::LocalPath(p) => assert_eq!(p, "../本地包"),
            other => panic!("应判定为本地路径: {:?}", other),
        }
        assert_eq!(manifest.dependencies["本地包"].path(), Some("../本地包"));
    }

    #[test]
    fn test_manifest_detail_remote_with_aliases() {
        let manifest = parse_manifest(
            r#"
[依赖]
Web = { git = "https://github.com/x/mono", 引用 = "v1.0", 子目录 = "web" }
Cli = { git = "git@github.com:x/cli.git", ref = "main" }
"#,
        );

        match manifest.dependencies["Web"].source().unwrap() {
            DependencySource::Remote(spec) => {
                assert_eq!(spec.reference.as_deref(), Some("v1.0"));
                assert_eq!(spec.subdir.as_deref(), Some("web"));
                assert_eq!(spec.repo, "mono");
            }
            other => panic!("应判定为远程依赖: {:?}", other),
        }
        match manifest.dependencies["Cli"].source().unwrap() {
            DependencySource::Remote(spec) => {
                assert_eq!(spec.reference.as_deref(), Some("main"));
                assert_eq!(spec.host, "github.com");
            }
            other => panic!("应判定为远程依赖: {:?}", other),
        }
    }

    #[test]
    fn test_manifest_detail_path_git_mutually_exclusive() {
        let manifest = parse_manifest(
            r#"
[依赖]
坏 = { 路径 = "../x", git = "https://github.com/x/y" }
"#,
        );
        let err = manifest.dependencies["坏"].source().unwrap_err();
        assert!(err.contains("不能同时指定"), "错误信息: {}", err);

        // `{ 版本 = "…" }` 从「三样都没给」改判为注册中心依赖 —— 有了注册中心，
        // 光写版本就有明确含义了。版本形状是否合法（三段数字）由 `qi 包 安装`
        // 报，那里能带上别名和更长的解释。
        let manifest = parse_manifest(
            r#"
[依赖]
仅版本 = { 版本 = "1.0" }
"#,
        );
        assert_eq!(
            manifest.dependencies["仅版本"].source().unwrap(),
            DependencySource::Registry {
                version: "1.0".to_string()
            }
        );

        // 三样一个都没给才是配置错误
        let manifest = parse_manifest(
            r#"
[依赖]
空 = { 子目录 = "x" }
"#,
        );
        let err = manifest.dependencies["空"].source().unwrap_err();
        assert!(
            err.contains("版本"),
            "错误信息应提到 版本 这条出路: {}",
            err
        );
    }

    #[test]
    fn test_lock_file_roundtrip() {
        let lock = LockFile {
            format_version: 1,
            root_package: LockPackageEntry {
                name: "主程序".to_string(),
                version: Some("0.1.0".to_string()),
                source: "workspace".to_string(),
                path: Some("/项目".to_string()),
                commit: None,
                sha256: None,
                dependencies: vec![LockDependencyEntry {
                    alias: "Web".to_string(),
                    name: "qi-web".to_string(),
                    version: Some("1.0".to_string()),
                    path: Some("/缓存/github.com/x/y@v1.0".to_string()),
                    source: Some("git+https://github.com/x/y.git@v1.0".to_string()),
                    commit: Some("abcdef0123456".to_string()),
                    sha256: None,
                }],
            },
            packages: vec![LockPackageEntry {
                name: "qi-web".to_string(),
                version: Some("1.0".to_string()),
                source: "git+https://github.com/x/y.git@v1.0".to_string(),
                path: Some("/缓存/github.com/x/y@v1.0".to_string()),
                commit: Some("abcdef0123456".to_string()),
                sha256: None,
                dependencies: vec![],
            }],
        };

        let text = toml::to_string_pretty(&lock).unwrap();
        let parsed: LockFile = toml::from_str(&text).unwrap();
        assert_eq!(parsed, lock);
    }

    #[test]
    fn test_resolve_remote_dependency_uses_cache_layout() {
        let manifest = parse_manifest(
            r#"
[依赖]
Web = "github.com/liliang-cn/qi-web@v1.0"
"#,
        );
        let resolved_manifest = ResolvedPackageManifest {
            manifest_path: PathBuf::from("/项目/qi.toml"),
            root_dir: PathBuf::from("/项目"),
            manifest,
        };
        let dep = resolved_manifest.resolve_dependency("Web").unwrap();
        let spec = dep.remote.expect("应为远程依赖");
        assert!(dep.root_dir.ends_with("github.com/liliang-cn/qi-web@v1.0"));
        assert_eq!(spec.repo, "qi-web");
    }
}
