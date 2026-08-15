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
# 三份样本各管一段：
#   斐波那契.qi —— 行号表 / 断点 / 单步 / 标量变量（第 1~4 节）
#   复合类型.qi —— 结构体展开 / 嵌套 / 自引用 / 数组 / 枚举（第 5 节）
#   协程闭包.qi —— 协程体与闭包体的 DISubprogram（第 6 节）
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

# Windows（git-bash）：qi 产的是 斐波那契.exe，不是无后缀的 斐波那契。
IS_WIN=0
case "$(uname -s)" in
  MINGW*|MSYS*|CYGWIN*) IS_WIN=1 ;;
esac
if [ "$IS_WIN" -eq 1 ]; then
  EXE_NAME="斐波那契.exe"
else
  EXE_NAME="斐波那契"
fi
# .o 的名字与后缀无关（源文件名换后缀），两个平台都是 斐波那契.o
OBJ_NAME="斐波那契.o"

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
# 只转非 ASCII：dwarfdump 把可打印 ASCII 原样输出（"外层·闭包0" 里那个 0
# 就是字面的 0，转成 \060 会永远匹配不上）。
to_octal() {
  printf '%s' "$1" | od -An -to1 -v | tr -s ' ' '\n' | grep -v '^$' | while read -r b; do
    if [ "$b" -ge 40 ] && [ "$b" -le 176 ]; then
      printf "\\$(printf '%03o' "0$b")"
    else
      printf '\\%s' "$b"
    fi
  done
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
cp "$SCRIPT_DIR/复合类型.qi" "$WORK_DIR/复合类型.qi"
cp "$SCRIPT_DIR/协程闭包.qi" "$WORK_DIR/协程闭包.qi"
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
  "$DWARFDUMP" --debug-info "$OBJ_NAME" >dwarf.txt 2>&1
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
  "$DWARFDUMP" --debug-line "$OBJ_NAME" >dwarfline.txt 2>&1
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
if [ "$IS_WIN" -eq 1 ]; then
  : # Windows 上不做预检，理由见下面的 SKIP 分支
elif [ -n "$LLDB" ] && command -v clang >/dev/null 2>&1; then
  printf '#include <stdio.h>\nint f(int x){return x*2;}\nint main(){printf("%%d\\n", f(21));return 0;}\n' >对照.c
  if clang -g -O0 -o 对照 对照.c >/dev/null 2>&1; then
    "$LLDB" -b -o "breakpoint set -f 对照.c -l 2" -o run -o quit ./对照 >对照.log 2>&1
    if grep -q "stop reason = breakpoint" 对照.log; then
      LLDB_USABLE=1
    fi
  fi
fi

if [ "$IS_WIN" -eq 1 ]; then
  # Windows 平台性 SKIP（不是「懒得测」，是这条路在 Windows 上本来就不通）：
  # qi 在 Windows 上把 DWARF 塞进 COFF，而 Windows 侧的调试器（VS / WinDbg /
  # lldb 的 Windows 端）走的是 PDB/CodeView 那套，认不出 DWARF；
  # runner 上也没有能控制进程的交互式调试器。
  # 所以 Windows 只验收「元数据到底有没有」（第 2、4 节），断点/单步不假装能验。
  skip "Windows：调试器端到端不验收 —— 产物是 DWARF-in-COFF，Windows 调试器走 PDB/CodeView；runner 无可用交互调试器"
elif [ -z "$LLDB" ]; then
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
rm -f "$EXE_NAME" "$OBJ_NAME"
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
  "$DWARFDUMP" --debug-info "$OBJ_NAME" >dwarf无.txt 2>&1
  assert_not_contains "无调试信息产物里没有 qi 函数的 DWARF 条目" dwarf无.txt "DW_TAG_subprogram"
  assert_not_contains "无调试信息产物里没有编译单元" dwarf无.txt "DW_TAG_compile_unit"
fi

# ---- 5. 复合类型：结构体 / 嵌套 / 自引用 / 数组 / 枚举 --------------------
# 上一轮这些全是 void*，`frame variable` 只给地址。这一节全部断言「看得到字段名和值」。
echo
echo "-- 5. 复合类型展开 --"
CSRC="复合类型.qi"
CEXE="复合类型"
CLINE=31   # 查看() 里的 `返回 岁数;` —— 五种复合值此刻都活着

if "$QI_BIN" -O none compile "$CSRC" >编译复合.log 2>&1; then
  pass "复合类型样本编译成功"
else
  fail "复合类型样本编译失败"
  cat 编译复合.log
fi

COUT="$(./"$CEXE" 2>&1)"
if [ "$COUT" = "18" ]; then
  pass "复合类型样本输出正确（18）"
else
  fail "复合类型样本输出错误：期望 18，实得 [$COUT]"
fi

if [ -z "$DWARFDUMP" ]; then
  skip "没有 dwarfdump，跳过复合类型元数据断言"
else
  "$DWARFDUMP" --debug-info "$CEXE.o" >dwarf复合.txt 2>&1
  assert_contains "有结构体条目 DW_TAG_structure_type" dwarf复合.txt "DW_TAG_structure_type"
  assert_contains "有字段条目 DW_TAG_member" dwarf复合.txt "DW_TAG_member"
  assert_contains "字段名「年龄」入表（中文原名）" dwarf复合.txt "\"$(to_octal 年龄)\""
  assert_contains "字段有偏移 DW_AT_data_member_location" dwarf复合.txt "DW_AT_data_member_location"
  assert_contains "有枚举条目 DW_TAG_enumeration_type" dwarf复合.txt "DW_TAG_enumeration_type"
  assert_contains "有枚举变体 DW_TAG_enumerator" dwarf复合.txt "DW_TAG_enumerator"
  assert_contains "枚举变体名「绿」入表" dwarf复合.txt "\"$(to_octal 绿)\""
  # 结构体值标成引用（qi 的结构体本来就是引用语义）—— lldb 靠它自动展开
  assert_contains "结构体标成 DW_TAG_reference_type" dwarf复合.txt "DW_TAG_reference_type"
  # 自引用没炸成无穷多份：限深展开下 车厢 的条目应是个位数
  CHE_COUNT="$(grep -cF "\"$(to_octal 车厢)\"" dwarf复合.txt)"
  if [ "$CHE_COUNT" -ge 1 ] && [ "$CHE_COUNT" -le 8 ]; then
    pass "自引用结构体条目数收敛（车厢 出现 $CHE_COUNT 次，限深展开生效）"
  else
    fail "自引用结构体条目数异常（车厢 出现 $CHE_COUNT 次，期望 1~8）"
  fi
fi

if [ -z "$LLDB" ] || [ "$LLDB_USABLE" -eq 0 ]; then
  skip "lldb 不可用，跳过复合类型的 lldb 断言"
else
  # 默认 frame variable：一层字段；-P 4 再往下钻（引用成员要显式给深度）
  "$LLDB" -b \
    -o "breakpoint set -f $CSRC -l $CLINE" \
    -o run \
    -o "frame variable" \
    -o "frame variable -P 4" \
    -o quit ./"$CEXE" >lldb复合.log 2>&1

  assert_contains "复合类型断点命中" lldb复合.log "stop reason = breakpoint"
  # 1) 结构体字段展开（不再是 void*）
  assert_contains "结构体展开出字段 年龄 和它的值" lldb复合.log "年龄 = 18"
  assert_contains "结构体的类型显示成 (学生 &) 而不是指针" lldb复合.log "(学生 &) 某学生"
  assert_contains "字符串字段按 C 串打印出内容" lldb复合.log "\"小明\""
  assert_contains "浮点字段值正确" lldb复合.log "分数 = 92.5"
  # 2) 嵌套结构体展开两层（学生 → 住址 → 邮编）
  assert_contains "嵌套结构体展开到第二层（住址.邮编）" lldb复合.log "邮编 = 310000"
  # 3) 自引用结构体：能显示且不死循环（脚本能跑到这里本身就说明没死循环）
  assert_contains "自引用结构体第一层（车头.编号）" lldb复合.log "编号 = 1"
  assert_contains "自引用结构体链下去第三层（编号 = 3）" lldb复合.log "编号 = 3"
  # 4) 数组：长度头看得见
  assert_contains "数组看得到长度字段" lldb复合.log "长度 = 3"
  assert_contains "数组看得到首元素" lldb复合.log "首元素 = 10"
  # 5) 枚举：无载荷打变体名，装箱打 标记 + 载荷
  assert_contains "无载荷枚举打印成变体名（绿）而不是序号" lldb复合.log "某色 = 绿"
  assert_contains "装箱枚举看得到 标记" lldb复合.log "标记 = 数字"
  assert_contains "装箱枚举看得到载荷位模式" lldb复合.log "载荷0 = 42"
fi

# ---- 6. 协程体 / 闭包体的 DISubprogram ----------------------------------
# 这两种函数以前完全没有调试条目：断点设不进去，backtrace 里只有裸地址。
echo
echo "-- 6. 协程与闭包 --"
ASRC="协程闭包.qi"
AEXE="协程闭包"
ALINE_CLOSURE=17   # 闭包体里的 `返回 局部和;`
ALINE_CORO=10      # 协程里 `等待 睡眠(1);` 之后那行

if "$QI_BIN" -O none compile "$ASRC" >编译协程.log 2>&1; then
  pass "协程闭包样本编译成功"
else
  fail "协程闭包样本编译失败"
  cat 编译协程.log
fi

AOUT="$(./"$AEXE" 2>&1 | tr '\n' ' ')"
if [ "$AOUT" = "105 18 " ]; then
  pass "协程闭包样本输出正确（105 18）"
else
  fail "协程闭包样本输出错误：期望 [105 18 ]，实得 [$AOUT]"
fi

if [ -z "$DWARFDUMP" ]; then
  skip "没有 dwarfdump，跳过协程/闭包元数据断言"
else
  "$DWARFDUMP" --debug-info "$AEXE.o" >dwarf协程.txt 2>&1
  assert_contains "闭包有 DISubprogram，名字可读（外层·闭包0）" dwarf协程.txt "\"$(to_octal 外层·闭包0)\""
  assert_contains "协程有 DISubprogram（中文名 慢加）" dwarf协程.txt "\"$(to_octal 慢加)\""
  assert_contains "协程的局部变量 中途 入表" dwarf协程.txt "\"$(to_octal 中途)\""
  assert_contains "闭包的捕获 基数 当局部变量入表" dwarf协程.txt "\"$(to_octal 基数)\""
fi

if [ -z "$LLDB" ] || [ "$LLDB_USABLE" -eq 0 ]; then
  skip "lldb 不可用，跳过协程/闭包的 lldb 断言"
else
  # 6.1 闭包体断点
  "$LLDB" -b \
    -o "breakpoint set -f $ASRC -l $ALINE_CLOSURE" \
    -o run \
    -o "thread backtrace" \
    -o "frame variable" \
    -o quit ./"$AEXE" >lldb闭包.log 2>&1
  assert_contains "闭包体断点命中" lldb闭包.log "stop reason = breakpoint"
  assert_contains "闭包帧定位到 $ASRC:$ALINE_CLOSURE" lldb闭包.log "$ASRC:$ALINE_CLOSURE"
  assert_contains "闭包帧下面就是外层函数帧（外层 里调回调那行）" lldb闭包.log "$ASRC:19"
  assert_contains "闭包形参可见" lldb闭包.log "增量 = 5"
  assert_contains "闭包捕获的外层变量可见" lldb闭包.log "基数 = 100"
  assert_contains "闭包局部变量可见" lldb闭包.log "局部和 = 105"

  # 6.2 协程体断点（等待 之后的行）。CoroSplit 把协程拆成 ramp/.resume/
  #     .destroy/.cleanup，同一行会有多个 location —— 命中哪个克隆都算数。
  SLOW_SYM="_Z_$(printf '%s' '主程序$慢加#1' | od -An -tx1 -v | tr -d ' \n' | tr 'a-f' 'A-F')"
  "$LLDB" -b \
    -o "breakpoint set -f $ASRC -l $ALINE_CORO" \
    -o "breakpoint list" \
    -o run \
    -o "thread backtrace" \
    -o "frame variable" \
    -o quit ./"$AEXE" >lldb协程.log 2>&1
  assert_contains "协程体断点命中（等待 之后的行）" lldb协程.log "stop reason = breakpoint"
  assert_contains "协程断点落在 慢加 的函数体（含其 mangled 符号）" lldb协程.log "$SLOW_SYM"
  assert_contains "协程帧定位回 $ASRC" lldb协程.log "$ASRC:"
  assert_contains "协程跨挂起存活的局部变量可读（中途 = 8）" lldb协程.log "中途 = 8"
  # CoroSplit 后行号表没被丢：三个克隆各自都有这一行的 location
  CORO_LOCS="$(grep -c "$ASRC:" lldb协程.log)"
  if [ "$CORO_LOCS" -ge 2 ]; then
    pass "CoroSplit 后行号表保留（$ASRC 位置出现 $CORO_LOCS 次，含各克隆）"
  else
    fail "CoroSplit 后行号表疑似丢失（$ASRC 位置只出现 $CORO_LOCS 次）"
  fi
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
