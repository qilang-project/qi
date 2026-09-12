#!/usr/bin/env bash
# wasm HTTP 对拍 —— 同一个 .qi 原生跑一遍（reqwest）、wasm 跑一遍（qi_host 宿主导入 →
# node 里的同步 XHR 替身），对着同一台本地服务器，stdout 必须逐字节一致。
#
# 为什么不并进 断言.sh：那套用 wasmtime 跑，wasmtime 不认识 qi_host 这个导入模块
# （它是给浏览器/Worker 宿主的），HTTP 用例在那边只能是「unknown import」。
# 依赖：node ≥ 18、curl。缺了就 SKIP，不把主干拖红。
# 注意 macOS bash 3.2：shell 变量名一律 ASCII。
set -uo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
QI="${1:-$ROOT/target/release/qi}"
export QI_RUNTIME_LIB="${QI_RUNTIME_LIB:-$ROOT/qi-runtime/target/release/libqi_runtime.a}"
RT="${QI_WASM_RUNTIME_LIB:-$ROOT/qi-runtime/wasm/target/wasm32-wasip1/release/libqi_runtime_wasm.a}"
export QI_WASM_RUNTIME_LIB="$RT"
HOST="$ROOT/qi/wasm演示/试_http.mjs"

if ! command -v node >/dev/null 2>&1; then echo "wasm HTTP: 跳过（没有 node）"; exit 0; fi
if ! command -v curl >/dev/null 2>&1; then echo "wasm HTTP: 跳过（没有 curl）"; exit 0; fi
if ! rustup target list --installed 2>/dev/null | grep -q '^wasm32-wasip1$'; then echo "wasm HTTP: 跳过（没有 wasm32-wasip1 目标）"; exit 0; fi
if [ ! -f "$RT" ]; then echo "wasm HTTP: 跳过（没有 ${RT}）"; exit 0; fi
case "$QI" in /*) ;; *) QI="$(cd "$(dirname "$QI")" && pwd)/$(basename "$QI")" ;; esac
[ -x "$QI" ] || { echo "找不到 qi：$QI" >&2; exit 1; }
export QI

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

total=0; failed=0
for f in "$HERE"/http/*.qi; do
    [ -e "$f" ] || continue
    name="$(basename "$f")"
    total=$((total+1))
    if ! ( cd "$HERE/http" && timeout 120 "$QI" --target wasm compile "$name" -o "$TMP/prog.wasm" ) > "$TMP/build.log" 2>&1; then
        echo "FAIL ${name}（wasm 编译/链接失败）"; grep -vE "^警告|归档:|workspace|同步本地" "$TMP/build.log" | tail -8 | sed 's/^/    /'; failed=$((failed+1)); continue
    fi
    if ! ( cd "$HERE/http" && timeout 90 node "$HOST" "$name" ) > "$TMP/native" 2>"$TMP/native.err"; then
        echo "FAIL ${name}（原生跑失败）"; sed 's/^/    /' "$TMP/native.err" | grep -v '^    {' | tail -5; failed=$((failed+1)); continue
    fi
    if ! ( cd "$HERE/http" && timeout 90 node "$HOST" "$TMP/prog.wasm" ) > "$TMP/wasm" 2>"$TMP/wasm.err"; then
        echo "FAIL ${name}（wasm 运行失败）"; sed 's/^/    /' "$TMP/wasm.err" | tail -5; failed=$((failed+1)); continue
    fi
    if cmp -s "$TMP/native" "$TMP/wasm"; then
        echo "PASS $name"
    else
        echo "FAIL ${name}（输出不一致）"
        diff "$TMP/native" "$TMP/wasm" | head -10 | sed 's/^/    /'
        failed=$((failed+1))
    fi
done
echo "wasm HTTP: $((total-failed))/$total 通过"
[ "$failed" -gt 0 ] && exit 1
exit 0
