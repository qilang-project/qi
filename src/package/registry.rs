//! 注册中心 HTTP 客户端 —— 协议见 `docs/包管理设计.md`。
//!
//! 五个端点（线上协议键一律英文，中文只出现在给人看的提示里）：
//! - `GET  /api/v1/packages`                       包列表
//! - `GET  /api/v1/packages/{name}`                包详情（含全部版本）
//! - `GET  /api/v1/packages/{name}/{version}`      单版本元数据（sha256 从这里来）
//! - `GET  /api/v1/packages/{name}/{version}/download`  tar.gz 包体
//! - `PUT  /api/v1/packages/{name}/{version}`      发布，Bearer token
//!
//! 中文包名走 percent-encode 进路径段。

use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};
use serde::Deserialize;

/// 默认注册中心地址（可用 `QI_REGISTRY` 覆盖，测试指本地假服务）。
pub const DEFAULT_REGISTRY: &str = "https://pkg.qilang.org";

/// 发布 token 的环境变量名。
pub const TOKEN_ENV: &str = "QI_REGISTRY_TOKEN";

/// URL **路径段**需要转义的字符集。
///
/// 只用 `CONTROLS` 不够：包名里若出现 `/ ? #` 会把路径结构撕开（`/` 变成多一层，
/// `?` 后面成 query），空格在 HTTP 请求行里直接是语法错。中文是非 ASCII，
/// `utf8_percent_encode` 本来就会按 UTF-8 字节逐个 %XX，不需要额外声明。
const PATH_SEGMENT: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'`')
    .add(b'{')
    .add(b'}')
    .add(b'/')
    .add(b'%');

/// 把一个路径段（包名 / 版本号）转义成可安全拼进 URL 的形式。
pub fn 转义路径段(raw: &str) -> String {
    utf8_percent_encode(raw, PATH_SEGMENT).to_string()
}

/// 当前生效的注册中心基地址（去掉尾部 `/`，方便拼接）。
pub fn 注册中心地址() -> String {
    let raw = std::env::var("QI_REGISTRY").unwrap_or_default();
    let raw = if raw.trim().is_empty() {
        DEFAULT_REGISTRY.to_string()
    } else {
        raw.trim().to_string()
    };
    raw.trim_end_matches('/').to_string()
}

/// 包列表里的一条（`GET /api/v1/packages`）。
#[derive(Debug, Clone, Deserialize)]
pub struct 包摘要 {
    pub name: String,
    #[serde(default)]
    pub latest: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub downloads: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct 包列表响应 {
    #[serde(default)]
    packages: Vec<包摘要>,
}

/// 单版本元数据（`GET /api/v1/packages/{name}/{version}`）。
#[derive(Debug, Clone, Deserialize)]
pub struct 版本元数据 {
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub sha256: String,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub uploaded_at: Option<String>,
}

/// 服务端错误响应体 `{"error":"人话原因"}`。
#[derive(Debug, Deserialize)]
struct 错误响应 {
    #[serde(default)]
    error: Option<String>,
}

/// 同步 HTTP 客户端。用 reqwest blocking：本 crate 已经依赖它（LLM 调用），
/// 不给包管理再引第二个 HTTP 栈。rustls-tls 保证交叉编译不拉系统 C 库。
pub struct 注册中心 {
    基地址: String,
    客户端: reqwest::blocking::Client,
}

impl 注册中心 {
    /// 按环境变量 / 默认值构造。
    pub fn 新建() -> Result<Self, String> {
        Self::新建于(&注册中心地址())
    }

    /// 指定基地址构造（测试用）。
    pub fn 新建于(基地址: &str) -> Result<Self, String> {
        let 客户端 = reqwest::blocking::Client::builder()
            // 发布大包和慢网都可能超过默认 30s；下载同理。给足 5 分钟，
            // 真挂了 TCP 层自己会先报错，不会白等。
            .timeout(std::time::Duration::from_secs(300))
            .user_agent(concat!("qi-pkg/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;
        Ok(Self {
            基地址: 基地址.trim_end_matches('/').to_string(),
            客户端,
        })
    }

    /// 当前基地址（写 qi.lock 的 `来源` 字段用）。
    pub fn 地址(&self) -> &str {
        &self.基地址
    }

    fn 拼(&self, 尾巴: &str) -> String {
        format!("{}{}", self.基地址, 尾巴)
    }

    /// `GET /api/v1/packages`
    pub fn 列出包(&self) -> Result<Vec<包摘要>, String> {
        let url = self.拼("/api/v1/packages");
        let 响应 = self
            .客户端
            .get(&url)
            .send()
            .map_err(|e| 网络错误(&url, e))?;
        let 正文 = 读正文(响应, &url)?;
        let 解析: 包列表响应 = serde_json::from_slice(&正文)
            .map_err(|e| format!("注册中心返回的包列表不是预期 JSON: {}", e))?;
        Ok(解析.packages)
    }

    /// `GET /api/v1/packages/{name}/{version}` —— 404 返回 `Ok(None)`，
    /// 让调用方能区分「没这个版本」和「服务挂了」。
    pub fn 取版本元数据(
        &self,
        名称: &str,
        版本: &str,
    ) -> Result<Option<版本元数据>, String> {
        let url = self.拼(&format!(
            "/api/v1/packages/{}/{}",
            转义路径段(名称),
            转义路径段(版本)
        ));
        let 响应 = self
            .客户端
            .get(&url)
            .send()
            .map_err(|e| 网络错误(&url, e))?;
        if 响应.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let 正文 = 读正文(响应, &url)?;
        let 元数据: 版本元数据 = serde_json::from_slice(&正文)
            .map_err(|e| format!("注册中心返回的版本元数据不是预期 JSON: {}", e))?;
        Ok(Some(元数据))
    }

    /// `GET /api/v1/packages/{name}/{version}/download` —— 返回 tar.gz 裸字节。
    pub fn 下载(&self, 名称: &str, 版本: &str) -> Result<Vec<u8>, String> {
        let url = self.拼(&format!(
            "/api/v1/packages/{}/{}/download",
            转义路径段(名称),
            转义路径段(版本)
        ));
        let 响应 = self
            .客户端
            .get(&url)
            .send()
            .map_err(|e| 网络错误(&url, e))?;
        读正文(响应, &url)
    }

    /// `PUT /api/v1/packages/{name}/{version}` —— body 是 **base64(tar.gz)**。
    ///
    /// 为什么不是裸字节：注册中心是用 qi-web 写的（吃自己的狗粮），qi-runtime
    /// 的 web FFI 用 `RcStr::from_bytes` 收请求体，为维持 C 串约定会把内嵌的
    /// 0x00 逐个换成空格。tar.gz 里 0x00 遍地都是，裸传上去**长度不变、只坏字节**
    /// —— 坏得极其安静，只有 sha256 对不上才露馅。base64 后全是可打印 ASCII，
    /// 绕开这个转换。下载方向不受影响（服务端 sendfile 无损），仍是裸字节。
    ///
    /// 409（版本已存在）与 401/403（token 不对）单独翻成人话，因为这两种是
    /// 发布时最常撞上的，笼统的「HTTP 409」对使用者毫无指导意义。
    pub fn 发布(
        &self, 名称: &str, 版本: &str, 包体: Vec<u8>, 令牌: &str
    ) -> Result<(), String> {
        use base64::Engine;
        let 编码后 = base64::engine::general_purpose::STANDARD.encode(&包体);

        let url = self.拼(&format!(
            "/api/v1/packages/{}/{}",
            转义路径段(名称),
            转义路径段(版本)
        ));
        let 响应 = self
            .客户端
            .put(&url)
            .header("Authorization", format!("Bearer {}", 令牌))
            // 服务端不看 Content-Type，标成 text/plain 是为了让抓包的人一眼
            // 看出这是 base64 文本而不是二进制包体
            .header("Content-Type", "text/plain; charset=utf-8")
            .body(编码后)
            .send()
            .map_err(|e| 网络错误(&url, e))?;

        let 状态 = 响应.status();
        if 状态.is_success() {
            return Ok(());
        }

        let 原因 = 取错误原因(响应);
        match 状态.as_u16() {
            409 => Err(format!(
                "该版本已发布过，版本号不可复用，请升版本：{} {} 已存在于 {}\n  改 qi.toml 的 [包] 版本 再发一次（服务端原因: {}）",
                名称, 版本, self.基地址, 原因
            )),
            401 | 403 => Err(format!(
                "发布被拒：token 无效或无权限（HTTP {}）\n  检查环境变量 {} 是否设成了注册中心签发的发布 token（服务端原因: {}）",
                状态.as_u16(),
                TOKEN_ENV,
                原因
            )),
            400 => Err(format!(
                "发布被拒：包内容与 URL 不符（HTTP 400）\n  服务端会校验包里 qi.toml 的 名称/版本 跟发布地址一致（服务端原因: {}）",
                原因
            )),
            码 => Err(format!("发布失败（HTTP {}）: {}", 码, 原因)),
        }
    }
}

/// 非 2xx 时把 `{"error":"…"}` 抽出来；抽不出就退回原始正文片段。
fn 取错误原因(响应: reqwest::blocking::Response) -> String {
    let 正文 = 响应.bytes().map(|b| b.to_vec()).unwrap_or_default();
    if let Ok(错误响应 {
        error: Some(原因)
    }) = serde_json::from_slice::<错误响应>(&正文)
    {
        return 原因;
    }
    let 文本 = String::from_utf8_lossy(&正文);
    let 文本 = 文本.trim();
    if 文本.is_empty() {
        "（服务端未给出原因）".to_string()
    } else {
        文本.chars().take(200).collect()
    }
}

/// 统一读正文：非 2xx 一律转成带人话原因的 Err。
fn 读正文(响应: reqwest::blocking::Response, url: &str) -> Result<Vec<u8>, String> {
    let 状态 = 响应.status();
    if !状态.is_success() {
        let 原因 = 取错误原因(响应);
        return Err(format!(
            "请求 {} 失败（HTTP {}）: {}",
            url,
            状态.as_u16(),
            原因
        ));
    }
    响应
        .bytes()
        .map(|b| b.to_vec())
        .map_err(|e| format!("读取 {} 响应体失败: {}", url, e))
}

fn 网络错误(url: &str, e: reqwest::Error) -> String {
    format!(
        "无法连接注册中心 {}: {}\n  检查网络，或用环境变量 QI_REGISTRY 指向可达的注册中心",
        url, e
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_转义中文包名() {
        // 中文按 UTF-8 字节逐个 %XX（大写十六进制）
        assert_eq!(转义路径段("海龟"), "%E6%B5%B7%E9%BE%9F");
        assert_eq!(转义路径段("0.1.0"), "0.1.0");
        assert_eq!(转义路径段("qi-web"), "qi-web");
        // 结构性字符必须转义，否则会撕开路径
        assert_eq!(转义路径段("a/b"), "a%2Fb");
        assert_eq!(转义路径段("a b"), "a%20b");
        assert_eq!(转义路径段("a?b"), "a%3Fb");
        assert_eq!(转义路径段("a#b"), "a%23b");
        // 已经含 % 的名字不能二次歧义
        assert_eq!(转义路径段("a%b"), "a%25b");
    }

    #[test]
    fn test_注册中心地址默认与覆盖() {
        // 不设环境变量时（本测试进程未设）走默认值
        std::env::remove_var("QI_REGISTRY");
        assert_eq!(注册中心地址(), DEFAULT_REGISTRY);
        std::env::set_var("QI_REGISTRY", "http://127.0.0.1:43510/");
        assert_eq!(注册中心地址(), "http://127.0.0.1:43510");
        std::env::set_var("QI_REGISTRY", "   ");
        assert_eq!(注册中心地址(), DEFAULT_REGISTRY);
        std::env::remove_var("QI_REGISTRY");
    }

    #[test]
    fn test_基地址去尾斜杠() {
        let 中心 = 注册中心::新建于("http://example.com/").unwrap();
        assert_eq!(中心.地址(), "http://example.com");
        assert_eq!(
            中心.拼("/api/v1/packages"),
            "http://example.com/api/v1/packages"
        );
    }
}
