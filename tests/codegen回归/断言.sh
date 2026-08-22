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
#   ⑧ C 回调（22 + C回调/23~26）：顶层函数当裸函数指针传给 libc qsort 要真能排序，
#     闭包 / 元数不符 / 返回不符 / 签名含字符串这四种一律编译期拒绝 ——
#     放过去都是运行期随机段错误或静默读坏内存，比编译失败糟得多
#   ⑨ 无限定内置原语给用户同名函数让路（28）：`完成` / `加锁` / `获取时间` /
#     `取消协程` 这些短裸名以前排在用户函数**之前**解析，用户定义了同名函数就
#     静默绑到运行时 FFI 上 —— 有类型标注报「聚合类型 vs 标量」（指错方向），
#     没标注直接段错误。撞名时元数不符也不许 panic，要报清楚（29）
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
for f in "$HERE"/0[1-68]_*.qi "$HERE"/11_*.qi "$HERE"/22_*.qi "$HERE"/27_*.qi "$HERE"/28_*.qi; do
    name=$(basename "$f")
    expect="${f%.qi}.期望"
    total=$((total+1))
    # 两边都 tr -d '\r' 再比。Windows 上 git 默认 core.autocrlf=true，
    # 检出到 runner 的 .期望 文件行尾是 CRLF，而程序输出是 LF ——
    # 于是每条正例都「值完全正确却报输出不符」，最难看的那种红。
    actual=$(run_limited "$QI" run "$f" 2>/dev/null | tr -d '\r')
    rc=$?
    if [ $rc -ne 0 ]; then
        echo "FAIL $name (运行失败 rc=$rc)"
        failed=$((failed+1))
        continue
    fi
    # 直接比字符串，不走 `diff <(...)`：进程替换在 git-bash(MSYS) 上不可靠，
    # 而且 $() 已经把两边的尾随换行都吃掉了，比出来的语义一样。
    expect_text=$(tr -d '\r' < "$expect")
    if [ "$actual" != "$expect_text" ]; then
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
# **每次都编到同一个输出路径**。以前是 det_1..det_5 五个不同文件名 ——
# 在 Windows 上这本身就会让产物不同：PE 的导出表里存着模块名（= 输出文件名），
# 名字一换字节就变，跟 codegen 确不确定毫无关系。同名重编照样能测出真问题
# （5 次编译跨越秒边界，时间戳类的不确定性一样会暴露）。
out="/tmp/codegen回归_det.bin"
det_why=""
det_md5s=""
for i in 1 2 3 4 5; do
    det_err=$(run_limited "$QI" compile "$det_src" -o "$out" 2>&1)
    if [ $? -ne 0 ]; then
        det_ok=0
        det_why="第 $i 次编译失败: $(echo "$det_err" | head -3 | tr '\n' ' ')"
        break
    fi
    m=$(md5 -q "$out" 2>/dev/null || md5sum "$out" | cut -d' ' -f1)
    det_md5s="$det_md5s $m"
    # 留一份副本：不一致时要拿它指出**差在第几个字节**，
    # 否则只知道「不一致」，得再烧一轮 CI 才知道是时间戳还是别的。
    cp "$out" "/tmp/codegen回归_det_副本$i.bin" 2>/dev/null
    if [ -z "$first_md5" ]; then first_md5="$m"
    elif [ "$m" != "$first_md5" ]; then det_ok=0; det_why="第 $i 次与第 1 次不同"
    fi
done
if [ $det_ok -eq 1 ]; then
    echo "PASS 04(编译5次产物一致)"
    passed=$((passed+1))
else
    echo "FAIL 04(编译5次产物不一致或编译失败: ${det_why})"
    echo "  各次 md5:${det_md5s}"
    if [ -f /tmp/codegen回归_det_副本1.bin ] && [ -f /tmp/codegen回归_det_副本2.bin ]; then
        echo "  头 20 处差异（字节偏移 十进制，1 起）:"
        cmp -l /tmp/codegen回归_det_副本1.bin /tmp/codegen回归_det_副本2.bin 2>&1 | head -20 | sed 's/^/    /'
    fi
    failed=$((failed+1))
fi
rm -f /tmp/codegen回归_det.bin /tmp/codegen回归_det.lib /tmp/codegen回归_det.exp \
      /tmp/codegen回归_det_副本*.bin

# ── 红码：必须报错的用例 ──
#
# 每个用例一行「文件名|错误里必须出现的关键词」。原来每一条都手抄一段十几行的
# if/else，加一条就复制粘贴一次 —— 而抄错了（比如忘改变量名）**不会报错，
# 只是那一条永远 PASS**。
# shell 标识符一律 ASCII。bash 的变量名**根本不接受非 ASCII** ——
# `红码用例=(` 报的是「未预期的记号 newline 附近有语法错误」，
# 一眼看不出是名字的问题。（部署脚本里踩过同一个坑。）
#
# 用位置参数而不是数组 + read：`set -u` 下 read 少读一个字段就报
# 「case_kw: 未绑定的变量」，跟用例本身毫无关系。
#
# ⚠ 变量后面紧跟全角标点要写 ${case_kw} —— `"…「$case_kw」…"` 里那个 」
# 会被 bash **吃进变量名**，报 "case_kw?: 未绑定的变量"（? 就是那半个字符），
# 一眼看不出是花括号的问题。部署脚本里的 `$one.qi` 是同一个坑。
set -- \
  "07_限定错模块_必须报错.qi::没有函数" \
  "10_导入不存在的标准库模块_必须报错.qi::标准库里没有模块" \
  "09_跨包同名_裸调用_必须报错.qi::歧义" \
  "12_跨包同名_无选择性导入_必须报错.qi::歧义" \
  "同文件重名/19_同文件重名_必须报错.qi::无法按元数区分" \
  "公开同名/20_公开同名_必须报错.qi::无法按元数区分" \
  "结构体喂标量/21_结构体喂给标量_必须报错.qi::把结构体传给了标量形参" \
  "C回调/23_C回调_闭包_必须报错.qi::不是无捕获顶层函数" \
  "C回调/24_C回调_元数不符_必须报错.qi::元数不符" \
  "C回调/25_C回调_返回不符_必须报错.qi::返回类型不符" \
  "C回调/26_C回调签名含字符串_必须报错.qi::不能出现在 C 回调签名里" \
  "29_内置元数不符_必须报错.qi::需要 2 个实参"

for one in "$@"; do
    case_file=${one%%::*}
    case_kw=${one##*::}
    total=$((total+1))
    if [ ! -f "$HERE/$case_file" ]; then
        echo "FAIL $case_file (用例文件不存在)"
        failed=$((failed+1))
        continue
    fi
    out=$(run_limited "$QI" compile "$HERE/$case_file" -o /tmp/codegen回归_红码.bin 2>&1)
    rc=$?
    rm -f /tmp/codegen回归_红码.bin
    if [ $rc -ne 0 ] && echo "$out" | grep -q "$case_kw"; then
        echo "PASS $case_file"
        passed=$((passed+1))
    else
        echo "FAIL $case_file (rc=$rc 期望含「${case_kw}」，实际: $(echo "$out" | head -1))"
        failed=$((failed+1))
    fi
done


echo "codegen回归: $passed/$total 通过"
[ $failed -eq 0 ] || exit 1
