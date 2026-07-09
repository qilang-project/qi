/* 多线程 FFI 重入 + qi_await 阻塞式异步桥 —— C 压测驱动。
 *
 * 开 1000 个外部 OS 线程，每线程调 Qi 导出函数数千次（整数 + 字符串 + 异步），
 * 校验结果全对、零崩溃。ARC 计数由 QI_RC_REPORT=1 在退出时报告（应归零）。
 *
 * 构建见同目录 构建并压测.sh（普通 / ASan / TSan 三种）。
 */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <pthread.h>

/* 也可 #include "lib多线程FFI库.h"；这里直接声明中文符号（clang 接受 UTF-8 标识符）。 */
extern int64_t 累加(int64_t x);
extern char *拼名(const char *s);
extern int64_t 异步计算(int64_t x);       /* async 桥：C 同步拿到 i64 */
extern char *异步问候(const char *名);     /* async 桥：C 同步拿到 char* */

#ifndef 线程数
#define 线程数 1000
#endif
#ifndef 同步次数
#define 同步次数 2000
#endif
#ifndef 异步次数
#define 异步次数 100
#endif

static volatile int 失败 = 0;

static void 记失败(const char *why) {
    __atomic_store_n(&失败, 1, __ATOMIC_RELAXED);
    fprintf(stderr, "[不匹配] %s\n", why);
}

static void *工作(void *arg) {
    long id = (long)arg;
    char 名[64], 期望[160];

    /* A. 线程安全重入：整数纯计算 + 字符串往返（触发 ARC 分配/释放） */
    for (int i = 0; i < 同步次数; i++) {
        int64_t r = 累加(100); /* 1+..+100 = 5050 */
        if (r != 5050) { 记失败("累加(100) != 5050"); }

        snprintf(名, sizeof 名, "线程%ld_%d", id, i);
        char *s = 拼名(名);
        snprintf(期望, sizeof 期望, "你好, %s!", 名);
        if (!s || strcmp(s, 期望) != 0) { 记失败("拼名 结果错"); }
        free(s); /* C 拥有返回串，free 释放（Qi 侧 ARC 已在包装内平衡） */
    }

    /* B. qi_await 阻塞桥：C 调 Qi 异步函数，同步拿到结果 */
    for (int i = 0; i < 异步次数; i++) {
        int64_t a = 异步计算((int64_t)i); /* 期望 i*2 */
        if (a != (int64_t)i * 2) { 记失败("异步计算 结果错"); }

        snprintf(名, sizeof 名, "异步%ld_%d", id, i);
        char *g = 异步问候(名);
        snprintf(期望, sizeof 期望, "异步你好, %s", 名);
        if (!g || strcmp(g, 期望) != 0) { 记失败("异步问候 结果错"); }
        free(g);
    }
    return NULL;
}

int main(void) {
    pthread_t t[线程数];
    printf("启动 %d 个外部线程：每线程 同步%d 次 + 异步%d 次...\n",
           线程数, 同步次数, 异步次数);

    for (long i = 0; i < 线程数; i++) {
        if (pthread_create(&t[i], NULL, 工作, (void *)i) != 0) {
            fprintf(stderr, "pthread_create 失败 @ %ld\n", i);
            return 2;
        }
    }
    for (int i = 0; i < 线程数; i++) {
        pthread_join(t[i], NULL);
    }

    if (失败) {
        printf("❌ 失败：有结果不匹配\n");
        return 1;
    }
    long 总调用 = (long)线程数 * (同步次数 * 2L + 异步次数 * 2L);
    printf("✅ 成功：%d 线程并发，共 %ld 次导出调用，整数+字符串+异步 结果全对，零崩溃\n",
           线程数, 总调用);
    return 0;
}
