//! `QI_TIME=1`：把编译各阶段耗时打到 stderr。
//!
//! 只是观测，不改变任何产物（所以它**不**进构建缓存的键）。
//! 量编译速度时先开它看大头在哪，别猜。

use std::sync::OnceLock;
use std::time::Instant;

fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        matches!(
            std::env::var("QI_TIME").as_deref(),
            Ok("1") | Ok("on") | Ok("true")
        )
    })
}

/// 从 `since` 到现在的耗时，记为阶段 `phase`；返回「现在」，方便串着量下一段。
pub fn mark(phase: &str, since: Instant) -> Instant {
    let now = Instant::now();
    if enabled() {
        eprintln!(
            "[qi 计时] {:<12} {:>8.1} ms",
            phase,
            now.duration_since(since).as_secs_f64() * 1000.0
        );
    }
    now
}
