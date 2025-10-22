#!/bin/bash
# 快速验证：显示程序是否使用了 Runtime

echo "🔍 快速验证 Qi Runtime 使用情况"
echo ""

# 检查 LLVM IR
echo "📄 LLVM IR 中的 Runtime 调用："
grep "call.*qi_runtime" examples/runtime_test.ll 2>/dev/null || echo "  未找到 LLVM IR 文件"
echo ""

# 编译并检查符号
if [ ! -f /tmp/runtime_test_exec ]; then
    echo "⚙️  正在编译..."
    clang -c -x ir examples/runtime_test.ll -o /tmp/runtime_test.o 2>/dev/null
    clang /tmp/runtime_test.o target/release/libqi_compiler.a -o /tmp/runtime_test_exec 2>/dev/null
fi

echo "🔤 可执行文件中的 Runtime 符号（前 5 个）："
nm /tmp/runtime_test_exec 2>/dev/null | grep qi_runtime | head -5 | awk '{print "  " $3}'
echo ""

echo "▶️  运行程序："
/tmp/runtime_test_exec
echo ""

echo "📊 统计："
runtime_count=$(nm /tmp/runtime_test_exec 2>/dev/null | grep qi_runtime | wc -l | tr -d ' ')
echo "  Runtime 函数数量: $runtime_count"
file_size=$(ls -lh /tmp/runtime_test_exec 2>/dev/null | awk '{print $5}')
echo "  可执行文件大小: $file_size"
echo ""

if [ "$runtime_count" -gt 0 ]; then
    echo "✅ 确认：程序使用了 Rust Runtime！"
else
    echo "❌ 警告：未检测到 Runtime 函数"
fi
