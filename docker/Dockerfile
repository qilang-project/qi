# qi 应用的容器化模板 —— 多阶段，运行层只有二进制。
#
# ── 为什么不在镜像里从源码编译 qi ────────────────────────────────
#
# 仓库里原来那几份 Dockerfile 都是「容器里装 Rust + LLVM 21，把编译器从源码
# 编出来」。那要十几分钟、几个 GB 的中间层，而且每个应用镜像都得重来一遍。
# 编译器本身已经有发布包了（三平台，CI 出的），直接下来用就行 ——
# 构建层从十几分钟变成十几秒，而且**装的是发过版的那一份**，跟本地一致。
#
# ── 运行层为什么可以这么小 ──────────────────────────────────────
#
# qi 编译出的是原生二进制，运行时（含 SQLite、TLS、HTTP、gRPC）都**静态**
# 链进去了。实测 ldd 只有三行：
#     libm.so.6  libgcc_s.so.1  libc.so.6
# 所以运行层不需要 qi、不需要 clang、不需要 openssl —— 一个 debian-slim
# 甚至 distroless 就够。**别把构建层当运行层用**：那样镜像里带着编译器、
# 源码和整条工具链，白白大几百 MB，还多一堆攻击面。
#
# ── 用法 ────────────────────────────────────────────────────────
#
#   docker build -f qi/docker/Dockerfile \
#     --build-arg APP=示例/问候服务端.qi \
#     --build-arg PACKAGES=qi_packages \
#     -t 我的应用 .
#
#   docker run --rm -p 47811:47811 我的应用
#
# 参数：
#   QI_VERSION     用哪个 qi 发布版（默认见下）
#   APP            .qi 入口文件，相对于构建上下文
#   PACKAGES       qi_packages 的位置（没有第三方包就留空）
#   RUNTIME_BASE   运行层的基础镜像，默认 debian:12-slim
#
# 参数名一律 ASCII —— Dockerfile 的阶段名和 ARG 不接受中文（跟 shell 变量、
# Go 的包路径是同一条规则）。注释照常写中文。

# FROM 里要用的 ARG 必须声明在**所有阶段之前**（全局 ARG），
# 写在阶段内部的话 FROM 那行读不到，报「base name should not be blank」。
ARG RUNTIME_BASE=debian:12-slim

# ── 构建层 ──────────────────────────────────────────────────────
FROM debian:12-slim AS builder

ARG QI_VERSION=2026.08.12-1
ARG TARGETARCH

# clang 是**必须**的：qi 生成 LLVM IR，最后一步链接交给 clang。
# 只装 clang 不装完整 LLVM —— 编译器自己带着 LLVM，这里只要链接器。
RUN apt-get update \
    && apt-get install -y --no-install-recommends curl ca-certificates clang \
    && rm -rf /var/lib/apt/lists/*

# 按构建平台挑发布包。**arm64 的包从 2026.08.12 那一版才开始有** ——
# 更早的版本在 ARM 上只能加 --platform linux/amd64 走模拟。
# 拉不到就明说是哪个版本没有这个架构，别让人对着
# "exec format error" 猜半天。
RUN set -eu; \
    case "${TARGETARCH:-amd64}" in \
      amd64) tarball="qi-${QI_VERSION}-linux-x64.tar.gz" ;; \
      arm64) tarball="qi-${QI_VERSION}-linux-arm64.tar.gz" ;; \
      *) echo "不支持的架构: ${TARGETARCH}" >&2; exit 1 ;; \
    esac; \
    if ! curl -fsIL "https://github.com/qilang-project/qi/releases/download/${QI_VERSION}/${tarball}" > /dev/null 2>&1; then \
      echo "qi ${QI_VERSION} 没有 ${TARGETARCH} 的包（${tarball}）" >&2; \
      echo "  换个版本，或者加 --platform linux/amd64 走模拟" >&2; \
      exit 1; \
    fi; \
    curl -fsSL -o /tmp/qi.tgz \
      "https://github.com/qilang-project/qi/releases/download/${QI_VERSION}/${tarball}"; \
    mkdir -p /opt/qi; \
    tar xzf /tmp/qi.tgz -C /opt/qi; \
    rm /tmp/qi.tgz; \
    ln -s /opt/qi/bin/qi /usr/local/bin/qi; \
    qi --version

WORKDIR /src
COPY . .

ARG APP
ARG PACKAGES=

# QI_PACKAGES_PATH 指过去，第三方包（Web / gRPC / Harness…）才找得到。
#
# ⚠ 构建上下文里**别留多余的 qi.toml**：编译器会从源文件往上逐级扫每个
# 子目录找包，祖先目录下任何一份同名包都会静默盖掉你指定的那个。
# 拷贝整个仓库进来时尤其容易踩（见 CLAUDE.md「依赖解析」那节）。
RUN set -eu; \
    if [ -z "${APP:-}" ]; then echo "要传 --build-arg APP=xxx.qi" >&2; exit 1; fi; \
    if [ -n "${PACKAGES}" ]; then export QI_PACKAGES_PATH="/src/${PACKAGES}"; fi; \
    mkdir -p /out; \
    qi compile "${APP}" -o /out/app; \
    ldd /out/app || true

# ── 运行层（distroless，最小）─────────────────────────────────
#
#   docker build --target runtime-distroless ...
#
# distroless **连 shell 都没有**，所以这一层里一行 RUN 都不能有 ——
# 有的话报的是 `/bin/sh: not found`，而不是「这个基底不支持」。
# 它自带 CA 证书和 nonroot 用户，本来也不需要装什么。
#
# 代价：进不去（没有 shell），排查只能靠日志和 `docker cp`。
# 线上跑最小面，联调用下面那个 debian 层，两个镜像同一份二进制。
FROM gcr.io/distroless/cc-debian12:nonroot AS runtime-distroless
COPY --from=builder /out/app /usr/local/bin/app
ENTRYPOINT ["/usr/local/bin/app"]

# ── 运行层（debian-slim，默认）────────────────────────────────
#
# 放在最后 = 不带 --target 时构建的就是它。有 shell、有包管理器，
# 方便进去看看。
FROM ${RUNTIME_BASE} AS runtime

# 只装证书：连 HTTPS/TLS 的应用需要根证书，别的什么都不要。
# 不装的话症状是「所有 https 请求都报证书校验失败」，而本机跑得好好的。
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# 不用 root 跑。容器里用 root 是默认值，不是好默认值。
RUN useradd --system --create-home --uid 10001 qi
USER qi
WORKDIR /home/qi

COPY --from=builder /out/app /usr/local/bin/app

# 应用自己的资源（.proto、模板、静态文件…）由使用方在这层之后再 COPY，
# 或者把它们放进构建上下文并在这儿加一行。
ENTRYPOINT ["/usr/local/bin/app"]
