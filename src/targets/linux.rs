//! Linux target implementation

use super::{Target, TargetError};

/// Linux target implementation
pub struct LinuxTarget {
    target_triple: String,
    cpu_features: Vec<&'static str>,
    linker_flags: Vec<&'static str>,
}

impl LinuxTarget {
    pub fn new() -> Self {
        Self {
            target_triple: "x86_64-unknown-linux-gnu".to_string(),
            cpu_features: vec![
                "sse2", "sse4.1", "sse4.2", "avx", "avx2"
            ],
            linker_flags: vec![
                "-no-pie", "-dynamic-linker", "/lib64/ld-linux-x86-64.so.2"
            ],
        }
    }
}

impl Target for LinuxTarget {
    fn target_triple(&self) -> &str {
        &self.target_triple
    }

    fn cpu_features(&self) -> &[&str] {
        &self.cpu_features
    }

    fn linker_flags(&self) -> &[&str] {
        &self.linker_flags
    }

    fn generate_runtime(&self) -> Result<String, TargetError> {
        let runtime_code = r#"
// Linux Runtime for Qi Language
// Qi语言Linux运行时库

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <sys/mman.h>
#include <stdarg.h>
#include <math.h>

// Memory management
// 内存管理
extern void* qi_malloc(size_t size) {
    void* ptr = malloc(size);
    if (!ptr) {
        fprintf(stderr, "内存分配失败\n");
        exit(1);
    }
    return ptr;
}

extern void qi_free(void* ptr) {
    if (ptr) {
        free(ptr);
    }
}

extern void* qi_realloc(void* ptr, size_t new_size) {
    void* new_ptr = realloc(ptr, new_size);
    if (!new_ptr && new_size > 0) {
        fprintf(stderr, "内存重新分配失败\n");
        exit(1);
    }
    return new_ptr;
}

// String operations
// 字符串操作
extern size_t qi_strlen(const char* s) {
    return strlen(s);
}

extern char* qi_strcpy(char* dest, const char* src) {
    return strcpy(dest, src);
}

extern char* qi_strdup(const char* s) {
    return strdup(s);
}

extern int qi_strcmp(const char* s1, const char* s2) {
    return strcmp(s1, s2);
}

// I/O operations
// 输入输出操作
extern void qi_print(const char* s) {
    printf("%s", s);
}

extern void qi_println(const char* s) {
    printf("%s\n", s);
}

extern void qi_print_int(long long n) {
    printf("%lld", n);
}

extern void qi_print_intln(long long n) {
    printf("%lld\n", n);
}

extern void qi_print_float(double d) {
    printf("%.6g", d);
}

extern void qi_print_floatln(double d) {
    printf("%.6g\n", d);
}

extern char* qi_read_line() {
    size_t capacity = 256;
    size_t size = 0;
    char* buffer = malloc(capacity);

    if (!buffer) {
        return NULL;
    }

    int c;
    while ((c = getchar()) != EOF && c != '\n') {
        if (size + 1 >= capacity) {
            capacity *= 2;
            char* new_buffer = realloc(buffer, capacity);
            if (!new_buffer) {
                free(buffer);
                return NULL;
            }
            buffer = new_buffer;
        }
        buffer[size++] = (char)c;
    }

    buffer[size] = '\0';
    return buffer;
}

// Math operations
// 数学运算
extern double qi_sqrt(double x) {
    return sqrt(x);
}

extern double qi_pow(double x, double y) {
    return pow(x, y);
}

extern double qi_sin(double x) {
    return sin(x);
}

extern double qi_cos(double x) {
    return cos(x);
}

extern double qi_tan(double x) {
    return tan(x);
}

extern long long qi_abs(long long x) {
    return llabs(x);
}

// System operations
// 系统操作
extern void qi_exit(int code) {
    exit(code);
}

extern int qi_getpid() {
    return getpid();
}

extern char* qi_getenv(const char* name) {
    return getenv(name);
}

extern int qi_system(const char* command) {
    return system(command);
}

// File operations
// 文件操作
extern FILE* qi_fopen(const char* filename, const char* mode) {
    return fopen(filename, mode);
}

extern int qi_fclose(FILE* fp) {
    return fclose(fp);
}

extern size_t qi_fread(void* ptr, size_t size, size_t nmemb, FILE* fp) {
    return fread(ptr, size, nmemb, fp);
}

extern size_t qi_fwrite(const void* ptr, size_t size, size_t nmemb, FILE* fp) {
    return fwrite(ptr, size, nmemb, fp);
}

extern char* qi_fgets(char* s, int size, FILE* fp) {
    return fgets(s, size, fp);
}

extern int qi_fprintf(FILE* fp, const char* format, ...) {
    va_list args;
    va_start(args, format);
    int result = vfprintf(fp, format, args);
    va_end(args);
    return result;
}

// Error handling
// 错误处理
extern void qi_perror(const char* s) {
    perror(s);
}

extern void qi_error(const char* message) {
    fprintf(stderr, "错误: %s\n", message);
}

// Linux-specific features
// Linux特定功能
extern int qi_mprotect(void* addr, size_t len, int prot) {
    return mprotect(addr, len, prot);
}

extern void* qi_mmap(void* addr, size_t length, int prot, int flags, int fd, off_t offset) {
    return mmap(addr, length, prot, flags, fd, offset);
}

extern int qi_munmap(void* addr, size_t length) {
    return munmap(addr, length);
}

// Time functions
// 时间函数

extern long long qi_time() {
    return (long long)time(NULL);
}

extern void qi_sleep(unsigned int seconds) {
    sleep(seconds);
}

extern void qi_usleep(unsigned int useconds) {
    usleep(useconds);
}

// Linux signal handling
// Linux信号处理
#include <signal.h>

extern void qi_signal(int signum, void (*handler)(int)) {
    signal(signum, handler);
}

extern int qi_raise(int signum) {
    return raise(signum);
}

// Linux process functions
// Linux进程函数
#include <sys/types.h>
#include <sys/wait.h>

extern pid_t qi_fork() {
    return fork();
}

extern int qi_waitpid(pid_t pid, int* status, int options) {
    return waitpid(pid, status, options);
}

extern pid_t qi_getppid() {
    return getppid();
}

// Linux shared memory
// Linux共享内存
#include <sys/shm.h>

extern int qi_shmget(key_t key, size_t size, int shmflg) {
    return shmget(key, size, shmflg);
}

extern void* qi_shmat(int shmid, const void* shmaddr, int shmflg) {
    return shmat(shmid, shmaddr, shmflg);
}

extern int qi_shmdt(const void* shmaddr) {
    return shmdt(shmaddr);
}

extern int qi_shmctl(int shmid, int cmd, struct shmid_ds* buf) {
    return shmctl(shmid, cmd, buf);
}
"#;

        Ok(runtime_code.to_string())
    }
}

impl Default for LinuxTarget {
    fn default() -> Self {
        Self::new()
    }
}