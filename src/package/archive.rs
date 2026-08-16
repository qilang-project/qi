//! 包体打包 / 解包 / sha256 校验。
//!
//! 一个包版本 = 一个 tar.gz：包根的 qi.toml + 全部源码，tar 内路径都是**相对包根**
//! 的（没有 `包名-版本/` 顶层目录），解包时直接铺进 `qi_packages/<名称>/`。

use std::collections::BTreeSet;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use sha2::{Digest, Sha256};

/// 打包时排除的目录名（任意层级命中即整棵剪掉）。
///
/// `tests/` 在列表里是**发布减肥**：测试只对开发者有意义，装到别人的
/// qi_packages 里既占体积又会被编译器的模块扫描看见。本地开发完全不受影响
/// —— 只有 `qi 包 发布` 这一条路径走这个排除表。
pub const 排除目录: &[&str] = &[".git", "target", "target-wt", "qi_packages", "tests"];

/// 打包时排除的文件扩展名。
pub const 排除扩展名: &[&str] = &["o"];

/// 其他按名字排除的文件（构建产物 / 编辑器垃圾 / 本地来源标记）。
///
/// `.qi_registry.toml` / `.qi_source.toml` 是**本机安装记录**，不是包内容。
/// 不排掉的话，`qi_packages/<名称>/` 里那份装好的包再打一次得到的字节跟
/// 发布时不一样，sha256 对不上 —— 「重打一次能还原成同一指纹」这条自检就废了，
/// 更糟的是转手再发布会把上一次的来源信息带进新包。
pub const 排除文件: &[&str] = &[
    ".DS_Store",
    ".qi_registry.toml",
    ".qi_source.toml",
    "qi.lock",
];

/// 判断一个**相对包根**的路径是否该被排除。
pub fn 应排除(相对路径: &Path) -> bool {
    for 段 in 相对路径.components() {
        let Component::Normal(名) = 段 else {
            // `..` / 绝对路径前缀之类一律不收，防止打出越界的 tar
            return true;
        };
        let 名 = 名.to_string_lossy();
        if 排除目录.contains(&名.as_ref()) {
            return true;
        }
    }
    if let Some(文件名) = 相对路径
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
    {
        if 排除文件.contains(&文件名.as_str()) {
            return true;
        }
    }
    if let Some(扩展) = 相对路径
        .extension()
        .map(|s| s.to_string_lossy().to_string())
    {
        if 排除扩展名.contains(&扩展.as_str()) {
            return true;
        }
    }
    false
}

/// 把 `包根` 目录打成 tar.gz 字节。
///
/// 文件按路径**排序**后写入：同样的目录内容必须打出同样的字节，否则 sha256
/// 每次都变，「版本不可变」这条协议就无从校验（本地重打一次对不上服务端）。
pub fn 打包(包根: &Path) -> Result<Vec<u8>, String> {
    let 文件列表 = 收集文件(包根)?;
    if 文件列表.is_empty() {
        return Err(format!(
            "{} 里没有可打包的文件（qi.toml 和 .qi 源码都被排除了？）",
            包根.display()
        ));
    }

    let mut 编码器 = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::default()));
    for 相对 in &文件列表 {
        let 绝对 = 包根.join(相对);
        let mut 文件 = std::fs::File::open(&绝对)
            .map_err(|e| format!("打开 {} 失败: {}", 绝对.display(), e))?;
        let 元数据 = 文件
            .metadata()
            .map_err(|e| format!("读取 {} 元数据失败: {}", 绝对.display(), e))?;

        // 手搓 header 而不是 append_path_with_name：后者会把本机 mtime/uid/权限
        // 原样带进去，同样的源码在两台机器上打出的 sha256 就不一样了。
        let mut 头 = tar::Header::new_gnu();
        头.set_size(元数据.len());
        头.set_mode(if 可执行(&元数据) { 0o755 } else { 0o644 });
        头.set_mtime(0);
        头.set_uid(0);
        头.set_gid(0);
        头.set_entry_type(tar::EntryType::Regular);
        头.set_cksum();

        编码器
            .append_data(&mut 头, 相对, &mut 文件)
            .map_err(|e| format!("写入 tar 条目 {} 失败: {}", 相对.display(), e))?;
    }

    let gz = 编码器
        .into_inner()
        .map_err(|e| format!("收尾 tar 失败: {}", e))?;
    gz.finish().map_err(|e| format!("gzip 压缩失败: {}", e))
}

#[cfg(unix)]
fn 可执行(元数据: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    元数据.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn 可执行(_元数据: &std::fs::Metadata) -> bool {
    false
}

/// 递归收集包根下所有该打包的文件（相对路径，已排序去重）。
fn 收集文件(包根: &Path) -> Result<Vec<PathBuf>, String> {
    if !包根.is_dir() {
        return Err(format!("包目录不存在: {}", 包根.display()));
    }
    let mut 结果 = BTreeSet::new();
    for 条目 in walkdir::WalkDir::new(包根).follow_links(false) {
        let 条目 = 条目.map_err(|e| format!("遍历 {} 失败: {}", 包根.display(), e))?;
        if !条目.file_type().is_file() {
            continue;
        }
        let 相对 = 条目
            .path()
            .strip_prefix(包根)
            .map_err(|e| format!("计算相对路径失败: {}", e))?
            .to_path_buf();
        if 相对.as_os_str().is_empty() || 应排除(&相对) {
            continue;
        }
        结果.insert(相对);
    }
    Ok(结果.into_iter().collect())
}

/// 十六进制小写 sha256。
pub fn 算sha256(字节: &[u8]) -> String {
    let mut 摘要 = Sha256::new();
    摘要.update(字节);
    摘要
        .finalize()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect()
}

/// 把 tar.gz 字节解到 `目标目录`（目录会被先建出来）。
///
/// tar 里的路径**必须**是相对且不含 `..`：注册中心不是可信输入，
/// 一个 `../../.ssh/authorized_keys` 条目就能写出安装目录之外。
pub fn 解包(包体: &[u8], 目标目录: &Path) -> Result<usize, String> {
    std::fs::create_dir_all(目标目录)
        .map_err(|e| format!("创建 {} 失败: {}", 目标目录.display(), e))?;
    let 根 = 目标目录
        .canonicalize()
        .unwrap_or_else(|_| 目标目录.to_path_buf());

    let mut 归档 = tar::Archive::new(GzDecoder::new(包体));
    let mut 计数 = 0usize;
    for 条目 in 归档
        .entries()
        .map_err(|e| format!("读取 tar 条目失败（包体可能损坏）: {}", e))?
    {
        let mut 条目 = 条目.map_err(|e| format!("读取 tar 条目失败: {}", e))?;
        let 路径 = 条目
            .path()
            .map_err(|e| format!("tar 条目路径非法: {}", e))?
            .into_owned();

        // 越界判定必须**跨平台一致**：同一个包在 Linux 上装得下、在 Windows 上逃逸，
        // 是最难查的一类。所以不问宿主系统，只看路径长什么样。
        //
        // 只问 is_absolute() 不够：Windows 上 `/tmp/x` 没有盘符，is_absolute() 是
        // **false**，可 `根.join("/tmp/x")` 照样把 `根` 整个丢掉，解到 C:\tmp\x 去。
        // 所以直接看组件 —— RootDir（打头的 /）和 Prefix（C: 或 UNC）都算越界。
        //
        // 反斜杠也拦：tar 规定路径用 `/`，名字里带 `\` 在 Linux 上是一个普通文件名，
        // 到 Windows 就成了目录分隔符 —— 同一个包在两个平台解出不同结构，
        // `..\..\evil` 这种正好绕过按组件做的检查。
        let 有越界组件 = 路径.components().any(|c| {
            matches!(
                c,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        });
        let 串 = 路径.to_string_lossy();
        let 有反斜杠 = 串.contains('\\');
        // `C:/x` 在 Unix 上只是个叫 "C:" 的普通目录名（不是 Prefix），到 Windows
        // 才变成盘符。按形状认，两边都拒。
        let 有盘符 = 串.split('/').any(|段| {
            let b = 段.as_bytes();
            b.len() >= 2 && b[1] == b':' && b[0].is_ascii_alphabetic()
        });
        if 路径.is_absolute() || 有越界组件 || 有反斜杠 || 有盘符 {
            return Err(format!(
                "包体含越界路径 {}，拒绝解包（注册中心返回的内容不可信）",
                路径.display()
            ));
        }
        if !条目.header().entry_type().is_file() {
            continue;
        }

        let 目标 = 根.join(&路径);
        if let Some(父) = 目标.parent() {
            std::fs::create_dir_all(父)
                .map_err(|e| format!("创建 {} 失败: {}", 父.display(), e))?;
        }
        let mut 内容 = Vec::new();
        条目
            .read_to_end(&mut 内容)
            .map_err(|e| format!("读取 {} 内容失败: {}", 路径.display(), e))?;
        std::fs::write(&目标, &内容).map_err(|e| format!("写入 {} 失败: {}", 目标.display(), e))?;
        计数 += 1;
    }

    if 计数 == 0 {
        return Err("包体里一个文件都没有（不是合法的 qi 包）".to_string());
    }
    Ok(计数)
}

/// 从 tar.gz 字节里只把包根 `qi.toml` 抠出来（发布前自检 名称/版本 用）。
pub fn 读包内清单(包体: &[u8]) -> Result<Option<String>, String> {
    let mut 归档 = tar::Archive::new(GzDecoder::new(包体));
    for 条目 in 归档
        .entries()
        .map_err(|e| format!("读取 tar 条目失败: {}", e))?
    {
        let mut 条目 = 条目.map_err(|e| format!("读取 tar 条目失败: {}", e))?;
        let 路径 = 条目
            .path()
            .map_err(|e| format!("tar 条目路径非法: {}", e))?
            .into_owned();
        if 路径 == Path::new("qi.toml") {
            let mut 文本 = String::new();
            条目
                .read_to_string(&mut 文本)
                .map_err(|e| format!("读取包内 qi.toml 失败: {}", e))?;
            return Ok(Some(文本));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn 造包(根: &Path) {
        std::fs::create_dir_all(根.join("子目录")).unwrap();
        std::fs::create_dir_all(根.join("target")).unwrap();
        std::fs::create_dir_all(根.join("tests")).unwrap();
        std::fs::create_dir_all(根.join(".git")).unwrap();
        std::fs::write(根.join("qi.toml"), "[包]\n名称 = \"试验包\"\n").unwrap();
        std::fs::write(根.join("试验包.qi"), "包 试验包;\n").unwrap();
        std::fs::write(根.join("子目录/工具.qi"), "包 试验包.工具;\n").unwrap();
        std::fs::write(根.join("垃圾.o"), "obj").unwrap();
        std::fs::write(根.join("target/产物"), "bin").unwrap();
        std::fs::write(根.join("tests/测.qi"), "测试").unwrap();
        std::fs::write(根.join(".git/HEAD"), "ref").unwrap();
    }

    #[test]
    fn test_打包排除规则() {
        assert!(应排除(Path::new(".git/HEAD")));
        assert!(应排除(Path::new("target/debug/x")));
        assert!(应排除(Path::new("target-wt/x")));
        assert!(应排除(Path::new("qi_packages/别的包/a.qi")));
        assert!(应排除(Path::new("tests/断言.sh")));
        assert!(应排除(Path::new("深/一层/tests/x.qi")));
        assert!(应排除(Path::new("a.o")));
        assert!(应排除(Path::new(".DS_Store")));
        assert!(!应排除(Path::new("qi.toml")));
        assert!(!应排除(Path::new("子目录/工具.qi")));
        // 名字里含 tests 但不是整段的不该误伤
        assert!(!应排除(Path::new("mytests/x.qi")));
    }

    #[test]
    fn test_打包解包往返且排除生效() {
        let 临时 = tempfile::TempDir::new().unwrap();
        let 源 = 临时.path().join("源");
        造包(&源);

        let 包体 = 打包(&源).unwrap();
        let 目标 = 临时.path().join("装到这");
        let 数量 = 解包(&包体, &目标).unwrap();

        assert_eq!(数量, 3, "应只有 qi.toml / 试验包.qi / 子目录/工具.qi");
        assert!(目标.join("qi.toml").exists());
        assert!(目标.join("试验包.qi").exists());
        assert!(目标.join("子目录/工具.qi").exists());
        assert!(!目标.join("target").exists());
        assert!(!目标.join("tests").exists());
        assert!(!目标.join(".git").exists());
        assert!(!目标.join("垃圾.o").exists());
    }

    #[test]
    fn test_打包是确定性的() {
        // 同样内容打两次必须字节一致，否则 sha256 校验根本没法用
        let 临时 = tempfile::TempDir::new().unwrap();
        let 甲 = 临时.path().join("甲");
        let 乙 = 临时.path().join("乙");
        造包(&甲);
        std::thread::sleep(std::time::Duration::from_millis(1100)); // 让 mtime 不同
        造包(&乙);
        assert_eq!(算sha256(&打包(&甲).unwrap()), 算sha256(&打包(&乙).unwrap()));
    }

    #[test]
    fn test_读包内清单() {
        let 临时 = tempfile::TempDir::new().unwrap();
        let 源 = 临时.path().join("源");
        造包(&源);
        let 包体 = 打包(&源).unwrap();
        let 清单 = 读包内清单(&包体).unwrap().unwrap();
        assert!(清单.contains("试验包"));
    }

    /// 手搓一个 tar 条目，路径**绕过** tar crate 的写入侧检查。
    ///
    /// `tar::Builder` 自己就拒绝写 `..` 路径，所以没法用它造这个恶意样本。
    /// 但注册中心返回的字节不归我们生成 —— 别的语言写的服务端、或者被入侵的
    /// 服务端，完全可以吐出这种包。这里直接按 ustar 格式排字节：
    /// 0..100 路径、100..108 mode、124..136 size、148..156 校验和、156 类型。
    fn 手搓tar(路径: &str, 内容: &[u8]) -> Vec<u8> {
        let mut 头 = [0u8; 512];
        头[..路径.len()].copy_from_slice(路径.as_bytes());
        头[100..107].copy_from_slice(b"0000644");
        头[108..115].copy_from_slice(b"0000000"); // uid
        头[116..123].copy_from_slice(b"0000000"); // gid
        let 大小 = format!("{:011o}", 内容.len());
        头[124..135].copy_from_slice(大小.as_bytes());
        头[136..147].copy_from_slice(b"00000000000"); // mtime
        头[156] = b'0'; // 普通文件
        头[257..262].copy_from_slice(b"ustar");
        头[263..265].copy_from_slice(b"00");
        // 校验和：把 148..156 当成空格算总和，再写回八进制
        头[148..156].copy_from_slice(b"        ");
        let 和: u32 = 头.iter().map(|b| *b as u32).sum();
        let 和文本 = format!("{:06o}\0 ", 和);
        头[148..156].copy_from_slice(和文本.as_bytes());

        let mut 归档 = 头.to_vec();
        归档.extend_from_slice(内容);
        归档.resize(归档.len().div_ceil(512) * 512, 0); // 补齐块
        归档.extend_from_slice(&[0u8; 1024]); // 结尾两个空块

        let mut 压缩 = GzEncoder::new(Vec::new(), Compression::default());
        use std::io::Write;
        压缩.write_all(&归档).unwrap();
        压缩.finish().unwrap()
    }

    #[test]
    fn test_拒绝越界路径() {
        let 临时 = tempfile::TempDir::new().unwrap();

        // 相对越界：../ 想写到安装目录之外
        let 包体 = 手搓tar("../越界.txt", "坏东西".as_bytes());
        let 错 = 解包(&包体, &临时.path().join("目标甲")).unwrap_err();
        assert!(错.contains("越界"), "实际: {}", 错);
        assert!(!临时.path().join("越界.txt").exists(), "不能真写出去");

        // 深一层的 ../ 同样要拦
        let 包体 = 手搓tar("子目录/../../越界.txt", "坏东西".as_bytes());
        assert!(解包(&包体, &临时.path().join("目标乙")).is_err());

        // 绝对路径也不收。注意这条在 Windows 上曾漏网：`/tmp/…` 没盘符，
        // is_absolute() 是 false，可 join 进去照样把安装目录整个丢掉。
        let 包体 = 手搓tar("/tmp/越界.txt", "坏东西".as_bytes());
        assert!(解包(&包体, &临时.path().join("目标丙")).is_err());

        // 带盘符的绝对路径（Windows 形态）
        let 包体 = 手搓tar("C:/越界.txt", "坏东西".as_bytes());
        assert!(解包(&包体, &临时.path().join("目标丁")).is_err());

        // 反斜杠：Linux 上是一个普通文件名，Windows 上是目录分隔符 —— 两边都拒
        let 包体 = 手搓tar("..\\越界.txt", "坏东西".as_bytes());
        assert!(解包(&包体, &临时.path().join("目标戊")).is_err());
    }

    #[test]
    fn test_手搓tar本身是合法的() {
        // 上面那条断言要有意义，前提是这个手搓样本除了路径以外都合法 ——
        // 否则「解包失败」可能只是因为 tar 头写坏了，跟越界检查无关。
        let 包体 = 手搓tar("正常.txt", "内容".as_bytes());
        let 临时 = tempfile::TempDir::new().unwrap();
        let 目标 = 临时.path().join("目标");
        assert_eq!(解包(&包体, &目标).unwrap(), 1);
        assert_eq!(
            std::fs::read_to_string(目标.join("正常.txt")).unwrap(),
            "内容"
        );
    }

    #[test]
    fn test_sha256() {
        assert_eq!(
            算sha256(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
