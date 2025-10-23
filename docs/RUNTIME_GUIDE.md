# Qi Runtime 功能添加指南

本文档详细说明如何向 Qi 语言的 Runtime 库添加新功能。

## 目录

1. [Runtime 架构概述](#runtime-架构概述)
2. [添加新的 Runtime 函数](#添加新的-runtime-函数)
3. [类型系统说明](#类型系统说明)
4. [代码生成器集成](#代码生成器集成)
5. [测试验证](#测试验证)
6. [常见问题](#常见问题)
7. [完整示例](#完整示例)

---

## Runtime 架构概述

Qi Runtime 由以下几个主要部分组成：

```
qi/
├── src/
│   ├── runtime/
│   │   ├── executor.rs      # Runtime 执行器 (FFI 函数导出)
│   │   ├── environment.rs   # 运行时环境管理
│   │   ├── memory/          # 内存管理
│   │   ├── io/              # I/O 操作
│   │   ├── stdlib/          # 标准库函数
│   │   └── mod.rs           # Runtime 模块入口
│   └── codegen/
│       └── builder.rs       # LLVM IR 代码生成器
└── examples/
    └── runtime/             # Runtime 功能测试示例
```

### Runtime 工作原理

Qi Runtime 采用 **编译器前端 + Rust Runtime 库** 的架构，通过 FFI (Foreign Function Interface) 将用户代码与底层运行时功能连接起来。

#### 1. 编译流程

```
┌─────────────┐
│  Qi 源代码   │  函数 主() { 打印("你好"); }
└──────┬──────┘
       │ 词法分析 + 语法分析
       ▼
┌─────────────┐
│  AST 树      │  Program { 函数声明(...) }
└──────┬──────┘
       │ 代码生成 (builder.rs)
       ▼
┌─────────────┐
│  LLVM IR    │  declare i32 @qi_runtime_println(ptr)
│             │  define i32 @main() {
│             │    call i32 @qi_runtime_println(...)
│             │  }
└──────┬──────┘
       │ Clang 编译
       ▼
┌─────────────┐
│  目标文件    │  .o (机器码 + 未解析的符号引用)
└──────┬──────┘
       │ 链接 Runtime 静态库
       ▼
┌─────────────┐
│  可执行文件  │  符号已解析，可以直接运行
└─────────────┘
```

#### 2. 函数映射机制

当用户在 Qi 代码中调用 `打印("你好")` 时，编译器会经过以下步骤：

**步骤 A: 名称映射 (builder.rs)**

```rust
// map_to_runtime_function 方法
"打印" | "print" → "qi_runtime_println"
```

**步骤 B: 生成 LLVM IR 声明**

```llvm
; 在 emit_llvm_ir 中自动添加
declare i32 @qi_runtime_println(ptr)
```

**步骤 C: 生成函数调用**

```llvm
; 在用户代码中
%str = ...  ; 字符串常量
%result = call i32 @qi_runtime_println(ptr %str)
```

**步骤 D: 链接到 Rust Runtime (executor.rs)**

```rust
#[no_mangle]
pub extern "C" fn qi_runtime_println(s: *const c_char) -> c_int {
    // 实际的打印实现
    unsafe {
        if let Ok(rust_str) = CStr::from_ptr(s).to_str() {
            println!("{}", rust_str);
            return 0;
        }
    }
    -1
}
```

#### 3. 类型系统桥接

Qi 语言的类型需要在三个层面保持一致：

| Qi 层面  | LLVM IR 层面 | Rust Runtime 层面 | 说明             |
| -------- | ------------ | ----------------- | ---------------- |
| `整数`   | `i64`        | `c_long` (i64)    | 64 位有符号整数  |
| `浮点数` | `double`     | `c_double` (f64)  | 64 位浮点数      |
| `字符串` | `ptr`        | `*const c_char`   | UTF-8 字符串指针 |
| `布尔`   | `i1`         | `c_int` (i32)     | 布尔值 (0/1)     |

**示例：整数打印**

```qi
打印整数(42);
```

生成的 LLVM IR：

```llvm
call i32 @qi_runtime_print_int(i64 42)
```

对应的 Rust 函数：

```rust
#[no_mangle]
pub extern "C" fn qi_runtime_print_int(value: c_long) -> c_int {
    print!("{}", value);
    0
}
```

#### 4. 多态函数处理

特殊情况：`打印` 函数根据参数类型选择不同的运行时函数

```rust
// 在 builder.rs 的 build_node 中
let runtime_function = if call_expr.callee == "打印" {
    // 根据参数类型选择正确的函数
    match expr_type {
        "string" => "qi_runtime_println",      // 字符串
        "float" => "qi_runtime_println_float", // 浮点数
        _ => "qi_runtime_println_int",         // 整数
    }
} else {
    // 其他函数通过 map_to_runtime_function 映射
    self.map_to_runtime_function(&call_expr.callee)
};
```

这种设计允许用户写出简洁的代码：

```qi
打印(42);        // 自动调用 qi_runtime_println_int
打印(3.14);      // 自动调用 qi_runtime_println_float
打印("你好");    // 自动调用 qi_runtime_println
```

#### 5. 符号导出与链接

**macOS 平台：**

```bash
# 编译 Rust Runtime 为静态库
cargo build --release
# 生成: target/release/libqi_runtime.a

# 符号名带下划线前缀
nm target/release/libqi_runtime.a | grep qi_runtime_println
# 输出: 0000000000001234 T _qi_runtime_println

# 链接时需要 -Wl,-force_load 强制加载所有符号
clang user_code.o -Wl,-force_load target/release/libqi_runtime.a -o program
```

**Linux 平台：**

```bash
# 符号名不带下划线
nm target/release/libqi_runtime.a | grep qi_runtime_println
# 输出: 0000000000001234 T qi_runtime_println

# 链接时使用 --whole-archive
clang user_code.o -Wl,--whole-archive target/release/libqi_runtime.a -Wl,--no-whole-archive -o program
```

**为什么需要强制加载？**

静态库默认只链接被引用的符号。但 LLVM IR 中的 `declare` 语句在编译为目标文件后，
符号引用可能不会被标记为"强引用"，导致链接器跳过这些符号。`-force_load` 或
`--whole-archive` 强制链接器包含静态库中的所有符号。

#### 6. 内存管理

**字符串返回值处理：**

```rust
// Runtime 函数返回新分配的字符串
#[no_mangle]
pub extern "C" fn qi_runtime_string_concat(s1: *const c_char, s2: *const c_char) -> *mut c_char {
    // ...
    let c_result = std::ffi::CString::new(result).unwrap();
    c_result.into_raw()  // ⚠️ 所有权转移给调用者！
}

// 提供释放函数
#[no_mangle]
pub extern "C" fn qi_runtime_free_string(s: *mut c_char) {
    if !s.is_null() {
        unsafe {
            let _ = std::ffi::CString::from_raw(s);
            // CString 析构时自动释放内存
        }
    }
}
```

未来可以通过编译器自动插入释放代码，避免内存泄漏。

---

### 核心原理总结

1. **统一的命名体系**：

   - 用户层：中文函数名 + 英文别名（如 `打印`、`print`）
   - 实现层：统一的 `qi_runtime_...` 格式

2. **三层类型系统**：

   - Qi 类型 → LLVM IR 类型 → Rust FFI 类型
   - 必须严格匹配，否则链接失败

3. **FFI 桥接**：

   - `#[no_mangle]` 防止符号改名
   - `extern "C"` 使用 C ABI 调用约定
   - Rust 静态库导出所有运行时函数

4. **灵活的映射机制**：
   - 简单函数：通过 `map_to_runtime_function` 一对一映射
   - 复杂函数：在代码生成时动态选择（如多态的 `打印`）

---

## 添加新的 Runtime 函数

### 步骤 1: 在 Runtime 库中添加函数

编辑 `src/runtime/lib.rs`，添加新的导出函数：

```rust
use std::ffi::{c_char, c_int, c_long, c_double};

/// 示例：求数字的平方
///
/// 参数:
///   - value: 要计算平方的整数
///
/// 返回:
///   - 平方值
#[no_mangle]
pub extern "C" fn qi_runtime_square(value: c_long) -> c_long {
    value * value
}

/// 示例：拼接两个字符串
///
/// 参数:
///   - s1: 第一个字符串指针
///   - s2: 第二个字符串指针
///
/// 返回:
///   - 新分配的拼接后的字符串指针
///   - 调用者负责释放内存
#[no_mangle]
pub extern "C" fn qi_runtime_string_concat(s1: *const c_char, s2: *const c_char) -> *mut c_char {
    if s1.is_null() || s2.is_null() {
        return std::ptr::null_mut();
    }

    unsafe {
        let c_str1 = std::ffi::CStr::from_ptr(s1);
        let c_str2 = std::ffi::CStr::from_ptr(s2);

        match (c_str1.to_str(), c_str2.to_str()) {
            (Ok(text1), Ok(text2)) => {
                let result = format!("{}{}", text1, text2);
                let c_result = std::ffi::CString::new(result).unwrap();
                c_result.into_raw()  // 转移所有权给调用者
            }
            _ => std::ptr::null_mut(),
        }
    }
}
```

**重要注意事项：**

1. **必须使用 `#[no_mangle]`**：防止 Rust 编译器修改函数名
2. **必须使用 `extern "C"`**：确保使用 C ABI 调用约定
3. **使用 C 类型**：`c_long` (i64), `c_double` (double), `c_char` (字符), `c_int` (i32)
4. **内存管理**：
   - 如果返回堆分配的指针（如字符串），调用者负责释放
   - 考虑提供对应的释放函数（如 `qi_runtime_free_string`）
5. **错误处理**：空指针检查、边界检查等

### 步骤 2: 验证符号导出

编译 Runtime 库并检查符号是否正确导出：

```bash
# 编译项目
cargo build --release

# 检查导出的符号
nm target/release/libqi_runtime.a | grep qi_runtime_square

# 应该看到类似输出：
# 0000000000002abc T _qi_runtime_square
```

**注意：** macOS 上符号名前有下划线 `_`，Linux 上没有。

---

## 类型系统说明

### LLVM IR 类型映射

| Qi 类型 | Rust C 类型           | LLVM IR 类型 | 说明               |
| ------- | --------------------- | ------------ | ------------------ |
| 整数    | `c_long` (i64)        | `i64`        | 64 位有符号整数    |
| 浮点数  | `c_double`            | `double`     | 64 位浮点数        |
| 字符串  | `*const c_char`       | `ptr`        | 字符串指针 (UTF-8) |
| 布尔    | `c_int`               | `i32`        | 0=false, 非 0=true |
| 指针    | `*mut T` / `*const T` | `ptr`        | 通用指针类型       |

### 参数传递方式

**按值传递 (Pass by Value):**

```rust
#[no_mangle]
pub extern "C" fn qi_runtime_print_int(value: c_long) -> c_int {
    println!("{}", value);
    0
}
```

LLVM IR 声明：

```llvm
declare i32 @qi_runtime_print_int(i64)
```

调用：

```llvm
%result = call i32 @qi_runtime_print_int(i64 42)
```

**按引用传递 (Pass by Reference):**

```rust
#[no_mangle]
pub extern "C" fn qi_runtime_println(s: *const c_char) -> c_int {
    // ...
}
```

LLVM IR 声明：

```llvm
declare i32 @qi_runtime_println(ptr)
```

调用：

```llvm
%str_ptr = /* ... */
%result = call i32 @qi_runtime_println(ptr %str_ptr)
```

---

## 代码生成器集成

### 步骤 3: 在代码生成器中注册函数

编辑 `src/codegen/builder.rs`：

#### 3.1 添加函数名映射

找到 `map_to_runtime_function` 方法，添加新函数的中英文映射：

```rust
/// Map Chinese function names to runtime function names
/// This bridges Qi language function names (Chinese/English aliases) to actual runtime C function names
fn map_to_runtime_function(&self, name: &str) -> Option<String> {
    let runtime_func = match name {
        // 现有映射...
        "打印整数" | "print_int" => Some("qi_runtime_print_int"),
        "字符串长度" | "长度" | "len" => Some("qi_runtime_string_length"),

        // 添加新的映射
        "平方" | "square" => Some("qi_runtime_square"),
        "拼接" | "concat" => Some("qi_runtime_string_concat"),

        _ => None,
    };

    runtime_func.map(|s| s.to_string())
}
```

**命名规范：**

- **键（Key）**：用户在 Qi 代码中使用的名字
  - 支持中文主名称（如 `"平方"`）
  - 支持英文别名（如 `"square"`）
  - 使用 `|` 分隔多个别名
- **值（Value）**：统一的 `qi_runtime_...` 格式
  - 必须与 Rust Runtime 中的函数名完全一致
  - 使用 snake_case 命名风格

#### 3.2 添加函数声明

在 `emit_llvm_ir` 方法中，函数声明会自动添加。确保你理解声明的格式：

```rust
// 在 emit_llvm_ir 方法中已经包含的声明
ir.push_str("; Qi Runtime declarations\n");
ir.push_str("declare i32 @qi_runtime_print_int(i64)\n");
ir.push_str("declare i64 @qi_runtime_string_length(ptr)\n");

// 如果需要添加新的运行时函数类别，在对应位置添加：
ir.push_str("; Math operations\n");
ir.push_str("declare i64 @qi_runtime_square(i64)\n");

ir.push_str("; String operations\n");
ir.push_str("declare ptr @qi_runtime_string_concat(ptr, ptr)\n");
```

**重要：** 声明的类型签名必须与 Rust 函数完全匹配！

**类型签名规则：**

- 返回类型在 `declare` 之后
- 函数名以 `@` 开头
- 参数类型在括号内，用逗号分隔

示例对照表：

| Rust 函数签名                                     | LLVM IR 声明                   |
| ------------------------------------------------- | ------------------------------ |
| `fn(i64) -> i64`                                  | `declare i64 @func(i64)`       |
| `fn(f64) -> f64`                                  | `declare double @func(double)` |
| `fn(*const c_char) -> i32`                        | `declare i32 @func(ptr)`       |
| `fn(i64, i64) -> i64`                             | `declare i64 @func(i64, i64)`  |
| `fn(*const c_char, *const c_char) -> *mut c_char` | `declare ptr @func(ptr, ptr)`  |

#### 3.3 理解类型推断（自动处理）

**好消息：** 对于大多数函数，你**不需要**手动配置类型推断！

代码生成器会自动根据以下规则推断参数类型：

1. **字面量类型**：

   - 整数 → `i64`
   - 浮点数 → `double`
   - 字符串 → `ptr`
   - 布尔 → `i1`

2. **变量类型**：

   - 从 `variable_types` 映射表中查找
   - 在变量声明时自动记录

3. **函数返回类型**：
   - 根据 `map_to_runtime_function` 的映射自动推断
   - 字符串函数返回 `ptr`
   - 数学函数返回 `double` 或 `i64`

**仅在特殊情况下需要手动配置：**

如果你的函数有特殊的类型要求（例如需要强制类型转换），可以在 `build_node` 方法的 `函数调用表达式` 分支中添加：

```rust
// 在 AstNode::函数调用表达式 的 build_node 中
let runtime_function = if call_expr.callee == "特殊函数" {
    // 特殊处理逻辑
    Some("qi_runtime_special".to_string())
} else {
    self.map_to_runtime_function(&call_expr.callee)
};
```

#### 3.4 验证映射（推荐）

添加完映射后，编译项目确保没有错误：

```bash
cargo build --release
```

如果有编译错误，检查：

- 函数名拼写是否正确
- 返回值类型是否正确使用 `Some(...)` 包装
- 是否有语法错误

---

## 测试验证

### 步骤 4: 创建测试文件

创建 `examples/runtime/平方测试.qi`：

```qi
函数 主() {
    打印("=== 平方函数测试 ===");

    变量 数字 = 5;
    变量 结果 = 平方(数字);

    打印("数字: ");
    打印整数(数字);
    打印(" 的平方是: ");
    打印整数(结果);
    打印("");

    // 测试多个值
    打印整数(平方(0));   // 0
    打印整数(平方(1));   // 1
    打印整数(平方(10));  // 100
    打印整数(平方(-5));  // 25

    打印("=== 测试完成 ===");
}
```

### 步骤 5: 编译并运行测试

```bash
# 方法 1: 使用 run 命令（推荐）
cargo run --release -- run examples/runtime/平方测试.qi

# 方法 2: 分步编译
cargo run --release -- compile examples/runtime/平方测试.qi
clang -c -x ir examples/runtime/平方测试.ll -o examples/runtime/平方测试.o
clang examples/runtime/平方测试.o -Wl,-force_load target/release/libqi_runtime.a -o examples/runtime/平方测试
./examples/runtime/平方测试
```

**预期输出：**

```
=== 平方函数测试 ===
数字: 5 的平方是: 25
0
1
100
25
=== 测试完成 ===
```

---

## 常见问题

### Q1: 链接时找不到符号 `undefined reference to qi_runtime_xxx`

**原因：** Runtime 库没有正确编译或链接

**解决：**

```bash
# 重新编译 Runtime
cargo clean
cargo build --release

# 检查符号是否存在
nm target/release/libqi_runtime.a | grep qi_runtime_xxx

# 确保 run 命令使用 -Wl,-force_load (macOS) 或 -Wl,--whole-archive (Linux)
```

### Q2: LLVM IR 编译失败：类型不匹配

**错误示例：**

```
error: '%t12' defined with type 'ptr' but expected 'i64'
```

**原因：** 函数声明的类型与实际调用时的参数类型不匹配

**解决：**

1. 检查 `emit_runtime_declarations` 中的声明类型
2. 检查 `build_function_call` 中的参数类型推断
3. 确保 Rust 函数签名与 LLVM IR 声明一致

### Q3: 浮点运算时整数字面量没有转换

**错误示例：**

```llvm
%t22 = fmul double 3.14, 2   ; 错误：2 应该是 2.0
```

**原因：** 代码生成器没有正确处理类型转换

**解决：** 在 builder.rs 中，浮点运算时确保整数字面量转换为浮点：

```rust
if is_float_operation {
    if value.parse::<i64>().is_ok() {
        // 整数字面量转浮点
        format!("{}.0", value)
    } else {
        value
    }
}
```

### Q4: 字符串操作返回的指针如何释放？

**问题：** Runtime 函数返回堆分配的字符串，可能导致内存泄漏

**解决：** 提供释放函数

```rust
#[no_mangle]
pub extern "C" fn qi_runtime_free_string(s: *mut c_char) {
    if !s.is_null() {
        unsafe {
            let _ = std::ffi::CString::from_raw(s);
            // CString 析构时自动释放内存
        }
    }
}
```

在 Qi 代码中手动释放（未来可以自动化）：

```qi
变量 结果 = 拼接("Hello", "World");
打印(结果);
释放字符串(结果);  // 防止内存泄漏
```

---

## 完整示例：添加数学函数 `求最大值`

### 1. 添加 Runtime 函数

`src/runtime/executor.rs`:

```rust
/// 计算两个整数的最大值
#[no_mangle]
pub extern "C" fn qi_runtime_math_max_int(a: i64, b: i64) -> i64 {
    if a > b { a } else { b }
}

/// 计算两个浮点数的最大值
#[no_mangle]
pub extern "C" fn qi_runtime_math_max_float(a: f64, b: f64) -> f64 {
    if a > b { a } else { b }
}v v操
```

### 2. 注册到代码生成器

`src/codegen/builder.rs`:

在 `map_to_runtime_function` 方法中添加：

```rust
// Math operations
"求最大值" | "最大值" | "max" => Some("qi_runtime_math_max_int"),
"浮点最大值" | "max_float" => Some("qi_runtime_math_max_float"),
```

在 `emit_llvm_ir` 方法中确认已有数学函数声明区域，添加：

```rust
ir.push_str("; Math operations\n");
// ... 现有声明 ...
ir.push_str("declare i64 @qi_runtime_math_max_int(i64, i64)\n");
ir.push_str("declare double @qi_runtime_math_max_float(double, double)\n");
```

**注意：** 由于代码生成器已经自动处理类型推断，你不需要手动配置参数类型！

### 3. 创建测试

`examples/runtime/最大值测试.qi`:

```qi
函数 主() {
    打印("=== 最大值测试 ===");

    // 整数最大值
    变量 a = 42;
    变量 b = 100;
    变量 最大 = 求最大值(a, b);

    打印("整数最大值: ");
    打印整数(最大);  // 输出: 100

    // 直接使用字面量
    打印整数(求最大值(10, 20));   // 输出: 20
    打印整数(求最大值(-5, -10));  // 输出: -5
    打印整数(求最大值(0, 0));     // 输出: 0

    // 浮点数最大值
    打印浮点数(浮点最大值(3.14, 2.718));  // 输出: 3.14
    打印浮点数(浮点最大值(-1.5, -2.5));  // 输出: -1.5

    打印("=== 测试完成 ===");
}
```

### 4. 运行测试

```bash
cargo run --release -- run examples/runtime/最大值测试.qi
```

**预期输出：**

```
Qi Runtime initialized
=== 最大值测试 ===
整数最大值: 100
20
-5
0
3.14
-1.5
=== 测试完成 ===
Qi Runtime shutdown
```

### 5. 验证生成的 LLVM IR（可选）

```bash
cargo run --release -- compile examples/runtime/最大值测试.qi
cat examples/runtime/最大值测试.ll
```

你应该看到类似的内容：

```llvm
; Qi Runtime declarations
declare i64 @qi_runtime_math_max_int(i64, i64)
declare double @qi_runtime_math_max_float(double, double)

define i32 @main() {
entry:
  ; ...
  %t1 = call i64 @qi_runtime_math_max_int(i64 42, i64 100)
  %t2 = call double @qi_runtime_math_max_float(double 3.14, double 2.718)
  ; ...
}
```

---

## 最佳实践

### 1. 命名约定

- **Rust 函数名：** `qi_runtime_<功能名>`（使用英文，snake_case）
- **Qi 函数名：** 使用中文或英文（用户友好）
- **LLVM IR 声明：** 与 Rust 函数名完全一致

### 2. 错误处理

```rust
#[no_mangle]
pub extern "C" fn qi_runtime_divide(a: c_long, b: c_long) -> c_long {
    if b == 0 {
        eprintln!("错误：除数不能为零");
        return 0;  // 或返回特殊错误码
    }
    a / b
}
```

### 3. 文档注释

````rust
/// 计算两个整数的最大值
///
/// # 参数
/// - `a`: 第一个整数
/// - `b`: 第二个整数
///
/// # 返回值
/// 返回 `a` 和 `b` 中较大的一个
///
/// # 示例
/// ```qi
/// 变量 最大值 = 最大(10, 20);  // 返回 20
/// ```
#[no_mangle]
pub extern "C" fn qi_runtime_max(a: c_long, b: c_long) -> c_long {
    if a > b { a } else { b }
}
````

### 4. 类型安全

对于可能的类型转换，提供明确的函数：

```rust
// 整数转浮点
#[no_mangle]
pub extern "C" fn qi_runtime_int_to_float(value: c_long) -> c_double {
    value as c_double
}

// 浮点转整数（截断）
#[no_mangle]
pub extern "C" fn qi_runtime_float_to_int(value: c_double) -> c_long {
    value as c_long
}
```

---

## 调试技巧

### 1. 检查生成的 LLVM IR

```bash
cargo run --release -- compile examples/runtime/测试.qi
cat examples/runtime/测试.ll
```

查找函数调用，确保：

- 函数名正确
- 参数类型匹配
- 返回值类型正确

### 2. 使用 verbose 模式

```bash
# 添加 --verbose 标志查看详细编译过程
cargo run --release -- run examples/runtime/测试.qi --verbose
```

### 3. 手动验证链接

```bash
# 编译 IR 到目标文件
clang -c -x ir examples/runtime/测试.ll -o test.o

# 查看需要的符号
nm -u test.o | grep qi_runtime

# 查看 Runtime 库提供的符号
nm target/release/libqi_runtime.a | grep qi_runtime

# 手动链接
clang test.o -Wl,-force_load target/release/libqi_runtime.a -o test
```

---

## 总结

添加新的 Runtime 功能的完整流程：

1. ✅ **定义函数** - 在 `src/runtime/executor.rs` 中添加 `#[no_mangle] extern "C"` 函数
2. ✅ **注册映射** - 在 `builder.rs` 的 `map_to_runtime_function` 中添加名称映射
3. ✅ **添加声明** - 在 `builder.rs` 的 `emit_llvm_ir` 中添加 LLVM IR 函数声明
4. ✅ **创建测试** - 编写 `.qi` 测试文件
5. ✅ **运行验证** - 使用 `cargo run -- run` 测试功能
6. ✅ **文档化** - 添加注释和使用示例

**关键要点：**

- **统一的命名体系**：所有运行时函数使用 `qi_runtime_...` 格式
- **类型必须匹配**：Rust FFI ↔ LLVM IR 的类型签名必须完全一致
- **使用 FFI 标记**：`#[no_mangle]` + `extern "C"` 是必需的
- **平台差异**：macOS 需要 `-Wl,-force_load`，Linux 需要 `--whole-archive`
- **自动类型推断**：大多数情况下不需要手动配置参数类型
- **测试先行**：先写测试，确保功能正常

---

## 附录：架构演进说明

### 历史架构（已废弃）

早期版本使用了混乱的三种函数命名风格：

1. **中文名称**：`"字符串长度"` → `qi_runtime_string_length`
2. **英文别名**：`"len"` → `qi_runtime_string_length`
3. **Hex 编码**：`"e5_ad_97_e7_ac_a6_e9_95_bf"` → `e5_ad_97_e7_ac_a6_e9_95_bf`（已删除）

Hex 编码方式是为了绕过某些限制而引入的，但这导致了：

- 代码库中存在冗余的函数定义
- LLVM IR 中有重复的 declare 语句
- 维护困难，新手难以理解

### 当前架构（统一设计）

**2025-10-23 重构：** 完全删除 Hex 相关代码，统一使用清晰的命名体系。

```
用户代码层：
  中文关键字：打印、字符串长度、求平方根
  英文别名：print, len, sqrt
         ↓
编译器映射层 (map_to_runtime_function):
  统一映射到 → qi_runtime_println, qi_runtime_string_length, qi_runtime_math_sqrt
         ↓
LLVM IR 层:
  declare i32 @qi_runtime_println(ptr)
  declare i64 @qi_runtime_string_length(ptr)
  declare double @qi_runtime_math_sqrt(double)
         ↓
运行时实现层 (executor.rs):
  #[no_mangle] pub extern "C" fn qi_runtime_println(...)
  #[no_mangle] pub extern "C" fn qi_runtime_string_length(...)
  #[no_mangle] pub extern "C" fn qi_runtime_math_sqrt(...)
```

**优势：**

- ✅ 清晰一致的命名规范
- ✅ 无冗余代码
- ✅ 易于维护和扩展
- ✅ 新手友好

**重构影响：**

- 删除约 72 行冗余代码
- 清理了 `builder.rs` 和 `executor.rs` 中的 Hex 函数
- 所有现有测试通过，无功能损失

---

**文档更新日期：** 2025-10-23  
**版本：** 2.0（重大架构更新）  
**维护者：** Qi Language Team
