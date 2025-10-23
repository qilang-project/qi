#!/bin/bash
# IO 测试套件运行脚本

echo "========================================="
echo "Qi 语言 IO 测试套件"
echo "========================================="
echo ""

# 测试文件列表
tests=(
    "examples/runtime/文件操作.qi"
    "examples/runtime/多文件操作.qi"
    "examples/runtime/文件性能测试.qi"
    "examples/runtime/中文文件测试.qi"
    "examples/runtime/文件边界测试.qi"
)

passed=0
failed=0

for test in "${tests[@]}"; do
    echo "运行测试: $test"
    echo "-----------------------------------"
    
    if cargo run --quiet -- run "$test"; then
        ((passed++))
        echo "✓ 测试通过"
    else
        ((failed++))
        echo "✗ 测试失败"
    fi
    
    echo ""
    echo ""
done

echo "========================================="
echo "测试总结"
echo "========================================="
echo "通过: $passed"
echo "失败: $failed"
echo "总计: $((passed + failed))"

if [ $failed -eq 0 ]; then
    echo ""
    echo "🎉 所有测试通过！"
    exit 0
else
    echo ""
    echo "⚠️  有测试失败"
    exit 1
fi
