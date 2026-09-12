#!/usr/bin/env bash
# 挡住「$变量 后面紧跟全角标点」的写法 —— bash（5.3 也一样）会把标点的第一个字节
# 吃进变量名，`set -u` 下直接报「var?: 未绑定的变量」并把脚本打死。
#
# 为什么值得一条门禁：这个写法只在**出错路径**上，平时永远不执行。
# qi/tests/wasm/断言.sh 的四个 FAIL 分支全中招，一直没人发现 —— 因为那套
# 从来没红过。等它第一次红的时候，你看到的不是 FAIL，是一句莫名其妙的报错。
#
# 写成 ${var} 再接全角标点。
set -uo pipefail
# ⚠ shell 变量名一律 ASCII —— bash 不支持非 ASCII 变量名，写 中文= 会被当成命令去执行，
# 而且失败后变量为空、断言静默通过（假绿）。这个脚本第一版就是这么写错的。

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"

# 出题.sh 里那两行是在讲这个坑本身，拿它当反面例子，放行。
hits=$(find "$ROOT" -name '*.sh' \
        -not -path '*/node_modules/*' -not -path '*/target/*' \
        -not -path '*/.claude/*' -not -name '出题.sh' -print0 2>/dev/null \
      | xargs -0 grep -nE '\$[A-Za-z_][A-Za-z0-9_]*[（【「，：？！》〉、。]' 2>/dev/null)

if [ -n "${hits}" ]; then
    echo "shell全角变量: 失败 —— 下面这些 \$变量 后面紧跟全角标点，bash 会把它算进变量名："
    echo "${hits}" | sed 's/^/    /'
    echo "    改成 \${变量} 即可。"
    exit 1
fi
echo "shell全角变量: 通过"
exit 0
