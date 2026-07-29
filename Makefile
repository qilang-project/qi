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
#   make examples         示例冒烟
#   make ci               CI 跑的全部（= check test regress examples）
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

.PHONY: help build runtime test regress examples check lint-strict ci install \
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

test:
	cargo test --release --lib

regress: build $(RUNTIME_LIB)
	QI_RUNTIME_LIB=$(RUNTIME_LIB) bash tests/codegen回归/断言.sh $(QI)

# 显式传 QI_RUNTIME_LIB：不依赖「归档正好在编译器找得到的相对位置」。
examples: build $(RUNTIME_LIB)
	QI_BIN=$(QI) QI_RUNTIME_LIB=$(RUNTIME_LIB) bash scripts/run_examples.sh

# 门禁用的 lint：格式必须齐，clippy 只挡真 error。
# 仓库里还有一批历史 warning，现在就上 -D warnings 会让 CI 当场全红；
# 想清理时跑 make lint-strict 看完整清单。
check:
	cargo fmt --all -- --check
	cargo clippy --release

lint-strict:
	cargo clippy --release -- -D warnings

ci: check test regress examples

install: build $(RUNTIME_LIB)
	QI_PREFIX=$(PREFIX) bash scripts/同步本地构建.sh --跳过构建

version:
	@grep -m1 '^version' Cargo.toml

# 发布：先过完整门禁，再改版本号 → 提交 → 打 tag。tag 不自动推，
# 确认无误后跑 make push-release V=…
#
# 版本号两种写法：tag 用零填充日期 2026.07.29-2，Cargo.toml 去前导零 2026.7.29-2。
release:
ifndef V
	$(error 用法: make release V=2026.07.29-2)
endif
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
