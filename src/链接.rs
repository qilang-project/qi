//! 链接期的库解析 —— 把 `外部 "库名" { ... }` 的库名和 `--库路径` 变成链接器参数。
//!
//! 背景：以前库名只能变成 `-l<库名>`，而且没有任何 `-L` 控制 —— 于是 homebrew 装的库
//! （/opt/homebrew/lib）、自己 `ar rcs` 出来的 .a、任何不在系统默认路径里的东西都链不上，
//! `外部 "pq"` 全靠碰运气。这里补齐三种写法：
//!
//! | 写法                          | 链接参数              |
//! |-------------------------------|-----------------------|
//! | `外部 "m"`                    | `-lm`                 |
//! | `外部 "本地库/libfoo.a"`      | 直接把该文件放进命令行 |
//! | `外部 "framework:Accelerate"` | `-framework Accelerate`（仅 macOS）|
//!
//! 外加 `--库路径 <目录>`（可重复）与环境变量 `QI_LIBRARY_PATH`（PATH 式多路径）→ `-L<目录>`。

use std::path::{Path, PathBuf};

/// 一个外部块解析出来的链接目标。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum 链接项 {
    /// 按名字找：`-l<名>`，配合 `-L` 搜索路径。
    库名(String),
    /// 直链文件：绝对路径直接放进链接命令（不经过 `-l` 的命名约定）。
    文件(PathBuf),
    /// macOS framework：`-framework <名>`。
    框架(String),
}

/// 被当成「直链文件」的后缀。注意 `.so.1` 这类带版本号的写法不在此列 ——
/// 它含不含分隔符都可能，与其猜，不如让用户写全路径（含 `/` 一样会走直链分支）。
const 直链后缀: &[&str] = &["a", "dylib", "so", "dll", "o"];

/// framework 写法的两种前缀（英文的是文档写法，中文的顺手也认）。
const 框架前缀: &[&str] = &["framework:", "框架:"];

/// 解析一个 `外部 "..."` 的库名。
///
/// `源目录` 是**声明该外部块的那个 .qi 文件所在目录**，不是当前工作目录 ——
/// 相对路径的基准必须跟着源码走：同一份源码在仓库根 `qi run 项目/a.qi` 和在
/// 项目目录里 `qi run a.qi` 都得链到同一个 libfoo.a，否则「在别的目录下编译就挂」
/// 会变成常态。这跟导入解析用源文件位置找 qi_packages 是同一个道理。
///
/// 空串返回 `Ok(None)`（历史语义：不额外链接任何东西）。
pub fn 解析外部库名(原始: &str, 源目录: &Path) -> Result<Option<链接项>, String> {
    let 名 = 原始.trim();
    if 名.is_empty() {
        return Ok(None);
    }

    // ① framework:Name —— macOS 专属，非 mac 上到发参数那一步再报错（这里只管解析，
    //    因为交叉编译时「宿主是 mac、目标是 linux」也该报错，判断得看目标而非宿主）。
    for 前缀 in 框架前缀 {
        if let Some(余) = 名.strip_prefix(前缀) {
            let 框架名 = 余.trim();
            if 框架名.is_empty() {
                return Err(format!(
                    "外部 \"{}\"：`{}` 后面得跟 framework 名字，例如 \"framework:Accelerate\"。",
                    原始, 前缀
                ));
            }
            return Ok(Some(链接项::框架(框架名.to_string())));
        }
    }

    // ② 直链文件：含路径分隔符，或以库文件后缀结尾。
    if 是直链写法(名) {
        let 原路径 = PathBuf::from(名);
        let 全路径 = if 原路径.is_absolute() {
            原路径
        } else {
            源目录.join(原路径)
        };
        if !全路径.exists() {
            return Err(format!(
                "外部 \"{}\"：找不到这个库文件 —— {}\n\
                 （相对路径以**声明它的 .qi 源文件所在目录**为基准，不是当前工作目录）",
                原始,
                全路径.display()
            ));
        }
        // 规范化成绝对路径：链接命令的工作目录不保证等于源目录。
        let 全路径 = std::fs::canonicalize(&全路径).unwrap_or(全路径);
        return Ok(Some(链接项::文件(全路径)));
    }

    // ③ 普通库名 → -l<名>
    Ok(Some(链接项::库名(名.to_string())))
}

/// 库名看着像个文件路径吗？含路径分隔符，或后缀是已知的库文件后缀。
fn 是直链写法(名: &str) -> bool {
    if 名.contains('/') || 名.contains(std::path::MAIN_SEPARATOR) {
        return true;
    }
    Path::new(名)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| 直链后缀.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// 汇总 `-L` 搜索路径：CLI 的 `--库路径` 在前，环境变量 `QI_LIBRARY_PATH` 在后。
///
/// 优先级这么定的理由：命令行是「这一次编译」的意图，环境变量是「这台机器」的默认，
/// 前者更具体所以先搜。两边写了同一个目录只保留第一次出现，`-L` 重复无害但吵。
///
/// 存在性：**CLI 传的目录不存在直接报错**（人手写的，写错就该当场说）；
/// 环境变量里的不存在则静默跳过 —— 这是 PATH 类变量的惯例，一台机器上的
/// 通用配置不该因为某个可选目录没装就让所有编译失败。
pub fn 库搜索路径(cli路径: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    let mut 结果: Vec<PathBuf> = Vec::new();
    for p in cli路径 {
        if !p.exists() {
            return Err(format!(
                "--库路径 指向的目录不存在：{}\n（检查拼写；相对路径以当前工作目录为基准）",
                p.display()
            ));
        }
        if !p.is_dir() {
            return Err(format!(
                "--库路径 得是目录，但 {} 是个文件。\n\
                 （想直接链某个 .a/.dylib，把文件路径写进 外部 \"...\" 里即可）",
                p.display()
            ));
        }
        加入去重(&mut 结果, p.clone());
    }
    if let Ok(值) = std::env::var("QI_LIBRARY_PATH") {
        // PATH 式多路径（Unix `:`、Windows `;`）；split_paths 对单路径原样返回。
        for p in std::env::split_paths(&值) {
            if p.as_os_str().is_empty() || !p.is_dir() {
                continue;
            }
            加入去重(&mut 结果, p);
        }
    }
    Ok(结果)
}

fn 加入去重(列表: &mut Vec<PathBuf>, p: PathBuf) {
    if !列表.iter().any(|x| *x == p) {
        列表.push(p);
    }
}

/// 把搜索路径 + 链接项追加到链接命令上。`-L` 全部排在前面，再是各链接项。
///
/// `目标是mac` 看的是**编译目标**而非宿主：mac 上交叉编译到 Linux 时
/// `framework:` 一样是错的。
pub fn 追加链接参数(
    命令: &mut std::process::Command,
    搜索路径: &[PathBuf],
    链接项列表: &[链接项],
    目标是mac: bool,
) -> Result<(), String> {
    for p in 搜索路径 {
        命令.arg(format!("-L{}", p.display()));
    }
    for 项 in 链接项列表 {
        match 项 {
            链接项::库名(名) => {
                命令.arg(format!("-l{}", 名));
            }
            链接项::文件(路径) => {
                命令.arg(路径);
            }
            链接项::框架(名) => {
                if !目标是mac {
                    return Err(format!(
                        "外部 \"framework:{}\"：framework 是 macOS 专有的链接方式，\n\
                         当前编译目标不是 macOS。改用 `外部 \"{}\"`（-l）或换成该平台上的等价库。",
                        名, 名
                    ));
                }
                命令.arg("-framework").arg(名);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod 测试 {
    use super::*;

    #[test]
    fn 普通库名走短横l() {
        let d = std::env::temp_dir();
        assert_eq!(
            解析外部库名("m", &d).unwrap(),
            Some(链接项::库名("m".into()))
        );
        // 前后空白无所谓
        assert_eq!(
            解析外部库名("  pq  ", &d).unwrap(),
            Some(链接项::库名("pq".into()))
        );
    }

    #[test]
    fn 空串不链接() {
        assert_eq!(解析外部库名("   ", &std::env::temp_dir()).unwrap(), None);
    }

    #[test]
    fn framework前缀() {
        let d = std::env::temp_dir();
        assert_eq!(
            解析外部库名("framework:Accelerate", &d).unwrap(),
            Some(链接项::框架("Accelerate".into()))
        );
        assert_eq!(
            解析外部库名("框架:CoreFoundation", &d).unwrap(),
            Some(链接项::框架("CoreFoundation".into()))
        );
        assert!(解析外部库名("framework:", &d).is_err());
    }

    #[test]
    fn 直链相对路径以源目录为基准() {
        let 临时 = std::env::temp_dir().join(format!("qi链接测试_{}", std::process::id()));
        let 子 = 临时.join("本地库");
        std::fs::create_dir_all(&子).unwrap();
        let 库 = 子.join("libfoo.a");
        std::fs::write(&库, b"not really an archive").unwrap();

        let 项 = 解析外部库名("本地库/libfoo.a", &临时).unwrap().unwrap();
        match 项 {
            链接项::文件(p) => assert!(p.ends_with("本地库/libfoo.a"), "得到 {}", p.display()),
            其他 => panic!("应当解析成直链文件，实际 {:?}", 其他),
        }
        // 换个基准目录就找不到 —— 证明基准确实是源目录而非 CWD
        assert!(解析外部库名("本地库/libfoo.a", &std::env::temp_dir()).is_err());
        std::fs::remove_dir_all(&临时).ok();
    }

    #[test]
    fn 后缀也算直链且文件不存在要报人话() {
        let e = 解析外部库名("libfoo.a", &std::env::temp_dir()).unwrap_err();
        assert!(e.contains("找不到这个库文件"), "错误信息: {}", e);
        assert!(
            e.contains("源文件所在目录"),
            "错误信息应解释相对路径基准: {}",
            e
        );
    }

    #[test]
    fn 非mac目标遇到框架要报错() {
        let mut cmd = std::process::Command::new("true");
        let e =
            追加链接参数(&mut cmd, &[], &[链接项::框架("Accelerate".into())], false).unwrap_err();
        assert!(e.contains("macOS 专有"), "错误信息: {}", e);
    }

    #[test]
    fn 搜索路径去重且拒绝不存在的目录() {
        let d = std::env::temp_dir();
        let 路径 = 库搜索路径(&[d.clone(), d.clone()]).unwrap();
        assert_eq!(路径.iter().filter(|p| **p == d).count(), 1);
        let e = 库搜索路径(&[d.join("这个目录一定不存在_qi链接测试")]).unwrap_err();
        assert!(e.contains("不存在"), "错误信息: {}", e);
    }
}
