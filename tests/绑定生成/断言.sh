#!/bin/bash
# `qi 绑定` 端到端验收 —— 真头文件、真链接、真调用。
#
# 三条主线：
#   1. zlib：系统头文件 → 生成 → qi check → compress/uncompress 往返一致
#   2. libm：最大的系统头文件之一 → cos(0)=1.0
#   3. bianjiao.h：现场手写的小头文件，覆盖函数指针/变参/宏/enum/struct*/去重/前缀
#
# bash 3.2 兼容（macOS 自带的就是 3.2）：不用 declare -A、不用 ${var^^}、不用 mapfile。
# 变量名一律 ASCII —— bash 3.2 的变量名只认 [A-Za-z_][A-Za-z0-9_]*，
# 写成中文会报「未绑定的变量」，而且报错信息还是乱码，排查起来很费劲。

set -u

SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd)
WORK="$SCRIPT_DIR/构建"

PASS=0
FAIL=0

pass() {
  PASS=$((PASS + 1))
  echo "PASS  $1"
}

fail() {
  FAIL=$((FAIL + 1))
  echo "FAIL  $1"
  if [ $# -gt 1 ]; then
    echo "      $2"
  fi
}

# 断言：文件里有某段文本
assert_has() {
  # $1=文件 $2=模式 $3=用例名
  if grep -q -- "$2" "$1" 2>/dev/null; then
    pass "$3"
  else
    fail "$3" "在 $1 里没找到 $2"
  fi
}

# 断言：文件里没有某段文本
assert_missing() {
  if grep -q -- "$2" "$1" 2>/dev/null; then
    fail "$3" "$1 里不该出现 $2"
  else
    pass "$3"
  fi
}

# 断言：某模式在文件里恰好出现 N 次
assert_count() {
  # $1=文件 $2=模式 $3=期望次数 $4=用例名
  GOT=$(grep -c -- "$2" "$1" 2>/dev/null | tr -d ' ')
  if [ "$GOT" = "$3" ]; then
    pass "$4"
  else
    fail "$4" "期望 $3 次，实际 $GOT 次"
  fi
}

# 断言：程序输出里有某行
assert_out() {
  # $1=输出文件 $2=期望子串 $3=用例名
  if grep -qF -- "$2" "$1" 2>/dev/null; then
    pass "$3"
  else
    fail "$3" "输出里没有 [$2]；实际输出见 $1"
  fi
}

# ── 找编译器 ─────────────────────────────────────────────
# 第一个位置参数优先（Makefile 那边是 `断言.sh $(QI)`），其次环境变量 QI，
# 最后按相对路径猜（worktree 用 target-wt、主树用 target）。
if [ $# -ge 1 ] && [ -n "$1" ]; then
  QI="$1"
elif [ -n "${QI:-}" ]; then
  :
elif [ -x "$SCRIPT_DIR/../../target-wt/release/qi" ]; then
  QI="$SCRIPT_DIR/../../target-wt/release/qi"
elif [ -x "$SCRIPT_DIR/../../../target/release/qi" ]; then
  QI="$SCRIPT_DIR/../../../target/release/qi"
else
  echo "找不到 qi 可执行文件，先 cargo build --release，或设 QI=<路径>"
  exit 1
fi
echo "使用编译器：$QI"

# 运行时归档：没设就自己找一份。
# 在 git worktree 里跑时，工作区只有 qi/ 这一层，qi-runtime 还在主仓那边，
# 所以先认 CARGO_WORKSPACE_DIR，再退回「qi/ 的兄弟目录」。
if [ -z "${QI_RUNTIME_LIB:-}" ]; then
  for CAND in \
    "${CARGO_WORKSPACE_DIR:-/nonexistent}/qi-runtime/target/release/libqi_runtime.a" \
    "$SCRIPT_DIR/../../../qi-runtime/target/release/libqi_runtime.a"; do
    if [ -f "$CAND" ]; then
      QI_RUNTIME_LIB="$CAND"
      export QI_RUNTIME_LIB
      echo "运行时归档：$QI_RUNTIME_LIB"
      break
    fi
  done
fi

# ── 找系统头文件 ─────────────────────────────────────────
if command -v xcrun >/dev/null 2>&1; then
  SDK=$(xcrun --show-sdk-path 2>/dev/null)
  INC_DIR="$SDK/usr/include"
else
  INC_DIR="/usr/include"
fi
ZLIB_H="$INC_DIR/zlib.h"
MATH_H="$INC_DIR/math.h"

rm -rf "$WORK"
mkdir -p "$WORK"
cd "$WORK" || exit 1

# ══ 1. zlib：生成 → check → 真压真解 ═══════════════════════
if [ ! -f "$ZLIB_H" ]; then
  fail "zlib 生成" "本机没有 $ZLIB_H"
else
  if "$QI" 绑定 "$ZLIB_H" --库 z -o zlib绑定.qi >生成.log 2>&1; then
    pass "zlib 生成绑定"
  else
    fail "zlib 生成绑定" "$(cat 生成.log)"
  fi

  if [ -f zlib绑定.qi ]; then
    assert_has zlib绑定.qi '外部 "z" {' "zlib 外部块用了 --库 给的库名"
    assert_has zlib绑定.qi '函数 compressBound(sourceLen: 整数): 整数;' "zlib compressBound 签名（uLong typedef 展开成整数）"
    assert_has zlib绑定.qi '函数 compress(dest: 指针, destLen: 指针, source: 指针, sourceLen: 整数): C整数;' "zlib compress 签名（Bytef*/uLongf* 展开成指针）"
    assert_has zlib绑定.qi '函数 zlibVersion(): 字符串;' "zlib const char* 返回映射成字符串"
    assert_has zlib绑定.qi 'in: 函数(指针, 指针): 整数' "zlib 函数指针参数映射成 qi 回调类型"
    assert_has zlib绑定.qi '常量 Z_OK = 0;' "zlib #define 数字宏变成常量"
    assert_has zlib绑定.qi '常量 ZLIB_VERNUM = 4800;' "zlib 十六进制宏换算成十进制"
    assert_missing zlib绑定.qi 'ZLIB_VERSION = ' "zlib 字符串宏不收"
    assert_missing zlib绑定.qi '函数 gzprintf' "zlib 变参函数被跳过"
    assert_has zlib绑定.qi '// 跳过清单' "zlib 顶部注释有跳过清单"
    assert_has zlib绑定.qi 'gzprintf：变参函数' "zlib 跳过清单记了 gzprintf 及原因"
    assert_has zlib绑定.qi '// 命令：qi 绑定' "zlib 顶部注释记了生成命令"
    assert_has zlib绑定.qi '// 生成日期：' "zlib 顶部注释记了生成日期"

    if "$QI" check zlib绑定.qi >check.log 2>&1; then
      pass "zlib 生成文件通过 qi check"
    else
      fail "zlib 生成文件通过 qi check" "$(cat check.log)"
    fi

    cp "$SCRIPT_DIR/zlib往返.qi" .
    if "$QI" run zlib往返.qi >zlib.out 2>&1; then
      pass "zlib 往返程序跑通"
      assert_out zlib.out "上界大于原长: 是" "zlib compressBound(512) > 512"
      assert_out zlib.out "compress 返回码=0" "zlib compress 返回 Z_OK"
      assert_out zlib.out "uncompress 返回码=0" "zlib uncompress 返回 Z_OK"
      assert_out zlib.out "还原长=512" "zlib 解压后长度回到原长"
      assert_out zlib.out "往返一致: 是" "zlib 压缩解压往返字节一致"
      assert_out zlib.out "返回码都是 Z_OK: 是" "zlib 生成的常量 Z_OK 可用"
      assert_out zlib.out "Z_BEST_COMPRESSION=9" "zlib 常量值正确"
      assert_out zlib.out "版本非空: 是" "zlib char* 返回拷成 qi 串可用"
      assert_out zlib.out "坏数据返回负数错误码: 是" "zlib C整数 返回：Z_DATA_ERROR 读成负数"
      assert_out zlib.out "内存已释放，未崩溃" "zlib 全程无崩溃"
    else
      fail "zlib 往返程序跑通" "$(tail -20 zlib.out)"
    fi
  fi
fi

# ══ 2. libm ═══════════════════════════════════════════════
if [ ! -f "$MATH_H" ]; then
  fail "libm 生成" "本机没有 $MATH_H"
else
  if "$QI" 绑定 "$MATH_H" --库 m -o libm绑定.qi >生成m.log 2>&1; then
    pass "libm 生成绑定"
  else
    fail "libm 生成绑定" "$(cat 生成m.log)"
  fi
  if [ -f libm绑定.qi ]; then
    assert_has libm绑定.qi '函数 cos(' "libm 收到了 cos"
    assert_missing libm绑定.qi '函数 __' "libm 编译器内建函数（__builtin 类）被排除"
    if "$QI" check libm绑定.qi >checkm.log 2>&1; then
      pass "libm 生成文件通过 qi check"
    else
      fail "libm 生成文件通过 qi check" "$(cat checkm.log)"
    fi
    cp "$SCRIPT_DIR/libm用例.qi" .
    if "$QI" run libm用例.qi >libm.out 2>&1; then
      pass "libm 程序跑通"
      assert_out libm.out "cos(0) 等于 1.0: 是" "libm cos(0)=1.0"
      assert_out libm.out "sqrt(16)=4" "libm sqrt(16)=4"
      assert_out libm.out "pow(2,10)=1024" "libm pow(2,10)=1024"
    else
      fail "libm 程序跑通" "$(tail -20 libm.out)"
    fi
  fi
fi

# ══ 3. 手写头文件的边角 ═══════════════════════════════════
cp "$SCRIPT_DIR/bianjiao.h" "$SCRIPT_DIR/bianjiao.c" "$SCRIPT_DIR/边角用例.qi" .
if clang -c bianjiao.c -o bianjiao.o 2>c.log && ar rcs libbianjiao.a bianjiao.o 2>>c.log; then
  pass "边角 C 库编译"
else
  fail "边角 C 库编译" "$(cat c.log)"
fi

# 3a. 带前缀过滤
if "$QI" 绑定 bianjiao.h --库 ./libbianjiao.a --前缀 bj_ -o 边角绑定.qi >生成b.log 2>&1; then
  pass "边角 生成绑定（--前缀 bj_）"
else
  fail "边角 生成绑定（--前缀 bj_）" "$(cat 生成b.log)"
fi

if [ -f 边角绑定.qi ]; then
  # 前缀过滤
  assert_has 边角绑定.qi '函数 bj_add(a: 整数, b: 整数): C整数;' "边角 前缀内的函数保留"
  assert_missing 边角绑定.qi 'other_add' "边角 前缀外的函数被过滤掉"
  # 去重（bianjiao.h 里 bj_add 声明了两遍）
  assert_count 边角绑定.qi '函数 bj_add(' 1 "边角 重复声明去重（bj_add 只出现一次）"
  # 函数指针参数
  assert_has 边角绑定.qi 'step: 函数(整数, 整数): 整数' "边角 函数指针参数映射成 qi 回调类型"
  # struct* → 指针，const char* 返回 → 字符串
  assert_has 边角绑定.qi '函数 bj_box_new(seed: 整数): 指针;' "边角 struct* 返回映射成指针"
  assert_has 边角绑定.qi '函数 bj_box_name(b: 指针): 字符串;' "边角 const char* 返回映射成字符串"
  assert_has 边角绑定.qi '函数 bj_box_free(b: 指针): 空;' "边角 void 返回映射成空"
  # const char* 形参 → 字符串，非 const char* 形参 → 指针（输出缓冲区）
  assert_has 边角绑定.qi '函数 bj_copy_name(src: 字符串, dst: 指针, cap: 整数): 整数;' "边角 const char* 形参→字符串、char* 形参→指针"
  # 无名形参补名
  assert_has 边角绑定.qi '函数 bj_hypot(arg1: 浮点数, arg2: 浮点数): 浮点数;' "边角 无名形参补成 arg1/arg2"
  # 无参 + void
  assert_has 边角绑定.qi '函数 bj_reset(): 空;' "边角 无参 void 函数"
  # 窄返回标注
  assert_has 边角绑定.qi '函数 bj_add(a: 整数, b: 整数): C整数;  // C:int 返回' "边角 C int 返回被标注"
  assert_missing 边角绑定.qi '函数 bj_counter(): 整数;  // C:int' "边角 C long 返回不标窄返回"

  # 跳过项：变参 / 按值结构体 / static inline —— 既不生成，也要出现在跳过清单
  assert_missing 边角绑定.qi '函数 bj_sum_all' "边角 变参函数不生成"
  assert_has 边角绑定.qi 'bj_sum_all：变参函数' "边角 跳过清单记了变参函数"
  assert_missing 边角绑定.qi '函数 bj_make_pair' "边角 按值返回结构体不生成"
  assert_has 边角绑定.qi 'bj_make_pair：返回类型 是按值传的 struct bj_pair' "边角 跳过清单记了按值返回结构体"
  assert_missing 边角绑定.qi '函数 bj_pair_sum' "边角 按值传结构体不生成"
  assert_has 边角绑定.qi 'bj_pair_sum：第 1 个参数 是按值传的 struct bj_pair' "边角 跳过清单记了按值传结构体"
  assert_missing 边角绑定.qi '函数 bj_inline_twice' "边角 static inline 不生成"
  assert_has 边角绑定.qi 'bj_inline_twice：头文件里的 static 定义' "边角 跳过清单记了 static inline"

  # 常量：宏 + enum
  assert_has 边角绑定.qi '常量 BJ_MAX = 42;' "边角 十进制宏"
  assert_has 边角绑定.qi '常量 BJ_HEX = 31;' "边角 十六进制宏换算成十进制（qi 没有 0x 字面量）"
  assert_has 边角绑定.qi '常量 BJ_NEG = -3;' "边角 带括号的负数宏"
  assert_has 边角绑定.qi '常量 BJ_BIG = 4000000000;' "边角 带 L 后缀的大整数宏"
  assert_missing 边角绑定.qi 'BJ_STR' "边角 字符串宏不收"
  assert_missing 边角绑定.qi 'BJ_FN' "边角 函数式宏不收"
  assert_missing 边角绑定.qi '_BJ_PRIVATE' "边角 下划线开头的宏不收"
  assert_has 边角绑定.qi '常量 BJ_RED = 0;' "边角 enum 首个值默认 0"
  assert_has 边角绑定.qi '常量 BJ_GREEN = 1;' "边角 enum 隐式递增"
  assert_has 边角绑定.qi '常量 BJ_BLUE = 10;' "边角 enum 显式赋值"
  assert_has 边角绑定.qi '常量 BJ_NEXT = 11;' "边角 enum 显式值之后继续递增"

  if "$QI" check 边角绑定.qi >checkb.log 2>&1; then
    pass "边角 生成文件通过 qi check"
  else
    fail "边角 生成文件通过 qi check" "$(cat checkb.log)"
  fi

  if "$QI" run 边角用例.qi >bj.out 2>&1; then
    pass "边角 程序跑通"
    assert_out bj.out "bj_add(2,3)=5" "边角 整数函数调用"
    assert_out bj.out "bj_hypot(3,4)=25" "边角 浮点函数调用（无名形参）"
    assert_out bj.out "bj_reduce(累加)=54" "边角 C 回调调 qi 函数（求和）"
    assert_out bj.out "bj_reduce(取最大)=30" "边角 换一个 qi 函数换一套回调规则"
    assert_out bj.out "bj_box_get=42" "边角 struct* 当不透明句柄往返"
    assert_out bj.out "bj_box_name=bianjiao-box" "边角 const char* 返回拷成 qi 串"
    assert_out bj.out "bj_copy_name 抄了=8" "边角 const char* 入参 + char* 输出缓冲区"
    assert_out bj.out "BJ_HEX=31" "边角 生成的十六进制常量可用"
    assert_out bj.out "BJ_NEXT=11" "边角 生成的 enum 常量可用"
  else
    fail "边角 程序跑通" "$(tail -20 bj.out)"
  fi
fi

# 3b. 不带前缀：other_add 就该出现（证明过滤确实是前缀干的，不是别的原因）
if "$QI" 绑定 bianjiao.h --库 ./libbianjiao.a -o 边角全量.qi >生成b2.log 2>&1; then
  assert_has 边角全量.qi '函数 other_add(' "边角 不给 --前缀 时 other_add 出现"
else
  fail "边角 不带前缀生成" "$(cat 生成b2.log)"
fi

# 3c. --无常量
if "$QI" 绑定 bianjiao.h --库 x --无常量 -o 边角无常量.qi >生成b3.log 2>&1; then
  assert_missing 边角无常量.qi '^常量 ' "边角 --无常量 时不产出常量"
  assert_has 边角无常量.qi '函数 bj_add(' "边角 --无常量 时函数照常产出"
else
  fail "边角 --无常量 生成" "$(cat 生成b3.log)"
fi

echo
echo "==== 绑定生成验收：PASS=$PASS FAIL=$FAIL ===="
if [ "$FAIL" -gt 0 ]; then
  exit 1
fi
exit 0
