#!/usr/bin/env bash
# wasm 回归 —— 同一个 .qi 原生跑一遍、wasm（wasmtime）跑一遍，stdout 必须逐字节一致。
#
# 不维护 .期望 文件：期望就是原生的输出。这样测的是「wasm 目标与原生行为一致」，
# 而不是「wasm 目标与某天手抄的一份文本一致」。
#
# 用法：qi/tests/wasm/断言.sh [qi二进制路径]
#   依赖：rustup target add wasm32-wasip1（wasm-ld + wasi libc）、brew/apt 装 wasmtime、
#   qi-runtime/wasm 已 cargo build --release --target wasm32-wasip1。
#   缺任何一样直接 exit 0 跳过（打印原因）—— 这套不是每台机器都有，别把主干门禁拖红。
# 注意 macOS bash 3.2：shell 变量名一律 ASCII。
set -uo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
QI="${1:-$ROOT/target/release/qi}"
export QI_RUNTIME_LIB="${QI_RUNTIME_LIB:-$ROOT/qi-runtime/target/release/libqi_runtime.a}"
RT="$ROOT/qi-runtime/wasm/target/wasm32-wasip1/release/libqi_runtime_wasm.a"

if ! command -v wasmtime >/dev/null 2>&1; then echo "wasm回归: 跳过（没有 wasmtime）"; exit 0; fi
if ! rustup target list --installed 2>/dev/null | grep -q '^wasm32-wasip1$'; then echo "wasm回归: 跳过（没有 wasm32-wasip1 目标）"; exit 0; fi
if [ ! -f "$RT" ]; then echo "wasm回归: 跳过（没有 $RT，先 cd qi-runtime/wasm && cargo build --release --target wasm32-wasip1）"; exit 0; fi
[ -x "$QI" ] || { echo "找不到 qi：$QI" >&2; exit 1; }

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

total=0; failed=0
for f in "$HERE"/*.qi; do
    [ -e "$f" ] || continue
    name="$(basename "$f")"
    total=$((total+1))
    # 原生
    if ! ( cd "$HERE" && timeout 60 "$QI" run "$name" ) > "$TMP/native" 2>"$TMP/native.err"; then
        echo "FAIL $name（原生跑失败）"; sed 's/^/    /' "$TMP/native.err" | tail -5; failed=$((failed+1)); continue
    fi
    # wasm：一步编译（含链接）
    if ! ( cd "$HERE" && timeout 120 "$QI" --target wasm compile "$name" -o "$TMP/prog.wasm" ) > "$TMP/build.log" 2>&1; then
        echo "FAIL $name（wasm 编译/链接失败）"; grep -vE "^警告|归档:|workspace|同步本地" "$TMP/build.log" | tail -8 | sed 's/^/    /'; failed=$((failed+1)); continue
    fi
    if ! ( cd "$HERE" && timeout 60 wasmtime run --dir . "$TMP/prog.wasm" ) > "$TMP/wasm" 2>"$TMP/wasm.err"; then
        echo "FAIL $name（wasm 运行失败）"; sed 's/^/    /' "$TMP/wasm.err" | tail -5; failed=$((failed+1)); continue
    fi
    if cmp -s "$TMP/native" "$TMP/wasm"; then
        echo "PASS $name"
    else
        echo "FAIL $name（输出不一致）"
        diff "$TMP/native" "$TMP/wasm" | head -10 | sed 's/^/    /'
        failed=$((failed+1))
    fi
done
echo "wasm回归: $((total-failed))/$total 通过"
[ "$failed" -gt 0 ] && exit 1
exit 0
