"""命名规范扫描。规则与用意见同目录的 命名规范.sh 抬头。"""
import os
import re
import sys

CJK = r"[一-鿿]"
SKIP_DIR = {"node_modules", "target", "target-wt", ".git", ".claude", "dist", "qi_packages"}
# 这两个文件拿反面写法当教材，放行
SKIP_FILE = {"出题.sh", "命名规范.py", "命名规范.sh"}
# qi-vscode/examples 是给 TextMate 语法高亮做的**样例**，里头是 function / let
# 这类根本不属于 qi 的伪代码，专门用来看高亮对不对，不是要编译的代码。
SKIP_PATH_PART = ("qi-vscode/examples/",)

# 一律**行首锚定**（允许缩进和 export）。不锚定的话散文会大量误命中：
# 注释里的「morph 不再管它的 class 和 style」、「非 const 的 char* 形参」、
# 「macOS 上 /var 是 /private/var 的软链」都会被当成声明。
声明 = r"^[ \t]*(?:export\s+|public\s+)?(?:var|let|const|function|class)\s+"

规则 = [
    ((".js", ".mjs", ".ts", ".css"),
     re.compile(rf"{声明}{CJK}", re.M),
     "JS/TS/CSS 里有中文标识符（中文只留给文本和注释）"),
    ((".qi",),
     re.compile(rf"{声明}{CJK}", re.M),
     "qi 内嵌 <script> 里有中文 JS 标识符"),
    # class="…" / id="…"：值里不能出现 + （那是字符串拼接不是字面属性），
    # 且要在同一个引号对里出现中文。
    ((".qi",),
     re.compile(rf'\b(?:class|id)=\\?"[^"+\\]*{CJK}'),
     "HTML class/id 用了中文（要跨 qi/TS 两侧对齐，必须英文）"),
    ((".sh",),
     re.compile(rf"^\s*{CJK}[\w一-鿿]*="),
     "shell 变量名用了中文（bash 不支持，会静默变空 → 假绿）"),
    ((".sh",),
     re.compile(r"\$[A-Za-z_]\w*[（【「，：？！》〉、。]"),
     "$变量 紧跟全角标点（bash 会把标点算进变量名），改成 ${变量}"),
]


def 遍历(根):
    for 目录, 子目录表, 文件表 in os.walk(根):
        子目录表[:] = [d for d in 子目录表 if d not in SKIP_DIR]
        for 名 in 文件表:
            路径 = os.path.join(目录, 名)
            if 名 in SKIP_FILE or any(k in 路径 for k in SKIP_PATH_PART):
                continue
            yield 路径


def 主():
    根 = sys.argv[1] if len(sys.argv) > 1 else "."
    命中 = {}
    for 路径 in 遍历(根):
        后缀 = os.path.splitext(路径)[1]
        适用 = [(p, t) for exts, p, t in 规则 if 后缀 in exts]
        if not 适用:
            continue
        try:
            with open(路径, encoding="utf-8") as f:
                行表 = f.readlines()
        except (UnicodeDecodeError, OSError):
            continue
        for 号, 行 in enumerate(行表, 1):
            for 正则, 标题 in 适用:
                if 正则.search(行):
                    命中.setdefault(标题, []).append(
                        f"{os.path.relpath(路径, 根)}:{号}: {行.strip()[:100]}"
                    )
    if not 命中:
        print("命名规范: 通过")
        return 0
    for 标题, 条目 in 命中.items():
        print(f"命名规范: {标题}")
        for e in 条目[:10]:
            print(f"    {e}")
        if len(条目) > 10:
            print(f"    …… 还有 {len(条目) - 10} 处")
    return 1


sys.exit(主())
