//! LLM 模块 FFI 接口
//!
//! 为 Qi 语言提供 C 接口的 LLM 调用函数

#![allow(non_snake_case)]

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::sync::{Mutex, OnceLock};
use std::collections::HashMap;
use serde_json::json;

// LLM 会话池
static LLM会话池: OnceLock<Mutex<HashMap<i64, LLM会话>>> = OnceLock::new();
static 会话计数器: OnceLock<Mutex<i64>> = OnceLock::new();

fn 获取会话池() -> &'static Mutex<HashMap<i64, LLM会话>> {
    LLM会话池.get_or_init(|| Mutex::new(HashMap::new()))
}

fn 获取会话计数器() -> &'static Mutex<i64> {
    会话计数器.get_or_init(|| Mutex::new(0))
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
    /// 对话历史
    历史: Vec<消息>,
    /// 配置参数
    配置: HashMap<String, String>,
}

#[derive(Debug, Clone)]
struct 消息 {
    角色: String,  // "user", "assistant", "system"
    内容: String,
}

impl LLM会话 {
    fn 创建(端点: String, 模型: String, 密钥: Option<String>) -> Self {
        Self {
            端点,
            密钥,
            模型,
            历史: Vec::new(),
            配置: HashMap::new(),
        }
    }

    /// 发送HTTP请求到LLM API
    fn 调用API(&self, 提示: &str) -> Result<String, String> {
        use reqwest::blocking::Client;
        use serde_json::Value;

        // 构建请求体
        let mut 消息列表 = self.历史.clone();
        消息列表.push(消息 {
            角色: "user".to_string(),
            内容: 提示.to_string(),
        });

        let 请求体 = json!({
            "model": self.模型,
            "messages": 消息列表.iter().map(|msg| {
                json!({
                    "role": msg.角色,
                    "content": msg.内容
                })
            }).collect::<Vec<_>>(),
            "temperature": self.配置.get("temperature")
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(0.7),
            "max_tokens": self.配置.get("max_tokens")
                .and_then(|s| s.parse::<i32>().ok())
                .unwrap_or(2000),
        });

        // 使用 reqwest 发送 HTTP POST 请求
        let 客户端 = Client::new();
        let mut 请求构建器 = 客户端
            .post(&self.端点)
            .header("Content-Type", "application/json");

        // 添加 API 密钥（如果有）
        if let Some(ref 密钥) = self.密钥 {
            请求构建器 = 请求构建器.header("Authorization", format!("Bearer {}", 密钥));
        }

        // 发送请求
        let 响应 = 请求构建器
            .json(&请求体)
            .send()
            .map_err(|e| format!("HTTP请求失败: {}", e))?;

        // 检查状态码
        if !响应.status().is_success() {
            let 状态码 = 响应.status();
            let 错误文本 = 响应.text().unwrap_or_else(|_| "无法读取错误响应".to_string());
            return Err(format!("API返回错误 {}: {}", 状态码, 错误文本));
        }

        // 解析响应
        let 响应体: Value = 响应
            .json()
            .map_err(|e| format!("解析响应失败: {}", e))?;

        // 提取 AI 回复文本
        let 回复文本 = 响应体
            .get("choices")
            .and_then(|choices| choices.get(0))
            .and_then(|choice| choice.get("message"))
            .and_then(|message| message.get("content"))
            .and_then(|content| content.as_str())
            .ok_or_else(|| "响应格式错误：无法提取回复内容".to_string())?;

        Ok(回复文本.to_string())
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
pub extern "C" fn qi_llm_chat(
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

            match 会话.调用API(&提示) {
                Ok(响应) => {
                    // 添加到历史
                    会话.历史.push(消息 {
                        角色: "user".to_string(),
                        内容: 提示.clone(),
                    });
                    会话.历史.push(消息 {
                        角色: "assistant".to_string(),
                        内容: 响应.clone(),
                    });

                    if let Ok(c_str) = CString::new(响应) {
                        return c_str.into_raw();
                    }
                }
                Err(错误) => {
                    let 错误信息 = format!("LLM调用失败: {}", 错误);
                    if let Ok(c_str) = CString::new(错误信息) {
                        return c_str.into_raw();
                    }
                }
            }
        }
    }

    std::ptr::null_mut()
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

use std::thread;
use crate::runtime::async_runtime::future::Future;

/// 异步发送消息到LLM (返回 未来<字符串>)
///
/// 参数:
/// - session_handle: 会话句柄
/// - prompt: 用户提示
///
/// 返回: Future 指针 (需要使用 等待 关键字获取结果)
#[no_mangle]
pub extern "C" fn qi_llm_chat_async(
    session_handle: i64,
    prompt: *const c_char,
) -> *mut Future {
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

        // 创建一个 pending Future
        let future_state = std::sync::Arc::new(std::sync::Mutex::new(
            crate::runtime::async_runtime::future::FutureState::Pending
        ));
        let future_value = std::sync::Arc::new(std::sync::Mutex::new(None));
        let future_error = std::sync::Arc::new(std::sync::Mutex::new(None));

        let state_clone = future_state.clone();
        let value_clone = future_value.clone();
        let error_clone = future_error.clone();

        // 在后台线程中执行 HTTP 请求
        thread::spawn(move || {
            match 会话克隆.调用API(&提示) {
                Ok(响应) => {
                    // 更新会话历史
                    {
                        let mut 会话池 = 获取会话池().lock().unwrap();
                        if let Some(会话) = 会话池.get_mut(&session_handle) {
                            会话.历史.push(消息 {
                                角色: "user".to_string(),
                                内容: 提示.clone(),
                            });
                            会话.历史.push(消息 {
                                角色: "assistant".to_string(),
                                内容: 响应.clone(),
                            });
                        }
                    }

                    // 更新 Future 状态
                    *value_clone.lock().unwrap() = Some(
                        crate::runtime::async_runtime::future::FutureValue::String(响应)
                    );
                    *state_clone.lock().unwrap() =
                        crate::runtime::async_runtime::future::FutureState::Completed;
                }
                Err(错误) => {
                    *error_clone.lock().unwrap() = Some(format!("LLM异步调用失败: {}", 错误));
                    *state_clone.lock().unwrap() =
                        crate::runtime::async_runtime::future::FutureState::Failed;
                }
            }
        });

        // 返回 Future 指针
        let future = Box::new(Future {
            state: future_state,
            value: future_value,
            error: future_error,
        });
        Box::into_raw(future)
    }
}
