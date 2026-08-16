//! 注册中心包的下载 / 校验 / 落地。
//!
//! 落地布局是**扁平**的：`<项目根>/qi_packages/<别名>/`，跟编译器原有的
//! 第三方包解析路径一致（见 lib.rs 的 `resolve_package_path_from_root`）。
//!
//! 每个装好的包根写一个 [`标记文件名`]，记 名称/版本/sha256/来源。有它才能做到：
//! - **幂等**：再装一次时先比对标记，一致就跳过，不白下一遍
//! - **写 qi.lock**：lock 的 sha256 从这里读，不用重新算包体

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::archive;
use super::registry::注册中心;

/// 装好的包根里记录来源的元数据文件名（与远程 git 依赖的 `.qi_source.toml` 平行）。
pub const 标记文件名: &str = ".qi_registry.toml";

/// 安装标记的内容。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct 安装标记 {
    #[serde(rename = "名称")]
    pub 名称: String,
    #[serde(rename = "版本")]
    pub 版本: String,
    #[serde(rename = "sha256")]
    pub sha256: String,
    #[serde(rename = "来源")]
    pub 来源: String,
}

/// 读取已装包的标记（不存在 / 读不动 → None）。
///
/// 跟 qi.toml / qi.lock 走同一条中文键归一化：写出来的是带引号的形式，
/// 但这文件躺在包目录里，人看见了顺手改成裸键 `名称 = "…"` 是很自然的事，
/// 那在 TOML 1.0 里非法 —— 不归一化的话表现是「莫名其妙每次都重装」。
pub fn 读标记(包目录: &Path) -> Option<安装标记> {
    let 内容 = std::fs::read_to_string(包目录.join(标记文件名)).ok()?;
    toml::from_str(&crate::package::normalize_manifest_text(&内容)).ok()
}

fn 写标记(包目录: &Path, 标记: &安装标记) -> Result<(), String> {
    let 内容 = toml::to_string_pretty(标记).map_err(|e| format!("序列化安装标记失败: {}", e))?;
    std::fs::write(包目录.join(标记文件名), 内容)
        .map_err(|e| format!("写入安装标记失败 {}: {}", 包目录.display(), e))
}

/// 一次安装的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum 安装结果 {
    /// 已装好且 sha256 一致 —— 跳过
    已是(String),
    /// 本次真装了（可能是首装，也可能是内容对不上后重装）
    已装 {
        版本: String,
        sha256: String,
        文件数: usize,
    },
}

/// 安装一个注册中心包到 `包目录`（幂等）。
///
/// `锁定sha256` 来自 qi.lock：给了就以它为准 —— lock 的意义就是把版本内容钉死，
/// 注册中心事后改了同一个版本号的包体（协议禁止但要防），或者 lock 被人改坏，
/// 都必须当场停下来而不是默默装个别的东西。
pub fn 安装一个包(
    中心: &注册中心,
    名称: &str,
    版本: &str,
    包目录: &Path,
    锁定sha256: Option<&str>,
) -> Result<安装结果, String> {
    // ── 幂等短路：目录在、标记在、版本对、sha256 对上就直接跳过 ──
    if let Some(标记) = 读标记(包目录) {
        let 期望 = 锁定sha256.unwrap_or(标记.sha256.as_str());
        if 标记.版本 == 版本 && 标记.sha256 == 期望 && 包目录.join("qi.toml").exists()
        {
            return Ok(安装结果::已是(标记.版本));
        }
    }

    // ── 先问元数据：拿权威 sha256，顺便把「没这个版本」跟「服务挂了」分开 ──
    let 元数据 = 中心.取版本元数据(名称, 版本)?.ok_or_else(|| {
        format!(
            "注册中心 {} 上没有 {} 的 {} 版本\n  用 `qi 包 搜索 {}` 看看有哪些包，或确认版本号",
            中心.地址(),
            名称,
            版本,
            名称
        )
    })?;

    if 元数据.sha256.trim().is_empty() {
        return Err(format!(
            "注册中心 {} 返回的 {} {} 版本元数据里没有 sha256，无法校验完整性，拒绝安装",
            中心.地址(),
            名称,
            版本
        ));
    }

    // lock 钉死的与注册中心当前给的不一致 —— 不下载就先停
    if let Some(锁定) = 锁定sha256 {
        if 锁定 != 元数据.sha256 {
            return Err(format!(
                "qi.lock 锁定的 {} {} 与注册中心当前的对不上，拒绝安装：\n  qi.lock : {}\n  注册中心: {}\n  版本内容一旦发布就不可变。要么 lock 被改过，要么注册中心换掉了同版本的包体。",
                名称, 版本, 锁定, 元数据.sha256
            ));
        }
    }

    let 包体 = 中心.下载(名称, 版本)?;
    let 实际 = archive::算sha256(&包体);
    if 实际 != 元数据.sha256 {
        return Err(format!(
            "{} {} 下载内容校验失败，已丢弃：\n  期望 sha256: {}\n  实际 sha256: {}\n  可能是传输损坏或注册中心数据不一致，重试一次；仍不符请报给注册中心维护者。",
            名称, 版本, 元数据.sha256, 实际
        ));
    }

    // ── 校验通过才动磁盘。先解到同级临时目录，成功后整体替换 ──
    // 直接往 包目录 里解会有半成品风险：解一半失败就留下个「看起来装好了、
    // 其实缺文件」的目录，下次幂等检查还可能因为标记不在而重装，但编译已经先炸了。
    let 父目录 = 包目录
        .parent()
        .ok_or_else(|| format!("无效的安装目录: {}", 包目录.display()))?;
    std::fs::create_dir_all(父目录)
        .map_err(|e| format!("创建 {} 失败: {}", 父目录.display(), e))?;

    let 临时目录 = 父目录.join(format!(
        ".{}.tmp{}",
        包目录
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "pkg".to_string()),
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&临时目录);

    let 落地 = (|| -> Result<usize, String> {
        let 文件数 = archive::解包(&包体, &临时目录)?;
        if !临时目录.join("qi.toml").exists() {
            return Err(format!(
                "{} {} 的包体里没有包根 qi.toml，不是合法的 qi 包",
                名称, 版本
            ));
        }
        写标记(
            &临时目录,
            &安装标记 {
                名称: 名称.to_string(),
                版本: 版本.to_string(),
                sha256: 元数据.sha256.clone(),
                来源: 中心.地址().to_string(),
            },
        )?;
        Ok(文件数)
    })();

    let 文件数 = match 落地 {
        Ok(n) => n,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&临时目录);
            return Err(e);
        }
    };

    // 内容对不上就是要**整体换掉**，不能只覆盖同名文件：旧版本多出来的
    // 文件留在那里，编译器的模块扫描照样能看见，会撞出莫名其妙的重复符号。
    let _ = std::fs::remove_dir_all(包目录);
    std::fs::rename(&临时目录, 包目录).map_err(|e| {
        let _ = std::fs::remove_dir_all(&临时目录);
        format!(
            "移动安装目录失败 {} -> {}: {}",
            临时目录.display(),
            包目录.display(),
            e
        )
    })?;

    Ok(安装结果::已装 {
        版本: 版本.to_string(),
        sha256: 元数据.sha256,
        文件数,
    })
}

/// 项目的 qi_packages 目录。
pub fn 包目录(项目根: &Path) -> PathBuf {
    项目根.join("qi_packages")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_标记往返() {
        let 临时 = tempfile::TempDir::new().unwrap();
        let 标记 = 安装标记 {
            名称: "试验包".to_string(),
            版本: "0.1.0".to_string(),
            sha256: "abc123".to_string(),
            来源: "http://127.0.0.1:43510".to_string(),
        };
        写标记(临时.path(), &标记).unwrap();
        assert_eq!(读标记(临时.path()).unwrap(), 标记);
    }

    #[test]
    fn test_没标记时读出_none() {
        let 临时 = tempfile::TempDir::new().unwrap();
        assert!(读标记(临时.path()).is_none());
    }
}
