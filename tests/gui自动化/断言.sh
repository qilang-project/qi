#!/usr/bin/env bash
# GUI 自动化冒烟 —— 靠 QI_GUI_AUTOCLOSE_MS 测试钩子把「开了窗等人关」变成可断言
#
# GUI 测试的死结：程序开了窗就等用户去关，CI 里没有用户，脚本只能挂死或者被
# timeout 杀掉（拿到的是 124，不是干净的退出码，分不清"跑通了"和"崩了"）。
# qi-gui 的 `帧开始` 认一个环境变量 QI_GUI_AUTOCLOSE_MS：设了之后，应用创建满
# 该毫秒数时 `帧开始` 返回 0 —— 效果等同用户关窗，qi 侧主循环正常结束，
# 走完 关闭应用 后**正常退出，退出码 0**。于是"跑起来没崩"变成可断言的事实。
#
# 本脚本断言：
#   ① 键盘演示.qi 带钩子跑 2 秒，退出码 0，且确实跑满了（不是秒退＝创建失败）
#   ② 控件演示.qi 同样（老例子防回归：键盘快照接在 帧开始 里，别把控件轨搞坏）
#   ③ 日志干净：不出现「认不出的键名」警告、不出现 panic
#
# 无显示环境（无头 CI、ssh 无 X）里 winit 起不来 —— 那不是回归，是环境没有。
# 脚本先探测一次，起不来就打 SKIP 并**退 0**，绝不假红。
#
# 用法：qi/tests/gui自动化/断言.sh [qi二进制路径]
#   需要 GUI 版运行时：export QI_RUNTIME_LIB=<qi-runtime>/target/release/libqi_runtime.a
#   （那份归档必须是 cargo build --release --features gui 编的，否则 GUI 全是 stub）
#
# 注意 macOS bash 3.2：shell 变量名与函数名一律 ASCII。
set -uo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
QI_DIR="$(cd "$HERE/../.." && pwd)"
ROOT="$(cd "$QI_DIR/.." && pwd)"
QI="${1:-$ROOT/target/release/qi}"
DEMO_DIR="$QI_DIR/示例/图形界面"

total=0
passed=0
failed=0

if [ ! -x "$QI" ]; then
    echo "SKIP: 找不到 qi 二进制 ${QI}（先 make build）"
    exit 0
fi

# ── 探测：GUI 能不能起来 ──────────────────────────────────────────
# 用最短的钩子跑一遍键盘演示。窗口起不来时 qi-gui 会往 stderr 打
# 「创建事件循环失败」/「窗口未能创建」/「softbuffer …失败」，
# qi 侧则打「应用创建失败」。命中任一条就是环境没有显示，不是代码坏了。
probe_log=$(mktemp -t qi_gui_probe)
QI_GUI_AUTOCLOSE_MS=300 "$QI" run "$DEMO_DIR/键盘演示.qi" >"$probe_log" 2>&1
probe_rc=$?
if grep -qE "创建事件循环失败|窗口未能创建|应用创建失败|softbuffer" "$probe_log"; then
    echo "SKIP: 当前环境起不来窗口（无头/无显示），GUI 冒烟跳过"
    echo "----- 探测输出 -----"
    cat "$probe_log"
    rm -f "$probe_log"
    exit 0
fi
if [ $probe_rc -ne 0 ]; then
    # 起得来窗口却非零退出：这是真失败，往下走让正式用例报出来
    echo "注意：探测运行退出码 ${probe_rc}，继续跑正式用例定位"
fi
rm -f "$probe_log"

# ── 用例：带钩子跑一个 GUI 示例，断言它**正常走完了主循环** ──────────
# 真正的判据是 stdout 里的「窗口已关闭」，退出码只是配菜（理由见函数体内注释）。
# run_case <用例名> <qi 文件> <钩子毫秒> <最少帧数>
#   最少帧数 > 0 时，示例必须打印「共渲染 N 帧」且 N 不小于它。传 0 表示不查
#   （老示例没有帧计数，不为了这条测试去改它们）。
run_case() {
    local name="$1"
    local file="$2"
    local ms="$3"
    local min_frames="${4:-0}"
    total=$((total+1))

    local log
    log=$(mktemp -t qi_gui_case)
    local t0 t1 elapsed rc
    t0=$(date +%s)
    QI_GUI_AUTOCLOSE_MS="$ms" "$QI" run "$file" >"$log" 2>&1
    rc=$?
    t1=$(date +%s)
    elapsed=$((t1-t0))

    local min_sec=$(( ms / 1000 ))
    local why=""
    if [ $rc -ne 0 ]; then
        why="退出码 ${rc}（期望 0）"
    elif grep -q "应用创建失败" "$log"; then
        why="应用创建失败 —— 窗口根本没起来"
    elif ! grep -q "窗口已关闭" "$log"; then
        # 关键断言：两个示例都只在**主循环正常结束之后**才打这一行。
        # 只看退出码是假绿 —— 创建窗口失败时示例走的是 返回 而不是 exit 1，
        # 照样 0；靠时长也不行，`qi run` 光编译就要好几秒，把钩子那 2 秒淹了。
        # 这一行出现 = 帧开始 确实返回过 0 = 自动关窗钩子真的生效了。
        why="没等到「窗口已关闭」—— 主循环没有正常走完"
    elif [ $elapsed -gt $((min_sec + 30)) ]; then
        why="跑了 ${elapsed}s，远超钩子时长 ${min_sec}s —— 自动关窗没生效？"
    elif grep -q "认不出的键名" "$log"; then
        why="日志里有「认不出的键名」警告，示例用了错的键名"
    elif grep -qE "panicked at|RUST_BACKTRACE" "$log"; then
        why="日志里有 panic"
    fi

    # 帧数断言。**这条是最硬的一条**：开了窗但一帧没画，程序照样打
    # 「窗口已关闭」、退出码照样 0 —— 只有帧数能把这种假绿揪出来。
    # （踩过：钩子最初从"应用创建"起算，而 macOS 上建窗 + 载 CJK 字体就要 2.4 秒，
    #  于是 2000ms 的用例一帧都没跑，前面几条断言全绿。）
    local frames
    if [ -z "$why" ] && [ "$min_frames" -gt 0 ]; then
        frames=$(sed -n 's/.*共渲染 \([0-9]*\) 帧.*/\1/p' "$log" | tail -1)
        if [ -z "$frames" ]; then
            why="日志里没有「共渲染 N 帧」，无法确认真的渲染过"
        elif [ "$frames" -lt "$min_frames" ]; then
            why="只渲染了 ${frames} 帧，少于下限 ${min_frames} —— 帧循环没真正跑起来"
        else
            name="${name}，${frames} 帧"
        fi
    fi

    if [ -z "$why" ]; then
        echo "PASS: ${name}（${elapsed}s，退出码 0，日志干净）"
        passed=$((passed+1))
    else
        echo "FAIL: $name —— $why"
        echo "----- 输出 -----"
        cat "$log"
        echo "----------------"
        failed=$((failed+1))
    fi
    rm -f "$log"
}

echo "GUI 自动化冒烟（QI_GUI_AUTOCLOSE_MS 钩子）"
echo "qi = $QI"
echo

# 帧数下限取 20 而不是"2 秒 × 60fps ≈ 120"：这条断言要抓的是**一帧都没画**的
# 回归，不是跑性能。软件光栅在忙机器上掉到三四十帧很正常，卡着 120 只会变成 flake。
run_case "键盘演示 自动关窗 2 秒" "$DEMO_DIR/键盘演示.qi" 2000 20
run_case "控件演示 自动关窗 2 秒（防回归）" "$DEMO_DIR/控件演示.qi" 2000 0

echo
echo "合计 ${total}，通过 ${passed}，失败 $failed"
[ $failed -eq 0 ]
