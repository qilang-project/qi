#!/usr/bin/env bash
# 语义类型检查器·全语料扫描 —— 量化假阳性版图，驱动补全收敛。
#
# 用法：qi/scripts/类型检查扫描.sh [qi二进制路径]
#   默认用 workspace 的 target/debug/qi。
# 输出：每文件的报错行数 + 按类别聚合统计 + 高频未定义名 Top20。
# 原始日志存 /tmp/类型检查扫描.log 供细看。
#
# 原则：语料全部是「已知能编译能跑」的绿码——检查器在其上报的每一条
# 都是假阳性。目标是把本脚本的总报错数打到 0，同时红码套件仍能抓住真错。
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
QI="${1:-$ROOT/target/debug/qi}"
export QI_PACKAGES_PATH="$ROOT/qi_packages"
export QI_TYPECHECK=1

LOG=/tmp/类型检查扫描.log
: > "$LOG"

# 语料：全部绿码（示例 + 各包 examples + 真实项目）
FILES=$(
  find "$ROOT/qi/示例" -name '*.qi' 2>/dev/null
  find "$ROOT/qi-web/examples" -name '*.qi' 2>/dev/null
  find "$ROOT/qi-harness/examples" -maxdepth 1 -name '*.qi' 2>/dev/null
  find "$ROOT/qi-cli/examples" -name '*.qi' 2>/dev/null
  echo "$ROOT/项目/家有小奇/主程序.qi"
  echo "$ROOT/aione-spike/服务.qi"
  echo "$ROOT/aione-spike/用户系统.qi"
)

total=0; bad=0
while IFS= read -r f; do
  [ -f "$f" ] || continue
  total=$((total+1))
  out=$("$QI" compile "$f" -o /tmp/类型检查扫描.ll 2>&1 | grep '\[类型检查\]' | grep '报错')
  if [ -n "$out" ]; then
    bad=$((bad+1))
    echo "### $f" >> "$LOG"
    echo "$out" >> "$LOG"
  fi
done <<< "$FILES"
rm -f /tmp/类型检查扫描.ll

echo "==================== 类型检查扫描结果 ===================="
echo "语料: $total 文件 | 有报错: $bad 文件"
echo ""
echo "--- 按类别聚合 ---"
printf "未定义函数     : %s\n" "$(grep -o "未定义的函数 '[^']*'" "$LOG" | wc -l | tr -d ' ')"
printf "未定义变量     : %s\n" "$(grep -o 'UndefinedVariable' "$LOG" | wc -l | tr -d ' ')"
printf "类型不匹配     : %s\n" "$(grep -o 'TypeMismatch {' "$LOG" | wc -l | tr -d ' ')"
printf "函数调用错误   : %s\n" "$(grep -o 'FunctionCallError' "$LOG" | wc -l | tr -d ' ')"
printf "作用域错误     : %s\n" "$(grep -o 'ScopeError' "$LOG" | wc -l | tr -d ' ')"
echo ""
echo "--- 高频未定义函数 Top20 ---"
grep -o "未定义的函数 '[^']*'" "$LOG" | sort | uniq -c | sort -rn | head -20
echo ""
echo "--- 高频未定义变量 Top20 ---"
grep -oE 'UndefinedVariable \{ name: \\?"[^"\\]*' "$LOG" | sed 's/.*name: \\\{0,1\}"//' | sort | uniq -c | sort -rn | head -20
echo ""
echo "原始日志: $LOG"
