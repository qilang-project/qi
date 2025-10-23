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
│   │   └── lib.rs           # Runtime 主文件 (Rust FFI 导出)
│   └── codegen/
│       └── builder.rs       # LLVM IR 代码生成器
├── runtime/                 # C Runtime (可选)
│   ├── include/
│   └── src/
└── examples/
    └── runtime/             # Runtime 功能测试示例
```

**核心原理：**

- Rust Runtime (`src/runtime/lib.rs`) 编译为静态库 (`libqi_runtime.a`)
- 使用 `#[no_mangle]` 和 `extern "C"` 导出 C ABI 兼容的函数
- LLVM IR 代码通过 `declare` 声明这些函数
- 链接时将 LLVM IR 编译的目标文件与 Runtime 静态库链接

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

找到 `map_function_name` 方法，添加新函数的中英文映射：

```rust
fn map_function_name(&self, name: &str) -> Option<&str> {
    match name {
        // 现有映射...
        "打印" => Some("qi_runtime_println"),
        "打印整数" => Some("qi_runtime_print_int"),

        // 添加新的映射
        "平方" => Some("qi_runtime_square"),
        "拼接" => Some("qi_runtime_string_concat"),

        _ => None,
    }
}
```

#### 3.2 添加函数声明

找到 `emit_runtime_declarations` 方法，添加 LLVM IR 函数声明：

```rust
fn emit_runtime_declarations(&self) -> String {
    let mut ir = String::new();

    // 现有声明...
    ir.push_str("declare i32 @qi_runtime_print_int(i64)\n");

    // 添加新的声明
    ir.push_str("declare i64 @qi_runtime_square(i64)\n");
    ir.push_str("declare ptr @qi_runtime_string_concat(ptr, ptr)\n");

    ir
}
```

**重要：** 声明的类型签名必须与 Rust 函数完全匹配！

#### 3.3 配置参数类型推断（如果需要）

如果函数参数类型有特殊要求，在 `build_function_call` 方法中添加类型检测：

```rust
// 在 build_function_call 方法内
if callee == "qi_runtime_square" {
    // 参数必须是 i64 类型
    param_type = "i64";
} else if callee == "qi_runtime_string_concat" {
    // 参数必须是 ptr 类型
    param_type = "ptr";
}
```

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

## 完整示例：添加数学函数 `绝对值`

### 1. 添加 Runtime 函数

`src/runtime/lib.rs`:

```rust
/// 计算整数的绝对值
#[no_mangle]
pub extern "C" fn qi_runtime_abs(value: c_long) -> c_long {
    value.abs()
}

/// 计算浮点数的绝对值
#[no_mangle]
pub extern "C" fn qi_runtime_fabs(value: c_double) -> c_double {
    value.abs()
}
```

### 2. 注册到代码生成器

`src/codegen/builder.rs`:

```rust
// 在 map_function_name 中
"绝对值" => Some("qi_runtime_abs"),
"浮点绝对值" => Some("qi_runtime_fabs"),

// 在 emit_runtime_declarations 中
ir.push_str("declare i64 @qi_runtime_abs(i64)\n");
ir.push_str("declare double @qi_runtime_fabs(double)\n");

// 在 build_function_call 中（参数类型推断）
if callee == "qi_runtime_abs" {
    param_type = "i64";
} else if callee == "qi_runtime_fabs" {
    param_type = "double";
}
```

### 3. 创建测试

`examples/runtime/绝对值测试.qi`:

```qi
函数 主() {
    打印("=== 绝对值测试 ===");

    打印整数(绝对值(-42));      // 42
    打印整数(绝对值(100));      // 100
    打印整数(绝对值(0));        // 0

    打印浮点数(浮点绝对值(-3.14));  // 3.14
    打印浮点数(浮点绝对值(2.718)); // 2.718

    打印("=== 测试完成 ===");
}
```

### 4. 运行测试

```bash
cargo run --release -- run examples/runtime/绝对值测试.qi
```

**输出：**

```
=== 绝对值测试 ===
42
100
0
3.14
2.718
=== 测试完成 ===
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

1. ✅ **定义函数** - 在 `src/runtime/lib.rs` 中添加 `#[no_mangle] extern "C"` 函数
2. ✅ **验证导出** - 使用 `nm` 检查符号是否正确导出
3. ✅ **注册到代码生成器** - 在 `builder.rs` 中添加映射、声明和类型推断
4. ✅ **创建测试** - 编写 `.qi` 测试文件
5. ✅ **运行验证** - 使用 `cargo run -- run` 测试功能
6. ✅ **文档化** - 添加注释和使用示例

**关键要点：**

- 类型必须匹配（Rust FFI ↔ LLVM IR）
- 使用 `#[no_mangle]` 和 `extern "C"`
- macOS 需要 `-Wl,-force_load` 强制加载静态库符号
- 测试先行，确保功能正常

---

**日期：** 2025-10-23  
**版本：** 1.0  
**维护者：** Qi Language Team
