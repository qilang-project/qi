//! Package manifest, identifiers, and lockfile support.

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
}

impl ManifestDependency {
    pub fn path(&self) -> Option<&str> {
        match self {
            ManifestDependency::PathString(path) => Some(path.as_str()),
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
}

#[derive(Debug, Clone)]
pub struct ResolvedPackageManifest {
    pub manifest_path: PathBuf,
    pub root_dir: PathBuf,
    pub manifest: PackageManifest,
}

#[derive(Debug, Clone)]
pub struct ResolvedDependency {
    pub alias: String,
    pub root_dir: PathBuf,
    pub manifest: Option<ResolvedPackageManifest>,
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

#[derive(Debug, Clone, Serialize)]
struct LockFile {
    #[serde(rename = "格式版本")]
    format_version: u32,
    #[serde(rename = "根包")]
    root_package: LockPackageEntry,
    #[serde(rename = "包", default, skip_serializing_if = "Vec::is_empty")]
    packages: Vec<LockPackageEntry>,
}

#[derive(Debug, Clone, Serialize)]
struct LockPackageEntry {
    #[serde(rename = "名称")]
    name: String,
    #[serde(rename = "版本", skip_serializing_if = "Option::is_none")]
    version: Option<String>,
    #[serde(rename = "来源")]
    source: String,
    #[serde(rename = "路径", skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    #[serde(rename = "依赖", default, skip_serializing_if = "Vec::is_empty")]
    dependencies: Vec<LockDependencyEntry>,
}

#[derive(Debug, Clone, Serialize)]
struct LockDependencyEntry {
    #[serde(rename = "别名")]
    alias: String,
    #[serde(rename = "名称")]
    name: String,
    #[serde(rename = "版本", skip_serializing_if = "Option::is_none")]
    version: Option<String>,
    #[serde(rename = "路径", skip_serializing_if = "Option::is_none")]
    path: Option<String>,
}

impl ResolvedPackageManifest {
    pub fn discover(start_path: &Path) -> Result<Option<Self>, String> {
        let start_dir = if start_path.is_dir() {
            start_path.to_path_buf()
        } else {
            start_path.parent().unwrap_or_else(|| Path::new(".")).to_path_buf()
        };

        let mut current = start_dir;
        loop {
            let manifest_path = current.join("qi.toml");
            if manifest_path.exists() {
                let content = std::fs::read_to_string(&manifest_path)
                    .map_err(|e| format!("读取 qi.toml 失败 {}: {}", manifest_path.display(), e))?;
                let normalized = normalize_manifest_text(&content);
                let manifest: PackageManifest = toml::from_str(&normalized)
                    .map_err(|e| format!("解析 qi.toml 失败 {}: {}", manifest_path.display(), e))?;
                let root_dir = current.canonicalize().unwrap_or(current.clone());
                let manifest_path = manifest_path.canonicalize().unwrap_or(manifest_path);
                return Ok(Some(Self {
                    manifest_path,
                    root_dir,
                    manifest,
                }));
            }

            if !current.pop() {
                break;
            }
        }

        Ok(None)
    }

    pub fn package_name(&self) -> Option<&str> {
        self.manifest.package.as_ref().and_then(|pkg| pkg.name.as_deref())
    }

    pub fn package_id(&self, source_kind: PackageSourceKind) -> Option<PackageId> {
        let name = self.package_name()?;
        Some(PackageId {
            name: PackageName::parse(name),
            version: self.manifest.package.as_ref().and_then(|pkg| pkg.version.clone()),
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
            .or_else(|| self.manifest.package.as_ref().and_then(|pkg| pkg.entry.clone()))
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
            dirs.into_iter().map(|dir| self.root_dir.join(dir)).collect()
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
            let entry_name = self
                .package_name()
                .unwrap_or(package_alias);
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
                let nested_entry = source_dir.join(nested_name).join(format!("{}.qi", nested_name));
                if nested_entry.exists() {
                    return Some(nested_entry);
                }
            }
        }

        None
    }

    pub fn resolve_dependency(&self, alias: &str) -> Option<ResolvedDependency> {
        let dependency = self.manifest.dependencies.get(alias)?;
        let rel_path = dependency.path()?;
        let root_dir = self.root_dir.join(rel_path).canonicalize().unwrap_or_else(|_| self.root_dir.join(rel_path));
        let manifest = Self::discover(&root_dir).ok().flatten();
        Some(ResolvedDependency {
            alias: alias.to_string(),
            root_dir,
            manifest,
        })
    }

    pub fn write_lock_file_for_entry(entry_file: &Path) -> Result<Option<PathBuf>, String> {
        let Some(root_manifest) = Self::discover(entry_file)? else {
            return Ok(None);
        };

        let lock = build_lock_file(&root_manifest)?;
        let lock_path = root_manifest.root_dir.join("qi.lock");
        let content = toml::to_string_pretty(&lock)
            .map_err(|e| format!("生成 qi.lock 失败 {}: {}", lock_path.display(), e))?;
        std::fs::write(&lock_path, content)
            .map_err(|e| format!("写入 qi.lock 失败 {}: {}", lock_path.display(), e))?;
        Ok(Some(lock_path))
    }
}

fn build_lock_file(root_manifest: &ResolvedPackageManifest) -> Result<LockFile, String> {
    let mut visited = HashSet::new();
    let mut packages = Vec::new();
    let root_package = collect_lock_package(root_manifest, true, &mut visited, &mut packages)?;
    packages.sort_by(|a, b| a.name.cmp(&b.name).then(a.path.cmp(&b.path)));
    Ok(LockFile {
        format_version: 1,
        root_package,
        packages,
    })
}

fn collect_lock_package(
    manifest: &ResolvedPackageManifest,
    is_root: bool,
    visited: &mut HashSet<PathBuf>,
    packages: &mut Vec<LockPackageEntry>,
) -> Result<LockPackageEntry, String> {
    let root_key = manifest.root_dir.canonicalize().unwrap_or_else(|_| manifest.root_dir.clone());
    let was_new = visited.insert(root_key.clone());

    let mut dependency_items: Vec<_> = manifest.manifest.dependencies.iter().collect();
    dependency_items.sort_by(|(left, _), (right, _)| left.cmp(right));

    let mut dependencies = Vec::new();
    for (alias, dep) in dependency_items {
        let resolved = manifest.resolve_dependency(alias);
        let (dep_name, dep_version, dep_path, dep_manifest) = if let Some(resolved) = resolved {
            let dep_manifest = resolved.manifest.clone();
            let dep_name = dep_manifest
                .as_ref()
                .and_then(|manifest| manifest.package_name().map(|name| name.to_string()))
                .unwrap_or_else(|| alias.clone());
            let dep_version = dep_manifest
                .as_ref()
                .and_then(|manifest| manifest.manifest.package.as_ref())
                .and_then(|pkg| pkg.version.clone())
                .or_else(|| dep.version().map(|v| v.to_string()));
            let dep_path = Some(resolved.root_dir.display().to_string());
            (dep_name, dep_version, dep_path, dep_manifest)
        } else {
            let dep_path = dep.path().map(|path| manifest.root_dir.join(path).display().to_string());
            (alias.clone(), dep.version().map(|v| v.to_string()), dep_path, None)
        };

        dependencies.push(LockDependencyEntry {
            alias: alias.clone(),
            name: dep_name,
            version: dep_version,
            path: dep_path,
        });

        if let Some(dep_manifest) = dep_manifest {
            let dep_key = dep_manifest.root_dir.canonicalize().unwrap_or_else(|_| dep_manifest.root_dir.clone());
            if !visited.contains(&dep_key) {
                let entry = collect_lock_package(&dep_manifest, false, visited, packages)?;
                packages.push(entry);
            }
        }
    }

    dependencies.sort_by(|a, b| a.alias.cmp(&b.alias));

    Ok(LockPackageEntry {
        name: manifest.package_name().unwrap_or("未命名包").to_string(),
        version: manifest.manifest.package.as_ref().and_then(|pkg| pkg.version.clone()),
        source: if is_root {
            "workspace".to_string()
        } else {
            "path".to_string()
        },
        path: Some(root_key.display().to_string()),
        dependencies: if was_new { dependencies } else { Vec::new() },
    })
}

fn normalize_manifest_text(raw: &str) -> String {
    let table_regex = Regex::new(r#"(?m)^\s*\[([^\]\n]+)\]\s*$"#).unwrap();
    let normalized_tables = table_regex.replace_all(raw, |caps: &regex::Captures| {
        let key = caps.get(1).map(|m| m.as_str().trim()).unwrap_or_default();
        if needs_quoted_manifest_key(key) {
            format!(r#"["{}"]"#, key)
        } else {
            caps.get(0).unwrap().as_str().to_string()
        }
    });

    let top_level_key_regex = Regex::new(
        r#"(?m)^(\s*)([^"\s=\[\]#][^=]*?)(\s*=)"#,
    )
    .unwrap();
    let normalized_keys = top_level_key_regex.replace_all(&normalized_tables, |caps: &regex::Captures| {
        let prefix = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
        let key = caps.get(2).map(|m| m.as_str().trim()).unwrap_or_default();
        let suffix = caps.get(3).map(|m| m.as_str()).unwrap_or_default();
        if needs_quoted_manifest_key(key) {
            format!(r#"{prefix}"{key}"{suffix}"#)
        } else {
            caps.get(0).unwrap().as_str().to_string()
        }
    });

    let inline_key_regex = Regex::new(
        r#"([\{,]\s*)([^"\s=\{\},][^=]*?)(\s*=)"#,
    )
    .unwrap();
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
}
