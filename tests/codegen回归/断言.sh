#!/usr/bin/env bash
# codegen 回归断言 —— 四个已修 codegen 真 bug 的行为契约：
#   ① 嵌套块同名变量遮蔽（01/02/03，02 额外查 QI_RC_REPORT 零泄漏）
#   ② 模块限定重名函数分发确定性（04 编译 5 次输出一致 + 07 错模块必须报错）
#   ③ 内建 长度 对 字符串/自己.字段 的分发（05）
#   ④ `x 作为 T` 类型转换表达式（06）
#   ⑤ 跨包同名函数：写了限定名就不该报歧义（08），不写仍要报（09）
#   ⑥ 导入不存在的标准库模块必须当场报错（10）——以前靠别名撞名能蒙混过关
#   ⑦ 选择性导入消歧要认「导入路径 ≠ 声明包名」（11，甲/乙.qi 声明 `包 乙;`），
#     不写选择性导入时歧义防线照旧（12）——以前只按全路径/首段对包名，末段缺失，
#     `导入 甲.乙::{X}` 写了也白写还照样报歧义，而报错建议的正是这个写法
#
# 用法：qi/tests/codegen回归/断言.sh [qi二进制路径]
#   默认用 workspace 的 target/debug/qi。
# 注意 macOS bash 3.2：shell 变量名一律 ASCII。
set -uo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
QI="${1:-$ROOT/target/debug/qi}"
export QI_PACKAGES_PATH="${QI_PACKAGES_PATH:-$ROOT/qi_packages}"

# 限时执行。**macOS 没有 GNU timeout** —— 那是 coreutils 带的，
# brew install coreutils 之后才有（还可能只叫 gtimeout）。CI 的 macos runner
# 上没有，于是每条用例都 rc=127「command not found」，整套 0/11 全红，
# 而报错长得像编译器坏了。我本地装了 coreutils 所以一直看不出来。
# 没有就退化成不限时跑：宁可挂住等 job 超时，也别假装是用例失败。
# 注意 macOS bash 3.2：函数名一律 ASCII。
if command -v timeout >/dev/null 2>&1; then
    run_limited() { timeout 120 "$@"; }
elif command -v gtimeout >/dev/null 2>&1; then
    run_limited() { gtimeout 120 "$@"; }
else
    run_limited() { "$@"; }
fi

total=0
passed=0
failed=0

# ── 正向用例：运行输出与 .期望 逐字节一致 ──
for f in "$HERE"/0[1-68]_*.qi "$HERE"/11_*.qi; do
    name=$(basename "$f")
    expect="${f%.qi}.期望"
    total=$((total+1))
    actual=$(run_limited "$QI" run "$f" 2>/dev/null)
    rc=$?
    if [ $rc -ne 0 ]; then
        echo "FAIL $name (运行失败 rc=$rc)"
        failed=$((failed+1))
        continue
    fi
    if ! diff -q <(printf '%s\n' "$actual") "$expect" >/dev/null 2>&1; then
        echo "FAIL $name (输出不符)"
        echo "  期望: $(tr '\n' ' ' < "$expect")"
        echo "  实际: $(printf '%s' "$actual" | tr '\n' ' ')"
        failed=$((failed+1))
        continue
    fi
    # 02：ARC 遮蔽变体 —— QI_RC_REPORT=1 下活跃对象/字符串/闭包必须全 0
    if [ "$name" = "02_遮蔽_字符串ARC.qi" ]; then
        rc_line=$(QI_RC_REPORT=1 run_limited "$QI" run "$f" 2>&1 | grep '\[qi-rc\]')
        if ! echo "$rc_line" | grep -q '活跃对象=0 活跃字符串=0 活跃闭包=0'; then
            echo "FAIL $name (RC 泄漏: $rc_line)"
            failed=$((failed+1))
            continue
        fi
    fi
    echo "PASS $name"
    passed=$((passed+1))
done

# ── 04 附加断言：同一源码编译 5 次，产物 md5 完全一致（分发确定性）──
total=$((total+1))
det_src="$HERE/04_模块限定分发确定性.qi"
det_ok=1
first_md5=""
for i in 1 2 3 4 5; do
    out="/tmp/codegen回归_det_$i.bin"
    if ! run_limited "$QI" compile "$det_src" -o "$out" >/dev/null 2>&1; then
        det_ok=0; break
    fi
    m=$(md5 -q "$out" 2>/dev/null || md5sum "$out" | cut -d' ' -f1)
    if [ -z "$first_md5" ]; then first_md5="$m"
    elif [ "$m" != "$first_md5" ]; then det_ok=0; break
    fi
done
rm -f /tmp/codegen回归_det_*.bin
if [ $det_ok -eq 1 ]; then
    echo "PASS 04(编译5次产物一致)"
    passed=$((passed+1))
else
    echo "FAIL 04(编译5次产物不一致或编译失败)"
    failed=$((failed+1))
fi

# ── 红码：限定到错模块必须报确定错误 ──
total=$((total+1))
err_out=$(run_limited "$QI" compile "$HERE/07_限定错模块_必须报错.qi" -o /tmp/codegen回归_err.bin 2>&1)
err_rc=$?
rm -f /tmp/codegen回归_err.bin
if [ $err_rc -ne 0 ] && echo "$err_out" | grep -q '没有函数'; then
    echo "PASS 07_限定错模块_必须报错.qi"
    passed=$((passed+1))
else
    echo "FAIL 07_限定错模块_必须报错.qi (rc=$err_rc 输出: $(echo "$err_out" | head -1))"
    failed=$((failed+1))
fi

echo ""

# ── 红码：导入一个不存在的标准库模块 ──
total=$((total+1))
mod_out=$(run_limited "$QI" compile "$HERE/10_导入不存在的标准库模块_必须报错.qi" -o /tmp/codegen回归_mod.bin 2>&1)
mod_rc=$?
rm -f /tmp/codegen回归_mod.bin
if [ $mod_rc -ne 0 ] && echo "$mod_out" | grep -q '标准库里没有模块'; then
    echo "PASS 10_导入不存在的标准库模块_必须报错.qi"
    passed=$((passed+1))
else
    echo "FAIL 10_导入不存在的标准库模块_必须报错.qi (rc=$mod_rc 输出: $(echo "$mod_out" | head -1))"
    failed=$((failed+1))
fi

echo ""

# ── 红码：跨包同名 + 不写限定名 → 歧义检查必须照旧生效 ──
total=$((total+1))
amb_out=$(run_limited "$QI" compile "$HERE/09_跨包同名_裸调用_必须报错.qi" -o /tmp/codegen回归_amb.bin 2>&1)
amb_rc=$?
rm -f /tmp/codegen回归_amb.bin
if [ $amb_rc -ne 0 ] && echo "$amb_out" | grep -q '歧义'; then
    echo "PASS 09_跨包同名_裸调用_必须报错.qi"
    passed=$((passed+1))
else
    echo "FAIL 09_跨包同名_裸调用_必须报错.qi (rc=$amb_rc 输出: $(echo "$amb_out" | head -1))"
    failed=$((failed+1))
fi

# ── 红码：跨包同名（且导入路径 ≠ 声明包名）+ 不写选择性导入 → 歧义防线照旧 ──
total=$((total+1))
amb2_out=$(run_limited "$QI" compile "$HERE/12_跨包同名_无选择性导入_必须报错.qi" -o /tmp/codegen回归_amb2.bin 2>&1)
amb2_rc=$?
rm -f /tmp/codegen回归_amb2.bin
if [ $amb2_rc -ne 0 ] && echo "$amb2_out" | grep -q '歧义'; then
    echo "PASS 12_跨包同名_无选择性导入_必须报错.qi"
    passed=$((passed+1))
else
    echo "FAIL 12_跨包同名_无选择性导入_必须报错.qi (rc=$amb2_rc 输出: $(echo "$amb2_out" | head -1))"
    failed=$((failed+1))
fi

echo "codegen回归: $passed/$total 通过"
[ $failed -eq 0 ] || exit 1
