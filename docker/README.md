# qi 应用的容器化

qi 编译出的是**原生二进制**，运行时（SQLite、TLS、HTTP、gRPC、Redis…）都静态
链进去了。实测 `ldd` 只有三行：

```
libm.so.6   libgcc_s.so.1   libc.so.6
```

所以运行层不需要 qi、不需要 clang、不需要 openssl —— 一个 debian-slim 就够，
distroless 更小。

## 一条命令

```bash
docker build -f qi/docker/Dockerfile \
  --build-arg APP=qi/docker/示例/健康页.qi \
  --build-arg PACKAGES=qi_packages \
  -t 我的应用 .

docker run --rm -p 47901:47901 我的应用
curl http://127.0.0.1:47901/健康
```

| 构建参数 | 说明 |
|---|---|
| `APP` | `.qi` 入口文件，相对构建上下文（必填） |
| `PACKAGES` | `qi_packages` 位置；不用第三方包就留空 |
| `QI_VERSION` | 用哪个 qi 发布版，默认见 Dockerfile |
| `RUNTIME_BASE` | 运行层基底，默认 `debian:12-slim` |

## 两个运行层，同一份二进制

```bash
docker build ... -t 应用:debian                       # 默认，127MB，有 shell
docker build ... --target runtime-distroless -t 应用:小  # 66MB，没 shell
```

线上跑 distroless（面最小），联调用 debian（能 `docker exec` 进去看）。
distroless **连 shell 都没有** —— 它那一层里一行 `RUN` 都不能写，
写了报的是 `/bin/sh: not found`，不是「这个基底不支持」。

## 为什么不在镜像里编译 qi 编译器

仓库里原来那几份 Dockerfile 都是「容器里装 Rust + LLVM 21，把编译器从源码编出来」。
那要十几分钟、几个 GB 中间层，每个应用镜像都得重来一遍。编译器本身已经有
发布包（三平台，CI 出的），直接下来用 —— 构建层从十几分钟变成十几秒，
而且**装的是发过版的那一份**，跟本地一致。

## 三个真会踩的坑

**1. 应用必须听 `0.0.0.0`，不能听 `127.0.0.1`。**
听回环的话端口映射过去也连不上，表现是「docker run 起来了但访问不了」，
日志还一切正常。示例里用 `HOST` 环境变量，默认就是 `0.0.0.0`。

**2. 构建上下文里别留多余的 `qi.toml`。**
编译器会从源文件往上逐级扫每个子目录找包，祖先目录下任何一份同名包都会
**静默盖掉**你 `QI_PACKAGES_PATH` 指定的那个。把整个仓库当上下文拷进去时
尤其容易踩（见 CLAUDE.md「依赖解析」那节）。

**3. `.dockerignore` 里排掉 `target/`。**
qilang 仓库的 `target/` 有好几个 GB，不排掉光传构建上下文就要等很久。

## 目前只有 linux-x64

发布包没有 `linux-arm64`，所以在 ARM 机器（Apple Silicon、Graviton、Ampere）
上要么加 `--platform linux/amd64` 走模拟，要么等 arm64 发布包。
Dockerfile 在 arm64 上会**明确报错**而不是拉一个跑不了的二进制 ——
否则报出来的是一句莫名其妙的 `exec format error`。

## compose

`docker-compose.yml` 里有一份可直接用的：应用 + 健康检查 + 重启策略。

```bash
docker compose -f qi/docker/docker-compose.yml up --build
```
