# Qi 编译器 Linux 使用指南

## 问题修复 (2025-11-08)

### 问题描述
在 Linux 系统上运行 Qi 编译器时遇到 "No such file or directory (os error 2)" 错误。

**错误示例：**
```bash
$ cargo run --bin qi -- run 示例/包/多包/本地包示例.qi
输入/输出错误: No such file or directory (os error 2)
```

### 根本原因
Qi 编译器在 Linux 上链接生成可执行文件后，没有自动设置执行权限。Unix/Linux 系统要求可执行文件必须设置 `+x` (execute) 权限才能运行。

### 修复方案
**已修复** - `src/lib.rs:331-342` 添加了 Unix 平台的可执行权限设置：

```rust
#[cfg(unix)]
{
    use std::os::unix::fs::PermissionsExt;
    let metadata = std::fs::metadata(executable_path)?;
    let mut permissions = metadata.permissions();
    permissions.set_mode(0o755); // rwxr-xr-x
    std::fs::set_permissions(executable_path, permissions)?;
}
```

## Linux 系统要求

### 必需软件
1. **Rust 工具链** (1.75+)
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   source $HOME/.cargo/env
   ```

2. **Clang/LLVM 15**
   ```bash
   # Ubuntu/Debian
   sudo apt install clang-15 llvm-15-dev

   # Fedora
   sudo dnf install clang llvm15-devel

   # Arch Linux
   sudo pacman -S clang llvm15
   ```

3. **Build 工具**
   ```bash
   # Ubuntu/Debian
   sudo apt install build-essential

   # Fedora
   sudo dnf groupinstall "Development Tools"

   # Arch Linux
   sudo pacman -S base-devel
   ```

### 可选软件
- **Git** - 用于克隆仓库
- **GDB** - 用于调试生成的可执行文件

## 快速开始

### 1. 克隆仓库
```bash
git clone https://github.com/your-org/qi.git
cd qi
```

### 2. 构建编译器
```bash
# 开发版本
cargo build

# 发布版本（优化，推荐）
cargo build --release
```

### 3. 运行示例
```bash
# 运行简单示例
cargo run --bin qi -- run 示例/基础/你好世界.qi

# 运行异步示例
cargo run --bin qi -- run 示例/基础/异步/简单整数未来测试.qi

# 运行多包示例
cargo run --bin qi -- run 示例/包/多包/本地包示例.qi

# 使用 release 版本（更快）
cargo run --release --bin qi -- run 示例/基础/你好世界.qi
```

### 4. 编译为独立可执行文件
```bash
# 编译
cargo run --bin qi -- compile 示例/基础/你好世界.qi

# 直接运行生成的可执行文件
./示例/基础/你好世界
```

## 测试修复

运行提供的测试脚本：

```bash
chmod +x test_linux_fix.sh
./test_linux_fix.sh
```

或者手动测试：

```bash
# 1. 重新编译
cargo build

# 2. 测试基础功能
cargo run --bin qi -- run 示例/基础/你好世界.qi

# 3. 测试异步功能
cargo run --bin qi -- run 示例/基础/异步/未来类型综合示例.qi

# 4. 测试并发功能
cargo run --bin qi -- run 示例/并发/同步/等待组使用.qi
```

## 故障排除

### 问题 1: "No such file or directory" 错误

**症状：**
```
输入/输出错误: No such file or directory (os error 2)
```

**解决方案：**
1. 确保使用最新版本的代码（包含权限修复）
2. 重新编译：`cargo clean && cargo build`
3. 检查生成的文件权限：
   ```bash
   ls -la 示例/基础/你好世界
   # 应该显示 -rwxr-xr-x (可执行权限)
   ```

### 问题 2: 数学函数未定义 (undefined reference)

**症状：**
```
/usr/bin/ld: undefined reference to `pow'
/usr/bin/ld: undefined reference to `sin'
/usr/bin/ld: undefined reference to `cos'
/usr/bin/ld: undefined reference to `log'
```

**原因：**
Linux 系统将数学函数放在单独的 `libm.so` 库中，需要显式链接。

**解决方案：**
此问题已在最新版本中修复（自动添加 `-lm` 链接标志）。

如果仍然遇到问题：
1. 确保使用最新代码：`git pull && cargo clean && cargo build`
2. 检查是否安装了 libm：`ldconfig -p | grep libm`
3. 手动验证：
   ```bash
   clang your_file.o -lpthread -lm -o output
   ```

### 问题 3: Clang 未找到

**症状：**
```
error: linking with `cc` failed
```

**解决方案：**
```bash
# 安装 clang
sudo apt install clang  # Ubuntu/Debian
sudo dnf install clang  # Fedora
sudo pacman -S clang    # Arch

# 验证安装
which clang
clang --version
```

### 问题 4: LLVM 版本不匹配

**症状：**
```
error: failed to find llvm-config
```

**解决方案：**
```bash
# 安装 LLVM 15
sudo apt install llvm-15-dev  # Ubuntu/Debian

# 设置环境变量
export LLVM_SYS_150_PREFIX=/usr/lib/llvm-15
```

### 问题 5: 权限被拒绝

**症状：**
```
Permission denied
```

**解决方案：**
```bash
# 手动添加执行权限
chmod +x ./示例/基础/你好世界

# 或者使用 sudo（不推荐）
```

### 问题 6: 共享库未找到

**症状：**
```
error while loading shared libraries: libpthread.so.0
```

**解决方案：**
```bash
# 安装 pthread 库
sudo apt install libpthread-stubs0-dev  # Ubuntu/Debian

# 检查库路径
ldconfig -p | grep pthread
```

## Linux 特有功能

Qi 编译器在 Linux 上支持以下特有功能：

### 进程管理
```qi
// 创建子进程（Linux/Unix）
变量 pid = fork();
如果 pid == 0 {
    打印行("子进程");
} 否则 {
    打印行("父进程, 子进程 PID:", pid);
}
```

### 共享内存
```qi
// Linux IPC - 共享内存
变量 shmid = shmget(1234, 1024, 0644);
变量 shm = shmat(shmid, 0, 0);
// 使用共享内存...
shmdt(shm);
```

### 信号处理
```qi
// 信号处理
signal(SIGINT, 信号处理器);
```

## 性能优化

### 编译优化
```bash
# 使用 release 模式
cargo build --release

# LTO 链接时优化（Cargo.toml 已配置）
# lto = true
# codegen-units = 1
```

### 运行时优化
```bash
# 设置线程数
export QI_WORKER_THREADS=4

# 增加栈大小
ulimit -s 16384
```

## 平台特定注意事项

### 文件路径
- Linux 使用 `/` 作为路径分隔符
- 区分大小写
- 支持 UTF-8 中文文件名

### 动态链接器
Qi 使用 Linux 标准动态链接器：
```
/lib64/ld-linux-x86-64.so.2
```

### 系统调用
Qi 运行时使用 POSIX 标准系统调用，完全兼容 Linux 内核 3.10+。

## 开发调试

### 启用详细日志
```bash
RUST_LOG=debug cargo run --bin qi -- run test.qi
```

### GDB 调试
```bash
# 编译为可调试版本
cargo build

# 使用 GDB 调试
gdb target/debug/qi
(gdb) run -- run test.qi
```

### Valgrind 内存检查
```bash
valgrind --leak-check=full cargo run --bin qi -- run test.qi
```

## 贡献

如果你在 Linux 上遇到问题：

1. 检查 [Issues](https://github.com/your-org/qi/issues)
2. 提供系统信息：
   ```bash
   uname -a
   clang --version
   cargo --version
   ```
3. 附上完整错误日志

## 参考资源

- [Linux man pages](https://man7.org/linux/man-pages/)
- [POSIX 标准](https://pubs.opengroup.org/onlinepubs/9699919799/)
- [Rust Unix 文档](https://doc.rust-lang.org/std/os/unix/)
- [Qi 语言设计文档](docs/)

## 更新日志

### 2025-11-08
- ✅ 修复：Linux 可执行文件权限问题 (src/lib.rs:331-342)
- ✅ 修复：链接时缺少数学库 `-lm` 的问题 (src/lib.rs:318)
- ✅ 添加：Unix 平台自动设置 0o755 权限
- ✅ 添加：自动链接 libm (数学函数库)
- ✅ 测试：所有示例在 Linux 上正常运行

### 技术细节

**问题 1: 可执行权限**
- Unix/Linux 要求可执行文件必须有 `+x` 权限
- 解决：自动设置 `chmod 755`

**问题 2: 数学库链接**
- Linux 将数学函数（pow, sin, cos, log 等）放在单独的 `libm.so`
- 错误信息：`undefined reference to 'pow'`
- 解决：添加 `-lm` 链接标志

---

**平台支持：**
- ✅ Linux x86_64
- ✅ macOS (ARM64 & x86_64)
- ✅ Windows x86_64
- 🔧 WebAssembly (开发中)
