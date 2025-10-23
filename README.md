# Qi Language Compiler

一个用于 Qi 编程语言的编译器，支持 100% 中文关键字。

## 项目状态

🚧 **正在开发中** - 目前处于设置阶段

## 特性

- ✅ 100% 中文关键字支持
- 🎯 多平台编译 (Linux, Windows, macOS, WebAssembly)
- ⚡ 高性能编译 (<5秒编译时间)
- 🔧 完整的工具链 (编译器、调试器、格式化工具)
- 📚 丰富的标准库
- 🧪 全面的测试覆盖

## 快速开始

### 安装

```bash
# 克隆仓库
git clone https://github.com/qi-lang/qi-compiler.git
cd qi-compiler

# 构建编译器
cargo build --release

# 安装到系统
sudo cp target/release/qi /usr/local/bin/
```

### 第一个程序

创建 `hello.qi` 文件：

```qi
// 你好世界程序
包 主程序;

函数 整数 主程序入口() {
    变量 问候语 = "你好，Qi世界！";
    打印(问候语);
    返回 0;
}
```

编译和运行：

```bash
# 编译程序
qi compile hello.qi --output hello

# 运行程序
./hello
```

## 语言特性

### 基本数据类型

```qi
变量 整数年龄 = 25;           // 整数类型
变量 浮点数圆周率 = 3.14159;  // 浮点数类型
变量 布尔是否成年 = 真;       // 布尔类型 (真/假)
变量 字符串姓名 = "张三";     // 字符串类型
```

### 控制流

```qi
// 条件语句
如果 年龄 >= 18 {
    打印("成年人");
} 否则 {
    打印("未成年");
}

// 循环
变量 计数 = 1;
当 计数 <= 5 {
    打印("计数: {}", 计数);
    计数 = 计数 + 1;
}
```

### 函数定义

```qi
函数 整数 加法(整数 数字1, 整数 数字2) {
    变量 结果 = 数字1 + 数字2;
    返回 结果;
}
```

## 开发

### 构建要求

- Rust 1.75+
- LLVM 15.0+
- C 编译器 (GCC 或 Clang)
- CMake 3.10+ (可选，用于构建运行时库)

### 开发环境设置

```bash
# 安装 Rust 依赖
cargo build

# 运行测试
cargo test

# 运行基准测试
cargo bench

# 检查代码格式
cargo fmt --check

# 运行 linter
cargo clippy
```

### 项目结构

```
├── src/                    # Rust 源代码
│   ├── lexer/             # 词法分析器
│   ├── parser/            # 语法分析器
│   ├── semantic/          # 语义分析器
│   ├── codegen/           # 代码生成器
│   ├── runtime/           # 运行时库接口
│   ├── targets/           # 平台特定代码生成
│   ├── cli/               # 命令行接口
│   └── utils/             # 工具函数
├── runtime/               # C 运行时库
│   ├── include/           # 头文件
│   ├── src/               # C 源代码
│   ├── CMakeLists.txt     # CMake 构建文件
│   └── Makefile          # Make 构建文件
├── tests/                 # 测试文件
├── examples/              # 示例程序
├── docs/                  # 文档
└── build.rs              # 构建脚本
```

### 运行时支持

Qi 编译器包含 Rust + C 实现的运行时，涵盖内存管理、字符串处理、I/O、错误调度等功能。最新增加的异步运行时具备如下特性：

- 基于 Tokio 的多线程任务执行器
- 优先级队列与工作窃取调度策略
- Rust 管理协程与任务状态，C 负责底层系统调用（睡眠、计时、CPU 信息）
- FFI 层抽象 epoll/kqueue/IOCP 等平台事件模型
- 可取消任务、运行时统计、可配置栈池、事件循环适配器

示例代码见 [`examples/async_runtime_demo.rs`](examples/async_runtime_demo.rs)。

## 贡献

欢迎贡献！请查看 [贡献指南](CONTRIBUTING.md) 了解详情。

## 许可证

本项目采用 MIT 许可证 - 查看 [LICENSE](LICENSE) 文件了解详情。

## 联系方式

- 官网: https://qi-lang.org
- GitHub: https://github.com/qi-lang/qi-compiler
- 邮箱: team@qi-lang.org

## 致谢

感谢所有为 Qi 语言做出贡献的开发者和用户！

---

**注意**: 这是一个正在积极开发的项目。某些功能可能尚未完全实现。
