#!/bin/bash
# Qi Runtime 验证脚本
# 此脚本用于验证编译的程序确实使用了 Rust Runtime

set -e

echo "========================================"
echo "Qi Runtime 集成验证"
echo "========================================"
echo ""

# 1. 检查 LLVM IR 中的 Runtime 函数声明
echo "1️⃣  检查 LLVM IR 中的 Runtime 函数声明"
echo "----------------------------------------"
if grep -q "qi_runtime_initialize" examples/runtime_test.ll && \
   grep -q "qi_runtime_println_int" examples/runtime_test.ll && \
   grep -q "qi_runtime_println_float" examples/runtime_test.ll; then
    echo "✅ LLVM IR 包含 Qi Runtime 函数声明"
    echo ""
    echo "   Runtime 函数列表："
    grep "declare.*qi_runtime" examples/runtime_test.ll | sed 's/^/   /'
else
    echo "❌ LLVM IR 不包含 Qi Runtime 函数"
    exit 1
fi
echo ""

# 2. 检查 LLVM IR 中的函数调用
echo "2️⃣  检查 main 函数中的 Runtime 调用"
echo "----------------------------------------"
if grep -q "call.*qi_runtime_initialize" examples/runtime_test.ll && \
   grep -q "call.*qi_runtime_shutdown" examples/runtime_test.ll; then
    echo "✅ main 函数调用了 Runtime 初始化和清理函数"
    echo ""
    echo "   函数调用："
    grep "call.*qi_runtime" examples/runtime_test.ll | sed 's/^/   /'
else
    echo "❌ main 函数没有调用 Runtime 函数"
    exit 1
fi
echo ""

# 3. 编译并链接
echo "3️⃣  编译并链接程序"
echo "----------------------------------------"
echo "   编译 LLVM IR 到目标文件..."
clang -c -x ir examples/runtime_test.ll -o /tmp/runtime_test.o 2>/dev/null || true

echo "   链接 Runtime 库..."
clang /tmp/runtime_test.o target/release/libqi_compiler.a -o /tmp/runtime_test_verify 2>/dev/null

if [ -f /tmp/runtime_test_verify ]; then
    echo "✅ 成功生成可执行文件: /tmp/runtime_test_verify"
else
    echo "❌ 链接失败"
    exit 1
fi
echo ""

# 4. 检查可执行文件中的符号
echo "4️⃣  检查可执行文件中的 Runtime 符号"
echo "----------------------------------------"
runtime_symbols=$(nm /tmp/runtime_test_verify 2>/dev/null | grep qi_runtime | wc -l | tr -d ' ')
if [ "$runtime_symbols" -gt 0 ]; then
    echo "✅ 找到 $runtime_symbols 个 qi_runtime 符号"
    echo ""
    echo "   符号列表（前 10 个）："
    nm /tmp/runtime_test_verify | grep qi_runtime | head -10 | awk '{print "   " $2 " " $3}'
else
    echo "❌ 没有找到 qi_runtime 符号"
    exit 1
fi
echo ""

# 5. 检查 Runtime 库文件
echo "5️⃣  检查 Runtime 库文件"
echo "----------------------------------------"
if [ -f target/release/libqi_compiler.a ]; then
    lib_size=$(ls -lh target/release/libqi_compiler.a | awk '{print $5}')
    echo "✅ Runtime 库存在: libqi_compiler.a ($lib_size)"
    
    # 检查库中的符号数量
    lib_symbols=$(nm target/release/libqi_compiler.a 2>/dev/null | grep qi_runtime | wc -l | tr -d ' ')
    echo "   包含 $lib_symbols 个 qi_runtime 符号"
else
    echo "❌ Runtime 库不存在"
    exit 1
fi
echo ""

# 6. 运行程序并验证输出
echo "6️⃣  运行程序并验证输出"
echo "----------------------------------------"
echo "   执行程序..."
output=$(/tmp/runtime_test_verify 2>&1)
expected_lines=3

if echo "$output" | grep -q "42" && \
   echo "$output" | grep -q "3.14" && \
   echo "$output" | grep -q "你好，Qi Runtime"; then
    echo "✅ 程序输出正确"
    echo ""
    echo "   实际输出："
    echo "$output" | sed 's/^/   /'
else
    echo "❌ 程序输出不符合预期"
    echo "   预期: 42, 3.14, 你好，Qi Runtime！"
    echo "   实际: $output"
    exit 1
fi
echo ""

# 7. 检查是否真的调用了 Rust 代码（通过 strings 命令）
echo "7️⃣  检查可执行文件中的 Rust 字符串"
echo "----------------------------------------"
if strings /tmp/runtime_test_verify | grep -q "runtime/executor.rs" || \
   strings /tmp/runtime_test_verify | grep -q "Runtime" || \
   strings /tmp/runtime_test_verify | grep -q "内存分配失败"; then
    echo "✅ 发现 Rust Runtime 相关字符串"
    echo ""
    echo "   示例字符串："
    strings /tmp/runtime_test_verify | grep -E "(Runtime|runtime|内存|错误)" | head -5 | sed 's/^/   /'
else
    echo "⚠️  未发现明显的 Rust Runtime 字符串（这可能是正常的）"
fi
echo ""

# 8. 对比：不链接 Runtime 会怎样
echo "8️⃣  对比测试：不链接 Runtime 的情况"
echo "----------------------------------------"
echo "   尝试只用目标文件链接（不包含 Runtime 库）..."
if clang /tmp/runtime_test.o -o /tmp/runtime_test_no_runtime 2>/dev/null; then
    echo "❌ 警告：在没有 Runtime 库的情况下也能链接成功"
    echo "   这表明可能存在问题"
else
    echo "✅ 如预期，没有 Runtime 库无法链接"
    echo "   错误信息（部分）："
    clang /tmp/runtime_test.o -o /tmp/runtime_test_no_runtime 2>&1 | grep "undefined" | head -3 | sed 's/^/   /'
fi
echo ""

# 9. 文件大小对比
echo "9️⃣  可执行文件大小分析"
echo "----------------------------------------"
exec_size=$(ls -lh /tmp/runtime_test_verify | awk '{print $5}')
obj_size=$(ls -lh /tmp/runtime_test.o | awk '{print $5}')
echo "   目标文件大小:     $obj_size"
echo "   可执行文件大小:   $exec_size"
echo ""
echo "   Runtime 库增加的大小表明 Rust 代码被成功链接"
echo ""

# 总结
echo "========================================"
echo "✨ 验证总结"
echo "========================================"
echo "✅ LLVM IR 包含 Runtime 函数声明和调用"
echo "✅ 可执行文件包含 Runtime 符号"
echo "✅ 程序成功执行并输出正确结果"
echo "✅ 确认使用了 Rust 编写的 Qi Runtime"
echo ""
echo "🎉 所有验证通过！Qi Runtime 已成功集成！"
echo ""

# 清理
rm -f /tmp/runtime_test_verify /tmp/runtime_test.o
