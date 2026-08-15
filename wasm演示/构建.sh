#!/usr/bin/env bash
# Qi → WebAssembly 一条龙：编译 → 链接 → 跑
#
#   ./构建.sh            # 构建并用 wasmtime 跑 试炼.qi
#   ./构建.sh 你好.qi     # 换一个源文件
#
# 三段路：
#   1. qi --target wasm compile  →  wasm32 目标文件（.o，编译器已内建支持）
#   2. wasm-ld                   →  .wasm（链最小运行时 + wasi libc）
#   3. wasmtime                  →  跑
#
# 链接器和 wasi sysroot 全部来自 rustup 装的 wasm32-wasip1 目标 ——
# 不需要 wasi-sdk、不需要 sudo、不需要 brew 装 lld。
#
# 注意：shell 变量名只能用 ASCII（macOS 自带 bash 3.2 不认中文变量名），
# 中文只留在注释和给人看的输出里。
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
src="${1:-试炼.qi}"
stem="${src%.qi}"

qic="$here/../target-wt/release/qi"
rt_src="$here/../../qi-runtime/wasm最小"
rt_lib="$rt_src/target/wasm32-wasip1/release/libqi_runtime_wasm.a"

# rustup 的 wasm32-wasip1 目标自带 wasi-libc（libc.a + crt1-command.o）和 rust-lld
sysroot_root="$(rustc --print sysroot)"
sysroot="$sysroot_root/lib/rustlib/wasm32-wasip1/lib/self-contained"
host_triple="$(rustc -vV | awk '/^host:/{print $2}')"
wasm_ld="$sysroot_root/lib/rustlib/$host_triple/bin/gcc-ld/wasm-ld"

for need in "$sysroot/libc.a" "$sysroot/crt1-command.o" "$wasm_ld"; do
  if [ ! -e "$need" ]; then
    echo "缺少 $need —— 先跑：rustup target add wasm32-wasip1" >&2
    exit 1
  fi
done
if ! command -v wasmtime >/dev/null 2>&1; then
  echo "缺少 wasmtime —— 先跑：brew install wasmtime" >&2
  exit 1
fi
if [ ! -x "$qic" ]; then
  echo "缺少编译器 $qic —— 先在 qi/ 跑：CARGO_TARGET_DIR=\$PWD/target-wt cargo build --release" >&2
  exit 1
fi

cd "$here"

echo "==> 1/3 构建 wasm 最小运行时"
( cd "$rt_src" && cargo build --release --target wasm32-wasip1 )
echo "    归档：$(du -h "$rt_lib" | cut -f1)"

echo "==> 2/3 编译 $src → $stem.o"
"$qic" --target wasm compile "$src" -o "$stem.o"

echo "==> 3/3 链接 → $stem.wasm"
"$wasm_ld" \
  "$sysroot/crt1-command.o" \
  "$stem.o" \
  "$rt_lib" \
  -L"$sysroot" -lc \
  --strip-debug \
  -o "$stem.wasm"
echo "    产物：$stem.wasm  $(du -h "$stem.wasm" | cut -f1)"
rm -f "$stem.o"

echo "==> 运行（wasmtime）"
echo "----------------------------------------"
wasmtime "$stem.wasm"
echo "----------------------------------------"
echo "浏览器版：在本目录起个静态服务器后打开 index.html"
echo "  python3 -m http.server 43510   →  http://localhost:43510/index.html"
