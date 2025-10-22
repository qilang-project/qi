# Qi 基础运行时快速入门指南
# Qi Basic Runtime Quick Start Guide

**创建时间**: 2025-10-22
**功能**: Qi 语言基础运行时和同步I/O支持
**目标开发者**: 使用 Qi 语言进行程序开发的开发者

## 概述 | Overview

Qi 语言的基础运行时为编译后的Qi程序提供执行环境，包括内存管理、同步I/O操作、标准库函数和完整的中文语言支持。本指南将帮助您快速上手Qi语言的基础运行时功能。

The Qi language basic runtime provides an execution environment for compiled Qi programs, including memory management, synchronous I/O operations, standard library functions, and comprehensive Chinese language support. This guide will help you quickly get started with Qi's basic runtime features.

## 安装要求 | Prerequisites

- Qi 编译器 (支持基础运行时版本)
- Rust 1.75+ (运行时实现语言)
- LLVM 15.0+ (代码生成后端)
- 支持的操作系统: Linux, Windows, macOS

## 基础概念 | Basic Concepts

### 运行时组件 | Runtime Components

| 组件 | 功能 | 用途 |
|------|------|------|
| Runtime Environment | 程序执行环境 | 管理程序生命周期 |
| Memory Manager | 内存管理 | 自动内存分配和回收 |
| File System Interface | 文件系统接口 | 文件读写操作 |
| Network Manager | 网络管理器 | HTTP/TCP连接 |
| Standard Library | 标准库 | 字符串、数学、系统函数 |
| Error Handler | 错误处理器 | 中文错误消息 |

## 基础示例 | Basic Examples

### 1. Hello World 程序 | Hello World Program

```qi
// 基础程序结构 | Basic program structure
函数 主() {
    // 输出中文字符串 | Output Chinese string
    打印("你好，世界！")

    // 程序正常结束 | Normal program termination
    返回 0
}
```

**编译和运行 | Compile and Run**:
```bash
# 编译Qi程序 | Compile Qi program
qi 编译 你好世界.qi --output 你好世界

# 运行编译后的程序 | Run compiled program
./你好世界
# 输出: 你好，世界！
```

### 2. 文件操作示例 | File Operations Example

```qi
// 文件读取示例 | File reading example
函数 读取文件(路径: 字符串) -> 字符串 {
    // 尝试读取文件 | Try to read file
    文件句柄 = 打开文件(路径, "读取")

    如果 文件句柄 != 空 {
        内容 = 读取文件内容(文件句柄)
        关闭文件(文件句柄)
        返回 内容
    } 否则 {
        打印("无法读取文件: " + 路径)
        返回 ""
    }
}

// 文件写入示例 | File writing example
函数 写入文件(路径: 字符串, 内容: 字符串) -> 布尔 {
    文件句柄 = 创建文件(路径, "写入")

    如果 文件句柄 != 空 {
        写入文件内容(文件句柄, 内容)
        关闭文件(文件句柄)
        返回 真
    } 否则 {
        打印("无法创建文件: " + 路径)
        返回 假
    }
}

// 主函数 | Main function
函数 主() {
    // 写入测试文件 | Write test file
    文件名 = "测试.txt"
    写入文件(文件名, "这是一个测试文件，包含中文内容。")

    // 读取文件内容 | Read file content
    内容 = 读取文件(文件名)
    打印("文件内容: " + 内容)

    // 删除测试文件 | Delete test file
    删除文件(文件名)
}
```

### 3. 网络操作示例 | Network Operations Example

```qi
// HTTP请求示例 | HTTP request example
函数 获取网页内容(网址: 字符串) -> 字符串 {
    // 创建HTTP请求 | Create HTTP request
    请求 = 创建HTTP请求("GET", 网址)

    // 设置超时时间 | Set timeout
    设置超时(请求, 30000)  // 30秒超时

    // 发送请求 | Send request
    响应 = 发送HTTP请求(请求)

    如果 响应.状态码 == 200 {
        返回 响应.内容
    } 否则 {
        打印("请求失败，状态码: " + 响应.状态码)
        返回 ""
    }
}

// TCP连接示例 | TCP connection example
函数 连接服务器(地址: 字符串, 端口: 整数) -> 布尔 {
    // 创建TCP连接 | Create TCP connection
    连接 = 创建TCP连接(地址, 端口)

    // 设置连接超时 | Set connection timeout
    设置超时(连接, 10000)  // 10秒超时

    // 建立连接 | Establish connection
    结果 = 连接到服务器(连接)

    如果 结果.成功 {
        打印("成功连接到服务器")
        关闭连接(连接)
        返回 真
    } else {
        打印("连接失败: " + 结果.错误消息)
        返回 假
    }
}

// 主函数 | Main function
函数 主() {
    // 测试HTTP请求 | Test HTTP request
    内容 = 获取网页内容("https://httpbin.org/get")
    如果 内容 != "" {
        打印("获取的内容长度: " + 字符串长度(内容))
    }

    // 测试TCP连接 | Test TCP connection
    连接服务器("example.com", 80)
}
```

### 4. 内存管理示例 | Memory Management Example

```qi
// 内存分配示例 | Memory allocation example
函数 处理大量数据() {
    // 创建大数据数组 | Create large data array
    数据数组 = 创建数组(10000)  // 创建1万个元素

    // 填充数据 | Fill data
    对于 i 从 0 到 数组长度(数据数组) - 1 {
        设置数组元素(数据数组, i, i * 2)
    }

    // 处理数据 | Process data
    总和 = 0
    对于 i 从 0 到 数组长度(数据数组) - 1 {
        总和 = 总和 + 获取数组元素(数据数组, i)
    }

    打印("数组总和: " + 总和)
    // 数组会自动回收 | Array will be automatically garbage collected
}

// 字符串操作示例 | String operations example
函数 处理中文文本() {
    文本 = "你好，Qi编程语言！欢迎使用"

    // 字符串操作 | String operations
    长度 = 字符串长度(文本)
    打印("文本长度: " + 长度)

    // 子字符串 | Substring
    子串 = 提取子串(文本, 0, 6)
    打印("前6个字符: " + 子串)

    // 字符串连接 | String concatenation
    拼接结果 = 连接字符串("Qi语言", "很棒")
    打印("连接结果: " + 拼接结果)

    // 字符串查找 | String search
    位置 = 查找子串(文本, "编程")
    如果 位置 >= 0 {
        打印("找到'编程'在位置: " + 位置)
    }
}

// 主函数 | Main function
函数 主() {
    处理大量数据()
    处理中文文本()
}
```

### 5. 数学运算示例 | Mathematical Operations Example

```qi
// 数学函数示例 | Mathematical function examples
函数 计算圆周率近似值(项数: 整数) -> 浮点数 {
    // 莱布尼茨级数计算π | Leibniz series for π
    π = 0.0
    符号 = 1.0

    对于 n 从 0 到 项数 - 1 {
        项 = 4.0 / (2 * n + 1) * 符号
        π = π + 项
        符号 = -符号  // 交替符号
    }

    返回 π
}

函数 斐波那契数列(项数: 整数) -> 整数 {
    如果 项数 <= 1 {
        返回 项数
    }

    // 递归计算斐波那契数列 | Recursive Fibonacci calculation
    前一项 = 斐波那契数列(项数 - 1)
    前两项 = 斐波那契数列(项数 - 2)

    返回 前一项 + 前两项
}

// 主函数 | Main function
函数 主() {
    // 计算π近似值 | Calculate π approximation
    π近似 = 计算圆周率近似值(100000)
    打印("π近似值(10万项): " + π近似)

    // 计算斐波那契数列 | Calculate Fibonacci sequence
    第10项 = 斐波那契数列(10)
    打印("斐波那契数列第10项: " + 第10项)

    // 数学运算示例 | Mathematical operation examples
    结果 = 幂运算(2, 8)  // 2^8
    打印("2的8次方: " + 结果)

    根 = 平方根(16)  // √16
    打印("16的平方根: " + 根)
}
```

## 编译和运行 | Compilation and Execution

### 基本编译命令 | Basic Compilation Commands

```bash
# 编译单个文件 | Compile single file
qi 编译 程序.qi --output 程序

# 启用调试模式 | Enable debug mode
qi 编译 程序.qi --output 程序 --debug

# 显示编译详细信息 | Show compilation details
qi 编译 程序.qi --output 程序 --verbose

# 指定运行时配置 | Specify runtime configuration
qi 编译 程序.qi --output 程序 --runtime-config "{ \"max_memory_mb\": 512, \"gc_threshold_percent\": 0.7 }"
```

### 运行选项 | Runtime Options

```bash
# 运行程序 | Run program
./程序 [参数1] [参数2] ...

# 显示运行时指标 | Show runtime metrics
./程序 --metrics

# 启用内存分析 | Enable memory profiling
./程序 --profile-memory

# 显示帮助信息 | Show help information
./程序 --help
```

## 错误处理和调试 | Error Handling and Debugging

### 常见错误类型 | Common Error Types

1. **内存不足错误 | Out of Memory Error**
```
错误: 内存不足，无法分配 1024 字节
Error: Out of memory, cannot allocate 1024 bytes
解决方法: 优化内存使用或增加系统内存
```

2. **文件访问错误 | File Access Error**
```
错误: 无法打开文件 "data.txt" (权限被拒绝)
Error: Cannot open file "data.txt" (Permission denied)
解决方法: 检查文件权限或使用不同文件名
```

3. **网络连接错误 | Network Connection Error**
```
错误: 连接到 "example.com:80" 超时
Error: Connection to "example.com:80" timeout
解决方法: 检查网络连接或增加超时时间
```

4. **语法错误 | Syntax Error
```
错误: 第5行：语法错误，缺少分号
Error: Line 5: Syntax error, missing semicolon
解决方法: 检查语法并重新编译
```

### 调试工具 | Debugging Tools

```bash
# 运行时调试 | Runtime debugging
qi 运行 程序.qi --debug --breakpoints main,process_data

# 内存泄漏检测 | Memory leak detection
qi 运行 程序.qi --profile-memory --leak-detection

# 性能分析 | Performance profiling
qi 运行 程序.qi --profile --output-profile profile.json

# 错误追踪 | Error tracing
qi 运行 程序.qi --error-stack-trace --verbose
```

## 性能优化 | Performance Optimization

### 内存优化 | Memory Optimization

```qi
// 使用对象池减少分配 | Use object pools to reduce allocation
函数 高效处理数据() {
    // 重用缓冲区而不是每次分配 | Reuse buffers instead of allocating each time
    缓冲池 = 获取缓冲池(1024)

    对于 i 从 0 到 10000 {
        缓冲区 = 从池获取(缓冲池)
        处理数据(缓冲区)
        归还到池(缓冲池, 缓冲区)
    }

    释放缓冲池(缓冲池)
}

// 避免不必要的内存分配 | Avoid unnecessary memory allocations
函数 高效字符串操作() {
    // 使用字符串构建器而不是连接 | Use string builder instead of concatenation
    构建器 = 创建字符串构建器()

    添加到构建器(构建器, "结果: ")
    添加到构建器(构建器, 计算结果)

    结果 = 转换为字符串(构建器)
    打印(结果)
}
```

### I/O优化 | I/O Optimization

```qi
// 使用缓冲区提高I/O性能 | Use buffers to improve I/O performance
函数 高效文件读取(文件名: 字符串) -> 字符串 {
    // 使用大缓冲区 | Use large buffer
    缓冲区大小 = 65536  // 64KB
    缓冲区 = 创建缓冲区(缓冲区大小)

    文件句柄 = 打开文件(文件名, "读取")
    如果 文件句柄 != 空 {
        总长度 = 获取文件大小(文件句柄)
        已读取 = 0

        当 已读取 < 总长度 {
            读取长度 = 读取文件到缓冲区(文件句柄, 缓冲区)
            处理缓冲数据(缓冲区, 读取长度)
            已读取 = 已读取 + 读取长度
        }

        关闭文件(文件句柄)
    }

    返回 缓冲区内容
}
```

## 中文语言支持 | Chinese Language Support

### UTF-8编码处理 | UTF-8 Encoding

```qi
// Unicode字符串操作 | Unicode string operations
函数 处理中文文本(文本: 字符串) -> 字符串 {
    // 确保UTF-8编码 | Ensure UTF-8 encoding
    如果 不是UTF8编码(文本) {
        打印("警告：输入文本不是有效的UTF-8编码")
        返回 ""
    }

    // 中文字符计数 | Count Chinese characters
    中文字符数 = 计算中文字符(文本)
    打印("中文字符数量: " + 中文字符数)

    // 按字符宽度处理 | Handle by character width
    格式化文本 = 格式化中文文本(文本, 20)  // 20字符宽度

    返回 格式化文本
}

// 中文排序 | Chinese sorting
函数 中文排序(文本列表: [字符串]) -> [字符串] {
    // 按拼音排序 | Sort by Pinyin
    按拼音排序结果 = 按拼音排序(文本列表)

    返回 按拼音排序结果
}
```

## 测试和验证 | Testing and Validation

### 基础测试框架 | Basic Testing Framework

```qi
// 测试程序验证基础功能 | Test program to verify basic functionality
函数 测试HelloWorld() -> 布尔 {
    // 测试Hello World程序 | Test Hello World program
    结果 = 运行程序("tests/hello_world.qi")

    如果 结果.输出 == "你好，世界！" 并且 结果.退出码 == 0 {
        打印("✅ Hello World测试通过")
        返回 真
    } else {
        打印("❌ Hello World测试失败")
        返回 假
    }
}

函数 测试文件操作() -> 布尔 {
    // 测试文件读写 | Test file read/write
    测试文件 = "测试_io.txt"
    测试内容 = "这是测试内容，包含中文：你好世界！"

    // 写入测试 | Write test
    写入结果 = 写入文件(测试文件, 测试内容)
    如果 写入结果 == 假 {
        打印("❌ 文件写入测试失败")
        返回 假
    }

    // 读取测试 | Read test
    读取内容 = 读取文件(测试文件)
    如果 读取内容 == 测试内容 {
        打印("✅ 文件读写测试通过")

        // 清理测试文件 | Clean up test file
        删除文件(测试文件)
        返回 真
    } else {
        打印("❌ 文件读取测试失败")
        返回 假
    }
}

// 主测试函数 | Main test function
函数 主() {
    总测试数 = 2
    通过测试数 = 0

    如果 测试HelloWorld() {
        通过测试数 = 通过测试数 + 1
    }

    如果 测试文件操作() {
        通过测试数 = 通过测试数 + 1
    }

    打印("测试完成: " + 通过测试数 + "/" + 总测试数 + " 通过")

    如果 通过测试数 == 总测试数 {
        返回 0  // 所有测试通过
    } else {
        返回 1  // 有测试失败
    }
}
```

## 故障排除 | Troubleshooting

### 常见问题及解决方案 | Common Issues and Solutions

1. **程序启动失败 | Program Startup Failure**
   - 检查Qi编译器版本兼容性
   - 验证运行时环境配置
   - 查看系统日志获取详细错误信息

2. **内存使用过高 | High Memory Usage**
   - 检查内存泄漏
   - 优化数据结构
   - 使用内存分析工具

3. **I/O操作缓慢 | Slow I/O Operations**
   - 增加I/O缓冲区大小
   - 使用批量操作
   - 检查磁盘性能

4. **中文显示异常 | Chinese Display Issues**
   - 确认终端支持UTF-8
   - 检查文件编码格式
   - 验证系统语言设置

## 更多资源 | Additional Resources

- [Qi语言运行时完整文档](docs/runtime-reference.md)
- [性能优化指南](docs/performance-optimization.md)
- [错误处理参考](docs/error-handling-reference.md)
- [API参考文档](docs/runtime-api-reference.md)
- [开发者指南](docs/developer-guide.md)

## 社区支持 | Community Support

- Qi语言官方论坛: [forum.qi-lang.org]
- GitHub问题反馈: [github.com/qi-lang/qi/issues]
- 中文交流群: [QQ群 123456789]

---

**注意**: 本指南基于Qi语言基础运行时的当前版本。功能可能随版本更新而变化。

**Note**: This guide is based on the current version of Qi's basic runtime. Features may change with version updates.