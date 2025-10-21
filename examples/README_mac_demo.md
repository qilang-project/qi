# 🍎 Qi语言macOS演示程序

这是一个完整的Qi语言演示程序，展示了在macOS平台上的各种功能特性。

## 📋 包含文件

- `mac_demo.qi` - 主演示程序源代码
- `mac_demo_instructions.md` - 详细的安装和使用说明
- `compile_mac_demo.sh` - 自动编译脚本
- `README_mac_demo.md` - 本文件

## 🚀 快速运行

### 方法1: 使用编译脚本 (推荐)

```bash
# 运行自动编译脚本
./compile_mac_demo.sh
```

### 方法2: 手动编译

```bash
# 编译为macOS可执行文件
qi compile --target macos --output mac_demo mac_demo.qi

# 运行程序
./mac_demo
```

### 方法3: 使用优化编译

```bash
# 使用标准优化编译
qi compile --target macos -O standard --output mac_demo_optimized mac_demo.qi

# 运行优化版本
./mac_demo_optimized
```

## 🎯 演示内容

程序将展示以下功能：

### 1. 基础语言特性
- ✅ 100%中文关键字
- ✅ 变量声明和类型推断
- ✅ 数学运算和函数调用
- ✅ 数组操作和循环结构
- ✅ 条件判断和递归函数

### 2. macOS平台特性
- ✅ 进程管理 (获取进程ID)
- ✅ 系统时间 (Unix时间戳和Mach时间)
- ✅ CoreFoundation集成
- ✅ 文件系统操作模拟
- ✅ 权限检查功能

### 3. 性能测试
- ✅ 计算密集型操作性能
- ✅ 内存分配和释放性能
- ✅ 函数调用开销测试
- ✅ 执行时间测量

## 📊 预期输出示例

```
=== Qi语言macOS演示程序 ===
Welcome to Qi Language macOS Demo

🔧 基础功能演示:
  消息: 你好，macOS世界！
  年份: 2024
  圆周率: 3.14159
  运行状态: 运行中
  斐波那契(10) = 55
  阶乘(6) = 720
  数组 [1-10] 求和: 55
  数组最大值: 10

🍎 macOS特定功能演示:
  进程ID: 12345
  当前时间戳: 1703123456
  Mach绝对时间: 876543210987
    CF字符串: "Hello from Qi on macOS!"
  CoreFoundation字符串创建成功
  模拟写入文件: /tmp/qi_demo.txt
  文件权限检查: 可访问

⚡ 性能测试:
  计算 100000 次平方和耗时: Xms
  结果: 333328333350000
  创建和处理 1000 个数组耗时: Xms
  计算 20 个斐波那契数列耗时: Xms

=== 程序执行完成 ===
```

## 🔧 自定义和扩展

您可以修改 `mac_demo.qi` 文件来添加自己的功能：

```qi
// 添加新的功能函数
函数 显示系统信息() {
    变量 hostname = 获取主机名();
    打印 "主机名: " + hostname;

    变量 memory = 获取内存使用();
    打印 "内存使用: " + memory + "MB";
}

// 在主函数中调用
函数 主函数() {
    // ... 现有代码 ...

    显示系统信息(); // 添加新功能
}
```

## 🐛 故障排除

### 编译错误
```bash
# 检查语法
qi check mac_demo.qi

# 查看详细错误
qi compile --target macos --verbose mac_demo.qi
```

### 运行时错误
```bash
# 使用调试模式编译
qi compile --target macos --debug-symbols --output mac_demo_debug mac_demo.qi

# 查看详细错误信息
./mac_demo_debug
```

### 性能问题
```bash
# 使用最高优化级别
qi compile --target macos -O maximum --output mac_demo_max mac_demo.qi

# 测量编译时间
time qi compile --target macos mac_demo.qi
```

## 📚 学习资源

- [Qi语言语法参考](../docs/language_reference.md)
- [macOS平台特定功能](../docs/platforms/macos.md)
- [性能优化指南](../docs/performance.md)

## 🤝 贡献

欢迎提交Issue和Pull Request来改进这个演示程序！

---

**享受Qi语言编程的乐趣！** 🚀✨