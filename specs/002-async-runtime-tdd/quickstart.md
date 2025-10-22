# Qi 异步运行时快速入门指南
# Qi Async Runtime Quick Start Guide

**创建时间**: 2025-10-22
**功能**: Qi 语言异步运行时和协程支持
**目标开发者**: 使用 Qi 语言进行异步编程的开发者

## 概述 | Overview

Qi 语言的异步运行时支持使用中文关键字进行异步编程，包括协程、异步 I/O 操作和事件循环。本指南将帮助您快速上手 Qi 语言的异步编程功能。

Qi's async runtime supports asynchronous programming using Chinese keywords, including coroutines, async I/O operations, and event loops. This guide will help you quickly get started with async programming in the Qi language.

## 安装要求 | Prerequisites

- Qi 编译器 (支持异步运行时版本)
- Rust 1.75+ (用于编译器开发)
- LLVM 15.0+ (代码生成后端)

## 基础概念 | Basic Concepts

### 异步关键字 | Async Keywords

| 中文关键字 | 英文对应 | 用途 |
|-----------|----------|------|
| `异步` | `async` | 声明异步函数 |
| `等待` | `await` | 等待异步操作完成 |
| `协程` | `coroutine` | 创建协程 |
| `创建` | `create` | 创建异步任务 |
| `挂起` | `suspend` | 暂停协程执行 |
| `恢复` | `resume` | 恢复协程执行 |

## 基础示例 | Basic Examples

### 1. Hello World 异步程序 | Hello World Async Program

```qi
// 异步函数声明 | Async function declaration
异步 函数 你好世界() -> 字符串 {
    等待 延迟(1000)  // 等待1秒 | Wait for 1 second
    返回 "你好，异步世界！"  // 返回中文问候语 | Return Chinese greeting
}

// 主函数 | Main function
函数 主() {
    // 创建异步任务 | Create async task
    任务 = 创建 你好世界()

    // 等待结果 | Wait for result
    结果 = 等待 任务

    // 输出结果 | Print result
    打印(结果)
}
```

### 2. 文件异步读取 | Async File Reading

```qi
// 异步文件读取函数 | Async file reading function
异步 函数 读取文件(路径: 字符串) -> 字符串 {
    打印("开始读取文件: " + 路径)

    // 异步读取文件 | Async file read
    内容 = 等待 文件读取_异步(路径)

    打印("文件读取完成")
    返回 内容
}

// 处理多个文件 | Process multiple files
异步 函数 处理多个文件(文件列表: [字符串]) {
    // 并发创建多个读取任务 | Create multiple concurrent read tasks
    任务列表 = []
    对于 文件列表 中的 文件 {
        任务 = 创建 读取文件(文件)
        任务列表.添加(任务)
    }

    // 等待所有任务完成 | Wait for all tasks to complete
    对于 任务列表 中的 任务 {
        内容 = 等待 任务
        处理文件内容(内容)
    }
}

函数 主() {
    文件列表 = ["数据1.txt", "配置.json", "日志.log"]
    等待 处理多个文件(文件列表)
}
```

### 3. 协程创建和管理 | Coroutine Creation and Management

```qi
// 协程函数 | Coroutine function
协程 生成数字序列(开始: 整数, 结束: 整数) {
    对于 i 从 开始 到 结束 {
        打印("生成数字: " + i)
        挂起  // 暂停协程 | Suspend coroutine
    }
}

// 协程管理器 | Coroutine manager
异步 函数 管理协程() {
    // 创建多个协程 | Create multiple coroutines
    协程1 = 协程 生成数字序列(1, 5)
    协程2 = 协程 生成数字序列(10, 15)

    // 恢复协程执行 | Resume coroutine execution
    恢复 协程1
    恢复 协程2

    // 等待协程完成 | Wait for coroutine completion
    等待 协程1
    等待 协程2
}
```

### 4. 网络异步操作 | Async Network Operations

```qi
// 异步HTTP请求函数 | Async HTTP request function
异步 函数 获取网页内容(网址: 字符串) -> 字符串 {
    打印("开始获取: " + 网址)

    // 异步网络请求 | Async network request
    响应 = 等待 网络请求_异步(网址, "GET")

    打印("请求完成")
    返回 响应.内容
}

// 并发网络请求 | Concurrent network requests
异步 函数 并发获取多个网站() {
    网站列表 = [
        "https://example.com",
        "https://httpbin.org/get",
        "https://jsonplaceholder.typicode.com/posts/1"
    ]

    // 并发创建请求任务 | Create concurrent request tasks
    任务列表 = []
    对于 网站列表 中的 网站 {
        任务 = 创建 获取网页内容(网站)
        任务列表.添加(任务)
    }

    // 收集所有结果 | Collect all results
    结果 = []
    对于 任务列表 中的 任务 {
        内容 = 等待 任务
        结果.添加(内容)
    }

    返回 结果
}
```

### 5. 错误处理 | Error Handling

```qi
// 带错误处理的异步函数 | Async function with error handling
异步 函数 安全读取文件(路径: 字符串) -> 结果<字符串, 错误> {
    尝试 {
        // 尝试异步读取 | Try async read
        内容 = 等待 文件读取_异步(路径)
        返回 成功(内容)
    } 捕获 错误 as e {
        打印("读取文件失败: " + e.消息)
        返回 失败(e)
    }
}

// 错误处理示例 | Error handling example
异步 函数 处理文件错误() {
    结果 = 等待 安全读取文件("不存在的文件.txt")

    匹配 结果 {
        成功(内容) {
            打印("文件内容: " + 内容)
        }
        失败(错误) {
            打印("处理错误: " + 错误.消息)
        }
    }
}
```

## 编译和运行 | Compilation and Execution

### 编译异步程序 | Compiling Async Programs

```bash
# 编译Qi异步程序 | Compile Qi async program
qi 编译 程序.qi --输出 程序 --启用-异步

# 运行编译后的程序 | Run compiled program
./程序
```

### 调试选项 | Debug Options

```bash
# 启用异步调试 | Enable async debugging
qi 编译 程序.qi --输出 程序 --启用-异步 --调试-异步

# 性能分析 | Performance profiling
qi 编译 程序.qi --输出 程序 --启用-异步 --性能分析
```

## 性能优化 | Performance Optimization

### 协程池化 | Coroutine Pooling

```qi
// 使用协程池优化 | Optimize with coroutine pool
异步 函数 批量处理任务(任务列表: [任务]) {
    // 配置协程池 | Configure coroutine pool
    池配置 = 协程池配置 {
        最大协程数: 100,
        栈大小: 64KB,
        队列大小: 1000
    }

    池 = 创建协程池(池配置)

    // 批量提交任务 | Batch submit tasks
    结果 = 等待 池.批量执行(任务列表)

    池.关闭()
    返回 结果
}
```

### 内存管理 | Memory Management

```qi
// 内存优化的协程 | Memory-optimized coroutines
协程 轻量级任务() {
    // 使用小栈 | Use small stack
    设置栈大小(8KB)

    // 避免大对象分配 | Avoid large object allocations
    小数据 = 处理小块数据()

    返回 小数据
}
```

## 最佳实践 | Best Practices

### 1. 异步函数设计 | Async Function Design

- 异步函数应该包含至少一个 `等待` 操作
- 避免在异步函数中进行长时间同步操作
- 使用合适的超时设置避免无限等待

```qi
// 好的设计 | Good design
异步 函数 获取数据(标识符: 字符串) -> 数据 {
    // 设置超时 | Set timeout
    结果 = 等待 带超时(数据库查询(标识符), 5000)
    返回 结果
}

// 避免的设计 | Design to avoid
异步 函数 混合操作() {
    // 避免：长时间同步操作 | Avoid: long synchronous operation
    同步_处理(10000)  // 这会阻塞整个运行时
}
```

### 2. 错误处理 | Error Handling

- 始终处理异步操作中的错误
- 提供有意义的中文错误消息
- 使用适当的错误恢复策略

```qi
异步 函数 健壮的网络操作() {
    尝试 {
        响应 = 等待 网络请求_异步(网址, 超时: 10000)
        处理响应(响应)
    } 捕获 网络错误 as e {
        记录错误("网络请求失败: " + e.消息)
        重试或使用缓存()
    } 捕获 超时错误 as e {
        记录错误("请求超时: " + e.消息)
        使用默认值()
    }
}
```

### 3. 资源管理 | Resource Management

- 确保文件和网络连接正确关闭
- 使用RAII模式管理资源
- 避免资源泄漏

```qi
异步 函数 安全文件操作() {
    文件句柄 = 等待 文件打开_异步("数据.txt", "读取")
    延迟关闭(文件句柄)  // 确保文件关闭 | Ensure file closure

    尝试 {
        内容 = 等待 文件读取_异步(文件句柄)
        处理内容(内容)
    } 终于 {
        等待 文件关闭_异步(文件句柄)
    }
}
```

## 故障排除 | Troubleshooting

### 常见问题 | Common Issues

1. **协程创建失败 | Coroutine Creation Failed**
   - 检查内存使用情况
   - 验证协程栈大小设置
   - 确认运行时配置正确

2. **异步操作超时 | Async Operation Timeout**
   - 增加超时时间设置
   - 检查网络连接状态
   - 验证I/O操作的正确性

3. **内存泄漏 | Memory Leaks**
   - 确保所有异步操作正确完成
   - 检查协程是否正确清理
   - 使用内存分析工具监控

### 调试工具 | Debugging Tools

```bash
# 异步运行时状态 | Async runtime status
qi 调试 --异步-状态 程序

# 协程跟踪 | Coroutine tracing
qi 调试 --协程跟踪 程序

# 性能分析 | Performance profiling
qi 分析 --异步 程序
```

## 更多资源 | Additional Resources

- [Qi 语言异步编程完整指南](docs/async-programming-guide.md)
- [协程最佳实践](docs/coroutine-best-practices.md)
- [性能优化指南](docs/performance-optimization.md)
- [API参考文档](docs/async-api-reference.md)

## 社区支持 | Community Support

- Qi 语言官方论坛: [forum.qi-lang.org]
- GitHub 问题反馈: [github.com/qi-lang/qi/issues]
- 中文交流群: [QQ群 123456789]

---

**注意**: 本指南基于 Qi 语言异步运行时的当前版本。功能可能随版本更新而变化。

**Note**: This guide is based on the current version of Qi's async runtime. Features may change with version updates.