#!/bin/bash

echo "Testing first 20 examples..."

find 示例 -name "*.qi" -type f | head -20 | while IFS= read -r file; do
    echo "Testing: $file"
    if timeout 10s cargo run -- run "$file" >/dev/null 2>&1; then
        echo "✓ PASSED"
    else
        echo "✗ FAILED"
        timeout 5s cargo run -- run "$file" 2>&1 | head -3
    fi
    echo "---"
done