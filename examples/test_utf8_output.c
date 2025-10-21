#include <stdio.h>
#include <string.h>

int main() {
    // Test direct UTF-8 string literal
    printf("Direct: 你好，世界！\n");

    // Test with UTF-8 bytes
    char utf8_bytes[] = {0xe4, 0xbd, 0xa0, 0xe5, 0xa5, 0xbd, 0xef, 0xbc, 0x8c, 0xe4, 0xb8, 0x96, 0xe7, 0x95, 0x8c, 0xef, 0xbc, 0x81, 0x0a, 0x00};
    printf("Bytes: %s", utf8_bytes);

    // Test with puts (adds newline automatically)
    puts("Puts: 你好，世界！");

    // Test with fwrite
    char* str = "Fwrite: 你好，世界！\n";
    fwrite(str, 1, strlen(str), stdout);

    return 0;
}