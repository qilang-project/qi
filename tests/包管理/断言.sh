#!/usr/bin/env bash
# `qi 包` 包管理客户端断言 —— 对着**真的一个服务端**跑完整链路。
#
# 覆盖 docs/包管理设计.md 的客户端侧契约：
#   ① 发布：打包 → PUT → 服务端收到，且两侧 sha256 逐字节一致
#   ② 打包排除：tests/ target/ .git/ *.o 不进包体（发布减肥，本地开发不受影响）
#   ③ 安装：读 [依赖] → 下载校验 → 落到 ./qi_packages/<名称>/ → 写 qi.lock（含 sha256）
#   ④ **装完真能用**：另一个工程 导入 试验包 后 qi run 跑通（全链路的意义所在）
#   ⑤ 版本不可变：重复发布同版本 → 409 且报的是人话「版本号不可复用」
#   ⑥ 坏 token → 401 人话
#   ⑦ 篡改 qi.lock 的 sha256 → 安装当场拒绝，不静默装个别的东西
#   ⑧ 添加：改写 qi.toml [依赖] 后立即安装
#   ⑨ 搜索：按名称/描述过滤，输出里有 试验包
#   ⑩ 幂等：连着装两次，第二次显示「已是 0.1.0」而不是重下一遍
#
# 用法：qi/tests/包管理/断言.sh [qi二进制路径]
#   默认用 workspace 的 target/debug/qi。qi run 需要 QI_RUNTIME_LIB 指向 libqi_runtime.a。
# 注意 macOS bash 3.2：shell 变量名一律 ASCII。
set -uo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
QI="${1:-$ROOT/target/debug/qi}"
case "$QI" in
    /*) ;;
    *) QI="$(cd "$(dirname "$QI")" && pwd)/$(basename "$QI")" ;;
esac

# 工作目录放在**仓库内**而不是 /tmp：编译器解析依赖时会扫源文件的每一级祖先
# 目录找 qi.toml，/tmp 下任何残留包会静默劫持解析（见仓库 CLAUDE.md 的踩坑记录）。
WORK="$HERE/临时"
TOKEN="qi-test-token-3076"
BAD_TOKEN="wrong-token"

# macOS 没有 GNU timeout（CI 的 macos runner 上也没有），没有就退化成不限时跑
if command -v timeout >/dev/null 2>&1; then
    run_limited() { timeout 120 "$@"; }
elif command -v gtimeout >/dev/null 2>&1; then
    run_limited() { gtimeout 120 "$@"; }
else
    run_limited() { "$@"; }
fi

total=0
passed=0
failed=0

pass() { echo "PASS $1"; passed=$((passed+1)); total=$((total+1)); }
fail() {
    echo "FAIL $1"
    if [ $# -gt 1 ]; then echo "     $2"; fi
    failed=$((failed+1)); total=$((total+1))
}

SERVER_PID=""
cleanup() {
    if [ -n "$SERVER_PID" ]; then kill "$SERVER_PID" 2>/dev/null; fi
    rm -rf "$WORK"
}
trap cleanup EXIT

if [ ! -x "$QI" ]; then
    echo "FAIL 找不到 qi 可执行文件: $QI"
    exit 1
fi

rm -rf "$WORK"
mkdir -p "$WORK" || exit 1

# ─────────────────────── 起假注册中心 ───────────────────────
PORT_FILE="$WORK/端口"
STATE_FILE="$WORK/库存.json"
python3 "$HERE/假注册中心.py" --port-file "$PORT_FILE" --state-file "$STATE_FILE" \
    --token "$TOKEN" >"$WORK/服务端.log" 2>&1 &
SERVER_PID=$!

for _ in $(seq 1 100); do
    [ -s "$PORT_FILE" ] && break
    sleep 0.1
done
if [ ! -s "$PORT_FILE" ]; then
    echo "FAIL 假注册中心没起来"
    cat "$WORK/服务端.log"
    exit 1
fi
PORT="$(cat "$PORT_FILE")"
export QI_REGISTRY="http://127.0.0.1:$PORT"
echo "假注册中心: $QI_REGISTRY"

# 从库存 json 里取 sha256（只用标准库，不引 jq）
state_sha() {
    python3 -c '
import json,sys
data=json.load(open(sys.argv[1],encoding="utf-8"))
print(data.get("包",{}).get(sys.argv[2],{}).get(sys.argv[3],{}).get("sha256",""))
' "$STATE_FILE" "$1" "$2"
}

# ─────────────────────── 造一个待发布的小包 ───────────────────────
PKG="$WORK/源码/试验包"
mkdir -p "$PKG/target" "$PKG/tests" "$PKG/.git"
cat > "$PKG/qi.toml" <<'EOF'
[包]
名称 = "试验包"
版本 = "0.1.0"
入口 = "试验包.qi"
EOF
cat > "$PKG/试验包.qi" <<'EOF'
包 试验包;

公开 函数 打招呼(名字: 字符串): 字符串 {
    返回 "你好，" + 名字;
}

公开 函数 翻倍(x: 整数): 整数 {
    返回 x * 2;
}
EOF
# 这三样按协议必须被排除，故意放进去当诱饵
echo "构建产物" > "$PKG/target/垃圾"
echo "测试文件" > "$PKG/tests/试验包_测.qi"
echo "ref: refs/heads/main" > "$PKG/.git/HEAD"
echo "目标文件" > "$PKG/顺手.o"

# ─────────────────────── 01 发布 ───────────────────────
OUT="$WORK/01.log"
( cd "$PKG" && QI_REGISTRY_TOKEN="$TOKEN" run_limited "$QI" 包 发布 ) >"$OUT" 2>&1
RC=$?
if [ $RC -eq 0 ] && grep -q "已发布 试验包 0.1.0" "$OUT"; then
    pass "01 qi 包 发布 成功"
else
    fail "01 qi 包 发布 成功" "rc=$RC $(tr '\n' ' ' < "$OUT")"
fi

CLIENT_SHA="$(sed -n 's/.*sha256 \([0-9a-f]\{64\}\).*/\1/p' "$OUT" | head -1)"
SERVER_SHA="$(state_sha 试验包 0.1.0)"
if [ -n "$SERVER_SHA" ] && [ "$CLIENT_SHA" = "$SERVER_SHA" ]; then
    pass "02 服务端收到包体且两侧 sha256 一致"
else
    fail "02 服务端收到包体且两侧 sha256 一致" "客户端=$CLIENT_SHA 服务端=$SERVER_SHA"
fi

# 上行必须是 base64(tar.gz)：注册中心是 qi-web 写的，qi-runtime 收请求体时会把
# 内嵌 0x00 换成空格，裸二进制上行会长度不变地悄悄损坏。退回裸字节的话这里会挂。
UP_OK="$(python3 -c '
import json,sys
up=json.load(open(sys.argv[1],encoding="utf-8")).get("上行",{})
# base64 膨胀约 4/3，编码长度必须明显大于解码长度
print("yes" if up.get("was_base64") and up.get("encoded_len",0) > up.get("decoded_len",0) else "no")
' "$STATE_FILE")"
if [ "$UP_OK" = "yes" ]; then
    pass "02b 发布 body 是 base64(tar.gz) 而非裸字节"
else
    fail "02b 发布 body 是 base64(tar.gz) 而非裸字节" "$(cat "$STATE_FILE")"
fi

# ─────────────────────── 03 重复发布 → 409 人话 ───────────────────────
OUT="$WORK/03.log"
( cd "$PKG" && QI_REGISTRY_TOKEN="$TOKEN" run_limited "$QI" 包 发布 ) >"$OUT" 2>&1
RC=$?
if [ $RC -ne 0 ] && grep -q "版本号不可复用" "$OUT"; then
    pass "03 重复发布同版本 → 409 报人话"
else
    fail "03 重复发布同版本 → 409 报人话" "rc=$RC $(tr '\n' ' ' < "$OUT")"
fi

# ─────────────────────── 04 坏 token → 401 人话 ───────────────────────
OUT="$WORK/04.log"
sed -i.bak 's/版本 = "0.1.0"/版本 = "0.9.0"/' "$PKG/qi.toml" && rm -f "$PKG/qi.toml.bak"
( cd "$PKG" && QI_REGISTRY_TOKEN="$BAD_TOKEN" run_limited "$QI" 包 发布 ) >"$OUT" 2>&1
RC=$?
if [ $RC -ne 0 ] && grep -q "token 无效或无权限" "$OUT" && grep -q "QI_REGISTRY_TOKEN" "$OUT"; then
    pass "04 坏 token → 401 报人话并点名环境变量"
else
    fail "04 坏 token → 401 报人话并点名环境变量" "rc=$RC $(tr '\n' ' ' < "$OUT")"
fi

# ─────────────────────── 05 缺 token → 本地就拦下 ───────────────────────
OUT="$WORK/05.log"
( cd "$PKG" && unset QI_REGISTRY_TOKEN && run_limited "$QI" 包 发布 ) >"$OUT" 2>&1
RC=$?
if [ $RC -ne 0 ] && grep -q "没有发布 token" "$OUT"; then
    pass "05 没设 token → 本地直接拦下，不做无谓的远程往返"
else
    fail "05 没设 token → 本地直接拦下，不做无谓的远程往返" "rc=$RC $(tr '\n' ' ' < "$OUT")"
fi
# 版本号改回去，后面的用例还要用 0.1.0
sed -i.bak 's/版本 = "0.9.0"/版本 = "0.1.0"/' "$PKG/qi.toml" && rm -f "$PKG/qi.toml.bak"

# ─────────────────────── 再发一个包，给「搜索」和「添加」用 ───────────────────────
PKG2="$WORK/源码/算术包"
mkdir -p "$PKG2"
cat > "$PKG2/qi.toml" <<'EOF'
[包]
名称 = "算术包"
版本 = "0.2.0"
入口 = "算术包.qi"
EOF
cat > "$PKG2/算术包.qi" <<'EOF'
包 算术包;

公开 函数 相加(甲: 整数, 乙: 整数): 整数 {
    返回 甲 + 乙;
}
EOF
( cd "$PKG2" && QI_REGISTRY_TOKEN="$TOKEN" run_limited "$QI" 包 发布 ) >"$WORK/06.log" 2>&1
if [ $? -eq 0 ] && [ -n "$(state_sha 算术包 0.2.0)" ]; then
    pass "06 第二个包也发布成功（中文包名 percent-encode 进 URL）"
else
    fail "06 第二个包也发布成功（中文包名 percent-encode 进 URL）" "$(tr '\n' ' ' < "$WORK/06.log")"
fi

# ─────────────────────── 07 安装 ───────────────────────
APP="$WORK/工程"
mkdir -p "$APP"
cat > "$APP/qi.toml" <<'EOF'
[包]
名称 = "应用"
版本 = "0.0.1"
入口 = "主程序.qi"

[依赖]
试验包 = "0.1.0"
EOF
cat > "$APP/主程序.qi" <<'EOF'
导入 试验包::{打招呼, 翻倍};

函数 入口() {
    打印行(打招呼("世界"));
    打印行(翻倍(21));
}
EOF

OUT="$WORK/07.log"
( cd "$APP" && run_limited "$QI" 包 安装 ) >"$OUT" 2>&1
RC=$?
if [ $RC -eq 0 ] && grep -q "已安装 试验包 0.1.0" "$OUT"; then
    pass "07 qi 包 安装 成功"
else
    fail "07 qi 包 安装 成功" "rc=$RC $(tr '\n' ' ' < "$OUT")"
fi

if [ -f "$APP/qi_packages/试验包/试验包.qi" ] && [ -f "$APP/qi_packages/试验包/qi.toml" ]; then
    pass "08 包落地到 ./qi_packages/试验包/"
else
    fail "08 包落地到 ./qi_packages/试验包/" "目录内容: $(ls -R "$APP/qi_packages" 2>&1 | tr '\n' ' ')"
fi

# 打包排除：这四样都不该出现在装好的包里
EXCLUDED_OK=1
for junk in tests target .git 顺手.o; do
    if [ -e "$APP/qi_packages/试验包/$junk" ]; then
        EXCLUDED_OK=0
        echo "     未排除: $junk"
    fi
done
if [ $EXCLUDED_OK -eq 1 ]; then
    pass "09 打包排除生效（tests/ target/ .git/ *.o 都没进包）"
else
    fail "09 打包排除生效（tests/ target/ .git/ *.o 都没进包）"
fi

# qi.lock 生成且 sha256 与服务端一致
if [ -f "$APP/qi.lock" ] && grep -q "$SERVER_SHA" "$APP/qi.lock"; then
    pass "10 qi.lock 生成且记录的 sha256 与服务端一致"
else
    fail "10 qi.lock 生成且记录的 sha256 与服务端一致" "$(cat "$APP/qi.lock" 2>&1 | tr '\n' ' ')"
fi

# 装下来的内容本身 sha256 正确 —— 就地重打一次，必须还原成同一个指纹
REPACK_SHA="$(cd "$APP/qi_packages/试验包" && run_limited "$QI" 包 发布 --只打包 2>/dev/null | sed -n 's/.*sha256 \([0-9a-f]\{64\}\).*/\1/p' | head -1)"
if [ -n "$REPACK_SHA" ] && [ "$REPACK_SHA" = "$SERVER_SHA" ]; then
    pass "11 装下来的内容重新打包 sha256 与服务端一致（内容逐字节正确）"
else
    fail "11 装下来的内容重新打包 sha256 与服务端一致（内容逐字节正确）" "重打=$REPACK_SHA 服务端=$SERVER_SHA"
fi

# ─────────────────────── 12 装完真能用 ───────────────────────
OUT="$WORK/12.log"
( cd "$APP" && run_limited "$QI" run 主程序.qi ) >"$OUT" 2>&1
RC=$?
if [ $RC -eq 0 ] && grep -q "你好，世界" "$OUT" && grep -q "^42$" "$OUT"; then
    pass "12 装完 qi run 真跑通（导入 试验包 → 输出正确）"
else
    fail "12 装完 qi run 真跑通（导入 试验包 → 输出正确）" "rc=$RC $(tr '\n' ' ' < "$OUT")"
fi

# ─────────────────────── 13 幂等 ───────────────────────
OUT="$WORK/13.log"
( cd "$APP" && run_limited "$QI" 包 安装 ) >"$OUT" 2>&1
RC=$?
if [ $RC -eq 0 ] && grep -q "试验包 已是 0.1.0" "$OUT" && grep -q "跳过 1" "$OUT"; then
    pass "13 连着装两次，第二次跳过（幂等）"
else
    fail "13 连着装两次，第二次跳过（幂等）" "rc=$RC $(tr '\n' ' ' < "$OUT")"
fi

# ─────────────────────── 14 添加 ───────────────────────
OUT="$WORK/14.log"
( cd "$APP" && run_limited "$QI" 包 添加 算术包 0.2.0 ) >"$OUT" 2>&1
RC=$?
if [ $RC -eq 0 ] && grep -q '算术包 = "0.2.0"' "$APP/qi.toml" && [ -f "$APP/qi_packages/算术包/算术包.qi" ]; then
    pass "14 qi 包 添加 改写 qi.toml 并立即安装"
else
    fail "14 qi 包 添加 改写 qi.toml 并立即安装" "rc=$RC $(tr '\n' ' ' < "$OUT")"
fi

# 添加不该毁掉已有内容（[包] 段、原有依赖都还在）
if grep -q '名称 = "应用"' "$APP/qi.toml" && grep -q '试验包 = "0.1.0"' "$APP/qi.toml"; then
    pass "15 添加时 qi.toml 原有内容完好"
else
    fail "15 添加时 qi.toml 原有内容完好" "$(tr '\n' ' ' < "$APP/qi.toml")"
fi

# 两个包一起用，还得跑得通
cat > "$APP/主程序.qi" <<'EOF'
导入 试验包::{打招呼, 翻倍};
导入 算术包::{相加};

函数 入口() {
    打印行(打招呼("世界"));
    打印行(翻倍(21));
    打印行(相加(40, 2));
}
EOF
OUT="$WORK/16.log"
( cd "$APP" && run_limited "$QI" run 主程序.qi ) >"$OUT" 2>&1
RC=$?
if [ $RC -eq 0 ] && grep -q "你好，世界" "$OUT" && [ "$(grep -c '^42$' "$OUT")" = "2" ]; then
    pass "16 两个注册中心包同时导入也跑得通"
else
    fail "16 两个注册中心包同时导入也跑得通" "rc=$RC $(tr '\n' ' ' < "$OUT")"
fi

# ─────────────────────── 17 搜索 ───────────────────────
OUT="$WORK/17.log"
run_limited "$QI" 包 搜索 试验 >"$OUT" 2>&1
RC=$?
if [ $RC -eq 0 ] && grep -q "试验包" "$OUT" && grep -q "0.1.0" "$OUT"; then
    pass "17 qi 包 搜索 命中试验包"
else
    fail "17 qi 包 搜索 命中试验包" "rc=$RC $(tr '\n' ' ' < "$OUT")"
fi

OUT="$WORK/18.log"
run_limited "$QI" 包 搜索 绝不存在的东西 >"$OUT" 2>&1
RC=$?
if [ $RC -eq 0 ] && grep -q "没有匹配" "$OUT"; then
    pass "18 搜不到时给人话（且不当成错误）"
else
    fail "18 搜不到时给人话（且不当成错误）" "rc=$RC $(tr '\n' ' ' < "$OUT")"
fi

# ─────────────────────── 19 篡改 sha256 → 拒绝安装 ───────────────────────
# 用**工具自己生成**的 qi.lock 再改坏，而不是手搓一份：手搓的容易在格式上
# 走样，走样了就被当成「没有 lock」跳过校验，这条断言会假绿。
BAD="$WORK/篡改工程"
mkdir -p "$BAD"
cat > "$BAD/qi.toml" <<'EOF'
[包]
名称 = "篡改应用"
版本 = "0.0.1"

[依赖]
试验包 = "0.1.0"
EOF
( cd "$BAD" && run_limited "$QI" 包 安装 ) >"$WORK/19a.log" 2>&1
if [ ! -f "$BAD/qi.lock" ]; then
    fail "19 前置：先正常装一次生成 qi.lock" "$(tr '\n' ' ' < "$WORK/19a.log")"
fi
# 把 lock 里的 sha256 换成全 0，并删掉已装的目录逼它重装
sed -i.bak "s/$SERVER_SHA/0000000000000000000000000000000000000000000000000000000000000000/" \
    "$BAD/qi.lock" && rm -f "$BAD/qi.lock.bak"
rm -rf "$BAD/qi_packages"
if grep -q "0000000000000000000000000000000000000000000000000000000000000000" "$BAD/qi.lock"; then
    pass "19a qi.lock 里的 sha256 已被改坏（前置条件成立）"
else
    fail "19a qi.lock 里的 sha256 已被改坏（前置条件成立）" "$(tr '\n' ' ' < "$BAD/qi.lock")"
fi

OUT="$WORK/19.log"
( cd "$BAD" && run_limited "$QI" 包 安装 ) >"$OUT" 2>&1
RC=$?
if [ $RC -ne 0 ] && grep -q "qi.lock 锁定的" "$OUT" && grep -q "拒绝安装" "$OUT"; then
    pass "19 qi.lock 的 sha256 被篡改 → 拒绝安装"
else
    fail "19 qi.lock 的 sha256 被篡改 → 拒绝安装" "rc=$RC $(tr '\n' ' ' < "$OUT")"
fi
if [ ! -d "$BAD/qi_packages/试验包" ]; then
    pass "20 拒绝安装时不留半成品目录"
else
    fail "20 拒绝安装时不留半成品目录" "$(ls -R "$BAD/qi_packages" 2>&1 | tr '\n' ' ')"
fi

# ─────────────────────── 21 装不存在的版本 → 人话 ───────────────────────
NOPE="$WORK/缺版本工程"
mkdir -p "$NOPE"
cat > "$NOPE/qi.toml" <<'EOF'
[包]
名称 = "缺版本应用"

[依赖]
试验包 = "9.9.9"
EOF
OUT="$WORK/21.log"
( cd "$NOPE" && run_limited "$QI" 包 安装 ) >"$OUT" 2>&1
RC=$?
if [ $RC -ne 0 ] && grep -q "没有 试验包 的 9.9.9 版本" "$OUT"; then
    pass "21 装不存在的版本 → 人话（并提示用 搜索）"
else
    fail "21 装不存在的版本 → 人话（并提示用 搜索）" "rc=$RC $(tr '\n' ' ' < "$OUT")"
fi

# ─────────────────────── 22 没装就编译 → 提示先装 ───────────────────────
FRESH="$WORK/没装工程"
mkdir -p "$FRESH"
cat > "$FRESH/qi.toml" <<'EOF'
[包]
名称 = "没装应用"
入口 = "主程序.qi"

[依赖]
试验包 = "0.1.0"
EOF
cat > "$FRESH/主程序.qi" <<'EOF'
导入 试验包::{翻倍};

函数 入口() {
    打印行(翻倍(1));
}
EOF
OUT="$WORK/22.log"
( cd "$FRESH" && run_limited "$QI" compile 主程序.qi -o "$WORK/没装产物" ) >"$OUT" 2>&1
RC=$?
if [ $RC -ne 0 ] && grep -q "qi 包 安装" "$OUT"; then
    pass "22 依赖没装就编译 → 报错指明跑 qi 包 安装"
else
    fail "22 依赖没装就编译 → 报错指明跑 qi 包 安装" "rc=$RC $(tr '\n' ' ' < "$OUT")"
fi

# ─────────────────────── 23 范围版本被拒 ───────────────────────
RANGE="$WORK/范围工程"
mkdir -p "$RANGE"
cat > "$RANGE/qi.toml" <<'EOF'
[包]
名称 = "范围应用"

[依赖]
试验包 = { 版本 = "^0.1" }
EOF
OUT="$WORK/23.log"
( cd "$RANGE" && run_limited "$QI" 包 安装 ) >"$OUT" 2>&1
RC=$?
if [ $RC -ne 0 ] && grep -q "三段数字" "$OUT"; then
    pass "23 版本范围（^0.1）被明确拒绝，而不是装错东西"
else
    fail "23 版本范围（^0.1）被明确拒绝，而不是装错东西" "rc=$RC $(tr '\n' ' ' < "$OUT")"
fi

# ─────────────────────── 24 注册中心不可达 → 人话 ───────────────────────
OUT="$WORK/24.log"
( cd "$APP" && QI_REGISTRY="http://127.0.0.1:1" run_limited "$QI" 包 搜索 试验 ) >"$OUT" 2>&1
RC=$?
if [ $RC -ne 0 ] && grep -q "无法连接注册中心" "$OUT"; then
    pass "24 注册中心不可达 → 人话（点名 QI_REGISTRY）"
else
    fail "24 注册中心不可达 → 人话（点名 QI_REGISTRY）" "rc=$RC $(tr '\n' ' ' < "$OUT")"
fi

# ─────────────────────── 25 列出已装 ───────────────────────
OUT="$WORK/25.log"
( cd "$APP" && run_limited "$QI" 包 列出 ) >"$OUT" 2>&1
RC=$?
if [ $RC -eq 0 ] && grep -q "试验包 0.1.0  已装" "$OUT" && grep -q "算术包 0.2.0  已装" "$OUT"; then
    pass "25 qi 包 列出 显示两个包都已装"
else
    fail "25 qi 包 列出 显示两个包都已装" "rc=$RC $(tr '\n' ' ' < "$OUT")"
fi

echo
echo "总计 ${total}，通过 ${passed}，失败 ${failed}"
[ $failed -eq 0 ]
