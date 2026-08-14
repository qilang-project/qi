# qi 编译器 —— 构建 / 测试 / 发布入口
#
# 本地和 CI 走同一套目标，避免两边各写一份逐渐漂移。
# （之前 CI 的示例冒烟一直挂在「找不到 qi-runtime 归档」：workflow 把 runtime
#  克隆到 checkout 的兄弟目录，而编译器是在 checkout **内**找，差一级。
#  统一到这里之后，路径只有一处定义。）
#
# 常用：
#   make build            构建编译器（release）
#   make runtime          构建 qi-runtime 归档（独立 cargo 项目，容易漏）
#   make test             库单测
#   make regress          codegen 回归
#   make ffi-link         FFI 链接控制（--库路径 / 直链 .a / macOS framework）
#   make examples         示例冒烟
#   make debuginfo        DWARF 调试信息验收（实跑 lldb 断点/单步/backtrace）
#   make grpc             gRPC 全套互通验收（要 qi-grpc 仓）
#   make ci               CI 跑的全部（= check test regress ffi-link examples debuginfo grpc）
#   make install          装到 /usr/local（同步编译器 + 运行时归档）
#   make release V=2026.07.29-2   全量门禁 → 改版本号 → 提交 → 打 tag
#   make push-release V=…         推 tag（触发 Release workflow）

SHELL := /bin/bash
.DEFAULT_GOAL := help

# qi-runtime 是**独立 cargo 项目**（有自己的 target/），仓库根的 cargo build
# 编不到它。本地它是 qilang/qi-runtime，CI 里克隆到 checkout 的兄弟目录 ——
# 相对 qi/ 都是 ../qi-runtime，所以一个变量两边通用。
RUNTIME_DIR ?= ../qi-runtime
RUNTIME_LIB := $(abspath $(RUNTIME_DIR)/target/release/libqi_runtime.a)

# target 目录问 cargo 要：本地 qi/ 是 qilang workspace 的成员，产物在
# 上一级的共享 target/；CI 里 qi 是独立 checkout，产物在 ./target/。
# 写死任何一个都会在另一边挂掉。
TARGET_DIR := $(shell cargo metadata --no-deps --format-version 1 2>/dev/null \
	| sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')
QI := $(TARGET_DIR)/release/qi
PREFIX ?= /usr/local

.PHONY: help build runtime test regress ffi-link examples debuginfo grpc check lint-strict ci install \
        release push-release clean version

help:
	@awk '/^# 常用：/{f=1;next} /^$$/{f=0} f{sub(/^#[ ]?/,"");print}' $(MAKEFILE_LIST)

build:
	cargo build --release

# 单独一条目标，因为它最容易被忘：改了 runtime 不重建，链进去的还是旧归档，
# 不报错、只是行为对不上。
runtime:
	cargo build --release --manifest-path $(RUNTIME_DIR)/Cargo.toml

$(RUNTIME_LIB):
	$(MAKE) runtime

# --test-threads=1 不是图省事:标准库 FFI 有几个测试会改进程环境变量
# (os_ffi/env_ffi/system 的 set_var)，而**任何**并发读 env 的测试都可能撞上 ——
# macOS 上 setenv/getenv 并发就是直接 SIGSEGV，Rust 把 set_var 标成 unsafe
# 正是这个原因。只给写方上锁没用:读的地方遍布各处，找不全。
# 408 个测试串行 0.3 秒，代价可以忽略，换掉的是一整类偶发段错误
# (本地 make ci 撞到过一次，重跑三次又都过 —— 这种 flake 最费时间)。
test:
	cargo test --release --lib -- --test-threads=1

regress: build $(RUNTIME_LIB)
	QI_RUNTIME_LIB=$(RUNTIME_LIB) bash tests/codegen回归/断言.sh $(QI)

# FFI 链接控制（--库路径 / 直链文件 / macOS framework）。**单列一条**是因为它
# 跟别的测试的失败方式不同：它要现场 cc + ar 造一个静态库，再让 qi 去链 ——
# 依赖的是机器上的 cc/ar 和链接器行为，不是编译器内部逻辑。混进 regress 里，
# 一旦某台机器没有 cc，看到的会是「codegen 回归挂了」这种指错方向的报错。
ffi-link: build $(RUNTIME_LIB)
	QI_RUNTIME_LIB=$(RUNTIME_LIB) bash tests/ffi链接/断言.sh $(QI)

# 显式传 QI_RUNTIME_LIB：不依赖「归档正好在编译器找得到的相对位置」。
examples: build $(RUNTIME_LIB)
	QI_BIN=$(QI) QI_RUNTIME_LIB=$(RUNTIME_LIB) bash scripts/run_examples.sh

# gRPC 全套互通验收（qi-grpc 仓）。**gRPC 的故障方式是「挂着不动」和
# 「静默丢消息」，不是干脆的崩溃** —— 别的测试全绿也照样发现不了，
# 所以必须有一条专门盯它的。qi-grpc 不在时跳过（它是独立仓）。
# DWARF 调试信息验收。**必须实跑 lldb** —— 元数据「生成了」和调试器「用得上」
# 是两回事：模块标志漏一个、finalize 漏一次、macOS 的 debug map 找不到 .o，
# dwarfdump 全都看不出问题，只有断点不命中才暴露。
# 无 lldb / lldb 控制不了进程（沙箱、无开发者模式）时脚本自己报 SKIP，不假红。
debuginfo: build $(RUNTIME_LIB)
	QI_RUNTIME_LIB=$(RUNTIME_LIB) bash tests/调试信息/断言.sh $(QI)

GRPC_DIR ?= $(CURDIR)/../qi-grpc
grpc: build $(RUNTIME_LIB)
	@if [ -x "$(GRPC_DIR)/跑验收.sh" ]; then \
	  QI_BIN=$(QI) QI_RUNTIME_LIB=$(RUNTIME_LIB) bash "$(GRPC_DIR)/跑验收.sh"; \
	else \
	  echo "没有 $(GRPC_DIR)/跑验收.sh，跳过 gRPC 验收"; \
	fi

# 门禁用的 lint。clippy 只挡真 error ——
# 仓库里还有一批历史 warning，现在就上 -D warnings 会让 CI 当场全红；
# 想清理时跑 make lint-strict 看完整清单。
#
# **格式检查故意不在这儿**，见下面 fmt-check。
check:
	cargo clippy --release

# 格式检查。**必须用钉死的 rustfmt 版本跑**（CI 里是
# .github/workflows/ci.yml 的 Format Check job，钉在 1.92.0）。
#
# 为什么单独拎出来：rustfmt 换版本会改主意，尤其是**中文标识符在 import
# 里的排序** —— 新版 stable 要 `{元素类型, Qi类型}`，1.92.0 要
# `{Qi类型, 元素类型}`。谁也不算错，但两边一起跑就必有一边红。
#
# 这个坑踩过一次：Format Check job 早就钉到 1.92.0 了，可 make check 里
# 还留着一份 fmt --check，而 CI 的 Build + Test 用的是浮动 @stable ——
# 于是那个钉子被绕开，stable 一升级，2026.07.29-2 那次发版的 CI 当场变红
# (Format Check 绿、Build + Test 红，看着莫名其妙)。
fmt-check:
	cargo fmt --all -- --check

lint-strict:
	cargo clippy --release -- -D warnings

# CI 的 Build + Test job 跑的就是这条(浮动 @stable 工具链)。
# **不含 fmt-check** —— 格式归钉死 1.92.0 的那个 job，理由见 fmt-check。
# 本地提交前想全查一遍：make fmt-check ci
ci: check test regress ffi-link examples debuginfo grpc

install: build $(RUNTIME_LIB)
	QI_PREFIX=$(PREFIX) bash scripts/同步本地构建.sh --跳过构建

version:
	@grep -m1 '^version' Cargo.toml

# 发布：先过完整门禁，再改版本号 → 提交 → 打 tag。tag 不自动推，
# 确认无误后跑 make push-release V=…
#
# 版本号两种写法：tag 用零填充日期 2026.07.29-2，Cargo.toml 去前导零 2026.7.29-2。
# 运行时归档的 tag 钉在 .github/workflows/release.yml 里。钉住是对的
# （发布要可复现），但**忘了跟着 bump 是静默的**：包照样出，用户装上之后
# 一编译就是 `undefined reference to qi_xxx` —— 2026.08.11-1 就是这么出的，
# 运行时还停在 07-29，后来加的 WebSocket/mailbox/JSON 那批符号一个都不在。
# 所以发布前对一次：钉的 tag 必须是 qi-runtime 的最新 tag。
check-runtime-pin:
	@pinned=$$(sed -n 's#.*--branch \([0-9.-]*\) https://github.com/qilang-project/qi-runtime.*#\1#p' \
	    .github/workflows/release.yml); \
	  latest=$$(git ls-remote --tags --refs https://github.com/qilang-project/qi-runtime.git \
	    | awk -F/ '{print $$NF}' | sort -V | tail -1); \
	  if [ "$$pinned" != "$$latest" ]; then \
	    echo "运行时 tag 落后：release.yml 钉的是 $${pinned}，qi-runtime 最新是 $${latest}"; \
	    echo "  要么给 qi-runtime 的新提交打 tag，要么把 release.yml 里的 --branch 改成 $${latest}"; \
	    exit 1; \
	  fi; \
	  echo "运行时 tag 对得上：$$pinned"

release:
ifndef V
	$(error 用法: make release V=2026.07.29-2)
endif
	@echo "==> 运行时 tag"
	$(MAKE) check-runtime-pin
	@echo "==> 门禁"
	$(MAKE) ci
	@echo "==> 改版本号 $(V)"
	@cargo_v=$$(echo "$(V)" | sed 's/\.0*\([0-9]\)/.\1/g'); \
	  sed -i.bak "s/^version = \".*\"/version = \"$$cargo_v\"/" Cargo.toml && rm -f Cargo.toml.bak; \
	  echo "    Cargo.toml → $$cargo_v"
	@echo "==> 重建（版本号进二进制）"
	cargo build --release
	@$(QI) --version
	@echo "==> 提交 + 打 tag"
	git add Cargo.toml
	git commit -m "release: $(V)"
	git tag -a $(V) -m "$(V)"
	@echo ""
	@echo "已打 tag $(V)。确认后推："
	@echo "  make push-release V=$(V)"

push-release:
ifndef V
	$(error 用法: make push-release V=2026.07.29-2)
endif
	git push origin main
	git push origin $(V)
	@echo "已推 $(V)，Release workflow 开始构建三平台产物。"

clean:
	cargo clean
