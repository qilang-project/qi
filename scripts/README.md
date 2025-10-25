# Scripts Directory

This directory contains scripts for running Qi examples across different platforms.

## Available Scripts

### Unix/Linux/macOS - `run_examples.sh`
Bash script to run all Qi files in the examples directory.

```bash
# Make executable (first time only)
chmod +x run_examples.sh

# Run from scripts directory
./run_examples.sh

# Or run from project root
./scripts/run_examples.sh
```

### Windows - `run_examples.ps1`
PowerShell script to run all Qi files in the examples directory.

```powershell
# Run from scripts directory
.\run_examples.ps1

# Or run from project root
.\scripts\run_examples.ps1

# If execution policy prevents running, you may need to run:
Set-ExecutionPolicy -ExecutionPolicy RemoteSigned -Scope CurrentUser
```

## Features

Both scripts provide:
- **Bilingual output** (Chinese/English) for better understanding
- **Colored output** to distinguish success/failure status
- **Progress tracking** showing current file number and total
- **Error handling** with proper exit codes
- **Summary statistics** showing success/failure counts

## Requirements

- **Cargo** and **Rust** must be installed and available in PATH
- **Git** (for relative path calculations)
- Scripts must be run from within the Qi project directory structure

## Output

The scripts will:
1. Find all `.qi` files in the `examples/` directory and subdirectories
2. Run each file using `cargo run -- run <relative_path>`
3. Display success/failure status for each file
4. Show a final summary with total counts

Example output:
```
========================================
运行所有Qi示例文件 | Running all Qi examples
========================================
找到 15 个Qi文件 | Found 15 Qi files

========================================
[1/15] 运行 | Running: examples/basic/hello_world/hello_world.qi
================================--------
...
✓ 成功 | Success: examples/basic/hello_world/hello_world.qi

========================================
运行总结 | Run Summary
========================================
成功 | Success: 15
失败 | Failed: 0
总计 | Total: 15
所有文件运行成功！| All files ran successfully!
```