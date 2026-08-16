#!/usr/bin/env bash
# RC 回归断言 —— 结构体本体（及其字段 / 数组元素 / 异常路径）的引用计数收支。
#
# 每个用例都是「跑 100 轮、每轮建了又丢」的小程序，判据两条，缺一不可：
#   ① QI_RC_REPORT=1 退出时 活跃对象/活跃字符串/活跃闭包 **全 0**（没漏）
#   ② stdout 与 .期望 逐字节一致（没提前放 —— 双放/悬垂会把数值算错或直接崩）
# 只查 ① 不查 ②是危险的：把释放插早一点净额照样归零，但读的是已释放内存。
#
# 覆盖的场景（红过的用 ← 标出，括号里是修之前的净额）：
#   01 局部出作用域 / 02 作返回值 / 03 传参 / 04 字符串字段 / 05 嵌套结构体
#   06 数组<结构体> / 07 分支内创建 / 08 字段重赋值 / 09 闭包捕获
#   10 抛出跨函数帧              ← 修前漏 50 个本体（longjmp 跳过出口释放）
#   11 方法的 OWNED 临时接收者
#   12 临时结构体读标量字段      ← 修前漏 200 个本体（临时基没人放）
#   13 捕获变量在循环里反复绑定  ← 修前漏 49 条串（槽覆写不放旧值）
#   14 临时结构体读 RC 字段      ← 修前漏 100 本体 + 100 串
#   15 同帧捕获不得双放          ← 「抛出前释放局部」的安全护栏，崩了就是双放
#   16 抛出的消息本身是局部串    ← 释放局部必须在消息 stage 之后
#
# 用法：qi/tests/rc回归/断言.sh [qi二进制路径]
#   默认用 workspace 的 target/debug/qi。
# 注意 macOS bash 3.2：shell 变量名一律 ASCII。
set -uo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
QI="${1:-$ROOT/target/debug/qi}"
export QI_PACKAGES_PATH="${QI_PACKAGES_PATH:-$ROOT/qi_packages}"

# macOS 没有 GNU timeout（coreutils 才带），没有就退化成不限时 ——
# 宁可挂住等 CI job 超时，也别让每条用例都 rc=127 装成用例失败。
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

for f in "$HERE"/结构体/*.qi; do
    name=$(basename "$f")
    expect="${f%.qi}.期望"
    total=$((total+1))

    if [ ! -f "$expect" ]; then
        echo "FAIL $name (缺 .期望 文件)"
        failed=$((failed+1))
        continue
    fi

    # ② 行为：stdout 必须与 .期望 一致
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

    # ① 净额：活跃对象/字符串/闭包必须全 0
    rc_line=$(QI_RC_REPORT=1 run_limited "$QI" run "$f" 2>&1 | grep '\[qi-rc\]')
    if ! echo "$rc_line" | grep -q '活跃对象=0 活跃字符串=0 活跃闭包=0'; then
        echo "FAIL $name (RC 净额非零: ${rc_line:-无 [qi-rc] 行})"
        failed=$((failed+1))
        continue
    fi

    echo "PASS $name"
    passed=$((passed+1))
done

echo "rc回归: $passed/$total 通过"
[ $failed -eq 0 ] || exit 1
