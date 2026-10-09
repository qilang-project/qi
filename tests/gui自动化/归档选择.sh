#!/usr/bin/env bash
# 运行时归档选择：命令行程序链普通版，GUI 程序链 GUI 版
#
# 发布包里有两份运行时归档（qi/src/runtime_archive.rs）：
#   lib/qi/libqi_runtime.a      普通版，不含 GUI
#   lib/qi/libqi_runtime_gui.a  --features gui 编的，含 winit / softbuffer / rodio
# 编译器只在程序导入 标准库.图形化 时选 GUI 版，并在 Linux 上补 libasound。
#
# 这里搭一个跟发布包同构的目录（bin/qi + lib/qi/两份归档），**清掉所有
# QI_RUNTIME_* 环境变量**，走用户装完包之后的那条查找路径，断言：
#   ① 命令行程序：能编能跑；Linux 上 ldd 里没有 libasound（服务器上不该要音频库）；
#      符号表里没有 winit（没被链进 GUI 版）
#   ② GUI 程序：能编；符号表里有 winit；Linux 上 ldd 里有 libasound.so.2
# 不开窗，不需要显示环境。
#
# 用法：
#   tests/gui自动化/归档选择.sh <qi> <普通版归档> <GUI 版归档>   # 现搭一个发布包布局
#   tests/gui自动化/归档选择.sh --prefix <打好的包目录>           # 直接验 bin/ + lib/qi/
# 后一种给 release.yml 打包之后用：macOS 上 bin/qi 已经 rpath 化，指望
# 同目录下 lib/qi/ 里捆的 dylib，拷出去就起不来，只能原地验。
# 注意 macOS bash 3.2：变量名、函数名一律 ASCII。
set -uo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"

TMP_ROOT="${TMPDIR:-/tmp}"
TMP_ROOT="${TMP_ROOT%/}"
work=$(mktemp -d "$TMP_ROOT/qi_archive_pick.XXXXXX")
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/src"

if [ "${1:-}" = "--prefix" ]; then
    PREFIX="$(cd "${2:?--prefix 后面要跟包目录}" && pwd)"
else
    QI="${1:?用法: 归档选择.sh <qi> <普通版归档> <GUI 版归档> | --prefix <包目录>}"
    PLAIN_LIB="${2:?缺普通版归档}"
    GUI_LIB="${3:?缺 GUI 版归档}"
    # 发布包布局。必须拷贝而不是软链：Linux 的 current_exe 会解开软链，
    # 编译器就会按真身的位置去找 lib/qi/。
    PREFIX="$work/pkg"
    mkdir -p "$PREFIX/bin" "$PREFIX/lib/qi"
    cp "$QI" "$PREFIX/bin/qi"
    cp "$PLAIN_LIB" "$PREFIX/lib/qi/libqi_runtime.a"
    cp "$GUI_LIB" "$PREFIX/lib/qi/libqi_runtime_gui.a"
fi
for f in "$PREFIX/bin/qi" "$PREFIX/lib/qi/libqi_runtime.a" "$PREFIX/lib/qi/libqi_runtime_gui.a"; do
    [ -f "$f" ] || { echo "FAIL: 找不到 $f"; exit 1; }
done
cp "$HERE/脚本冒烟.qi" "$work/src/gui.qi"
cat > "$work/src/cli.qi" <<'EOF'
包 主程序;
导入 标准库.输入输出 作为 IO;
函数 入口() {
    IO.打印行("命令行程序跑起来了");
}
EOF

total=0
failed=0
check() {
    total=$((total+1))
    if [ "$2" = "0" ]; then
        echo "PASS: $1"
    else
        echo "FAIL: $1"
        failed=$((failed+1))
    fi
}

compile() {
    (cd "$work/src" && env -u QI_RUNTIME_LIB -u QI_RUNTIME_GUI_LIB QI_NO_CACHE=1 QI_NO_STALE_WARN=1 \
        "$PREFIX/bin/qi" compile "$1.qi" -o "$1" >"$1.log" 2>&1)
}

# 别写 `nm | grep -q`：pipefail 下 grep -q 一命中就退出，nm 吃 SIGPIPE，
# 整条管道返回 141，「找到了」反而成了失败。先数出来再比。
has_winit() {
    local n
    n=$(nm "$1" 2>/dev/null | grep -c winit)
    [ "${n:-0}" -gt 0 ]
}

needs_asound() {
    local n
    n=$(ldd "$1" 2>/dev/null | grep -c libasound)
    [ "${n:-0}" -gt 0 ]
}

compile cli
rc=$?
[ $rc -ne 0 ] && cat "$work/src/cli.log"
check "命令行程序编译成功" "$rc"
if [ $rc -eq 0 ]; then
    out=$("$work/src/cli" 2>&1)
    echo "$out" | grep -q "命令行程序跑起来了"
    check "命令行程序运行输出正确" "$?"
    if has_winit "$work/src/cli"; then r=1; else r=0; fi
    check "命令行程序没有链进 winit（用的是普通版归档）" "$r"
    if [ "$(uname -s)" = "Linux" ]; then
        if needs_asound "$work/src/cli"; then r=1; ldd "$work/src/cli"; else r=0; fi
        check "命令行程序不依赖 libasound" "$r"
    fi
fi

compile gui
rc=$?
[ $rc -ne 0 ] && cat "$work/src/gui.log"
check "GUI 程序编译成功" "$rc"
if [ $rc -eq 0 ]; then
    has_winit "$work/src/gui"
    check "GUI 程序链进了 winit（用的是 GUI 版归档）" "$?"
    if [ "$(uname -s)" = "Linux" ]; then
        needs_asound "$work/src/gui"
        check "GUI 程序依赖 libasound.so.2" "$?"
    fi
fi

echo
echo "合计 ${total}，失败 $failed"
[ $failed -eq 0 ]
