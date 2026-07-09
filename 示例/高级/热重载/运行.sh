#!/usr/bin/env bash
# 热重载示例一键复现：编两个插件动态库 → 运行主程序（同进程内 v1→v2 热切换）。
#
# 用法：
#   cd qi/示例/高级/热重载
#   QI_BIN=../../../target/release/qi ./运行.sh
set -euo pipefail

QI_BIN="${QI_BIN:-qi}"
cd "$(dirname "$0")"

echo "==> 编译插件 v1 → 工具v1.plugin"
"$QI_BIN" compile --库 动态 插件v1.qi -o 工具v1.plugin

echo "==> 编译插件 v2 → 工具v2.plugin"
"$QI_BIN" compile --库 动态 插件v2.qi -o 工具v2.plugin

echo "==> 运行主程序（反射自省 + 插件热重载）"
echo
"$QI_BIN" run 主程序.qi

echo
echo "==> 清理插件产物"
rm -f 工具v1.plugin 工具v2.plugin 工具v1.h 工具v2.h 插件v1.o 插件v2.o
