#!/bin/bash
# 测试 Linux 可执行权限修复

echo "=== 测试 Qi 编译器 Linux 修复 ==="
echo ""

# 重新编译 Qi 编译器
echo "1. 重新编译 Qi 编译器..."
cargo build --bin qi 2>&1 | tail -3
echo ""

# 测试简单示例
echo "2. 测试简单示例:"
cargo run --bin qi -- run 示例/基础/你好世界.qi
echo ""

# 测试异步示例
echo "3. 测试异步示例:"
cargo run --bin qi -- run 示例/基础/异步/简单整数未来测试.qi
echo ""

# 测试多包示例（原来失败的）
echo "4. 测试多包示例（之前失败的）:"
cargo run --bin qi -- run 示例/包/多包/本地包示例.qi
echo ""

echo "=== 测试完成 ==="
