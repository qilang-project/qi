# Qi语言 命令行标准库 设计文档

## 一、设计目标

为Qi语言提供简洁、强大的命令行参数解析库，参考Rust的clap设计理念，但采用更符合中文编程习惯的API。

### 核心原则
1. **中文优先** - 所有API使用中文命名
2. **类型安全** - 参数类型明确，编译期检查
3. **零配置默认** - 自动生成帮助信息、版本信息
4. **渐进式增强** - 从简单到复杂，逐步添加功能
5. **性能优先** - 使用C/Rust实现核心解析逻辑

## 二、核心功能

### 2.1 基础功能
- ✅ 位置参数（positional arguments）
- ✅ 命名选项（named options）`--output`, `-o`
- ✅ 布尔标志（boolean flags）`--verbose`, `-v`
- ✅ 默认值
- ✅ 必需参数 vs 可选参数
- ✅ 参数验证
- ✅ 帮助信息自动生成
- ✅ 版本信息

### 2.2 高级功能
- ✅ 子命令（subcommands）
- ✅ 参数组（argument groups）
- ✅ 多值参数（multiple values）
- ✅ 环境变量回退
- ✅ 自动补全生成（Bash/Zsh/Fish）
- ✅ 拼写建议（Did you mean...?）
- ✅ 彩色输出

## 三、API设计

### 3.1 基本结构

```qi
// 导入命令行模块
导入 标准库.命令行;

函数 入口() {
    // 创建命令行应用
    变量 应用 = 命令行.创建应用("我的工具")
        .版本("1.0.0")
        .作者("张三")
        .关于("一个强大的CLI工具");

    // 添加参数
    应用.添加参数(
        命令行.创建参数("输入文件")
            .简称("i")
            .长名("input")
            .帮助("输入文件路径")
            .必需(真)
    );

    // 解析参数
    变量 匹配结果 = 应用.解析();

    // 获取参数值
    变量 文件: 字符串 = 匹配结果.获取值("输入文件");
    打印行(文件);
}
```

### 3.2 参数类型

#### 位置参数（Positional）
```qi
应用.添加参数(
    命令行.创建参数("文件名")
        .帮助("要处理的文件")
        .索引(1)  // 第一个位置参数
);
```

#### 命名选项（Named Options）
```qi
应用.添加选项(
    命令行.创建选项("输出")
        .短名("o")
        .长名("output")
        .默认值("out.txt")
        .帮助("输出文件路径")
);
```

#### 布尔标志（Flags）
```qi
应用.添加标志(
    命令行.创建标志("详细")
        .短名("v")
        .长名("verbose")
        .帮助("显示详细信息")
);
```

### 3.3 子命令

```qi
// 创建主命令
变量 应用 = 命令行.创建应用("git");

// 添加 "clone" 子命令
变量 克隆命令 = 命令行.创建子命令("clone")
    .关于("克隆仓库");

克隆命令.添加参数(
    命令行.创建参数("仓库地址")
        .必需(真)
);

应用.添加子命令(克隆命令);

// 解析并检查子命令
变量 匹配结果 = 应用.解析();

如果 (匹配结果.包含子命令("clone")) {
    变量 克隆匹配 = 匹配结果.获取子命令("clone");
    变量 仓库: 字符串 = 克隆匹配.获取值("仓库地址");
}
```

### 3.4 验证器

```qi
// 数值范围验证
应用.添加选项(
    命令行.创建选项("端口")
        .短名("p")
        .类型(命令行.类型.整数)
        .范围(1, 65535)
        .默认值("8080")
);

// 文件存在验证
应用.添加参数(
    命令行.创建参数("配置文件")
        .验证器(命令行.验证.文件存在)
);

// 自定义验证器
应用.添加选项(
    命令行.创建选项("邮箱")
        .验证器(函数(值: 字符串) : 布尔 {
            返回 值.包含("@");
        })
);
```

### 3.5 多值参数

```qi
应用.添加选项(
    命令行.创建选项("文件列表")
        .短名("f")
        .多值(真)
        .分隔符(",")
);

// 使用: 程序 -f a.txt,b.txt,c.txt
变量 文件们: 列表<字符串> = 匹配结果.获取多值("文件列表");
```

### 3.6 环境变量回退

```qi
应用.添加选项(
    命令行.创建选项("API密钥")
        .长名("api-key")
        .环境变量("API_KEY")
        .帮助("API密钥（可从 API_KEY 环境变量读取）")
);
```

## 四、FFI接口设计

### 4.1 核心数据结构（C/Rust实现）

```c
// cli_ffi.rs

// 应用句柄
typedef void* QiCliApp;
typedef void* QiCliArg;
typedef void* QiCliMatches;

// 创建应用
QiCliApp qi_cli_create_app(const char* name);
void qi_cli_set_version(QiCliApp app, const char* version);
void qi_cli_set_author(QiCliApp app, const char* author);
void qi_cli_set_about(QiCliApp app, const char* about);

// 参数构建
QiCliArg qi_cli_create_arg(const char* name);
void qi_cli_arg_set_short(QiCliArg arg, const char* short_name);
void qi_cli_arg_set_long(QiCliArg arg, const char* long_name);
void qi_cli_arg_set_help(QiCliArg arg, const char* help_text);
void qi_cli_arg_set_required(QiCliArg arg, int required);
void qi_cli_arg_set_default(QiCliArg arg, const char* default_value);

// 添加参数到应用
void qi_cli_app_add_arg(QiCliApp app, QiCliArg arg);

// 解析参数
QiCliMatches qi_cli_parse_args(QiCliApp app, int argc, char** argv);

// 获取解析结果
const char* qi_cli_get_value(QiCliMatches matches, const char* name);
int qi_cli_get_flag(QiCliMatches matches, const char* name);
int qi_cli_has_value(QiCliMatches matches, const char* name);

// 内存管理
void qi_cli_free_app(QiCliApp app);
void qi_cli_free_matches(QiCliMatches matches);
```

### 4.2 Rust实现（使用clap）

```rust
// src/runtime/stdlib/cli_ffi.rs

use clap::{Arg, ArgMatches, Command};
use std::ffi::{CStr, CString};
use std::os::raw::c_char;

pub struct QiCliApp {
    command: Command,
    args: Vec<Arg>,
}

#[no_mangle]
pub extern "C" fn qi_cli_create_app(name: *const c_char) -> *mut QiCliApp {
    let name_str = unsafe { CStr::from_ptr(name).to_string_lossy().to_string() };
    let app = QiCliApp {
        command: Command::new(name_str),
        args: Vec::new(),
    };
    Box::into_raw(Box::new(app))
}

#[no_mangle]
pub extern "C" fn qi_cli_set_version(app: *mut QiCliApp, version: *const c_char) {
    if app.is_null() || version.is_null() {
        return;
    }
    unsafe {
        let version_str = CStr::from_ptr(version).to_string_lossy().to_string();
        (*app).command = (*app).command.clone().version(version_str);
    }
}

// ... 更多FFI函数实现
```

## 五、类型系统

### 5.1 参数值类型

```qi
命令行.类型.字符串     // 默认
命令行.类型.整数       // i64
命令行.类型.浮点数     // f64
命令行.类型.布尔       // bool
命令行.类型.路径       // 文件路径
```

### 5.2 验证器类型

```qi
命令行.验证.文件存在
命令行.验证.目录存在
命令行.验证.可读文件
命令行.验证.可写目录
命令行.验证.邮箱格式
命令行.验证.URL格式
命令行.验证.IP地址
```

## 六、使用示例

### 6.1 简单示例：文件转换工具

```qi
包 主程序;

导入 标准库.命令行;

函数 入口() {
    变量 应用 = 命令行.创建应用("convert")
        .版本("1.0.0")
        .关于("文件格式转换工具");

    应用.添加参数(
        命令行.创建参数("输入")
            .帮助("输入文件")
            .验证器(命令行.验证.文件存在)
    );

    应用.添加选项(
        命令行.创建选项("输出")
            .短名("o")
            .长名("output")
            .帮助("输出文件")
            .默认值("output.txt")
    );

    应用.添加标志(
        命令行.创建标志("强制")
            .短名("f")
            .长名("force")
            .帮助("覆盖已存在的文件")
    );

    变量 匹配 = 应用.解析();

    变量 输入: 字符串 = 匹配.获取值("输入");
    变量 输出: 字符串 = 匹配.获取值("输出");
    变量 强制: 布尔 = 匹配.获取标志("强制");

    打印("转换 ");
    打印(输入);
    打印(" -> ");
    打印行(输出);

    如果 (强制) {
        打印行("强制覆盖模式");
    }
}
```

### 6.2 复杂示例：包管理器

```qi
包 主程序;

导入 标准库.命令行;

函数 入口() {
    变量 应用 = 命令行.创建应用("qpkg")
        .版本("0.1.0")
        .作者("Qi Team")
        .关于("Qi语言包管理器");

    // 安装子命令
    变量 安装命令 = 命令行.创建子命令("install")
        .别名("i")
        .关于("安装包");

    安装命令.添加参数(
        命令行.创建参数("包名")
            .帮助("要安装的包名")
            .多值(真)
    );

    安装命令.添加标志(
        命令行.创建标志("保存")
            .短名("S")
            .长名("save")
            .帮助("保存到依赖列表")
    );

    // 卸载子命令
    变量 卸载命令 = 命令行.创建子命令("uninstall")
        .别名("un")
        .关于("卸载包");

    卸载命令.添加参数(
        命令行.创建参数("包名")
            .帮助("要卸载的包名")
            .必需(真)
    );

    // 搜索子命令
    变量 搜索命令 = 命令行.创建子命令("search")
        .别名("s")
        .关于("搜索包");

    搜索命令.添加参数(
        命令行.创建参数("关键词")
            .帮助("搜索关键词")
    );

    // 添加子命令
    应用.添加子命令(安装命令);
    应用.添加子命令(卸载命令);
    应用.添加子命令(搜索命令);

    // 解析
    变量 匹配 = 应用.解析();

    // 处理子命令
    如果 (匹配.包含子命令("install")) {
        变量 安装匹配 = 匹配.获取子命令("install");
        变量 包列表: 列表<字符串> = 安装匹配.获取多值("包名");
        变量 保存: 布尔 = 安装匹配.获取标志("保存");

        打印("安装包: ");
        打印行(包列表);
    } 否则 如果 (匹配.包含子命令("uninstall")) {
        变量 卸载匹配 = 匹配.获取子命令("uninstall");
        变量 包名: 字符串 = 卸载匹配.获取值("包名");

        打印("卸载包: ");
        打印行(包名);
    } 否则 如果 (匹配.包含子命令("search")) {
        变量 搜索匹配 = 匹配.获取子命令("search");
        变量 关键词: 字符串 = 搜索匹配.获取值("关键词");

        打印("搜索包: ");
        打印行(关键词);
    } 否则 {
        打印行("请使用 --help 查看帮助信息");
    }
}
```

## 七、自动生成的帮助信息

### 7.1 主命令帮助

```bash
$ convert --help

convert 1.0.0
文件格式转换工具

用法：
    convert [选项] <输入>

参数：
    <输入>    输入文件

选项：
    -o, --output <输出>    输出文件 [默认: output.txt]
    -f, --force            覆盖已存在的文件
    -h, --help             显示此帮助信息
    -V, --version          显示版本信息
```

### 7.2 子命令帮助

```bash
$ qpkg --help

qpkg 0.1.0
Qi Team
Qi语言包管理器

用法：
    qpkg <子命令>

子命令：
    install, i     安装包
    uninstall, un  卸载包
    search, s      搜索包
    help           显示此帮助信息或子命令帮助

选项：
    -h, --help       显示帮助信息
    -V, --version    显示版本信息
```

## 八、实现路径

### 阶段1：基础实现（第1周）
- [ ] 实现C/Rust FFI层（基于clap）
- [ ] 注册到模块系统
- [ ] 生成LLVM IR声明
- [ ] 基础参数解析（位置参数、选项、标志）

### 阶段2：高级功能（第2周）
- [ ] 子命令支持
- [ ] 参数验证器
- [ ] 多值参数
- [ ] 环境变量回退

### 阶段3：完善与优化（第3周）
- [ ] 自动补全生成
- [ ] 彩色输出
- [ ] 拼写建议
- [ ] 完整文档和示例

## 九、依赖项

### Rust依赖（Cargo.toml）
```toml
[dependencies]
clap = { version = "4.5", features = ["cargo", "env", "suggestions", "color"] }
```

### 目标文件大小估算
- FFI层 + clap静态链接：约 300-500KB
- 对比：操作系统模块约 50KB

## 十、与现有模块对比

| 特性 | 操作系统 | HTTP | 命令行 |
|-----|---------|------|--------|
| FFI函数数 | 18 | ~25 | ~40 |
| 依赖 | std::env | reqwest | clap |
| 复杂度 | 简单 | 中等 | 中高 |
| 二进制大小 | 小 | 大 | 中 |

## 十一、测试计划

### 单元测试
- [ ] 参数解析正确性
- [ ] 默认值处理
- [ ] 验证器功能
- [ ] 错误处理

### 集成测试
- [ ] 简单CLI工具
- [ ] 带子命令的工具
- [ ] 复杂参数组合
- [ ] 帮助信息生成

### 示例程序
- [ ] 文件转换工具（convert）
- [ ] 包管理器（qpkg）
- [ ] 文本处理工具（grep-like）
- [ ] 服务器启动工具（server）

## 十二、文档要求

- [ ] API参考文档（中文）
- [ ] 快速入门教程
- [ ] 常见用例
- [ ] 最佳实践
- [ ] 故障排查指南

---

**设计版本**: v1.0
**创建日期**: 2025-01-XX
**状态**: 设计阶段 📋
