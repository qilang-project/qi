#!/usr/bin/env bash
# 构建缓存断言（见 src/build_cache.rs）：
#   ① 什么都没变 → 命中，产物与第一次、与关掉缓存重编的都逐字节一致
#   ② 入口文件改了 → 不命中，跑出来是新行为（不能拿旧产物）
#   ③ 被导入的模块改了 → 不命中；改回原样 → 又命中（按内容而不是按时间）
#   ④ 优化档位变了 → 不命中
#   ⑤ 影响 codegen 的环境变量（QI_LINT / QI_ARC）变了 → 不命中
#   ⑥ QI_NO_CACHE=1 → 既不查也不写
#   ⑦ codegen 期间的警告（环检测）命中时原样重放
#   ⑧ 运行时归档换了（QI_RUNTIME_LIB 指向另一份）→ 不命中
#   ⑨ 并行 codegen（QI_CODEGEN_PARTITIONS）产出的多个目标文件一并缓存/还原，
#      回到串行时旧分片文件被清掉
#
# 用法：tests/构建缓存/断言.sh <qi 二进制>
# 夹具写在本目录下的临时子目录里（不放 /tmp：编译器会往祖先目录找 qi.toml）。
# 缓存目录用 QI_CACHE_DIR 隔离，不碰用户的 ~/Library/Caches/qi。
# 注意 macOS bash 3.2：shell 变量名一律 ASCII。
set -uo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
QI="${1:?用法: 断言.sh <qi 二进制>}"
WORK="$HERE/_work_$$"
rm -rf "$WORK"
mkdir -p "$WORK/src"
trap 'rm -rf "$WORK"' EXIT

export QI_CACHE_DIR="$WORK/cache"
export QI_CACHE_TRACE=1
unset QI_NO_CACHE QI_EMIT_LL QI_EMIT_LL_POST QI_LINT QI_ARC QI_CODEGEN_PARTITIONS 2>/dev/null || true

passed=0
failed=0
pass() { echo "PASS $1"; passed=$((passed+1)); }
fail() { echo "FAIL $1"; failed=$((failed+1)); }

ENTRY="$WORK/src/主.qi"
MOD="$WORK/src/工具.qi"
OUT="$WORK/out"

write_entry() {
    cat > "$ENTRY" <<QI
包 主程序;
导入 标准库.输入输出 作为 IO;
导入 工具::{问候};
函数 入口() {
    IO.打印行(问候("$1"));
}
QI
}
write_mod() {
    cat > "$MOD" <<QI
包 工具;
公开 函数 问候(名: 字符串) : 字符串 {
    返回 "$1，\${名}";
}
QI
}

# 编译一次，把 [qi 缓存] 那几行留在 $LOG 里；返回编译器退出码
LOG="$WORK/log"
build() {
    "$QI" "$@" compile "$ENTRY" -o "$OUT" > "$LOG" 2>&1
}
hit() { grep -q '^\[qi 缓存\] 命中' "$LOG"; }
miss() { grep -q '^\[qi 缓存\] 未命中' "$LOG"; }
stored() { grep -q '^\[qi 缓存\] 已写入' "$LOG"; }
output_is() {
    local got
    got=$("$OUT" 2>&1)
    [ "$got" = "$1" ]
}

write_entry "世界"
write_mod "你好"

# ① 首次：未命中 + 写入；再编：命中，产物一致；关缓存重编：一致
if build && miss && stored && output_is "你好，世界"; then pass "首次编译未命中并写入"; else fail "首次编译未命中并写入"; cat "$LOG"; fi
cp "$OUT" "$WORK/first.bin"
if build && hit && output_is "你好，世界" && cmp -s "$OUT" "$WORK/first.bin"; then
    pass "未改动再编命中，产物逐字节一致"
else
    fail "未改动再编命中，产物逐字节一致"; cat "$LOG"
fi
if QI_NO_CACHE=1 "$QI" compile "$ENTRY" -o "$WORK/fresh.bin" > "$LOG" 2>&1 && cmp -s "$OUT" "$WORK/fresh.bin"; then
    pass "命中产物与不走缓存重编的逐字节一致"
else
    fail "命中产物与不走缓存重编的逐字节一致"; cat "$LOG"
fi

# ⑥ QI_NO_CACHE：不查不写（前面那次也不该写入 —— 条目数不变）
entries_before=$(find "$QI_CACHE_DIR/build" -name exe | wc -l | tr -d ' ')
if QI_NO_CACHE=1 build && ! hit && ! miss && ! stored && grep -q 'QI_NO_CACHE' "$LOG" \
   && [ "$(find "$QI_CACHE_DIR/build" -name exe | wc -l | tr -d ' ')" = "$entries_before" ]; then
    pass "QI_NO_CACHE=1 既不查也不写"
else
    fail "QI_NO_CACHE=1 既不查也不写"; cat "$LOG"
fi

# ② 改入口
write_entry "缓存"
if build && miss && output_is "你好，缓存"; then pass "入口改动不命中"; else fail "入口改动不命中"; cat "$LOG"; fi

# ③ 改被导入的模块；改回原样又命中
write_mod "早上好"
if build && miss && output_is "早上好，缓存"; then pass "导入模块改动不命中"; else fail "导入模块改动不命中"; cat "$LOG"; fi
write_mod "你好"
if build && hit && output_is "你好，缓存"; then pass "模块改回原样重新命中"; else fail "模块改回原样重新命中"; cat "$LOG"; fi

# ④ 优化档位
if build -O standard && miss && output_is "你好，缓存"; then pass "优化档位变化不命中"; else fail "优化档位变化不命中"; cat "$LOG"; fi
if build -O standard && hit; then pass "同一优化档位再编命中"; else fail "同一优化档位再编命中"; cat "$LOG"; fi

# ⑤ 影响 codegen 的环境变量
if QI_LINT=0 build && miss; then pass "QI_LINT 变化不命中"; else fail "QI_LINT 变化不命中"; cat "$LOG"; fi
if QI_ARC=0 build && miss && output_is "你好，缓存"; then pass "QI_ARC 变化不命中"; else fail "QI_ARC 变化不命中"; cat "$LOG"; fi

# ⑧ 运行时归档换一份（复制到别处：路径/修改时间都变）
if [ -n "${QI_RUNTIME_LIB:-}" ] && [ -f "$QI_RUNTIME_LIB" ]; then
    cp "$QI_RUNTIME_LIB" "$WORK/libqi_runtime.a"
    if QI_RUNTIME_LIB="$WORK/libqi_runtime.a" build && miss && output_is "你好，缓存"; then
        pass "运行时归档变化不命中"
    else
        fail "运行时归档变化不命中"; cat "$LOG"
    fi
    touch "$WORK/libqi_runtime.a"
    if QI_RUNTIME_LIB="$WORK/libqi_runtime.a" build && miss; then
        pass "运行时归档被 touch 后不命中"
    else
        fail "运行时归档被 touch 后不命中"; cat "$LOG"
    fi
else
    echo "跳过运行时归档用例（没设 QI_RUNTIME_LIB）"
fi

# ⑨ 并行 codegen 的多个目标文件：分片文件一起进缓存、一起还原；
#    回到串行时旧分片文件被清掉（否则会跟这次的 .o 混在一起）
PART1="$WORK/src/主.qi-part1.o"
PART2="$WORK/src/主.qi-part2.o"
if QI_CODEGEN_PARTITIONS=3 build && miss && [ -f "$PART1" ] && [ -f "$PART2" ] && output_is "你好，缓存"; then
    pass "并行 codegen 产出分片目标文件且程序正确"
else
    fail "并行 codegen 产出分片目标文件且程序正确"; cat "$LOG"; ls "$WORK/src"
fi
cp "$OUT" "$WORK/par.bin"
rm -f "$PART1" "$PART2"
if QI_CODEGEN_PARTITIONS=3 build && hit && [ -f "$PART1" ] && [ -f "$PART2" ] && cmp -s "$OUT" "$WORK/par.bin"; then
    pass "并行产物命中时分片文件一并还原"
else
    fail "并行产物命中时分片文件一并还原"; cat "$LOG"; ls "$WORK/src"
fi
if build && hit && [ ! -e "$PART1" ] && [ ! -e "$PART2" ] && output_is "你好，缓存"; then
    pass "回到串行产物时旧分片文件被清掉"
else
    fail "回到串行产物时旧分片文件被清掉"; cat "$LOG"; ls "$WORK/src"
fi
if QI_NO_CACHE=1 QI_CODEGEN_PARTITIONS=3 build && QI_NO_CACHE=1 build && [ ! -e "$PART1" ]; then
    pass "不走缓存时串行编译也清掉旧分片文件"
else
    fail "不走缓存时串行编译也清掉旧分片文件"; cat "$LOG"; ls "$WORK/src"
fi

# ⑦ 警告重放：结构体互相强持有 → 环检测警告
cat > "$ENTRY" <<'QI'
包 主程序;
导入 标准库.输入输出 作为 IO;
结构体 甲 { 伙伴: 乙 }
结构体 乙 { 伙伴: 甲 }
函数 入口() {
    IO.打印行("环");
}
QI
if build && miss && grep -q '环检测' "$LOG"; then pass "首次编译打出环检测警告"; else fail "首次编译打出环检测警告"; cat "$LOG"; fi
if build && hit && grep -q '环检测' "$LOG" && output_is "环"; then
    pass "命中时重放环检测警告"
else
    fail "命中时重放环检测警告"; cat "$LOG"
fi

# 编译失败不写缓存，也不影响下次
cat > "$ENTRY" <<'QI'
包 主程序;
函数 入口() {
    变量 x: 整数 = "不是整数";
}
QI
if ! build && ! stored; then pass "编译失败不写缓存"; else fail "编译失败不写缓存"; cat "$LOG"; fi

echo "构建缓存: $passed/$((passed+failed)) 通过"
[ "$failed" -eq 0 ]
