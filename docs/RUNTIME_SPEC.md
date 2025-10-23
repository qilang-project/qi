# Qi 运行时系统规范文档

## 概述

Qi 运行时系统是一个轻量级、高性能的运行时环境，为 Qi 语言提供内存管理、并发调度、系统调用抽象等核心功能。运行时采用 Rust + C 混合架构，实现跨平台兼容性。

## 架构设计

```
┌─────────────────────────────────────┐
│          Qi 运行时系统              │
├─────────────────────────────────────┤
│         Rust 运行时层               │
│  ┌─────────────┬─────────────────┐   │
│  │  内存分配器  │    并发调度器    │   │
│  │ (Allocator) │ (M:N, Async)    │   │
│  └─────────────┴─────────────────┘   │
│  ┌─────────────┬─────────────────┐   │
│  │  错误处理   │    FFI 绑定      │   │
│  │ (Panic)     │ (C Interface)   │   │
│  └─────────────┴─────────────────┘   │
├─────────────────────────────────────┤
│         C 平台抽象层                │
│  ┌─────────────┬─────────────────┐   │
│  │  内存管理   │    线程管理      │   │
│  │ (Memory)    │ (Thread)        │   │
│  └─────────────┴─────────────────┘   │
│  ┌─────────────┬─────────────────┐   │
│  │  文件 I/O   │    错误处理      │   │
│  │ (IO)        │ (Error)         │   │
│  └─────────────┴─────────────────┘   │
├─────────────────────────────────────┤
│        操作系统接口                 │
│  ┌─────────┬─────────┬─────────┐   │
│  │  Windows │  Linux  │  macOS  │   │
│  ├─────────┴─────────┴─────────┤   │
│  │      WASM (WASI)            │   │
│  └─────────────────────────────┘   │
└─────────────────────────────────────┘
```

## 核心模块

### 1. 内存分配器 (Memory Allocator)

**技术选型**: 基于 jemalloc/mimalloc 的 Rust 封装

**功能特性**:
- 高性能内存分配
- 内存池管理
- 缓存友好的分配策略
- 内存泄漏检测 (开发模式)

**API 设计**:

```rust
// runtime/src/memory/allocator.rs
pub struct QiAllocator;

impl QiAllocator {
    // 分配指定大小的内存块
    pub fn alloc(size: usize) -> *mut u8;

    // 重新分配内存块
    pub fn realloc(ptr: *mut u8, old_size: usize, new_size: usize) -> *mut u8;

    // 释放内存块
    pub fn dealloc(ptr: *mut u8, size: usize);

    // 获取内存使用统计
    pub fn stats() -> MemoryStats;
}

#[derive(Debug)]
pub struct MemoryStats {
    pub allocated: usize,
    pub cached: usize,
    pub peak: usize,
    pub fragments: usize,
}
```

**内存管理策略**:
1. **小对象分配**: 使用内存池，大小分类管理
2. **大对象分配**: 直接向操作系统申请
3. **内存回收**: 延迟回收 + 压缩策略
4. **NUMA 优化**: 多节点环境下的内存亲和性

### 2. 并发调度器 (Concurrency Scheduler)

**调度模型**: M:N 协程调度

**核心组件**:
- **任务队列**: 多级优先级队列
- **工作线程**: 固定数量的 OS 线程
- **调度算法**: 工作窃取 + 抢占式调度
- **负载均衡**: 动态负载均衡

**API 设计**:

```rust
// runtime/src/concurrent/scheduler.rs
pub struct Scheduler;

impl Scheduler {
    // 启动调度器
    pub fn start(thread_count: usize) -> Result<(), Error>;

    // 关闭调度器
    pub fn shutdown() -> Result<(), Error>;

    // 提交任务
    pub fn spawn<F, R>(future: F) -> TaskHandle<R>
    where
        F: Future<Output = R> + Send + 'static,
        R: Send + 'static;

    // 让出当前任务
    pub fn yield_now();

    // 获取调度器状态
    pub fn stats() -> SchedulerStats;
}

pub struct TaskHandle<T> {
    task_id: TaskId,
    _phantom: PhantomData<T>,
}

impl<T> TaskHandle<T> {
    // 等待任务完成
    pub async fn join(self) -> Result<T, PanicError>;

    // 取消任务
    pub fn cancel(&self) -> bool;

    // 检查任务状态
    pub fn is_finished(&self) -> bool;
}

#[derive(Debug)]
pub struct SchedulerStats {
    pub total_tasks: usize,
    pub running_tasks: usize,
    pub worker_threads: usize,
    pub idle_threads: usize,
    pub queue_length: usize,
}
```

**调度算法**:
1. **任务创建**: 任务进入全局队列
2. **本地队列**: 每个工作线程维护本地队列
3. **工作窃取**: 空闲线程从其他线程窃取任务
4. **抢占机制**: 长时间运行的任务会被抢占
5. **优先级调度**: 支持任务优先级

### 3. 系统调用抽象层 (System Call Abstraction)

**设计原则**: 最小化接口，跨平台兼容

**C 接口定义**:

```c
// runtime_c_iface/include/qi_sys_memory.h
#ifndef QI_SYS_MEMORY_H
#define QI_SYS_MEMORY_H

#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

// 向操作系统请求原始内存页
void* qi_sys_alloc_pages(size_t num_pages);

// 释放内存页
void qi_sys_free_pages(void* ptr, size_t num_pages);

// 获取页面大小
size_t qi_sys_get_page_size(void);

// 内存保护
int qi_sys_protect_memory(void* ptr, size_t size, int protection);

// 内存同步
void qi_sys_sync_memory(void* ptr, size_t size);

#ifdef __cplusplus
}
#endif
#endif // QI_SYS_MEMORY_H
```

```c
// runtime_c_iface/include/qi_sys_io.h
#ifndef QI_SYS_IO_H
#define QI_SYS_IO_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef int qi_sys_fd_t;

// 文件操作
qi_sys_fd_t qi_sys_open(const char* path, int flags, int mode);
int qi_sys_close(qi_sys_fd_t fd);
ssize_t qi_sys_read(qi_sys_fd_t fd, void* buf, size_t count);
ssize_t qi_sys_write(qi_sys_fd_t fd, const void* buf, size_t count);
int64_t qi_sys_lseek(qi_sys_fd_t fd, int64_t offset, int whence);

// 文件信息
int qi_sys_stat(const char* path, qi_sys_stat_t* stat);
int qi_sys_fstat(qi_sys_fd_t fd, qi_sys_stat_t* stat);

// 目录操作
qi_sys_fd_t qi_sys_opendir(const char* path);
int qi_sys_readdir(qi_sys_fd_t fd, qi_sys_dirent_t* entry);
int qi_sys_closedir(qi_sys_fd_t fd);

#ifdef __cplusplus
}
#endif
#endif // QI_SYS_IO_H
```

```c
// runtime_c_iface/include/qi_sys_thread.h
#ifndef QI_SYS_THREAD_H
#define QI_SYS_THREAD_H

#include <stdbool.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef void* qi_sys_thread_t;
typedef void* qi_sys_mutex_t;
typedef void* qi_sys_condvar_t;
typedef void* (*qi_sys_thread_func_t)(void* arg);

// 线程操作
int qi_sys_thread_create(qi_sys_thread_t* out_thread,
                        qi_sys_thread_func_t func, void* arg);
int qi_sys_thread_join(qi_sys_thread_t thread);
void qi_sys_thread_detach(qi_sys_thread_t thread);
void qi_sys_thread_sleep(uint32_t milliseconds);
void qi_sys_thread_yield(void);

// 互斥锁操作
qi_sys_mutex_t* qi_sys_mutex_create(void);
void qi_sys_mutex_lock(qi_sys_mutex_t* mutex);
bool qi_sys_mutex_trylock(qi_sys_mutex_t* mutex);
void qi_sys_mutex_unlock(qi_sys_mutex_t* mutex);
void qi_sys_mutex_destroy(qi_sys_mutex_t* mutex);

// 条件变量操作
qi_sys_condvar_t* qi_sys_condvar_create(void);
void qi_sys_condvar_wait(qi_sys_condvar_t* cond, qi_sys_mutex_t* mutex);
bool qi_sys_condvar_timedwait(qi_sys_condvar_t* cond,
                              qi_sys_mutex_t* mutex, uint32_t timeout_ms);
void qi_sys_condvar_signal(qi_sys_condvar_t* cond);
void qi_sys_condvar_broadcast(qi_sys_condvar_t* cond);
void qi_sys_condvar_destroy(qi_sys_condvar_t* cond);

// 原子操作
int32_t qi_sys_atomic_add(volatile int32_t* ptr, int32_t value);
int32_t qi_sys_atomic_sub(volatile int32_t* ptr, int32_t value);
int32_t qi_sys_atomic_cas(volatile int32_t* ptr, int32_t expected, int32_t desired);
void* qi_sys_atomic_ptr_cas(void* volatile* ptr, void* expected, void* desired);

#ifdef __cplusplus
}
#endif
#endif // QI_SYS_THREAD_H
```

```c
// runtime_c_iface/include/qi_sys_error.h
#ifndef QI_SYS_ERROR_H
#define QI_SYS_ERROR_H

#ifdef __cplusplus
extern "C" {
#endif

// 错误码定义
typedef enum {
    QI_SYS_OK = 0,
    QI_SYS_ERROR_INVALID_ARG = -1,
    QI_SYS_ERROR_OUT_OF_MEMORY = -2,
    QI_SYS_ERROR_PERMISSION_DENIED = -3,
    QI_SYS_ERROR_NOT_FOUND = -4,
    QI_SYS_ERROR_ALREADY_EXISTS = -5,
    QI_SYS_ERROR_IO_ERROR = -6,
    QI_SYS_ERROR_TIMEOUT = -7,
    QI_SYS_ERROR_UNKNOWN = -99,
} qi_sys_error_t;

// 错误处理
int qi_sys_get_last_error(void);
const char* qi_sys_error_string(int error_code);
void qi_sys_panic(const char* message, const char* file, uint32_t line);

// 错误回调
typedef void (*qi_sys_error_handler_t)(qi_sys_error_t error, const char* message);
void qi_sys_set_error_handler(qi_sys_error_handler_t handler);

#ifdef __cplusplus
}
#endif
#endif // QI_SYS_ERROR_H
```

### 4. 错误处理系统 (Error Handling)

**错误处理策略**:
- **运行时错误**: panic 机制，提供详细错误信息
- **系统调用错误**: 错误码转换，提供统一错误接口
- **错误恢复**: 支持 panic hook 和错误恢复机制

**API 设计**:

```rust
// runtime/src/error.rs
use std::panic::{self, PanicInfo};

pub struct ErrorHandler;

impl ErrorHandler {
    // 设置 panic hook
    pub fn set_panic_hook(hook: Box<dyn Fn(&PanicInfo) + Send + Sync>);

    // 触发 panic
    pub fn panic(message: &str) -> !;

    // 捕获 panic
    pub fn catch_unwind<F, R>(f: F) -> Result<R, Box<dyn Any + Send>>
    where
        F: FnOnce() -> R + PanicUnwindSafe;
}

#[derive(Debug)]
pub struct Error {
    pub code: ErrorCode,
    pub message: String,
    pub source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

#[derive(Debug, Clone)]
pub enum ErrorCode {
    OutOfMemory,
    StackOverflow,
    DivisionByZero,
    IndexOutOfRange,
    InvalidArgument,
    PermissionDenied,
    NotFound,
    IoError,
    Timeout,
    Unknown(i32),
}
```

## 平台特定实现

### POSIX 平台 (Linux, macOS)

**实现特点**:
- 使用 `pthread` 进行线程管理
- 使用 `mmap` 进行内存分配
- 使用标准 POSIX 文件 I/O
- 支持 `epoll`/`kqueue` 高性能 I/O

**关键实现**:

```c
// runtime_c_iface/src/posix/memory.c
#include "qi_sys_memory.h"
#include <sys/mman.h>
#include <unistd.h>

void* qi_sys_alloc_pages(size_t num_pages) {
    size_t page_size = qi_sys_get_page_size();
    size_t total_size = num_pages * page_size;

    void* ptr = mmap(NULL, total_size,
                     PROT_READ | PROT_WRITE,
                     MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);

    return ptr == MAP_FAILED ? NULL : ptr;
}

size_t qi_sys_get_page_size(void) {
    return sysconf(_SC_PAGESIZE);
}
```

### Windows 平台

**实现特点**:
- 使用 Windows API 进行线程管理
- 使用 `VirtualAlloc` 进行内存分配
- 使用 Windows 文件 API
- 支持 I/O 完成端口

**关键实现**:

```c
// runtime_c_iface/src/windows/memory.c
#include "qi_sys_memory.h"
#include <windows.h>

void* qi_sys_alloc_pages(size_t num_pages) {
    SYSTEM_INFO si;
    GetSystemInfo(&si);

    size_t total_size = num_pages * si.dwPageSize;

    return VirtualAlloc(NULL, total_size,
                       MEM_COMMIT | MEM_RESERVE,
                       PAGE_READWRITE);
}

size_t qi_sys_get_page_size(void) {
    SYSTEM_INFO si;
    GetSystemInfo(&si);
    return si.dwPageSize;
}
```

### WebAssembly 平台 (WASI)

**实现特点**:
- 单线程运行时，M:1 调度
- 使用 WASI 接口进行系统调用
- 内存管理基于 WebAssembly 内存模型
- 支持异步 I/O 通过 JavaScript 互操作

**关键实现**:

```c
// runtime_c_iface/src/wasm_wasi/memory.c
#include "qi_sys_memory.h"
#include <stddef.h>

void* qi_sys_alloc_pages(size_t num_pages) {
    // WebAssembly 内存增长
    size_t current_size = __builtin_wasm_memory_size(0);
    size_t requested_size = current_size + num_pages;

    if (__builtin_wasm_memory_grow(0, requested_size - current_size) == -1) {
        return NULL;
    }

    return (void*)(current_size * 65536); // 页面大小为 64KB
}

size_t qi_sys_get_page_size(void) {
    return 65536; // WebAssembly 页面大小
}
```

## 性能优化策略

### 1. 内存优化

**策略**:
- **内存池**: 预分配常用大小的内存块
- **延迟分配**: 按需分配，避免内存浪费
- **内存压缩**: 定期压缩内存碎片
- **NUMA 感知**: 在多节点系统上优化内存访问

**监控指标**:
- 内存使用率
- 分配/释放频率
- 内存碎片率
- 缓存命中率

### 2. 调度优化

**策略**:
- **工作窃取**: 平衡工作负载
- **任务亲和性**: 相关任务在同一线程执行
- **自适应调度**: 根据负载调整调度策略
- **批处理**: 批量处理小任务减少上下文切换

**监控指标**:
- 任务队列长度
- 线程利用率
- 上下文切换次数
- 任务平均执行时间

### 3. I/O 优化

**策略**:
- **异步 I/O**: 非阻塞 I/O 操作
- **I/O 线程池**: 专门的 I/O 处理线程
- **缓存策略**: 文件系统缓存优化
- **批量操作**: 合并小 I/O 请求

## 测试与验证

### 1. 单元测试

- 内存分配器测试
- 调度器功能测试
- 系统调用接口测试
- 错误处理测试

### 2. 集成测试

- 跨平台兼容性测试
- 性能基准测试
- 压力测试
- 内存泄漏测试

### 3. 性能基准

**内存分配基准**:
- 分配速度: 1M 次分配/秒
- 释放速度: 1M 次释放/秒
- 内存碎片率: < 5%

**调度性能基准**:
- 任务创建延迟: < 1μs
- 上下文切换开销: < 100ns
- 吞吐量: > 1M 任务/秒

这个运行时系统规范为 Qi 语言的运行时实现提供了详细的技术指导和架构设计，确保高性能、高可靠性和跨平台兼容性。