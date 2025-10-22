# Qi 调试指南

## 概述

`qi debug` 命令是 Qi 语言编译器提供的调试工具，用于编译并以调试模式运行 Qi 程序。该命令会自动启用调试符号、禁用优化，并提供多种调试选项来帮助你分析程序行为。

## 基本用法

```bash
qi debug <文件路径> [运行参数...] [调试选项]
```

### 示例

```bash
# 基本调试运行
qi debug examples/basic/greet.qi

# 带详细输出
qi debug examples/basic/greet.qi --verbose

# 启用所有调试选项
qi debug examples/basic/simple_runtime_test.qi --verbose --memory --profile --stack-trace
```

## 调试选项详解

### 1. `--verbose` / `-v` - 详细输出

**作用：** 显示详细的编译和调试过程信息

**输出内容：**

- 🔍 源代码分析过程
- ✓ 语法解析成功/失败状态
- 📊 解析统计信息（语句数量等）
- 🛠️ 编译进度和耗时
- 🔧 调试符号状态
- ⚡ 优化级别信息

**使用场景：**

- 想了解编译过程的每一步
- 调试编译器本身的问题
- 学习 Qi 程序的编译流程

**示例输出：**

```
🔍 正在分析源代码...
  ✓ 语法解析成功
  📊 解析统计:
    - 语句数量: 7
🛠️  正在编译调试版本...
  ✓ 编译完成，耗时: 73ms
  🔧 调试符号: 已嵌入
  ⚡ 优化级别: 无
```

### 2. `--memory` - 内存监控

**作用：** 启用运行时内存监控功能

**工作原理：**

- 设置环境变量 `QI_DEBUG_MEMORY=1`
- Runtime 会在执行时监控内存分配和释放
- 可以检测内存泄漏、双重释放等问题

**使用场景：**

- 怀疑程序有内存泄漏
- 调试内存相关的崩溃
- 优化程序内存使用

**如何查看内存信息：**
目前这个功能设置了环境变量，Runtime 需要实现具体的监控逻辑。你可以：

1. 在 Runtime C 代码中检查这个环境变量：

```c
// runtime/src/memory.c
void* qi_runtime_alloc(size_t size) {
    if (getenv("QI_DEBUG_MEMORY")) {
        fprintf(stderr, "[MEM] 分配 %zu 字节\n", size);
    }
    // ... 分配内存
}
```

2. 使用系统工具监控：

```bash
# macOS 使用 leaks
leaks --atExit -- ./program_debug.exec

# 使用 valgrind（Linux）
valgrind --leak-check=full ./program_debug.exec
```

### 3. `--profile` - 性能分析

**作用：** 启用性能分析功能

**工作原理：**

- 设置环境变量 `QI_DEBUG_PROFILE=1`
- Runtime 可以记录函数调用次数、执行时间等

**使用场景：**

- 找出性能瓶颈
- 优化程序性能
- 分析函数调用频率

**如何实现性能分析：**

1. 在 Runtime 中添加性能计数：

```c
// runtime/src/profiling.c
typedef struct {
    const char* function_name;
    uint64_t call_count;
    uint64_t total_time_ns;
} ProfileEntry;

void qi_profile_start(const char* name) {
    if (!getenv("QI_DEBUG_PROFILE")) return;
    // 记录开始时间
}

void qi_profile_end(const char* name) {
    if (!getenv("QI_DEBUG_PROFILE")) return;
    // 记录结束时间并累加
}
```

2. 或使用系统工具：

```bash
# macOS 使用 Instruments
instruments -t "Time Profiler" ./program_debug.exec

# Linux 使用 perf
perf record ./program_debug.exec
perf report
```

### 4. `--stack-trace` - 堆栈跟踪

**作用：** 启用详细的堆栈跟踪功能

**工作原理：**

- 设置环境变量 `QI_DEBUG_STACK=1`
- Runtime 在错误发生时打印完整调用栈
- 结合调试符号可以定位错误源头

**使用场景：**

- 程序崩溃时定位问题
- 追踪错误传播路径
- 调试复杂的函数调用链

**如何实现堆栈跟踪：**

在 Runtime 错误处理中添加：

```c
// runtime/src/errors.c
void qi_runtime_error(const char* message) {
    fprintf(stderr, "错误: %s\n", message);

    if (getenv("QI_DEBUG_STACK")) {
        // 使用 backtrace (Linux/macOS)
        void* buffer[100];
        int nptrs = backtrace(buffer, 100);
        char** strings = backtrace_symbols(buffer, nptrs);

        fprintf(stderr, "堆栈跟踪:\n");
        for (int i = 0; i < nptrs; i++) {
            fprintf(stderr, "  %s\n", strings[i]);
        }
        free(strings);
    }
}
```

## 调试工作流程

`qi debug` 命令执行的步骤：

1. **📁 加载源文件** - 读取并验证源代码文件
2. **🔍 语法分析** - 解析 AST 并显示统计信息
3. **🛠️ 编译调试版本** - 启用调试符号，禁用优化（OptimizationLevel::None）
4. **🎯 设置调试环境** - 根据选项设置环境变量
5. **🚀 运行程序** - 执行编译后的程序
6. **✅ 输出结果** - 显示程序输出和调试信息

## 实用技巧

### 1. 组合使用多个选项

```bash
# 全面调试，查看所有信息
qi debug program.qi --verbose --memory --profile --stack-trace
```

### 2. 查看 AST 结构

使用 `--verbose` 选项可以看到程序的 AST 解析结果：

```bash
qi debug examples/basic/greet.qi --verbose
```

会显示类似：

```
Parsed with LALRPOP: Program {
    package_name: None,
    imports: [],
    statements: [函数声明(...)]
}
```

### 3. 对比调试版和发布版

```bash
# 调试版（无优化）
qi debug program.qi

# 发布版（优化）
qi run program.qi
```

### 4. 使用外部调试器

调试版本包含调试符号，可以用 LLDB/GDB 调试：

```bash
# 先编译调试版
qi debug program.qi --verbose  # 会生成 program_debug.exec

# 使用 LLDB (macOS)
lldb ./program_debug.exec

# 使用 GDB (Linux)
gdb ./program_debug.exec
```

## 调试输出格式说明

### 启动信息

```
🐛 调试模式启动
📁 源文件: "examples/basic/simple_runtime_test.qi"
⚙️  调试选项:
  • 详细输出: 开启
  • 内存监控: 开启
  • 性能分析: 开启
  • 堆栈跟踪: 开启
```

### 程序输出

```
──────────────────────────────────────────────────
=== Qi Runtime 测试 ===
42
3.14
你好，Qi Runtime！
=== 测试完成 ===
──────────────────────────────────────────────────
✅ 调试运行完成
```

程序的实际输出会在分隔线之间显示。

## 环境变量列表

`qi debug` 设置的环境变量：

| 环境变量           | 触发选项        | 用途         |
| ------------------ | --------------- | ------------ |
| `QI_DEBUG_MEMORY`  | `--memory`      | 启用内存监控 |
| `QI_DEBUG_PROFILE` | `--profile`     | 启用性能分析 |
| `QI_DEBUG_STACK`   | `--stack-trace` | 启用堆栈跟踪 |

Runtime C 代码可以通过 `getenv()` 读取这些变量来启用相应功能。

## 下一步：完善 Runtime 调试功能

当前 `qi debug` 命令已经设置好了调试环境，但 Runtime 还需要实现具体的监控逻辑：

### 1. 添加内存监控

在 `runtime/src/memory.c` 中：

```c
#include <stdlib.h>
#include <stdio.h>

static size_t total_allocated = 0;
static size_t total_freed = 0;
static int memory_tracking = 0;

void qi_memory_init() {
    memory_tracking = getenv("QI_DEBUG_MEMORY") != NULL;
}

void* qi_alloc(size_t size) {
    void* ptr = malloc(size);
    if (memory_tracking) {
        total_allocated += size;
        fprintf(stderr, "[MEM] +%zu bytes (total: %zu)\n",
                size, total_allocated - total_freed);
    }
    return ptr;
}

void qi_free(void* ptr) {
    if (memory_tracking) {
        // 需要记录分配大小
        fprintf(stderr, "[MEM] freed pointer\n");
    }
    free(ptr);
}
```

### 2. 添加性能分析

创建 `runtime/src/profiling.c`

### 3. 添加堆栈跟踪

在 `runtime/src/errors.c` 中使用 `backtrace()`

## 常见问题

### Q: 为什么调试版运行较慢？

A: 调试版禁用了所有优化（`OptimizationLevel::None`），并包含调试符号，这会让程序更大更慢，但更容易调试。

### Q: 如何保存调试输出？

A: 使用重定向：

```bash
qi debug program.qi --verbose 2>&1 | tee debug.log
```

### Q: 环境变量设置了但没效果？

A: 需要在 Runtime C 代码中实现对应的监控逻辑。当前只是设置了变量，Runtime 需要读取并实现功能。

### Q: 可以调试已编译的程序吗？

A: 可以，先找到生成的 `*_debug.exec` 文件，然后用 LLDB/GDB 调试：

```bash
lldb ./program_debug.exec
```

## 相关文档

- [Qi 语言设计文档](QI_LANGUAGE_DESIGN.md)
- [运行时集成文档](RUNTIME_INTEGRATION.md)
- [调试技巧](debugging.md)

---

**提示：** 这些调试功能是帮助你开发和调试 Qi 程序的强大工具。根据具体需求选择合适的选项，可以大大提高调试效率！
