//! `qi 绑定` 的类型翻译层 —— C 类型串 → qi 类型，以及 #define 的数字字面量。
//!
//! 跟 绑定生成.rs 分家的理由：那边全是「跟 clang 打交道」（起进程、走 AST JSON、
//! 追行标记、排版输出），这边全是纯字符串→类型的判断，没有任何 IO。
//! 拆开之后这一半可以完全靠单元测试盯住 —— 类型映射恰恰是最容易出静默错误的地方
//! （`Bytef *` 少展开一层就成了 字符串，C 往里一写就是内存损坏）。

use std::collections::HashMap;

use serde_json::Value;

/// C 类型翻过来的 qi 类型。只有这几种能出现在外部块里。
#[derive(Clone, PartialEq, Debug)]
pub(super) enum 映射 {
    整数,
    浮点数,
    字符串,
    指针,
    空,
    /// C 回调槽：`函数(形参...): 返回`
    函数(Vec<映射>, Box<映射>),
}

impl 映射 {
    pub(super) fn 写法(&self) -> String {
        match self {
            映射::整数 => "整数".to_string(),
            映射::浮点数 => "浮点数".to_string(),
            映射::字符串 => "字符串".to_string(),
            映射::指针 => "指针".to_string(),
            映射::空 => "空".to_string(),
            映射::函数(参, 返) => {
                let ps: Vec<String> = 参.iter().map(|p| p.写法()).collect();
                format!("函数({}): {}", ps.join(", "), 返.写法())
            }
        }
    }
}

// ───────────────────────── C 类型 → qi 类型 ─────────────────────────

/// 取 (原始 qualType, 该用的类型串)。clang 给了 desugaredQualType 就用它。
pub(super) fn 取类型串(t: &Value) -> (String, String) {
    let 原 = t
        .get("qualType")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let 用 = t
        .get("desugaredQualType")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| 原.clone());
    (原, 用)
}

/// 去掉限定词、把 `*` 的空白规整成 `T *` / `T **`。
///
/// 带括号的（函数指针 `int (*)(char *)`）原样放过 —— 这里的「把星号都挪到末尾」
/// 对它是毁灭性的：`int (*)(void *)` 会被压成 `int ( ) ( void ) **`，
/// 函数指针的形状就没了。这类串由 映射类型 单独走一条路，逐段再规范化。
fn 规范化(s: &str) -> String {
    if s.contains('(') {
        return s.split_whitespace().collect::<Vec<_>>().join(" ");
    }
    let mut 词: Vec<String> = Vec::new();
    let mut 星 = 0usize;
    for 片 in s.replace('*', " * ").split_whitespace() {
        match 片 {
            "const"
            | "volatile"
            | "restrict"
            | "__restrict"
            | "__restrict__"
            | "_Nullable"
            | "_Nonnull"
            | "_Null_unspecified"
            | "__unsafe_unretained"
            | "__strong" => {}
            "*" => 星 += 1,
            其他 => 词.push(其他.to_string()),
        }
    }
    let 基 = 词.join(" ");
    if 星 == 0 {
        基
    } else {
        format!("{} {}", 基, "*".repeat(星))
    }
}

/// 按 typedef 表把类型串展开到底。`Bytef *` → `unsigned char *`。
pub(super) fn 解别名(s: &str, 别名: &HashMap<String, String>) -> String {
    let mut 当前 = 规范化(s);
    for _ in 0..16 {
        let (基, 星) = 拆指针(&当前);
        if !是标识符(&基) {
            break;
        }
        let Some(底) = 别名.get(&基) else { break };
        let 新 = 规范化(底);
        // typedef struct Foo Foo; —— 展开后还是自己，停。
        let 展开 = if 星 == 0 {
            新.clone()
        } else {
            format!("{} {}", 新, "*".repeat(星))
        };
        if 展开 == 当前 {
            break;
        }
        当前 = 规范化(&展开);
    }
    当前
}

fn 拆指针(s: &str) -> (String, usize) {
    let 星 = s.chars().filter(|c| *c == '*').count();
    (s.replace('*', "").trim().to_string(), 星)
}

fn 是标识符(s: &str) -> bool {
    !s.is_empty()
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !s.chars().next().unwrap().is_ascii_digit()
}

const 浮点集: &[&str] = &["float", "double", "long double"];

const 整数集: &[&str] = &[
    "int",
    "signed",
    "signed int",
    "unsigned",
    "unsigned int",
    "char",
    "signed char",
    "unsigned char",
    "short",
    "short int",
    "signed short",
    "signed short int",
    "unsigned short",
    "unsigned short int",
    "long",
    "long int",
    "signed long",
    "signed long int",
    "unsigned long",
    "unsigned long int",
    "long long",
    "long long int",
    "signed long long",
    "signed long long int",
    "unsigned long long",
    "unsigned long long int",
    "_Bool",
    "bool",
    "wchar_t",
];

/// C 里宽度 < 64 位的有符号整数 —— 返回位会出 4294967295 那个坑。
const 窄有符号集: &[&str] = &[
    "int",
    "signed",
    "signed int",
    "char",
    "signed char",
    "short",
    "short int",
    "signed short",
    "signed short int",
    "wchar_t",
];

pub(super) fn 是窄有符号整数(s: &str) -> bool {
    let n = 规范化(s);
    窄有符号集.contains(&n.as_str()) || n.starts_with("enum ")
}

/// 类型出现的位置 —— 同一个 `char *` 在三个位置要翻成三种东西。
#[derive(Clone, Copy, PartialEq)]
pub(super) enum 位置 {
    参数,
    返回,
    /// C 回调签名里（形参与返回都算）
    回调,
}

/// C 类型串 → qi 类型。
///
/// `char *` 的三分法（这是本生成器唯一一处「不照字面翻」的地方）：
///   - 返回位：→ 字符串。C 给的裸串在调用点被拷进 qi 自己的堆串，只读不写，安全。
///   - 参数位且带 const（`const char *`）：→ 字符串。是输入串，C 保证不写。
///   - 参数位不带 const（`char *`）：→ 指针。这种十有八九是**输出缓冲区**
///     （`gzgets(file, buf, len)`），翻成 字符串 就等于把 qi 的 ARC 堆串
///     交给 C 去写 —— 字面量还可能在只读段里，一写就是静默的内存损坏。
///     宁可让人用 malloc + 内存:: 原语自己配缓冲，也不留这个坑。
///   - 回调签名里：一律 指针。qi 的 字符串 是带隐藏 ARC header 的堆对象，
///     让 qi 回调按 字符串 接住 C 递来的裸 `const char*`，第一次读 header
///     就越界到串数据前面去了（这条是编译器 校验回调签名 定的，不是这里加的）。
pub(super) fn 映射类型(
    用: &str,
    原: &str,
    别名: &HashMap<String, String>,
    位: 位置,
) -> Result<映射, String> {
    let 回调内 = 位 == 位置::回调;
    // va_list 在 arm64 上 desugar 成 char*，放过去会被当成字符串，必须先拦。
    if 原.contains("va_list") || 用.contains("va_list") {
        return Err("是 va_list —— 没有对应的 qi 类型".to_string());
    }
    let s = 解别名(用, 别名);

    if s.contains("(^)") {
        return Err("是 Objective-C block —— 不是 C 函数指针".to_string());
    }
    if let Some(i) = s.find("(*)") {
        if 回调内 {
            return Err("是嵌套函数指针 —— 回调签名里放不下".to_string());
        }
        let 返回串 = s[..i].trim().to_string();
        let 尾 = &s[i + 3..];
        let 参数串 = 括号内容(尾).ok_or_else(|| format!("函数指针类型 `{}` 看不懂", s))?;
        let 返回 = 映射类型(&返回串, &返回串, 别名, 位置::回调)?;
        let mut 参数 = Vec::new();
        for p in 顶层逗号分割(&参数串) {
            let p = p.trim();
            if p == "void" || p.is_empty() {
                continue;
            }
            if p == "..." {
                return Err("是变参回调 —— C 回调槽不支持 `...`".to_string());
            }
            参数.push(映射类型(p, p, 别名, 位置::回调)?);
        }
        return Ok(映射::函数(参数, Box::new(返回)));
    }
    // 数组形参在 C 里退化成指针，clang 一般已经给成指针；兜个底。
    if s.contains('[') {
        return Ok(映射::指针);
    }
    if s.ends_with('*') {
        let (基, 星) = 拆指针(&s);
        if 星 == 1 && 基 == "char" && !回调内 {
            let 只读 = 位 == 位置::返回 || 指针指向常量(用) || 指针指向常量(原);
            if 只读 {
                return Ok(映射::字符串);
            }
        }
        return Ok(映射::指针);
    }
    if s == "void" {
        return Ok(映射::空);
    }
    if 浮点集.contains(&s.as_str()) {
        return Ok(映射::浮点数);
    }
    if 整数集.contains(&s.as_str()) || s.starts_with("enum ") {
        return Ok(映射::整数);
    }
    if s.starts_with("struct ") || s.starts_with("union ") {
        return Err(format!(
            "是按值传的 {} —— 生成器 v1 不产出结构体声明，请手写（见 示例/基础/外部函数v2）",
            s
        ));
    }
    Err(format!("是不支持的 C 类型 `{}`", s))
}

/// 指针**指向的东西**是不是 const。看最后一个 `*` 前面有没有 const：
/// `const char *`（指向常量）→ 真；`char *const`（指针本身是常量，内容可写）→ 假。
fn 指针指向常量(s: &str) -> bool {
    match s.rfind('*') {
        Some(i) => s[..i].contains("const"),
        None => false,
    }
}

/// 从函数类型串 `RET (ARGS)` 里切出 RET：从尾巴上那个 `)` 反向配对找到它的 `(`。
pub(super) fn 拆返回类型(函数类型: &str) -> Option<String> {
    let s = 函数类型.trim();
    if !s.ends_with(')') {
        return None;
    }
    let 字节: Vec<char> = s.chars().collect();
    let mut 深 = 0i32;
    for i in (0..字节.len()).rev() {
        match 字节[i] {
            ')' => 深 += 1,
            '(' => {
                深 -= 1;
                if 深 == 0 {
                    let ret: String = 字节[..i].iter().collect();
                    let ret = ret.trim().to_string();
                    // `int (*(int))(void)` 这种返回函数指针的，前缀里还带括号，判掉。
                    if ret.is_empty() || ret.contains('(') {
                        return None;
                    }
                    return Some(ret);
                }
            }
            _ => {}
        }
    }
    None
}

/// 取开头那个 `(...)` 里的内容（要求 s 以 `(` 开头）。
fn 括号内容(s: &str) -> Option<String> {
    let s = s.trim_start();
    let 字节: Vec<char> = s.chars().collect();
    if 字节.first() != Some(&'(') {
        return None;
    }
    let mut 深 = 0i32;
    for (i, c) in 字节.iter().enumerate() {
        match c {
            '(' => 深 += 1,
            ')' => {
                深 -= 1;
                if 深 == 0 {
                    return Some(字节[1..i].iter().collect());
                }
            }
            _ => {}
        }
    }
    None
}

fn 顶层逗号分割(s: &str) -> Vec<String> {
    let mut 出 = Vec::new();
    let mut 深 = 0i32;
    let mut 当前 = String::new();
    for c in s.chars() {
        match c {
            '(' | '[' => {
                深 += 1;
                当前.push(c);
            }
            ')' | ']' => {
                深 -= 1;
                当前.push(c);
            }
            ',' if 深 == 0 => {
                出.push(std::mem::take(&mut 当前));
            }
            _ => 当前.push(c),
        }
    }
    if !当前.trim().is_empty() {
        出.push(当前);
    }
    出
}

// ───────────────────────── 宏值 ─────────────────────────

/// 只收「纯数字字面量」的宏。返回 qi 写得出来的十进制文本。
/// qi 没有 `0x` 字面量，十六/八进制在这里就换算成十进制。
pub(super) fn 数字字面量(原: &str) -> Option<String> {
    let mut s = 原.trim();
    if s.is_empty() {
        return None;
    }
    // 去掉行尾注释。
    if let Some(i) = s.find("/*") {
        s = s[..i].trim();
    }
    if let Some(i) = s.find("//") {
        s = s[..i].trim();
    }
    // 层层剥掉包裹的括号：`(-1)` → `-1`。
    let mut 当前 = s.to_string();
    loop {
        let t = 当前.trim().to_string();
        if t.starts_with('(')
            && t.ends_with(')')
            && 括号内容(&t).map(|c| c.len() + 2) == Some(t.len())
        {
            当前 = 括号内容(&t)?;
            continue;
        }
        当前 = t;
        break;
    }
    let mut t = 当前.trim();
    let mut 负 = false;
    while let Some(r) = t.strip_prefix('-').or_else(|| t.strip_prefix('+')) {
        if t.starts_with('-') {
            负 = !负;
        }
        t = r.trim();
    }
    if t.is_empty() {
        return None;
    }
    // 浮点：含小数点或指数。
    let 像浮点 = t.contains('.')
        || (!t.starts_with("0x") && !t.starts_with("0X") && (t.contains('e') || t.contains('E')));
    if 像浮点 {
        let 去后缀 = t.trim_end_matches(['f', 'F', 'l', 'L']);
        let v: f64 = 去后缀.parse().ok()?;
        let v = if 负 { -v } else { v };
        if !v.is_finite() {
            return None;
        }
        // qi 没有科学计数法字面量（`1.4e16` 直接语法错误，math.h 的 X_TLOSS 就是），
        // 所以用 Display 展开成纯十进制；Display 对 f64 从不输出指数形式，
        // 而 Debug（`{:?}`）会，别换回去。整数值再补个 .0 才是浮点字面量。
        let mut 文本 = format!("{}", v);
        if !文本.contains('.') {
            文本.push_str(".0");
        }
        return Some(文本);
    }
    let 去后缀 = t.trim_end_matches(['u', 'U', 'l', 'L']);
    if 去后缀.is_empty() {
        return None;
    }
    let v: i128 = if let Some(h) = 去后缀
        .strip_prefix("0x")
        .or_else(|| 去后缀.strip_prefix("0X"))
    {
        i128::from_str_radix(h, 16).ok()?
    } else if let Some(b) = 去后缀
        .strip_prefix("0b")
        .or_else(|| 去后缀.strip_prefix("0B"))
    {
        i128::from_str_radix(b, 2).ok()?
    } else if 去后缀.len() > 1 && 去后缀.starts_with('0') {
        i128::from_str_radix(&去后缀[1..], 8).ok()?
    } else {
        去后缀.parse::<i128>().ok()?
    };
    let v = if 负 { -v } else { v };
    // qi 的 整数 是 i64，装不下的（0xFFFFFFFFFFFFFFFF 这类）不给。
    if v > i64::MAX as i128 || v < i64::MIN as i128 {
        return None;
    }
    Some(v.to_string())
}

#[cfg(test)]
mod 测 {
    use super::*;

    fn 空表() -> HashMap<String, String> {
        HashMap::new()
    }

    #[test]
    fn 标量映射() {
        let m = 空表();
        // size_t / int64_t 这类由 clang 给 desugaredQualType（或走 typedef 表），
        // 到这里已经是 unsigned long / long long 了，见 typedef展开。
        for c in [
            "int",
            "unsigned",
            "unsigned long",
            "long long",
            "char",
            "_Bool",
        ] {
            assert_eq!(映射类型(c, c, &m, 位置::参数).unwrap(), 映射::整数, "{}", c);
        }
        assert_eq!(
            映射类型("double", "double", &m, 位置::参数).unwrap(),
            映射::浮点数
        );
        assert_eq!(映射类型("void", "void", &m, 位置::返回).unwrap(), 映射::空);
    }

    #[test]
    fn 指针与字符串() {
        let m = 空表();
        assert_eq!(
            映射类型("const char *", "const char *", &m, 位置::参数).unwrap(),
            映射::字符串
        );
        // 回调签名里 char* 必须降成 指针（qi 的 字符串 是带 ARC header 的堆对象）
        assert_eq!(
            映射类型("const char *", "const char *", &m, 位置::回调).unwrap(),
            映射::指针
        );
        // 非 const 的 char* 形参多半是输出缓冲区（gzgets 的 buf），翻成 字符串
        // 就等于把 qi 的 ARC 堆串交给 C 去写 —— 只给 指针。
        assert_eq!(
            映射类型("char *", "char *", &m, 位置::参数).unwrap(),
            映射::指针
        );
        // 返回位不管 const 与否都是 字符串：调用点会拷一份，只读，安全。
        assert_eq!(
            映射类型("char *", "char *", &m, 位置::返回).unwrap(),
            映射::字符串
        );
        // `char *const` 是「指针本身 const」，内容照样可写 → 仍是 指针
        assert_eq!(
            映射类型("char *const", "char *const", &m, 位置::参数).unwrap(),
            映射::指针
        );
        assert_eq!(
            映射类型("char **", "char **", &m, 位置::参数).unwrap(),
            映射::指针
        );
        assert_eq!(
            映射类型("unsigned char *", "unsigned char *", &m, 位置::参数).unwrap(),
            映射::指针
        );
        assert_eq!(
            映射类型("struct Foo *", "struct Foo *", &m, 位置::参数).unwrap(),
            映射::指针
        );
    }

    #[test]
    fn typedef展开() {
        let mut m = 空表();
        m.insert("Bytef".into(), "unsigned char".into());
        m.insert("uLong".into(), "unsigned long".into());
        m.insert("uLongf".into(), "uLong".into());
        m.insert("gzFile".into(), "struct gzFile_s *".into());
        assert_eq!(
            映射类型("Bytef *", "Bytef *", &m, 位置::参数).unwrap(),
            映射::指针
        );
        assert_eq!(
            映射类型("uLongf *", "uLongf *", &m, 位置::参数).unwrap(),
            映射::指针
        );
        assert_eq!(
            映射类型("uLong", "uLong", &m, 位置::参数).unwrap(),
            映射::整数
        );
        assert_eq!(
            映射类型("gzFile", "gzFile", &m, 位置::参数).unwrap(),
            映射::指针
        );
    }

    #[test]
    fn 自指typedef不死循环() {
        let mut m = 空表();
        m.insert("Foo".into(), "struct Foo".into());
        assert_eq!(解别名("Foo *", &m), "struct Foo *");
    }

    #[test]
    fn 函数指针typedef展开() {
        // zlib 的 in_func：typedef 后面藏着函数指针，展开时不能被「星号挪末尾」压坏
        let mut m = 空表();
        m.insert(
            "in_func".into(),
            "unsigned int (*)(void *, unsigned char **)".into(),
        );
        let t = 映射类型("in_func", "in_func", &m, 位置::参数).unwrap();
        assert_eq!(t.写法(), "函数(指针, 指针): 整数");
    }

    #[test]
    fn 函数指针参数() {
        let m = 空表();
        let t = 映射类型(
            "int (*)(const char *, void *)",
            "int (*)(const char *, void *)",
            &m,
            位置::参数,
        )
        .unwrap();
        assert_eq!(t.写法(), "函数(指针, 指针): 整数");
        let v = 映射类型("void (*)(void)", "void (*)(void)", &m, 位置::参数).unwrap();
        assert_eq!(v.写法(), "函数(): 空");
    }

    #[test]
    fn 结构体按值被拒() {
        let m = 空表();
        assert!(映射类型("struct Pair", "struct Pair", &m, 位置::参数).is_err());
        assert!(映射类型("va_list", "char *", &m, 位置::参数).is_err());
    }

    #[test]
    fn 返回类型切分() {
        assert_eq!(拆返回类型("int (int, int)").unwrap(), "int");
        assert_eq!(拆返回类型("const char *(void)").unwrap(), "const char *");
        assert_eq!(
            拆返回类型("void (int *, size_t, int (*)(const char *, void *), void *)").unwrap(),
            "void"
        );
        // 返回函数指针：判不出干净的返回类型，直接不要
        assert!(拆返回类型("int (*(int))(void)").is_none());
    }

    #[test]
    fn 宏字面量() {
        assert_eq!(数字字面量("0").unwrap(), "0");
        assert_eq!(数字字面量("(-1)").unwrap(), "-1");
        assert_eq!(数字字面量("0x12c0").unwrap(), "4800");
        assert_eq!(数字字面量("16U").unwrap(), "16");
        assert_eq!(数字字面量("1L").unwrap(), "1");
        assert_eq!(数字字面量("3.5").unwrap(), "3.5");
        // qi 没有科学计数法字面量，得展成十进制（math.h 的 X_TLOSS 就长这样）
        assert_eq!(
            数字字面量("1.414847550405688e16").unwrap(),
            "14148475504056880.0"
        );
        assert_eq!(数字字面量("2.0e0").unwrap(), "2.0");
        assert_eq!(数字字面量("0755").unwrap(), "493");
        assert!(数字字面量("\"1.2.12\"").is_none());
        assert!(数字字面量("some_call(x)").is_none());
    }

    #[test]
    fn 窄返回识别() {
        assert!(是窄有符号整数("int"));
        assert!(是窄有符号整数("short"));
        assert!(是窄有符号整数("enum Color"));
        assert!(!是窄有符号整数("long"));
        assert!(!是窄有符号整数("unsigned long"));
        assert!(!是窄有符号整数("double"));
    }
}
