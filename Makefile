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
#   make bindgen          `qi 绑定` 端到端（zlib/libm/手写头文件 → 真链接真调用）
#   make examples         示例冒烟
#   make debuginfo        DWARF 调试信息验收（实跑 lldb 断点/单步/backtrace）
#   make fuzz             差分模糊（参考求值器 / 无优化 / 最大优化 三方比对）
#   make grpc             gRPC 全套互通验收（要 qi-grpc 仓）
#   make gui-smoke        GUI 冒烟（要显示环境 + 带 gui feature 的运行时，不进 ci）
#   make ci               CI 跑的全部（= check test regress ffi-link bindgen examples debuginfo fuzz grpc）
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

# ── LLVM 版本：必须是 21 ────────────────────────────────────────
#
# llvm-sys 的 crate 名（llvm-sys 211）钉的是 LLVM 21.1。装了别的大版本时
# **它不会拒绝构建**，只会在生成 IR 时发出这一版的 intrinsic 签名，然后：
#
#   LLVM 模块校验失败: Intrinsic has incorrect return type! ptr @llvm.coro.end
#
# 于是 `make ci` 挂掉 17 条协程/调试信息断言，看起来像真 bug，实际上只是
# 本机装了 LLVM 22。这个错误信息里没有一个字提到版本。
#
# 更坑的是 homebrew：`llvm@21` 和 `llvm@22` 是**同一个 llvm formula 的别名**，
# 升级到 22 之后两个别名都被指向 22.1.8 —— `/opt/homebrew/opt/llvm@21`
# 这个名字直接在撒谎。所以下面按 Cellar 里的**真实版本目录**找，不认别名。
#
# mac 上还有一层：21.1.6 的 llvm-config 链的是 libz3.4.15.dylib，而 brew 的
# z3 早升到 4.16，于是 llvm-config 一跑就 dyld 报 Library not loaded。
# 补个软链就好（brew 升级 z3 之后可能要再补一次）：
#   ln -sfn /opt/homebrew/Cellar/z3/4.16.0/lib/libz3.dylib \
#           /opt/homebrew/opt/z3/lib/libz3.4.15.dylib
#
# 显式设了 LLVM_SYS_211_PREFIX 就用它（CI 就是这么干的），不去猜。
ifeq ($(origin LLVM_SYS_211_PREFIX), undefined)
  LLVM21 := $(firstword $(wildcard /opt/homebrew/Cellar/llvm/21.*) \
                        $(wildcard /opt/homebrew/Cellar/llvm@21/21.*) \
                        $(wildcard /usr/lib/llvm-21) \
                        $(wildcard /usr/local/opt/llvm@21))
  ifneq ($(LLVM21),)
    export LLVM_SYS_211_PREFIX := $(LLVM21)
  endif
endif

# 版本对不上就**当场停下**，别让它跑到 17 条看不懂的断言失败那儿去。
check-llvm:
	@if [ -n "$$LLVM_SYS_211_PREFIX" ]; then cfg="$$LLVM_SYS_211_PREFIX/bin/llvm-config"; \
	  else cfg=llvm-config; fi; \
	  v=$$("$$cfg" --version 2>/dev/null); \
	  case "$$v" in \
	    21.*) echo "LLVM $${v}  ($${LLVM_SYS_211_PREFIX:-PATH})" ;; \
	    "") echo "找不到 llvm-config。装 LLVM 21，或设 LLVM_SYS_211_PREFIX"; exit 1 ;; \
	    *) echo "LLVM 版本不对：找到 $${v}，要 21.x"; \
	       echo "  设 LLVM_SYS_211_PREFIX 指向 21，比如："; \
	       echo "    export LLVM_SYS_211_PREFIX=/opt/homebrew/Cellar/llvm/21.1.6"; \
	       echo "  （注意 /opt/homebrew/opt/llvm@21 可能是指向 22 的别名，不可信）"; \
	       exit 1 ;; \
	  esac

.PHONY: help build runtime test regress ffi-link bindgen examples debuginfo fuzz grpc gui-smoke check lint-strict ci install \
        release push-release clean version check-llvm

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

# `qi 绑定` 的端到端验收（C 头文件 → 外部块 → 真链接真调用）。也**单列一条**：
# 它依赖机器上的 clang（要 -ast-dump=json）和系统头文件（zlib.h/math.h），
# 跟编译器内部逻辑无关，混进 regress 里失败信息会指错方向。
bindgen: build $(RUNTIME_LIB)
	QI_RUNTIME_LIB=$(RUNTIME_LIB) bash tests/绑定生成/断言.sh $(QI)

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

# 差分模糊：随机生成良类型程序，比 参考求值器 / 无优化 / 最大优化 三份答案。
# 单列一条并且 cargo test 里 #[ignore]，是因为每个程序要编译链接**两次**，
# 12 个程序 ~20 秒，比整套单测（1.8 秒）还慢一个量级 —— 混进 `make test`
# 会让所有人以后都觉得单测慢。
#
# 数量默认 12、种子固定：CI 每次跑同一批程序，红了原样复现。想扩覆盖面就
# 改种子，而不是让它每次随机 —— 随机的红一次就再也复现不出来。
#   QI_FUZZ_COUNT=300 QI_FUZZ_SEED=777000 make fuzz
FUZZ_COUNT ?= 12
FUZZ_SEED ?= 20260822
fuzz: build $(RUNTIME_LIB)
	QI_RUNTIME_LIB=$(RUNTIME_LIB) QI_FUZZ_COUNT=$(FUZZ_COUNT) QI_FUZZ_SEED=$(FUZZ_SEED) \
	  cargo test --release --test 差分模糊 -- --ignored --nocapture

# WebAssembly 目标回归：同一个 .qi 原生跑一遍、wasm（wasmtime）跑一遍，stdout 逐字节比。
# 期望就是原生的输出，不维护期望文件。缺 wasmtime / wasm32-wasip1 目标 / wasm 运行时归档
# 时脚本自己 SKIP —— 这三样不是每台机器都有，别把主干门禁拖红。CI 的 Linux job 装齐了它们。
# wasm 运行时归档：cd ../qi-runtime/wasm && cargo build --release --target wasm32-wasip1
wasm: build $(RUNTIME_LIB)
	QI_RUNTIME_LIB=$(RUNTIME_LIB) bash tests/wasm/断言.sh $(QI)

GRPC_DIR ?= $(CURDIR)/../qi-grpc
grpc: build $(RUNTIME_LIB)
	@if [ -x "$(GRPC_DIR)/跑验收.sh" ]; then \
	  QI_BIN=$(QI) QI_RUNTIME_LIB=$(RUNTIME_LIB) bash "$(GRPC_DIR)/跑验收.sh"; \
	else \
	  echo "没有 $(GRPC_DIR)/跑验收.sh，跳过 gRPC 验收"; \
	fi

# GUI 冒烟。**故意不进 ci** —— 它要真开窗，依赖显示环境；CI 的 runner 是无头的，
# 挂进去只会得到一条永远 SKIP（或者更糟：偶发假红）的噪音用例。
# 想跑就单独 make gui-smoke，前提是 RUNTIME_LIB 那份归档是带 --features gui 编的
# （不带的话 GUI 全是 stub，窗口根本不会出现，脚本会 SKIP 掉）。
gui-smoke: build $(RUNTIME_LIB)
	QI_RUNTIME_LIB=$(RUNTIME_LIB) bash tests/gui自动化/断言.sh $(QI)

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
# 格式检查必须用**跟 CI 一样的 rustfmt 版本**。
#
# CI 的 fmt job 钉在 dtolnay/rust-toolchain@1.92.0（ci.yml 的注释写着「钉死版本，
# 且要与开发机一致」）。可开发机的 stable 会自己往前跑 —— 2026-09-03 本机已经是
# 1.97.1，它的 rustfmt 跟 1.92.0 的排版不一样。后果是**本地 cargo fmt 一跑就把
# CI 弄红**，而且本地 `cargo fmt --all -- --check` 还是绿的，完全看不出来。
# 那次是发版之后才发现：Release 绿、CI 红，8 个我根本没碰过的文件被重排了。
FMT_TOOLCHAIN ?= 1.92.0

fmt-check:
	@rustup toolchain list | grep -q '^$(FMT_TOOLCHAIN)' || { \
	    echo "缺 rustfmt $(FMT_TOOLCHAIN)（CI 钉的就是它）："; \
	    echo "  rustup toolchain install $(FMT_TOOLCHAIN) --component rustfmt --profile minimal"; \
	    exit 1; \
	}
	cargo +$(FMT_TOOLCHAIN) fmt --all -- --check

# 按 CI 那版 rustfmt 格式化。别直接跑 `cargo fmt`，见上面。
fmt:
	cargo +$(FMT_TOOLCHAIN) fmt --all

lint-strict:
	cargo clippy --release -- -D warnings

# CI 的 Build + Test job 跑的就是这条(浮动 @stable 工具链)。
#
# **不含 fmt-check，别加** —— 格式归 CI 里钉死 1.92.0 的那个独立 job。
# 2026-09-03 试着加进来过，当场把 ubuntu/macOS 两个 Build+Test 干红：
# 那两个 job 用浮动 @stable，装不到 1.92.0，fmt-check 的守卫直接 exit 1。
# 发布路径由 `make release` 单独带一道 fmt-check（那是本机跑，有 1.92.0）。
# 本地提交前想全查一遍：make fmt-check ci
ci: check-llvm check test regress ffi-link bindgen examples debuginfo fuzz grpc wasm

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
	@echo "==> 格式（用 CI 钉的 rustfmt —— 本机 stable 版本不同，直接 cargo fmt 查不出来）"
	$(MAKE) fmt-check
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
