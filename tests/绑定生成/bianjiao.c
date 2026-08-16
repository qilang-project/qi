/* bianjiao.h 的实现，编成 libbianjiao.a 供 qi 直链。 */
#include "bianjiao.h"
#include <stdarg.h>
#include <stdlib.h>

struct bj_box {
  long value;
  const char *name;
};

static long g_counter = 0;

int bj_add(int a, int b) { return a + b; }
int other_add(int a, int b) { return a + b + 1000; }

long bj_reduce(const long *arr, long n, long (*step)(long acc, long item)) {
  long acc = 0;
  for (long i = 0; i < n; i++) {
    acc = step(acc, arr[i]);
  }
  return acc;
}

struct bj_box *bj_box_new(long seed) {
  struct bj_box *b = (struct bj_box *)malloc(sizeof(struct bj_box));
  b->value = seed * 3;
  b->name = "bianjiao-box";
  g_counter++;
  return b;
}

long bj_box_get(struct bj_box *b) { return b->value; }
const char *bj_box_name(struct bj_box *b) { return b->name; }
void bj_box_free(struct bj_box *b) { free(b); }

int bj_sum_all(int count, ...) {
  va_list ap;
  int s = 0;
  va_start(ap, count);
  for (int i = 0; i < count; i++) s += va_arg(ap, int);
  va_end(ap);
  return s;
}

struct bj_pair bj_make_pair(long a, long b) {
  struct bj_pair p;
  p.a = a;
  p.b = b;
  return p;
}

long bj_pair_sum(struct bj_pair p) { return p.a + p.b; }

double bj_hypot(double x, double y) { return x * x + y * y; }

void bj_reset(void) { g_counter = 0; }
long bj_counter(void) { return g_counter; }

long bj_copy_name(const char *src, char *dst, long cap) {
  long i = 0;
  while (src[i] != '\0' && i < cap - 1) {
    dst[i] = src[i];
    i++;
  }
  dst[i] = '\0';
  return i;
}
