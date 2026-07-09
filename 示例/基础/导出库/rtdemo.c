#include <stdio.h>
#include <stdint.h>
#include "lib运行时演示.h"
int main(void){
    int64_t total = qi_runtime_demo(8);
    printf("[C] qi_runtime_demo(8) = %lld (期望 8000)\n", (long long)total);
    return total == 8000 ? 0 : 1;
}
