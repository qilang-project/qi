/* 现场手写的小头文件 —— 专门覆盖 `qi 绑定` 的各个边角。
 *
 * 注意：C 侧一律 ASCII 命名（中文只写在 .qi 里）。
 * 每一条都在断言.sh 里有对应断言，改这里要同步改那边。 */
#ifndef BIANJIAO_H
#define BIANJIAO_H

/* ── #define 常量：只有纯数字字面量的会变成 qi 的 常量 ── */
#define BJ_MAX 42
#define BJ_HEX 0x1F           /* qi 没有 0x 字面量，生成时换算成十进制 31 */
#define BJ_NEG (-3)           /* 带括号的负数 */
#define BJ_BIG 4000000000L    /* 超出 int，但 i64 装得下 */
#define BJ_STR "not a number" /* 字符串宏 —— 不收 */
#define BJ_FN(x) ((x) + 1)    /* 函数式宏 —— 不收 */
#define _BJ_PRIVATE 7         /* 下划线开头 —— 不收 */

/* ── enum：每个枚举值一条 常量，不写 = 的按「上一个 + 1」算 ── */
enum bj_color { BJ_RED, BJ_GREEN, BJ_BLUE = 10, BJ_NEXT };

/* ── 不透明结构体：只以 struct* 出现，映射成 指针 ── */
struct bj_box;

/* ── 按值传/返的结构体：v1 跳过并记录 ── */
struct bj_pair {
  long a;
  long b;
};

/* ── 前缀过滤：加了 --前缀 bj_ 就只剩 bj_add ── */
int bj_add(int a, int b);
int other_add(int a, int b);

/* ── 重复声明：生成时去重，只出现一次 ── */
int bj_add(int a, int b);

/* ── 函数指针参数（C 回调槽）── */
long bj_reduce(const long *arr, long n, long (*step)(long acc, long item));

/* ── struct* 参数 + const char* 返回 ── */
struct bj_box *bj_box_new(long seed);
long bj_box_get(struct bj_box *b);
const char *bj_box_name(struct bj_box *b);
void bj_box_free(struct bj_box *b);

/* ── 变参：qi 外部块不支持，跳过并记录 ── */
int bj_sum_all(int count, ...);

/* ── 按值传/返结构体：跳过并记录 ── */
struct bj_pair bj_make_pair(long a, long b);
long bj_pair_sum(struct bj_pair p);

/* ── 无名形参：生成时补 arg1 / arg2 ── */
double bj_hypot(double, double);

/* ── 头文件里的 static inline：库里没有这个符号，跳过并记录 ── */
static inline int bj_inline_twice(int x) { return x * 2; }

/* ── 无参 + void 返回 ── */
void bj_reset(void);
long bj_counter(void);

/* ── 非 const 的 char* 形参是输出缓冲区 → 指针；const char* 形参 → 字符串 ── */
long bj_copy_name(const char *src, char *dst, long cap);

#endif
