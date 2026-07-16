#!/usr/bin/env bash
# 类型检查·绿码断言 —— 假阳性攻击套件。
#
# 每个 .qi 都是刁钻但完全合法的程序，两步断言：
#   ① `qi run` 真跑通（15_AI原语_只编译.qi 只要求 compile 成功——运行需在线 LLM）；
#   ② QI_TYPECHECK=1 compile 下类型检查器零报错。
#
# 用法：qi/tests/类型检查绿码/断言.sh [qi二进制路径]
#   默认用 workspace 的 target/debug/qi。
# 与红码套件（qi/tests/类型检查红码/断言.sh）互为合同：绿码零报错 + 红码全抓住。
# 注意 macOS bash 3.2：shell 变量名一律 ASCII。
set -uo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
QI="${1:-$ROOT/target/debug/qi}"
export QI_PACKAGES_PATH="$ROOT/qi_packages"

total=0
passed=0
failed=0

# 收集测试文件：顶层 .qi + 多文件项目入口（被导入模块不单测）
FILES=$(ls "$HERE"/*.qi 2>/dev/null; echo "$HERE/14_多文件互调/主.qi")

check_one() {
    # $1=文件路径
    local f="$1"
    local name
    name=$(basename "$f")
    total=$((total+1))

    # ① 跑通（AI 原语文件只编译：运行需要在线 LLM 端点）
    local run_out run_rc
    if [ "$name" = "15_AI原语_只编译.qi" ]; then
        run_out=$(QI_TYPECHECK=0 timeout 120 "$QI" compile "$f" -o /tmp/绿码断言.ll 2>&1)
        run_rc=$?
    else
        run_out=$(QI_TYPECHECK=0 timeout 120 "$QI" run "$f" 2>&1)
        run_rc=$?
    fi
    if [ "$run_rc" -ne 0 ]; then
        echo "FAIL $name —— 程序本身跑不通（不合格的绿码）"
        printf '%s\n' "$run_out" | tail -3 | sed 's/^/       /'
        failed=$((failed+1))
        return
    fi

    # ② QI_TYPECHECK=1 零报错
    local tc_out
    tc_out=$(QI_TYPECHECK=1 timeout 120 "$QI" compile "$f" -o /tmp/绿码断言.ll 2>&1 | grep '\[类型检查\] 报错' || true)
    if [ -n "$tc_out" ]; then
        echo "FAIL $name —— 类型检查器误报（假阳性）"
        printf '%s\n' "$tc_out" | head -3 | sed 's/^/       /'
        failed=$((failed+1))
        return
    fi
    echo "PASS $name"
    passed=$((passed+1))
}

while IFS= read -r f; do
    [ -f "$f" ] || continue
    check_one "$f"
done <<< "$FILES"

rm -f /tmp/绿码断言.ll
echo ""
echo "绿码断言: $passed/$total 通过, $failed 失败"
[ "$failed" -eq 0 ]
