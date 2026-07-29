#!/usr/bin/env bash
# 把本地源码构建出来的 qi 编译器 + qi-runtime 归档装到已安装的位置。
#
# 解决什么问题：
#   1. qi-runtime **不是 workspace 成员**，有自己的 target/。在仓库根跑
#      `cargo build` 根本编不到它 —— 改了 runtime 却链着上一次的归档，不报错、
#      只是行为对不上（踩过：WS 客户端的 bug 修完测了半天没反应）。
#   2. 编译器找归档时「安装位置」优先于源码树，装过一次之后 <prefix>/lib/qi/
#      那份就会一直被用，除非设了 QI_RUNTIME_LIB 或手动同步。
#   本脚本把两件事一起做掉：构建 → 安装 → 真编一个程序验证链得上。
#
# 用法：
#   qi/scripts/同步本地构建.sh            # 构建 + 安装 + 校验
#   qi/scripts/同步本地构建.sh --检查      # 只报告是否过期，不改任何东西（CI 用）
#   qi/scripts/同步本地构建.sh --跳过构建  # 只安装现有产物
#
# 安装位置默认跟着 `which qi` 走；也可以 QI_PREFIX=/opt/qi 指定。
#
# 注意 macOS 自带 bash 3.2：shell 变量名/函数名一律 ASCII（中文只出现在注释和输出里）。
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"

if [ -t 1 ]; then
    B=$'\e[1m'; R=$'\e[31m'; G=$'\e[32m'; Y=$'\e[33m'; N=$'\e[0m'
else
    B=""; R=""; G=""; Y=""; N=""
fi
die()  { echo "${R}错误${N}: $*" >&2; exit 1; }
warn() { echo "${Y}!${N} $*"; }
ok()   { echo "${G}✓${N} $*"; }
step() { echo "${B}→${N} $*"; }

CHECK_ONLY=0
NO_BUILD=0
for arg in "$@"; do
    case "$arg" in
        --检查|--check)     CHECK_ONLY=1 ;;
        --跳过构建|--no-build) NO_BUILD=1 ;;
        -h|--help) sed -n '2,17p' "$0"; exit 0 ;;
        *) die "不认识的参数：$arg（用 --help 看用法）" ;;
    esac
done

# ── 定位安装位置 ──────────────────────────────────────────────
if [ -n "${QI_PREFIX:-}" ]; then
    PREFIX="$QI_PREFIX"
else
    INSTALLED="$(command -v qi 2>/dev/null || true)"
    [ -n "$INSTALLED" ] || die "PATH 里找不到 qi。先装一次，或用 QI_PREFIX=… 指定安装位置。"
    PREFIX="$(cd "$(dirname "$INSTALLED")/.." && pwd)"   # <prefix>/bin/qi → <prefix>
fi

DST_BIN="$PREFIX/bin/qi"
DST_LIBDIR="$PREFIX/lib/qi"
DST_LIB="$DST_LIBDIR/libqi_runtime.a"

SRC_BIN="$ROOT/target/release/qi"
SRC_LIB="$ROOT/qi-runtime/target/release/libqi_runtime.a"

echo "仓库：  $ROOT"
echo "安装到：$PREFIX"
echo ""

# ── 构建 ──────────────────────────────────────────────────────
if [ "$CHECK_ONLY" = 0 ] && [ "$NO_BUILD" = 0 ]; then
    step "构建 qi-runtime（独立 cargo 项目，不在 workspace 里，容易漏）"
    ( cd "$ROOT/qi-runtime" && cargo build --release ) || die "qi-runtime 构建失败"
    step "构建 qi 编译器"
    ( cd "$ROOT" && cargo build --release -p qi-compiler ) || die "编译器构建失败"
    echo ""
fi

[ -f "$SRC_LIB" ] || die "找不到 $SRC_LIB（先在 qi-runtime/ 跑 cargo build --release）"

# ── 比新旧 ────────────────────────────────────────────────────
# 也比一次「源码 vs 归档」：源码改了但没重新构建，同样是过期的。
if [ -n "$(find "$ROOT/qi-runtime/src" -name '*.rs' -newer "$SRC_LIB" -print -quit 2>/dev/null)" ]; then
    warn "qi-runtime 源码比 target/ 里的归档还新 —— 归档没重新构建过"
fi

LIB_STALE=0
if [ ! -f "$DST_LIB" ]; then
    warn "安装位置没有归档：$DST_LIB"
    LIB_STALE=1
elif [ "$SRC_LIB" -nt "$DST_LIB" ]; then
    warn "已安装的运行时归档比本地构建旧"
    echo "    已装：$(date -r "$DST_LIB" '+%Y-%m-%d %H:%M')  $DST_LIB"
    echo "    本地：$(date -r "$SRC_LIB" '+%Y-%m-%d %H:%M')  $SRC_LIB"
    LIB_STALE=1
else
    ok "运行时归档已是最新（$(date -r "$DST_LIB" '+%Y-%m-%d %H:%M')）"
fi

BIN_STALE=0
if [ -f "$SRC_BIN" ]; then
    if [ ! -f "$DST_BIN" ] || [ "$SRC_BIN" -nt "$DST_BIN" ]; then
        warn "已安装的 qi 编译器比本地构建旧"
        BIN_STALE=1
    else
        ok "qi 编译器已是最新"
    fi
fi

if [ "$CHECK_ONLY" = 1 ]; then
    if [ "$LIB_STALE" = 1 ] || [ "$BIN_STALE" = 1 ]; then
        echo ""
        echo "跑 ${B}qi/scripts/同步本地构建.sh${N} 同步。"
        exit 1
    fi
    exit 0
fi

if [ "$LIB_STALE" = 0 ] && [ "$BIN_STALE" = 0 ]; then
    echo ""
    ok "无需同步"
    exit 0
fi

# ── 安装（目录不可写就自动提权）─────────────────────────────
echo ""
# 拷不动不直接退出：把该手动跑的命令攒起来，最后一次性告诉用户
# （常见情形：/usr/local/bin 归 admin 组可写，/usr/local/lib/qi 归 root，
#  一半能装一半不能。中途 die 掉的话另一半也白搭了。）
MANUAL=""
copy_to() {
    src="$1"; dst="$2"
    if [ -w "$(dirname "$dst")" ]; then
        cp "$src" "$dst" && return 0
    elif sudo -n true 2>/dev/null; then
        sudo cp "$src" "$dst" && return 0
    fi
    MANUAL="$MANUAL  sudo cp $src $dst
"
    warn "$(dirname "$dst") 需要 sudo，跳过（末尾会给出命令）"
    return 1
}

if [ "$LIB_STALE" = 1 ]; then
    step "安装运行时归档 → $DST_LIB"
    if [ ! -d "$DST_LIBDIR" ]; then
        if [ -w "$(dirname "$DST_LIBDIR")" ]; then
            mkdir -p "$DST_LIBDIR"
        elif sudo -n true 2>/dev/null; then
            sudo mkdir -p "$DST_LIBDIR"
        else
            MANUAL="$MANUAL  sudo mkdir -p $DST_LIBDIR
"
        fi
    fi
    if copy_to "$SRC_LIB" "$DST_LIB"; then
        ok "归档已更新"
    fi
fi

if [ "$BIN_STALE" = 1 ]; then
    step "安装编译器 → $DST_BIN"
    if copy_to "$SRC_BIN" "$DST_BIN"; then
        ok "编译器已更新（$("$DST_BIN" --version 2>/dev/null | head -1)）"
    fi
fi

if [ -n "$MANUAL" ]; then
    echo ""
    warn "有文件需要提权才能写，请在终端里手动跑："
    echo ""
    printf '%s' "$MANUAL"
    echo ""
    exit 1
fi

# ── 校验：真编一个程序，确认链得上 ────────────────────────────
echo ""
step "校验：编译并运行一个最小程序"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
cat > "$TMP/smoke.qi" <<'EOF'
包 主程序;
函数 入口() {
    打印("同步校验通过");
}
EOF
if ! "$DST_BIN" run "$TMP/smoke.qi" 2>"$TMP/err" | grep -q "同步校验通过"; then
    die "校验失败：$(head -3 "$TMP/err")"
fi
ok "全部就绪"
