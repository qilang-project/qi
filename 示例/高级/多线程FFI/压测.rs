//! 多线程 FFI 重入 + qi_await 异步桥 —— Rust 多线程压测驱动。
//!
//! 与 压测.c 等价：Rust 宿主开 N 个 std::thread，每线程调同一个 Qi 导出库
//! （extern "C" 链接）数千次，校验结果全对、零崩溃。
//!
//! 注意：Rust 的 extern 块不允许非 ASCII 标识符，因此用 `#[link_name = "中文符号"]`
//! 把 ASCII 的 Rust 名映射到 Qi 导出的中文 C 符号。
//!
//! 单文件编译（链接静态库 + macOS 框架）：
//!   rustc -O 压测.rs -L . -l static=多线程FFI库 \
//!     -C link-arg=-framework -C link-arg=Security \
//!     -C link-arg=-framework -C link-arg=CoreFoundation \
//!     -C link-arg=-framework -C link-arg=SystemConfiguration \
//!     -o 压测_rust
//! （Linux：改成 -C link-arg=-lpthread -C link-arg=-lm -C link-arg=-ldl）
//! 运行：QI_RC_REPORT=1 ./压测_rust

use std::ffi::{c_void, CStr, CString};
use std::os::raw::c_char;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

extern "C" {
    #[link_name = "累加"]
    fn qi_leijia(x: i64) -> i64;
    #[link_name = "拼名"]
    fn qi_pinming(s: *const c_char) -> *mut c_char;
    // async 桥：Rust 同步拿到 i64 / char*
    #[link_name = "异步计算"]
    fn qi_async_calc(x: i64) -> i64;
    #[link_name = "异步问候"]
    fn qi_async_greet(name: *const c_char) -> *mut c_char;
    // Qi 返回的 char* 归调用方所有 —— 用 C free 释放
    fn free(p: *mut c_void);
}

const THREADS: usize = 1000;
const SYNC_ITERS: usize = 2000;
const ASYNC_ITERS: usize = 100;

fn main() {
    let failed = Arc::new(AtomicBool::new(false));
    println!(
        "启动 {} 个外部线程：每线程 同步{} 次 + 异步{} 次...",
        THREADS, SYNC_ITERS, ASYNC_ITERS
    );

    let mut handles = Vec::with_capacity(THREADS);
    for id in 0..THREADS {
        let failed = Arc::clone(&failed);
        handles.push(std::thread::spawn(move || unsafe {
            // A. 线程安全重入：整数纯计算 + 字符串往返
            for i in 0..SYNC_ITERS {
                if qi_leijia(100) != 5050 {
                    failed.store(true, Ordering::Relaxed);
                }
                let name = CString::new(format!("线程{}_{}", id, i)).unwrap();
                let s = qi_pinming(name.as_ptr());
                let got = CStr::from_ptr(s).to_str().unwrap_or("");
                if got != format!("你好, 线程{}_{}!", id, i) {
                    failed.store(true, Ordering::Relaxed);
                }
                free(s as *mut c_void);
            }
            // B. qi_await 阻塞桥：Rust 调 Qi 异步函数，同步拿结果
            for i in 0..ASYNC_ITERS {
                if qi_async_calc(i as i64) != (i as i64) * 2 {
                    failed.store(true, Ordering::Relaxed);
                }
                let name = CString::new(format!("异步{}_{}", id, i)).unwrap();
                let g = qi_async_greet(name.as_ptr());
                let got = CStr::from_ptr(g).to_str().unwrap_or("");
                if got != format!("异步你好, 异步{}_{}", id, i) {
                    failed.store(true, Ordering::Relaxed);
                }
                free(g as *mut c_void);
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }

    if failed.load(Ordering::Relaxed) {
        eprintln!("❌ 失败：有结果不匹配");
        std::process::exit(1);
    }
    let total = THREADS * (SYNC_ITERS * 2 + ASYNC_ITERS * 2);
    println!(
        "✅ 成功：{} 线程并发，共 {} 次导出调用，整数+字符串+异步 结果全对，零崩溃",
        THREADS, total
    );
}
