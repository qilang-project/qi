#!/usr/bin/env bash
# Windows runner 侦察 —— 只打印，不断言，永远 exit 0。
#
# 存在的理由：本机没有 Windows，唯一的验证环是 GitHub Actions 的 windows runner。
# 「那台机器上有没有 cc/ar」「qi 产的 .obj 里到底是 DWARF 还是 CodeView」这类问题
# 靠猜要烧掉一整轮 CI（15~25 分钟），不如一次把现场全拍下来。
#
# 用法: bash tests/侦察Windows.sh [qi.exe 路径]
# 注意 shell 变量名一律 ASCII（脚本要能被 macOS 的 bash 3.2 读，虽然只在 Windows 跑）。
set -u

QI="${1:-target/release/qi.exe}"
case "$QI" in
  /*) ;;
  *) QI="$PWD/$QI" ;;
esac

echo "=============== 1. uname / shell ==============="
uname -a
echo "BASH_VERSION=$BASH_VERSION"
echo "PWD=$PWD"
echo "cygpath -w PWD = $(cygpath -w "$PWD" 2>/dev/null || echo '(无 cygpath)')"
echo "QI_RUNTIME_LIB=${QI_RUNTIME_LIB:-（未设）}"

echo
echo "=============== 2. PATH 上的工具 ==============="
for t in cc gcc g++ clang clang-cl cl ar lib llvm-ar llvm-lib lld-link link \
         llvm-dwarfdump dwarfdump llvm-objdump objdump llvm-nm nm lldb gdb \
         make md5sum timeout od diff; do
  printf '%-16s %s\n' "$t" "$(command -v "$t" 2>/dev/null || echo '(无)')"
done

echo
echo "=============== 3. LLVM 包内容 ==============="
if [ -n "${LLVM_SYS_211_PREFIX:-}" ]; then
  llvmbin="$(cygpath -u "$LLVM_SYS_211_PREFIX" 2>/dev/null || echo "$LLVM_SYS_211_PREFIX")/bin"
  echo "LLVM bin = $llvmbin"
  ls "$llvmbin" 2>/dev/null | tr '\n' ' '
  echo
else
  echo "LLVM_SYS_211_PREFIX 未设"
fi
clang --version 2>&1 | head -3

echo
echo "=============== 4. 现场造一个静态库（qitest）==============="
# ffi链接 那套要现场 cc + ar 出一个库。Windows 上这两个命令通常都不存在，
# 这里把「用什么编 / 用什么打包 / 打出来的文件 clang 认不认」当场试一遍。
W="$(mktemp -d)"
cat > "$W/qitest.c" <<'EOF'
long qi_test_triple(long x) { return x * 3; }
EOF
for c in clang cc gcc; do
  if command -v "$c" >/dev/null 2>&1; then
    echo "-- $c -c --"
    "$c" -c -o "$W/qitest.o" "$W/qitest.c" 2>&1 | head -5
    echo "   rc=$? 产物: $(ls -l "$W/qitest.o" 2>/dev/null | awk '{print $5}') 字节"
    break
  fi
done
if [ -f "$W/qitest.o" ]; then
  for a in llvm-lib lib; do
    if command -v "$a" >/dev/null 2>&1; then
      echo "-- $a /out: --"
      "$a" "/out:$(cygpath -w "$W/qitest.lib")" "$(cygpath -w "$W/qitest.o")" 2>&1 | head -5
      echo "   rc=$? 产物: $(ls -l "$W/qitest.lib" 2>/dev/null | awk '{print $5}') 字节"
      break
    fi
  done
  for a in llvm-ar ar; do
    if command -v "$a" >/dev/null 2>&1; then
      echo "-- $a rcs（GNU 格式，link.exe 未必吃）--"
      "$a" rcs "$W/qitest_gnu.lib" "$W/qitest.o" 2>&1 | head -5
      echo "   rc=$? 产物: $(ls -l "$W/qitest_gnu.lib" 2>/dev/null | awk '{print $5}') 字节"
      break
    fi
  done
fi

echo
echo "=============== 5. clang 当链接器驱动能不能用 ==============="
# qi 在 Windows 上是 `clang -o x.exe x.o qi_runtime.lib -lkernel32 ...`。
# clang 要自己找到 MSVC 的 link.exe 和 CRT 库；没有 vcvars 环境时能不能找到，
# 是整条链接路径成不成立的前提。
cat > "$W/hello.c" <<'EOF'
#include <stdio.h>
int main(void) { printf("clang-link-ok\n"); return 0; }
EOF
clang -o "$W/hello.exe" "$W/hello.c" 2>&1 | head -20
echo "rc=$?"
if [ -x "$W/hello.exe" ]; then
  echo "运行: $("$W/hello.exe" 2>&1)"
else
  echo "没链出 hello.exe"
fi
# qi 在 Windows 上要加的两个链接开关，先确认 clang 认（不认就是「未知参数」，
# 一眼看得出，省得埋进编译器里再猜）：
#   -fms-runtime-lib=dll  动态 CRT，与 rustc 的 msvc 默认一致
#   -Wl,/Brepro           PE 头时间戳写内容哈希 → 同源码重编字节一致
echo "-- -fms-runtime-lib=dll --"
clang -fms-runtime-lib=dll -o "$W/hello2.exe" "$W/hello.c" 2>&1 | head -10
echo "   rc=$? 运行: $("$W/hello2.exe" 2>&1)"
echo "-- -Wl,/Brepro --"
clang -Wl,/Brepro -o "$W/hello3.exe" "$W/hello.c" 2>&1 | head -10
echo "   rc=$? 运行: $("$W/hello3.exe" 2>&1)"
clang -Wl,/Brepro -o "$W/hello4.exe" "$W/hello.c" 2>&1 | head -5
echo "   两次 /Brepro 产物一致? $(md5sum "$W/hello3.exe" "$W/hello4.exe" | awk '{print $1}' | uniq | wc -l) 个不同哈希（1 = 一致）"

echo
echo "=============== 6. qi 编译一个 .qi ==============="
if [ ! -x "$QI" ]; then
  echo "找不到 $QI，跳过"
else
  "$QI" --version 2>&1 | head -2
  D="$W/qi试"
  mkdir -p "$D"
  cp tests/调试信息/斐波那契.qi "$D/" 2>/dev/null
  ( cd "$D" || exit 0
    echo "-- -O none compile（默认带调试信息）--"
    # 别 head 截断：链接失败时真正有用的是 LNK2019 那几十行，
    # 上一轮就是被 head -30 切掉，只看到「exit code 1120」这么一句。
    "$QI" -O none compile 斐波那契.qi 2>&1 | grep -v "LNK4286\|LNK4217"
    echo "rc=$?"
    ls -l
    # 产物到底叫什么、能不能在 bash 里直接跑
    for cand in 斐波那契 斐波那契.exe; do
      if [ -f "$cand" ]; then
        echo "-- 运行 ./$cand --"
        ./"$cand" 2>&1 | head -3
        echo "   rc=$?"
      fi
    done
    echo
    echo "-- 链接开关组合实验（qi 的 .o + qi_runtime.lib）--"
    # 这一段是本脚本最值钱的地方：把 qi 真正发的那条 clang 链接命令**原样**
    # 试几种 CRT 开关组合，一次跑完就知道哪组能过。否则每猜一次要烧一轮 CI。
    #
    # 背景：clang 驱动无条件写 -defaultlib:libcmt（静态 CRT），而 rustc 编的
    # qi_runtime.lib 要动态 CRT，__imp_* 全解析不了。
    # （-fms-runtime-lib=dll 是**编译期**开关，对纯链接是空操作，别被名字骗了。）
    SYSLIBS="-lkernel32 -luser32 -ladvapi32 -lntdll -luserenv -lws2_32 -lshell32 -lole32"
    RTLIB="$(cygpath -u "${QI_RUNTIME_LIB:-}" 2>/dev/null || echo "${QI_RUNTIME_LIB:-}")"
    if [ -f "斐波那契.o" ] && [ -n "$RTLIB" ]; then
      i=0
      # 组合名::额外开关（用位置参数，bash 3.2 数组的老坑）
      set -- \
        "A 裸链（对照）::" \
        "B 只踢 libcmt::-Wl,/nodefaultlib:libcmt" \
        "C 踢 libcmt + 显式动态 CRT::-Wl,/nodefaultlib:libcmt -Wl,/defaultlib:msvcrt -Wl,/defaultlib:ucrt" \
        "D C + Brepro::-Wl,/nodefaultlib:libcmt -Wl,/defaultlib:msvcrt -Wl,/defaultlib:ucrt -Wl,/Brepro"
      for one in "$@"; do
        name=${one%%::*}
        flags=${one##*::}
        i=$((i+1))
        # shellcheck disable=SC2086
        if clang -o "试$i.exe" 斐波那契.o "$RTLIB" $flags $SYSLIBS >"链接$i.log" 2>&1; then
          echo "  [OK]   $name → 运行: $(./"试$i.exe" 2>&1 | head -1)"
        else
          echo "  [失败] $name"
          grep -E "LNK[0-9]+|error" "链接$i.log" | grep -v "LNK4286\|LNK4217\|LNK4098" | head -4 | sed 's/^/         /'
        fi
      done
      # 确定性：同一组开关连编两次，字节必须一致（PE 头有 TimeDateStamp）
      if [ -f "试4.exe" ]; then
        clang -o "试4b.exe" 斐波那契.o "$RTLIB" \
          -Wl,/nodefaultlib:libcmt -Wl,/defaultlib:msvcrt -Wl,/defaultlib:ucrt -Wl,/Brepro \
          >/dev/null 2>&1
        echo "  /Brepro 两次产物一致? $(md5sum 试4.exe 试4b.exe | awk '{print $1}' | uniq | wc -l) 个不同哈希（1 = 一致）"
      fi
      if [ -f "试3.exe" ]; then
        clang -o "试3b.exe" 斐波那契.o "$RTLIB" \
          -Wl,/nodefaultlib:libcmt -Wl,/defaultlib:msvcrt -Wl,/defaultlib:ucrt >/dev/null 2>&1
        echo "  不加 /Brepro 两次产物一致? $(md5sum 试3.exe 试3b.exe 2>/dev/null | awk '{print $1}' | uniq | wc -l) 个不同哈希"
      fi
    else
      echo "  （没有 斐波那契.o 或 QI_RUNTIME_LIB，跳过）"
    fi

    echo
    echo "-- .o 的节表（找 .debug_* / CodeView 的 .debug\$S）--"
    if command -v llvm-objdump >/dev/null 2>&1; then
      llvm-objdump -h 斐波那契.o 2>&1 | head -40
    fi
    echo
    echo "-- llvm-dwarfdump --debug-info 头 40 行 --"
    if command -v llvm-dwarfdump >/dev/null 2>&1; then
      llvm-dwarfdump --debug-info 斐波那契.o 2>&1 | head -40
    else
      echo "(无 llvm-dwarfdump)"
    fi
    echo
    echo "-- lldb 能不能控制进程（用 clang -g -O0 的 C 程序当对照）--"
    if command -v lldb >/dev/null 2>&1; then
      printf '#include <stdio.h>\nint f(int x){return x*2;}\nint main(){printf("%%d\\n", f(21));return 0;}\n' > 对照.c
      clang -g -O0 -o 对照.exe 对照.c 2>&1 | head -5
      lldb -b -o "breakpoint set -f 对照.c -l 2" -o run -o quit ./对照.exe 2>&1 | tail -20
    else
      echo "(无 lldb)"
    fi
  )
  echo
  echo "-- 外部 \"c\" 在 MSVC 上会变成 c.lib：单独试一下 --"
  D2="$W/qi外部c"
  mkdir -p "$D2"
  cat > "$D2/外部c.qi" <<'EOF'
外部 "c" {
    函数 malloc(字节数: 整数): 指针;
    函数 free(句柄: 指针): 空;
}

函数 入口() {
    变量 p: 指针 = malloc(16);
    free(p);
    打印行("外部c-ok");
}
EOF
  ( cd "$D2" || exit 0
    "$QI" compile 外部c.qi 2>&1 | head -20
    echo "rc=$?"
    for cand in 外部c 外部c.exe; do
      [ -f "$cand" ] && { echo "运行: $(./"$cand" 2>&1)"; }
    done
  )
fi

rm -rf "$W"
echo
echo "=============== 侦察结束（本步骤永不失败）==============="
exit 0
