#!/usr/bin/env bash
# Qi → WebAssembly：编译（含链接）→ wasmtime 跑 → 提示浏览器版
#
#   ./构建.sh            # 试炼.qi
#   ./构建.sh 你好.qi     # 换一个源文件
#
# `qi --target wasm compile 源.qi -o 源.wasm` 一步到位：编译器自己找
#   - wasm-ld（rustup 的 wasm32-wasip1 目标自带 rust-lld）
#   - wasi sysroot（同上，crt1-command.o + libc.a）
#   - wasm 运行时归档 qi-runtime/wasm/target/wasm32-wasip1/release/libqi_runtime_wasm.a
# 三样都能用 QI_WASM_LD / QI_WASM_SYSROOT / QI_WASM_RUNTIME_LIB 指定。
# 这个脚本只负责：确保运行时归档是新的，然后跑。
#
# 注意：shell 变量名只能用 ASCII（macOS 自带 bash 3.2 不认中文变量名）。
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
src="${1:-试炼.qi}"
stem="${src%.qi}"

qic="${QI:-$here/../../target/release/qi}"
if [ ! -x "$qic" ] && command -v qi >/dev/null 2>&1; then
  qic="$(command -v qi)"
fi
rt_src="$here/../../qi-runtime/wasm"

if ! rustup target list --installed 2>/dev/null | grep -q '^wasm32-wasip1$'; then
  echo "缺 wasm32-wasip1 目标 —— 先跑：rustup target add wasm32-wasip1" >&2
  exit 1
fi
if ! command -v wasmtime >/dev/null 2>&1; then
  echo "缺少 wasmtime —— 先跑：brew install wasmtime" >&2
  exit 1
fi
if [ ! -x "$qic" ]; then
  echo "缺少编译器 $qic —— 先在 workspace 根跑：cargo build --release -p qi-compiler" >&2
  exit 1
fi

cd "$here"

echo "==> 1/2 构建 wasm 运行时（复用主运行时源码，改了那边这里也要重编）"
( cd "$rt_src" && cargo build --release --target wasm32-wasip1 2>&1 | grep -E "^(error|warning: unused)" || true )
echo "    归档：$(du -h "$rt_src/target/wasm32-wasip1/release/libqi_runtime_wasm.a" | cut -f1)"

echo "==> 2/2 编译 $src → $stem.wasm"
"$qic" --target wasm compile "$src" -o "$stem.wasm"
echo "    产物：$stem.wasm  $(du -h "$stem.wasm" | cut -f1)"

echo "==> 运行（wasmtime）"
echo "----------------------------------------"
wasmtime run --dir . "$stem.wasm"
echo "----------------------------------------"
echo "浏览器版：在本目录起个静态服务器后打开 index.html"
echo "  python3 -m http.server 43510   →  http://localhost:43510/index.html"
