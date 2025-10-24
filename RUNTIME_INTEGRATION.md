# Qi 异步运行时集成指南

本文档说明 Qi 编译器的完整实现，包括 lexer、parser、AST、codegen 和异步运行时的集成。

## 编译器架构

### 1. Lexer (词法分析器)

位置：`src/lexer/mod.rs`

**特性：**
- 完整支持中文关键字
- Unicode 字符处理
- 行号和列号跟踪
- 错误诊断

**支持的 Token 类型：**
- 中文关键字：如果、否则、循环、当、对于、函数、返回、变量、常量等
- 运算符：加、减、乘、除、取余、等于、不等于、大于、小于等
- 字面量：整数、浮点数、字符串、字符、布尔值
- 标识符：支持中文和英文混合命名

**示例：**
```rust
use qi_compiler::lexer::Lexer;

let source = "变量 x = 42;".to_string();
let mut lexer = Lexer::new(source);
let tokens = lexer.tokenize().expect("词法分析成功");
```

### 2. Parser (语法分析器)

位置：`src/parser/mod.rs`, `src/parser/grammar.lalrpop`

**特性：**
- 使用 LALRPOP 生成的解析器
- 完整的中文语法支持
- 错误恢复机制
- 支持注释处理

**支持的语法结构：**
- 变量声明：`变量 x = 42;`
- 函数声明：`函数 测试() { 返回 0; }`
- 控制流：
  - if 语句：`如果 x > 5 { ... }`
  - while 循环：`当 x > 0 { ... }`
  - for 循环：`对于 i 在 范围 { ... }`
- 表达式：二元运算、函数调用、数组访问等

**示例：**
```rust
use qi_compiler::parser::Parser;

let parser = Parser::new();
let program = parser.parse_source("变量 x = 42;").expect("解析成功");
```

### 3. AST (抽象语法树)

位置：`src/parser/ast.rs`

**定义的节点类型：**
- `Program`: 程序根节点
- `VariableDeclaration`: 变量声明
- `FunctionDeclaration`: 函数声明
- `IfStatement`: 条件语句
- `WhileStatement`: 循环语句
- `BinaryExpression`: 二元表达式
- `FunctionCallExpression`: 函数调用
- 等等...

**示例：**
```rust
use qi_compiler::parser::ast::*;

match ast_node {
    AstNode::变量声明(decl) => {
        println!("变量名: {}", decl.name);
        println!("初始值: {:?}", decl.initializer);
    }
    _ => {}
}
```

### 4. Code Generator (代码生成器)

位置：`src/codegen/mod.rs`, `src/codegen/builder.rs`

**特性：**
- 生成 LLVM IR 代码
- 支持多种优化级别
- 跨平台目标支持（Linux、Windows、macOS、WebAssembly）

**支持的代码生成：**
- 变量声明和赋值
- 函数定义和调用
- 控制流语句
- 算术和逻辑运算

**示例：**
```rust
use qi_compiler::codegen::CodeGenerator;
use qi_compiler::config::CompilationTarget;

let mut codegen = CodeGenerator::new(CompilationTarget::Linux);
let ir = codegen.generate(&ast_node).expect("代码生成成功");
```

### 5. 异步运行时

位置：`src/runtime/async_runtime/`

**组件：**
- **Executor**: 任务执行器，使用 tokio 运行时
- **Scheduler**: 协程调度器，支持任务优先级
- **Task**: 任务抽象，支持取消和等待
- **Pool**: 工作线程池，支持工作窃取
- **Queue**: 任务队列，无锁并发实现

**特性：**
- 多线程工作窃取调度
- 任务优先级支持（Low、Normal、High、Critical）
- 协程栈池管理
- 跨平台 I/O 事件循环支持

**示例：**
```rust
use qi_compiler::runtime::{AsyncRuntime, AsyncRuntimeConfig};

let runtime = AsyncRuntime::new(AsyncRuntimeConfig::default()).unwrap();

let handle = runtime.spawn(async {
    println!("异步任务执行中");
});

handle.join().await.expect("任务完成");
```

### 6. 基础运行时

位置：`src/runtime/`

**模块：**
- **Memory Manager**: 内存分配和垃圾回收
- **I/O Interface**: 文件系统和网络操作
- **Standard Library**: 标准库函数（字符串、数学、系统调用等）
- **Error Handler**: 错误处理和中文错误消息
- **Debug System**: 调试支持和性能监控

**示例：**
```rust
use qi_compiler::runtime::{RuntimeEnvironment, RuntimeConfig};

let config = RuntimeConfig::default();
let mut runtime = RuntimeEnvironment::new(config).unwrap();
runtime.initialize().unwrap();
```

## 编译和运行 Qi 程序

### 使用 CLI

**编译程序：**
```bash
# 基本编译
cargo run -- compile examples/hello.qi

# 指定目标平台
cargo run -- compile --target linux examples/hello.qi
cargo run -- compile --target macos examples/hello.qi

# 启用优化
cargo run -- compile -O standard examples/hello.qi
```

**运行程序：**
```bash
# 编译并运行
cargo run -- run examples/hello.qi

# 使用参数运行
cargo run -- run examples/hello.qi arg1 arg2

# 调试运行
cargo run -- debug examples/hello.qi --verbose --memory --profile
```

**语法检查：**
```bash
# 仅检查语法
cargo run -- check examples/hello.qi

# 检查并运行
cargo run -- check-run examples/hello.qi
```

### 使用 API

**完整编译流程：**
```rust
use qi_compiler::QiCompiler;
use std::path::PathBuf;

let compiler = QiCompiler::new();
let result = compiler.compile(PathBuf::from("examples/hello.qi")).unwrap();

println!("编译完成，耗时: {}ms", result.duration_ms);
println!("输出文件: {:?}", result.executable_path);
```

## 测试

**运行所有测试：**
```bash
cargo test
```

**运行特定测试：**
```bash
# 词法分析测试
cargo test lexer

# 语法分析测试
cargo test parser

# 代码生成测试
cargo test codegen

# 异步运行时测试
cargo test async_runtime

# 端到端测试
cargo test --test end_to_end_test
```

## 示例程序

### Hello World
```qi
函数 主函数() {
    变量 问候语 = "你好，世界！";
    返回 0;
}
```

### 斐波那契数列
```qi
函数 斐波那契(参数 n) {
    如果 n <= 1 {
        返回 n;
    }
    
    变量 a = 0;
    变量 b = 1;
    变量 i = 2;
    
    当 i <= n {
        变量 temp = a + b;
        a = b;
        b = temp;
        i = i + 1;
    }
    
    返回 b;
}

函数 主函数() {
    变量 结果 = 斐波那契(10);
    返回 0;
}
```

### 数组操作
```qi
函数 主函数() {
    变量 数组 = [1, 2, 3, 4, 5];
    变量 总和 = 0;
    
    对于 元素 在 数组 {
        总和 = 总和 + 元素;
    }
    
    返回 总和;
}
```

## 性能特性

### 异步运行时性能

- **工作线程数**: 自动检测 CPU 核心数
- **任务队列容量**: 默认 1024 个任务
- **协程栈大小**: 默认 2MB
- **栈池大小**: 默认预分配 128 个栈

### 编译器优化级别

1. **None**: 无优化，用于调试
2. **Basic**: 基本优化，快速编译
3. **Standard**: 标准优化，平衡性能和编译时间
4. **Maximum**: 最大优化，追求最佳性能

## 调试和诊断

### 启用详细输出
```bash
cargo run -- --verbose run examples/hello.qi
```

### 启用调试模式
```bash
cargo run -- debug examples/hello.qi --verbose --memory --profile
```

### 查看编译器信息
```bash
# 版本信息
cargo run -- info --version

# 语言特性
cargo run -- info --language

# 支持的目标平台
cargo run -- info --targets
```

## 扩展和自定义

### 添加新的关键字

1. 在 `src/lexer/tokens.rs` 中添加新的 `TokenKind`
2. 在 `src/lexer/keywords.rs` 中注册关键字
3. 在 `src/parser/grammar.lalrpop` 中添加语法规则
4. 在 `src/parser/ast.rs` 中添加对应的 AST 节点

### 实现新的运行时功能

1. 在 `src/runtime/` 下创建新模块
2. 在 `src/runtime/executor.rs` 中添加 FFI 导出函数
3. 在代码生成器中添加对新功能的支持

## 已知限制和未来改进

### 当前限制

1. WebAssembly 目标运行支持尚未完全实现
2. 某些高级语言特性（如泛型、trait）尚未实现
3. 标准库功能仍在扩展中

### 计划改进

1. 完善 WebAssembly 支持
2. 添加更多标准库函数
3. 实现增量编译
4. 添加语言服务器协议 (LSP) 支持
5. 改进错误消息和诊断信息

## 贡献指南

欢迎贡献代码！请参考以下步骤：

1. Fork 本仓库
2. 创建特性分支
3. 提交更改并添加测试
4. 确保所有测试通过
5. 提交 Pull Request

## 许可证

MIT License - 详见 LICENSE 文件
