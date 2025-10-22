#!/bin/bash
# 对比：使用 Runtime vs 不使用 Runtime

echo "========================================"
echo "对比：使用 Qi Runtime 的区别"
echo "========================================"
echo ""

# 创建一个简单的不使用 runtime 的 LLVM IR
cat > /tmp/no_runtime.ll << 'EOF'
; 不使用 Qi Runtime 的版本（使用 printf）
@.str = private unnamed_addr constant [4 x i8] c"%d\0A\00", align 1

declare i32 @printf(ptr, ...)

define i32 @main() {
entry:
  %1 = call i32 (ptr, ...) @printf(ptr @.str, i32 42)
  ret i32 0
}
EOF

echo "1️⃣  不使用 Runtime 的程序"
echo "----------------------------------------"
echo "LLVM IR:"
cat /tmp/no_runtime.ll | sed 's/^/  /'
echo ""

echo "编译..."
clang /tmp/no_runtime.ll -o /tmp/no_runtime_exec 2>/dev/null
no_runtime_size=$(ls -lh /tmp/no_runtime_exec 2>/dev/null | awk '{print $5}')

echo "运行输出:"
/tmp/no_runtime_exec | sed 's/^/  /'
echo ""

echo "符号表（Runtime 相关）:"
runtime_syms=$(nm /tmp/no_runtime_exec 2>/dev/null | grep qi_runtime | wc -l | tr -d ' ')
echo "  qi_runtime 符号数量: $runtime_syms"
echo "  可执行文件大小: $no_runtime_size"
echo ""

echo "2️⃣  使用 Qi Runtime 的程序"
echo "----------------------------------------"
echo "LLVM IR (部分):"
head -20 examples/runtime_test.ll | sed 's/^/  /'
echo "  ..."
echo ""

echo "运行输出:"
/tmp/runtime_test_exec | sed 's/^/  /'
echo ""

echo "符号表（Runtime 相关）:"
runtime_syms=$(nm /tmp/runtime_test_exec 2>/dev/null | grep qi_runtime | wc -l | tr -d ' ')
with_runtime_size=$(ls -lh /tmp/runtime_test_exec 2>/dev/null | awk '{print $5}')
echo "  qi_runtime 符号数量: $runtime_syms"
echo "  可执行文件大小: $with_runtime_size"
echo ""

echo "3️⃣  主要区别"
echo "----------------------------------------"
echo "📌 函数调用："
echo "  不使用 Runtime: printf()"
echo "  使用 Runtime:  qi_runtime_println_int(), qi_runtime_println_float(), etc."
echo ""

echo "📌 生命周期管理："
echo "  不使用 Runtime: 无"
echo "  使用 Runtime:  qi_runtime_initialize() -> ... -> qi_runtime_shutdown()"
echo ""

echo "📌 功能特性："
echo "  不使用 Runtime:"
echo "    - 简单的 C 标准库调用"
echo "    - 无内存管理"
echo "    - 无错误处理"
echo ""
echo "  使用 Runtime:"
echo "    - ✅ Rust 实现的内存管理（引用计数 + GC）"
echo "    - ✅ 中文错误消息支持"
echo "    - ✅ I/O 统计和监控"
echo "    - ✅ 文件系统和网络操作"
echo "    - ✅ 标准库函数（字符串、数学、系统等）"
echo "    - ✅ 为异步/并发提供基础"
echo ""

echo "📌 可执行文件大小："
echo "  不使用 Runtime: $no_runtime_size"
echo "  使用 Runtime:  $with_runtime_size"
echo "  (增加的大小来自完整的 Rust Runtime 实现)"
echo ""

echo "========================================"
echo "✨ 结论"
echo "========================================"
echo "Qi Runtime 提供了完整的运行时支持，"
echo "虽然增加了可执行文件大小，但带来了："
echo "  • 更好的内存管理"
echo "  • 中文原生支持"
echo "  • 完整的标准库"
echo "  • 为未来高级特性奠定基础"
echo ""

# 清理
rm -f /tmp/no_runtime.ll /tmp/no_runtime_exec
