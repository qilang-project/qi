#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "lib数学库.h"
int main(void){
    long bad = 0;
    for (long i = 0; i < 1000000; i++) {
        char *g = 问候("世界");
        if (!g || strcmp(g, "你好, 世界") != 0) bad++;
        free(g);
    }
    printf("100万次 问候 往返完成，错误=%ld\n", bad);
    return bad ? 1 : 0;
}
