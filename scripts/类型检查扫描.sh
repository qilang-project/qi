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

# ROOT 默认是 workspace 根。给 qi 单独开的 worktree（~/qi-agents/<字母>/qi）里
# 只有编译器，qi-web / 项目 / aione-spike 都不在 —— 语料会缩到只剩示例，
# 扫出 0 条会给人一种「宽语料也干净」的错觉。这种时候显式指过去：
#   QI_SCAN_ROOT=~/Things/dev/lang/qilang qi/scripts/类型检查扫描.sh <qi二进制>
ROOT="${QI_SCAN_ROOT:-$(cd "$(dirname "$0")/../.." && pwd)}"
QI="${1:-$ROOT/target/debug/qi}"
export QI_PACKAGES_PATH="$ROOT/qi_packages"
# 扫描只量假阳性，绝不能让 strict（现在的默认档）中止在第一个报错上
export QI_TYPECHECK=warn

LOG=/tmp/类型检查扫描.log
: > "$LOG"

# 语料：全部绿码（示例 + 各包 examples + 真实项目的全部入口）
#
# 2026-08-22 扩面：原来只有示例 + 三个包的 examples + 三个点名的项目文件，
# 覆盖不到 项目/ 底下真正在线上跑的那批（投资面板/小小英语/奇仓/学外语…）。
# 抬默认档位（QI_TYPECHECK 未设 = strict）之前用这份宽语料量过：
# 211 个示例入口 0 条，221 个项目入口 4 条且全是真错。语料窄 = 抬档没底气，
# 所以这里按「带 入口() 的文件」全收，而不是手点几个。
#
# 缺席的目录（独立 git 仓没 clone 全、worktree 里不存在）由 -f 判断静默跳过，
# 跟 make ci 里 gRPC 那一节同样的降级方式。
FILES=$(
  find "$ROOT/qi/示例" -name '*.qi' 2>/dev/null
  for d in qi-web/examples qi-harness/examples qi-cli/examples qi-widgets \
           qi-registry qi-playground qi-todo-web qi-todo-api qi-todo-cli 项目 aione-spike; do
    grep -rl "函数 入口()" --include='*.qi' "$ROOT/$d" 2>/dev/null | grep -v qi_packages
  done
)

# 已知坏码：不是检查器误报，是文件本身就编不过，收进语料只会把基线搞脏。
# 每一条都要写清楚为什么，修好了就从这儿删掉。
is_excluded() {
  case "$1" in
    # 重构到一半：取课文/生成语言版/记课文 三处实参个数对不上，compile 本身就失败。
    # 这恰好是宽语料捞出来的**真错**，留个坑位提醒去修。
    */项目/学外语/出一课.qi) return 0 ;;
    # 存量解析错误（意外的标记 `{`），与类型检查无关。
    */qi-todo-web/主程序.qi) return 0 ;;
  esac
  return 1
}

total=0; bad=0; skipped=0
while IFS= read -r f; do
  [ -f "$f" ] || continue
  if is_excluded "$f"; then skipped=$((skipped+1)); continue; fi
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
echo "语料: $total 文件 | 有报错: $bad 文件 | 已知坏码跳过: $skipped"
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
