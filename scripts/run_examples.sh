#!/usr/bin/env bash
# 跑所有"稳定"示例做回归。CI 和本地 pre-push 都能用。
#
# 用法：
#   ./scripts/run_examples.sh                # 跑全部稳定示例
#   ./scripts/run_examples.sh --quick        # 只跑基础组（最快）
#   QI_BIN=./target/debug/qi ./scripts/run_examples.sh  # 用 debug build
#
# 跳过需要外部条件的示例：QI_LLM_KEY / 网络 / 端口监听 / GUI。
set -euo pipefail

QI_BIN="${QI_BIN:-./target/release/qi}"
MODE="${1:-full}"

if [ ! -x "$QI_BIN" ]; then
    echo "找不到 qi 二进制：$QI_BIN" >&2
    echo "先 cargo build --release，或设 QI_BIN=./target/debug/qi" >&2
    exit 1
fi

QUICK_EXAMPLES=(
    "示例/基础/格式字符串.qi"
    "示例/基础/新功能演示.qi"
    "示例/基础/枚举/枚举示例.qi"
    "示例/基础/选项与结果/示例.qi"
    "示例/基础/泛型/泛型示例.qi"
    "示例/基础/外部函数/示例.qi"
    "示例/基础/外部函数v2/示例.qi"
    "示例/基础/外部函数回调/示例.qi"
)
STDLIB_EXAMPLES=(
    "示例/标准库/路径处理/路径示例.qi"
    "示例/标准库/加密/加密.qi"
    "示例/标准库/正则表达式/正则示例.qi"
    "示例/标准库/日期时间示例/日期时间示例.qi"
    "示例/标准库/随机数/随机示例.qi"
    "示例/标准库/向量/向量示例.qi"
)

if [ "$MODE" = "--quick" ] || [ "$MODE" = "-q" ]; then
    EXAMPLES=("${QUICK_EXAMPLES[@]}")
else
    EXAMPLES=("${QUICK_EXAMPLES[@]}" "${STDLIB_EXAMPLES[@]}")
fi

pass=0
fail=0
skipped=0
failed_files=()

for example in "${EXAMPLES[@]}"; do
    if [ ! -f "$example" ]; then
        skipped=$((skipped + 1))
        echo "  ⊘ $example (缺失)"
        continue
    fi
    if "$QI_BIN" run "$example" >/dev/null 2>&1; then
        pass=$((pass + 1))
        echo "  ✓ $example"
    else
        fail=$((fail + 1))
        failed_files+=("$example")
        echo "  ✗ $example"
    fi
done

echo
echo "  通过: $pass    失败: $fail    跳过: $skipped"
if [ $fail -gt 0 ]; then
    echo
    echo "失败详情："
    for f in "${failed_files[@]}"; do
        echo "  $QI_BIN run $f"
        "$QI_BIN" run "$f" 2>&1 | tail -40 | sed 's/^/    /'
    done
    exit 1
fi
