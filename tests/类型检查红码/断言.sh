#!/usr/bin/env bash
# 类型检查·红码断言 —— 每个坏程序都必须被 QI_TYPECHECK=1 抓住，且报对类别。
#
# 用法：qi/tests/类型检查红码/断言.sh [qi二进制路径]
#   默认用 workspace 的 target/debug/qi。从 workspace 根或任意目录跑均可。
#
# 这是「宽容检查器」的校准合同：绿码扫描（qi/scripts/类型检查扫描.sh）打到
# 0 报错的同时，本套件必须全数抓住 —— 防止用「全部静默」作弊清零。
# 注意 macOS bash 3.2：shell 变量名一律 ASCII。
set -uo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
QI="${1:-$ROOT/target/debug/qi}"
export QI_PACKAGES_PATH="$ROOT/qi_packages"
export QI_TYPECHECK=1

total=0
passed=0
failed=0

check_one() {
    # $1=文件名  $2=期望在 stderr 里出现的报错子串
    local f="$1" expect="$2"
    total=$((total+1))
    local out
    out=$("$QI" compile "$HERE/$f" -o /tmp/红码断言.ll 2>&1 | grep '\[类型检查\] 报错' || true)
    if [ -z "$out" ]; then
        echo "FAIL $f —— 未报任何类型检查错误（静默漏报）"
        failed=$((failed+1))
        return
    fi
    if ! printf '%s' "$out" | grep -qF "$expect"; then
        echo "FAIL $f —— 报了错但类别不对，期望含: $expect"
        printf '%s\n' "$out" | head -3 | sed 's/^/       /'
        failed=$((failed+1))
        return
    fi
    echo "PASS $f"
    passed=$((passed+1))
}

check_one 01_字符串赋整数变量.qi 'TypeMismatch'
check_one 02_整数赋数组变量.qi   'TypeMismatch'
check_one 03_未定义变量.qi       'UndefinedVariable'
check_one 04_未定义函数.qi       "未定义的函数 '不存在的函数'"
check_one 05_实参个数错.qi       '参数数量不匹配'
check_one 06_实参类型错.qi       'TypeMismatch'
check_one 07_返回类型错.qi       'TypeMismatch'
check_one 08_结构体字段名错.qi   "没有字段 'z'"
check_one 09_赋值类型错.qi       'TypeMismatch'
check_one 10_未定义结构体.qi     "未定义的结构体类型 '不存在的类型'"
# ── 对抗校验新增变体（2026-07：错配藏深处/边界元数/字面量变量中转/枚举载荷/方法元数）──
check_one 11_错配藏循环嵌套块.qi 'TypeMismatch'
check_one 12_错配藏匹配臂.qi     'TypeMismatch'
check_one 13_常量声明错配.qi     'TypeMismatch'
check_one 14_嵌套调用实参错.qi   'TypeMismatch'
check_one 15_嵌套字面量字段名错.qi "没有字段 '不存在'"
check_one 16_默认参数缺必需.qi   '参数数量不匹配'
check_one 17_变参缺必需.qi       '参数数量不匹配'
check_one 18_多路径返回一条错.qi 'TypeMismatch'
check_one 19_局部函数体内错.qi   'TypeMismatch'
check_one 20_默认参数多传.qi     '参数数量不匹配'
check_one 21_字段赋值类型错.qi   'TypeMismatch'
check_one 22_数组元素赋值错.qi   'TypeMismatch'
check_one 23_字面量变量中转.qi   'TypeMismatch'
check_one 24_字面量变量返回错.qi 'TypeMismatch'
check_one 25_枚举载荷元数错.qi   '载荷元数不匹配'
check_one 26_方法元数错.qi       '参数数量不匹配'
check_one 27_跨文件错配.qi       'TypeMismatch'
check_one 28_turbofish元数错.qi  '参数数量不匹配'
# ── 波2 召回提升（2026-07：载荷类型/常量重赋值/混族数组/模板洞/重载实参）──
check_one 29_枚举载荷类型错_裸构造.qi 'TypeMismatch'
check_one 30_枚举载荷类型错_限定构造.qi 'TypeMismatch'
check_one 31_常量重赋值.qi       '不能给常量'
check_one 32_结构体字段值类型错.qi 'TypeMismatch'
check_one 33_数组字面量混族.qi   'TypeMismatch'
check_one 34_模板串洞未定义变量.qi 'UndefinedVariable'
check_one 35_模板串洞未定义函数.qi "未定义的函数 '不存在的函数'"
check_one 36_重载多候选实参错.qi 'TypeMismatch'
# ── 2026-08：注册表 ptr 返回=字符串（闪购超卖假象的根因）──
check_one 37_模块字符串返回赋整数.qi 'TypeMismatch'
check_one 38_模块字符串返回传整数形参.qi 'TypeMismatch'
# ── 2026-08：数组元素门槛从「直接字面量」放宽到「字面量来源」──
check_one 39_数组元素字面量变量混族.qi 'TypeMismatch'
# ── 2026-08：注册表区分「真整数」与「句柄」，整数返回不再一律沉默 ──
check_one 40_时间整数返回赋字符串.qi 'TypeMismatch'
check_one 41_字符串下标返回赋字符串.qi 'TypeMismatch'
check_one 42_句柄返回赋字符串.qi 'TypeMismatch'
check_one 43_列表大小赋字符串.qi 'TypeMismatch'

rm -f /tmp/红码断言.ll
echo ""
echo "红码断言: $passed/$total 通过, $failed 失败"
[ "$failed" -eq 0 ]
