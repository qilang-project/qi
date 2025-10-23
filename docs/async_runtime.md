# Qi 异步运行时文档 (Async Runtime Documentation)

## 概述 (Overview)

Qi 异步运行时是一个高性能、基于 Rust 和 C 的异步任务执行系统，为 Qi 编程语言提供并发和异步编程能力。

The Qi async runtime is a high-performance asynchronous task execution system based on Rust and C, providing concurrency and asynchronous programming capabilities for the Qi programming language.

## 架构 (Architecture)

### 组件 (Components)

1. **执行器 (Executor)** - `src/runtime/async_runtime/executor.rs`
   - 管理任务的创建和生命周期
   - 基于 Tokio 的 Future 执行
   - 任务计数和统计信息

2. **调度器 (Scheduler)** - `src/runtime/async_runtime/scheduler.rs`
   - 任务元数据管理
   - 任务注册与注销
   - 调度统计

3. **任务 (Task)** - `src/runtime/async_runtime/task.rs`
   - 任务抽象与句柄
   - 优先级支持 (Low, Normal, High, Critical)
   - 任务状态追踪 (Pending, Running, Waiting, Completed, Cancelled, Failed)

4. **工作池 (Worker Pool)** - `src/runtime/async_runtime/pool.rs`
   - 工作线程配置
   - 任务队列管理
   - 可配置的工作窃取

5. **任务队列 (Task Queue)** - `src/runtime/async_runtime/queue.rs`
   - 线程安全的任务队列
   - 支持任务插入、弹出和删除

6. **状态管理 (State Manager)** - `src/runtime/async_runtime/state.rs`
   - 运行时状态追踪 (Idle, Running, ShuttingDown, Stopped)

7. **FFI/系统调用 (FFI/Syscalls)** - `src/runtime/async_runtime/ffi/`
   - 平台特定的系统调用包装
   - C 实现的底层操作：睡眠、计时、CPU 信息
   - 事件循环抽象 (epoll/kqueue/IOCP)

### Rust vs C 分工 (Rust vs C Responsibilities)

#### Rust 部分
- 任务调度和执行逻辑
- Future/async-await 抽象
- 线程安全的数据结构
- 高层错误处理
- 与 Tokio 运行时集成

#### C 部分 (`src/runtime/async_runtime/c_runtime/syscalls.c`)
- `qi_async_sys_sleep_ms`: 毫秒级睡眠
- `qi_async_sys_monotonic_time_ns`: 获取单调时间（纳秒）
- `qi_async_sys_cpu_time_ns`: 获取 CPU 时间
- `qi_async_sys_yield`: 线程让步
- `qi_async_sys_cpu_count`: 获取 CPU 核心数

这些 C 函数通过 FFI 在 Rust 中调用，提供跨平台的系统调用接口。

## 使用方法 (Usage)

### 基本示例 (Basic Example)

```rust
use qi_compiler::runtime::{AsyncRuntime, AsyncRuntimeConfig};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 创建配置
    let config = AsyncRuntimeConfig {
        worker_threads: 4,
        queue_capacity: 1024,
        max_stack_size: 2 * 1024 * 1024,
        stack_pool_size: 128,
        poll_interval: Duration::from_millis(1),
        enable_work_stealing: true,
        debug: false,
    };

    // 创建运行时
    let runtime = AsyncRuntime::new(config)?;

    // 生成任务
    let handle = runtime.spawn(async {
        println!("异步任务执行中...");
        tokio::time::sleep(Duration::from_secs(1)).await;
        println!("任务完成！");
    });

    // 等待任务完成
    handle.join().await?;

    // 关闭运行时
    runtime.shutdown()?;

    Ok(())
}
```

### 优先级任务 (Priority Tasks)

```rust
use qi_compiler::runtime::async_runtime::TaskPriority;

// 生成高优先级任务
let high_priority_handle = runtime.spawn_with_priority(
    async { /* 任务代码 */ },
    TaskPriority::High
);

// 生成普通优先级任务
let normal_priority_handle = runtime.spawn_with_priority(
    async { /* 任务代码 */ },
    TaskPriority::Normal
);

// 生成低优先级任务
let low_priority_handle = runtime.spawn_with_priority(
    async { /* 任务代码 */ },
    TaskPriority::Low
);
```

### 任务取消 (Task Cancellation)

```rust
let handle = runtime.spawn(async {
    tokio::time::sleep(Duration::from_secs(10)).await;
});

// 取消任务
handle.cancel()?;
```

### 运行时统计 (Runtime Statistics)

```rust
let stats = runtime.stats();
println!("活跃任务数: {}", stats.active_tasks);
println!("队列任务数: {}", stats.queued_tasks);
println!("已完成任务数: {}", stats.completed_tasks);
println!("工作线程数: {}", stats.worker_threads);
```

## 配置选项 (Configuration Options)

| 选项 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `worker_threads` | `usize` | CPU 核心数 | 工作线程数量 |
| `queue_capacity` | `usize` | 1024 | 每个工作线程的队列容量 |
| `max_stack_size` | `usize` | 2 MB | 协程最大栈大小 |
| `stack_pool_size` | `usize` | 128 | 预分配栈池大小 |
| `poll_interval` | `Duration` | 1 ms | 任务轮询间隔 |
| `enable_work_stealing` | `bool` | `true` | 启用工作窃取 |
| `debug` | `bool` | `false` | 启用调试日志 |

## 任务优先级 (Task Priority)

任务优先级从低到高：

1. **Low** - 低优先级后台任务
2. **Normal** - 普通优先级任务（默认）
3. **High** - 高优先级任务
4. **Critical** - 关键优先级任务

## 任务状态 (Task States)

- **Pending** - 待执行
- **Running** - 执行中
- **Waiting** - 等待（I/O 或事件）
- **Completed** - 已完成
- **Cancelled** - 已取消
- **Failed** - 失败

## 平台支持 (Platform Support)

### Linux
- 使用 epoll 进行事件循环（待完全实现）
- POSIX 线程和计时器

### macOS
- 使用 kqueue 进行事件循环（待完全实现）
- POSIX 线程和计时器

### Windows
- 使用 IOCP 进行事件循环（待完全实现）
- Windows 线程和计时器 API

## 性能考虑 (Performance Considerations)

1. **工作线程数量**: 建议设置为 CPU 核心数或略少
2. **队列容量**: 根据任务数量和内存限制调整
3. **栈大小**: 对于复杂的递归任务可能需要更大的栈
4. **工作窃取**: 在负载不均时可以提高吞吐量

## 示例 (Examples)

完整示例见 [`examples/async_runtime_demo.rs`](../examples/async_runtime_demo.rs)

运行示例：
```bash
cargo run --example async_runtime_demo
```

## 未来计划 (Future Plans)

- [ ] 完整的 epoll/kqueue/IOCP 事件循环实现
- [ ] 更细粒度的任务调度策略
- [ ] 任务依赖图和 DAG 执行
- [ ] 分布式任务执行支持
- [ ] 性能分析和监控工具
- [ ] 更多的异步 I/O 原语

## 故障排除 (Troubleshooting)

### 编译错误

如果遇到 C 代码编译错误，确保：
- 安装了 C 编译器（GCC 或 Clang）
- `cc` crate 在 `build-dependencies` 中

### 运行时错误

- 检查工作线程数量配置
- 确保队列容量足够
- 验证任务没有死锁

## 相关文档 (Related Documentation)

- [运行时环境](runtime.md)
- [内存管理](memory.md)
- [错误处理](errors.md)
