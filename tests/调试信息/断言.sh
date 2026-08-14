#!/bin/bash
# DWARF 调试信息端到端验收 —— 真的把 lldb 跑起来，不看 IR 猜。
#
# 用法: bash tests/调试信息/断言.sh <qi 可执行文件路径>
#       QI_RUNTIME_LIB=/path/to/libqi_runtime.a bash tests/调试信息/断言.sh target/release/qi
#
# 为什么必须实跑 lldb：调试信息「生成了」和「调试器用得上」是两回事 ——
# 元数据齐全但模块标志漏一个、finalize 漏一次、macOS 的 debug map 找不到 .o，
# dwarfdump 全都看不出问题，只有断点不命中才暴露。
#
# 优化档位的差别（重要，别当成 bug）：
#   qi 默认 -O basic（= LLVM O1）。O1 下小函数被内联、语句被重排，
#   「按 文件:行号 下断点」经常落在一份从不执行的 out-of-line 副本上 ——
#   **clang -O1 编的 C 程序表现完全一样**，这是优化的固有代价，不是 qi 的缺陷。
#   所以本脚本：
#     - 无优化档（-O none）验收完整调试体验：文件行号断点 / backtrace / 单步 / 变量；
#     - 默认优化档只验收较弱但稳定的性质：函数符号断点能命中、backtrace 有函数名。
#
# shell 变量一律 ASCII（macOS 自带 bash 3.2，中文变量名在它上面不可靠）。

set -u

QI_BIN="${1:-}"
if [ -z "$QI_BIN" ]; then
  QI_BIN="${QI_BIN:-}"
fi
if [ -z "$QI_BIN" ] || [ ! -x "$QI_BIN" ]; then
  echo "用法: bash $0 <qi 可执行文件路径>"
  exit 2
fi
case "$QI_BIN" in
  /*) ;;
  *) QI_BIN="$PWD/$QI_BIN" ;;
esac

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
WORK_DIR="$SCRIPT_DIR/_产物"
SRC_NAME="斐波那契.qi"
EXE_NAME="斐波那契"

PASS=0
FAIL=0

pass() { PASS=$((PASS + 1)); echo "  [PASS] $1"; }
fail() { FAIL=$((FAIL + 1)); echo "  [FAIL] $1"; }
skip() { echo "  [SKIP] $1"; }

# 断言 文件 里含有 模式（grep -F 定值匹配，避免中文/正则元字符互相干扰）
assert_contains() {
  # $1=描述 $2=文件 $3=定值模式
  if grep -qF "$3" "$2" 2>/dev/null; then
    pass "$1"
  else
    fail "$1（在 $2 中找不到: $3）"
  fi
}

assert_not_contains() {
  if grep -qF "$3" "$2" 2>/dev/null; then
    fail "$1（$2 中不该出现却出现了: $3）"
  else
    pass "$1"
  fi
}

# UTF-8 字符串 → dwarfdump 打印用的八进制转义（"斐" → \346\226\220）。
# dwarfdump 对 .debug_str 里的非 ASCII 一律转义，DW_AT_decl_file 却是原样，
# 两种都要断言，所以得能算出转义形式。
to_octal() {
  printf '%s' "$1" | od -An -to1 -v | tr -s ' ' '\n' | grep -v '^$' | sed 's/^/\\/' | tr -d '\n'
}

# ---- 工具探测 ----------------------------------------------------------
DWARFDUMP=""
for c in llvm-dwarfdump dwarfdump; do
  if command -v "$c" >/dev/null 2>&1; then DWARFDUMP="$c"; break; fi
done
# macOS 上 Homebrew 的 lldb 常年缺 debugserver 的签名，控制不了进程；
# 系统自带那个（随 Xcode CLT）才是能用的。优先它。
LLDB=""
if [ -x /usr/bin/lldb ]; then
  LLDB=/usr/bin/lldb
elif command -v lldb >/dev/null 2>&1; then
  LLDB="$(command -v lldb)"
fi

RUNTIME_ARG=""
if [ -n "${QI_RUNTIME_LIB:-}" ]; then
  export QI_RUNTIME_LIB
fi

echo "=== DWARF 调试信息验收 ==="
echo "编译器: $QI_BIN"
echo "dwarfdump: ${DWARFDUMP:-（无）}"
echo "lldb: ${LLDB:-（无）}"
echo

rm -rf "$WORK_DIR"
mkdir -p "$WORK_DIR"
cp "$SCRIPT_DIR/$SRC_NAME" "$WORK_DIR/$SRC_NAME"
cd "$WORK_DIR" || exit 2

# ---- 1. 无优化档编译（调试信息默认开，不用加任何开关）------------------
echo "-- 1. 编译（-O none，默认带调试信息）--"
if "$QI_BIN" -O none compile "$SRC_NAME" >编译.log 2>&1; then
  pass "编译成功"
else
  fail "编译失败"
  cat 编译.log
  echo "PASS=$PASS FAIL=$FAIL"
  exit 1
fi

OUT="$(./"$EXE_NAME" 2>&1)"
if [ "$OUT" = "88" ]; then
  pass "程序输出正确（88）"
else
  fail "程序输出错误：期望 88，实得 [$OUT]"
fi

# ---- 2. dwarfdump：中文函数名 + .qi 源文件 ------------------------------
echo
echo "-- 2. DWARF 元数据 --"
if [ -z "$DWARFDUMP" ]; then
  skip "没有 llvm-dwarfdump/dwarfdump，跳过元数据断言"
else
  "$DWARFDUMP" --debug-info "$EXE_NAME.o" >dwarf.txt 2>&1
  # DW_AT_name 里的中文函数名（八进制转义形式）
  for fname in 斐波那契 累加 入口; do
    assert_contains "DW_AT_name 含中文函数名「${fname}」" dwarf.txt "\"$(to_octal "$fname")\""
  done
  # DW_AT_decl_file 指向 .qi 源文件（这一项 dwarfdump 原样打印）
  assert_contains "DW_AT_decl_file 指向 ${SRC_NAME}" dwarf.txt "DW_AT_decl_file"
  assert_contains "DW_AT_decl_file 路径含 ${SRC_NAME}" dwarf.txt "$SRC_NAME"
  # linkage name = mangled 符号，能跟 nm 对上
  assert_contains "DW_AT_linkage_name 是 mangled 符号" dwarf.txt "DW_AT_linkage_name"
  # 语言码
  assert_contains "编译单元语言标 DW_LANG_C99" dwarf.txt "DW_LANG_C99"
  # 局部变量 / 形参（P1）
  assert_contains "有形参条目 DW_TAG_formal_parameter" dwarf.txt "DW_TAG_formal_parameter"
  assert_contains "有局部变量条目 DW_TAG_variable" dwarf.txt "DW_TAG_variable"
  assert_contains "局部变量名「合计」入表" dwarf.txt "\"$(to_octal 合计)\""

  # 行号表非空 —— 断点全靠它
  "$DWARFDUMP" --debug-line "$EXE_NAME.o" >dwarfline.txt 2>&1
  # 行号表的 file_names 也是八进制转义的
  assert_contains "行号表里有 ${SRC_NAME}" dwarfline.txt "$(to_octal 斐波那契).qi"
  assert_contains "行号表有真实行号条目（is_stmt 行）" dwarfline.txt "is_stmt"
fi

# ---- 3. lldb 实跑 -------------------------------------------------------
echo
echo "-- 3. lldb 端到端 --"

# 预检：这台机器/这个 shell 到底能不能控制进程？沙箱、缺 debugserver 签名、
# 无 developer mode 都会让 lldb「设得上断点但进程照跑不停」。用一个
# clang -g -O0 的 C 程序当对照——它停不下来就说明是环境问题，不是 qi 的问题，
# 此时报 SKIP 而不是 FAIL（否则每个沙箱里跑的人都会看到一片假红）。
LLDB_USABLE=0
if [ -n "$LLDB" ] && command -v clang >/dev/null 2>&1; then
  printf '#include <stdio.h>\nint f(int x){return x*2;}\nint main(){printf("%%d\\n", f(21));return 0;}\n' >对照.c
  if clang -g -O0 -o 对照 对照.c >/dev/null 2>&1; then
    "$LLDB" -b -o "breakpoint set -f 对照.c -l 2" -o run -o quit ./对照 >对照.log 2>&1
    if grep -q "stop reason = breakpoint" 对照.log; then
      LLDB_USABLE=1
    fi
  fi
fi

if [ -z "$LLDB" ]; then
  skip "没有 lldb，跳过端到端调试验收"
elif [ "$LLDB_USABLE" -eq 0 ]; then
  skip "lldb 在本环境无法控制进程（clang -g -O0 的对照程序也停不下来）——沙箱/签名/开发者模式问题，非 qi 缺陷"
else
  # 3.1 按 文件:行号 下断点（第 16 行 = 累加() 循环体里调 斐波那契 那行）
  "$LLDB" -b \
    -o "breakpoint set -f $SRC_NAME -l 16" \
    -o run \
    -o "thread backtrace" \
    -o "frame variable" \
    -o next -o "frame info" \
    -o next -o "frame info" \
    -o quit ./"$EXE_NAME" >lldb.log 2>&1

  assert_contains "文件:行号 断点真的命中" lldb.log "stop reason = breakpoint"
  assert_contains "backtrace 里有 ${SRC_NAME}:16" lldb.log "$SRC_NAME:16"
  assert_contains "backtrace 里有 qi 函数名（累加 的 mangled 符号）" lldb.log "_Z_"
  assert_contains "backtrace 里有 main 帧且定位到 .qi" lldb.log "main at $SRC_NAME"
  # 形参 / 局部变量（P1）：中文名 + 值
  assert_contains "frame variable 看得到形参 上限" lldb.log "上限 = 10"
  assert_contains "frame variable 看得到局部变量 合计" lldb.log "合计 = 0"

  # 单步：两次 next，行号必须动过（不要求动到具体哪行 —— 那会把测试焊死在
  # 当前的代码生成顺序上，codegen 一改就假红）。
  STEP_LINES="$(grep -o "at $SRC_NAME:[0-9]*" lldb.log | sed "s/.*://" | tr '\n' ' ')"
  STEP_COUNT="$(echo "$STEP_LINES" | tr ' ' '\n' | grep -c '[0-9]')"
  UNIQ_COUNT="$(echo "$STEP_LINES" | tr ' ' '\n' | grep '[0-9]' | sort -u | wc -l | tr -d ' ')"
  if [ "$STEP_COUNT" -ge 3 ] && [ "$UNIQ_COUNT" -ge 2 ]; then
    pass "单步 next 推进了行号（经过的行: ${STEP_LINES}）"
  else
    fail "单步没有推进行号（经过的行: ${STEP_LINES}）"
  fi

  # 3.2 默认优化档（-O basic）：只断言函数级断点 + backtrace 有函数名。
  # 行号在 O1 下会跳（内联/重排），见文件头说明。
  "$QI_BIN" compile "$SRC_NAME" >编译O1.log 2>&1
  SYM="$(nm -U ./"$EXE_NAME" 2>/dev/null | grep -o '_Z_[0-9A-F]*' | head -1)"
  if [ -z "$SYM" ]; then
    SYM="$(nm ./"$EXE_NAME" 2>/dev/null | grep -o '_Z_[0-9A-F]*' | head -1)"
  fi
  if [ -z "$SYM" ]; then
    fail "优化档产物里找不到 qi 函数符号"
  else
    "$LLDB" -b -o "breakpoint set -n $SYM" -o run -o "thread backtrace" -o quit \
      ./"$EXE_NAME" >lldbO1.log 2>&1
    assert_contains "优化档：函数符号断点命中" lldbO1.log "stop reason = breakpoint"
    assert_contains "优化档：backtrace 仍能定位到 ${SRC_NAME}" lldbO1.log "$SRC_NAME:"
  fi
fi

# ---- 4. --无调试信息：产物里查不到调试条目 ------------------------------
echo
echo "-- 4. --无调试信息 开关 --"
rm -f "$EXE_NAME" "$EXE_NAME.o"
if "$QI_BIN" -O none --无调试信息 compile "$SRC_NAME" >编译无调试.log 2>&1; then
  pass "--无调试信息 编译成功"
else
  fail "--无调试信息 编译失败"
  cat 编译无调试.log
fi

OUT2="$(./"$EXE_NAME" 2>&1)"
if [ "$OUT2" = "88" ]; then
  pass "关调试信息后程序行为不变（仍输出 88）"
else
  fail "关调试信息后输出变了：期望 88，实得 [$OUT2]"
fi

if [ -z "$DWARFDUMP" ]; then
  skip "没有 dwarfdump，跳过「无调试条目」断言"
else
  "$DWARFDUMP" --debug-info "$EXE_NAME.o" >dwarf无.txt 2>&1
  assert_not_contains "无调试信息产物里没有 qi 函数的 DWARF 条目" dwarf无.txt "DW_TAG_subprogram"
  assert_not_contains "无调试信息产物里没有编译单元" dwarf无.txt "DW_TAG_compile_unit"
fi

# ---- 收尾 --------------------------------------------------------------
cd "$SCRIPT_DIR" || exit 2
rm -rf "$WORK_DIR"

echo
echo "=== 结果: PASS=$PASS FAIL=$FAIL ==="
if [ "$FAIL" -gt 0 ]; then
  exit 1
fi
exit 0
