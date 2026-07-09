#!/usr/bin/env bash
# 多线程 FFI 重入 + qi_await 异步桥 —— 一键构建并压测。
#
# 用法：
#   ./构建并压测.sh            # 普通构建 + 运行（QI_RC_REPORT=1 看 ARC 归零）
#   ./构建并压测.sh asan       # AddressSanitizer 构建 + 运行
#   ./构建并压测.sh tsan       # ThreadSanitizer 构建 + 运行（线程更少以控内存）
#
# 依赖：已构建的 qi 编译器（../../../../target/debug/qi）与 qi-runtime 静态库。
set -euo pipefail
cd "$(dirname "$0")"

QI="${QI:-../../../../target/debug/qi}"
MODE="${1:-normal}"

echo "==> 编译 Qi 导出库 → 静态库 + C 头文件"
"$QI" compile --库 静态 导出库.qi -o lib多线程FFI库.a

# macOS 需要的系统框架（reqwest/rustls/GUI 传递依赖）；Linux 用 -lpthread -lm -ldl。
if [ "$(uname -s)" = "Darwin" ]; then
  SYS_LIBS=(-framework Security -framework CoreFoundation -framework SystemConfiguration
            -framework CoreServices -framework AppKit -framework AudioUnit -framework AudioToolbox
            -framework CoreAudio -framework Cocoa -framework QuartzCore -framework Carbon
            -framework CoreGraphics -framework CoreVideo)
else
  SYS_LIBS=(-lpthread -lm -ldl)
fi

SAN_FLAGS=()
DEFS=()
BIN=压测
case "$MODE" in
  asan) SAN_FLAGS=(-fsanitize=address -g); BIN=压测_asan
        # ASan 拦截每次分配/访问，1000 线程 × 异步 block_on 极慢；线程/循环收敛，
        # 仍覆盖 整数+字符串+异步 全路径，足以查 UAF / double-free / 越界。
        DEFS=(-D线程数=200 -D同步次数=500 -D异步次数=50)
        echo "==> AddressSanitizer 模式（线程/循环收敛）" ;;
  tsan) SAN_FLAGS=(-fsanitize=thread -g); BIN=压测_tsan
        # TSan 影子内存开销大：线程数与循环数收敛，仍足以证明并发正确
        DEFS=(-D线程数=200 -D同步次数=500 -D异步次数=50)
        echo "==> ThreadSanitizer 模式（线程/循环收敛）" ;;
  normal) echo "==> 普通模式" ;;
  *) echo "未知模式: $MODE（用 normal|asan|tsan）"; exit 2 ;;
esac

echo "==> 链接 C 压测驱动"
clang -O1 "${SAN_FLAGS[@]}" "${DEFS[@]}" -o "$BIN" 压测.c lib多线程FFI库.a "${SYS_LIBS[@]}"

echo "==> 运行（QI_RC_REPORT=1：退出时报告 ARC 活跃计数，应全 0）"
QI_RC_REPORT=1 "./$BIN"
