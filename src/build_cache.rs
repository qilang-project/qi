//! 构建缓存：输入没变就不重新编译。
//!
//! ## 缓存什么
//!
//! 可执行模式（非库模式、非 wasm）的**最终产物**：链接好的可执行文件 + 旁边那个
//! `.o`（macOS 的调试信息留在 .o 里，可执行文件只有一张指回它的 debug map，
//! 所以两个都要）+ codegen 期间打到 stderr 的警告（命中时原样重放）。
//!
//! ## 键里有什么（缺一样就可能拿到旧产物，所以宁多勿少）
//!
//! - 编译器身份：版本号 + 可执行文件路径/大小/修改时间（重编编译器必然变）
//! - 所有参与编译的源文件：**解析时读到的那份文本**的 SHA-256 + 路径
//!   （路径也进 DWARF 和私有函数消歧；标准库虚拟模块同样按文本算）
//! - 编译配置（优化档位、目标、调试信息开关、库搜索路径……，输出路径除外）
//! - 入口路径（决定 .o / 可执行文件落在哪，.o 路径写进 debug map）
//! - 运行时归档、链接器（clang / ld / zig）、外部库文件：路径 + 大小 + 修改时间
//! - 环境变量：全部 `QI_*`（除了只影响观测/缓存本身的几个），外加
//!   PATH / SDKROOT / DEVELOPER_DIR / MACOSX_DEPLOYMENT_TARGET / LIBRARY_PATH
//!
//! 任何一样拿不准（比如 `-l库名` 在搜索路径里找不到对应文件、读元数据失败），
//! 就**不缓存**：宁可慢，不能错。
//!
//! ## 开关
//!
//! - `QI_NO_CACHE=1`：不查也不写
//! - `QI_CACHE_DIR=<目录>`：换缓存目录（默认 macOS `~/Library/Caches/qi`，
//!   其它平台 `$XDG_CACHE_HOME/qi` 或 `~/.cache/qi`）
//! - `QI_CACHE_MAX_MB=<n>`：总大小上限，默认 2048；超了按最近使用时间淘汰
//! - `QI_EMIT_LL` / `QI_EMIT_LL_POST` 设了就不走缓存（用户要的是 IR 落盘这个副作用）

use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::path::{Path, PathBuf};

/// 键格式版本：键的组成方式一改就加一，旧条目自然失效。
const KEY_VERSION: &str = "qi-build-cache v1";

/// 这些 `QI_*` 变量不影响产物，不进键（进了只会白白失配）。
const ENV_NOT_IN_KEY: &[&str] = &[
    "QI_NO_CACHE",
    "QI_CACHE_DIR",
    "QI_CACHE_MAX_MB",
    "QI_CACHE_TRACE",
    "QI_TIME",
    "QI_RESOLVE_TRACE",
    "QI_NO_STALE_WARN",
];

/// 影响链接/工具链选择的非 `QI_` 变量。
const EXTRA_ENV_IN_KEY: &[&str] = &[
    "PATH",
    "SDKROOT",
    "DEVELOPER_DIR",
    "MACOSX_DEPLOYMENT_TARGET",
    "LIBRARY_PATH",
];

const DEFAULT_MAX_MB: u64 = 2048;

// ── 警告录制：codegen 的 stderr 警告在命中时要能重放 ─────────────────────

thread_local! {
    static RECORDER: RefCell<Option<Vec<String>>> = const { RefCell::new(None) };
}

/// 打一条编译警告到 stderr；录制中则顺带记下来（进缓存，命中时重放）。
pub fn emit_warning(message: &str) {
    eprintln!("{}", message);
    RECORDER.with(|r| {
        if let Some(v) = r.borrow_mut().as_mut() {
            v.push(message.to_string());
        }
    });
}

fn start_recording() {
    RECORDER.with(|r| *r.borrow_mut() = Some(Vec::new()));
}

fn stop_recording() -> Vec<String> {
    RECORDER.with(|r| r.borrow_mut().take().unwrap_or_default())
}

/// 录制 `f` 期间的警告。
pub fn record_warnings<T>(f: impl FnOnce() -> T) -> (T, Vec<String>) {
    start_recording();
    let out = f();
    (out, stop_recording())
}

fn trace_on() -> bool {
    std::env::var_os("QI_CACHE_TRACE").is_some()
}

fn trace(msg: &str) {
    if trace_on() {
        eprintln!("[qi 缓存] {}", msg);
    }
}

// ── 键 ─────────────────────────────────────────────────────────────────

/// 逐项累积键。`manifest` 是人可读的明细（写进条目，方便查「为什么没命中」）。
pub struct KeyBuilder {
    hasher: Sha256,
    manifest: String,
}

impl Default for KeyBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyBuilder {
    pub fn new() -> Self {
        let mut k = Self {
            hasher: Sha256::new(),
            manifest: String::new(),
        };
        k.field("key-version", KEY_VERSION);
        k
    }

    /// 记一项。名字和值都带长度前缀进哈希，杜绝拼接歧义。
    pub fn field(&mut self, name: &str, value: &str) {
        for part in [name.as_bytes(), value.as_bytes()] {
            self.hasher.update((part.len() as u64).to_le_bytes());
            self.hasher.update(part);
        }
        self.manifest.push_str(name);
        self.manifest.push_str(" = ");
        self.manifest.push_str(value);
        self.manifest.push('\n');
    }

    /// 记一段内容的摘要（源文件文本）。
    pub fn content(&mut self, name: &str, bytes: &[u8]) {
        let digest = hex(&Sha256::digest(bytes));
        self.field(name, &digest);
    }

    /// 记一个文件的身份：规范路径 + 大小 + 修改时间（纳秒）。读不到元数据 → Err（调用方放弃缓存）。
    pub fn file_identity(&mut self, name: &str, path: &Path) -> Result<(), String> {
        let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let meta = std::fs::metadata(&canonical)
            .map_err(|e| format!("读 {} 的元数据失败: {}", canonical.display(), e))?;
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_nanos())
            .ok_or_else(|| format!("{} 没有修改时间", canonical.display()))?;
        self.field(
            name,
            &format!(
                "{} size={} mtime={}",
                canonical.display(),
                meta.len(),
                mtime
            ),
        );
        Ok(())
    }

    /// 编译器自身 + 相关环境变量。
    pub fn compiler_and_env(&mut self) -> Result<(), String> {
        self.field("qi-version", env!("CARGO_PKG_VERSION"));
        let exe = std::env::current_exe().map_err(|e| format!("取编译器路径失败: {}", e))?;
        self.file_identity("qi-exe", &exe)?;

        let mut vars: Vec<(String, String)> = std::env::vars_os()
            .filter_map(|(k, v)| {
                let k = k.into_string().ok()?;
                let in_key = (k.starts_with("QI_") && !ENV_NOT_IN_KEY.contains(&k.as_str()))
                    || EXTRA_ENV_IN_KEY.contains(&k.as_str());
                in_key.then(|| (k, v.to_string_lossy().into_owned()))
            })
            .collect();
        vars.sort();
        for (k, v) in vars {
            self.field(&format!("env {}", k), &v);
        }
        Ok(())
    }

    /// 在 PATH 上找一个工具，记它的身份（找不到 → Err，链接也必然失败，不缓存）。
    pub fn tool_on_path(&mut self, tool: &str) -> Result<(), String> {
        let found = find_on_path(tool).ok_or_else(|| format!("PATH 上找不到 {}", tool))?;
        self.file_identity(&format!("tool {}", tool), &found)
    }

    pub fn finish(self) -> CacheKey {
        CacheKey {
            hex: hex(&self.hasher.finalize()),
            manifest: self.manifest,
        }
    }
}

fn find_on_path(tool: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let names: Vec<String> = if cfg!(windows) {
        vec![format!("{}.exe", tool), tool.to_string()]
    } else {
        vec![tool.to_string()]
    };
    for dir in std::env::split_paths(&path) {
        for n in &names {
            let p = dir.join(n);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

pub struct CacheKey {
    hex: String,
    manifest: String,
}

impl CacheKey {
    pub fn hex(&self) -> &str {
        &self.hex
    }
}

// ── 缓存目录 ────────────────────────────────────────────────────────────

pub struct BuildCache {
    root: PathBuf,
    max_bytes: u64,
}

const EXE_FILE: &str = "exe";
const OBJ_FILE: &str = "obj";
const WARNINGS_FILE: &str = "warnings.txt";
const KEY_FILE: &str = "key.txt";

impl BuildCache {
    /// 缓存开着就返回它；QI_NO_CACHE 或找不到缓存目录 → None。
    pub fn open() -> Option<Self> {
        if std::env::var_os("QI_NO_CACHE").is_some_and(|v| !v.is_empty() && v != "0") {
            trace("QI_NO_CACHE：跳过");
            return None;
        }
        if std::env::var_os("QI_EMIT_LL").is_some() || std::env::var_os("QI_EMIT_LL_POST").is_some()
        {
            trace("QI_EMIT_LL*：要落盘 IR，跳过缓存");
            return None;
        }
        let root = match std::env::var_os("QI_CACHE_DIR") {
            Some(d) if !d.is_empty() => PathBuf::from(d),
            _ => default_cache_root()?,
        };
        let max_mb = std::env::var("QI_CACHE_MAX_MB")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .unwrap_or(DEFAULT_MAX_MB);
        Some(Self {
            root: root.join("build"),
            max_bytes: max_mb.saturating_mul(1024 * 1024),
        })
    }

    fn entry_dir(&self, key: &CacheKey) -> PathBuf {
        self.root.join(&key.hex[..2]).join(&key.hex)
    }

    /// 命中就把产物复制到 `exe_dst` / `obj_dst`，重放警告，返回 true。
    /// 任何一步失败都当未命中（返回 false），调用方照常编译。
    pub fn restore(&self, key: &CacheKey, exe_dst: &Path, obj_dst: &Path) -> bool {
        let dir = self.entry_dir(key);
        let exe = dir.join(EXE_FILE);
        let obj = dir.join(OBJ_FILE);
        if !exe.is_file() || !obj.is_file() {
            trace(&format!("未命中 {}", key.hex));
            return false;
        }
        let copied = copy_replace(&obj, obj_dst).and_then(|_| copy_replace(&exe, exe_dst));
        if let Err(e) = copied {
            trace(&format!("命中但复制失败（改为重新编译）: {}", e));
            return false;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(exe_dst, std::fs::Permissions::from_mode(0o755));
        }
        if let Ok(text) = std::fs::read_to_string(dir.join(WARNINGS_FILE)) {
            for w in text.split('\u{0}').filter(|w| !w.is_empty()) {
                eprintln!("{}", w);
            }
        }
        // 最近使用时间 = 条目目录的修改时间（淘汰按它排）
        touch(&dir);
        trace(&format!("命中 {}", key.hex));
        true
    }

    /// 写入一个条目。先写到临时目录再整体 rename，别的进程看到的要么没有、要么完整。
    /// 失败只打 trace，不影响这次编译。
    pub fn store(&self, key: &CacheKey, exe: &Path, obj: &Path, warnings: &[String]) {
        if let Err(e) = self.store_inner(key, exe, obj, warnings) {
            trace(&format!("写缓存失败: {}", e));
            return;
        }
        trace(&format!("已写入 {}", key.hex));
        self.evict();
    }

    fn store_inner(
        &self,
        key: &CacheKey,
        exe: &Path,
        obj: &Path,
        warnings: &[String],
    ) -> std::io::Result<()> {
        let final_dir = self.entry_dir(key);
        if final_dir.join(EXE_FILE).is_file() {
            return Ok(());
        }
        let tmp_root = self.root.join("tmp");
        std::fs::create_dir_all(&tmp_root)?;
        let tmp = tmp_root.join(format!(
            "{}-{}-{}",
            &key.hex[..16],
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&tmp)?;
        let result = (|| {
            std::fs::copy(exe, tmp.join(EXE_FILE))?;
            std::fs::copy(obj, tmp.join(OBJ_FILE))?;
            std::fs::write(tmp.join(WARNINGS_FILE), warnings.join("\u{0}"))?;
            std::fs::write(tmp.join(KEY_FILE), &key.manifest)?;
            if let Some(parent) = final_dir.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::rename(&tmp, &final_dir)
        })();
        if result.is_err() {
            // 包括「别的进程抢先写好了同一个键」—— 那份一样好，丢掉自己这份
            let _ = std::fs::remove_dir_all(&tmp);
        }
        match result {
            Err(_) if final_dir.join(EXE_FILE).is_file() => Ok(()),
            other => other,
        }
    }

    /// 总大小超上限就按最近使用时间从旧到新删条目。
    fn evict(&self) {
        let Ok(shards) = std::fs::read_dir(&self.root) else {
            return;
        };
        let mut entries: Vec<(std::time::SystemTime, u64, PathBuf)> = Vec::new();
        let mut total: u64 = 0;
        for shard in shards.flatten() {
            let shard_path = shard.path();
            if shard.file_name() == "tmp" {
                clean_stale_tmp(&shard_path);
                continue;
            }
            if !shard_path.is_dir() {
                continue;
            }
            let Ok(items) = std::fs::read_dir(&shard_path) else {
                continue;
            };
            for item in items.flatten() {
                let dir = item.path();
                let size = dir_size(&dir);
                let used = std::fs::metadata(&dir)
                    .and_then(|m| m.modified())
                    .unwrap_or(std::time::UNIX_EPOCH);
                total += size;
                entries.push((used, size, dir));
            }
        }
        if total <= self.max_bytes {
            return;
        }
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        for (_, size, dir) in entries {
            if total <= self.max_bytes {
                break;
            }
            if std::fs::remove_dir_all(&dir).is_ok() {
                trace(&format!("淘汰 {}", dir.display()));
                total = total.saturating_sub(size);
            }
        }
    }
}

/// 写到一半就被杀掉的进程会在 tmp/ 留下残骸；一小时以上没动的清掉。
fn clean_stale_tmp(tmp_root: &Path) {
    let Ok(items) = std::fs::read_dir(tmp_root) else {
        return;
    };
    let hour = std::time::Duration::from_secs(3600);
    for item in items.flatten() {
        let old = item
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age > hour);
        if old {
            let _ = std::fs::remove_dir_all(item.path());
        }
    }
}

fn default_cache_root() -> Option<PathBuf> {
    // dirs::cache_dir：macOS ~/Library/Caches；Linux $XDG_CACHE_HOME 或 ~/.cache；
    // Windows %LOCALAPPDATA%
    dirs::cache_dir().map(|d| d.join("qi"))
}

/// 复制到目标位置：先删旧的再拷（目标可能是正在跑的可执行文件，直接覆盖写会
/// 撞 ETXTBSY / 让 macOS 的代码签名缓存认错）。
fn copy_replace(src: &Path, dst: &Path) -> std::io::Result<()> {
    let _ = std::fs::remove_file(dst);
    std::fs::copy(src, dst).map(|_| ())
}

fn touch(dir: &Path) {
    // 在目录里建删一个文件就会更新目录的修改时间，不需要额外依赖
    let marker = dir.join(".used");
    if std::fs::write(&marker, b"").is_ok() {
        let _ = std::fs::remove_file(&marker);
    }
}

fn dir_size(dir: &Path) -> u64 {
    std::fs::read_dir(dir)
        .map(|it| {
            it.flatten()
                .filter_map(|e| e.metadata().ok())
                .map(|m| m.len())
                .sum()
        })
        .unwrap_or(0)
}

// ── qi 专属：把一次可执行编译的全部输入装进键 ──────────────────────────

/// 系统 C 运行时自带的库：`-l` 它们等于链系统库，身份由 SDK / 工具链（已进键）决定。
const SYSTEM_LIBS: &[&str] = &["c", "m", "pthread", "dl", "System", "rt"];

impl crate::QiCompiler {
    /// 算这次可执行编译的缓存键。有任何一项拿不准就返回 None（不缓存）。
    pub(crate) fn build_cache_key(
        &self,
        source_file: &Path,
        obj: &Path,
        exe: &Path,
        programs: &[crate::parser::ast::Program],
        digests: &[String],
    ) -> Option<CacheKey> {
        match self.build_cache_key_inner(source_file, obj, exe, programs, digests) {
            Ok(k) => Some(k),
            Err(why) => {
                trace(&format!("不缓存: {}", why));
                None
            }
        }
    }

    fn build_cache_key_inner(
        &self,
        source_file: &Path,
        obj: &Path,
        exe: &Path,
        programs: &[crate::parser::ast::Program],
        digests: &[String],
    ) -> Result<CacheKey, String> {
        let mut k = KeyBuilder::new();
        k.compiler_and_env()?;

        // 配置：整个序列化，以后加字段自动进键；只清掉与产物无关的几项
        let mut cfg = self.config.clone();
        cfg.output_file = None;
        cfg.verbose = false;
        cfg.config_file = None;
        let cfg_json = serde_json::to_string(&cfg).map_err(|e| e.to_string())?;
        k.field("config", &cfg_json);

        k.field("entry", &source_file.display().to_string());
        k.field("obj", &obj.display().to_string());
        k.field("exe", &exe.display().to_string());

        if programs.len() != digests.len() {
            return Err("源码摘要与模块数对不上".into());
        }
        for (p, d) in programs.iter().zip(digests) {
            if d.is_empty() {
                return Err("有模块缺源码摘要".into());
            }
            let path = p.source_path.clone().unwrap_or_default();
            k.field(&format!("source {}", path), d);
        }

        // 链接器 + 运行时归档
        if self.交叉到linux() {
            k.tool_on_path("zig")?;
            let rt = self
                .find_linux_runtime_library()
                .map_err(|e| format!("{:?}", e))?;
            k.file_identity("runtime", &rt)?;
        } else {
            k.tool_on_path("clang")?;
            // clang 驱动实际调的链接器：mac 是 /usr/bin/ld（工具链更新会换掉它）
            if cfg!(target_os = "macos") {
                k.file_identity("tool ld", Path::new("/usr/bin/ld"))?;
            } else if let Some(ld) = find_on_path("ld") {
                k.file_identity("tool ld", &ld)?;
            }
            let rt = self
                .find_host_runtime_library()
                .map_err(|e| format!("{:?}", e))?;
            k.file_identity("runtime", &rt)?;
        }

        // 外部库：直链文件记身份；-l 库名在搜索路径里找到的每个候选都记身份
        let fallback_dir = source_file.parent().unwrap_or(Path::new("."));
        let libs =
            Self::collect_extern_libs(programs, fallback_dir).map_err(|e| format!("{:?}", e))?;
        let search = self.库搜索路径().map_err(|e| format!("{:?}", e))?;
        for dir in &search {
            k.field("lib-search", &dir.display().to_string());
        }
        for item in &libs {
            match item {
                crate::链接::链接项::文件(p) => k.file_identity("extern-file", p)?,
                crate::链接::链接项::框架(name) => k.field("extern-framework", name),
                crate::链接::链接项::库名(name) => {
                    let found = find_library_candidates(name, &search);
                    if found.is_empty() {
                        if SYSTEM_LIBS.contains(&name.as_str()) {
                            k.field("extern-system-lib", name);
                        } else {
                            return Err(format!("-l{} 在已知目录里找不到对应文件", name));
                        }
                    }
                    for f in found {
                        k.file_identity(&format!("extern-lib {}", name), &f)?;
                    }
                }
            }
        }
        Ok(k.finish())
    }
}

/// `-l<name>` 可能落到的文件：显式搜索路径 + LIBRARY_PATH + 常见前缀 + SDK。
fn find_library_candidates(name: &str, search: &[PathBuf]) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = search.to_vec();
    if let Some(lp) = std::env::var_os("LIBRARY_PATH") {
        dirs.extend(std::env::split_paths(&lp));
    }
    for d in ["/usr/local/lib", "/opt/homebrew/lib", "/usr/lib"] {
        dirs.push(PathBuf::from(d));
    }
    if let Some(sdk) = std::env::var_os("SDKROOT") {
        dirs.push(PathBuf::from(sdk).join("usr/lib"));
    }
    let mut found = Vec::new();
    for d in dirs {
        for ext in ["a", "dylib", "so", "tbd", "lib"] {
            let f = d.join(format!("lib{}.{}", name, ext));
            if f.is_file() {
                found.push(f);
            }
        }
        let f = d.join(format!("{}.lib", name));
        if f.is_file() {
            found.push(f);
        }
    }
    found
}
