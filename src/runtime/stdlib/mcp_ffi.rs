//! MCP 服务器模块 FFI 接口
//!
//! 为 Qi 语言提供 C 接口的 MCP 服务器调用函数

#![allow(non_snake_case)]

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::sync::{Mutex, OnceLock};
use std::collections::HashMap;
use serde_json::{json, Value as JsonValue};

use super::mcp::{
    MCP服务器模块, MCP服务器, MCP工具, MCP资源, MCP提示,
    资源类型, MCP服务器配置, 工具参数,
};

// MCP 服务器池
static MCP服务器池: OnceLock<Mutex<HashMap<i64, MCP服务器>>> = OnceLock::new();
static 服务器计数器: OnceLock<Mutex<i64>> = OnceLock::new();

fn 获取服务器池() -> &'static Mutex<HashMap<i64, MCP服务器>> {
    MCP服务器池.get_or_init(|| Mutex::new(HashMap::new()))
}

fn 获取服务器计数器() -> &'static Mutex<i64> {
    服务器计数器.get_or_init(|| Mutex::new(0))
}

/// 创建MCP服务器
///
/// 参数:
/// - name: 服务器名称
/// - version: 服务器版本
/// - description: 服务器描述 (可选，传入空字符串表示无描述)
///
/// 返回: 服务器句柄 (>0 成功, <0 失败)
#[no_mangle]
pub extern "C" fn qi_mcp_create_server(
    name: *const c_char,
    version: *const c_char,
    description: *const c_char,
) -> i64 {
    if name.is_null() || version.is_null() {
        return -1;
    }

    unsafe {
        let 名称 = CStr::from_ptr(name).to_string_lossy().to_string();
        let 版本 = CStr::from_ptr(version).to_string_lossy().to_string();
        let 描述 = if description.is_null() || CStr::from_ptr(description).to_bytes().is_empty() {
            None
        } else {
            Some(CStr::from_ptr(description).to_string_lossy().to_string())
        };

        let 配置 = MCP服务器配置 {
            名称,
            版本,
            描述,
            协议版本: "2025-06-18".to_string(),
        };

        let 模块 = MCP服务器模块::创建();
        let 服务器 = 模块.创建服务器(Some(配置));

        // 生成新的服务器ID
        let mut 计数器 = 获取服务器计数器().lock().unwrap();
        *计数器 += 1;
        let 服务器ID = *计数器;

        // 存储服务器
        let mut 服务器池 = 获取服务器池().lock().unwrap();
        服务器池.insert(服务器ID, 服务器);

        服务器ID
    }
}

/// 注册工具到MCP服务器
///
/// 参数:
/// - server_id: 服务器句柄
/// - tool_name: 工具名称
/// - tool_description: 工具描述
///
/// 返回: 0 成功, -1 失败
#[no_mangle]
pub extern "C" fn qi_mcp_register_tool(
    server_id: i64,
    tool_name: *const c_char,
    tool_description: *const c_char,
) -> i32 {
    if tool_name.is_null() || tool_description.is_null() {
        return -1;
    }

    unsafe {
        let 名称 = CStr::from_ptr(tool_name).to_string_lossy().to_string();
        let 描述 = CStr::from_ptr(tool_description).to_string_lossy().to_string();

        let 工具 = MCP工具::创建(名称, 描述);

        let mut 服务器池 = 获取服务器池().lock().unwrap();
        if let Some(服务器) = 服务器池.get_mut(&server_id) {
            match 服务器.注册工具(工具) {
                Ok(_) => 0,
                Err(_) => -1,
            }
        } else {
            -1
        }
    }
}

/// 添加工具参数
///
/// 参数:
/// - server_id: 服务器句柄
/// - tool_name: 工具名称
/// - param_name: 参数名称
/// - param_type: 参数类型 ("string", "number", "boolean", "object", "array")
/// - param_description: 参数描述
/// - required: 是否必需 (1=必需, 0=可选)
///
/// 返回: 0 成功, -1 失败
///
/// 注意: 必须先调用 qi_mcp_register_tool 注册工具，再调用此函数添加参数
#[no_mangle]
pub extern "C" fn qi_mcp_add_tool_parameter(
    server_id: i64,
    tool_name: *const c_char,
    param_name: *const c_char,
    param_type: *const c_char,
    param_description: *const c_char,
    required: i32,
) -> i32 {
    if tool_name.is_null() || param_name.is_null() || param_type.is_null() || param_description.is_null() {
        return -1;
    }

    unsafe {
        let 工具名 = CStr::from_ptr(tool_name).to_string_lossy().to_string();
        let 参数名 = CStr::from_ptr(param_name).to_string_lossy().to_string();
        let 参数类型 = CStr::from_ptr(param_type).to_string_lossy().to_string();
        let 参数描述 = CStr::from_ptr(param_description).to_string_lossy().to_string();
        let 是否必需 = required != 0;

        let 参数 = 工具参数::创建(参数名, 参数类型, 参数描述, 是否必需);

        let mut 服务器池 = 获取服务器池().lock().unwrap();
        if let Some(服务器) = 服务器池.get_mut(&server_id) {
            match 服务器.为工具添加参数(&工具名, 参数) {
                Ok(_) => 0,
                Err(_) => -1,
            }
        } else {
            -1
        }
    }
}

/// 注册资源到MCP服务器
///
/// 参数:
/// - server_id: 服务器句柄
/// - resource_uri: 资源URI
/// - resource_name: 资源名称
/// - resource_description: 资源描述
/// - resource_type: 资源类型 (0=文本, 1=二进制, 2=JSON)
///
/// 返回: 0 成功, -1 失败
#[no_mangle]
pub extern "C" fn qi_mcp_register_resource(
    server_id: i64,
    resource_uri: *const c_char,
    resource_name: *const c_char,
    resource_description: *const c_char,
    resource_type: i32,
) -> i32 {
    if resource_uri.is_null() || resource_name.is_null() || resource_description.is_null() {
        return -1;
    }

    unsafe {
        let uri = CStr::from_ptr(resource_uri).to_string_lossy().to_string();
        let 名称 = CStr::from_ptr(resource_name).to_string_lossy().to_string();
        let 描述 = CStr::from_ptr(resource_description).to_string_lossy().to_string();

        let 类型 = match resource_type {
            0 => 资源类型::文本,
            1 => 资源类型::二进制,
            2 => 资源类型::JSON,
            _ => return -1,
        };

        let 资源 = MCP资源::创建(uri, 名称, 描述, 类型);

        let mut 服务器池 = 获取服务器池().lock().unwrap();
        if let Some(服务器) = 服务器池.get_mut(&server_id) {
            match 服务器.注册资源(资源) {
                Ok(_) => 0,
                Err(_) => -1,
            }
        } else {
            -1
        }
    }
}

/// 注册提示到MCP服务器
///
/// 参数:
/// - server_id: 服务器句柄
/// - prompt_name: 提示名称
/// - prompt_description: 提示描述
/// - prompt_template: 提示模板 (使用 {变量名} 作为占位符)
///
/// 返回: 0 成功, -1 失败
#[no_mangle]
pub extern "C" fn qi_mcp_register_prompt(
    server_id: i64,
    prompt_name: *const c_char,
    prompt_description: *const c_char,
    prompt_template: *const c_char,
) -> i32 {
    if prompt_name.is_null() || prompt_description.is_null() || prompt_template.is_null() {
        return -1;
    }

    unsafe {
        let 名称 = CStr::from_ptr(prompt_name).to_string_lossy().to_string();
        let 描述 = CStr::from_ptr(prompt_description).to_string_lossy().to_string();
        let 模板 = CStr::from_ptr(prompt_template).to_string_lossy().to_string();

        let 提示 = MCP提示::创建(名称, 描述, 模板);

        let mut 服务器池 = 获取服务器池().lock().unwrap();
        if let Some(服务器) = 服务器池.get_mut(&server_id) {
            match 服务器.注册提示(提示) {
                Ok(_) => 0,
                Err(_) => -1,
            }
        } else {
            -1
        }
    }
}

/// 启动MCP服务器
///
/// 参数:
/// - server_id: 服务器句柄
///
/// 返回: 0 成功, -1 失败
#[no_mangle]
pub extern "C" fn qi_mcp_start_server(server_id: i64) -> i32 {
    let mut 服务器池 = 获取服务器池().lock().unwrap();
    if let Some(服务器) = 服务器池.get_mut(&server_id) {
        match 服务器.启动() {
            Ok(_) => 0,
            Err(_) => -1,
        }
    } else {
        -1
    }
}

/// 停止MCP服务器
///
/// 参数:
/// - server_id: 服务器句柄
///
/// 返回: 0 成功, -1 失败
#[no_mangle]
pub extern "C" fn qi_mcp_stop_server(server_id: i64) -> i32 {
    let mut 服务器池 = 获取服务器池().lock().unwrap();
    if let Some(服务器) = 服务器池.get_mut(&server_id) {
        match 服务器.停止() {
            Ok(_) => 0,
            Err(_) => -1,
        }
    } else {
        -1
    }
}

/// 获取服务器信息 (JSON格式)
///
/// 参数:
/// - server_id: 服务器句柄
///
/// 返回: JSON字符串 (需要调用 qi_mcp_free_string 释放), NULL 失败
#[no_mangle]
pub extern "C" fn qi_mcp_get_server_info(server_id: i64) -> *mut c_char {
    let 服务器池 = 获取服务器池().lock().unwrap();
    if let Some(服务器) = 服务器池.get(&server_id) {
        let 信息 = 服务器.获取服务器信息();
        let json_str = 信息.to_string();
        match CString::new(json_str) {
            Ok(c_str) => c_str.into_raw(),
            Err(_) => std::ptr::null_mut(),
        }
    } else {
        std::ptr::null_mut()
    }
}

/// 获取工具列表 (JSON格式)
///
/// 参数:
/// - server_id: 服务器句柄
///
/// 返回: JSON字符串 (需要调用 qi_mcp_free_string 释放), NULL 失败
#[no_mangle]
pub extern "C" fn qi_mcp_list_tools(server_id: i64) -> *mut c_char {
    let 服务器池 = 获取服务器池().lock().unwrap();
    if let Some(服务器) = 服务器池.get(&server_id) {
        let 工具列表 = 服务器.获取工具列表();
        let json_str = json!(工具列表).to_string();
        match CString::new(json_str) {
            Ok(c_str) => c_str.into_raw(),
            Err(_) => std::ptr::null_mut(),
        }
    } else {
        std::ptr::null_mut()
    }
}

/// 获取资源列表 (JSON格式)
///
/// 参数:
/// - server_id: 服务器句柄
///
/// 返回: JSON字符串 (需要调用 qi_mcp_free_string 释放), NULL 失败
#[no_mangle]
pub extern "C" fn qi_mcp_list_resources(server_id: i64) -> *mut c_char {
    let 服务器池 = 获取服务器池().lock().unwrap();
    if let Some(服务器) = 服务器池.get(&server_id) {
        let 资源列表 = 服务器.获取资源列表();
        let json_str = json!(资源列表).to_string();
        match CString::new(json_str) {
            Ok(c_str) => c_str.into_raw(),
            Err(_) => std::ptr::null_mut(),
        }
    } else {
        std::ptr::null_mut()
    }
}

/// 获取提示列表 (JSON格式)
///
/// 参数:
/// - server_id: 服务器句柄
///
/// 返回: JSON字符串 (需要调用 qi_mcp_free_string 释放), NULL 失败
#[no_mangle]
pub extern "C" fn qi_mcp_list_prompts(server_id: i64) -> *mut c_char {
    let 服务器池 = 获取服务器池().lock().unwrap();
    if let Some(服务器) = 服务器池.get(&server_id) {
        let 提示列表 = 服务器.获取提示列表();
        let json_str = json!(提示列表).to_string();
        match CString::new(json_str) {
            Ok(c_str) => c_str.into_raw(),
            Err(_) => std::ptr::null_mut(),
        }
    } else {
        std::ptr::null_mut()
    }
}

/// 执行工具
///
/// 参数:
/// - server_id: 服务器句柄
/// - tool_name: 工具名称
/// - params_json: 参数 JSON 字符串
///
/// 返回: 执行结果 JSON 字符串 (需要调用 qi_mcp_free_string 释放), NULL 失败
#[no_mangle]
pub extern "C" fn qi_mcp_call_tool(
    server_id: i64,
    tool_name: *const c_char,
    params_json: *const c_char,
) -> *mut c_char {
    if tool_name.is_null() || params_json.is_null() {
        return std::ptr::null_mut();
    }

    unsafe {
        let 工具名 = CStr::from_ptr(tool_name).to_string_lossy().to_string();
        let json_str = CStr::from_ptr(params_json).to_string_lossy().to_string();

        // 解析参数JSON
        let 参数: HashMap<String, JsonValue> = match serde_json::from_str(&json_str) {
            Ok(params) => params,
            Err(_) => return std::ptr::null_mut(),
        };

        let 服务器池 = 获取服务器池().lock().unwrap();
        if let Some(服务器) = 服务器池.get(&server_id) {
            match 服务器.执行工具(&工具名, &参数) {
                Ok(结果) => {
                    let 结果字符串 = 结果.to_string();
                    match CString::new(结果字符串) {
                        Ok(c_str) => c_str.into_raw(),
                        Err(_) => std::ptr::null_mut(),
                    }
                }
                Err(_) => std::ptr::null_mut(),
            }
        } else {
            std::ptr::null_mut()
        }
    }
}

/// 填充提示模板
///
/// 参数:
/// - server_id: 服务器句柄
/// - prompt_name: 提示名称
/// - params_json: 参数 JSON 字符串 (键值对)
///
/// 返回: 填充后的提示文本 (需要调用 qi_mcp_free_string 释放), NULL 失败
#[no_mangle]
pub extern "C" fn qi_mcp_get_prompt(
    server_id: i64,
    prompt_name: *const c_char,
    params_json: *const c_char,
) -> *mut c_char {
    if prompt_name.is_null() || params_json.is_null() {
        return std::ptr::null_mut();
    }

    unsafe {
        let 提示名 = CStr::from_ptr(prompt_name).to_string_lossy().to_string();
        let json_str = CStr::from_ptr(params_json).to_string_lossy().to_string();

        // 解析参数JSON
        let 参数: HashMap<String, String> = match serde_json::from_str(&json_str) {
            Ok(params) => params,
            Err(_) => return std::ptr::null_mut(),
        };

        let 服务器池 = 获取服务器池().lock().unwrap();
        if let Some(服务器) = 服务器池.get(&server_id) {
            match 服务器.获取提示(&提示名) {
                Ok(提示) => match 提示.填充(&参数) {
                    Ok(结果文本) => match CString::new(结果文本) {
                        Ok(c_str) => c_str.into_raw(),
                        Err(_) => std::ptr::null_mut(),
                    },
                    Err(_) => std::ptr::null_mut(),
                },
                Err(_) => std::ptr::null_mut(),
            }
        } else {
            std::ptr::null_mut()
        }
    }
}

/// 检查服务器是否正在运行
///
/// 参数:
/// - server_id: 服务器句柄
///
/// 返回: 1 运行中, 0 未运行, -1 失败
#[no_mangle]
pub extern "C" fn qi_mcp_is_running(server_id: i64) -> i32 {
    let 服务器池 = 获取服务器池().lock().unwrap();
    if let Some(服务器) = 服务器池.get(&server_id) {
        if 服务器.是否运行中() {
            1
        } else {
            0
        }
    } else {
        -1
    }
}

/// 释放MCP服务器
///
/// 参数:
/// - server_id: 服务器句柄
///
/// 返回: 0 成功, -1 失败
#[no_mangle]
pub extern "C" fn qi_mcp_destroy_server(server_id: i64) -> i32 {
    let mut 服务器池 = 获取服务器池().lock().unwrap();
    if 服务器池.remove(&server_id).is_some() {
        0
    } else {
        -1
    }
}

/// 设置资源文本内容
///
/// 参数:
/// - server_id: 服务器句柄
/// - resource_uri: 资源URI
/// - content: 文本内容
///
/// 返回: 0 成功, -1 失败
#[no_mangle]
pub extern "C" fn qi_mcp_set_resource_text_content(
    server_id: i64,
    resource_uri: *const c_char,
    content: *const c_char,
) -> i32 {
    if resource_uri.is_null() || content.is_null() {
        return -1;
    }

    unsafe {
        let uri = CStr::from_ptr(resource_uri).to_string_lossy().to_string();
        let 内容 = CStr::from_ptr(content).to_string_lossy().to_string();

        let mut 服务器池 = 获取服务器池().lock().unwrap();
        if let Some(服务器) = 服务器池.get_mut(&server_id) {
            match 服务器.设置资源文本内容(&uri, 内容) {
                Ok(_) => 0,
                Err(_) => -1,
            }
        } else {
            -1
        }
    }
}

/// 设置资源JSON内容
///
/// 参数:
/// - server_id: 服务器句柄
/// - resource_uri: 资源URI
/// - json_content: JSON字符串内容
///
/// 返回: 0 成功, -1 失败
#[no_mangle]
pub extern "C" fn qi_mcp_set_resource_json_content(
    server_id: i64,
    resource_uri: *const c_char,
    json_content: *const c_char,
) -> i32 {
    if resource_uri.is_null() || json_content.is_null() {
        return -1;
    }

    unsafe {
        let uri = CStr::from_ptr(resource_uri).to_string_lossy().to_string();
        let json_str = CStr::from_ptr(json_content).to_string_lossy().to_string();

        // 解析JSON
        let json_value = match serde_json::from_str(&json_str) {
            Ok(v) => v,
            Err(_) => return -1,
        };

        let mut 服务器池 = 获取服务器池().lock().unwrap();
        if let Some(服务器) = 服务器池.get_mut(&server_id) {
            match 服务器.设置资源JSON内容(&uri, json_value) {
                Ok(_) => 0,
                Err(_) => -1,
            }
        } else {
            -1
        }
    }
}

/// 读取资源文本内容
///
/// 参数:
/// - server_id: 服务器句柄
/// - resource_uri: 资源URI
///
/// 返回: 文本内容的C字符串指针，失败返回NULL
/// 注意: 调用者需要使用 qi_mcp_free_string 释放返回的字符串
#[no_mangle]
pub extern "C" fn qi_mcp_read_resource_text(
    server_id: i64,
    resource_uri: *const c_char,
) -> *mut c_char {
    if resource_uri.is_null() {
        return std::ptr::null_mut();
    }

    unsafe {
        let uri = CStr::from_ptr(resource_uri).to_string_lossy().to_string();

        let 服务器池 = 获取服务器池().lock().unwrap();
        if let Some(服务器) = 服务器池.get(&server_id) {
            match 服务器.读取资源文本(&uri) {
                Ok(text) => {
                    CString::new(text).unwrap_or_else(|_| CString::new("").unwrap()).into_raw()
                },
                Err(_) => std::ptr::null_mut(),
            }
        } else {
            std::ptr::null_mut()
        }
    }
}

/// 读取资源JSON内容
///
/// 参数:
/// - server_id: 服务器句柄
/// - resource_uri: 资源URI
///
/// 返回: JSON内容的C字符串指针，失败返回NULL
/// 注意: 调用者需要使用 qi_mcp_free_string 释放返回的字符串
#[no_mangle]
pub extern "C" fn qi_mcp_read_resource_json(
    server_id: i64,
    resource_uri: *const c_char,
) -> *mut c_char {
    if resource_uri.is_null() {
        return std::ptr::null_mut();
    }

    unsafe {
        let uri = CStr::from_ptr(resource_uri).to_string_lossy().to_string();

        let 服务器池 = 获取服务器池().lock().unwrap();
        if let Some(服务器) = 服务器池.get(&server_id) {
            match 服务器.读取资源JSON(&uri) {
                Ok(json) => {
                    let json_str = json.to_string();
                    CString::new(json_str).unwrap_or_else(|_| CString::new("{}").unwrap()).into_raw()
                },
                Err(_) => std::ptr::null_mut(),
            }
        } else {
            std::ptr::null_mut()
        }
    }
}

/// 设置工具回调ID
///
/// 参数:
/// - server_id: 服务器句柄
/// - tool_name: 工具名称
/// - callback_id: 回调标识符
///
/// 返回: 0 成功, -1 失败
#[no_mangle]
pub extern "C" fn qi_mcp_set_tool_callback(
    server_id: i64,
    tool_name: *const c_char,
    callback_id: *const c_char,
) -> i32 {
    if tool_name.is_null() || callback_id.is_null() {
        return -1;
    }

    unsafe {
        let 工具名 = CStr::from_ptr(tool_name).to_string_lossy().to_string();
        let 回调ID = CStr::from_ptr(callback_id).to_string_lossy().to_string();

        let mut 服务器池 = 获取服务器池().lock().unwrap();
        if let Some(服务器) = 服务器池.get_mut(&server_id) {
            match 服务器.设置工具回调ID(&工具名, 回调ID) {
                Ok(_) => 0,
                Err(_) => -1,
            }
        } else {
            -1
        }
    }
}

/// 释放字符串
///
/// 参数:
/// - s: 由 MCP FFI 函数返回的字符串指针
#[no_mangle]
pub extern "C" fn qi_mcp_free_string(s: *mut c_char) {
    if !s.is_null() {
        unsafe {
            let _ = CString::from_raw(s);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    #[test]
    fn 测试创建服务器() {
        let 名称 = CString::new("测试服务器").unwrap();
        let 版本 = CString::new("1.0.0").unwrap();
        let 描述 = CString::new("测试描述").unwrap();

        let 服务器ID = qi_mcp_create_server(
            名称.as_ptr(),
            版本.as_ptr(),
            描述.as_ptr(),
        );

        assert!(服务器ID > 0);

        // 清理
        qi_mcp_destroy_server(服务器ID);
    }

    #[test]
    fn 测试注册工具() {
        let 名称 = CString::new("测试服务器").unwrap();
        let 版本 = CString::new("1.0.0").unwrap();
        let 描述 = CString::new("").unwrap();

        let 服务器ID = qi_mcp_create_server(
            名称.as_ptr(),
            版本.as_ptr(),
            描述.as_ptr(),
        );

        let 工具名 = CString::new("echo").unwrap();
        let 工具描述 = CString::new("回显工具").unwrap();

        let 结果 = qi_mcp_register_tool(
            服务器ID,
            工具名.as_ptr(),
            工具描述.as_ptr(),
        );

        assert_eq!(结果, 0);

        // 清理
        qi_mcp_destroy_server(服务器ID);
    }

    #[test]
    fn 测试启动停止服务器() {
        let 名称 = CString::new("测试服务器").unwrap();
        let 版本 = CString::new("1.0.0").unwrap();
        let 描述 = CString::new("").unwrap();

        let 服务器ID = qi_mcp_create_server(
            名称.as_ptr(),
            版本.as_ptr(),
            描述.as_ptr(),
        );

        // 启动服务器
        let 启动结果 = qi_mcp_start_server(服务器ID);
        assert_eq!(启动结果, 0);
        assert_eq!(qi_mcp_is_running(服务器ID), 1);

        // 停止服务器
        let 停止结果 = qi_mcp_stop_server(服务器ID);
        assert_eq!(停止结果, 0);
        assert_eq!(qi_mcp_is_running(服务器ID), 0);

        // 清理
        qi_mcp_destroy_server(服务器ID);
    }
}
