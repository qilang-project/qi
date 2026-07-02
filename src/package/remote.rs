//! 远程包依赖支持：spec 解析、缓存布局、元数据与 git 拉取。
//!
//! 缓存布局（Go 风格）:
//! `~/.qi/packages/src/<host>/<owner>/<repo>@<ref>/`
//! ref 缺省时目录名用 `@default`（默认分支）。
//! 可用环境变量 `QI_HOME` 覆盖 `~/.qi`（主要用于测试隔离）。

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

/// 缓存目录中记录来源信息的元数据文件名（clone 后删 .git 前写入）。
pub const META_FILE_NAME: &str = ".qi_source.toml";

/// 一个远程 git 依赖的规范化描述。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteSpec {
    /// git clone 用的 URL
    pub url: String,
    /// tag / branch / commit（None = 默认分支）
    pub reference: Option<String>,
    /// 缓存目录 host 段（如 github.com；file:// 仓库用 "file"）
    pub host: String,
    /// 缓存目录 owner 段（可能含多级，用 '/' 分隔）
    pub owner: String,
    /// 缓存目录 repo 段
    pub repo: String,
    /// 包在仓库中的子目录（monorepo 场景，可选）
    pub subdir: Option<String>,
}

impl RemoteSpec {
    /// 人类可读坐标：`url@ref`（无 ref 时 `url@default`）。
    pub fn coordinate(&self) -> String {
        format!(
            "{}@{}",
            self.url,
            self.reference.as_deref().unwrap_or("default")
        )
    }

    /// ref 对应的目录名片段（'/' 等非法字符替换为 '-'）。
    pub fn ref_dir_name(&self) -> String {
        sanitize_path_component(self.reference.as_deref().unwrap_or("default"))
    }

    /// 仓库缓存目录：`<cache_root>/<host>/<owner>/<repo>@<ref>`。
    pub fn cache_dir(&self) -> PathBuf {
        self.cache_dir_under(&cache_root())
    }

    /// 在指定缓存根目录下计算缓存目录（便于测试）。
    pub fn cache_dir_under(&self, root: &Path) -> PathBuf {
        let mut dir = root.join(sanitize_path_component(&self.host));
        for part in self.owner.split('/').filter(|s| !s.is_empty()) {
            dir.push(sanitize_path_component(part));
        }
        dir.push(format!(
            "{}@{}",
            sanitize_path_component(&self.repo),
            self.ref_dir_name()
        ));
        dir
    }

    /// 包根目录 = 缓存目录 + 可选子目录。
    pub fn package_root(&self) -> PathBuf {
        let dir = self.cache_dir();
        match &self.subdir {
            Some(sub) => dir.join(sub),
            None => dir,
        }
    }
}

/// 全局缓存根目录：`$QI_HOME/packages/src` 或 `~/.qi/packages/src`。
pub fn cache_root() -> PathBuf {
    if let Ok(home) = std::env::var("QI_HOME") {
        if !home.trim().is_empty() {
            return PathBuf::from(home).join("packages").join("src");
        }
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".qi")
        .join("packages")
        .join("src")
}

fn sanitize_path_component(raw: &str) -> String {
    raw.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            other => other,
        })
        .collect()
}

/// 解析一个可能是远程依赖的字符串。
///
/// 支持的形式：
/// - 裸形式：`github.com/owner/repo[/子目录…][@ref]`（首段形如域名）
/// - URL 形式：`https://…`、`http://…`、`ssh://…`、`git://…`、`file://…`（可带尾部 `@ref`）
/// - scp 形式：`git@host:owner/repo[.git][@ref]`
///
/// 不是远程形式（本地路径等）返回 `None`。
pub fn parse_remote_string(raw: &str) -> Option<RemoteSpec> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }

    for scheme in ["https://", "http://", "ssh://", "git://", "file://"] {
        if raw.starts_with(scheme) {
            let (body, reference) = split_trailing_ref(raw, scheme.len());
            return Some(spec_from_git_url(&body, reference, None));
        }
    }

    if raw.starts_with("git@") {
        let (body, reference) = split_trailing_ref(raw, "git@".len());
        return Some(spec_from_git_url(&body, reference, None));
    }

    parse_bare_remote(raw)
}

/// 从详细表 `git = "…"` 字段构造 RemoteSpec；显式 `引用`/`子目录` 覆盖 URL 内嵌值。
pub fn spec_from_detail(
    git: &str,
    reference: Option<String>,
    subdir: Option<String>,
) -> RemoteSpec {
    let mut spec = parse_remote_string(git).unwrap_or_else(|| spec_from_git_url(git, None, None));
    if reference.is_some() {
        spec.reference = reference;
    }
    if subdir.is_some() {
        spec.subdir = subdir;
    }
    spec
}

/// 从 clone URL 推导 host/owner/repo（缓存布局用）。
fn spec_from_git_url(url: &str, reference: Option<String>, subdir: Option<String>) -> RemoteSpec {
    let (host, path) = if let Some(rest) = url.strip_prefix("file://") {
        ("file".to_string(), rest.trim_start_matches('/').to_string())
    } else if let Some(idx) = url.find("://") {
        let rest = &url[idx + 3..];
        let (host_part, path) = rest.split_once('/').unwrap_or((rest, ""));
        // 去掉可能的 userinfo 与端口
        let host = host_part.rsplit('@').next().unwrap_or(host_part);
        let host = host.split(':').next().unwrap_or(host);
        (host.to_string(), path.to_string())
    } else if let Some(rest) = url.strip_prefix("git@") {
        let (host, path) = rest.split_once(':').unwrap_or((rest, ""));
        (host.to_string(), path.to_string())
    } else {
        ("unknown".to_string(), url.to_string())
    };

    let path = path.trim_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let mut segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let repo = segments.pop().unwrap_or("包").to_string();
    let owner = if segments.is_empty() {
        "_".to_string()
    } else {
        segments.join("/")
    };

    RemoteSpec {
        url: url.to_string(),
        reference,
        host,
        owner,
        repo,
        subdir,
    }
}

/// 裸形式：`host.tld/owner/repo[/子目录…][@ref]`。
fn parse_bare_remote(raw: &str) -> Option<RemoteSpec> {
    let (body, reference) = split_trailing_ref(raw, 0);
    let mut segments = body.split('/');
    let host = segments.next()?;
    if !looks_like_host(host) {
        return None;
    }
    let owner = segments.next()?;
    let repo = segments.next()?;
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    let rest: Vec<&str> = segments.filter(|s| !s.is_empty()).collect();
    let subdir = if rest.is_empty() {
        None
    } else {
        Some(rest.join("/"))
    };

    Some(RemoteSpec {
        url: format!("https://{}/{}/{}.git", host, owner, repo),
        reference,
        host: host.to_string(),
        owner: owner.to_string(),
        repo: repo.to_string(),
        subdir,
    })
}

/// 首段是否形如域名：`a.b`、`code.example.co`，排除 `..`、`./x`、中文目录等。
fn looks_like_host(segment: &str) -> bool {
    if !segment.contains('.') {
        return false;
    }
    let labels: Vec<&str> = segment.split('.').collect();
    if labels.len() < 2 || labels.iter().any(|l| l.is_empty()) {
        return false;
    }
    if !labels
        .iter()
        .all(|l| l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
    {
        return false;
    }
    // 末段（TLD）必须是 ≥2 个纯字母，排除 "v1.0/x"、"a.1/b" 之类
    labels
        .last()
        .map(|l| l.len() >= 2 && l.chars().all(|c| c.is_ascii_alphabetic()))
        .unwrap_or(false)
}

/// 从字符串尾部切出 `@ref`。仅当最后一个 '@' 出现在 `skip` 之后、且其后不含 '/'
/// 时才切（避免误切 URL userinfo，如 `https://token@host/x/y`）。
fn split_trailing_ref(raw: &str, skip: usize) -> (String, Option<String>) {
    if let Some(pos) = raw.rfind('@') {
        if pos > skip {
            let candidate = &raw[pos + 1..];
            if !candidate.is_empty() && !candidate.contains('/') {
                return (raw[..pos].to_string(), Some(candidate.to_string()));
            }
        }
    }
    (raw.to_string(), None)
}

/// ref 是否像 commit hash（全十六进制且 ≥7 位）。
pub fn looks_like_commit_hash(reference: &str) -> bool {
    reference.len() >= 7
        && reference.len() <= 40
        && reference.chars().all(|c| c.is_ascii_hexdigit())
}

/// 缓存目录里的来源元数据（写在 [`META_FILE_NAME`]）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteSourceMeta {
    /// clone URL
    pub url: String,
    /// 请求的 ref（tag/branch/commit），缺省为默认分支
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
    /// 解析出的 commit hash
    pub commit: String,
}

/// 读取缓存目录的来源元数据。
pub fn read_meta(cache_dir: &Path) -> Option<RemoteSourceMeta> {
    let content = std::fs::read_to_string(cache_dir.join(META_FILE_NAME)).ok()?;
    toml::from_str(&content).ok()
}

fn write_meta(cache_dir: &Path, meta: &RemoteSourceMeta) -> Result<(), String> {
    let content =
        toml::to_string_pretty(meta).map_err(|e| format!("序列化来源元数据失败: {}", e))?;
    std::fs::write(cache_dir.join(META_FILE_NAME), content)
        .map_err(|e| format!("写入来源元数据失败: {}", e))
}

/// 拉取结果。
#[derive(Debug)]
pub enum FetchOutcome {
    /// 缓存已存在，跳过（幂等）
    Cached(PathBuf),
    /// 本次新拉取
    Fetched {
        /// 缓存目录
        dir: PathBuf,
        /// 解析出的 commit hash
        commit: String,
    },
}

/// 把远程依赖拉取进缓存目录（幂等）。
///
/// - 已存在且非空 → `Cached`
/// - tag/branch → `git clone --depth 1 --branch <ref>`
/// - commit hash → 普通 clone + checkout
/// - clone 成功后记录元数据并删除 `.git` 省空间
pub fn fetch(spec: &RemoteSpec) -> Result<FetchOutcome, String> {
    ensure_git_available()?;

    let dir = spec.cache_dir();
    if dir_is_nonempty(&dir) {
        return Ok(FetchOutcome::Cached(dir));
    }

    let parent = dir
        .parent()
        .ok_or_else(|| format!("无效的缓存目录: {}", dir.display()))?;
    std::fs::create_dir_all(parent)
        .map_err(|e| format!("创建缓存目录失败 {}: {}", parent.display(), e))?;

    // 先 clone 到临时目录，成功后原子重命名，避免半成品缓存
    let tmp_name = format!(
        ".{}.tmp{}",
        dir.file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "pkg".to_string()),
        std::process::id()
    );
    let tmp_dir = parent.join(tmp_name);
    let _ = std::fs::remove_dir_all(&tmp_dir);

    let clone_result = clone_into(spec, &tmp_dir);
    let commit = match clone_result {
        Ok(commit) => commit,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&tmp_dir);
            return Err(e);
        }
    };

    // 记录来源元数据后再删 .git 省空间
    write_meta(
        &tmp_dir,
        &RemoteSourceMeta {
            url: spec.url.clone(),
            reference: spec.reference.clone(),
            commit: commit.clone(),
        },
    )?;
    let _ = std::fs::remove_dir_all(tmp_dir.join(".git"));

    if dir_is_nonempty(&dir) {
        // 并发竞争：别人已放好，丢弃本次结果
        let _ = std::fs::remove_dir_all(&tmp_dir);
        return Ok(FetchOutcome::Cached(dir));
    }
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::rename(&tmp_dir, &dir).map_err(|e| {
        format!(
            "移动缓存目录失败 {} -> {}: {}",
            tmp_dir.display(),
            dir.display(),
            e
        )
    })?;

    Ok(FetchOutcome::Fetched { dir, commit })
}

fn dir_is_nonempty(dir: &Path) -> bool {
    dir.is_dir()
        && std::fs::read_dir(dir)
            .map(|mut entries| entries.next().is_some())
            .unwrap_or(false)
}

fn ensure_git_available() -> Result<(), String> {
    match Command::new("git").arg("--version").output() {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(
            "未找到 git 命令：拉取远程依赖需要系统安装 git。\n  macOS: xcode-select --install 或 brew install git\n  Linux: apt/dnf install git"
                .to_string(),
        ),
        Err(e) => Err(format!("无法执行 git 命令: {}", e)),
    }
}

fn clone_into(spec: &RemoteSpec, dst: &Path) -> Result<String, String> {
    match spec.reference.as_deref() {
        Some(reference) if looks_like_commit_hash(reference) => {
            // commit hash：普通 clone 后 checkout
            run_git(
                Command::new("git").arg("clone").arg(&spec.url).arg(dst),
                spec,
            )?;
            run_git(
                Command::new("git")
                    .arg("-C")
                    .arg(dst)
                    .arg("checkout")
                    .arg("--detach")
                    .arg(reference),
                spec,
            )
            .map_err(|e| {
                format!(
                    "引用 {} 在 {} 中不存在（checkout 失败）\n{}",
                    reference, spec.url, e
                )
            })?;
        }
        Some(reference) => {
            // tag / branch 通吃
            run_git(
                Command::new("git")
                    .arg("clone")
                    .arg("--depth")
                    .arg("1")
                    .arg("--branch")
                    .arg(reference)
                    .arg(&spec.url)
                    .arg(dst),
                spec,
            )?;
        }
        None => {
            run_git(
                Command::new("git")
                    .arg("clone")
                    .arg("--depth")
                    .arg("1")
                    .arg(&spec.url)
                    .arg(dst),
                spec,
            )?;
        }
    }

    // 解析 commit hash
    let output = Command::new("git")
        .arg("-C")
        .arg(dst)
        .arg("rev-parse")
        .arg("HEAD")
        .output()
        .map_err(|e| format!("执行 git rev-parse 失败: {}", e))?;
    if !output.status.success() {
        return Err(format!(
            "读取 commit hash 失败: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn run_git(cmd: &mut Command, spec: &RemoteSpec) -> Result<(), String> {
    let output = cmd
        // 固定 git 输出为英文，保证错误分类稳定（对外提示仍是中文）
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .output()
        .map_err(|e| format!("执行 git 失败: {}", e))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(map_git_error(&stderr, spec))
}

fn map_git_error(stderr: &str, spec: &RemoteSpec) -> String {
    let reference = spec.reference.as_deref().unwrap_or("默认分支");
    let lower = stderr.to_lowercase();
    if lower.contains("could not resolve host")
        || lower.contains("unable to access")
        || lower.contains("connection refused")
        || lower.contains("connection timed out")
        || lower.contains("operation timed out")
        || lower.contains("network is unreachable")
    {
        return format!(
            "网络失败：无法访问 {}\n请检查网络连接或代理设置。\ngit 输出: {}",
            spec.url,
            stderr.trim()
        );
    }
    if lower.contains("remote branch") && lower.contains("not found")
        || lower.contains("could not find remote branch")
        || lower.contains("not found in upstream")
        || (stderr.contains("远程分支") && stderr.contains("未发现"))
    {
        return format!(
            "引用 {} 在 {} 中不存在，请确认 tag/分支名是否正确。\ngit 输出: {}",
            reference,
            spec.url,
            stderr.trim()
        );
    }
    if lower.contains("repository not found")
        || lower.contains("does not exist")
        || lower.contains("does not appear to be a git repository")
    {
        return format!(
            "仓库 {} 不存在或无权访问，请检查地址是否正确。\ngit 输出: {}",
            spec.url,
            stderr.trim()
        );
    }
    format!("git 操作失败（{}）: {}", spec.url, stderr.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bare_remote_with_ref() {
        let spec = parse_remote_string("github.com/liliang-cn/qi-web@v1.0").unwrap();
        assert_eq!(spec.url, "https://github.com/liliang-cn/qi-web.git");
        assert_eq!(spec.reference.as_deref(), Some("v1.0"));
        assert_eq!(spec.host, "github.com");
        assert_eq!(spec.owner, "liliang-cn");
        assert_eq!(spec.repo, "qi-web");
        assert_eq!(spec.subdir, None);
    }

    #[test]
    fn test_parse_bare_remote_without_ref_and_subdir() {
        let spec = parse_remote_string("gitee.com/user/mono/pkgs/web").unwrap();
        assert_eq!(spec.url, "https://gitee.com/user/mono.git");
        assert_eq!(spec.reference, None);
        assert_eq!(spec.subdir.as_deref(), Some("pkgs/web"));
    }

    #[test]
    fn test_local_paths_are_not_remote() {
        assert!(parse_remote_string("../qi-web").is_none());
        assert!(parse_remote_string("./本地/包").is_none());
        assert!(parse_remote_string("qi_packages/Web").is_none());
        assert!(parse_remote_string("/usr/local/lib/qi/packages/Web").is_none());
        assert!(parse_remote_string("包目录/子目录").is_none());
        assert!(parse_remote_string("..").is_none());
        // 首段末位不是纯字母 TLD → 不算 host
        assert!(parse_remote_string("v1.0/x/y").is_none());
    }

    #[test]
    fn test_parse_https_url_with_ref() {
        let spec = parse_remote_string("https://github.com/liliang-cn/qi-web.git@v2.1").unwrap();
        assert_eq!(spec.url, "https://github.com/liliang-cn/qi-web.git");
        assert_eq!(spec.reference.as_deref(), Some("v2.1"));
        assert_eq!(spec.host, "github.com");
        assert_eq!(spec.owner, "liliang-cn");
        assert_eq!(spec.repo, "qi-web");
    }

    #[test]
    fn test_parse_ssh_scp_form() {
        let spec = parse_remote_string("git@github.com:liliang-cn/qi-web.git").unwrap();
        assert_eq!(spec.url, "git@github.com:liliang-cn/qi-web.git");
        assert_eq!(spec.host, "github.com");
        assert_eq!(spec.owner, "liliang-cn");
        assert_eq!(spec.repo, "qi-web");
    }

    #[test]
    fn test_parse_file_url() {
        let spec = parse_remote_string("file:///tmp/qi-e2e/repo.git@v1.0").unwrap();
        assert_eq!(spec.url, "file:///tmp/qi-e2e/repo.git");
        assert_eq!(spec.reference.as_deref(), Some("v1.0"));
        assert_eq!(spec.host, "file");
        assert_eq!(spec.owner, "tmp/qi-e2e");
        assert_eq!(spec.repo, "repo");
    }

    #[test]
    fn test_cache_dir_layout() {
        let spec = parse_remote_string("github.com/liliang-cn/qi-web@v1.0").unwrap();
        let dir = spec.cache_dir_under(Path::new("/cache"));
        assert_eq!(
            dir,
            PathBuf::from("/cache/github.com/liliang-cn/qi-web@v1.0")
        );

        let no_ref = parse_remote_string("github.com/liliang-cn/qi-web").unwrap();
        let dir = no_ref.cache_dir_under(Path::new("/cache"));
        assert_eq!(
            dir,
            PathBuf::from("/cache/github.com/liliang-cn/qi-web@default")
        );

        // ref 含 '/'（分支名）时替换为 '-'
        let branchy = spec_from_detail(
            "https://github.com/x/y",
            Some("feature/new".to_string()),
            None,
        );
        let dir = branchy.cache_dir_under(Path::new("/cache"));
        assert_eq!(dir, PathBuf::from("/cache/github.com/x/y@feature-new"));
    }

    #[test]
    fn test_spec_from_detail_overrides() {
        let spec = spec_from_detail(
            "https://github.com/x/mono.git@v0.9",
            Some("v1.0".to_string()),
            Some("web".to_string()),
        );
        // 显式 引用 覆盖 URL 尾部 @ref
        assert_eq!(spec.reference.as_deref(), Some("v1.0"));
        assert_eq!(spec.subdir.as_deref(), Some("web"));
        assert_eq!(spec.repo, "mono");
    }

    #[test]
    fn test_looks_like_commit_hash() {
        assert!(looks_like_commit_hash("abc1234"));
        assert!(looks_like_commit_hash(
            "0123456789abcdef0123456789abcdef01234567"
        ));
        assert!(!looks_like_commit_hash("v1.0"));
        assert!(!looks_like_commit_hash("main"));
        assert!(!looks_like_commit_hash("abc123")); // 少于 7 位
    }

    #[test]
    fn test_meta_roundtrip() {
        let tmp = tempfile::TempDir::new().unwrap();
        let meta = RemoteSourceMeta {
            url: "https://github.com/x/y.git".to_string(),
            reference: Some("v1.0".to_string()),
            commit: "abcdef0123456789".to_string(),
        };
        write_meta(tmp.path(), &meta).unwrap();
        let read = read_meta(tmp.path()).unwrap();
        assert_eq!(read, meta);
    }
}
