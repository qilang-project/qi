如果需要支持**异步和协程**，我强烈建议用 **Rust** 而不是 C 来写 runtime。原因如下：

## 为什么用 Rust 写 Runtime 更好

### 1. **协程/异步支持** 🚀

**Rust 优势：**

- Rust 有成熟的 async/await 生态系统
- 可以直接使用 tokio、async-std 等现成的异步 runtime
- Future trait 和 Waker 机制已经实现好了

**C 劣势：**

- 需要从零实现协程调度器
- 需要手动管理栈切换（汇编代码）
- 上下文切换容易出错

### 2. **内存安全** 🛡️

**Rust 优势：**

- 编译期防止数据竞争
- 借用检查器保证无悬垂指针
- 类型系统防止内存泄漏

**C 劣势：**

- 手动管理内存容易出错
- 数据竞争难以调试
- 需要大量测试

### 3. **性能** ⚡

两者性能相当，但 Rust：

- 零成本抽象
- LLVM 优化
- 无 GC 开销

### 4. **开发效率** 💻

**Rust 优势：**

- Cargo 包管理
- 丰富的生态系统
- 编译器错误提示友好
- 可以直接集成到 Qi 项目中

## 推荐方案：Rust Runtime

让我为 Qi 设计一个基于 Rust 的 runtime，支持异步和协程：

### 项目结构

```
qi/
├── Cargo.toml
├── runtime/
│   ├── Cargo.toml
│   ├── src/
│   │   ├── lib.rs           # Runtime 入口
│   │   ├── memory.rs        # 内存管理
│   │   ├── string.rs        # 字符串操作
│   │   ├── array.rs         # 动态数组
│   │   ├── async_rt.rs      # 异步 runtime
│   │   ├── coroutine.rs     # 协程支持
│   │   └── ffi.rs           # C FFI 接口
│   └── build.rs
└── src/
    └── codegen/
        └── runtime.rs       # 生成调用 runtime 的代码
```

### 实现：Qi Runtime (Rust)

```rust
[package]
name = "qi-runtime"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["staticlib", "cdylib"]

[dependencies]
tokio = { version = "1.35", features = ["full"] }
futures = "0.3"
crossbeam = "0.8"

[build-dependencies]
cbindgen = "0.26"
```

```rust
//! Qi Language Runtime
//!
//! 提供内存管理、字符串操作、异步执行和协程支持

pub mod memory;
pub mod string;
pub mod array;
pub mod async_rt;
pub mod coroutine;
pub mod ffi;

// 导出 C FFI 接口
pub use ffi::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_initialization() {
        unsafe {
            qi_runtime_init();
        }
    }
}
```

```rust
//! 内存管理模块
//!
//! 提供内存分配、释放和引用计数

use std::alloc::{alloc, dealloc, Layout};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Qi 对象头（用于引用计数和类型信息）
#[repr(C)]
pub struct QiObjectHeader {
    /// 引用计数
    ref_count: AtomicUsize,
    /// 对象大小（字节）
    size: usize,
    /// 类型标签
    type_tag: u32,
}

impl QiObjectHeader {
    pub fn new(size: usize, type_tag: u32) -> Self {
        Self {
            ref_count: AtomicUsize::new(1),
            size,
            type_tag,
        }
    }

    pub fn retain(&self) {
        self.ref_count.fetch_add(1, Ordering::SeqCst);
    }

    pub fn release(&self) -> bool {
        self.ref_count.fetch_sub(1, Ordering::SeqCst) == 1
    }
}

/// 分配内存（带引用计数）
///
/// # Safety
/// 调用者必须确保正确管理返回的指针
pub unsafe fn qi_alloc(size: usize, type_tag: u32) -> *mut u8 {
    let total_size = std::mem::size_of::<QiObjectHeader>() + size;
    let layout = Layout::from_size_align_unchecked(total_size, 8);

    let ptr = alloc(layout);
    if ptr.is_null() {
        panic!("Out of memory: failed to allocate {} bytes", total_size);
    }

    // 初始化对象头
    let header = ptr as *mut QiObjectHeader;
    header.write(QiObjectHeader::new(size, type_tag));

    // 返回数据部分的指针
    ptr.add(std::mem::size_of::<QiObjectHeader>())
}

/// 释放内存
///
/// # Safety
/// ptr 必须是通过 qi_alloc 分配的有效指针
pub unsafe fn qi_free(ptr: *mut u8) {
    if ptr.is_null() {
        return;
    }

    // 获取对象头
    let header_ptr = ptr.sub(std::mem::size_of::<QiObjectHeader>()) as *mut QiObjectHeader;
    let header = &*header_ptr;

    // 减少引用计数
    if header.release() {
        // 引用计数为 0，真正释放内存
        let total_size = std::mem::size_of::<QiObjectHeader>() + header.size;
        let layout = Layout::from_size_align_unchecked(total_size, 8);
        dealloc(header_ptr as *mut u8, layout);
    }
}

/// 增加引用计数
///
/// # Safety
/// ptr 必须是通过 qi_alloc 分配的有效指针
pub unsafe fn qi_retain(ptr: *mut u8) {
    if ptr.is_null() {
        return;
    }

    let header_ptr = ptr.sub(std::mem::size_of::<QiObjectHeader>()) as *mut QiObjectHeader;
    let header = &*header_ptr;
    header.retain();
}
```

```rust
//! 字符串操作模块
//!
//! 提供 UTF-8 字符串的分配、拼接和操作

use crate::memory::{qi_alloc, qi_free};
use std::ffi::CStr;
use std::slice;

/// Qi 字符串结构
#[repr(C)]
pub struct QiString {
    data: *mut u8,
    length: usize,
    capacity: usize,
}

impl QiString {
    /// 创建新字符串
    pub fn new(s: &str) -> *mut QiString {
        unsafe {
            let string_ptr = qi_alloc(std::mem::size_of::<QiString>(), 1) as *mut QiString;
            let data_ptr = qi_alloc(s.len() + 1, 0);

            std::ptr::copy_nonoverlapping(s.as_ptr(), data_ptr, s.len());
            data_ptr.add(s.len()).write(0); // null terminator

            (*string_ptr).data = data_ptr;
            (*string_ptr).length = s.len();
            (*string_ptr).capacity = s.len() + 1;

            string_ptr
        }
    }

    /// 字符串拼接
    pub fn concat(a: *const QiString, b: *const QiString) -> *mut QiString {
        unsafe {
            let a_ref = &*a;
            let b_ref = &*b;

            let new_len = a_ref.length + b_ref.length;
            let string_ptr = qi_alloc(std::mem::size_of::<QiString>(), 1) as *mut QiString;
            let data_ptr = qi_alloc(new_len + 1, 0);

            // 拷贝第一个字符串
            std::ptr::copy_nonoverlapping(a_ref.data, data_ptr, a_ref.length);
            // 拷贝第二个字符串
            std::ptr::copy_nonoverlapping(b_ref.data, data_ptr.add(a_ref.length), b_ref.length);
            // null terminator
            data_ptr.add(new_len).write(0);

            (*string_ptr).data = data_ptr;
            (*string_ptr).length = new_len;
            (*string_ptr).capacity = new_len + 1;

            string_ptr
        }
    }

    /// 获取字符串切片
    pub fn as_str(&self) -> &str {
        unsafe {
            let slice = slice::from_raw_parts(self.data, self.length);
            std::str::from_utf8_unchecked(slice)
        }
    }

    /// 释放字符串
    pub fn free(ptr: *mut QiString) {
        unsafe {
            if ptr.is_null() {
                return;
            }

            qi_free((*ptr).data);
            qi_free(ptr as *mut u8);
        }
    }
}
```

```rust
//! 异步运行时模块
//!
//! 基于 Tokio 提供异步执行支持

use std::future::Future;
use std::pin::Pin;
use std::sync::OnceLock;
use tokio::runtime::{Builder, Runtime};

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

/// 初始化异步运行时
pub fn init_async_runtime() {
    RUNTIME.get_or_init(|| {
        Builder::new_multi_thread()
            .worker_threads(4)
            .enable_all()
            .build()
            .expect("Failed to create Tokio runtime")
    });
}

/// 获取运行时引用
pub fn get_runtime() -> &'static Runtime {
    RUNTIME.get().expect("Runtime not initialized")
}

/// 异步任务句柄
#[repr(C)]
pub struct QiAsyncTask {
    // 使用 Box 包装以保持稳定的内存地址
    inner: Box<tokio::task::JoinHandle<()>>,
}

/// 生成异步任务
pub fn spawn_async<F>(future: F) -> *mut QiAsyncTask
where
    F: Future<Output = ()> + Send + 'static,
{
    let handle = get_runtime().spawn(future);
    Box::into_raw(Box::new(QiAsyncTask {
        inner: Box::new(handle),
    }))
}

/// 等待异步任务完成
pub async fn await_task(task: *mut QiAsyncTask) {
    unsafe {
        if task.is_null() {
            return;
        }
        let task_box = Box::from_raw(task);
        let _ = task_box.inner.await;
    }
}

/// 阻塞等待异步任务
pub fn block_on_task(task: *mut QiAsyncTask) {
    get_runtime().block_on(async {
        await_task(task).await;
    });
}
```

```rust
//! 协程模块
//!
//! 提供轻量级协程（绿色线程）支持

use crossbeam::channel::{bounded, Sender, Receiver};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::thread;

/// 协程状态
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CoroutineState {
    Ready,
    Running,
    Suspended,
    Completed,
}

/// 协程结构
pub struct Coroutine {
    id: usize,
    state: CoroutineState,
    // 协程函数指针（从 Qi 代码生成）
    func: extern "C" fn(*mut u8) -> *mut u8,
    // 协程上下文数据
    context: *mut u8,
}

unsafe impl Send for Coroutine {}

/// 协程调度器
pub struct CoroutineScheduler {
    ready_queue: Arc<Mutex<VecDeque<Coroutine>>>,
    worker_count: usize,
    shutdown_tx: Sender<()>,
    shutdown_rx: Receiver<()>,
}

impl CoroutineScheduler {
    /// 创建新调度器
    pub fn new(worker_count: usize) -> Self {
        let (shutdown_tx, shutdown_rx) = bounded(1);

        Self {
            ready_queue: Arc::new(Mutex::new(VecDeque::new())),
            worker_count,
            shutdown_tx,
            shutdown_rx,
        }
    }

    /// 生成协程
    pub fn spawn(&self, func: extern "C" fn(*mut u8) -> *mut u8, context: *mut u8) -> usize {
        static mut NEXT_ID: usize = 0;

        let id = unsafe {
            let id = NEXT_ID;
            NEXT_ID += 1;
            id
        };

        let coroutine = Coroutine {
            id,
            state: CoroutineState::Ready,
            func,
            context,
        };

        self.ready_queue.lock().unwrap().push_back(coroutine);
        id
    }

    /// 启动调度器
    pub fn run(&self) {
        let mut workers = Vec::new();

        for _ in 0..self.worker_count {
            let queue = Arc::clone(&self.ready_queue);
            let shutdown = self.shutdown_rx.clone();

            let handle = thread::spawn(move || {
                loop {
                    // 检查是否需要关闭
                    if shutdown.try_recv().is_ok() {
                        break;
                    }

                    // 从队列获取协程
                    let mut coroutine = {
                        let mut q = queue.lock().unwrap();
                        q.pop_front()
                    };

                    if let Some(mut co) = coroutine {
                        // 执行协程
                        co.state = CoroutineState::Running;
                        let result = (co.func)(co.context);

                        // 检查协程是否完成
                        if result.is_null() {
                            co.state = CoroutineState::Completed;
                        } else {
                            // 协程被挂起，重新加入队列
                            co.state = CoroutineState::Suspended;
                            queue.lock().unwrap().push_back(co);
                        }
                    } else {
                        // 队列为空，休眠一会
                        thread::sleep(std::time::Duration::from_millis(10));
                    }
                }
            });

            workers.push(handle);
        }

        // 等待所有 worker 完成
        for handle in workers {
            handle.join().unwrap();
        }
    }

    /// 关闭调度器
    pub fn shutdown(&self) {
        let _ = self.shutdown_tx.send(());
    }
}

// 全局调度器
static mut SCHEDULER: Option<CoroutineScheduler> = None;

/// 初始化协程调度器
pub fn init_coroutine_scheduler(worker_count: usize) {
    unsafe {
        SCHEDULER = Some(CoroutineScheduler::new(worker_count));
    }
}

/// 生成协程
pub fn spawn_coroutine(func: extern "C" fn(*mut u8) -> *mut u8, context: *mut u8) -> usize {
    unsafe {
        SCHEDULER
            .as_ref()
            .expect("Coroutine scheduler not initialized")
            .spawn(func, context)
    }
}

/// 启动调度器
pub fn run_scheduler() {
    unsafe {
        SCHEDULER
            .as_ref()
            .expect("Coroutine scheduler not initialized")
            .run()
    }
}
```

```rust
//! C FFI 接口
//!
//! 为 LLVM 生成的代码提供 C ABI 接口

use crate::memory::*;
use crate::string::*;
use crate::async_rt::*;
use crate::coroutine::*;
use std::ffi::CStr;
use std::os::raw::c_char;

/// 初始化 Qi Runtime
#[no_mangle]
pub extern "C" fn qi_runtime_init() {
    init_async_runtime();
    init_coroutine_scheduler(4);
}

/// 分配内存
#[no_mangle]
pub unsafe extern "C" fn qi_malloc(size: usize) -> *mut u8 {
    qi_alloc(size, 0)
}

/// 释放内存
#[no_mangle]
pub unsafe extern "C" fn qi_free_ptr(ptr: *mut u8) {
    qi_free(ptr);
}

/// 增加引用计数
#[no_mangle]
pub unsafe extern "C" fn qi_retain_ptr(ptr: *mut u8) {
    qi_retain(ptr);
}

/// 创建字符串
#[no_mangle]
pub unsafe extern "C" fn qi_string_new(data: *const c_char) -> *mut QiString {
    let c_str = CStr::from_ptr(data);
    let str_slice = c_str.to_str().expect("Invalid UTF-8 string");
    QiString::new(str_slice)
}

/// 字符串拼接
#[no_mangle]
pub unsafe extern "C" fn qi_string_concat(
    a: *const QiString,
    b: *const QiString,
) -> *mut QiString {
    QiString::concat(a, b)
}

/// 释放字符串
#[no_mangle]
pub unsafe extern "C" fn qi_string_free(ptr: *mut QiString) {
    QiString::free(ptr);
}

/// 打印整数
#[no_mangle]
pub extern "C" fn qi_print_int(value: i64) {
    println!("{}", value);
}

/// 打印字符串
#[no_mangle]
pub unsafe extern "C" fn qi_print_string(str_ptr: *const QiString) {
    if str_ptr.is_null() {
        return;
    }
    let string = &*str_ptr;
    println!("{}", string.as_str());
}

/// Panic 处理
#[no_mangle]
pub unsafe extern "C" fn qi_panic(message: *const c_char) {
    let c_str = CStr::from_ptr(message);
    let msg = c_str.to_str().unwrap_or("Unknown error");
    panic!("Qi Runtime Panic: {}", msg);
}

/// 生成异步任务（简化接口）
#[no_mangle]
pub unsafe extern "C" fn qi_spawn_async(
    func: extern "C" fn(*mut u8),
    context: *mut u8,
) -> *mut QiAsyncTask {
    spawn_async(async move {
        func(context);
    })
}

/// 等待异步任务
#[no_mangle]
pub unsafe extern "C" fn qi_await_task(task: *mut QiAsyncTask) {
    block_on_task(task);
}

/// 生成协程
#[no_mangle]
pub unsafe extern "C" fn qi_spawn_coroutine(
    func: extern "C" fn(*mut u8) -> *mut u8,
    context: *mut u8,
) -> usize {
    spawn_coroutine(func, context)
}

/// 启动协程调度器
#[no_mangle]
pub extern "C" fn qi_run_scheduler() {
    run_scheduler();
}

/// Yield 当前协程
#[no_mangle]
pub extern "C" fn qi_yield() {
    std::thread::yield_now();
}
```

### 集成到 Qi 编译器

```rust
//! Runtime 集成
//!
//! 在代码生成时声明和调用 runtime 函数

use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::types::*;
use inkwell::AddressSpace;

pub struct RuntimeFunctions<'ctx> {
    // 内存管理
    pub qi_malloc: FunctionValue<'ctx>,
    pub qi_free: FunctionValue<'ctx>,
    pub qi_retain: FunctionValue<'ctx>,

    // 字符串操作
    pub qi_string_new: FunctionValue<'ctx>,
    pub qi_string_concat: FunctionValue<'ctx>,
    pub qi_string_free: FunctionValue<'ctx>,

    // 打印
    pub qi_print_int: FunctionValue<'ctx>,
    pub qi_print_string: FunctionValue<'ctx>,

    // 异步/协程
    pub qi_spawn_async: FunctionValue<'ctx>,
    pub qi_await_task: FunctionValue<'ctx>,
    pub qi_spawn_coroutine: FunctionValue<'ctx>,
    pub qi_yield: FunctionValue<'ctx>,
}

impl<'ctx> RuntimeFunctions<'ctx> {
    pub fn declare(context: &'ctx Context, module: &Module<'ctx>) -> Self {
        let i8_type = context.i8_type();
        let i32_type = context.i32_type();
        let i64_type = context.i64_type();
        let void_type = context.void_type();
        let i8_ptr = i8_type.ptr_type(AddressSpace::default());

        // void* qi_malloc(size_t)
        let qi_malloc = module.add_function(
            "qi_malloc",
            i8_ptr.fn_type(&[i64_type.into()], false),
            None,
        );

        // void qi_free(void*)
        let qi_free = module.add_function(
            "qi_free_ptr",
            void_type.fn_type(&[i8_ptr.into()], false),
            None,
        );

        // void qi_retain(void*)
        let qi_retain = module.add_function(
            "qi_retain_ptr",
            void_type.fn_type(&[i8_ptr.into()], false),
            None,
        );

        // QiString* qi_string_new(const char*)
        let qi_string_new = module.add_function(
            "qi_string_new",
            i8_ptr.fn_type(&[i8_ptr.into()], false),
            None,
        );

        // QiString* qi_string_concat(QiString*, QiString*)
        let qi_string_concat = module.add_function(
            "qi_string_concat",
            i8_ptr.fn_type(&[i8_ptr.into(), i8_ptr.into()], false),
            None,
        );

        // void qi_string_free(QiString*)
        let qi_string_free = module.add_function(
            "qi_string_free",
            void_type.fn_type(&[i8_ptr.into()], false),
            None,
        );

        // void qi_print_int(i64)
        let qi_print_int = module.add_function(
            "qi_print_int",
            void_type.fn_type(&[i64_type.into()], false),
            None,
        );

        // void qi_print_string(QiString*)
        let qi_print_string = module.add_function(
            "qi_print_string",
            void_type.fn_type(&[i8_ptr.into()], false),
            None,
        );

        // void* qi_spawn_async(func, context)
        let func_type = void_type.fn_type(&[i8_ptr.into()], false);
        let func_ptr = func_type.ptr_type(AddressSpace::default());
        let qi_spawn_async = module.add_function(
            "qi_spawn_async",
            i8_ptr.fn_type(&[func_ptr.into(), i8_ptr.into()], false),
            None,
        );

        // void qi_await_task(void*)
        let qi_await_task = module.add_function(
            "qi_await_task",
            void_type.fn_type(&[i8_ptr.into()], false),
            None,
        );

        // usize qi_spawn_coroutine(func, context)
        let coroutine_func_type = i8_ptr.fn_type(&[i8_ptr.into()], false);
        let coroutine_func_ptr = coroutine_func_type.ptr_type(AddressSpace::default());
        let qi_spawn_coroutine = module.add_function(
            "qi_spawn_coroutine",
            i64_type.fn_type(&[coroutine_func_ptr.into(), i8_ptr.into()], false),
            None,
        );

        // void qi_yield()
        let qi_yield = module.add_function(
            "qi_yield",
            void_type.fn_type(&[], false),
            None,
        );

        Self {
            qi_malloc,
            qi_free,
            qi_retain,
            qi_string_new,
            qi_string_concat,
            qi_string_free,
            qi_print_int,
            qi_print_string,
            qi_spawn_async,
            qi_await_task,
            qi_spawn_coroutine,
            qi_yield,
        }
    }
}
```

### 更新 Qi 编译器使用 Runtime

```rust
// 在 src/codegen/builder.rs 中
use crate::codegen::runtime::RuntimeFunctions;

impl<'ctx> CodeGen<'ctx> {
    pub fn new(context: &'ctx Context, module_name: &str) -> Self {
        let module = context.create_module(module_name);
        let builder = context.create_builder();

        // 声明 runtime 函数
        let runtime = RuntimeFunctions::declare(context, &module);

        Self {
            context,
            module,
            builder,
            runtime,
            // ... 其他字段
        }
    }

    fn compile_print(&mut self, value: &AstNode) {
        let compiled_value = self.compile_expression(value);

        // 使用 runtime 函数打印
        self.builder.build_call(
            self.runtime.qi_print_int,
            &[compiled_value.into()],
            "print_call"
        );
    }
}
```

### 使用示例：Qi 代码

```qi
// 异步函数
异步 函数 获取数据(url: 字符串): 字符串 {
    // 异步 HTTP 请求
    变量 响应 = 等待 http.get(url);
    返回 响应.文本();
}

// 协程
协程 函数 生成器(n: 整数) {
    对于 i 在 范围(0, n) {
        让出 i;  // yield
    }
}

函数 主函数() {
    // 使用异步
    变量 任务 = 启动 获取数据("https://api.example.com");
    变量 数据 = 等待 任务;
    打印 数据;

    // 使用协程
    变量 生成 = 启动协程 生成器(10);
    当 变量 值 = 恢复(生成) {
        打印 值;
    }
}
```

## 总结

**强烈推荐用 Rust 写 Runtime**，因为：

✅ **异步支持更容易** - 直接使用 Tokio
✅ **内存安全** - 编译期保证
✅ **性能相当** - 零成本抽象
✅ **开发效率高** - 丰富的生态系统
✅ **集成简单** - 与 Qi 编译器在同一个项目中

我已经提供了完整的实现框架，你可以：

1. 创建 runtime 子项目
2. 逐步实现各个模块
3. 在 Qi 编译器中集成 runtime
