//! Build script for Qi compiler runtime library and LALRPOP grammar

use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=runtime/");
    println!("cargo:rerun-if-changed=src/parser/");

    // Process LALRPOP grammar
    // Note: This may report shift/reduce conflicts which are benign
    // See grammar.lalrpop for documentation of expected conflicts
    match lalrpop::process_root() {
        Ok(_) => eprintln!("✓ LALRPOP grammar processed successfully"),
        Err(e) => {
            eprintln!("✗ LALRPOP processing failed!");
            eprintln!("Error details: {:#?}", e);
            eprintln!("\nNote: Shift/reduce conflicts are expected in this grammar.");
            eprintln!("See comments in grammar.lalrpop for details.");
            panic!("LALRPOP failed to generate parser");
        }
    }

    // Build the C runtime library
    build_runtime_library();

    // Link against the runtime library
    link_runtime_library();
}


fn build_runtime_library() {
    let runtime_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("runtime");

    if !runtime_dir.exists() {
        println!("Runtime directory not found, creating it...");
        std::fs::create_dir_all(&runtime_dir).expect("Failed to create runtime directory");
    }

    // Create basic runtime files if they don't exist
    create_runtime_files(&runtime_dir);

    // Try to build with CMake first, fallback to Make
    let cmake_file = runtime_dir.join("CMakeLists.txt");
    if cmake_file.exists() {
        build_with_cmake(&runtime_dir);
    } else {
        build_with_make(&runtime_dir);
    }
}

fn create_runtime_files(runtime_dir: &PathBuf) {
    let include_dir = runtime_dir.join("include");
    let src_dir = runtime_dir.join("src");

    std::fs::create_dir_all(&include_dir).ok();
    std::fs::create_dir_all(&src_dir).ok();

    // Create basic header files
    let headers = vec![
        ("qi_runtime.h", include_runtime_h()),
        ("qi_memory.h", include_memory_h()),
        ("qi_strings.h", include_strings_h()),
        ("qi_errors.h", include_errors_h()),
    ];

    for (filename, content) in headers {
        let header_path = include_dir.join(filename);
        if !header_path.exists() {
            std::fs::write(header_path, content).expect("Failed to write header file");
        }
    }

    // Create basic source files
    let sources = vec![
        ("memory.c", source_memory_c()),
        ("strings.c", source_strings_c()),
        ("errors.c", source_errors_c()),
        ("platform.c", source_platform_c()),
    ];

    for (filename, content) in sources {
        let source_path = src_dir.join(filename);
        if !source_path.exists() {
            std::fs::write(source_path, content).expect("Failed to write source file");
        }
    }

    // Create CMakeLists.txt
    let cmake_path = runtime_dir.join("CMakeLists.txt");
    if !cmake_path.exists() {
        std::fs::write(cmake_path, cmake_content()).expect("Failed to write CMakeLists.txt");
    }

    // Create Makefile
    let makefile_path = runtime_dir.join("Makefile");
    if !makefile_path.exists() {
        std::fs::write(makefile_path, makefile_content()).expect("Failed to write Makefile");
    }
}

fn build_with_cmake(runtime_dir: &PathBuf) {
    let build_dir = runtime_dir.join("build");
    std::fs::create_dir_all(&build_dir).ok();

    let output = Command::new("cmake")
        .args(&["..", "-DCMAKE_BUILD_TYPE=Release"])
        .current_dir(&build_dir)
        .output();

    match output {
        Ok(output) if output.status.success() => {
            let output = Command::new("cmake")
                .args(&["--build", "."])
                .current_dir(&build_dir)
                .output()
                .expect("Failed to run cmake build");

            if !output.status.success() {
                eprintln!("CMake build failed: {}", String::from_utf8_lossy(&output.stderr));
            }
        }
        _ => {
            println!("CMake not available, trying Make...");
            build_with_make(runtime_dir);
        }
    }
}

fn build_with_make(runtime_dir: &PathBuf) {
    let output = Command::new("make")
        .arg("-C")
        .arg(runtime_dir)
        .arg("-j")
        .output();

    match output {
        Ok(output) if output.status.success() => {
            println!("Runtime library built successfully with Make");
        }
        Ok(output) => {
            eprintln!("Make build failed: {}", String::from_utf8_lossy(&output.stderr));
        }
        Err(e) => {
            println!("Make not available: {}. Runtime library will not be built.", e);
        }
    }
}

fn link_runtime_library() {
    let runtime_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("runtime");

    // Look for built library files
    let lib_paths = vec![
        runtime_dir.join("build/libqi_runtime.a"),
        runtime_dir.join("libqi_runtime.a"),
        runtime_dir.join("libqi_runtime.so"),
        runtime_dir.join("libqi_runtime.dylib"),
    ];

    for lib_path in lib_paths {
        if lib_path.exists() {
            println!("cargo:rustc-link-lib=static=qi_runtime");
            println!("cargo:rustc-link-search=native={}", lib_path.parent().unwrap().display());
            return;
        }
    }

    println!("Runtime library not found, continuing without it...");
}

// Basic file contents
fn include_runtime_h() -> String {
    r#"#ifndef QI_RUNTIME_H
#define QI_RUNTIME_H

#include <stdint.h>
#include <stddef.h>

// Runtime initialization
void qi_runtime_init(void);
void qi_runtime_cleanup(void);

// Version information
#define QI_RUNTIME_VERSION_MAJOR 0
#define QI_RUNTIME_VERSION_MINOR 1
#define QI_RUNTIME_VERSION_PATCH 0

#endif // QI_RUNTIME_H
"#.to_string()
}

fn include_memory_h() -> String {
    r#"#ifndef QI_MEMORY_H
#define QI_MEMORY_H

#include <stddef.h>

// Memory allocation
void* qi_malloc(size_t size);
void* qi_realloc(void* ptr, size_t size);
void qi_free(void* ptr);

// Memory management utilities
size_t qi_get_allocated_memory(void);
void qi_reset_memory_stats(void);

#endif // QI_MEMORY_H
"#.to_string()
}

fn include_strings_h() -> String {
    r#"#ifndef QI_STRINGS_H
#define QI_STRINGS_H

#include <stddef.h>

// String operations
size_t qi_strlen(const char* str);
char* qi_strcpy(char* dest, const char* src);
char* qi_strdup(const char* str);
int qi_strcmp(const char* str1, const char* str2);

// Unicode string operations
size_t qi_utf8_strlen(const char* str);
int qi_utf8_validate(const char* str);

#endif // QI_STRINGS_H
"#.to_string()
}

fn include_errors_h() -> String {
    r#"#ifndef QI_ERRORS_H
#define QI_ERRORS_H

#include <stdarg.h>

// Error codes
typedef enum {
    QI_ERROR_NONE = 0,
    QI_ERROR_OUT_OF_MEMORY,
    QI_ERROR_INVALID_ARGUMENT,
    QI_ERROR_DIVISION_BY_ZERO,
    QI_ERROR_INDEX_OUT_OF_BOUNDS,
    QI_ERROR_STACK_OVERFLOW,
    QI_ERROR_UNDEFINED,
} qi_error_code_t;

// Error handling
void qi_set_error(qi_error_code_t code, const char* message);
qi_error_code_t qi_get_last_error(void);
const char* qi_get_error_message(void);

// Panic handling
void qi_panic(const char* message) __attribute__((noreturn));

#endif // QI_ERRORS_H
"#.to_string()
}

fn source_memory_c() -> String {
    r#"#include "qi_memory.h"
#include <stdlib.h>
#include <string.h>

static size_t total_allocated = 0;

void* qi_malloc(size_t size) {
    void* ptr = malloc(size);
    if (ptr) {
        total_allocated += size;
    }
    return ptr;
}

void* qi_realloc(void* ptr, size_t size) {
    // Note: This is a simplified implementation
    // A real implementation would track the old size
    void* new_ptr = realloc(ptr, size);
    return new_ptr;
}

void qi_free(void* ptr) {
    free(ptr);
}

size_t qi_get_allocated_memory(void) {
    return total_allocated;
}

void qi_reset_memory_stats(void) {
    total_allocated = 0;
}
"#.to_string()
}

fn source_strings_c() -> String {
    r#"#include "qi_strings.h"
#include <stdlib.h>
#include <string.h>

size_t qi_strlen(const char* str) {
    return strlen(str);
}

char* qi_strcpy(char* dest, const char* src) {
    return strcpy(dest, src);
}

char* qi_strdup(const char* str) {
    return strdup(str);
}

int qi_strcmp(const char* str1, const char* str2) {
    return strcmp(str1, str2);
}

size_t qi_utf8_strlen(const char* str) {
    // Simplified UTF-8 length calculation
    // A real implementation would handle multi-byte sequences properly
    return strlen(str);
}

int qi_utf8_validate(const char* str) {
    // Simplified UTF-8 validation
    // A real implementation would check for valid UTF-8 sequences
    return str != NULL;
}
"#.to_string()
}

fn source_errors_c() -> String {
    r#"#include "qi_errors.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static qi_error_code_t last_error = QI_ERROR_NONE;
static char error_message[256] = {0};

void qi_set_error(qi_error_code_t code, const char* message) {
    last_error = code;
    if (message) {
        strncpy(error_message, message, sizeof(error_message) - 1);
        error_message[sizeof(error_message) - 1] = '\0';
    } else {
        error_message[0] = '\0';
    }
}

qi_error_code_t qi_get_last_error(void) {
    return last_error;
}

const char* qi_get_error_message(void) {
    return error_message;
}

void qi_panic(const char* message) {
    if (message) {
        fprintf(stderr, "Qi runtime panic: %s\n", message);
    } else {
        fprintf(stderr, "Qi runtime panic: Unknown error\n");
    }
    abort();
}
"#.to_string()
}

fn source_platform_c() -> String {
    r#"#include "qi_runtime.h"
#include <stdio.h>

void qi_runtime_init(void) {
    // Platform-specific initialization
    printf("Qi runtime initialized\n");
}

void qi_runtime_cleanup(void) {
    // Platform-specific cleanup
    printf("Qi runtime cleaned up\n");
}
"#.to_string()
}

fn cmake_content() -> String {
    r#"cmake_minimum_required(VERSION 3.10)
project(qi_runtime VERSION 0.1.0 LANGUAGES C)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# Include directories
include_directories(include)

# Source files
set(RUNTIME_SOURCES
    src/memory.c
    src/strings.c
    src/errors.c
    src/platform.c
)

# Create static library
add_library(qi_runtime STATIC ${RUNTIME_SOURCES})

# Set output directory
set_target_properties(qi_runtime PROPERTIES
    ARCHIVE_OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR}
)

# Installation
install(TARGETS qi_runtime
    ARCHIVE DESTINATION lib
    LIBRARY DESTINATION lib
)

install(DIRECTORY include/ DESTINATION include)
"#.to_string()
}

fn makefile_content() -> String {
    r#"CC = gcc
CFLAGS = -std=c11 -Wall -Wextra -O2 -fPIC
AR = ar
ARFLAGS = rcs

# Directories
INCLUDE_DIR = include
SRC_DIR = src
BUILD_DIR = build

# Source files
SOURCES = $(wildcard $(SRC_DIR)/*.c)
OBJECTS = $(SOURCES:$(SRC_DIR)/%.c=$(BUILD_DIR)/%.o)
TARGET = libqi_runtime.a

.PHONY: all clean install

all: $(TARGET)

$(TARGET): $(OBJECTS)
	$(AR) $(ARFLAGS) $@ $^

$(BUILD_DIR)/%.o: $(SRC_DIR)/%.c | $(BUILD_DIR)
	$(CC) $(CFLAGS) -I$(INCLUDE_DIR) -c $< -o $@

$(BUILD_DIR):
	mkdir -p $(BUILD_DIR)

clean:
	rm -rf $(BUILD_DIR) $(TARGET)

install: $(TARGET)
	install -d $(DESTDIR)/lib
	install -d $(DESTDIR)/include/qi
	install -m 644 $(TARGET) $(DESTDIR)/lib/
	install -m 644 $(INCLUDE_DIR)/*.h $(DESTDIR)/include/qi/
"#.to_string()
}