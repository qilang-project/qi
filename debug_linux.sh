#!/bin/bash
# Linux 调试脚本 - 检查 Qi 编译问题

echo "=== Qi Linux 调试信息 ==="
echo ""

# 1. 系统信息
echo "1. 系统信息:"
uname -a
echo ""

# 2. 检查必要工具
echo "2. 检查编译工具:"
which clang && clang --version | head -1 || echo "❌ clang 未安装"
which cargo && cargo --version || echo "❌ cargo 未安装"
echo ""

# 3. 编译简单示例并保留中间文件
echo "3. 编译测试（保留中间文件）:"
TEST_FILE="示例/基础/你好世界.qi"

if [ -f "$TEST_FILE" ]; then
    echo "   源文件: $TEST_FILE"

    # 使用 verbose 模式编译
    RUST_BACKTRACE=1 cargo run --bin qi -- run "$TEST_FILE" 2>&1 | tee /tmp/qi_debug.log

    echo ""
    echo "4. 检查生成的文件:"
    ls -lah 示例/基础/你好世界* 2>/dev/null || echo "   没有找到生成的文件"

    echo ""
    echo "5. 检查当前目录的可执行文件:"
    find . -maxdepth 1 -type f -executable -newer /tmp/qi_debug.log 2>/dev/null

    echo ""
    echo "6. 检查 .ll 和 .o 文件:"
    find 示例/ -name "*.ll" -o -name "*.o" | head -10

else
    echo "   ❌ 测试文件不存在: $TEST_FILE"
fi

echo ""
echo "=== 调试完成 ==="
echo "日志已保存到: /tmp/qi_debug.log"
