# run_examples.ps1 - 运行examples目录下所有Qi文件的PowerShell脚本
# PowerShell script to run all Qi files in the examples directory

# 设置错误处理
$ErrorActionPreference = "Stop"

# 颜色定义
$Colors = @{
    Red = "Red"
    Green = "Green"
    Yellow = "Yellow"
    Blue = "Blue"
    White = "White"
}

# 打印标题函数
function Write-ColorOutput {
    param(
        [string]$Message,
        [string]$Color = "White"
    )
    Write-Host $Message -ForegroundColor $Colors[$Color]
}

# 打印分隔线
function Write-Separator {
    Write-ColorOutput "========================================" "Blue"
}

# 主程序开始
Write-Separator
Write-ColorOutput "运行所有Qi示例文件 | Running all Qi examples" "Blue"
Write-Separator

# 获取脚本所在目录的父目录（项目根目录）
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$ProjectRoot = Split-Path -Parent $ScriptDir
$ExamplesDir = Join-Path $ProjectRoot "examples"

# 检查examples目录是否存在
if (-not (Test-Path $ExamplesDir)) {
    Write-ColorOutput "错误: examples目录不存在 | Error: examples directory not found" "Red"
    exit 1
}

# 查找所有.qi文件
Write-ColorOutput "正在查找Qi文件... | Finding Qi files..." "Yellow"
$qiFiles = Get-ChildItem -Path $ExamplesDir -Filter "*.qi" -Recurse -File | Sort-Object FullName

if ($qiFiles.Count -eq 0) {
    Write-ColorOutput "未找到任何.qi文件 | No .qi files found" "Red"
    exit 1
}

# 计算文件数量
$fileCount = $qiFiles.Count
Write-ColorOutput "找到 $fileCount 个Qi文件 | Found $fileCount Qi files" "Green"
Write-Host ""

# 计数器
$current = 0
$successCount = 0
$errorCount = 0

# 遍历并运行每个Qi文件
foreach ($qiFile in $qiFiles) {
    $current++

    # 获取相对于项目根目录的路径
    $relativePath = $qiFile.FullName.Replace($ProjectRoot, "").TrimStart("\", "/").Replace("\", "/")

    Write-Separator
    Write-ColorOutput "[$current/$fileCount] 运行 | Running: $relativePath" "Blue"
    Write-Separator

    try {
        # 切换到项目根目录并运行（默认启用详细输出）
        Push-Location $ProjectRoot
        $result = cargo run -- -v run $relativePath

        if ($LASTEXITCODE -eq 0) {
            Write-ColorOutput "✓ 成功 | Success: $relativePath" "Green"
            $successCount++
        } else {
            Write-ColorOutput "✗ 失败 | Failed: $relativePath (Exit code: $LASTEXITCODE)" "Red"
            $errorCount++
        }
    }
    catch {
        Write-ColorOutput "✗ 异常 | Exception: $relativePath - $($_.Exception.Message)" "Red"
        $errorCount++
    }
    finally {
        Pop-Location
    }

    Write-Host ""
    Write-Separator
    Write-Host ""
}

# 打印总结
Write-Separator
Write-ColorOutput "运行总结 | Run Summary" "Blue"
Write-Separator
Write-ColorOutput "成功 | Success: $successCount" "Green"
Write-ColorOutput "失败 | Failed: $errorCount" "Red"
Write-ColorOutput "总计 | Total: $fileCount" "Yellow"

if ($errorCount -eq 0) {
    Write-ColorOutput "所有文件运行成功！| All files ran successfully!" "Green"
    exit 0
} else {
    Write-ColorOutput "有 $errorCount 个文件运行失败。| $errorCount files failed to run." "Red"
    exit 1
}