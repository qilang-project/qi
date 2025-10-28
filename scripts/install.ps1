# Qi Compiler Installation Script for Windows
# PowerShell script to install Qi programming language compiler

param(
    [string]$InstallDir = "$env:USERPROFILE\.qi",
    [switch]$Help = $false
)

# Colors for output
$Colors = @{
    Red = "Red"
    Green = "Green"
    Yellow = "Yellow"
    Blue = "Blue"
    White = "White"
}

function Write-ColorOutput {
    param(
        [string]$Message,
        [string]$Color = "White"
    )
    Write-Host $Message -ForegroundColor $Colors[$Color]
}

function Write-Status {
    param([string]$Message)
    Write-ColorOutput "[INFO] $Message" "Green"
}

function Write-Warning {
    param([string]$Message)
    Write-ColorOutput "[WARN] $Message" "Yellow"
}

function Write-Error {
    param([string]$Message)
    Write-ColorOutput "[ERROR] $Message" "Red"
}

# Show help
if ($Help) {
    Write-Host "Qi Compiler Installation Script for Windows"
    Write-Host ""
    Write-Host "Usage: .\install.ps1 [OPTIONS]"
    Write-Host ""
    Write-Host "Options:"
    Write-Host "  -InstallDir DIR    Installation directory (default: $env:USERPROFILE\.qi)"
    Write-Host "  -Help              Show this help message"
    Write-Host ""
    Write-Host "This script installs the Qi compiler and runtime library to the specified directory."
    exit 0
}

Write-ColorOutput "Qi Compiler Installation Script for Windows" "Blue"
Write-Status "Installing to: $InstallDir"

# Get project root directory
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$ProjectRoot = Split-Path -Parent $ScriptDir
$ReleaseDir = Join-Path $ProjectRoot "target\release"

$ExecutableName = "qi.exe"
$LibraryName = "qi_compiler.lib"

# Check if source files exist
$ExecutablePath = Join-Path $ReleaseDir $ExecutableName
$LibraryPath = Join-Path $ReleaseDir $LibraryName

if (-not (Test-Path $ExecutablePath)) {
    Write-Error "Compiler executable not found: $ExecutablePath"
    Write-Host "Please run 'cargo build --release' first"
    exit 1
}

if (-not (Test-Path $LibraryPath)) {
    Write-Error "Runtime library not found: $LibraryPath"
    Write-Host "Please run 'cargo build --release' first"
    exit 1
}

# Create installation directory
Write-Status "Creating installation directory..."
if (-not (Test-Path $InstallDir)) {
    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
}

# Copy files
Write-Status "Installing compiler executable..."
Copy-Item $ExecutablePath $InstallDir -Force

Write-Status "Installing runtime library..."
Copy-Item $LibraryPath $InstallDir -Force

# Add to PATH
try {
    $CurrentPath = [Environment]::GetEnvironmentVariable("PATH", "User")
    if ($CurrentPath -and $CurrentPath.Split(';') -contains $InstallDir) {
        Write-Status "Installation directory already in PATH"
    } else {
        Write-Status "Adding installation directory to user PATH..."
        if ($CurrentPath) {
            $NewPath = "$CurrentPath;$InstallDir"
        } else {
            $NewPath = $InstallDir
        }
        [Environment]::SetEnvironmentVariable("PATH", $NewPath, "User")
        Write-Status "PATH updated. You may need to restart your terminal or PowerShell session."
    }
} catch {
    Write-Warning "Could not automatically update PATH. Please add $InstallDir to your PATH manually."
}

# Create test script
Write-Status "Creating test script..."
$TestScript = @'
函数 入口() {
    打印("Qi 编译器安装成功！");
    打印("如果你看到这条消息，说明编译器工作正常。");
    返回 0;
}
'@

Set-Content -Path (Join-Path $InstallDir "test_installation.qi") -Value $TestScript -Encoding UTF8

# Test installation
Write-Status "Testing installation..."
Push-Location $InstallDir
try {
    $QiExecutable = Join-Path $InstallDir $ExecutableName
    & $QiExecutable run test_installation.qi
    if ($LASTEXITCODE -eq 0) {
        Write-Status "Installation test passed!"
    } else {
        Write-Warning "Installation test failed"
    }
} finally {
    Pop-Location
}

# Clean up test files
$TestFiles = @("test_installation.qi", "test_installation.exe", "test_installation.obj", "test_installation.ll")
foreach ($file in $TestFiles) {
    $FilePath = Join-Path $InstallDir $file
    if (Test-Path $FilePath) {
        Remove-Item $FilePath -Force
    }
}

# Create uninstall script
Write-Status "Creating uninstall script..."
$UninstallScript = @"
# Qi Compiler Uninstall Script
# Run this script to remove Qi compiler from your system

param(
    [switch] `$Confirm = `$false
)

`$InstallDir = "$InstallDir"

if (`$Confirm) {
    Write-Host "This will remove Qi compiler from: `$InstallDir"
    `$choice = Read-Host "Are you sure? (y/N)"
    if (`$choice -ne "y" -and `$choice -ne "Y") {
        Write-Host "Uninstallation cancelled."
        exit 0
    }
}

if (Test-Path `$InstallDir) {
    try {
        Remove-Item -Path `$InstallDir -Recurse -Force
        Write-Host "Qi compiler uninstalled successfully."

        # Remove from PATH
        `$CurrentPath = [Environment]::GetEnvironmentVariable("PATH", "User")
        if (`$CurrentPath) {
            `$NewPath = `$CurrentPath.Split(';') | Where-Object { `$_ -ne `$InstallDir } | Join-String -Separator ';'
            [Environment]::SetEnvironmentVariable("PATH", `$NewPath, "User")
            Write-Host "Removed from PATH."
        }
    } catch {
        Write-Error "Failed to uninstall: `$_"
    }
} else {
    Write-Host "Qi compiler not found at `$InstallDir"
}
"@

Set-Content -Path (Join-Path $InstallDir "uninstall.ps1") -Value $UninstallScript -Encoding UTF8

# Print installation summary
Write-Host ""
Write-ColorOutput "Installation completed successfully!" "Green"
Write-Host ""
Write-ColorOutput "Installation Summary:" "Blue"
Write-Host "  Platform: Windows"
Write-Host "  Install Directory: $InstallDir"
Write-Host "  Executable: $InstallDir\$ExecutableName"
Write-Host "  Library: $InstallDir\$LibraryName"
Write-Host ""

Write-ColorOutput "Usage:" "Blue"
Write-Host "  qi help                 - Show help"
Write-Host "  qi run <file.qi>        - Compile and run a Qi file"
Write-Host "  qi compile <file.qi>    - Compile a Qi file to executable"
Write-Host "  qi check <file.qi>      - Check syntax without compiling"
Write-Host ""

Write-Status "Installation complete! Happy coding with Qi! 🚀"
Write-Status "Uninstall script created: $InstallDir\uninstall.ps1"