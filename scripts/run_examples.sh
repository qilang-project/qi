#!/bin/bash

# run_examples.sh - 运行示例目录下所有Qi文件的脚本
# Script to run all Qi files in the 示例 directory

set -e  # 遇到错误时立即退出

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# 打印标题
echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}运行所有Qi示例文件 | Running all Qi examples${NC}"
echo -e "${BLUE}========================================${NC}"

# 获取脚本所在目录的父目录（项目根目录）
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
EXAMPLES_DIR="$PROJECT_ROOT/示例"

# 检查示例目录是否存在
if [ ! -d "$EXAMPLES_DIR" ]; then
    echo -e "${RED}错误: 示例目录不存在 | Error: 示例 directory not found${NC}"
    exit 1
fi

# 查找所有.qi文件
echo -e "${YELLOW}正在查找Qi文件... | Finding Qi files...${NC}"
qi_files=$(find "$EXAMPLES_DIR" -name "*.qi" -type f | sort)

if [ -z "$qi_files" ]; then
    echo -e "${RED}未找到任何.qi文件 | No .qi files found${NC}"
    exit 1
fi

# 计算文件数量
file_count=$(echo "$qi_files" | wc -l | tr -d ' ')
echo -e "${GREEN}找到 $file_count 个Qi文件 | Found $file_count Qi files${NC}"
echo

# 计数器
current=0
success_count=0
error_count=0

# 遍历并运行每个Qi文件
while IFS= read -r qi_file; do
    current=$((current + 1))

    # 获取相对于项目根目录的路径 (纯bash实现)
    relative_path=${qi_file#$PROJECT_ROOT/}

    # 如果文件在 multi_file 目录中,检查是否包含 "包 主程序;" 和 "函数 入口"
    # 只运行包含这两个标记的主文件,跳过库文件
    if [[ "$relative_path" == *"/multi_file/"* ]]; then
        if ! grep -q "包 主程序;" "$qi_file" || ! grep -q "函数 入口" "$qi_file"; then
            echo -e "${YELLOW}[$current/$file_count] 跳过库文件 | Skipping library: $relative_path${NC}"
            echo "========================================"
            echo
            continue
        fi
    fi

    echo -e "${BLUE}[$current/$file_count] 运行 | Running: $relative_path${NC}"
    echo "----------------------------------------"

    # 切换到项目根目录并运行（默认启用详细输出）
    if cd "$PROJECT_ROOT" && cargo run -- -v run "$relative_path"; then
        echo -e "${GREEN}✓ 成功 | Success: $relative_path${NC}"
        success_count=$((success_count + 1))
    else
        # 检查是否是 broken pipe 错误 (exit code 141)
        if [ $? -eq 141 ]; then
            echo -e "${GREEN}✓ 成功 | Success: $relative_path${NC} (输出被截断 | Output truncated)"
            success_count=$((success_count + 1))
        else
            echo -e "${RED}✗ 失败 | Failed: $relative_path${NC}"
            error_count=$((error_count + 1))
        fi
    fi

    echo
    echo "========================================"
    echo

done <<< "$qi_files"

# 打印总结
echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}运行总结 | Run Summary${NC}"
echo -e "${BLUE}========================================${NC}"
echo -e "${GREEN}成功 | Success: $success_count${NC}"
echo -e "${RED}失败 | Failed: $error_count${NC}"
echo -e "${YELLOW}总计 | Total: $file_count${NC}"

if [ $error_count -eq 0 ]; then
    echo -e "${GREEN}所有文件运行成功！| All files ran successfully!${NC}"
    exit 0
else
    echo -e "${RED}有 $error_count 个文件运行失败。| $error_count files failed to run.${NC}"
    exit 1
fi