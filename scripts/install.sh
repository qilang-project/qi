#!/usr/bin/env bash
# qi 一键安装脚本（macOS + Linux）
#
# 用法：
#   curl -fsSL https://raw.githubusercontent.com/qilang-project/qi/main/scripts/install.sh | bash
#   # 或装到自定义目录：
#   curl -fsSL .../install.sh | INSTALL_DIR=$HOME/bin bash
#
# 工作流程：
#   1. 探测 OS + 架构（macos-arm64 / macos-x64 / linux-x64）
#   2. 从 GitHub Releases 拉对应 tarball
#   3. 二进制装到 INSTALL_DIR（默认 /usr/local/bin，没权限降级到 $HOME/.local/bin），
#      运行时装到同 prefix 的 lib/qi —— 编译器按 <prefix>/bin/qi → <prefix>/lib/qi 找它
#   4. 真编译一个 hello world 验证（只跑 --version 验不出运行时缺失）
set -euo pipefail

REPO="${QI_REPO:-qilang-project/qi}"
VERSION="${QI_VERSION:-latest}"
INSTALL_DIR="${INSTALL_DIR:-}"

# 颜色输出（无 tty 时退化为纯文本）
if [ -t 1 ]; then
    BOLD=$'\e[1m'; RED=$'\e[31m'; GREEN=$'\e[32m'; YELLOW=$'\e[33m'; RESET=$'\e[0m'
else
    BOLD=""; RED=""; GREEN=""; YELLOW=""; RESET=""
fi

# 提示一律走 stderr。resolve_version() 这类函数的 stdout 是**返回值**，
# 被 `version=$(resolve_version)` 捕获 —— 提示混进去，版本号就成了
# 「→ 查询最新 release...\n2026.08.16-2」，拼出来的下载 URL 里带换行和中文，
# curl 直接报 bad range。安装脚本坏在这里最难发现：它只在联网真跑时才炸。
err()  { echo "${RED}错误${RESET}: $*" >&2; exit 1; }
warn() { echo "${YELLOW}警告${RESET}: $*" >&2; }
ok()   { echo "${GREEN}✓${RESET} $*" >&2; }
info() { echo "${BOLD}→${RESET} $*" >&2; }

# 1. 探测平台
detect_platform() {
    local os arch
    case "$(uname -s)" in
        Darwin) os=macos ;;
        Linux)  os=linux ;;
        *) err "不支持的操作系统：$(uname -s)" ;;
    esac
    case "$(uname -m)" in
        arm64|aarch64) arch=arm64 ;;
        x86_64|amd64)  arch=x64 ;;
        *) err "不支持的 CPU 架构：$(uname -m)" ;;
    esac
    if [ "$os" = linux ] && [ "$arch" = arm64 ]; then
        err "linux-arm64 暂未发布预编译包。请从源码构建：git clone … && cargo build --release"
    fi
    echo "${os}-${arch}"
}

# 2. 确定安装目录（优先 /usr/local/bin，没写权限降级 $HOME/.local/bin）
choose_install_dir() {
    if [ -n "$INSTALL_DIR" ]; then
        mkdir -p "$INSTALL_DIR"
        echo "$INSTALL_DIR"
        return
    fi
    if [ -w /usr/local/bin ] || sudo -n true 2>/dev/null; then
        echo /usr/local/bin
    else
        local fallback="$HOME/.local/bin"
        mkdir -p "$fallback"
        warn "无 /usr/local/bin 写权限，装到 $fallback"
        warn "记得把它加进 PATH：export PATH=\"\$HOME/.local/bin:\$PATH\""
        echo "$fallback"
    fi
}

# 目标路径要不要 sudo？目录可能还不存在，得往上走到第一个存在的祖先再看写权限
# （对着不存在的路径测 -w 永远是 false，会平白无故请求 sudo）。
need_sudo_for() {
    local p="$1"
    while [ ! -e "$p" ] && [ "$p" != / ] && [ -n "$p" ]; do
        p=$(dirname "$p")
    done
    [ -w "$p" ] && echo "" || echo "sudo"
}

# 3. 拉取最新 release tag（如果用户没指定）
resolve_version() {
    if [ "$VERSION" = latest ]; then
        info "查询最新 release..."
        VERSION=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
            | grep -o '"tag_name": *"[^"]*"' \
            | head -1 \
            | sed 's/.*"\([^"]*\)"/\1/')
        if [ -z "$VERSION" ]; then
            err "查不到 latest tag（检查仓库是否发布过 release：$REPO/releases）"
        fi
    fi
    echo "$VERSION"
}

# trap 在函数外触发，那时 main() 的 local 变量已出作用域 —— set -u 下会报
# 「未绑定的变量」。清理用的临时目录必须是全局的。
TMP_WORK=""

main() {
    local platform install_dir version archive url
    platform=$(detect_platform)
    version=$(resolve_version)
    install_dir=$(choose_install_dir)
    archive="qi-${version}-${platform/x64/x64}.tar.gz"
    case "$platform" in
        macos-arm64) archive="qi-${version}-macos-arm64.tar.gz" ;;
        macos-x64)   archive="qi-${version}-macos-x64.tar.gz" ;;
        linux-x64)   archive="qi-${version}-linux-x64.tar.gz" ;;
    esac
    url="https://github.com/$REPO/releases/download/$version/$archive"

    info "下载 $archive"
    info "URL: $url"
    TMP_WORK=$(mktemp -d); tmp="$TMP_WORK"
    trap 'rm -rf "$TMP_WORK"' EXIT
    if ! curl -fL --progress-bar -o "$tmp/$archive" "$url"; then
        err "下载失败。手动看一眼：$url"
    fi

    info "解压到 $install_dir"
    tar xzf "$tmp/$archive" -C "$tmp"

    # 发布包的布局是 bin/qi + lib/qi/（运行时归档等）。二进制单独拷过去没用：
    # 编译器按 <prefix>/bin/qi → <prefix>/lib/qi/libqi_runtime.a 找运行时，
    # 缺了它 qi --version 照样能过，一编译就报「找不到运行时归档」。
    local src_bin
    if [ -f "$tmp/bin/qi" ]; then
        src_bin="$tmp/bin/qi"
    elif [ -f "$tmp/qi" ]; then
        src_bin="$tmp/qi"      # 旧布局：整包只有一个二进制
    else
        err "解压后找不到 qi 二进制（archive 结构可能变了？）"
    fi

    # install_dir 是 <prefix>/bin，运行时要落到同一个 prefix 下的 lib/qi。
    local prefix lib_dst
    prefix=$(dirname "$install_dir")
    lib_dst="$prefix/lib/qi"

    # bin 和 lib 的写权限要**分别**判断。真实踩过：/usr/local/bin 属于当前用户
    # （Homebrew 建的），/usr/local/lib/qi 却是上一次 sudo 安装留下的 root 目录 ——
    # 按 bin 的权限推出 SUDO=""，二进制装进去了，运行时 cp 全线 Permission denied，
    # 于是留下一个「装好了但编译不了」的半吊子安装。
    local SUDO_BIN SUDO_LIB
    SUDO_BIN=$(need_sudo_for "$install_dir")
    SUDO_LIB=$(need_sudo_for "$lib_dst")

    $SUDO_BIN install -m 0755 "$src_bin" "$install_dir/qi" \
        || err "写不进 $install_dir。换个目录：curl … | INSTALL_DIR=\$HOME/.local/bin bash"
    ok "已安装 → $install_dir/qi"

    if [ -d "$tmp/lib/qi" ]; then
        if ! ($SUDO_LIB mkdir -p "$lib_dst" && $SUDO_LIB cp -R "$tmp/lib/qi/." "$lib_dst/"); then
            err "写不进 $lib_dst（多半是上次 sudo 安装留下的 root 目录）。二选一：
     sudo rm -rf $lib_dst   然后重跑本脚本
     或整个装到自己目录：curl … | INSTALL_DIR=\$HOME/.local/bin bash"
        fi
        ok "运行时 → $lib_dst"
    else
        warn "包里没有 lib/qi（旧版发布包？）编译时可能要自己设 QI_RUNTIME_LIB"
    fi

    # 验证要真编译一次。只跑 --version 是这个脚本以前的验证方式 ——
    # 它在运行时缺失时照样绿，坏包就是这么发出去的。
    info "验证（真编译一个程序）..."
    local probe="$tmp/hi.qi"
    printf '包 主程序;\n导入 标准库.输入输出 作为 IO;\n函数 入口() { IO.打印行("你好 qi"); }\n' > "$probe"
    if [ "$("$install_dir/qi" run "$probe" 2>/dev/null | tail -1)" = "你好 qi" ]; then
        ok "qi $version 安装成功，编译运行都正常"
        echo
        echo "  跑个 hello world：" >&2
        echo "    echo '包 主程序; 导入 标准库.输入输出 作为 IO; 函数 入口() { IO.打印行(\"你好 qi\"); }' > hi.qi" >&2
        echo "    qi run hi.qi" >&2
    elif "$install_dir/qi" --version >/dev/null 2>&1; then
        warn "qi 装好了，但编译测试程序失败。多半是运行时没到位："
        warn "  期望在 $lib_dst/libqi_runtime.a"
        warn "  或手动设 export QI_RUNTIME_LIB=<libqi_runtime.a 路径>"
    else
        warn "qi 二进制装好了但 --version 跑不通。检查 $install_dir 是否在 PATH 里。"
    fi
}

main "$@"
