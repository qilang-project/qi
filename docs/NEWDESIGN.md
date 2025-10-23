Qi 编程语言统一设计文档

---

### Qi 编程语言统一设计文档

**Qi 语言**：100% 中文关键字的现代编程语言，文件后缀 .qi
**设计理念**：中文语法 + Rust 编译器前端 + Rust 运行时 (M:N 调度) + C 平台抽象接口 + LLVM 后端
**目标**：提供完全中文的开发体验，同时保持高性能、高并发和底层控制能力，支持编译到 Windows, Linux, macOS 和 WebAssembly。

---

### 目录

1.  设计概述
2.  语言关键字总表
3.  完整语法规范
4.  编译器架构设计 (含多平台目标)
5.  运行时 C 接口（平台抽象层）
6.  项目结构 (含多平台实现)
7.  标准库与包系统
8.  并发模型 (M:N 协程调度)
9.  示例代码
10. 实现计划
11. 工具链支持 (LSP 与语法高亮)

---

### 1\. 设计概述

#### 1.1 核心设计原则

- **100% 中文关键字**：所有语言关键字均使用中文，无任何英文保留字。
- **统一架构**：Rust 编译器 + Rust 运行时 + C 系统调用 + LLVM 后端。
- **性能优先**：编译到原生机器码，零运行时开销。
- **现代特性**：支持 M:N 协程并发 (类似 Goroutine)、错误处理、泛型等。
- **跨平台**：通过 LLVM 和平台抽象层，原生支持 Windows, Linux, macOS 和 WASM (WASI)。
- **开发友好**：提供完整的工具链 (LSP、语法高亮) 和标准库。

#### 1.2 技术栈

| 组件         | 技术栈                | 作用                                                  |
| :----------- | :-------------------- | :---------------------------------------------------- |
| **词法分析** | Rust + pest           | Token 解析，支持 UTF-8 中文                           |
| **语法分析** | Rust + chumsky        | AST 构建，错误恢复                                    |
| **中间代码** | Rust + inkwell        | LLVM IR 生成                                          |
| **代码优化** | LLVM                  | 标准优化管道                                          |
| **运行时**   | **Rust + C**          | 内存管理、并发调度 (Rust, M:N), 系统调用 (C 抽象接口) |
| **标准库**   | Qi 语言 + C/Rust 绑定 | 基础设施和常用功能                                    |

#### 1.3 文件命名

- **源文件**：.qi 后缀
- **包文件**：包名.qi 或 模块名.qi
- **配置文件**：qimod.json
- **构建产物**：.o、.ll、可执行文件

---

### 2\. 语言关键字总表

#### 2.1 程序结构与模块

| 关键字 | 含义            | 类似于        | 示例                           |
| :----- | :-------------- | :------------ | :----------------------------- |
| `包`   | 声明包名/模块名 | mod/namespace | `包 主程序`                    |
| `导入` | 导入模块或库    | use/\#include | `导入 标准库.输入输出`         |
| `公开` | 对外可见        | pub/extern    | `公开 函数 ...`                |
| `私有` | 模块内可见      | private       | `私有 变量 ...`                |
| `作为` | 引入别名        | as            | `导入 标准库.数学 作为 数学库` |
| `常量` | 定义常量        | const         | `常量 PI = 3.14159`            |
| `静态` | 静态变量        | static        | `静态 计数器 = 0`              |

#### 2.2 类型与变量

| 关键字     | 含义             | 类似于          | 示例                   |
| :--------- | :--------------- | :-------------- | :--------------------- |
| `变量`     | 声明变量（可变） | let mut/auto    | `变量 计数器 = 0`      |
| `不可变`   | 声明不可变变量   | let             | `不可变 名称 = "张三"` |
| `类型`     | 定义自定义类型   | type/typedef    | `类型 用户ID = 整数`   |
| `结构体`   | 定义结构体       | struct          | `结构体 用户 {...}`    |
| `枚举`     | 定义枚举         | enum            | `枚举 结果 {...}`      |
| `联合体`   | 定义联合体       | union           | `联合体 数值 {...}`    |
| `实现`     | 实现方法或特性   | impl            | `实现 用户 {...}`      |
| `特性`     | trait/接口       | trait/interface | `特性 显示 {...}`      |
| `自我`     | 当前类型         | Self            | `返回 自我`            |
| `自身`     | 当前对象         | self/this       | `函数 显示(自身)`      |
| `指针`     | 指针类型         | \*              | `指针<整数>`           |
| `引用`     | 引用类型         | &               | `引用<字符串>`         |
| `可变引用` | 可变引用         | \&mut           | `可变引用<数组>`       |

#### 2.3 控制流程

| 关键字 | 含义         | 类似于        | 示例                        |
| :----- | :----------- | :------------ | :-------------------------- |
| `如果` | 条件判断     | if            | `如果 条件 {...}`           |
| `否则` | 否则分支     | else          | `否则 {...}`                |
| `匹配` | 模式匹配     | match         | `匹配 值 {...}`             |
| `循环` | 无限循环     | loop/while(1) | `循环 {...}`                |
| `当`   | 条件循环     | while         | `当 条件 {...}`             |
| `对于` | for 循环     | for           | `对于 元素 在 列表中 {...}` |
| `中断` | 跳出循环     | break         | `中断`                      |
| `继续` | 跳过当前循环 | continue      | `继续`                      |
| `返回` | 返回函数结果 | return        | `返回 值`                   |
| `跳转` | goto 跳转    | goto          | `跳转 标签`                 |

#### 2.4 函数与闭包

| 关键字     | 含义         | 类似于         | 示例                          |
| :--------- | :----------- | :------------- | :---------------------------- |
| `函数`     | 定义函数     | fn/function    | `函数 整数 计算(...)`         |
| `内联`     | 内联函数     | inline         | `内联 函数 快速计算()`        |
| `异步`     | 异步函数     | async          | `异步 函数 网络请求()`        |
| `等待`     | 等待异步完成 | await          | `等待 网络请求()`             |
| `闭包`     | 匿名函数     | lambda/closure | `闭包 (参数) {...}`           |
| `返回类型` | 指定返回类型 | -\>            | `函数 返回类型 字符串 名称()` |

#### 2.5 错误与异常

| 关键字 | 含义          | 类似于 | 示例                       |
| :----- | :------------ | :----- | :------------------------- |
| `抛出` | 抛出异常/错误 | throw  | `抛出 错误信息`            |
| `捕获` | 捕获异常      | catch  | `捕获 错误 {...}`          |
| `尝试` | 尝试执行      | try    | `尝试 危险操作()`          |
| `结果` | Result 类型   | Result | `结果<成功类型, 错误类型>` |
| `选项` | Option 类型   | Option | `选项<类型>`               |

#### 2.6 内存与所有权

| 关键字 | 含义          | 类似于     | 示例          |
| :----- | :------------ | :--------- | :------------ |
| `拥有` | 拥有权标识    | 所有权机制 | `拥有 变量`   |
| `借用` | 借用          | Rust 借用  | `借用 引用`   |
| `移动` | 所有权移动    | move       | `移动 变量`   |
| `克隆` | 显式复制      | clone      | `克隆 对象`   |
| `释放` | 释放内存/资源 | drop       | `释放 资源`   |
| `新建` | 创建对象      | new        | `新建 实例()` |

#### 2.7 并发与线程

| 关键字 | 含义               | 类似于     | 示例              |
| :----- | :----------------- | :--------- | :---------------- |
| `并行` | 并行块/计算        | parallel   | `并行 {...}`      |
| `并发` | 并发块             | concurrent | `并发 {...}`      |
| `任务` | 轻量线程           | task/tokio | `任务 异步工作()` |
| `启动` | 启动轻量任务(协程) | go / spawn | `启动 后台任务()` |
| `线程` | 系统线程           | thread     | `线程 工作线程()` |
| `锁`   | 互斥锁             | mutex      | `锁 保护变量`     |
| `原子` | 原子操作           | atomic     | `原子 计数器`     |

#### 2.8 基础数据类型

| 关键字   | 含义           | 类似于     | 示例                        |
| :------- | :------------- | :--------- | :-------------------------- |
| `整数`   | 整型           | i32/int    | `整数 年龄 = 25`            |
| `长整数` | 64 位整型      | i64/long   | `长整数 大数 = 1000000`     |
| `短整数` | 16 位整型      | i16/short  | `短整数 小数 = 100`         |
| `字节`   | 8 位无符号整数 | u8/byte    | `字节 数据 = 255`           |
| `浮点数` | 浮点型         | f64/double | `浮点数 精度 = 3.14`        |
| `布尔`   | 布尔型         | bool       | `布尔 标志 = 真`            |
| `字符`   | 单个字符       | char       | `字符 字母 = 'A'`           |
| `字符串` | 字符串类型     | String     | `字符串 姓名 = "张三"`      |
| `空`     | 空类型         | void/()    | `函数 空 初始化()`          |
| `数组`   | 数组类型       | array      | `数组<整数> 数字列表`       |
| `字典`   | 键值映射       | map/dict   | `字典<字符串, 整数> 映射表` |
| `列表`   | 顺序列表       | Vec/List   | `列表<字符串> 名称列表`     |
| `集合`   | 无重复集合     | Set        | `集合<整数> 唯一数字`       |

#### 2.9 操作符关键字

| 关键字     | ASCII 等价 | 含义       | 优先级 |
| :--------- | :--------- | :--------- | :----- | ------ | --- |
| `加`       | +          | 加法       | 高     |
| `减`       | -          | 减法       | 高     |
| `乘`       | \*         | 乘法       | 高     |
| `除`       | /          | 除法       | 高     |
| `取余`     | %          | 取余数     | 高     |
| `等于`     | ==         | 等于比较   | 中     |
| `不等于`   | \!=        | 不等于比较 | 中     |
| `大于`     | \>         | 大于比较   | 中     |
| `小于`     | \<         | 小于比较   | 中     |
| `大于等于` | \>=        | 大于等于   | 中     |
| `小于等于` | \<=        | 小于等于   | 中     |
| `与`       | &&         | 逻辑与     | 低     |
| `或`       | `          |            | `      | 逻辑或 | 低  |
| `非`       | \!         | 逻辑非     | 高     |

#### 2.10 特殊关键字

| 关键字       | 含义     | 用途               |
| :----------- | :------- | :----------------- |
| `真`         | 布尔真值 | `真` / `假`        |
| `假`         | 布尔假值 | `真` / `假`        |
| `空指针`     | 空指针   | `空指针`           |
| `主程序入口` | 主函数   | `主程序入口()`     |
| `打印`       | 标准输出 | `打印("消息")`     |
| `输入`       | 标准输入 | `变量 值 = 输入()` |
| `长度`       | 获取长度 | `字符串.长度()`    |
| `包含`       | 检查包含 | `列表.包含(元素)`  |

---

### 3\. 完整语法规范

#### 3.1 基本语法规则

```qi
// 注释：单行注释（//）和块注释（/* */）
// 语句结束：分号（;）
// 代码块：大括号（{}）
// 字符集：UTF-8，支持所有中文字符

// 示例：基础函数
函数 整数 计算总和(整数 数字1, 整数 数字2) {
    变量 结果 = 数字1 加 数字2; // 中文操作符
    返回 结果;
}
```

#### 3.2 变量声明

```qi
// 可变变量
变量 计数器 = 0;
变量 姓名 = "张三";
变量 年龄 = 25;

// 不可变变量
不可变 常量值 = 3.14159;
不可变 固定名称 = "固定名称";

// 类型推断
变量 推断值 = 42; // 自动推断为整数
变量 文本 = "Hello"; // 自动推断为字符串

// 显式类型声明
变量 整数值: 整数 = 100;
变量 浮点值: 浮点数 = 3.14;
变量 文本值: 字符串 = "文本";
```

#### 3.3 结构体定义

```qi
// 基础结构体
结构体 用户 {
    整数 ID;
    字符串 姓名;
    整数 年龄;
    布尔 活跃状态;
}

// 嵌套结构体
结构体 地址 {
    字符串 国家;
    字符串 城市;
    字符串 街道;
}

结构体 完整用户 {
    用户 基本信息;
    地址 联系地址;
    列表<字符串> 兴趣爱好;
}

// 泛型结构体
结构体 容器<类型 T> {
    数据: T;
    大小: 整数;
}
```

#### 3.4 枚举定义

```qi
// 基础枚举
枚举 状态 {
    成功,
    失败,
    进行中
}

// 带数据的枚举
枚举 结果<成功类型, 错误类型> {
    成功(成功类型),
    失败(错误类型)
}

// 复杂枚举
枚举 网络响应 {
    成功(字符串 数据),
    失败(整数 错误码, 字符串 错误信息),
    重定向(字符串 新地址),
    超时
}
```

#### 3.5 函数定义

```qi
// 基础函数
函数 整数 加法(整数 a, 整数 b) {
    返回 a 加 b;
}

// 返回多个值（使用元组或结构体）
函数 (整数, 字符串) 获取用户信息(整数 用户ID) {
    // ... 实现
    返回 (ID, "用户名");
}

// 异步函数
异步 函数 字符串 网络请求(字符串 URL) {
    // ... 网络请求实现
    等待 连接(URL);
    返回 "响应数据";
}

// 泛型函数
函数<T> T 交换(可变引用<T> a, 可变引用<T> b) {
    变量 临时 = a;
    a = b;
    b = 临时;
}
```

#### 3.6 控制流程

```qi
// 条件语句
函数 字符串 判断年龄分级(整数 年龄) {
    如果 年龄 < 18 {
        返回 "未成年";
    } 否则 如果 年龄 < 65 {
        返回 "成年人";
    } 否则 {
        返回 "老年人";
    }
}

// 循环语句
函数 整数 计算1到n的和(整数 n) {
    变量 总和 = 0;
    变量 i = 1;

    当 i <= n {
        总和 = 总和 + i;
        i = i + 1;
    }

    返回 总和;
}

// For 循环
函数 空 打印列表元素(列表<字符串> 名称列表) {
    对于 名称 在 名称列表 {
        打印("姓名: {}", 名称);
    }
}

// 模式匹配
函数 字符串 处理结果(结果<字符串, 整数> 结果) {
    匹配 结果 {
        结果::成功(数据) => {
            返回 "成功: " + 数据;
        }
        结果::失败(错误码) => {
            返回 "失败，错误码: " + 错误码.转字符串();
        }
    }
}
```

#### 3.7 包系统

```qi
// 文件：数学工具.qi
包 数学工具;

// 导出函数
公开 函数 浮点数 计算圆面积(浮点数 半径) {
    常量 PI = 3.14159265359;
    返回 PI 乘 半径 乘 半径;
}

公开 函数 整数 阶乘(整数 n) {
    如果 n <= 1 {
        返回 1;
    } 否则 {
        返回 n 乘 阶乘(n - 1);
    }
}

// 文件：主程序.qi
导入 数学工具;

函数 整数 主程序入口() {
    变量 面积 = 数学工具.计算圆面积(5.0);
    打印("圆的面积: {}", 面积);

    变量 结果 = 数学工具.阶乘(6);
    打印("6的阶乘: {}", 结果);

    返回 0;
}
```

#### 3.8 并发与任务 (协程)

`启动` 关键字用于并发执行一个函数调用或一个异步块，而不会阻塞当前线程。这会创建一个新的 Qi `任务`（等同于 Goroutine），并将其提交给 Rust 运行时的 M:N 调度器。

```qi
// 示例 1：启动一个普通函数

函数 轻量任务() {
    变量 i = 0;
    当 i < 5 {
        打印("... 任务运行中 ... {}", i);
        i = i + 1;
        // 让出CPU，允许其他任务运行
        // (标准库需要提供 '任务::让出()' 或 '休眠')
        任务.休眠(100);
    }
    打印("轻量任务完成");
}

函数 主程序入口() {
    打印("启动一个新任务...");

    // 使用 '启动' 关键字，立即返回
    启动 轻量任务();

    打印("主函数继续执行...");
    // 主函数必须等待，否则程序会立即退出
    任务.休眠(1000);
    打印("主函数退出");
}
```

```qi
// 示例 2：启动一个 '异步' 块

异步 函数 字符串 获取数据(整数 任务ID) {
    打印("任务 {} 开始获取数据...", 任务ID);
    // 模拟非阻塞 I/O
    等待 任务.异步休眠(500);
    返回 "数据 " + 任务ID.转字符串();
}

函数 主程序入口() {
    // 启动一个异步块
    启动 异步 {
        打印("异步块 1 开始...");
        变量 数据1 = 等待 获取数据(1);
        打印("异步块 1 收到: {}", 数据1);
    };

    // 启动另一个异步块
    启动 异步 {
        打印("异步块 2 开始...");
        变量 数据2 = 等待 获取数据(2);
        打印("异步块 2 收到: {}", 数据2);
    };

    // 保持主线程存活以等待异步任务完成
    任务.休眠(2000);
}
```

---

### 4\. 编译器架构设计

#### 4.1 整体架构 (含多平台目标)

```
┌─────────────────────────────────────┐
│           Qi 源代码 (.qi)           │
├─────────────────────────────────────┤
│         Rust 编译器前端              │
│  ┌─────────────┬─────────────────┐   │
│  │  词法分析器  │    语法分析器    │   │
│  │  (Lexer)    │    (Parser)     │   │
│  └─────────────┴─────────────────┘   │
├─────────────────────────────────────┤
│         Rust 编译器中端              │
│  ┌─────────────┬─────────────────┐   │
│  │   语义分析   │    中间代码生成  │   │
│  │   (Sema)    │     (IR Gen)    │   │
│  └─────────────┴─────────────────┘   │
├─────────────────────────────────────┤
│         Rust 编译器后端 (LLVM)       │
│  ┌─────────────┬─────────────────┐   │
│  │  LLVM IR 生成 │   LLVM 优化器   │   │
│  │  (inkwell)  │   (Optimizer)   │   │
│  └─────────────┴─────────────────┘   │
├─────────────────────────────────────┤
│          Rust 运行时 (Qi-Runtime)   │
│  ┌─────────────┬─────────────────┐   │
│  │  内存管理    │    并发调度器    │   │
│  │ (GC/ARC)    │ (M:N, Async)    │   │
│  └─────────────┴─────────────────┘   │
├─────────────────────────────────────┤
│       C 系統調用接口 (平台抽象层)    │
│    (平台抽象层 - 见第 5 节)        │
├─────────────────────────────────────┤
│        目标平台 (通过 LLVM 支持)      │
│  ┌─────────┬─────────┬─────────┐   │
│  │  Windows │  Linux  │  macOS  │   │
│  │ (x86_64)│ (x86_64)│ (ARM64) │   │
│  ├─────────┴─────────┴─────────┤   │
│  │      WASM (WASI)            │   │
│  └─────────────────────────────┘   │
└─────────────────────────────────────┘
```

#### 4.2 词法分析器 (Lexer)

```rust
// compiler/src/lexer.rs
use std::collections::HashMap;

pub struct Token {
    pub kind: TokenKind,
    pub text: String,
    pub span: Span,
}

pub enum TokenKind {
    // 程序结构
    包, 导入, 公开, 私有, 作为, 常量, 静态,

    // 类型与变量
    变量, 不可变, 类型, 结构体, 枚举, 联合体, 实现, 特性,
    自我, 自身, 指针, 引用, 可变引用,

    // 控制流程
    如果, 否则, 匹配, 循环, 当, 对于, 中断, 继续, 返回, 跳转,

    // 函数与闭包
    函数, 内联, 异步, 等待, 闭包, 返回类型,

    // 错误处理
    抛出, 捕获, 尝试, 结果, 选项,

    // 内存管理
    拥有, 借用, 移动, 克隆, 释放, 新建,

    // 并发
    并行, 并发, 任务, 启动, 线程, 锁, 原子,

    // 数据类型
    整数, 长整数, 短整数, 字节, 浮点数, 布尔, 字符, 字符串,
    空, 数组, 字典, 列表, 集合,

    // 操作符
    加, 减, 乘, 除, 取余, 等于, 不等于, 大于, 小于, 大于等于, 小于等于,
    与, 或, 非,

    // 特殊值
    真, 假, 空指针,

    // 标识符和字面量
    标识符(String), 字符串(String), 整数(i64), 浮点数(f64),

    // 分隔符
    分号, 逗号, 左括号, 右括号, 左大括号, 右大括号, 左方括号, 右方括号,
}
// ... Lexer 实现 ...
```

#### 4.3 语法分析器 (Parser)

```rust
// compiler/src/parser.rs

// AST 节点定义
#[derive(Debug, Clone)]
pub enum Expr {
    字面量 { value: Literal },
    标识符 { name: String },
    二元操作 { left: Box<Expr>, op: BinOp, right: Box<Expr> },
    函数调用 { callee: Box<Expr>, args: Vec<Expr> },
    // ... 其他 AST 节点 ...
}

#[derive(Debug, Clone)]
pub enum Stmt {
    变量声明 { name: String, ty: Option<Type>, init: Option<Expr> },
    函数定义 { name: String, params: Vec<Param>, return_ty: Option<Type>, body: Vec<Stmt> },
    结构体定义 { name: String, fields: Vec<(String, Type)> },
    枚举定义 { name:String, variants: Vec<EnumVariant> },
    如果语句 { condition: Expr, then_branch: Vec<Stmt>, else_branch: Option<Vec<Stmt>> },
    // ... 其他语句节点 ...
}

#[derive(Debug, Clone)]
pub struct Program {
    pub 包名: Option<String>,
    pub 导入: Vec<Import>,
    pub 项列表: Vec<Item>,
}
// ... Parser 实现 ...
```

#### 4.4 代码生成器 (LLVM)

```rust
// compiler/src/codegen/llvm.rs
use inkwell::{context::Context, module::Module, builder::Builder, values::FunctionValue};
use inkwell::targets::{Target, TargetMachine, InitializationConfig};

pub struct LlvmCodeGenerator<'ctx> {
    context: &'ctx Context,
    module: Module<'ctx>,
    builder: Builder<'ctx>,
    target_machine: TargetMachine,
    // ...
}

impl<'ctx> LlvmCodeGenerator<'ctx> {
    // 初始化时需要指定目标平台 (e.g., "x86_64-pc-windows-msvc", "wasm32-unknown-unknown-wasi")
    pub fn new(context: &'ctx Context, target_triple: &str) -> Self {
        Target::initialize_all(&InitializationConfig::default());
        let triple = inkwell::targets::TargetTriple::create(target_triple);
        let target = Target::from_triple(&triple).unwrap();
        let target_machine = target
            .create_target_machine(
                &triple,
                "generic", // "native", "generic", or specific CPU
                "",        // features
                inkwell::OptimizationLevel::Default,
                inkwell::targets::RelocMode::Default,
                inkwell::targets::CodeModel::Default,
            )
            .unwrap();

        let module = context.create_module("qi_module");
        module.set_triple(&triple);
        module.set_data_layout(&target_machine.get_target_data().get_data_layout());

        let builder = context.create_builder();

        Self { context, module, builder, target_machine }
    }

    fn generate_function(&mut self, name: &str, params: &[Param], return_ty: &Option<Type>, body: &[Stmt]) -> Result<FunctionValue<'ctx>, CodeGenError> {
        // ... LLVM IR 生成逻辑 ...
    }

    // ...
}
```

---

### 5\. 运行时 C 接口（平台抽象层）

根据设计，Qi 的核心运行时（如内存管理、M:N 协程调度）将主要由 Rust 实现。

本节定义的 C 头文件（.h）代表了 **Rust 运行时** 与 **底层操作系统** 之间的**最小接口层 (FFI)**。**这是一个“平台抽象层”**。

`qi-runtime` (Rust) 只会调用这些 C 头文件中定义的函数 (例如 `qi_sys_open`, `qi_sys_thread_create`)。它不关心这些函数是如何实现的。

底层的实现（`.c` 文件）将针对**每个目标平台**（Windows, Linux, macOS, WASI）进行**单独编写**，以适配不同操作系统的 API。

#### 5.1 平台实现说明

- **对于 Windows (Win32)**：`qi_sys_io.c` 将使用 `CreateFileW`, `ReadFile`, `WriteFile`。`qi_sys_thread.c` 将使用 `CreateThread`, `WaitForSingleObject` 和 `CRITICAL_SECTION`。
- **对于 Linux/macOS (POSIX)**：`qi_sys_io.c` 将使用 `open`, `read`, `write` (libc)。`qi_sys_thread.c` 将使用 `pthread_create`, `pthread_join` 和 `pthread_mutex_t`。
- **对于 WASM (WASI)**：这是一个关键区别。WASM 运行在沙箱中，没有传统的系统调用。
  - 我们将目标定为 **WASI (WebAssembly System Interface)**，这是一个为 WASM 定义的标准系统接口。
  - `qi_sys_io.c` 将调用 WASI 函数，如 `fd_openat`, `fd_read`, `fd_write`。
  - `qi_sys_memory.c` 将使用 WASM 的 `memory.grow` 指令。
  - **并发限制**：WASI 的线程支持（`wasi-threads`）仍处于试验阶段。因此，**Qi 语言的 WASI 目标在初期将实现为单线程**。
    - 这意味着 `qi_sys_thread_create` 在 WASI 平台上将返回错误或不被支持。
    - Rust 运行时的 M:N 调度器（第 8 节）在 WASI 平台上将退化为 N=1 的单线程事件循环（`启动` 关键字仍可用于创建 `任务`，但它们将在同一个 OS 线程上并发执行，而不是并行）。

#### 5.2 底层内存操作 (qi_sys_memory.h)

（注意：Qi 的主内存管理，如 GC 或 ARC，将在 Rust 中实现。此 C 接口仅用于向 OS 请求原始内存块。）

```c
// runtime_c_iface/include/qi_sys_memory.h
#ifndef QI_SYS_MEMORY_H
#define QI_SYS_MEMORY_H

#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

// 向操作系统请求原始内存页
void* qi_sys_alloc_pages(size_t num_pages);

// 释放内存页
void qi_sys_free_pages(void* ptr, size_t num_pages);

// 获取页面大小
size_t qi_sys_get_page_size(void);

#ifdef __cplusplus
}
#endif
#endif // QI_SYS_MEMORY_H
```

#### 5.3 底层文件 I/O (qi_sys_io.h)

```c
// runtime_c_iface/include/qi_sys_io.h
#ifndef QI_SYS_IO_H
#define QI_SYS_IO_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// 文件描述符类型（平台相关）
typedef int qi_sys_fd_t;

// 打开文件 (返回文件描述符)
qi_sys_fd_t qi_sys_open(const char* path, int flags, int mode);

// 关闭文件
int qi_sys_close(qi_sys_fd_t fd);

// 读取文件
// 返回读取的字节数，-1 表示错误
ssize_t qi_sys_read(qi_sys_fd_t fd, void* buf, size_t count);

// 写入文件
// 返回写入的字节数，-1 表示错误
ssize_t qi_sys_write(qi_sys_fd_t fd, const void* buf, size_t count);

// 移动文件指针
int64_t qi_sys_lseek(qi_sys_fd_t fd, int64_t offset, int whence);

#ifdef __cplusplus
}
#endif
#endif // QI_SYS_IO_H
```

#### 5.4 底层线程支持 (qi_sys_thread.h)

（注意：Qi 的 `任务` (Task) 和 M:N 调度在 Rust 运行时中实现。此 C 接口仅用于创建和管理**操作系统原生线程 (OS Thread)**，Rust 调度器将运行在这些原生线程之上。）

```c
// runtime_c_iface/include/qi_sys_thread.h
#ifndef QI_SYS_THREAD_H
#define QI_SYS_THREAD_H

#include <stdbool.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// 原生线程句柄 (平台特定，例如 pthread_t)
typedef void* qi_sys_thread_t;
// 互斥锁 (平台特定)
typedef void* qi_sys_mutex_t;
// 条件变量 (平台特定)
typedef void* qi_sys_condvar_t;

// 线程函数指针类型
typedef void* (*qi_sys_thread_func_t)(void* arg);

// 创建一个操作系统原生线程
int qi_sys_thread_create(qi_sys_thread_t* out_thread, qi_sys_thread_func_t func, void* arg);

// 等待原生线程结束
int qi_sys_thread_join(qi_sys_thread_t thread);

// 释放原生线程资源
void qi_sys_thread_detach(qi_sys_thread_t thread);

// 线程休眠 (毫秒)
void qi_sys_thread_sleep(uint32_t milliseconds);

// 线程让出 CPU
void qi_sys_thread_yield(void);

// 互斥锁操作
qi_sys_mutex_t* qi_sys_mutex_create(void);
void qi_sys_mutex_lock(qi_sys_mutex_t* mutex);
void qi_sys_mutex_unlock(qi_sys_mutex_t* mutex);
void qi_sys_mutex_destroy(qi_sys_mutex_t* mutex);

// 条件变量操作
qi_sys_condvar_t* qi_sys_condvar_create(void);
void qi_sys_condvar_wait(qi_sys_condvar_t* cond, qi_sys_mutex_t* mutex);
void qi_sys_condvar_signal(qi_sys_condvar_t* cond);
void qi_sys_condvar_broadcast(qi_sys_condvar_t* cond);
void qi_sys_condvar_destroy(qi_sys_condvar_t* cond);

#ifdef __cplusplus
}
#endif
#endif // QI_SYS_THREAD_H
```

#### 5.5 运行时错误处理 (qi_sys_error.h)

```c
// runtime_c_iface/include/qi_sys_error.h
#ifndef QI_SYS_ERROR_H
#define QI_SYS_ERROR_H

#ifdef __cplusplus
extern "C" {
#endif

// 获取上一个系统调用的错误码 (例如 errno)
int qi_sys_get_last_error(void);

// 将错误码转换为字符串描述
// (注意：返回的字符串缓冲区管理)
const char* qi_sys_error_string(int error_code);

// 致命错误处理
// (由 Rust 运行时的 panic 处理器调用)
void qi_sys_panic(const char* message, const char* file, uint32_t line);

#ifdef __cplusplus
}
#endif
#endif // QI_SYS_ERROR_H
```

---

### 6\. 项目结构

#### 6.1 完整目录结构

```
qi/
├── Cargo.toml              # Rust 工作区配置
├── README.md               # 项目说明
├── LICENSE                 # 开源协议
├── .gitignore              # Git 忽略文件
│
├── compiler/               # Rust 编译器源码 (qic)
│   ├── Cargo.toml
│   ├── src/
│   │   ├── main.rs         # 主程序入口
│   │   ├── lib.rs
│   │   ├── cli/            # 命令行接口
│   │   ├── lexer/          # 词法分析器
│   │   ├── parser/         # 语法分析器
│   │   ├── semantic/       # 语义分析器
│   │   ├── ir/             # 中间表示
│   │   ├── codegen/        # 代码生成 (LLVM)
│   │   └── utils/
│
├── runtime/                # Rust 运行时 (libqi_rt)
│   ├── Cargo.toml
│   ├── src/
│   │   ├── lib.rs
│   │   ├── memory/         # 内存管理 (GC/ARC)
│   │   ├── concurrent/     # 并发调度器 (M:N, 协程)
│   │   ├── collections/    # 核心数据结构 (Rust 实现)
│   │   ├── error.rs        # 错误处理 (Panic)
│   │   └── ffi.rs          # 对 C 系統接口的 Rust 绑定
│
├── runtime_c_iface/        # C 系統接口 (平台抽象层)
│   ├── include/            # C 头文件 (平台无关的抽象)
│   │   ├── qi_sys_memory.h
│   │   ├── qi_sys_io.h
│   │   ├── qi_sys_thread.h
│   │   └── qi_sys_error.h
│   │
│   ├── src/                # C 实现文件 (平台特定)
│   │   ├── posix/          # (适用于 Linux, macOS, BSD)
│   │   │   ├── memory.c
│   │   │   ├── io.c
│   │   │   └── thread.c
│   │   │
│   │   ├── windows/        # (适用于 Windows)
│   │   │   ├── memory.c    (Win32 VirtualAlloc)
│   │   │   ├── io.c        (Win32 CreateFile)
│   │   │   └── thread.c    (Win32 CreateThread)
│   │   │
│   │   ├── wasm_wasi/      # (适用于 WASM + WASI)
│   │   │   ├── memory.c    (WASM memory.grow)
│   │   │   ├── io.c        (WASI fd_read/write)
│   │   │   └── thread.c    (单线程实现或 stub)
│   │   │
│   │   └── error.c         (平台通用的错误处理逻辑)
│   │
│   ├── CMakeLists.txt      # C 部分构建配置 (将包含逻辑以选择正确的 src 目录)
│
├── stdlib/                 # Qi 标准库 (用 .qi 语言编写)
│   ├── core/               # 核心库
│   │   ├── 基础类型.qi
│   │   └── 错误处理.qi
│   ├── io/                 # 输入输出库
│   │   ├── 文件操作.qi
│   │   └── 控制台.qi
│   ├── threading/          # 线程与任务库
│   │   ├── 任务.qi         (定义 任务.休眠, 任务.异步休眠 等)
│   │   └── 同步.qi         (定义 锁, 原子 等)
│   └── ...
│
├── tools/                  # 工具链
│   ├── qi-lsp/             # 语言服务器 (Rust Crate)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs     # LSP 主循环
│   │       └── server.rs   # LSP 功能实现
│   │
│   └── vscode-qi/          # VS Code 扩展
│       ├── package.json
│       ├── client/         # 客户端 (TypeScript)
│       └── syntaxes/
│           └── qi.tmLanguage.json # 语法高亮文件
│
├── tests/                  # 测试文件
│   ├── qi_tests/           # Qi 语言特性测试
│   │   ├── hello_world.qi
│   │   └── concurrency.qi
│   ├── compiler_tests/     # 编译器单元测试 (Rust)
│   └── runtime_tests/      # 运行时单元测试 (Rust)
│
└── examples/               # 示例程序
    ├── 猜数字.qi
    └── http服务器.qi
```

---

### 7\. 标准库与包系统

（本节为高级概述，待详细设计）

- **包系统**：基于文件系统。一个目录是一个包。使用 `qimod.json` 管理依赖。
- **标准库 (`stdlib/`)**：将提供 `io` (输入输出)、`collections` (集合)、`threading` (并发与任务)、`net` (网络)、`math` (数学) 等核心模块。
- **实现**：大部分标准库将用 Qi 语言（`.qi`）编写，底层操作（如 I/O）通过 Rust 运行时的 FFI 接口调用 C 系统调用。

---

### 8\. 并发模型 (M:N 协程调度)

Qi 语言采用 M:N 协程调度模型，类似于 Go 语言的 Goroutine 和 Rust 的 `async` 运行时（如 Tokio）。

#### 8.1 核心概念

1.  **任务 (Task)**：Qi 的最小并发单元，等同于一个“协程”或“Goroutine”。这是一个由 Qi 运行时管理的轻量级用户态“线程”。
2.  **原生线程 (OS Thread)**：由操作系统内核调度的标准线程。这些是“重量级”的，且数量受限于系统资源。
3.  **调度器 (Scheduler)**：位于 **Rust 运行时 (`runtime/src/concurrent/`)** 的核心组件。

#### 8.2 M:N 调度模型

- Qi 运行时将在启动时创建一个“原生线程池”，线程数量 (N) 通常等于 CPU 的逻辑核心数。
- 这些原生线程充当“工作线程 (Worker Threads)”。
- 当 Qi 程序员使用 `启动` 关键字时，他们会创建 M 个 `任务`。
- Qi 调度器负责将这 M 个 `任务`（的执行）在 N 个工作线程上进行复用和分发。

#### 8.3 工作流程

1.  **启动**：程序启动时，`runtime/src/concurrent/` (Rust 运行时) 会调用 C 接口 `qi_sys_thread_create` 来创建 N 个工作线程。
2.  **任务创建**：用户代码调用 `启动 函数()`。
    - Qi 编译器将此转换为对 Rust 运行时 `Scheduler::spawn(task)` 的调用。
    - 调度器将这个新 `任务` 放入一个全局或工作线程本地的“待运行队列 (Run Queue)”。
3.  **任务执行**：
    - 工作线程 (N) 不断地从队列中取出 `任务` (M) 并执行它。
4.  **非阻塞 I/O (等待)**：
    - 当一个 `任务` 执行到 `等待` (await) 一个 I/O 操作时（例如 `等待 文件.读取()`），它**不会阻塞**其所在的**原生线程**。
    - 相反，`任务` 会“挂起”(yield)，将其状态保存起来，然后将 I/O 请求提交给运行时（例如 `epoll`, `iocp`, `wasi_poll`)。
    - 工作线程立即丢弃这个挂起的 `任务`，并从队列中获取**下一个** `任务` (M+1) 来执行。
5.  **唤醒 (Wakeup)**：
    - 当 I/O 操作完成时，运行时会通知调度器。
    - 调度器将对应的 `任务` (M) 标记为“可运行”，并将其放回“待运行队列”。
    - 某个空闲的工作线程会（在未来某个时刻）获取并继续执行它。

#### 8.4 平台特定行为 (WASM)

- 如第 5.1 节所述，WASI 平台的原生线程支持尚不成熟。
- 在编译到 WASI 目标时，Rust 运行时将自动检测到 `qi_sys_thread_create` 不可用，并将 N (工作线程数) 设置为 1。
- M:N 调度器将退化为 **M:1 调度器（单线程事件循环）**。
- `启动` 关键字仍然有效，`任务` 仍然会被创建，但所有任务都将在同一个操作系统线程上**并发**（通过 `等待` 切换）执行，而不是**并行**执行。

---

### 9\. 示例代码

（本节为占位符，完整示例见 `examples/` 目录）

---

### 10\. 实现计划

（本节为占位符，待详细规划）

1.  **阶段一：核心编译器**
    - 完成词法分析器、语法分析器。
    - 实现 AST 构建。
    - 实现 LLVM IR 生成（针对基础类型、函数、控制流）。
2.  **阶段二：运行时与平台接口**
    - 实现 C FFI 接口 (`runtime_c_iface`) 针对 POSIX 和 Windows 的实现。
    - 实现 Rust 运行时（基础内存管理、Panic 处理、FFI 绑定）。
3.  **阶段三：高级特性**
    - 实现 M:N 协程调度器 (`启动`, `异步`, `等待`)。
    - 实现泛型、结构体、枚举。
    - 实现标准库 (`io`, `threading`)。
4.  **阶段四：工具链与 Wasm**
    - 开发 `qi-lsp` 语言服务器。
    - 发布 VS Code 语法高亮插件。
    - 实现 `runtime_c_iface` 针对 WASI 的实现，并支持 WASI 目标编译。

---

### 11\. 工具链支持 (LSP 与语法高亮)

为了提供现代化的开发体验，Qi 语言项目将包含 `qi-lsp`（语言服务器）和 TextMate 语法定义。

#### 11.1 语言服务器 (qi-lsp)

`qi-lsp` 是一个独立的二进制文件（位于 `tools/qi-lsp`），它将重用 `compiler` crate 的分析器来提供智能感知。

**支持功能：**

1.  **诊断 (`textDocument/publishDiagnostics`)**：
    - **实现**：实时调用词法和语法分析器，捕获错误并将其作为 `Diagnostic` 发送给编辑器（显示为红色波浪线）。
2.  **代码补全 (`textDocument/completion`)**：
    - **关键字补全**：提供 `函数`, `结构体`, `如果` 等关键字。
    - **变量/函数补全**：分析当前作用域，提供可见的符号。
    - **成员访问补全**：输入 `实例.` 时，列出该 `结构体` 的字段和方法。
3.  **悬停提示 (`textDocument/hover`)**：
    - **实现**：鼠标悬停在符号上时，显示其类型和定义（例如 `变量 计数器: 整数` 或 `函数 签名(...)`）。
4.  **定义跳转 (`textDocument/definition`)**：
    - **实现**：(F12 或 Ctrl+点击) 跳转到变量、函数或类型的定义位置。

#### 11.2 语法高亮 (TextMate)

一个 `qi.tmLanguage.json` 文件（位于 `tools/vscode-qi/syntaxes/`）将使用正则表达式为 Qi 语法提供高亮。

**核心作用域 (Scopes) 定义：**

- `keyword.control.qi`：`如果`, `否则`, `对于`, `当`, `循环`, `返回`
- `keyword.declaration.qi`：`函数`, `结构体`, `枚举`, `常量`, `静态`
- `keyword.concurrency.qi`：`启动`, `异步`, `等待`, `任务`
- `storage.type.primitive.qi`：`整数`, `浮点数`, `字符串`, `布尔`
- `storage.modifier.qi`：`变量`, `不可变`, `公开`, `私有`
- `entity.name.function.qi`：函数名称
- `entity.name.type.struct.qi`：结构体或枚举名称
- `string.quoted.double.qi`：`"..."` 字符串
- `comment.line.qi`：`// ...` 注释
- `comment.block.qi`：`/* ... */` 块注释
- `constant.language.qi`：`真`, `假`, `空指针`
- `support.function.builtin.qi`：`打印`, `输入`
