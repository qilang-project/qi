# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Qi is a programming language compiler with 100% Chinese keywords. It compiles Qi source code to LLVM IR and then to native executables. The project is written in Rust and includes a custom async runtime with M:N coroutine scheduling.

## Common Development Commands

### Building and Testing
```bash
# Build the compiler (development build)
cargo build

# Build with optimizations
cargo build --release

# Run tests
cargo test

# Run a specific test
cargo test test_name

# Run examples (verification)
cargo run -- run examples/basic/hello_world.qi
cargo run -- run examples/basic/calculations.qi
```

### Compiler Usage
```bash
# Check syntax only
cargo run -- check source_file.qi

# Compile to LLVM IR
cargo run -- compile source_file.qi -o output.ll

# Compile and run
cargo run -- run source_file.qi

# Format source code
cargo fmt

# Run linter
cargo clippy
```

### Development Workflow
```bash
# Build compiler and test with Chinese examples
cargo build
cargo run -- run examples/basic/变量类型演示.qi

# Test float operations
cargo run -- run examples/basic/float_test.qi

# Verify compilation pipeline
cargo run -- compile examples/basic/hello_world.qi -o test.ll
clang -c test.ll -o test.o  # Verify LLVM IR syntax
```

## Architecture Overview

### Core Compilation Pipeline
1. **Lexer** (`src/lexer/`): Tokenizes Chinese keywords and UTF-8 source
2. **Parser** (`src/parser/`): LALRPOP-generated parser with Chinese grammar rules
3. **AST** (`src/parser/ast.rs`): Abstract syntax tree with Chinese node names
4. **Code Generation** (`src/codegen/`): Converts AST to LLVM IR
5. **Runtime** (`src/runtime/`): Provides execution environment and async support

### Key Components

#### Parser (`src/parser/grammar.lalrpop`)
- Uses LALRPOP for parsing Chinese grammar
- Supports all Chinese keywords: `如果/否则`, `当`, `函数`, `变量`, etc.
- 中文逻辑运算符：`且`/`与`==`&&`，`或`==`||`，`非`==`!`（优先级与 `&& || !` 一致）
- **保留字 2026-08 精简（102 → 64）**：每个保留字都从用户手里偷走一个标识符，加词前先想清楚。
  - 词形比较/四则运算符已删（加/减/乘/除/取余/等于/不等于/大于/小于/大于等于/小于等于）——
    中文习惯写数学就用 `+ - * / % == != > < >= <=`；`字符串::等于`、`向量.加` 这类函数名不受影响
  - 已删的舶来语法：借用/克隆/拥有/移动/释放/锁/线程/原子/并行/并发/中断/跳转/私有/
    联合体/循环/内联/异步块/解引用/取地址（& * 符号形保留）/创建通道（用 `通道<T>(n)`）/
    引用/可变引用/字典/集合/长整数/短整数（字典集合功能在 标准库 模块里，类型关键字是空壳）
  - 真事实源是 `src/parser/grammar.lalrpop` 的字面量终结符；`src/lexer/keywords.rs` 只服务
    诊断/工具链，`tests/关键字表一致性.rs` 强制它是语法字面量的子集，别再手抄第三份词表
- Handles operator precedence with 8-level expression hierarchy
- Generates AST with Chinese node variants

#### Code Generation (`src/codegen/builder.rs`)
- `IrBuilder` constructs LLVM IR through `IrInstruction` enum
- Special handling for Chinese function name mangling
- Parameter vs variable distinction (parameters used directly, variables loaded)
- Type-aware binary operations (integer vs float detection via `is_float_operand`)

#### Async Runtime (`src/runtime/async_runtime/`)
- M:N coroutine scheduler with work-stealing
- C FFI layer in `c_runtime/syscalls.c`
- Task queues, executor, and memory pools
- Integrated with LLVM IR generation through runtime calls

### Language Features Support
- **Chinese Keywords**: 100% Chinese identifiers and syntax
- **Type System**: Basic types (整数, 浮点数, 字符串, 布尔)
- **Functions**: Regular functions with Chinese names
- **Async/Future**: Asynchronous programming using `未来<T>` (Future) type
- **Control Flow**: 如果/否则/否则如果 (else if 链，连写 `否则如果` 或空格分开 `否则 如果` 等价，脱糖为嵌套 else{if}), 当, 对于 loops
  - 整数区间循环三种写法：`对于 i 在 起..止`（半开，不含止）、`对于 i 在 起 直到 止`（半开，不含止，中文自然读法，典型 `0 直到 长度(数组)` 取代 `0 到 长度(数组) - 1`）、`对于 i 在 起 到 止`（闭区间，含止）。半开区间在 `起 >= 止` 时零次迭代，不倒走。区分由 AST `RangeExpression.inclusive` 承载（到=true，直到/`..`=false）。
- **Structs/Enums**: Chinese field names and variant names

### Async Programming with Future Types
Qi uses the `未来<T>` (Future) type for asynchronous operations:

```qi
// Function returning Future<整数>
函数 异步计算(值: 整数) : 未来<整数> {
    返回 值 * 2;  // Automatically wrapped as Future
}

函数 入口() {
    // Store Future in a variable
    变量 结果未来: 未来<整数> = 异步计算(21);

    // Await the Future to get the value
    变量 结果: 整数 = 等待 结果未来;
    打印行(结果);  // Prints: 42

    // Or await function call directly
    变量 结果2: 整数 = 等待 异步计算(30);
    打印行(结果2);  // Prints: 60
}
```

**Supported Future Types:**
- `未来<整数>` (Future<i64>) - Integer futures
- `未来<浮点数>` (Future<double>) - Floating point futures
- `未来<布尔>` (Future<bool>) - Boolean futures
- `未来<字符串>` (Future<String>) - String futures

**Key Syntax:**
- **Function Declaration**: `函数 名称(...) : 未来<T>` - Returns a Future
- **Await Expression**: `等待 表达式` - Awaits a Future value
- **Static Method**: `未来::就绪(值)` - Creates a ready Future

**Note**: Qi uses explicit `未来<T>` type annotations instead of `async/await` keywords. Functions returning `未来<T>` automatically wrap return values in Futures.

**真协程默认开（2026-07-12）**：`QI_CORO` 默认开启（R1-R6 全绿）。「返回 `未来<T>` 且含 `等待`」的函数
编译成 LLVM coroutine（`llvm.coro.*` 状态机），由多核 M:N 执行器调度（默认 worker 数 = CPU 逻辑核数，
`QI_CORO_WORKERS` 可覆盖）。门控是**函数级**：无 `未来<T>+等待` 的普通程序 IR 逐字节不变、性能零回退。
通道（`通道<T>`/`<-`/`发送`/`select 选择`）在协程模式走协程原生实现（park-wake 背压 + 死锁检测）；
非协程上下文（顶层、tokio handler）里裸用带缓冲通道做信号量按非阻塞轮询正常工作。
`QI_CORO=0` 退回 eager future（调试用）。


### WebAssembly 目标（2026-09-05 起可用）

```bash
qi --target wasm compile 程序.qi -o 程序.wasm   # 一步到位：编译 + wasm-ld 链接
wasmtime run 程序.wasm
```

- 产物是 **wasm32-wasip1** 模块。`-o 程序.o` 或不给 `-o` 只出目标文件不链接。
- 编译器自己找 wasm-ld / wasi sysroot / wasm 运行时归档（rustup 的 wasm32-wasip1 目标自带前两样；
  归档在 `qi-runtime/wasm` 里 `cargo build --release --target wasm32-wasip1`），找不到报中文提示；
  可用 `QI_WASM_LD` / `QI_WASM_SYSROOT` / `QI_WASM_RUNTIME_LIB` 钉死。
- wasm 运行时 `qi-runtime/wasm` 用 `#[path]` **复用主运行时源文件**（一份源码两个目标），只手写了
  打印 / 分配 / goroutine（就地同步跑）/ Future（同步）/ 异常（只能 abort）。
- **不可用**：网络 / HTTP / 数据库 / Redis / 大模型 / MCP / gRPC / 图形化 / 子进程 / `尝试/捕获`
  （wasi libc 没有 setjmp）。链接期报 `undefined symbol: qi_xxx`，编译器附提示。
- 入口符号：wasm 目标下 main 叫 `__main_argc_argv(i32, ptr) -> i32`（新版 wasi-libc 的
  `__main_void` 只弱引用它，不再回退到 `main`；见 mod.rs 生成入口）。
- **32 位指针暴露的老 bug**：数组/结构体/枚举都是「每槽 8 字节」布局，`槽指针` 以前按元素类型做
  GEP，x86_64 上指针也 8 字节所以没事，wasm32 上 `数组<字符串>` 第 1 个元素压在长度头上。
  注册表里写 `i32` 的返回值以前一律按 i64 声明，Rust 返回 i32，wasm-ld 直接判签名不匹配插
  unreachable。这两类在 wasm 上是必崩，在 x86_64 上是靠寄存器高位碰巧为 0 —— 改 FFI 签名时
  用 `tests/wasm/断言.sh`（原生 vs wasm 差分）兜底，`make wasm`。
- 详见 `wasm演示/README.md`。

## Important Implementation Details

### Chinese Name Handling
- Function names are mangled using `mangle_function_name()` for LLVM compatibility
- Parameter names get `%` prefix in LLVM IR
- Chinese identifiers are encoded as hexadecimal in LLVM symbols

### Type System
- Type annotations use `get_llvm_type()` to map Chinese types to LLVM types
- Float vs integer detection checks both literal content and variable types
- Parameters tracked with `param_` prefix in `variable_types` HashMap

### C FFI 的 32 位整数：`C整数` / `C无符号整数`

qi 的 `整数` 是 i64，C 的 `int` 只有 32 位。用 i64 原型去接 `int` 返回值，读到的是
被调方**根本没写过**的高 32 位 —— `-1` 变 4294967295，负数错误码判负全线失效
（zlib 一半以上的函数都靠负数报错）。

外部块签名里把这种位置写成 `C整数`（无符号则 `C无符号整数`）：

```qi
外部 "z" {
    函数 inflate(strm: 指针, flush: 整数): C整数;   // C 侧是 int
}
```

编译器按 i32 建原型、返回补 sext（无符号 zext）、实参补 trunc；**调用方拿到的仍是
普通 `整数`**，类型推断毫不知情。`qi 绑定` 对 C 的 int/short/enum 返回自动产出它。

实现要点：
- **不占保留字**。`C整数` 以 ASCII 字母开头，词法上就是普通标识符 →
  `TypeNode::自定义类型`，在 `登记外部函数` 里拦截，别处照常按未定义类型走
- 宽度存在 `符号表.外部c宽度`（函数名 → (形参宽度表, 返回宽度)），**签名表里仍是
  `整数`** —— 这是刻意的，类型系统不该知道 C ABI 的事
- 只有 `外部.rs` 的建原型和调用点查这张表；表里没有的函数逐字节走老路
- 写在外部块之外会被类型检查器报错（`QI_TYPECHECK=1`）—— 名字暗示 32 位却静默
  退化成 `整数` 是骗人的
- 回归钉在 `tests/ffi链接/断言.sh` 用例 10-12（含"不标宽度仍走 i64"的防回归）

### LLVM IR Generation
- Parameters used directly (no load instructions needed)
- Variables require alloca + store + load pattern
- Binary operations use operator-specific LLVM instructions
- String concatenation handled specially for `+` operator

### Known Issues and Workarounds
- Complex multi-function examples with mixed types may have type inference issues
- Chinese examples with multiple functions sometimes generate mismatched parameter types
- Simple examples and single functions work correctly

## File Structure Notes

### Critical Files
- `src/parser/grammar.lalrpop`: Chinese grammar definition
- `src/codegen/builder.rs`: LLVM IR generation logic
- `src/lexer/keywords.rs`: Chinese keyword definitions
- `src/main.rs`: CLI entry point
- `build.rs`: LALRPOP processing and C runtime compilation

### Example Organization
- `examples/basic/`: Working examples with Chinese syntax
- Chinese examples demonstrate full language capabilities
- Simple examples (hello_world, calculations) are most reliable

## Development Notes

### Compiler Flags
- Use `--features llvm` to enable LLVM code generation (when available)
- `--release` builds optimize for performance
- Debug builds include verbose logging for compilation steps

### Testing Strategy
- Test both simple and complex Chinese examples
- Verify LLVM IR compilation with clang
- Run generated executables to ensure runtime works
- Focus on type system correctness for mixed-type operations

### Performance Considerations
- M:N async runtime provides lightweight concurrency
- LLVM optimization passes in release builds
- Memory pools for coroutine stack management
- 可执行的 文件必须 包 主程序;

函数 入口() { ！！！