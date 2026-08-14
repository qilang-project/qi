#!/usr/bin/env bash
# FFI 链接控制断言 —— `外部 "库名"` 的三种写法 + 库搜索路径的行为契约：
#   ① --库路径 <目录>  → clang -L<目录>，`外部 "qitest"` 能链上非系统路径的 libqitest.a
#   ② 直链文件写法      → `外部 "lib/libqitest.a"`，相对路径以**源文件所在目录**为基准
#                        （用例 2 故意在别的 CWD 下编译，CWD 基准会当场挂）
#   ③ QI_LIBRARY_PATH  → 环境变量等价于 --库路径（PATH 式多路径）
#   ④ macOS framework  → `外部 "framework:CoreFoundation"` → -framework CoreFoundation
#                        （非 mac 上跳过；改动前这里会变成 -lframework:CoreFoundation 链接失败）
#   ⑤ qi run 与 qi compile 同样生效（run 内部也走同一条链接路径）
#   ⑥ 负例：--库路径 目录不存在 / 直链文件不存在，都要非零退出且给人话
#
# 现场用 cc 编一个 libqitest.a（qi_test_triple(x)=3x），不依赖机器上装了什么库。
# 注意 C 侧签名用 long 而非 int：qi 的 整数 是 i64，C ABI 对应 long ——
# 写 int 在 arm64 上是「调用方传 64 位、被调方只写 w0」，返回值高 32 位是垃圾。
#
# 用法：qi/tests/ffi链接/断言.sh [qi二进制路径]
#   默认用 workspace 的 target/debug/qi。链接要 QI_RUNTIME_LIB 指向 libqi_runtime.a。
# 注意 macOS bash 3.2：shell 变量名一律 ASCII。
set -uo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
QI="${1:-$ROOT/target/debug/qi}"
# 转绝对路径：用例 02 会 cd 到别的目录去编译（验证相对路径基准），
# 相对的 QI 到那儿就成了 rc=127「找不到命令」，看着像编译器坏了。
case "$QI" in
    /*) ;;
    *) QI="$(cd "$(dirname "$QI")" && pwd)/$(basename "$QI")" ;;
esac

# 工作目录放在**仓库内**而不是 /tmp：编译器解析依赖时会扫源文件的每一级祖先目录
# 找 qi.toml，/tmp 下的任何残留包会静默劫持解析（见仓库 CLAUDE.md 的踩坑记录）。
WORK="$HERE/临时"

# 限时执行。macOS 没有 GNU timeout（coreutils 才有，CI 的 macos runner 上没有），
# 没有就退化成不限时跑 —— 与 codegen回归/断言.sh 保持同一写法。
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

pass() { echo "PASS $1"; passed=$((passed+1)); }
fail() { echo "FAIL $1"; failed=$((failed+1)); }

cleanup() { rm -rf "$WORK"; }
trap cleanup EXIT

rm -rf "$WORK"
mkdir -p "$WORK/lib" || exit 1

# ── 准备：现场编一个静态库 ──
cat > "$WORK/qitest.c" <<'EOF'
/* qi 的 整数 ↔ C 的 long（i64）。测试用：返回 3 倍。 */
long qi_test_triple(long x) { return x * 3; }
EOF
if ! cc -c -o "$WORK/qitest.o" "$WORK/qitest.c" 2>"$WORK/cc.err"; then
    echo "FAIL 准备阶段：cc 编不出测试库"
    cat "$WORK/cc.err"
    exit 1
fi
if ! ar rcs "$WORK/lib/libqitest.a" "$WORK/qitest.o" 2>"$WORK/ar.err"; then
    echo "FAIL 准备阶段：ar 打不出 libqitest.a"
    cat "$WORK/ar.err"
    exit 1
fi

# 三个正例共用的程序体：3 * 14 = 42
写用例() {
    # $1 = 目标 .qi 路径，$2 = 外部块的库名
    cat > "$1" <<EOF
外部 "$2" {
    函数 qi_test_triple(x: 整数): 整数;
}

函数 入口() {
    变量 n: 整数 = qi_test_triple(14);
    打印行(n);
}
EOF
}

# 编译 + 运行 + 断言输出 42。$1=用例名 $2=.qi 路径 $3=可执行输出 $4.. = 额外 qi 参数
编译运行断言42() {
    case_name="$1"; src="$2"; bin="$3"; shift 3
    total=$((total+1))
    out=$(run_limited "$QI" compile "$src" -o "$bin" "$@" 2>&1)
    rc=$?
    if [ $rc -ne 0 ]; then
        fail "$case_name (编译失败 rc=$rc: $(echo "$out" | head -3 | tr '\n' ' '))"
        return
    fi
    got=$(run_limited "$bin" 2>&1)
    rc=$?
    if [ $rc -ne 0 ]; then
        fail "$case_name (运行失败 rc=$rc: $got)"
        return
    fi
    if [ "$got" != "42" ]; then
        fail "$case_name (期望 42，实际: $got)"
        return
    fi
    pass "$case_name"
}

# ── ① --库路径 + -l 写法 ──
写用例 "$WORK/01_库路径.qi" "qitest"
编译运行断言42 "01 --库路径 + 外部 \"qitest\"" \
    "$WORK/01_库路径.qi" "$WORK/bin01" --库路径 "$WORK/lib"

# ── ② 直链文件写法（相对路径，基准 = 源文件所在目录）──
# 故意在 $ROOT 下调用，且相对路径 lib/libqitest.a 相对 CWD 并不存在 ——
# 只有以源文件目录为基准才找得到。
写用例 "$WORK/02_直链相对.qi" "lib/libqitest.a"
total=$((total+1))
out=$(cd "$ROOT" && run_limited "$QI" compile "$WORK/02_直链相对.qi" -o "$WORK/bin02" 2>&1)
rc=$?
if [ $rc -ne 0 ]; then
    fail "02 直链相对路径 (编译失败 rc=$rc: $(echo "$out" | head -3 | tr '\n' ' '))"
else
    got=$("$WORK/bin02" 2>&1)
    if [ "$got" = "42" ]; then
        pass "02 直链相对路径(基准=源文件目录)"
    else
        fail "02 直链相对路径 (期望 42，实际: $got)"
    fi
fi

# 直链绝对路径写法
写用例 "$WORK/03_直链绝对.qi" "$WORK/lib/libqitest.a"
编译运行断言42 "03 直链绝对路径" "$WORK/03_直链绝对.qi" "$WORK/bin03"

# ── ③ QI_LIBRARY_PATH 环境变量 ──
写用例 "$WORK/04_env.qi" "qitest"
total=$((total+1))
out=$(QI_LIBRARY_PATH="$WORK/lib" run_limited "$QI" compile "$WORK/04_env.qi" -o "$WORK/bin04" 2>&1)
rc=$?
if [ $rc -ne 0 ]; then
    fail "04 QI_LIBRARY_PATH (编译失败 rc=$rc: $(echo "$out" | head -3 | tr '\n' ' '))"
else
    got=$("$WORK/bin04" 2>&1)
    if [ "$got" = "42" ]; then
        pass "04 QI_LIBRARY_PATH"
    else
        fail "04 QI_LIBRARY_PATH (期望 42，实际: $got)"
    fi
fi

# PATH 式多路径：前面塞一个不存在的目录，后面才是真的 —— 不存在的应被静默跳过。
total=$((total+1))
out=$(QI_LIBRARY_PATH="$WORK/没这个目录:$WORK/lib" \
      run_limited "$QI" compile "$WORK/04_env.qi" -o "$WORK/bin04b" 2>&1)
rc=$?
if [ $rc -eq 0 ] && [ "$("$WORK/bin04b" 2>&1)" = "42" ]; then
    pass "05 QI_LIBRARY_PATH 多路径(不存在的静默跳过)"
else
    fail "05 QI_LIBRARY_PATH 多路径 (rc=$rc: $(echo "$out" | head -3 | tr '\n' ' '))"
fi

# ── ⑤ qi run 也生效 ──
total=$((total+1))
got=$(run_limited "$QI" --库路径 "$WORK/lib" run "$WORK/01_库路径.qi" 2>&1)
rc=$?
if [ $rc -eq 0 ] && [ "$got" = "42" ]; then
    pass "06 qi run + --库路径"
else
    fail "06 qi run + --库路径 (rc=$rc, 输出: $(echo "$got" | head -3 | tr '\n' ' '))"
fi

# ── ④ macOS framework 写法 ──
if [ "$(uname -s)" = "Darwin" ]; then
    cat > "$WORK/07_框架.qi" <<'EOF'
外部 "framework:CoreFoundation" {
    函数 CFAbsoluteTimeGetCurrent(): 浮点数;
}

函数 入口() {
    变量 t: 浮点数 = CFAbsoluteTimeGetCurrent();
    如果 (t > 0.0) {
        打印行("正数");
    } 否则 {
        打印行("非正数");
    }
}
EOF
    total=$((total+1))
    out=$(run_limited "$QI" compile "$WORK/07_框架.qi" -o "$WORK/bin07" 2>&1)
    rc=$?
    if [ $rc -ne 0 ]; then
        fail "07 framework:CoreFoundation (编译失败 rc=$rc: $(echo "$out" | head -3 | tr '\n' ' '))"
    else
        got=$("$WORK/bin07" 2>&1)
        if [ "$got" = "正数" ]; then
            pass "07 framework:CoreFoundation"
        else
            fail "07 framework:CoreFoundation (期望 正数，实际: $got)"
        fi
    fi
else
    echo "SKIP 07 framework 写法（非 macOS）"
fi

# ── ⑥ 负例：必须非零退出且错误信息是人话 ──
# 每行「用例名::关键词」，用位置参数而非数组（bash 3.2 + set -u 的老坑）。
写用例 "$WORK/08_坏库路径.qi" "qitest"
cat > "$WORK/09_坏直链.qi" <<'EOF'
外部 "lib/根本没有这个库.a" {
    函数 qi_test_triple(x: 整数): 整数;
}

函数 入口() {
    打印行(qi_test_triple(14));
}
EOF

total=$((total+1))
out=$(run_limited "$QI" compile "$WORK/08_坏库路径.qi" -o "$WORK/bin08" \
      --库路径 "$WORK/这个目录不存在" 2>&1)
rc=$?
if [ $rc -ne 0 ] && echo "$out" | grep -q "不存在"; then
    pass "08 负例：--库路径 目录不存在"
else
    fail "08 负例：--库路径 目录不存在 (rc=$rc，输出: $(echo "$out" | head -2 | tr '\n' ' '))"
fi

total=$((total+1))
out=$(run_limited "$QI" compile "$WORK/09_坏直链.qi" -o "$WORK/bin09" 2>&1)
rc=$?
if [ $rc -ne 0 ] && echo "$out" | grep -q "找不到这个库文件"; then
    pass "09 负例：直链文件不存在"
else
    fail "09 负例：直链文件不存在 (rc=$rc，输出: $(echo "$out" | head -2 | tr '\n' ' '))"
fi

echo "ffi链接: $passed/$total 通过"
[ $failed -eq 0 ] || exit 1
