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
#   ⑩ 协程 llvm.coro.end 签名跟着链接的 LLVM 走（32）：LLVM 21 返回 i1、
#     LLVM 22 返回 void，写死一边另一边就在模块校验期炸，且只有真协程程序才炸
#   ⑪ 写了包限定名的调用不被内建原语劫持（33）：用户包定义 长度 时，
#     `几何.长度(v)` 要调到用户那个（模长），裸 `长度(v)` 仍是内建（元素个数）。
#     旁边 数组长度 那条拦截一直带 module_qualifier 守卫，长度 这条漏了 ——
#     两个都返回数字，抢错了不报错也不崩，只是值悄悄变成元素个数
#   ⑫ 用户包与标准库模块**重名**时，别名调用要解析到标准库那个（34）：
#     qi-grpc 是 `包 gRPC;` + `导入 标准库.gRPC 作为 运行时`，两边都有
#     调用带元数据 / 开流。守卫只按「函数按包 有没有同名函数」让路的话，
#     标准库调用会被本包同名函数截走，报的是 LLVM「Incorrect number of
#     arguments」—— 跟重名八竿子打不着，极难往这儿想
#   ⑬ 同一接收器类型上同名方法按元数重载（35）：方法表键与 LLVM 符号都要带元数，
#     否则第二个同名方法盖掉第一个的签名，调短的那个报「缺少第 N 个形参」——
#     自由函数早就能按元数重载，方法不能是不一致
#   ⑭ 非入口模块里「本模块函数当函数值」不被同名导入包函数顶掉（36）：
#     函数值签名索引表原来按**裸名**建键、先登记的赢，而导入包先于本地模块登记。
#     本地 删一条(上下文) 传给 提交 时会按包里那个四参 删一条 的元数去 mangle，
#     报「trampoline 目标缺失: _Z_...#4」—— 跟撞名毫无字面联系。入口模块碰不到
#     （它先于包登记），所以只有拆了模块的多文件项目才中招
#   ⑮ 闭包捕获要穿过插值串、数组下标/字面量、结构体字面量、嵌套闭包（37）：
#     捕获分析只认 17 种节点，其余落兜底被吞，外层变量只在这些位置出现就漏捕，
#     报「未声明的变量」。插值串最要命 —— 字符串一律用模板串，写闭包必撞
#   ⑯ 插值洞里能直接写字符串字面量（38）：预处理链每个 pass 原来都「碰到 " 就读到
#     下一个 "」，洞里的 "1" 把外层串截断，报「插值未闭合」。还要保证洞里字面量的
#     全角冒号不被全角冒号那一 pass 改掉。洞里写 \" 以前静默输出空串，改为报错（39）
#   ⑰ 字符串插值降成一次 qi_string_format_parts（41）：数值洞直接格式化进结果、
#     N 段一次分配；输出与老 `+` 链逐字节一致，洞里新建的临时串要释放、变量不许
#     释放 —— 跑 QI_RC_REPORT=1 查零泄漏（顺带抓到 填充模板 结果每次漏一条串）
#   ⑱ 整数字面量 0x / 0b / 下划线分组（42），超出 64 位要报错（48）
#   ⑲ 可增长数组 数组<T>() + 添加 / 弹出（43）：整数/浮点/布尔/字符串/结构体元素、
#     结构体字段里的数组、共享时写时复制、遍历中修改遍历快照；QI_RC_REPORT 全 0。
#     元素类型放错（46）、接收者是临时值没法写回（47）必须编译期报错
#   ⑳ 新建 是上下文关键字（44）：函数名 / 方法名 / 变量名都能叫 新建
#   ㉑ 字符串.字节 / 清空字符串列表 / 清空浮点列表 / 进程.参数（45）
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
for f in "$HERE"/0[1-68]_*.qi "$HERE"/11_*.qi "$HERE"/22_*.qi "$HERE"/27_*.qi "$HERE"/28_*.qi "$HERE"/30_*.qi "$HERE"/31_*.qi "$HERE"/32_*.qi "$HERE"/33_*.qi "$HERE"/34_*.qi "$HERE"/35_*.qi "$HERE"/36_*.qi "$HERE"/37_*.qi "$HERE"/38_*.qi "$HERE"/41_*.qi "$HERE"/4[2-5]_*.qi; do
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
    # 02：ARC 遮蔽变体 / 41：插值一次分配（洞里临时串的释放）——
    # QI_RC_REPORT=1 下活跃对象/字符串/闭包必须全 0
    if [ "$name" = "02_遮蔽_字符串ARC.qi" ] || [ "$name" = "41_插值一次分配.qi" ] \
        || [ "$name" = "43_可增长数组.qi" ] || [ "$name" = "45_标准库小缺口.qi" ]; then
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

# ── 编译确定性：同一源码编译 5 次，产物 md5 完全一致 ──
#   04：模块限定分发（最早的那条）
#   40：出口释放 RC 局部 / main 出口释放 RC 全局 / 协程 cleanup —— 三处原来都直接
#       iter() 一张 HashMap，release 顺序跟着随机哈希种子走，全仓 1528 个程序里
#       385 个连编两次产物不同
det_check() {
    total=$((total+1))
    det_src="$1"
    det_tag="$2"
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
        # 同时比 IR（QI_EMIT_LL）：二进制不同时，IR 能直接指出差在哪个函数。
        det_err=$(QI_EMIT_LL=/tmp/codegen回归_det.ll run_limited "$QI" compile "$det_src" -o "$out" 2>&1)
        if [ $? -ne 0 ]; then
            det_ok=0
            det_why="第 $i 次编译失败: $(echo "$det_err" | head -3 | tr '\n' ' ')"
            break
        fi
        m=$(md5 -q "$out" 2>/dev/null || md5sum "$out" | cut -d' ' -f1)
        m_ir=$(md5 -q /tmp/codegen回归_det.ll 2>/dev/null || md5sum /tmp/codegen回归_det.ll | cut -d' ' -f1)
        m="$m/$m_ir"
        det_md5s="$det_md5s $m"
        # 留一份副本：不一致时要拿它指出**差在第几个字节**，
        # 否则只知道「不一致」，得再烧一轮 CI 才知道是时间戳还是别的。
        cp "$out" "/tmp/codegen回归_det_副本$i.bin" 2>/dev/null
        if [ -z "$first_md5" ]; then first_md5="$m"
        # 只记**第一处**不一致：原来没有 break、每次覆盖，永远报「第 5 次」，
    # 看上去像前四次都一致、只是偶发 —— 实际是每次都不同。
    elif [ "$m" != "$first_md5" ] && [ $det_ok -eq 1 ]; then det_ok=0; det_why="第 $i 次与第 1 次不同"
        fi
    done
    if [ $det_ok -eq 1 ]; then
        echo "PASS ${det_tag}(编译5次产物一致)"
        passed=$((passed+1))
    else
        echo "FAIL ${det_tag}(编译5次产物不一致或编译失败: ${det_why})"
        echo "  各次 md5:${det_md5s}"
        if [ -f /tmp/codegen回归_det_副本1.bin ] && [ -f /tmp/codegen回归_det_副本2.bin ]; then
            echo "  头 20 处差异（字节偏移 十进制，1 起）:"
            cmp -l /tmp/codegen回归_det_副本1.bin /tmp/codegen回归_det_副本2.bin 2>&1 | head -20 | sed 's/^/    /'
        fi
        failed=$((failed+1))
    fi
    rm -f /tmp/codegen回归_det.bin /tmp/codegen回归_det.ll /tmp/codegen回归_det.lib /tmp/codegen回归_det.exp \
          /tmp/codegen回归_det_副本*.bin
}
det_check "$HERE/04_模块限定分发确定性.qi" "04"
det_check "$HERE/40_编译确定性_RC局部全局协程.qi" "40"
# 并行机器码生成（按函数切片、多线程、多个目标文件）也必须确定：分片数固定、
# 装箱平局按编号。小程序默认不切，这里强制切 4 份。
QI_CODEGEN_PARTITIONS=4 det_check "$HERE/40_编译确定性_RC局部全局协程.qi" "40并行codegen"

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
  "29_内置元数不符_必须报错.qi::需要 2 个实参" \
  "39_插值洞里转义引号_必须报错.qi::字符串直接写引号、不用转义" \
  "46_添加元素类型不符_必须报错.qi::添加 的元素类型不对" \
  "47_添加接收者是临时值_必须报错.qi::接收者必须是变量或结构体字段" \
  "48_十六进制超出64位_必须报错.qi::超出 64 位"

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
