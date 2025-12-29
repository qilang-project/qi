#!/bin/bash

# Script to test all Qi example files
# 这会测试所有Qi示例文件

echo "开始测试所有Qi示例文件..."
echo "================================"

# Create results file
results_file="test_results.txt"
echo "测试结果 - $(date)" > "$results_file"
echo "================================" >> "$results_file"

# Counters
total=0
passed=0
failed=0

# Find all .qi files
find 示例 -name "*.qi" -type f | while read file; do
    total=$((total + 1))
    echo -n "测试: $file ... "

    # Try to run the file
    if timeout 10s cargo run -- run "$file" >/dev/null 2>&1; then
        echo "✓ 通过"
        echo "$file: 通过" >> "$results_file"
        passed=$((passed + 1))
    else
        echo "✗ 失败"
        echo "$file: 失败" >> "$results_file"
        failed=$((failed + 1))

        # Show error details for failed files
        echo "  错误详情:"
        timeout 10s cargo run -- run "$file" 2>&1 | head -5 | sed 's/^/    /'
    fi
done

echo "================================" >> "$results_file"
echo "总计: $total, 通过: $passed, 失败: $failed" >> "$results_file"
echo "测试完成! 结果已保存到 $results_file"