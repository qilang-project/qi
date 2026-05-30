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
#   3. 解到 INSTALL_DIR（默认 /usr/local/bin，没权限自动降级到 $HOME/.local/bin）
#   4. 跑 qi --version 验证
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

err()  { echo "${RED}错误${RESET}: $*" >&2; exit 1; }
warn() { echo "${YELLOW}警告${RESET}: $*"; }
ok()   { echo "${GREEN}✓${RESET} $*"; }
info() { echo "${BOLD}→${RESET} $*"; }

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

main() {
    local platform install_dir version archive url tmp
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
    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"' EXIT
    if ! curl -fL --progress-bar -o "$tmp/$archive" "$url"; then
        err "下载失败。手动看一眼：$url"
    fi

    info "解压到 $install_dir"
    tar xzf "$tmp/$archive" -C "$tmp"
    if [ ! -f "$tmp/qi" ]; then
        err "解压后找不到 qi 二进制（archive 结构可能变了？）"
    fi
    if [ -w "$install_dir" ]; then
        install -m 0755 "$tmp/qi" "$install_dir/qi"
    else
        sudo install -m 0755 "$tmp/qi" "$install_dir/qi"
    fi
    ok "已安装 → $install_dir/qi"

    info "验证..."
    if "$install_dir/qi" --version >/dev/null 2>&1; then
        ok "qi $version 安装成功"
        echo
        echo "  跑个 hello world："
        echo "    echo '包 主程序; 导入 标准库.输入输出 作为 IO; 函数 入口() { IO.打印行(\"你好 qi\"); }' > hi.qi"
        echo "    qi run hi.qi"
    else
        warn "qi 二进制装好了但 --version 跑不通。检查 $install_dir 是否在 PATH 里。"
    fi
}

main "$@"
