//! LLM 模块 FFI 接口
//!
//! 为 Qi 语言提供 C 接口的 LLM 调用函数

#![allow(non_snake_case)]

use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::ffi::{CStr, CString};
use std::io::Read;
use std::os::raw::c_char;
use std::sync::{Mutex, OnceLock};

// LLM 会话池
static LLM会话池: OnceLock<Mutex<HashMap<i64, LLM会话>>> = OnceLock::new();
static 会话计数器: OnceLock<Mutex<i64>> = OnceLock::new();
static LLM流池: OnceLock<Mutex<HashMap<i64, LLM流>>> = OnceLock::new();
static 流计数器: OnceLock<Mutex<i64>> = OnceLock::new();

fn 获取会话池() -> &'static Mutex<HashMap<i64, LLM会话>> {
    LLM会话池.get_or_init(|| Mutex::new(HashMap::new()))
}

fn 获取会话计数器() -> &'static Mutex<i64> {
    会话计数器.get_or_init(|| Mutex::new(0))
}

fn 获取流池() -> &'static Mutex<HashMap<i64, LLM流>> {
    LLM流池.get_or_init(|| Mutex::new(HashMap::new()))
}

fn 获取流计数器() -> &'static Mutex<i64> {
    流计数器.get_or_init(|| Mutex::new(0))
}

/// LLM 会话结构
#[derive(Debug, Clone)]
struct LLM会话 {
    /// API 端点
    端点: String,
    /// API 密钥
    密钥: Option<String>,
    /// 模型名称
    模型: String,
    /// 对话历史，使用 OpenAI-compatible message JSON 表示，便于保存 tool_calls/tool 结果
    历史: Vec<Value>,
    /// 可用工具定义
    工具列表: Vec<Value>,
    /// provider-safe 工具名 -> Qi 原始工具名
    工具名称映射: HashMap<String, String>,
    /// 配置参数
    配置: HashMap<String, String>,
    /// 提供商："openai"（默认）/"anthropic"/"gemini"。经 设置配置(会话,"provider",..) 切换。
    /// 历史始终以 OpenAI 消息格式为内部规范表示，请求时按提供商翻译形状。
    提供商: String,
}

impl LLM会话 {
    fn 创建(端点: String, 模型: String, 密钥: Option<String>) -> Self {
        Self {
            // 存原始端点（去尾斜杠），实际请求 URL 由 请求端点() 按提供商推导。
            端点: 端点.trim_end_matches('/').to_string(),
            密钥,
            模型,
            历史: Vec::new(),
            工具列表: Vec::new(),
            工具名称映射: HashMap::new(),
            配置: HashMap::new(),
            提供商: "openai".to_string(),
        }
    }

    /// 按提供商推导实际请求 URL：
    /// - openai：base + /chat/completions（已带则原样，兼容旧行为）
    /// - anthropic：base + /v1/messages（base 已到 /v1 则补 /messages）
    /// - gemini：base + /models/<model>:generateContent（已含 :generateContent 则原样）
    fn 请求端点(&self) -> String {
        let 基 = self.端点.trim_end_matches('/');
        match self.提供商.as_str() {
            "anthropic" => {
                if 基.ends_with("/messages") {
                    基.to_string()
                } else if 基.ends_with("/v1") {
                    format!("{}/messages", 基)
                } else {
                    format!("{}/v1/messages", 基)
                }
            }
            "gemini" => {
                if 基.contains(":generateContent") {
                    基.to_string()
                } else if 基.ends_with("/models") {
                    format!("{}/{}:generateContent", 基, self.模型)
                } else {
                    format!("{}/models/{}:generateContent", 基, self.模型)
                }
            }
            _ => {
                if 基.ends_with("/chat/completions") {
                    基.to_string()
                } else {
                    format!("{}/chat/completions", 基)
                }
            }
        }
    }

    /// 把配置里的 system 提示作为首条 system 消息插到 messages 最前面。
    /// 之前 set_config("system", ...) 只存进配置 map 却从不发出去 —— 系统提示
    /// 形同虚设。这里在构建请求体时统一注入（历史已含 system 时不重复）。
    fn 注入系统消息(&self, 消息列表: &mut Vec<Value>) {
        if let Some(系统) = self.配置.get("system") {
            if !系统.is_empty() {
                let 已有系统 = 消息列表
                    .first()
                    .and_then(|m| m.get("role"))
                    .and_then(|r| r.as_str())
                    == Some("system");
                if !已有系统 {
                    消息列表.insert(0, json!({ "role": "system", "content": 系统 }));
                }
            }
        }
    }

    /// 组装 OpenAI 规范格式消息列表（内部统一表示）：历史 + 可选新 user 消息 + 系统消息。
    fn 组装消息(&self, 用户内容: Option<Value>) -> Vec<Value> {
        let mut 消息列表 = self.历史.clone();
        if let Some(内容) = 用户内容 {
            消息列表.push(json!({
                "role": "user",
                "content": 内容
            }));
        }
        self.注入系统消息(&mut 消息列表);
        消息列表
    }

    fn 构建请求体(&self, 提示: &str, 流式: bool, 使用工具: bool) -> Value {
        self.构建请求体带内容(Some(json!(提示)), 流式, 使用工具)
    }

    fn 构建继续请求体(&self, 使用工具: bool) -> Value {
        self.构建请求体带内容(None, false, 使用工具)
    }

    /// 按提供商分支构造请求体。用户内容 为 None 表示 continue（历史已含最新消息）。
    fn 构建请求体带内容(
        &self,
        用户内容: Option<Value>,
        流式: bool,
        使用工具: bool,
    ) -> Value {
        let 消息列表 = self.组装消息(用户内容);
        match self.提供商.as_str() {
            "anthropic" => self.构建Anthropic请求体(消息列表, 流式, 使用工具),
            "gemini" => self.构建Gemini请求体(消息列表, 使用工具),
            _ => self.构建OpenAI请求体(消息列表, 流式, 使用工具),
        }
    }

    /// OpenAI chat completions 形状（现状路径，字段不变）。
    fn 构建OpenAI请求体(
        &self, 消息列表: Vec<Value>, 流式: bool, 使用工具: bool
    ) -> Value {
        let mut 请求体 = json!({
            "model": self.模型,
            "messages": 消息列表,
            "temperature": self.配置.get("temperature")
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(0.7),
            "max_tokens": self.配置.get("max_tokens")
                .and_then(|s| s.parse::<i32>().ok())
                .unwrap_or(2000),
        });

        if 流式 {
            请求体["stream"] = json!(true);
        }

        if 使用工具 && !self.工具列表.is_empty() {
            请求体["tools"] = Value::Array(self.工具列表.clone());
            请求体["tool_choice"] = json!("auto");
        }

        请求体
    }

    /// Anthropic Messages API 形状：顶层 model/max_tokens(必填)/messages/system(可选)/tools。
    fn 构建Anthropic请求体(
        &self, 消息列表: Vec<Value>, 流式: bool, 使用工具: bool
    ) -> Value {
        let (系统, 消息) = Self::转Anthropic消息(&消息列表);
        let mut 请求体 = json!({
            "model": self.模型,
            "max_tokens": self.配置.get("max_tokens")
                .and_then(|s| s.parse::<i32>().ok())
                .unwrap_or(2000),
            "messages": 消息,
            "temperature": self.配置.get("temperature")
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(0.7),
        });
        if let Some(s) = 系统 {
            if !s.is_empty() {
                请求体["system"] = json!(s);
            }
        }
        if 流式 {
            请求体["stream"] = json!(true);
        }
        if 使用工具 && !self.工具列表.is_empty() {
            let 工具: Vec<Value> = self.工具列表.iter().map(Self::转Anthropic工具).collect();
            请求体["tools"] = Value::Array(工具);
        }
        请求体
    }

    /// OpenAI 消息列表 → (顶层 system 文本, Anthropic messages)。
    fn 转Anthropic消息(消息列表: &[Value]) -> (Option<String>, Vec<Value>) {
        let mut 系统: Option<String> = None;
        let mut 消息 = Vec::new();
        for m in 消息列表 {
            let 角色 = m.get("role").and_then(|r| r.as_str()).unwrap_or("user");
            match 角色 {
                "system" => {
                    let 文本 = m
                        .get("content")
                        .and_then(|c| c.as_str())
                        .unwrap_or("")
                        .to_string();
                    系统 = Some(match 系统 {
                        Some(旧) => format!("{}\n{}", 旧, 文本),
                        None => 文本,
                    });
                }
                "tool" => {
                    let id = m.get("tool_call_id").and_then(|v| v.as_str()).unwrap_or("");
                    let 内容 = m.get("content").cloned().unwrap_or(json!(""));
                    消息.push(json!({
                        "role": "user",
                        "content": [{
                            "type": "tool_result",
                            "tool_use_id": id,
                            "content": 内容
                        }]
                    }));
                }
                "assistant" => {
                    let mut 块 = Vec::new();
                    if let Some(t) = m.get("content").and_then(|c| c.as_str()) {
                        if !t.is_empty() {
                            块.push(json!({"type": "text", "text": t}));
                        }
                    }
                    if let Some(调用列表) = m.get("tool_calls").and_then(|v| v.as_array()) {
                        for c in 调用列表 {
                            let id = c.get("id").cloned().unwrap_or(json!(""));
                            let 函数 = c.get("function");
                            let 名 = 函数
                                .and_then(|f| f.get("name"))
                                .cloned()
                                .unwrap_or(json!(""));
                            let 入参 = 函数
                                .and_then(|f| f.get("arguments"))
                                .and_then(|a| a.as_str())
                                .and_then(|s| serde_json::from_str::<Value>(s).ok())
                                .unwrap_or(json!({}));
                            块.push(json!({
                                "type": "tool_use",
                                "id": id,
                                "name": 名,
                                "input": 入参
                            }));
                        }
                    }
                    消息.push(json!({"role": "assistant", "content": 块}));
                }
                _ => {
                    let 内容 = m.get("content").cloned().unwrap_or(json!(""));
                    消息.push(json!({
                        "role": "user",
                        "content": Self::转Anthropic用户内容(内容)
                    }));
                }
            }
        }
        (系统, 消息)
    }

    /// user content 翻译：字符串原样；OpenAI 多模态数组 → Anthropic 块。
    fn 转Anthropic用户内容(内容: Value) -> Value {
        match 内容 {
            Value::Array(块列表) => Value::Array(
                块列表
                    .into_iter()
                    .map(|块| {
                        let 类型 = 块.get("type").and_then(|t| t.as_str()).unwrap_or("");
                        if 类型 == "image_url" {
                            let url = 块
                                .get("image_url")
                                .and_then(|i| i.get("url"))
                                .and_then(|u| u.as_str())
                                .unwrap_or("");
                            json!({
                                "type": "image",
                                "source": {"type": "url", "url": url}
                            })
                        } else {
                            块
                        }
                    })
                    .collect(),
            ),
            其他 => 其他,
        }
    }

    /// OpenAI 工具定义 → Anthropic：{name, description, input_schema}。
    fn 转Anthropic工具(工具: &Value) -> Value {
        let f = 工具.get("function").unwrap_or(工具);
        json!({
            "name": f.get("name").cloned().unwrap_or(json!("")),
            "description": f.get("description").cloned().unwrap_or(json!("")),
            "input_schema": f.get("parameters").cloned()
                .unwrap_or(json!({"type": "object", "properties": {}}))
        })
    }

    /// Gemini generateContent 形状：顶层 contents/systemInstruction/generationConfig。
    fn 构建Gemini请求体(&self, 消息列表: Vec<Value>, 使用工具: bool) -> Value {
        let (系统, 内容) = Self::转Gemini内容(&消息列表);
        let mut 请求体 = json!({
            "contents": 内容,
            "generationConfig": {
                "temperature": self.配置.get("temperature")
                    .and_then(|s| s.parse::<f64>().ok())
                    .unwrap_or(0.7),
                "maxOutputTokens": self.配置.get("max_tokens")
                    .and_then(|s| s.parse::<i32>().ok())
                    .unwrap_or(2000),
            },
        });
        if let Some(s) = 系统 {
            if !s.is_empty() {
                请求体["systemInstruction"] = json!({"parts": [{"text": s}]});
            }
        }
        if 使用工具 && !self.工具列表.is_empty() {
            let 声明: Vec<Value> = self
                .工具列表
                .iter()
                .map(|t| {
                    let f = t.get("function").unwrap_or(t);
                    json!({
                        "name": f.get("name").cloned().unwrap_or(json!("")),
                        "description": f.get("description").cloned().unwrap_or(json!("")),
                        "parameters": f.get("parameters").cloned()
                            .unwrap_or(json!({"type": "object", "properties": {}}))
                    })
                })
                .collect();
            请求体["tools"] = json!([{"functionDeclarations": 声明}]);
        }
        请求体
    }

    /// OpenAI 消息列表 → (systemInstruction 文本, Gemini contents)。
    fn 转Gemini内容(消息列表: &[Value]) -> (Option<String>, Vec<Value>) {
        let mut 系统: Option<String> = None;
        let mut 内容列表 = Vec::new();
        for m in 消息列表 {
            let 角色 = m.get("role").and_then(|r| r.as_str()).unwrap_or("user");
            match 角色 {
                "system" => {
                    let 文本 = m
                        .get("content")
                        .and_then(|c| c.as_str())
                        .unwrap_or("")
                        .to_string();
                    系统 = Some(match 系统 {
                        Some(旧) => format!("{}\n{}", 旧, 文本),
                        None => 文本,
                    });
                }
                "tool" => {
                    let 名 = m.get("name").and_then(|v| v.as_str()).unwrap_or("tool");
                    let 结果 = m.get("content").cloned().unwrap_or(json!(""));
                    内容列表.push(json!({
                        "role": "user",
                        "parts": [{
                            "functionResponse": {
                                "name": 名,
                                "response": {"result": 结果}
                            }
                        }]
                    }));
                }
                "assistant" => {
                    let mut 部件 =
                        Self::转Gemini部件(m.get("content").cloned().unwrap_or(Value::Null));
                    if let Some(调用列表) = m.get("tool_calls").and_then(|v| v.as_array()) {
                        for c in 调用列表 {
                            let 函数 = c.get("function");
                            let 名 = 函数
                                .and_then(|f| f.get("name"))
                                .cloned()
                                .unwrap_or(json!(""));
                            let 入参 = 函数
                                .and_then(|f| f.get("arguments"))
                                .and_then(|a| a.as_str())
                                .and_then(|s| serde_json::from_str::<Value>(s).ok())
                                .unwrap_or(json!({}));
                            部件.push(json!({"functionCall": {"name": 名, "args": 入参}}));
                        }
                    }
                    内容列表.push(json!({"role": "model", "parts": 部件}));
                }
                _ => {
                    内容列表.push(json!({
                        "role": "user",
                        "parts": Self::转Gemini部件(
                            m.get("content").cloned().unwrap_or(json!(""))
                        )
                    }));
                }
            }
        }
        (系统, 内容列表)
    }

    /// content → Gemini parts：字符串 → [{text}]；OpenAI 多模态数组 →
    /// text → {text}，image_url → {file_data:{file_uri}}。
    fn 转Gemini部件(内容: Value) -> Vec<Value> {
        match 内容 {
            Value::String(s) => {
                if s.is_empty() {
                    vec![]
                } else {
                    vec![json!({"text": s})]
                }
            }
            Value::Array(块列表) => 块列表
                .into_iter()
                .filter_map(|块| {
                    let 类型 = 块.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    match 类型 {
                        "text" => {
                            Some(json!({"text": 块.get("text").cloned().unwrap_or(json!(""))}))
                        }
                        "image_url" => {
                            let url = 块
                                .get("image_url")
                                .and_then(|i| i.get("url"))
                                .and_then(|u| u.as_str())
                                .unwrap_or("")
                                .to_string();
                            Some(json!({"file_data": {"file_uri": url}}))
                        }
                        _ => None,
                    }
                })
                .collect(),
            _ => vec![],
        }
    }

    /// 按提供商决定端点 + 鉴权头：
    /// openai: Authorization: Bearer <key>
    /// anthropic: x-api-key: <key> + anthropic-version: 2023-06-01
    /// gemini: x-goog-api-key: <key>
    fn 发送请求体(&self, 请求体: Value) -> Result<reqwest::blocking::Response, String> {
        use reqwest::blocking::Client;

        let 客户端 = Client::new();
        let mut 请求构建器 = 客户端
            .post(self.请求端点())
            .header("Content-Type", "application/json");

        match self.提供商.as_str() {
            "anthropic" => {
                if let Some(ref 密钥) = self.密钥 {
                    请求构建器 = 请求构建器.header("x-api-key", 密钥.as_str());
                }
                请求构建器 = 请求构建器.header("anthropic-version", "2023-06-01");
            }
            "gemini" => {
                if let Some(ref 密钥) = self.密钥 {
                    请求构建器 = 请求构建器.header("x-goog-api-key", 密钥.as_str());
                }
            }
            _ => {
                if let Some(ref 密钥) = self.密钥 {
                    请求构建器 = 请求构建器.header("Authorization", format!("Bearer {}", 密钥));
                }
            }
        }

        let 响应 = 请求构建器
            .json(&请求体)
            .send()
            .map_err(|e| format!("HTTP请求失败: {}", e))?;

        if !响应.status().is_success() {
            let 状态码 = 响应.status();
            let 错误文本 = 响应
                .text()
                .unwrap_or_else(|_| "无法读取错误响应".to_string());
            return Err(format!("API返回错误 {}: {}", 状态码, 错误文本));
        }

        Ok(响应)
    }

    /// 提取 assistant 消息并归一化为 OpenAI 消息形状（content 文本 + tool_calls）。
    fn 提取消息(&self, 响应体: &Value) -> Result<Value, String> {
        match self.提供商.as_str() {
            "anthropic" => Self::提取Anthropic消息(响应体),
            "gemini" => Self::提取Gemini消息(响应体),
            _ => 响应体
                .get("choices")
                .and_then(|choices| choices.get(0))
                .and_then(|choice| choice.get("message"))
                .cloned()
                .ok_or_else(|| "响应格式错误：无法提取 message".to_string()),
        }
    }

    /// Anthropic 响应：content 是块数组，text 块拼文本，tool_use 块 → tool_calls。
    fn 提取Anthropic消息(响应体: &Value) -> Result<Value, String> {
        let 块列表 = 响应体
            .get("content")
            .and_then(|c| c.as_array())
            .ok_or_else(|| "响应格式错误：无法提取 content 块".to_string())?;
        let mut 文本 = String::new();
        let mut 调用 = Vec::new();
        for 块 in 块列表 {
            match 块.get("type").and_then(|t| t.as_str()).unwrap_or("") {
                "text" => 文本.push_str(块.get("text").and_then(|t| t.as_str()).unwrap_or("")),
                "tool_use" => {
                    let 入参 = 块.get("input").cloned().unwrap_or(json!({}));
                    调用.push(json!({
                        "id": 块.get("id").cloned().unwrap_or(json!("")),
                        "type": "function",
                        "function": {
                            "name": 块.get("name").cloned().unwrap_or(json!("")),
                            "arguments": 入参.to_string()
                        }
                    }));
                }
                _ => {}
            }
        }
        let mut 消息 = json!({"role": "assistant"});
        消息["content"] = if 文本.is_empty() && !调用.is_empty() {
            Value::Null
        } else {
            json!(文本)
        };
        if !调用.is_empty() {
            消息["tool_calls"] = Value::Array(调用);
        }
        Ok(消息)
    }

    /// Gemini 响应：candidates[0].content.parts，text 拼文本，functionCall → tool_calls。
    fn 提取Gemini消息(响应体: &Value) -> Result<Value, String> {
        let 部件列表 = 响应体
            .get("candidates")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("content"))
            .and_then(|c| c.get("parts"))
            .and_then(|p| p.as_array())
            .ok_or_else(|| "响应格式错误：无法提取 candidates".to_string())?;
        let mut 文本 = String::new();
        let mut 调用 = Vec::new();
        for (序, 部件) in 部件列表.iter().enumerate() {
            if let Some(t) = 部件.get("text").and_then(|t| t.as_str()) {
                文本.push_str(t);
            }
            if let Some(fc) = 部件.get("functionCall") {
                let 入参 = fc.get("args").cloned().unwrap_or(json!({}));
                调用.push(json!({
                    "id": format!("call_{}", 序),
                    "type": "function",
                    "function": {
                        "name": fc.get("name").cloned().unwrap_or(json!("")),
                        "arguments": 入参.to_string()
                    }
                }));
            }
        }
        let mut 消息 = json!({"role": "assistant"});
        消息["content"] = if 文本.is_empty() && !调用.is_empty() {
            Value::Null
        } else {
            json!(文本)
        };
        if !调用.is_empty() {
            消息["tool_calls"] = Value::Array(调用);
        }
        Ok(消息)
    }

    fn 提取文本(消息: &Value) -> Result<String, String> {
        Ok(消息
            .get("content")
            .and_then(|content| content.as_str())
            .unwrap_or("")
            .to_string())
    }

    /// 发送HTTP请求到LLM API
    fn 调用API(&self, 提示: &str) -> Result<String, String> {
        let 请求体 = self.构建请求体(提示, false, false);
        let 响应体: Value = self
            .发送请求体(请求体)?
            .json()
            .map_err(|e| format!("解析响应失败: {}", e))?;

        let 消息 = self.提取消息(&响应体)?;
        Self::提取文本(&消息)
    }

    /// 多模态图像对话：文本 + 单图 URL。内部以 OpenAI 多模态 content 数组为规范表示，
    /// 各提供商在 构建请求体带内容 分支里翻译形状。返回 (规范 user content, 回答文本)。
    fn 调用图像API(&self, 提示: &str, 图像URL: &str) -> Result<(Value, String), String> {
        let 用户内容 = json!([
            {"type": "text", "text": 提示},
            {"type": "image_url", "image_url": {"url": 图像URL}}
        ]);
        let 请求体 = self.构建请求体带内容(Some(用户内容.clone()), false, false);
        let 响应体: Value = self
            .发送请求体(请求体)?
            .json()
            .map_err(|e| format!("解析响应失败: {}", e))?;

        let 消息 = self.提取消息(&响应体)?;
        let 文本 = Self::提取文本(&消息)?;
        Ok((用户内容, 文本))
    }

    /// 带工具定义发送请求，返回完整 assistant message JSON
    fn 调用工具API(&self, 提示: &str) -> Result<Value, String> {
        let 请求体 = self.构建请求体(提示, false, true);
        let 响应体: Value = self
            .发送请求体(请求体)?
            .json()
            .map_err(|e| format!("解析响应失败: {}", e))?;

        self.提取消息(&响应体)
    }

    /// 继续工具对话，通常在添加 tool 结果后调用
    fn 继续工具API(&self) -> Result<Value, String> {
        let 请求体 = self.构建继续请求体(true);
        let 响应体: Value = self
            .发送请求体(请求体)?
            .json()
            .map_err(|e| format!("解析响应失败: {}", e))?;

        self.提取消息(&响应体)
    }

    /// 打开流式响应
    fn 打开流(&self, 提示: &str) -> Result<reqwest::blocking::Response, String> {
        let 请求体 = self.构建请求体(提示, true, false);
        self.发送请求体(请求体)
    }

    /// 流式 + 工具：流式请求里带上 tools，模型可在流中吐 tool_calls。
    fn 打开流带工具(&self, 提示: &str) -> Result<reqwest::blocking::Response, String> {
        let 请求体 = self.构建请求体(提示, true, true);
        self.发送请求体(请求体)
    }

    /// 工具结果回写后，流式继续推理（仍带 tools，可继续调工具或给最终答复）。
    fn 打开续传流(&self) -> Result<reqwest::blocking::Response, String> {
        let mut 请求体 = self.构建继续请求体(true);
        请求体["stream"] = json!(true);
        self.发送请求体(请求体)
    }
}

struct LLM流 {
    会话句柄: i64,
    提示: String,
    响应: reqwest::blocking::Response,
    缓冲: String,
    待返回: VecDeque<String>,
    完成: bool,
    累计: String,
    // 流式 + 工具调用：带工具时把分块到达的 tool_calls 按 index 累积成 (id, name, arguments)；
    // 是续传 = 这条流是 tool 结果后的 continue（历史里已有 user 消息，不再重复 push）。
    带工具: bool,
    是续传: bool,
    工具分块: Vec<(String, String, String)>,
}

impl LLM流 {
    fn 创建(
        会话句柄: i64,
        提示: String,
        响应: reqwest::blocking::Response,
        带工具: bool,
        是续传: bool,
    ) -> Self {
        Self {
            会话句柄,
            提示,
            响应,
            缓冲: String::new(),
            待返回: VecDeque::new(),
            完成: false,
            累计: String::new(),
            带工具,
            是续传,
            工具分块: Vec::new(),
        }
    }

    fn 读取下个片段(&mut self) -> Result<Option<String>, String> {
        loop {
            if let Some(片段) = self.待返回.pop_front() {
                self.累计.push_str(&片段);
                return Ok(Some(片段));
            }

            if self.完成 {
                return Ok(None);
            }

            let mut 字节 = [0u8; 4096];
            let 数量 = self
                .响应
                .read(&mut 字节)
                .map_err(|e| format!("读取流失败: {}", e))?;

            if 数量 == 0 {
                self.完成 = true;
                return Ok(None);
            }

            self.缓冲.push_str(&String::from_utf8_lossy(&字节[..数量]));
            self.解析缓冲();
        }
    }

    fn 查找事件分隔符(文本: &str) -> Option<(usize, usize)> {
        let 双换行 = 文本.find("\n\n").map(|pos| (pos, 2));
        let 双回车换行 = 文本.find("\r\n\r\n").map(|pos| (pos, 4));
        match (双换行, 双回车换行) {
            (Some(a), Some(b)) => Some(if a.0 < b.0 { a } else { b }),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        }
    }

    fn 解析缓冲(&mut self) {
        while let Some((位置, 分隔长度)) = Self::查找事件分隔符(&self.缓冲) {
            let 事件 = self.缓冲[..位置].to_string();
            self.缓冲.drain(..位置 + 分隔长度);

            let mut 数据 = String::new();
            for 行 in 事件.lines() {
                let 去空白 = 行.trim_start();
                if let Some(内容) = 去空白.strip_prefix("data:") {
                    数据.push_str(内容.trim());
                }
            }

            if 数据.is_empty() {
                continue;
            }

            if 数据 == "[DONE]" {
                self.完成 = true;
                continue;
            }

            if let Ok(JSON) = serde_json::from_str::<Value>(&数据) {
                let delta = JSON
                    .get("choices")
                    .and_then(|choices| choices.get(0))
                    .and_then(|choice| choice.get("delta"));

                if let Some(内容) = delta
                    .and_then(|d| d.get("content"))
                    .and_then(|content| content.as_str())
                {
                    if !内容.is_empty() {
                        self.待返回.push_back(内容.to_string());
                    }
                }

                // 流式 tool_calls 是增量的：第一块给 index/id/name，后续块拼 arguments。
                // 按 index 累积进 工具分块，结束后由 组装助手消息 拼成完整 assistant 消息。
                if let Some(数组) = delta
                    .and_then(|d| d.get("tool_calls"))
                    .and_then(|v| v.as_array())
                {
                    for tc in 数组 {
                        let idx = tc.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                        while self.工具分块.len() <= idx {
                            self.工具分块
                                .push((String::new(), String::new(), String::new()));
                        }
                        if let Some(id) = tc.get("id").and_then(|v| v.as_str()) {
                            if !id.is_empty() {
                                self.工具分块[idx].0 = id.to_string();
                            }
                        }
                        if let Some(函数) = tc.get("function") {
                            if let Some(名) = 函数.get("name").and_then(|v| v.as_str()) {
                                if !名.is_empty() {
                                    self.工具分块[idx].1 = 名.to_string();
                                }
                            }
                            if let Some(参) = 函数.get("arguments").and_then(|v| v.as_str()) {
                                self.工具分块[idx].2.push_str(参);
                            }
                        }
                    }
                }
            }
        }
    }

    /// 把累积的内容 + 工具分块拼成完整 assistant 消息（OpenAI 格式），供上层派发工具/入历史。
    fn 组装助手消息(&self) -> Value {
        let mut 消息 = json!({ "role": "assistant" });
        if self.累计.is_empty() {
            消息["content"] = Value::Null;
        } else {
            消息["content"] = json!(self.累计);
        }
        if !self.工具分块.is_empty() {
            let 调用: Vec<Value> = self
                .工具分块
                .iter()
                .map(|(id, 名, 参)| {
                    json!({
                        "id": id,
                        "type": "function",
                        "function": { "name": 名, "arguments": 参 }
                    })
                })
                .collect();
            消息["tool_calls"] = Value::Array(调用);
        }
        消息
    }
}

fn 转为C字符串指针(文本: String) -> *mut c_char {
    match CString::new(文本.replace('\0', "")) {
        Ok(c_str) => c_str.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

fn 工具安全名称(名称: &str) -> String {
    if !名称.is_empty()
        && 名称
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return 名称.to_string();
    }

    let mut 安全名称 = String::from("qi_tool_");
    for 字节 in 名称.as_bytes() {
        安全名称.push_str(&format!("{:02x}", 字节));
    }
    安全名称
}

fn 保存流历史(流: &LLM流) {
    let mut 会话池 = 获取会话池().lock().unwrap();
    if let Some(会话) = 会话池.get_mut(&流.会话句柄) {
        if 流.带工具 {
            // 工具流：assistant 消息要带 tool_calls，否则后续 continue / tool 结果无法对齐。
            // 续传流历史里已有 user 消息，不再重复 push。
            if !流.是续传 {
                会话
                    .历史
                    .push(json!({ "role": "user", "content": 流.提示 }));
            }
            会话.历史.push(流.组装助手消息());
        } else {
            // 纯文本流（流式问）：只存内容。
            if 流.累计.is_empty() {
                return;
            }
            会话
                .历史
                .push(json!({ "role": "user", "content": 流.提示 }));
            会话
                .历史
                .push(json!({ "role": "assistant", "content": 流.累计 }));
        }
    }
}

/// 创建LLM会话
///
/// 参数:
/// - endpoint: API端点 (如 "https://api.openai.com/v1/chat/completions")
/// - model: 模型名称 (如 "gpt-3.5-turbo")
/// - api_key: API密钥 (可选，传入空字符串表示不需要)
///
/// 返回: 会话句柄 (>0 成功, <0 失败)
#[no_mangle]
pub extern "C" fn qi_llm_create_session(
    endpoint: *const c_char,
    model: *const c_char,
    api_key: *const c_char,
) -> i64 {
    if endpoint.is_null() || model.is_null() {
        return -1;
    }

    unsafe {
        let 端点 = CStr::from_ptr(endpoint).to_string_lossy().to_string();
        let 模型 = CStr::from_ptr(model).to_string_lossy().to_string();
        let 密钥 = if api_key.is_null() {
            None
        } else {
            let key = CStr::from_ptr(api_key).to_string_lossy().to_string();
            if key.is_empty() {
                None
            } else {
                Some(key)
            }
        };

        let 会话 = LLM会话::创建(端点, 模型, 密钥);

        let mut 计数器 = 获取会话计数器().lock().unwrap();
        *计数器 += 1;
        let 句柄 = *计数器;

        let mut 会话池 = 获取会话池().lock().unwrap();
        会话池.insert(句柄, 会话);

        句柄
    }
}

/// 发送消息到LLM
///
/// 参数:
/// - session_handle: 会话句柄
/// - prompt: 用户提示
///
/// 返回: LLM响应文本 (需要调用 qi_llm_free_string 释放)
#[no_mangle]
pub extern "C" fn qi_llm_chat(session_handle: i64, prompt: *const c_char) -> *mut c_char {
    if prompt.is_null() {
        return std::ptr::null_mut();
    }

    let mut 会话池 = 获取会话池().lock().unwrap();

    if let Some(会话) = 会话池.get_mut(&session_handle) {
        unsafe {
            let 提示 = CStr::from_ptr(prompt).to_string_lossy().to_string();

            match 会话.调用API(&提示) {
                Ok(响应) => {
                    // 添加到历史
                    会话.历史.push(json!({
                        "role": "user",
                        "content": 提示.clone()
                    }));
                    会话.历史.push(json!({
                        "role": "assistant",
                        "content": 响应.clone()
                    }));

                    return 转为C字符串指针(响应);
                }
                Err(错误) => {
                    let 错误信息 = format!("LLM调用失败: {}", 错误);
                    return 转为C字符串指针(错误信息);
                }
            }
        }
    }

    std::ptr::null_mut()
}

/// 多模态图像对话：文本提示 + 单张图 URL。
///
/// 按会话 provider 构造带图消息（openai: image_url / anthropic: image+source.url /
/// gemini: file_data.file_uri）。返回回答文本（需 qi_llm_free_string 释放）。
#[no_mangle]
pub extern "C" fn qi_llm_chat_image(
    session_handle: i64,
    prompt: *const c_char,
    image_url: *const c_char,
) -> *mut c_char {
    if prompt.is_null() || image_url.is_null() {
        return 转为C字符串指针("LLM调用失败: 提示或图像URL为空".to_string());
    }

    let mut 会话池 = 获取会话池().lock().unwrap();

    if let Some(会话) = 会话池.get_mut(&session_handle) {
        unsafe {
            let 提示 = CStr::from_ptr(prompt).to_string_lossy().to_string();
            let 图像 = CStr::from_ptr(image_url).to_string_lossy().to_string();

            match 会话.调用图像API(&提示, &图像) {
                Ok((用户内容, 响应)) => {
                    会话.历史.push(json!({
                        "role": "user",
                        "content": 用户内容
                    }));
                    会话.历史.push(json!({
                        "role": "assistant",
                        "content": 响应.clone()
                    }));

                    return 转为C字符串指针(响应);
                }
                Err(错误) => {
                    return 转为C字符串指针(format!("LLM调用失败: {}", 错误));
                }
            }
        }
    }

    转为C字符串指针("LLM调用失败: 无效会话句柄".to_string())
}

/// 打开流式 LLM 对话。
///
/// 返回: 流句柄 (>0 成功, <0 失败)
#[no_mangle]
pub extern "C" fn qi_llm_stream_chat(session_handle: i64, prompt: *const c_char) -> i64 {
    if prompt.is_null() {
        return -1;
    }

    unsafe {
        let 提示 = CStr::from_ptr(prompt).to_string_lossy().to_string();

        let 会话克隆 = {
            let 会话池 = 获取会话池().lock().unwrap();
            match 会话池.get(&session_handle) {
                Some(会话) => 会话.clone(),
                None => return -1,
            }
        };

        let 响应 = match 会话克隆.打开流(&提示) {
            Ok(响应) => 响应,
            Err(_) => return -1,
        };

        let mut 计数器 = 获取流计数器().lock().unwrap();
        *计数器 += 1;
        let 流句柄 = *计数器;

        let mut 流池 = 获取流池().lock().unwrap();
        流池.insert(
            流句柄,
            LLM流::创建(session_handle, 提示, 响应, false, false),
        );

        流句柄
    }
}

/// 流式 + 工具：开一个带 tools 的流式对话。内容片段照常通过 读取流 流出，
/// tool_calls 在 runtime 侧按 index 累积，结束后用 流取助手消息 取完整 assistant 消息。
#[no_mangle]
pub extern "C" fn qi_llm_stream_chat_with_tools(session_handle: i64, prompt: *const c_char) -> i64 {
    if prompt.is_null() {
        return -1;
    }
    unsafe {
        let 提示 = CStr::from_ptr(prompt).to_string_lossy().to_string();
        let 会话克隆 = {
            let 会话池 = 获取会话池().lock().unwrap();
            match 会话池.get(&session_handle) {
                Some(会话) => 会话.clone(),
                None => return -1,
            }
        };
        let 响应 = match 会话克隆.打开流带工具(&提示) {
            Ok(响应) => 响应,
            Err(_) => return -1,
        };
        let 流句柄 = {
            let mut 计数器 = 获取流计数器().lock().unwrap();
            *计数器 += 1;
            *计数器
        };
        let mut 流池 = 获取流池().lock().unwrap();
        流池.insert(流句柄, LLM流::创建(session_handle, 提示, 响应, true, false));
        流句柄
    }
}

/// 工具结果回写后，流式继续推理。返回新的流句柄。
#[no_mangle]
pub extern "C" fn qi_llm_stream_continue_with_tools(session_handle: i64) -> i64 {
    let 会话克隆 = {
        let 会话池 = 获取会话池().lock().unwrap();
        match 会话池.get(&session_handle) {
            Some(会话) => 会话.clone(),
            None => return -1,
        }
    };
    let 响应 = match 会话克隆.打开续传流() {
        Ok(响应) => 响应,
        Err(_) => return -1,
    };
    let 流句柄 = {
        let mut 计数器 = 获取流计数器().lock().unwrap();
        *计数器 += 1;
        *计数器
    };
    let mut 流池 = 获取流池().lock().unwrap();
    流池.insert(
        流句柄,
        LLM流::创建(session_handle, String::new(), 响应, true, true),
    );
    流句柄
}

/// 取本轮流式对话拼好的 assistant 消息 JSON（含 content + tool_calls）。
/// 须在该流读到底（读取流 返回空）后调用；可直接喂给 has_tool_call / get_tool_call_* 派发。
#[no_mangle]
pub extern "C" fn qi_llm_stream_assistant_message(stream_handle: i64) -> *mut c_char {
    let 流池 = 获取流池().lock().unwrap();
    if let Some(流) = 流池.get(&stream_handle) {
        return 转为C字符串指针(流.组装助手消息().to_string());
    }
    转为C字符串指针(String::new())
}

/// 读取流式对话的下一个片段。结束时返回空字符串。
#[no_mangle]
pub extern "C" fn qi_llm_stream_next(stream_handle: i64) -> *mut c_char {
    let mut 流池 = 获取流池().lock().unwrap();

    if let Some(流) = 流池.get_mut(&stream_handle) {
        match 流.读取下个片段() {
            Ok(Some(片段)) => return 转为C字符串指针(片段),
            Ok(None) => return 转为C字符串指针(String::new()),
            Err(错误) => return 转为C字符串指针(format!("流式读取失败: {}", 错误)),
        }
    }

    std::ptr::null_mut()
}

/// 关闭流式对话，并把已经收到的内容写入会话历史。
#[no_mangle]
pub extern "C" fn qi_llm_stream_close(stream_handle: i64) -> i64 {
    let 流 = {
        let mut 流池 = 获取流池().lock().unwrap();
        流池.remove(&stream_handle)
    };

    if let Some(流) = 流 {
        保存流历史(&流);
        return 1;
    }

    -1
}

/// 注册工具定义。
///
/// parameters_json 应是 JSON Schema 对象字符串，例如 {"type":"object","properties":{...}}
#[no_mangle]
pub extern "C" fn qi_llm_register_tool(
    session_handle: i64,
    tool_name: *const c_char,
    tool_description: *const c_char,
    parameters_json: *const c_char,
) -> i64 {
    if tool_name.is_null() || tool_description.is_null() || parameters_json.is_null() {
        return -1;
    }

    let mut 会话池 = 获取会话池().lock().unwrap();
    if let Some(会话) = 会话池.get_mut(&session_handle) {
        unsafe {
            let 工具名 = CStr::from_ptr(tool_name).to_string_lossy().to_string();
            let 工具描述 = CStr::from_ptr(tool_description)
                .to_string_lossy()
                .to_string();
            let 参数文本 = CStr::from_ptr(parameters_json)
                .to_string_lossy()
                .to_string();
            let 安全工具名 = 工具安全名称(&工具名);
            let 参数结构 = serde_json::from_str::<Value>(&参数文本)
                .unwrap_or_else(|_| json!({"type": "object", "properties": {}}));

            会话.工具列表.push(json!({
                "type": "function",
                "function": {
                    "name": 安全工具名,
                    "description": format!("{}（Qi工具名：{}）", 工具描述, 工具名),
                    "parameters": 参数结构
                }
            }));
            会话.工具名称映射.insert(安全工具名, 工具名);
            return 1;
        }
    }

    -1
}

/// 清空会话工具定义。
#[no_mangle]
pub extern "C" fn qi_llm_clear_tools(session_handle: i64) -> i64 {
    let mut 会话池 = 获取会话池().lock().unwrap();
    if let Some(会话) = 会话池.get_mut(&session_handle) {
        会话.工具列表.clear();
        会话.工具名称映射.clear();
        return 1;
    }
    -1
}

/// 带工具调用能力的对话，返回 assistant message JSON。
#[no_mangle]
pub extern "C" fn qi_llm_chat_with_tools(
    session_handle: i64,
    prompt: *const c_char,
) -> *mut c_char {
    if prompt.is_null() {
        return std::ptr::null_mut();
    }

    let mut 会话池 = 获取会话池().lock().unwrap();
    if let Some(会话) = 会话池.get_mut(&session_handle) {
        unsafe {
            let 提示 = CStr::from_ptr(prompt).to_string_lossy().to_string();
            match 会话.调用工具API(&提示) {
                Ok(消息) => {
                    会话.历史.push(json!({
                        "role": "user",
                        "content": 提示
                    }));
                    会话.历史.push(消息.clone());
                    return 转为C字符串指针(消息.to_string());
                }
                Err(错误) => return 转为C字符串指针(format!("工具对话失败: {}", 错误)),
            }
        }
    }

    std::ptr::null_mut()
}

/// 添加工具结果后继续模型推理，返回 assistant message JSON。
#[no_mangle]
pub extern "C" fn qi_llm_continue_with_tools(session_handle: i64) -> *mut c_char {
    let mut 会话池 = 获取会话池().lock().unwrap();
    if let Some(会话) = 会话池.get_mut(&session_handle) {
        match 会话.继续工具API() {
            Ok(消息) => {
                会话.历史.push(消息.clone());
                return 转为C字符串指针(消息.to_string());
            }
            Err(错误) => return 转为C字符串指针(format!("继续工具对话失败: {}", 错误)),
        }
    }

    std::ptr::null_mut()
}

fn 解析工具调用(assistant_message_json: *const c_char) -> Option<Value> {
    解析工具调用按索引(assistant_message_json, 0)
}

/// 按 index 取 tool_calls[index]。支持 parallel tool_calls：模型一次返
/// N 个工具调用时，harness 循环 0..N 各取一个 dispatch。
fn 解析工具调用按索引(
    assistant_message_json: *const c_char,
    index: usize,
) -> Option<Value> {
    if assistant_message_json.is_null() {
        return None;
    }
    unsafe {
        let 文本 = CStr::from_ptr(assistant_message_json)
            .to_string_lossy()
            .to_string();
        let 消息: Value = serde_json::from_str(&文本).ok()?;
        消息
            .get("tool_calls")
            .and_then(|calls| calls.get(index))
            .cloned()
    }
}

/// 取 tool_calls 数组长度。模型一次返多个 parallel tool_calls 时用。
#[no_mangle]
pub extern "C" fn qi_llm_get_tool_call_count(assistant_message_json: *const c_char) -> i64 {
    if assistant_message_json.is_null() {
        return 0;
    }
    unsafe {
        let 文本 = CStr::from_ptr(assistant_message_json)
            .to_string_lossy()
            .to_string();
        let 消息: Value = match serde_json::from_str(&文本) {
            Ok(v) => v,
            Err(_) => return 0,
        };
        消息
            .get("tool_calls")
            .and_then(|v| v.as_array())
            .map(|a| a.len() as i64)
            .unwrap_or(0)
    }
}

/// 按 index 取第 i 个工具调用的 ID
#[no_mangle]
pub extern "C" fn qi_llm_get_tool_call_id_at(
    assistant_message_json: *const c_char,
    index: i64,
) -> *mut c_char {
    let 调用 = match 解析工具调用按索引(assistant_message_json, index as usize) {
        Some(c) => c,
        None => return std::ptr::null_mut(),
    };
    let id = 调用
        .get("id")
        .and_then(|i| i.as_str())
        .unwrap_or("")
        .to_string();
    转为C字符串指针(id)
}

/// 按 index 取第 i 个工具调用的名称（中文化）
#[no_mangle]
pub extern "C" fn qi_llm_get_tool_call_name_at(
    session_handle: i64,
    assistant_message_json: *const c_char,
    index: i64,
) -> *mut c_char {
    let 调用 = match 解析工具调用按索引(assistant_message_json, index as usize) {
        Some(c) => c,
        None => return std::ptr::null_mut(),
    };
    let 安全名称 = 调用
        .get("function")
        .and_then(|f| f.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or("")
        .to_string();
    let 会话池 = 获取会话池().lock().unwrap();
    let 名称 = 会话池
        .get(&session_handle)
        .and_then(|s| s.工具名称映射.get(&安全名称))
        .cloned()
        .unwrap_or(安全名称);
    转为C字符串指针(名称)
}

/// 按 index 取第 i 个工具调用的参数 JSON
#[no_mangle]
pub extern "C" fn qi_llm_get_tool_call_arguments_at(
    assistant_message_json: *const c_char,
    index: i64,
) -> *mut c_char {
    let 调用 = match 解析工具调用按索引(assistant_message_json, index as usize) {
        Some(c) => c,
        None => return std::ptr::null_mut(),
    };
    let 参数 = 调用.get("function").and_then(|f| f.get("arguments"));
    let 文本 = match 参数 {
        Some(Value::String(s)) => s.clone(),
        Some(v) => v.to_string(),
        None => String::new(),
    };
    转为C字符串指针(文本)
}

/// 判断 assistant message JSON 是否包含工具调用。
#[no_mangle]
pub extern "C" fn qi_llm_has_tool_call(assistant_message_json: *const c_char) -> i64 {
    if 解析工具调用(assistant_message_json).is_some() {
        1
    } else {
        0
    }
}

/// 获取第一个工具调用 ID。
#[no_mangle]
pub extern "C" fn qi_llm_get_tool_call_id(assistant_message_json: *const c_char) -> *mut c_char {
    let 调用 = match 解析工具调用(assistant_message_json) {
        Some(调用) => 调用,
        None => return std::ptr::null_mut(),
    };

    let 调用ID = 调用
        .get("id")
        .and_then(|id| id.as_str())
        .unwrap_or("")
        .to_string();

    转为C字符串指针(调用ID)
}

/// 获取第一个工具调用名称；返回 Qi 原始中文工具名。
#[no_mangle]
pub extern "C" fn qi_llm_get_tool_call_name(
    session_handle: i64,
    assistant_message_json: *const c_char,
) -> *mut c_char {
    let 调用 = match 解析工具调用(assistant_message_json) {
        Some(调用) => 调用,
        None => return std::ptr::null_mut(),
    };

    let 安全名称 = 调用
        .get("function")
        .and_then(|func| func.get("name"))
        .and_then(|name| name.as_str())
        .unwrap_or("")
        .to_string();

    let 会话池 = 获取会话池().lock().unwrap();
    let 名称 = 会话池
        .get(&session_handle)
        .and_then(|会话| 会话.工具名称映射.get(&安全名称))
        .cloned()
        .unwrap_or(安全名称);

    转为C字符串指针(名称)
}

/// 获取第一个工具调用参数 JSON。
#[no_mangle]
pub extern "C" fn qi_llm_get_tool_call_arguments(
    assistant_message_json: *const c_char,
) -> *mut c_char {
    let 调用 = match 解析工具调用(assistant_message_json) {
        Some(调用) => 调用,
        None => return std::ptr::null_mut(),
    };

    let 参数 = 调用.get("function").and_then(|func| func.get("arguments"));

    let 参数文本 = match 参数 {
        Some(Value::String(s)) => s.clone(),
        Some(value) => value.to_string(),
        None => String::new(),
    };

    转为C字符串指针(参数文本)
}

/// 添加工具执行结果到会话历史，供下一次工具对话继续推理。
#[no_mangle]
pub extern "C" fn qi_llm_add_tool_result(
    session_handle: i64,
    tool_call_id: *const c_char,
    tool_name: *const c_char,
    result: *const c_char,
) -> i64 {
    if tool_call_id.is_null() || result.is_null() {
        return -1;
    }

    let mut 会话池 = 获取会话池().lock().unwrap();
    if let Some(会话) = 会话池.get_mut(&session_handle) {
        unsafe {
            let 调用ID = CStr::from_ptr(tool_call_id).to_string_lossy().to_string();
            let 结果 = CStr::from_ptr(result).to_string_lossy().to_string();
            let 工具名 = if tool_name.is_null() {
                String::new()
            } else {
                CStr::from_ptr(tool_name).to_string_lossy().to_string()
            };
            let 安全工具名 = 工具安全名称(&工具名);

            let mut 工具消息 = json!({
                "role": "tool",
                "tool_call_id": 调用ID,
                "content": 结果
            });

            if !安全工具名.is_empty() {
                工具消息["name"] = json!(安全工具名);
            }

            会话.历史.push(工具消息);
            return 1;
        }
    }

    -1
}

/// 设置会话配置参数
///
/// 参数:
/// - session_handle: 会话句柄
/// - key: 配置键 (如 "temperature", "max_tokens")
/// - value: 配置值
///
/// 返回: 1 成功, -1 失败
#[no_mangle]
pub extern "C" fn qi_llm_set_config(
    session_handle: i64,
    key: *const c_char,
    value: *const c_char,
) -> i64 {
    if key.is_null() || value.is_null() {
        return -1;
    }

    let mut 会话池 = 获取会话池().lock().unwrap();

    if let Some(会话) = 会话池.get_mut(&session_handle) {
        unsafe {
            let 键 = CStr::from_ptr(key).to_string_lossy().to_string();
            let 值 = CStr::from_ptr(value).to_string_lossy().to_string();
            // provider 走配置口切换："openai"（默认）/"anthropic"/"gemini"
            if 键 == "provider" {
                会话.提供商 = 值.trim().to_ascii_lowercase();
            }
            会话.配置.insert(键, 值);
            return 1;
        }
    }

    -1
}

/// 清空对话历史
///
/// 参数:
/// - session_handle: 会话句柄
///
/// 返回: 1 成功, -1 失败
#[no_mangle]
pub extern "C" fn qi_llm_clear_history(session_handle: i64) -> i64 {
    let mut 会话池 = 获取会话池().lock().unwrap();

    if let Some(会话) = 会话池.get_mut(&session_handle) {
        会话.历史.clear();
        return 1;
    }

    -1
}

/// 获取对话历史记录数
///
/// 参数:
/// - session_handle: 会话句柄
///
/// 返回: 历史记录数 (>=0 成功, <0 失败)
#[no_mangle]
pub extern "C" fn qi_llm_get_history_count(session_handle: i64) -> i64 {
    let 会话池 = 获取会话池().lock().unwrap();

    if let Some(会话) = 会话池.get(&session_handle) {
        return 会话.历史.len() as i64;
    }

    -1
}

/// 关闭LLM会话
///
/// 参数:
/// - session_handle: 会话句柄
///
/// 返回: 1 成功, -1 失败
#[no_mangle]
pub extern "C" fn qi_llm_close_session(session_handle: i64) -> i64 {
    let mut 会话池 = 获取会话池().lock().unwrap();

    if 会话池.remove(&session_handle).is_some() {
        return 1;
    }

    -1
}

/// 释放LLM返回的字符串
///
/// 参数:
/// - s: 字符串指针
#[no_mangle]
pub extern "C" fn qi_llm_free_string(s: *mut c_char) {
    if !s.is_null() {
        unsafe {
            let _ = CString::from_raw(s);
        }
    }
}

// ============================================================================
// 异步 LLM API
// ============================================================================

use crate::runtime::async_runtime::future::Future;
use std::thread;

/// 异步发送消息到LLM (返回 未来<字符串>)
///
/// 参数:
/// - session_handle: 会话句柄
/// - prompt: 用户提示
///
/// 返回: Future 指针 (需要使用 等待 关键字获取结果)
#[no_mangle]
pub extern "C" fn qi_llm_chat_async(session_handle: i64, prompt: *const c_char) -> *mut Future {
    if prompt.is_null() {
        return std::ptr::null_mut();
    }

    unsafe {
        let 提示 = CStr::from_ptr(prompt).to_string_lossy().to_string();

        // 获取会话的克隆
        let 会话克隆 = {
            let 会话池 = 获取会话池().lock().unwrap();
            match 会话池.get(&session_handle) {
                Some(会话) => 会话.clone(),
                None => return std::ptr::null_mut(),
            }
        };

        // 创建一个 pending Future。
        // 完成/失败必须走 complete()/fail() —— 它们会 fire notify + sm_wakers。
        // （旧实现手工翻 state/value Arc，notify 在 spawn 之后另建、没传进线程：
        //   HTTP 完成时无人被唤醒，等待 只要在完成前开始就永远 park —— lost wakeup。
        //   并行扇出「先发后等」必踩。）
        let future = Box::new(Future::pending());
        let 完成端 = Future {
            state: future.state.clone(),
            value: future.value.clone(),
            error: future.error.clone(),
            notify: future.notify.clone(),
            sm_wakers: future.sm_wakers.clone(),
        };

        // 在后台线程中执行 HTTP 请求
        thread::spawn(move || {
            match 会话克隆.调用API(&提示) {
                Ok(响应) => {
                    // 更新会话历史
                    {
                        let mut 会话池 = 获取会话池().lock().unwrap();
                        if let Some(会话) = 会话池.get_mut(&session_handle) {
                            会话.历史.push(json!({
                                "role": "user",
                                "content": 提示.clone()
                            }));
                            会话.历史.push(json!({
                                "role": "assistant",
                                "content": 响应.clone()
                            }));
                        }
                    }

                    完成端.complete(crate::runtime::async_runtime::future::FutureValue::String(
                        响应,
                    ));
                }
                Err(错误) => {
                    完成端.fail(format!("LLM异步调用失败: {}", 错误));
                }
            }
        });

        Box::into_raw(future)
    }
}

// ───────────────── Provider 形状单元测试（与 qi-runtime 孪生副本同步；不打网络） ─────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn 建会话(提供商: &str) -> LLM会话 {
        let mut 会话 = LLM会话::创建(
            "https://example.com".to_string(),
            "test-model".to_string(),
            Some("test-key".to_string()),
        );
        会话.提供商 = 提供商.to_string();
        会话
    }

    #[test]
    fn openai_请求体形状不变() {
        let mut 会话 = 建会话("openai");
        会话
            .配置
            .insert("system".to_string(), "你是助手".to_string());
        let 体 = 会话.构建请求体("你好", false, false);
        assert_eq!(体["model"], json!("test-model"));
        assert!(体["messages"].is_array());
        assert_eq!(体["messages"][0]["role"], json!("system"));
        assert_eq!(体["messages"][1]["content"], json!("你好"));
        assert!(体.get("system").is_none());
        assert!(体.get("contents").is_none());
        assert_eq!(会话.请求端点(), "https://example.com/chat/completions");
    }

    #[test]
    fn anthropic_请求体形状() {
        let mut 会话 = 建会话("anthropic");
        会话
            .配置
            .insert("system".to_string(), "你是助手".to_string());
        会话.历史.push(json!({"role": "user", "content": "1+1=?"}));
        会话.历史.push(json!({"role": "assistant", "content": "2"}));
        let 体 = 会话.构建请求体("再加一", false, false);
        assert!(体["max_tokens"].is_i64(), "anthropic 必须有顶层 max_tokens");
        assert_eq!(体["system"], json!("你是助手"));
        let 消息 = 体["messages"].as_array().unwrap();
        assert!(消息.iter().all(|m| m["role"] != json!("system")));
        assert_eq!(体["model"], json!("test-model"));
        assert_eq!(会话.请求端点(), "https://example.com/v1/messages");
    }

    #[test]
    fn anthropic_图像块() {
        let 会话 = 建会话("anthropic");
        let 内容 = json!([
            {"type": "text", "text": "什么颜色?"},
            {"type": "image_url", "image_url": {"url": "https://img.example/red.png"}}
        ]);
        let 体 = 会话.构建请求体带内容(Some(内容), false, false);
        let 块 = &体["messages"][0]["content"];
        assert_eq!(块[1]["type"], json!("image"));
        assert_eq!(块[1]["source"]["type"], json!("url"));
        assert_eq!(块[1]["source"]["url"], json!("https://img.example/red.png"));
    }

    #[test]
    fn anthropic_响应归一化() {
        let 会话 = 建会话("anthropic");
        let 响应 = json!({
            "content": [
                {"type": "text", "text": "好的。"},
                {"type": "tool_use", "id": "tu_1", "name": "get_weather", "input": {"city": "东京"}}
            ],
            "stop_reason": "tool_use"
        });
        let 消息 = 会话.提取消息(&响应).unwrap();
        assert_eq!(消息["content"], json!("好的。"));
        assert_eq!(消息["tool_calls"][0]["id"], json!("tu_1"));
        assert_eq!(
            消息["tool_calls"][0]["function"]["name"],
            json!("get_weather")
        );
    }

    #[test]
    fn gemini_请求体形状() {
        let mut 会话 = 建会话("gemini");
        会话
            .配置
            .insert("system".to_string(), "你是助手".to_string());
        会话.历史.push(json!({"role": "user", "content": "1+1=?"}));
        会话.历史.push(json!({"role": "assistant", "content": "2"}));
        let 体 = 会话.构建请求体("再加一", false, false);
        let 内容 = 体["contents"].as_array().unwrap();
        assert_eq!(内容[0]["role"], json!("user"));
        assert_eq!(内容[1]["role"], json!("model"));
        assert_eq!(
            体["systemInstruction"]["parts"][0]["text"],
            json!("你是助手")
        );
        assert!(体["generationConfig"]["maxOutputTokens"].is_i64());
        assert!(体.get("messages").is_none());
        assert_eq!(
            会话.请求端点(),
            "https://example.com/models/test-model:generateContent"
        );
    }

    #[test]
    fn gemini_图像部件() {
        let 会话 = 建会话("gemini");
        let 内容 = json!([
            {"type": "text", "text": "什么颜色?"},
            {"type": "image_url", "image_url": {"url": "https://img.example/red.png"}}
        ]);
        let 体 = 会话.构建请求体带内容(Some(内容), false, false);
        let 部件 = &体["contents"][0]["parts"];
        assert_eq!(
            部件[1]["file_data"]["file_uri"],
            json!("https://img.example/red.png")
        );
    }

    #[test]
    fn gemini_响应归一化() {
        let 会话 = 建会话("gemini");
        let 响应 = json!({
            "candidates": [{"content": {"role": "model", "parts": [{"text": "答案是 3"}]}}]
        });
        let 消息 = 会话.提取消息(&响应).unwrap();
        assert_eq!(消息["content"], json!("答案是 3"));
    }
}
