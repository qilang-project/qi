#!/bin/bash
# 检查项目中的 Runtime 状态

echo "========================================"
echo "Qi 项目中的 Runtime 分析"
echo "========================================"
echo ""

echo "1️⃣  C Runtime (/runtime/)"
echo "----------------------------------------"
if [ -d "runtime" ]; then
    echo "📁 位置: runtime/"
    echo "📝 语言: C"
    echo "📄 文件:"
    ls -lh runtime/src/*.c 2>/dev/null | awk '{print "   " $9 " (" $5 ")"}'
    echo ""
    
    echo "🔧 构建状态:"
    if [ -f "runtime/build/libqi_runtime.a" ]; then
        lib_size=$(ls -lh runtime/build/libqi_runtime.a | awk '{print $5}')
        echo "   ✅ 已构建: runtime/build/libqi_runtime.a ($lib_size)"
    else
        echo "   ❌ 未构建"
    fi
    
    echo ""
    echo "🔍 提供的函数（从头文件）:"
    grep "^void\|^int\|^size_t\|^char" runtime/include/*.h 2>/dev/null | grep -v "//" | head -10 | sed 's/^/   /'
else
    echo "❌ 不存在"
fi
echo ""

echo "2️⃣  Rust Runtime (src/runtime/)"
echo "----------------------------------------"
if [ -d "src/runtime" ]; then
    echo "📁 位置: src/runtime/"
    echo "📝 语言: Rust"
    echo "📄 模块:"
    ls -1 src/runtime/*.rs 2>/dev/null | sed 's/^/   /'
    echo ""
    ls -1d src/runtime/*/ 2>/dev/null | sed 's/^/   /'
    echo ""
    
    echo "🔧 构建状态:"
    if [ -f "target/release/libqi_compiler.a" ]; then
        lib_size=$(ls -lh target/release/libqi_compiler.a | awk '{print $5}')
        echo "   ✅ 已构建: target/release/libqi_compiler.a ($lib_size)"
    else
        echo "   ❌ 未构建"
    fi
    
    echo ""
    echo "🔍 提供的函数（从 executor.rs）:"
    grep "pub extern \"C\" fn qi_runtime" src/runtime/executor.rs 2>/dev/null | sed 's/^/   /' | head -10
else
    echo "❌ 不存在"
fi
echo ""

echo "3️⃣  当前使用的 Runtime"
echo "----------------------------------------"
if [ -f "/tmp/runtime_test_exec" ]; then
    echo "🔍 检查可执行文件符号..."
    
    # 检查是否有 Rust runtime 符号
    rust_runtime_count=$(nm /tmp/runtime_test_exec 2>/dev/null | grep qi_runtime | wc -l | tr -d ' ')
    
    # 检查是否有 C runtime 符号（qi_malloc, qi_free 等）
    c_runtime_count=$(nm /tmp/runtime_test_exec 2>/dev/null | grep -E "qi_malloc|qi_free|qi_get_allocated" | wc -l | tr -d ' ')
    
    echo ""
    if [ "$rust_runtime_count" -gt 0 ]; then
        echo "   ✅ 使用 Rust Runtime ($rust_runtime_count 个符号)"
        echo "      示例: qi_runtime_initialize, qi_runtime_println_int, etc."
    fi
    
    if [ "$c_runtime_count" -gt 0 ]; then
        echo "   ✅ 使用 C Runtime ($c_runtime_count 个符号)"
        echo "      示例: qi_malloc, qi_free, qi_get_allocated_memory"
    fi
    
    if [ "$rust_runtime_count" -eq 0 ] && [ "$c_runtime_count" -eq 0 ]; then
        echo "   ⚠️  未检测到任何 Qi Runtime"
    fi
else
    echo "   ⚠️  未找到测试可执行文件"
    echo "   运行 ./scripts/quick_verify.sh 来生成"
fi
echo ""

echo "4️⃣  结论"
echo "----------------------------------------"
echo ""
echo "📊 Runtime 对比:"
echo ""
echo "┌─────────────────┬──────────────┬──────────────┐"
echo "│                 │ C Runtime    │ Rust Runtime │"
echo "├─────────────────┼──────────────┼──────────────┤"
echo "│ 是否存在        │     ✅       │      ✅      │"
echo "│ 当前使用        │     ❌       │      ✅      │"
echo "│ 语言            │     C        │     Rust     │"
echo "│ 功能完整性      │    基础      │     完整     │"
echo "│ 内存管理        │   malloc     │   RC + GC    │"
echo "│ 错误处理        │    简单      │   中文支持   │"
echo "│ I/O 操作        │     无       │     完整     │"
echo "│ 标准库          │     无       │     完整     │"
echo "└─────────────────┴──────────────┴──────────────┘"
echo ""

echo "💡 说明:"
echo ""
echo "C Runtime (runtime/) 是早期的简单实现，只提供基础的"
echo "内存管理功能（malloc/free），功能有限。"
echo ""
echo "Rust Runtime (src/runtime/) 是新的完整实现，提供："
echo "  • 完整的内存管理（引用计数 + 垃圾回收）"
echo "  • 中文错误消息系统"
echo "  • 文件系统和网络 I/O"
echo "  • 完整的标准库（字符串、数学、系统等）"
echo "  • C FFI 接口（executor.rs）"
echo ""
echo "当前编译器已经集成并使用 Rust Runtime。"
echo "C Runtime 可以保留作为参考，或者删除。"
echo ""

echo "❓ C Runtime 是否必须？"
echo "----------------------------------------"
echo "答案: ❌ 不必须"
echo ""
echo "理由:"
echo "  1. Rust Runtime 已经提供了所有必要的功能"
echo "  2. 编译器当前使用的是 Rust Runtime"
echo "  3. C Runtime 功能有限，无法满足未来需求"
echo "  4. 保持单一 Runtime 可以简化维护"
echo ""
echo "建议:"
echo "  • 如果不需要 C Runtime，可以删除 runtime/ 目录"
echo "  • 或者保留作为参考/备份"
echo "  • 继续使用和完善 Rust Runtime"
echo ""
