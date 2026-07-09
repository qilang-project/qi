/* 从 C 调用 Qi 中文函数（反向 FFI）。
 *
 * 构建并运行（静态库）：
 *   qi compile --库 静态 数学库.qi -o lib数学库.a
 *   clang test.c lib数学库.a -o test && ./test
 *
 * clang/gcc 默认接受 UTF-8 标识符，故可直接书写并调用中文函数名 加法/减法/问候。
 * 也可用显式 ASCII 符号 qi_add / qi_fib（见 数学库.qi）。
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "lib数学库.h"

int main(void) {
    /* 整数：直接用中文符号 */
    int64_t s = 加法(3, 4);
    printf("加法(3, 4)     = %lld\n", (long long)s);

    int64_t d = 减法(10, 3);
    printf("减法(10, 3)    = %lld\n", (long long)d);

    /* 字符串往返：返回值 C 拥有，用毕 free() */
    char *g = 问候("世界");
    printf("问候(\"世界\")   = %s\n", g);
    int str_ok = (g != NULL && strcmp(g, "你好, 世界") == 0);
    free(g);

    /* 显式 ASCII 符号 */
    printf("qi_add(20, 22) = %lld\n", (long long)qi_add(20, 22));
    printf("qi_fib(10)     = %lld\n", (long long)qi_fib(10));

    /* 断言 */
    int ok = (s == 7) && (d == 7) && str_ok
             && (qi_add(20, 22) == 42) && (qi_fib(10) == 55);
    printf(ok ? "\n[OK] 所有断言通过\n" : "\n[FAIL] 断言失败\n");
    return ok ? 0 : 1;
}
