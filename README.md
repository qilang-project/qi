# Qi 编程语言编译器

一个用于 Qi 编程语言的编译器，支持 100% 中文关键字。

## 项目状态

🚧 **正在开发中** - 核心编译器和运行时正在实现

## 特性

- ✅ **100% 中文关键字支持** - 完全使用中文编程
- 🎯 **多平台编译** - 支持 Linux, Windows, macOS, WebAssembly
- ⚡ **高性能编译** - 基于 LLVM 的优化编译
- 🔧 **完整的工具链** - 编译器、运行时、标准库
- 📚 **现代语言特性** - 结构体、枚举、泛型、模式匹配
- 🔄 **M:N 协程调度** - 类似 Go 的轻量级并发模型
- 🧪 **全面的测试覆盖** - 单元测试和集成测试

## 文档

- 📖 **[统一设计文档](docs/qi-unified-design/README.md)** - 完整的语言设计规范
- 📚 **[示例程序](examples/qi/)** - 中文代码示例
- 🔧 **[开发指南](CLAUDE.md)** - 技术栈和开发指导

## 快速开始

### 安装

```bash
# 克隆仓库
git clone https://github.com/qi-lang/qi-compiler.git
cd qi-compiler

# 构建编译器
cargo build --release

# 安装到系统（可选）
sudo cp target/release/qi /usr/local/bin/
```

### 第一个程序

创建 `你好世界.qi` 文件：

```qi
// 你好世界程序
包 主程序;

函数 入口() {
    打印("欢迎使用 Qi 编程语言！");
    打印("这是一个完全使用中文关键字的现代编程语言。");

    变量 问候语 = "你好，世界！";
    打印(问候语);
}
```

编译和运行（开发中）：

```bash
# 检查语法
qi check 你好世界.qi

# 编译程序（即将支持）
qi compile 你好世界.qi --output 你好世界

# 运行程序
./你好世界
```

## 语言特性

### 基本数据类型

```qi
// 变量声明
变量 年龄 = 25;              // 整数类型（自动推断）
变量 圆周率 = 3.14159;       // 浮点数类型
变量 是否成年 = 真;           // 布尔类型 (真/假)
变量 姓名 = "张三";           // 字符串类型
```

### 控制流

```qi
// 条件语句
如果 年龄 >= 18 {
    打印("成年人");
} 否则 如果 年龄 >= 13 {
    打印("青少年");
} 否则 {
    打印("儿童");
}

// 循环
变量 计数 = 1;
当 计数 <= 5 {
    打印("计数: {}", 计数);
    计数 = 计数 + 1;
}

// For 循环
对于 数字 在 数组 {
    打印("数字: {}", 数字);
}
```

### 函数定义

```qi
// 基础函数
函数 加法(a: 整数, b: 整数): 整数 {
    返回 a + b;
}

// 函数调用
函数 入口() {
    变量 结果 = 加法(5, 3);
    打印(结果);
}
```

### 更多类型

Qi 编译器支持多种数据类型：

```qi
// 整数类型
变量 整数 = 42;
变量 长整数 = 1000000000;

// 浮点数类型
变量 浮点数 = 3.14;

// 布尔类型
变量 布尔值 = 真;
变量 假值 = 假;

// 字符串类型
变量 文本 = "Hello Qi";
```

### 异步编程

```qi
// 异步函数
异步 函数 网络请求(): 字符串 {
    返回 "网络数据";
}

异步 函数 获取数据(): 字符串 {
    变量 结果 = 等待 网络请求();
    返回 结果;
}

函数 入口() {
    变量 数据 = 等待 获取数据();
    打印(数据);
}
```

## 示例程序

查看 `examples/basic/` 目录获取完整的中文示例程序：

### 基础示例
1. **[hello_world](examples/basic/hello_world/)** - 最简单的 Qi 程序
   ```bash
   cargo run -- run examples/basic/hello_world/hello_world.qi
   ```

2. **[showcase](examples/basic/showcase/)** - 编译器功能完整演示
   ```bash
   cargo run -- run examples/basic/showcase/showcase.qi
   ```

### 语法特性示例
3. **[calculations](examples/basic/calculations/)** - 数学计算和运算符
4. **[numbers](examples/basic/numbers/)** - 数字类型和操作
5. **[type](examples/basic/type/)** - 类型系统演示
6. **[function](examples/basic/function/)** - 函数定义和调用

### 控制流示例
7. **[branching](examples/basic/branching/)** - 条件语句和分支
8. **[loop](examples/basic/loop/)** - 循环结构
9. **[for_loop](examples/basic/for_loop/)** - For 循环语句
10. **[control_flow](examples/basic/control_flow/)** - 综合控制流

### 高级特性示例
11. **[async](examples/basic/async/)** - 异步编程支持
12. **[struct](examples/basic/struct/)** - 结构体定义
13. **[multi_file](examples/basic/multi_file/)** - 多文件项目演示
14. **[greet](examples/basic/greet/)** - 交互式程序

运行示例：

```bash
# 运行单个示例
cargo run -- run examples/basic/hello_world/hello_world.qi

# 检查语法
cargo run -- check examples/basic/showcase/showcase.qi

# 编译到 LLVM IR
cargo run -- compile examples/basic/calculations/calculations.qi -o output.ll
```

## 开发

### 构建要求

- **Rust** 1.75+
- **LLVM** 15.0+ （可选，用于代码生成）
- **C 编译器** (GCC 或 Clang)
- **CMake** 3.10+ （可选，用于构建运行时）

### 开发环境设置

```bash
# 安装 Rust 依赖
cargo build

# 运行测试
cargo test

# 运行示例
cargo run -- run examples/basic/hello_world/hello_world.qi

# 检查语法
cargo run -- check examples/basic/showcase/showcase.qi

# 编译到 LLVM IR
cargo run -- compile examples/basic/calculations/calculations.qi -o test.ll

# 检查代码格式
cargo fmt --check

# 运行 linter
cargo clippy
```

### 项目结构

```
qi-compiler/
├── src/                      # Rust 编译器源码
│   ├── lexer/               # 词法分析器（支持中文关键字）
│   ├── parser/              # 语法分析器（LALRPOP）
│   ├── semantic/            # 语义分析器
│   ├── codegen/             # 代码生成器（LLVM）
│   ├── runtime/             # Rust 运行时
│   │   ├── async_runtime/  # 异步运行时（M:N 调度）
│   │   ├── memory/         # 内存管理
│   │   ├── io/             # I/O 接口
│   │   └── stdlib/         # 标准库绑定
│   ├── targets/            # 目标平台支持
│   ├── cli/                # 命令行接口
│   └── utils/              # 工具函数
├── tests/                   # 测试文件
├── examples/                # 示例程序
│   └── basic/              # 基础示例
│       ├── hello_world/    # Hello World 程序
│       ├── showcase/       # 功能演示
│       ├── calculations/   # 数学计算
│       ├── async/          # 异步编程
│       ├── function/       # 函数定义
│       ├── struct/         # 结构体
│       ├── branching/      # 分支结构
│       ├── loop/           # 循环结构
│       ├── multi_file/     # 多文件项目
│       └── ...            # 更多示例
├── scripts/                 # 构建和运行脚本
│   ├── run_examples.sh     # 示例运行脚本
│   └── run_examples.ps1    # PowerShell 脚本
├── docs/                    # 文档
│   └── qi-unified-design/   # 统一设计文档（分章版）
└── build.rs                # 构建脚本（LALRPOP）
```

### 运行时支持

Qi 编译器包含 Rust 实现的运行时，提供：

#### 核心功能
- **内存管理** - 垃圾回收（GC）和引用计数
- **字符串处理** - UTF-8 中文字符串支持
- **I/O 系统** - 文件操作、网络、标准输入输出
- **错误处理** - 结构化错误和异常处理

#### 异步运行时（M:N 协程）
- **任务调度器** - 基于 Tokio 的多线程执行器
- **工作窃取** - 高效的任务分发算法
- **优先级队列** - 任务优先级管理
- **取消支持** - 可取消的异步任务
- **运行时统计** - 性能监控和指标

示例代码：
- Rust 示例：[`examples/async_runtime_demo.rs`](examples/async_runtime_demo.rs)
- Qi 示例：[`examples/qi/异步并发示例.qi`](examples/qi/异步并发示例.qi)

## 语言关键字

Qi 语言使用 100% 中文关键字。核心关键字包括：

| 分类     | 关键字                                     |
| -------- | ------------------------------------------ |
| 程序结构 | 包、导入、公开、私有、常量、静态           |
| 变量     | 变量、不可变、类型                         |
| 数据类型 | 结构体、枚举、联合体、特性、实现           |
| 控制流   | 如果、否则、匹配、循环、当、对于、中断、继续、返回 |
| 函数     | 函数、异步、等待、闭包                     |
| 并发     | 启动、任务、线程、锁、原子                 |
| 类型     | 整数、浮点数、字符串、布尔、数组、列表、字典、集合 |
| 值       | 真、假、空指针                             |
| 内存     | 拥有、借用、移动、克隆、释放、新建         |
| 错误     | 抛出、捕获、尝试、结果、选项               |

完整关键字列表见 [统一设计文档](docs/qi-unified-design/02-language-reference.md)。

## 贡献

欢迎贡献！请遵循以下步骤：

1. Fork 本仓库
2. 创建特性分支 (`git checkout -b feature/新特性`)
3. 提交更改 (`git commit -m '添加新特性'`)
4. 推送到分支 (`git push origin feature/新特性`)
5. 创建 Pull Request

请确保：
- 代码通过所有测试 (`cargo test`)
- 代码符合格式规范 (`cargo fmt`)
- 代码通过 linter 检查 (`cargo clippy`)
- 添加必要的文档和注释

## 路线图

### 阶段一：核心编译器 ✅ (进行中)
- [x] 词法分析器（中文关键字支持）
- [x] 语法分析器（LALRPOP）
- [ ] 语义分析器
- [ ] LLVM IR 生成

### 阶段二：运行时与平台接口 ✅ (进行中)
- [x] Rust 运行时框架
- [x] 异步运行时（M:N 调度器）
- [x] 内存管理接口
- [ ] 平台抽象层（C FFI）

### 阶段三：高级特性 (待实现)
- [ ] 完整的中文语法支持
- [ ] 泛型系统
- [ ] 标准库实现
- [ ] 错误处理系统

### 阶段四：工具链 (待实现)
- [ ] LSP 语言服务器
- [ ] VS Code 插件
- [ ] 调试器支持
- [ ] 文档生成器

## 许可证

本项目采用 MIT 许可证 - 查看 [LICENSE](LICENSE) 文件了解详情。

## 联系方式

- **官网**: https://qi-lang.org
- **GitHub**: https://github.com/qi-lang/qi-compiler
- **邮箱**: team@qi-lang.org
- **社区**: https://community.qi-lang.org

## 致谢

感谢所有为 Qi 语言做出贡献的开发者和用户！

特别感谢：
- Rust 社区提供的优秀工具和库
- LLVM 项目提供的编译器基础设施
- LALRPOP 解析器生成器
- Tokio 异步运行时

---

**注意**: 这是一个正在积极开发的项目。某些功能可能尚未完全实现。欢迎试用并提供反馈！

**版权所有 © 2025 Qi 语言团队**
