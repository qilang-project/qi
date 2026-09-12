"""命名规范扫描。规则与用意见同目录的 命名规范.sh 抬头。"""
import os
import re
import sys

CJK = r"[一-鿿]"
SKIP_DIR = {"node_modules", "target", "target-wt", ".git", ".claude", "dist", "qi_packages"}
# 这两个文件拿反面写法当教材，放行
SKIP_FILE = {"出题.sh", "命名规范.sh"}
# qi-vscode/examples 是给 TextMate 语法高亮做的**样例**，里头是 function / let
# 这类根本不属于 qi 的伪代码，专门用来看高亮对不对，不是要编译的代码。
SKIP_PATH_PART = ("qi-vscode/examples/",)

# 一律**行首锚定**（允许缩进和 export）。不锚定的话散文会大量误命中：
# 注释里的「morph 不再管它的 class 和 style」、「非 const 的 char* 形参」、
# 「macOS 上 /var 是 /private/var 的软链」都会被当成声明。
DECL = r"^[ \t]*(?:export\s+|public\s+)?(?:var|let|const|function|class)\s+"

RULES = [
    ((".js", ".mjs", ".ts", ".css"),
     re.compile(rf"{DECL}{CJK}", re.M),
     "JS/TS/CSS 里有中文标识符（中文只留给文本和注释）"),
    ((".qi",),
     re.compile(rf"{DECL}{CJK}", re.M),
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
    # Python 跟 JS 一个待遇：中文只留给字符串和注释。只看声明位置 ——
    # def/class/for/import/global/lambda 后面、赋值左边、as 后面、def 的形参表里。
    ((".py",),
     re.compile(
         rf"^[ \t]*(?:async\s+)?(?:def|class|for|import|from|global|nonlocal|lambda)\s+{CJK}"
         rf"|^[ \t]*{CJK}[\w一-鿿]*\s*(?:=[^=]|:\s*\w|,\s*\w)"
         # 形参表：引号之前出现的中文才算 —— character="奇" 这种默认值是字符串
         rf"|^[ \t]*(?:async\s+)?def\s+\w+\([^)\"']*{CJK}"
         # with/except … as 中文名 —— 后面得紧跟 : , ) 之一，注释里的「as 后面」不算
         rf"|\bas\s+{CJK}[\w一-鿿]*\s*[:,)]",
         re.M),
     "Python 里有中文标识符（中文只留给文本和注释）"),
]


def walk_files(root):
    for dir_path, dirs, files in os.walk(root):
        dirs[:] = [d for d in dirs if d not in SKIP_DIR]
        for name in files:
            file_path = os.path.join(dir_path, name)
            if name in SKIP_FILE or any(k in file_path for k in SKIP_PATH_PART):
                continue
            yield file_path


def main():
    root = sys.argv[1] if len(sys.argv) > 1 else "."
    hits = {}
    for file_path in walk_files(root):
        ext = os.path.splitext(file_path)[1]
        applicable = [(p, t) for exts, p, t in RULES if ext in exts]
        if not applicable:
            continue
        try:
            with open(file_path, encoding="utf-8") as f:
                lines = f.readlines()
        except (UnicodeDecodeError, OSError):
            continue
        for line_no, line in enumerate(lines, 1):
            for regex, title in applicable:
                if regex.search(line):
                    hits.setdefault(title, []).append(
                        f"{os.path.relpath(file_path, root)}:{line_no}: {line.strip()[:100]}"
                    )
    if not hits:
        print("命名规范: 通过")
        return 0
    for title, entry in hits.items():
        print(f"命名规范: {title}")
        for e in entry[:10]:
            print(f"    {e}")
        if len(entry) > 10:
            print(f"    …… 还有 {len(entry) - 10} 处")
    return 1


sys.exit(main())
