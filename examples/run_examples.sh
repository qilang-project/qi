#!/bin/bash
# Qi 语言示例运行脚本
#
# 这个脚本演示如何运行 Qi 语言的示例程序
# 注意：当前版本仍在开发中，.qi 文件解析功能正在实现

set -e

echo "=================================="
echo "Qi 编程语言 - 示例程序运行脚本"
echo "=================================="
echo ""

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# 检查 qi 编译器是否存在
if [ ! -f "../target/debug/qi" ] && [ ! -f "../target/release/qi" ]; then
    echo -e "${YELLOW}警告: qi 编译器未找到，正在构建...${NC}"
    cd .. && cargo build --release && cd examples
fi

QI_COMPILER="../target/release/qi"
if [ ! -f "$QI_COMPILER" ]; then
    QI_COMPILER="../target/debug/qi"
fi

echo -e "${GREEN}使用编译器: $QI_COMPILER${NC}"
echo ""

# 显示可用的示例
echo "可用的 Qi 语言示例："
echo "  1. 你好世界.qi - 最简单的 Hello World 程序"
echo "  2. 基础语法示例.qi - 展示基本语法特性"
echo "  3. 异步并发示例.qi - 展示 M:N 协程调度"
echo "  4. 结构体和枚举.qi - 展示数据结构定义"
echo ""

# 函数：显示文件内容
show_file() {
    local file=$1
    echo -e "${GREEN}文件: $file${NC}"
    echo "----------------------------------------"
    cat "qi/$file" | head -20
    echo "... (查看完整内容请打开文件) ..."
    echo "----------------------------------------"
    echo ""
}

# 函数：模拟编译（当前阶段）
simulate_compile() {
    local file=$1
    echo -e "${YELLOW}[模拟] 编译 $file...${NC}"
    echo "  词法分析: 解析中文关键字..."
    echo "  语法分析: 构建抽象语法树..."
    echo "  语义分析: 类型检查..."
    echo "  代码生成: 生成 LLVM IR..."
    echo -e "${GREEN}  编译完成${NC}"
    echo ""
}

# 主菜单
echo "请选择操作："
echo "  [1] 查看所有示例文件"
echo "  [2] 显示 你好世界.qi"
echo "  [3] 显示 基础语法示例.qi"
echo "  [4] 显示 异步并发示例.qi"
echo "  [5] 显示 结构体和枚举.qi"
echo "  [6] 测试编译器（检查语法）"
echo "  [0] 退出"
echo ""
echo -n "请输入选项 [0-6]: "

read -r choice

case $choice in
    1)
        echo ""
        echo "=== 所有示例文件 ==="
        ls -lh qi/*.qi
        echo ""
        echo "请使用文本编辑器查看这些文件"
        ;;
    2)
        show_file "你好世界.qi"
        ;;
    3)
        show_file "基础语法示例.qi"
        ;;
    4)
        show_file "异步并发示例.qi"
        ;;
    5)
        show_file "结构体和枚举.qi"
        ;;
    6)
        echo ""
        echo "=== 测试编译器 ==="
        echo ""
        
        for file in qi/*.qi; do
            filename=$(basename "$file")
            echo -e "${GREEN}检查: $filename${NC}"
            
            # 当前阶段：只显示文件信息
            echo "  文件大小: $(wc -l < "$file") 行"
            echo "  编码: UTF-8"
            echo "  中文关键字: $(grep -o "函数\|变量\|如果\|否则\|返回" "$file" | wc -l) 个"
            echo ""
            
            # simulate_compile "$filename"
        done
        
        echo -e "${YELLOW}注意: Qi 编译器仍在开发中。${NC}"
        echo "完整的编译功能将在后续版本中实现。"
        echo "当前可以使用 'qi check <文件>' 来验证语法。"
        ;;
    0)
        echo "退出"
        exit 0
        ;;
    *)
        echo -e "${RED}无效选项${NC}"
        exit 1
        ;;
esac

echo ""
echo "==================================="
echo "感谢使用 Qi 编程语言！"
echo "==================================="
echo ""
echo "更多信息："
echo "  - 文档: ../docs/qi-unified-design-zh-cn.md"
echo "  - 仓库: https://github.com/qi-lang/qi-compiler"
echo "  - 社区: https://community.qi-lang.org"
echo ""
