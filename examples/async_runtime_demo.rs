//! Example demonstrating the Qi async runtime
//!
//! This example shows how to use the async runtime for concurrent task execution.

use qi_compiler::runtime::{AsyncRuntime, AsyncRuntimeConfig, AsyncRuntimeStats};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Qi 异步运行时示例 ===\n");

    // Create a runtime configuration
    let config = AsyncRuntimeConfig {
        worker_threads: 4,
        queue_capacity: 1024,
        max_stack_size: 2 * 1024 * 1024,
        stack_pool_size: 128,
        poll_interval: Duration::from_millis(1),
        enable_work_stealing: true,
        debug: false,
    };

    println!("配置:");
    println!("  工作线程数: {}", config.worker_threads);
    println!("  队列容量: {}", config.queue_capacity);
    println!("  最大栈大小: {} MB", config.max_stack_size / (1024 * 1024));
    println!("  启用工作窃取: {}\n", config.enable_work_stealing);

    // Create the runtime
    let runtime = AsyncRuntime::new(config)?;
    println!("异步运行时创建成功！\n");

    // Spawn some tasks
    println!("生成异步任务...");
    
    let handle1 = runtime.spawn(async {
        println!("  任务 1: 开始执行");
        tokio::time::sleep(Duration::from_millis(100)).await;
        println!("  任务 1: 执行完成");
    });

    let handle2 = runtime.spawn(async {
        println!("  任务 2: 开始执行");
        tokio::time::sleep(Duration::from_millis(200)).await;
        println!("  任务 2: 执行完成");
    });

    let handle3 = runtime.spawn(async {
        println!("  任务 3: 开始执行");
        tokio::time::sleep(Duration::from_millis(150)).await;
        println!("  任务 3: 执行完成");
    });

    // Wait for all tasks to complete
    println!("\n等待所有任务完成...\n");
    handle1.join().await?;
    handle2.join().await?;
    handle3.join().await?;

    // Print statistics
    let stats: AsyncRuntimeStats = runtime.stats();
    println!("\n运行时统计:");
    println!("  活跃任务数: {}", stats.active_tasks);
    println!("  队列任务数: {}", stats.queued_tasks);
    println!("  已完成任务数: {}", stats.completed_tasks);
    println!("  工作线程数: {}", stats.worker_threads);

    // Demonstrate priority-based scheduling
    println!("\n\n=== 优先级调度示例 ===\n");

    use qi_compiler::runtime::async_runtime::TaskPriority;

    let high_priority = runtime.spawn_with_priority(async {
        println!("  高优先级任务: 执行中");
        tokio::time::sleep(Duration::from_millis(50)).await;
        println!("  高优先级任务: 完成");
    }, TaskPriority::High);

    let normal_priority = runtime.spawn_with_priority(async {
        println!("  普通优先级任务: 执行中");
        tokio::time::sleep(Duration::from_millis(50)).await;
        println!("  普通优先级任务: 完成");
    }, TaskPriority::Normal);

    let low_priority = runtime.spawn_with_priority(async {
        println!("  低优先级任务: 执行中");
        tokio::time::sleep(Duration::from_millis(50)).await;
        println!("  低优先级任务: 完成");
    }, TaskPriority::Low);

    high_priority.join().await?;
    normal_priority.join().await?;
    low_priority.join().await?;

    // Demonstrate task cancellation
    println!("\n\n=== 任务取消示例 ===\n");

    let cancellable = runtime.spawn(async {
        println!("  可取消任务: 开始执行");
        tokio::time::sleep(Duration::from_secs(10)).await;
        println!("  可取消任务: 完成（不会执行到这里）");
    });

    tokio::time::sleep(Duration::from_millis(100)).await;
    println!("  取消任务...");
    cancellable.cancel()?;
    println!("  任务已取消");

    // Graceful shutdown
    println!("\n\n关闭运行时...");
    runtime.shutdown()?;
    println!("运行时已成功关闭！");

    Ok(())
}
