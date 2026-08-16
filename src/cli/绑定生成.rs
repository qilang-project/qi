//! `qi 绑定` —— C 头文件 → qi 外部块绑定生成器（bindgen）。
//!
//! 手写 `外部 "z" { 函数 compress(...): 整数; }` 一两个函数还行，接一整个 C 库
//! （zlib 60 个函数、sqlite3 两百多个）就不可能了。这里把头文件交给 clang，
//! 让**它**去解析 C（预处理器、typedef、宏、平台条件编译全在里面），我们只读它
//! 吐出来的 AST JSON，翻译成 qi 的外部块。
//!
//! ```text
//! qi 绑定 /usr/include/zlib.h --库 z -o zlib绑定.qi
//! qi 绑定 sqlite3.h --库 sqlite3 --前缀 sqlite3_ -o sqlite绑定.qi
//! ```
//!
//! ## 为什么不自己写 C parser
//!
//! C 的声明语法（`int (*fp[3])(char*)`）、宏、`#if` 平台分支、typedef 链 ——
//! 自己写一个能对付真实系统头文件的解析器是几万行的事，而且永远比 clang 差。
//! `clang -Xclang -ast-dump=json -fsyntax-only` 现成、快（zlib.h 2MB JSON 约 60ms）、
//! 已经把 typedef 和条件编译都解开了。宏常量走另一趟 `clang -E -dD`
//! （AST 里没有宏，`-dD` 会把 `#define` 原样留在输出里，配合行标记还能知道
//! 每个宏出自哪个文件）。
//!
//! ## 边界：什么生成、什么跳过
//!
//! 跳过的一律记进生成文件顶部的「跳过清单」，不静默丢 —— 用户得知道
//! 这个库有哪几个函数是这台生成器给不出来的，好手写补上。
//!
//! - **变参函数**（`printf` 类）：qi 外部块不支持变参，`登记外部函数` 会直接报错。
//! - **按值传/返结构体**：qi v2 其实支持「≤16 字节、字段全标量」的小结构体，
//!   但那要求生成端同时产出 `类型 X { ... }` 声明并算准 C 的布局与大小
//!   （clang 的 AST JSON 不给字段偏移/大小），算错就是静默的 ABI 错位。
//!   v1 一律跳过并记录，需要的人按 `示例/基础/外部函数v2/示例.qi` 手写。
//! - **头文件里带函数体的定义**（`static inline`）：那些符号根本不在 .so/.a 里，
//!   声明出来只会在链接期报 undefined symbol，跳过更诚实。
//! - **`va_list` / `_Complex` / `__int128` / 嵌套函数指针**：没有对应的 qi 类型。
//!
//! ## 一个必须知道的坑：C 的 int 返回
//!
//! qi 的 整数 是 64 位，C 的 int 是 32 位。C 函数返回 `int` 时只写寄存器的低 32 位
//! （x86-64 写 eax、arm64 写 w0，两边都会把高 32 位清零），qi 按 64 位读回来，
//! 于是 `-1` 变成 4294967295。这不是本生成器引入的，手写外部块一样会踩，
//! 但绑定一次几百个函数会把它放大 —— 所以凡是 C 侧返回窄有符号整数的函数，
//! 生成的行尾都标一个 `// C:int 返回`，文件头也写清楚怎么把它转回负数。

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

use super::绑定类型::{
    位置, 取类型串, 拆返回类型, 数字字面量, 映射, 映射类型, 是窄有符号整数, 解别名,
};

/// 生成选项（由 CLI 层填好后交给 [`运行`]）。
pub struct 选项 {
    /// C 头文件路径。
    pub 头文件: PathBuf,
    /// 外部块的库名（`外部 "<库>"`）。空串表示不额外链接。
    pub 库: String,
    /// 只收这些前缀的函数（可给多个，任一命中即收）。空 = 全收。
    pub 前缀: Vec<String>,
    /// 连被 `#include` 进来的头文件里的声明一起收（默认只收目标头文件自身的）。
    pub 全部头文件: bool,
    /// 不生成 `常量`（宏 + 枚举）。
    pub 无常量: bool,
    /// 透传给 clang 的 `-I` 目录。
    pub 包含目录: Vec<PathBuf>,
    /// 透传给 clang 的任意其他参数（`-D`、`-std=` 等）。
    pub clang参数: Vec<String>,
    /// 记进生成文件注释的命令行原文。
    pub 命令行: String,
}

/// 生成结果：qi 源码文本 + 统计（CLI 层打摘要用）。
pub struct 产物 {
    pub 源码: String,
    pub 函数数: usize,
    pub 常量数: usize,
    pub 跳过数: usize,
}

/// 一条生成出来的外部函数。
struct 外部函数 {
    名字: String,
    参数: Vec<(String, 映射)>,
    返回: 映射,
    /// C 侧返回的是窄有符号整数（int/short/…）—— 行尾要标注。
    窄返回: bool,
}

/// 一条常量（宏或枚举值）。
struct 常量项 {
    名字: String,
    值: String,
    来源: &'static str,
}

/// 被跳过的东西：名字 + 人话原因。
struct 跳过项 {
    名字: String,
    原因: String,
}

// ───────────────────────── 入口 ─────────────────────────

/// 跑一次生成。所有失败都返回人话错误（CLI 层直接打出去）。
pub fn 运行(opts: &选项) -> Result<产物, String> {
    let 头文件 = opts
        .头文件
        .canonicalize()
        .map_err(|e| format!("读不到头文件 {:?}：{}", opts.头文件, e))?;

    let ast = 取ast(&头文件, opts)?;
    let tu = ast
        .get("inner")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "clang 的 AST JSON 里没有顶层声明（inner）—— 头文件是空的？".to_string())?;

    let 别名表 = 收typedef(tu);

    let mut 函数表: Vec<外部函数> = Vec::new();
    let mut 常量表: Vec<常量项> = Vec::new();
    let mut 跳过表: Vec<跳过项> = Vec::new();
    let mut 已见函数: HashSet<String> = HashSet::new();
    let mut 已见常量: HashSet<String> = HashSet::new();

    // clang 的 JSON 位置信息是「变了才打」的增量编码：同一个文件里连续几个声明，
    // 只有第一个带 file 字段。所以得按文档顺序走一遍，自己维护「当前文件」。
    // glibc 把声明拆进 bits/*.h，只认「文件名相等」会一个函数都收不到。
    let 直接包含 = 直接包含集(opts, &头文件);
    let mut 当前文件 = String::new();
    for 节点 in tu {
        if let Some(l) = 节点.get("loc") {
            收文件(l, &mut 当前文件);
        }
        let 本节点文件 = 当前文件.clone();
        // 先把子树走完（保持文件跟踪对下一个兄弟节点准确），再处理本节点。
        if let Some(r) = 节点.get("range") {
            收文件(r, &mut 当前文件);
        }
        if let Some(内) = 节点.get("inner").and_then(|v| v.as_array()) {
            for 子 in 内 {
                走子树(子, &mut 当前文件);
            }
        }

        if !opts.全部头文件 && !是本头文件的(&本节点文件, &头文件, &直接包含)
        {
            continue;
        }
        if 节点.get("isImplicit").and_then(|v| v.as_bool()) == Some(true) {
            continue;
        }

        match 节点.get("kind").and_then(|v| v.as_str()) {
            Some("FunctionDecl") => {
                收函数(节点, &别名表, opts, &mut 已见函数, &mut 函数表, &mut 跳过表)
            }
            Some("EnumDecl") if !opts.无常量 => 收枚举(节点, &mut 已见常量, &mut 常量表),
            _ => {}
        }
    }

    if !opts.无常量 {
        收宏(opts, &头文件, &mut 已见常量, &mut 常量表)?;
    }

    // 常量名与函数名撞了就让函数赢（函数是主角），常量改记进跳过清单。
    let 函数名集: HashSet<&str> = 函数表.iter().map(|f| f.名字.as_str()).collect();
    let mut 保留常量 = Vec::new();
    for c in 常量表 {
        if 函数名集.contains(c.名字.as_str()) {
            跳过表.push(跳过项 {
                名字: c.名字,
                原因: "常量名与同名函数冲突（外部函数与常量共用命名空间）".to_string(),
            });
        } else {
            保留常量.push(c);
        }
    }

    let 源码 = 排版(opts, &头文件, &函数表, &保留常量, &跳过表);
    Ok(产物 {
        源码,
        函数数: 函数表.len(),
        常量数: 保留常量.len(),
        跳过数: 跳过表.len(),
    })
}

// ───────────────────────── clang 调用 ─────────────────────────

fn clang程序() -> String {
    std::env::var("QI_CLANG").unwrap_or_else(|_| "clang".to_string())
}

fn 取ast(头文件: &Path, opts: &选项) -> Result<Value, String> {
    let mut cmd = Command::new(clang程序());
    cmd.args(["-Xclang", "-ast-dump=json", "-fsyntax-only"]);
    for d in &opts.包含目录 {
        cmd.arg("-I").arg(d);
    }
    for a in &opts.clang参数 {
        cmd.arg(a);
    }
    cmd.arg(头文件);
    let 输出 = cmd.output().map_err(|e| {
        format!(
            "跑不起来 clang（{}）：{} —— 需要本机装有 clang。",
            clang程序(),
            e
        )
    })?;
    if 输出.stdout.is_empty() {
        return Err(format!(
            "clang 没有产出 AST：\n{}",
            String::from_utf8_lossy(&输出.stderr)
        ));
    }
    // clang 对系统头文件常有一堆 warning，只要 AST 出来了就不当失败。
    serde_json::from_slice(&输出.stdout).map_err(|e| format!("clang 的 AST JSON 解析失败：{}", e))
}

/// 宏常量走 `-E -dD`：`-dD` 把 `#define` 留在预处理输出里，行标记
/// （`# 123 "file.h"`）给出每个宏来自哪个文件，据此只收目标头文件自己定义的宏。
/// `-dM` 只给一份「最终宏表」，没有文件归属，收进来会混一堆编译器内建宏。
fn 收宏(
    opts: &选项,
    头文件: &Path,
    已见: &mut HashSet<String>,
    出: &mut Vec<常量项>,
) -> Result<(), String> {
    let mut cmd = Command::new(clang程序());
    cmd.args(["-E", "-dD"]);
    for d in &opts.包含目录 {
        cmd.arg("-I").arg(d);
    }
    for a in &opts.clang参数 {
        cmd.arg(a);
    }
    cmd.arg(头文件);
    let 输出 = cmd
        .output()
        .map_err(|e| format!("跑不起来 clang 预处理：{}", e))?;
    let 文本 = String::from_utf8_lossy(&输出.stdout);

    // 宏跟函数同一条规矩：glibc 的 math.h 把宏也散在 bits/ 里。
    let 直接包含 = 直接包含集(opts, 头文件);
    let mut 当前文件 = String::new();
    for 行 in 文本.lines() {
        let 行 = 行.trim_start();
        if let Some(剩) = 行.strip_prefix("# ") {
            if let Some(f) = 行标记文件(剩) {
                当前文件 = f;
            }
            continue;
        }
        let Some(剩) = 行.strip_prefix("#define ") else {
            continue;
        };
        if !opts.全部头文件 && !是本头文件的(&当前文件, 头文件, &直接包含) {
            continue;
        }
        let 剩 = 剩.trim();
        let 名结束 = 剩
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(剩.len());
        let 名 = &剩[..名结束];
        if 名.is_empty() || 名.starts_with('_') {
            continue; // 下划线开头是编译器/实现保留名，噪音。
        }
        let 尾 = &剩[名结束..];
        if 尾.starts_with('(') {
            continue; // 函数式宏，不是常量。
        }
        let Some(值) = 数字字面量(尾.trim()) else {
            continue;
        };
        if !已见.insert(名.to_string()) {
            continue;
        }
        出.push(常量项 {
            名字: 名.to_string(),
            值,
            来源: "宏",
        });
    }
    Ok(())
}

/// 解析 `# 123 "path" 1 2` 里的文件名。
///
/// 这里必须按 **C 字符串字面量** 解转义，不能只处理 `\"` 和 `\\`：clang 打行标记时
/// 会把所有非 ASCII 字节写成八进制转义，路径 `.../绑定生成/...` 出来长这样：
/// `"/x/tests/\347\273\221\345\256\232.../bianjiao.h"`。只吃掉反斜杠的话，
/// 文件名会变成一串数字，跟目标头文件永远对不上 —— 表现是「中文目录下所有
/// #define 常量静默消失，enum 却好好的」（enum 走 AST JSON，那边是正经 UTF-8）。
fn 行标记文件(剩: &str) -> Option<String> {
    let 起 = 剩.find('"')? + 1;
    let 字节 = 剩.as_bytes();
    let mut 出: Vec<u8> = Vec::new();
    let mut i = 起;
    while i < 字节.len() {
        match 字节[i] {
            b'"' => return Some(String::from_utf8_lossy(&出).into_owned()),
            b'\\' => {
                i += 1;
                if i >= 字节.len() {
                    break;
                }
                let c = 字节[i];
                if c.is_ascii_digit() && c < b'8' {
                    // 八进制转义，最多三位
                    let mut 值: u32 = 0;
                    let mut 位 = 0;
                    while i < 字节.len() && 位 < 3 && 字节[i].is_ascii_digit() && 字节[i] < b'8'
                    {
                        值 = 值 * 8 + (字节[i] - b'0') as u32;
                        i += 1;
                        位 += 1;
                    }
                    出.push(值 as u8);
                    continue;
                }
                出.push(match c {
                    b'n' => b'\n',
                    b't' => b'\t',
                    b'r' => b'\r',
                    其他 => 其他, // \" \\ \' 等原样
                });
                i += 1;
            }
            其他 => {
                出.push(其他);
                i += 1;
            }
        }
    }
    None
}

// ───────────────────────── AST 遍历 ─────────────────────────

/// 递归更新「当前文件」。只认直接的 file 字段，不认 includedFrom
/// （那是「谁 include 了我」，不是当前位置）。
fn 收文件(位置: &Value, 当前: &mut String) {
    if let Some(f) = 位置.get("file").and_then(|v| v.as_str()) {
        *当前 = f.to_string();
    }
    for k in ["begin", "end", "spellingLoc", "expansionLoc"] {
        if let Some(子) = 位置.get(k) {
            收文件(子, 当前);
        }
    }
}

fn 走子树(节点: &Value, 当前: &mut String) {
    if let Some(l) = 节点.get("loc") {
        收文件(l, 当前);
    }
    if let Some(r) = 节点.get("range") {
        收文件(r, 当前);
    }
    if let Some(内) = 节点.get("inner").and_then(|v| v.as_array()) {
        for 子 in 内 {
            走子树(子, 当前);
        }
    }
}

fn 同一文件(a: &str, b: &Path) -> bool {
    if a.is_empty() {
        return false;
    }
    let pa = Path::new(a);
    match pa.canonicalize() {
        Ok(p) => p == b,
        Err(_) => pa == b,
    }
}

/// 「这个声明算不算目标头文件的」—— 目标头本身，或者它**直接** `#include` 的头。
///
/// 光比文件名在 glibc 上会颗粒无收：`/usr/include/math.h` 自己一个函数都不声明，
/// 全都在 `bits/mathcalls.h` 里（macOS 的 math.h 才是直接写在里面）。拆到 `bits/`
/// 是 glibc 的标准做法，那些声明**就是** math.h 对外的 API，不收等于什么都没生成。
///
/// 只放行深度 1，不放行整棵包含树：math.h 也会拉进 features.h / stddef.h，
/// 那些该由用户单独绑，不该混进 libm 的绑定里。
fn 是本头文件的(文件: &str, 头文件: &Path, 直接包含: &HashSet<PathBuf>) -> bool {
    if 同一文件(文件, 头文件) {
        return true;
    }
    if 文件.is_empty() {
        return false;
    }
    let p = Path::new(文件);
    let 规范 = p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    直接包含.contains(&规范)
}

/// 跑一遍预处理，从行标记的进/出栈算出目标头文件**直接**包含了哪些头。
///
/// 行标记形如 `# 1 "/usr/include/bits/mathcalls.h" 1`，末尾的 `1` = 进入新文件、
/// `2` = 返回上一层。维护一个栈，凡是「栈顶是目标头时进入的那个文件」就是深度 1。
/// 拿不到（clang 跑不起来等）就返回空集 —— 退化成原来的「只认本文件」，不会更糟。
fn 直接包含集(opts: &选项, 头文件: &Path) -> HashSet<PathBuf> {
    let mut 出: HashSet<PathBuf> = HashSet::new();
    let mut cmd = Command::new(clang程序());
    cmd.arg("-E");
    for d in &opts.包含目录 {
        cmd.arg("-I").arg(d);
    }
    for a in &opts.clang参数 {
        cmd.arg(a);
    }
    cmd.arg(头文件);
    let Ok(输出) = cmd.output() else {
        return 出;
    };
    let 文本 = String::from_utf8_lossy(&输出.stdout);

    let mut 栈: Vec<PathBuf> = Vec::new();
    for 行 in 文本.lines() {
        let Some(剩) = 行.trim_start().strip_prefix("# ") else {
            continue;
        };
        let Some(f) = 行标记文件(剩) else {
            continue;
        };
        // `<built-in>` / `<command line>` / `<scratch space>` 是预处理器的伪文件，
        // 不是真 include。当成直接包含放进来的话，Apple clang 预定义的 TARGET_OS_*
        // 那一大票会被认成「目标头文件自己的宏」—— 实测漏进 18 个。
        if f.starts_with('<') {
            continue;
        }
        let p = Path::new(&f);
        let 规范 = p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
        // 标记末尾的 1/2 是进栈/出栈；两者都没有就是同一文件内的行号重置。
        let 尾: Vec<&str> = 剩
            .rsplit('"')
            .next()
            .unwrap_or("")
            .split_whitespace()
            .collect();
        if 尾.contains(&"1") {
            if 栈.last().map(|t| 同一文件(&t.to_string_lossy(), 头文件)) == Some(true) {
                出.insert(规范.clone());
            }
            栈.push(规范);
        } else if 尾.contains(&"2") {
            栈.pop();
            // 返回后栈顶就是当前文件，行标记给的正是它，保持栈与文本同步。
            if 栈.last() != Some(&规范) {
                栈.push(规范);
            }
        } else if 栈.is_empty() {
            栈.push(规范); // 最开头那条 `# 1 "目标.h"`，没有 flag
        }
    }
    出
}

/// typedef 名 → 底层类型串。clang 只对「最外层是 typedef」的类型给
/// desugaredQualType：`uLong` 有，`Bytef *`（指针里面才是 typedef）没有。
/// 所以自己建一张表，按名字逐步展开。
fn 收typedef(顶层: &[Value]) -> HashMap<String, String> {
    let mut 表 = HashMap::new();
    收typedef递归(顶层, &mut 表);
    表
}

fn 收typedef递归(节点集: &[Value], 表: &mut HashMap<String, String>) {
    for n in 节点集 {
        if n.get("kind").and_then(|v| v.as_str()) == Some("TypedefDecl") {
            if let (Some(名), Some(t)) = (
                n.get("name").and_then(|v| v.as_str()),
                n.get("type")
                    .and_then(|t| t.get("qualType"))
                    .and_then(|v| v.as_str()),
            ) {
                表.entry(名.to_string()).or_insert_with(|| t.to_string());
            }
        }
        // 嵌在 extern "C" / 记录体里的 typedef 也要收。
        if let Some(内) = n.get("inner").and_then(|v| v.as_array()) {
            收typedef递归(内, 表);
        }
    }
}

fn 收函数(
    节点: &Value,
    别名: &HashMap<String, String>,
    opts: &选项,
    已见: &mut HashSet<String>,
    出: &mut Vec<外部函数>,
    跳过: &mut Vec<跳过项>,
) {
    let Some(名) = 节点.get("name").and_then(|v| v.as_str()) else {
        return;
    };
    if 名.starts_with("__") {
        return; // 编译器内建，不是库 API。
    }
    if !opts.前缀.is_empty() && !opts.前缀.iter().any(|p| 名.starts_with(p.as_str())) {
        return;
    }
    // 重复声明（头文件里前向声明 + 正式声明）只留第一条。
    if !已见.insert(名.to_string()) {
        return;
    }

    let 记跳过 = |跳过: &mut Vec<跳过项>, 原因: String| {
        跳过.push(跳过项 {
            名字: 名.to_string(),
            原因,
        })
    };

    if 节点.get("variadic").and_then(|v| v.as_bool()) == Some(true) {
        记跳过(
            跳过,
            "变参函数（`...`）—— qi 的外部块不支持变参".to_string(),
        );
        return;
    }
    if 节点.get("storageClass").and_then(|v| v.as_str()) == Some("static") {
        记跳过(
            跳过,
            "头文件里的 static 定义 —— 内部链接，库里没有这个符号".to_string(),
        );
        return;
    }
    if 有函数体(节点) {
        记跳过(
            跳过,
            "头文件里带函数体（inline 定义）—— 库里未必有外部符号".to_string(),
        );
        return;
    }

    let 函数类型 = 节点.get("type").map(取类型串).unwrap_or_default();
    let Some(返回串) = 拆返回类型(&函数类型.1) else {
        记跳过(跳过, format!("看不懂的函数类型 `{}`", 函数类型.1));
        return;
    };
    let 返回 = match 映射类型(&返回串, &函数类型.0, 别名, 位置::返回) {
        Ok(t) => t,
        Err(e) => {
            记跳过(跳过, format!("返回类型 {}", e));
            return;
        }
    };
    if matches!(返回, 映射::函数(..)) {
        记跳过(
            跳过,
            "返回函数指针 —— qi 外部块的返回位不收函数类型".to_string(),
        );
        return;
    }

    let mut 参数 = Vec::new();
    let 形参: Vec<&Value> = 节点
        .get("inner")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter(|p| p.get("kind").and_then(|v| v.as_str()) == Some("ParmVarDecl"))
                .collect()
        })
        .unwrap_or_default();
    for (i, p) in 形参.iter().enumerate() {
        let (原, 用) = p.get("type").map(取类型串).unwrap_or_default();
        let t = match 映射类型(&用, &原, 别名, 位置::参数) {
            Ok(t) => t,
            Err(e) => {
                记跳过(跳过, format!("第 {} 个参数 {}", i + 1, e));
                return;
            }
        };
        if t == 映射::空 {
            记跳过(跳过, format!("第 {} 个参数是 void", i + 1));
            return;
        }
        // C 允许不写形参名（`int f(int, char*)`），qi 的签名要名字，补一个。
        let 名 = p
            .get("name")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("arg{}", i + 1));
        参数.push((名, t));
    }

    出.push(外部函数 {
        名字: 名.to_string(),
        参数,
        返回,
        窄返回: 是窄有符号整数(&解别名(&返回串, 别名)),
    });
}

fn 有函数体(节点: &Value) -> bool {
    节点
        .get("inner")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .any(|c| c.get("kind").and_then(|v| v.as_str()) == Some("CompoundStmt"))
        })
        .unwrap_or(false)
}

fn 收枚举(节点: &Value, 已见: &mut HashSet<String>, 出: &mut Vec<常量项>) {
    let Some(内) = 节点.get("inner").and_then(|v| v.as_array()) else {
        return;
    };
    // C 的枚举：不写 `= n` 就是「上一个 + 1」，第一个默认 0。
    let mut 下一个: i64 = 0;
    for c in 内 {
        if c.get("kind").and_then(|v| v.as_str()) != Some("EnumConstantDecl") {
            continue;
        }
        let Some(名) = c.get("name").and_then(|v| v.as_str()) else {
            continue;
        };
        let 值 = 找显式值(c).unwrap_or(下一个);
        下一个 = 值.saturating_add(1);
        if 名.starts_with('_') || !已见.insert(名.to_string()) {
            continue;
        }
        出.push(常量项 {
            名字: 名.to_string(),
            值: 值.to_string(),
            来源: "枚举",
        });
    }
}

/// 枚举值写了 `= 表达式` 时，clang 会挂一个带 value 的 ConstantExpr。
fn 找显式值(节点: &Value) -> Option<i64> {
    if let Some(v) = 节点.get("value").and_then(|v| v.as_str()) {
        if let Ok(n) = v.parse::<i64>() {
            return Some(n);
        }
    }
    节点
        .get("inner")
        .and_then(|v| v.as_array())
        .and_then(|a| a.iter().find_map(找显式值))
}

// ───────────────────────── 排版 ─────────────────────────

fn 排版(
    opts: &选项,
    头文件: &Path,
    函数表: &[外部函数],
    常量表: &[常量项],
    跳过表: &[跳过项],
) -> String {
    let mut 出 = String::new();
    let 窄数 = 函数表.iter().filter(|f| f.窄返回).count();

    出.push_str("// 由 `qi 绑定` 自动生成 —— 请勿手改。头文件变了就重新生成一遍。\n");
    出.push_str(&format!("// 命令：{}\n", opts.命令行));
    出.push_str(&format!("// 头文件：{}\n", 头文件.display()));
    出.push_str(&format!(
        "// 生成日期：{}\n",
        chrono::Local::now().format("%Y-%m-%d")
    ));
    出.push_str(&format!(
        "// 链接：{}\n",
        if opts.库.is_empty() {
            "（库名为空，不额外链接）".to_string()
        } else {
            format!("外部 \"{}\"", opts.库)
        }
    ));
    出.push_str(&format!(
        "// 统计：函数 {} 个，常量 {} 个，跳过 {} 项。\n",
        函数表.len(),
        常量表.len(),
        跳过表.len()
    ));
    出.push_str("//\n");
    出.push_str("// 类型映射：\n");
    出.push_str("//   int / long / size_t / int64_t / enum ...  → 整数\n");
    出.push_str("//   float / double                            → 浮点数\n");
    出.push_str(
        "//   const char* 形参 / char* 返回             → 字符串（返回值在调用点拷成 qi 堆串）\n",
    );
    出.push_str("//   非 const 的 char* 形参                    → 指针（多半是输出缓冲区，别拿 qi 串去接）\n");
    出.push_str(
        "//   其余一切指针（struct* / void* / T*）      → 指针（不透明句柄，qi 不解引用）\n",
    );
    出.push_str("//   void 返回                                 → 空\n");
    出.push_str("//   函数指针参数                              → 函数(...): ...（C 回调槽）\n");
    出.push_str("//\n");
    if 窄数 > 0 {
        出.push_str(&format!(
            "// C 的 int 返回（本文件 {} 个，行尾标了 `// C:int 返回`）写成 `C整数`：\n",
            窄数
        ));
        出.push_str("//   qi 的 整数 是 64 位，C 的 int 只写寄存器低 32 位 —— 直接当 整数 接，\n");
        出.push_str("//   负数返回码会读成 4294967295 那类大正数。`C整数` 让编译器按 i32 收、\n");
        出.push_str("//   再符号扩展成 整数，负数照常是负数，调用方拿到的仍是普通 整数。\n");
        出.push_str("//\n");
    }
    if 跳过表.is_empty() {
        出.push_str("// 跳过清单：无。\n");
    } else {
        出.push_str(&format!(
            "// 跳过清单（{} 项，需要就照下面的原因手写补上）：\n",
            跳过表.len()
        ));
        for s in 跳过表 {
            出.push_str(&format!("//   - {}：{}\n", s.名字, s.原因));
        }
    }
    出.push('\n');

    出.push_str(&format!(
        "外部 \"{}\" {{\n",
        opts.库.replace('\\', "\\\\").replace('"', "\\\"")
    ));
    for f in 函数表 {
        let 参: Vec<String> = f
            .参数
            .iter()
            .map(|(n, t)| format!("{}: {}", n, t.写法()))
            .collect();
        // 窄有符号返回（C 的 int/short/enum…）：写成 `C整数`，编译器就按 i32 收
        // 再符号扩展。写 `整数` 的话 -1 会读成 4294967295 —— 光靠注释提醒挡不住。
        let 返回写法 = if f.窄返回 {
            "C整数".to_string()
        } else {
            f.返回.写法()
        };
        let 尾注 = if f.窄返回 { "  // C:int 返回" } else { "" };
        出.push_str(&format!(
            "    函数 {}({}): {};{}\n",
            f.名字,
            参.join(", "),
            返回写法,
            尾注
        ));
    }
    出.push_str("}\n");

    if !常量表.is_empty() {
        出.push('\n');
        出.push_str("// 常量（来自 #define 数字宏与 enum）\n");
        for c in 常量表 {
            出.push_str(&format!("常量 {} = {};  // {}\n", c.名字, c.值, c.来源));
        }
    }
    出
}
