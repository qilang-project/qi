#!/bin/bash

# Qi Compiler Installation Script
# Cross-platform installation script for Qi programming language compiler

set -e  # Exit on any error

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Default installation directory
DEFAULT_INSTALL_DIR="$HOME/.qi"
INSTALL_DIR="$DEFAULT_INSTALL_DIR"

# Platform detection
PLATFORM=$(uname -s)
ARCH=$(uname -m)

# File extensions and names based on platform
if [[ "$PLATFORM" == "Linux"* ]]; then
    PLATFORM_NAME="linux"
    EXECUTABLE_NAME="qi"
    LIBRARY_NAME="libqi_compiler.a"
    SHELL_CONFIG_FILE="$HOME/.bashrc"
elif [[ "$PLATFORM" == "Darwin"* ]]; then
    PLATFORM_NAME="macos"
    EXECUTABLE_NAME="qi"
    LIBRARY_NAME="libqi_compiler.a"
    SHELL_CONFIG_FILE="$HOME/.zshrc"
    # Check if .zshrc exists, if not use .bash_profile
    if [[ ! -f "$HOME/.zshrc" ]]; then
        SHELL_CONFIG_FILE="$HOME/.bash_profile"
    fi
elif [[ "$PLATFORM" == "MINGW"* ]] || [[ "$PLATFORM" == "CYGWIN"* ]] || [[ "$PLATFORM" == "MSYS"* ]]; then
    PLATFORM_NAME="windows"
    EXECUTABLE_NAME="qi.exe"
    LIBRARY_NAME="qi_compiler.lib"
    SHELL_CONFIG_FILE="$HOME/.bashrc"
else
    echo -e "${RED}Error: Unsupported platform: $PLATFORM${NC}"
    exit 1
fi

echo -e "${BLUE}Qi Compiler Installation Script${NC}"
echo -e "${BLUE}Platform: $PLATFORM_NAME ($ARCH)${NC}"
echo

# Function to print colored output
print_status() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Function to check if command exists
command_exists() {
    command -v "$1" >/dev/null 2>&1
}

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -d|--directory)
            INSTALL_DIR="$2"
            shift 2
            ;;
        -h|--help)
            echo "Usage: $0 [OPTIONS]"
            echo
            echo "Options:"
            echo "  -d, --directory DIR    Installation directory (default: $DEFAULT_INSTALL_DIR)"
            echo "  -h, --help            Show this help message"
            echo
            echo "This script installs the Qi compiler and runtime library to the specified directory."
            exit 0
            ;;
        *)
            print_error "Unknown option: $1"
            echo "Use -h or --help for usage information"
            exit 1
            ;;
    esac
done

print_status "Installing Qi compiler to: $INSTALL_DIR"

# Check if source files exist
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RELEASE_DIR="$PROJECT_ROOT/target/release"

if [[ ! -f "$RELEASE_DIR/$EXECUTABLE_NAME" ]]; then
    print_error "Compiler executable not found: $RELEASE_DIR/$EXECUTABLE_NAME"
    echo "Please run 'cargo build --release' first"
    exit 1
fi

if [[ ! -f "$RELEASE_DIR/$LIBRARY_NAME" ]]; then
    print_error "Runtime library not found: $RELEASE_DIR/$LIBRARY_NAME"
    echo "Please run 'cargo build --release' first"
    exit 1
fi

# Create installation directory
print_status "Creating installation directory..."
mkdir -p "$INSTALL_DIR"

# Copy files
print_status "Installing compiler executable..."
cp "$RELEASE_DIR/$EXECUTABLE_NAME" "$INSTALL_DIR/"
chmod +x "$INSTALL_DIR/$EXECUTABLE_NAME"

print_status "Installing runtime library..."
cp "$RELEASE_DIR/$LIBRARY_NAME" "$INSTALL_DIR/"

# Create symbolic links or add to PATH
if [[ "$PLATFORM_NAME" != "windows" ]]; then
    # Unix-like systems: create symlinks in /usr/local/bin if possible
    if [[ -w "/usr/local/bin" ]] || command_exists sudo; then
        print_status "Creating symbolic link in /usr/local/bin..."

        if [[ -w "/usr/local/bin" ]]; then
            ln -sf "$INSTALL_DIR/$EXECUTABLE_NAME" "/usr/local/bin/qi"
        else
            sudo ln -sf "$INSTALL_DIR/$EXECUTABLE_NAME" "/usr/local/bin/qi"
        fi

        print_status "Qi compiler can be used with 'qi' command from anywhere"
    else
        print_warning "Cannot create symbolic link in /usr/local/bin"
        print_status "Add $INSTALL_DIR to your PATH manually"
    fi
else
    # Windows: suggest manual PATH addition
    print_warning "On Windows, please add $INSTALL_DIR to your PATH manually"
    print_status "You can do this through System Properties > Environment Variables"
fi

# Add to shell configuration
if [[ "$PLATFORM_NAME" != "windows" ]]; then
    if ! grep -q "$INSTALL_DIR" "$SHELL_CONFIG_FILE" 2>/dev/null; then
        print_status "Adding installation directory to PATH in $SHELL_CONFIG_FILE"
        echo "" >> "$SHELL_CONFIG_FILE"
        echo "# Qi Compiler" >> "$SHELL_CONFIG_FILE"
        echo "export PATH=\"\$PATH:$INSTALL_DIR\"" >> "$SHELL_CONFIG_FILE"
        print_status "Run 'source $SHELL_CONFIG_FILE' or restart your terminal to use qi command"
    else
        print_status "Installation directory already in PATH"
    fi
fi

# Create test script
print_status "Creating test script..."
cat > "$INSTALL_DIR/test_installation.qi" << 'EOF'
函数 入口() {
    打印("Qi 编译器安装成功！");
    打印("如果你看到这条消息，说明编译器工作正常。");
    返回 0;
}
EOF

# Test installation
print_status "Testing installation..."
cd "$INSTALL_DIR"

if [[ "$PLATFORM_NAME" == "windows" ]]; then
    ./"$EXECUTABLE_NAME" run test_installation.qi
else
    ./"$EXECUTABLE_NAME" run test_installation.qi
fi

# Clean up test file
rm -f "$INSTALL_DIR/test_installation.qi" "$INSTALL_DIR/test_installation" "$INSTALL_DIR/test_installation.o" "$INSTALL_DIR/test_installation.ll"

# Print installation summary
echo
echo -e "${GREEN}Installation completed successfully!${NC}"
echo
echo -e "${BLUE}Installation Summary:${NC}"
echo "  Platform: $PLATFORM_NAME ($ARCH)"
echo "  Install Directory: $INSTALL_DIR"
echo "  Executable: $INSTALL_DIR/$EXECUTABLE_NAME"
echo "  Library: $INSTALL_DIR/$LIBRARY_NAME"
echo

if [[ "$PLATFORM_NAME" != "windows" ]]; then
    if [[ -L "/usr/local/bin/qi" ]]; then
        echo -e "${GREEN}✓${NC} Qi compiler is available as 'qi' command"
    else
        echo -e "${YELLOW}⚠${NC} Add $INSTALL_DIR to your PATH to use 'qi' command"
    fi
fi

echo
echo -e "${BLUE}Usage:${NC}"
echo "  qi help                 - Show help"
echo "  qi run <file.qi>        - Compile and run a Qi file"
echo "  qi compile <file.qi>    - Compile a Qi file to executable"
echo "  qi check <file.qi>      - Check syntax without compiling"
echo

print_status "Installation complete! Happy coding with Qi! 🚀"