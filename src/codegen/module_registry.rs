//! Module Registry for Qi Language
//!
//! This module provides a registry system for managing standard library modules
//! and their functions, enabling modular imports and namespace resolution.

use std::collections::HashMap;

use super::stdlib_abi::{TOOL_CONTROL_ABI, WEB_RUNTIME_ABI};

/// Represents a single function in a module
#[derive(Debug, Clone)]
pub struct ModuleFunction {
    /// Function name in Chinese (e.g., "MD5哈希")
    pub name: String,
    /// Corresponding runtime C FFI function name (e.g., "qi_crypto_md5")
    pub runtime_name: String,
    /// Parameter types
    pub param_types: Vec<String>,
    /// Return type
    pub return_type: String,
}

impl ModuleFunction {
    /// Create a new module function
    pub fn new(
        name: impl Into<String>,
        runtime_name: impl Into<String>,
        param_types: Vec<String>,
        return_type: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            runtime_name: runtime_name.into(),
            param_types,
            return_type: return_type.into(),
        }
    }
}

/// Represents a module containing related functions
#[derive(Debug, Clone)]
pub struct Module {
    /// Module name (e.g., "加密")
    pub name: String,
    /// Functions in this module
    functions: HashMap<String, ModuleFunction>,
}

impl Module {
    /// Create a new module
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            functions: HashMap::new(),
        }
    }

    /// Add a function to this module
    pub fn add_function(&mut self, function: ModuleFunction) {
        self.functions.insert(function.name.clone(), function);
    }

    /// Get a function by name
    pub fn get_function(&self, name: &str) -> Option<&ModuleFunction> {
        self.functions.get(name)
    }

    /// Check if a function exists
    pub fn has_function(&self, name: &str) -> bool {
        self.functions.contains_key(name)
    }

    /// Get all function names
    pub fn function_names(&self) -> Vec<&String> {
        self.functions.keys().collect()
    }
}

/// Registry for managing all available modules
#[derive(Debug, Clone)]
pub struct ModuleRegistry {
    /// Registered modules: path -> Module
    /// e.g., "标准库.加密" -> Module
    modules: HashMap<String, Module>,
}

impl ModuleRegistry {
    /// Create a new module registry
    pub fn new() -> Self {
        let mut registry = Self {
            modules: HashMap::new(),
        };
        registry.register_stdlib_modules();
        registry
    }

    /// Register all standard library modules
    fn register_stdlib_modules(&mut self) {
        // Register crypto module (加密模块)
        self.register_crypto_module();

        // Register IO module (IO模块)
        self.register_io_module();

        // Register network module (网络模块)
        self.register_network_module();

        // Register HTTP module (HTTP模块)
        self.register_http_module();

        // Register WebSocket module (WebSocket模块)
        self.register_websocket_module();

        // Register vector module (向量模块)
        self.register_vector_module();

        // Register data structure modules (数据结构模块)
        self.register_list_module();
        self.register_hashmap_module();

        // Register datetime module (日期时间模块)
        self.register_datetime_module();

        // Register JSON module (JSON模块)
        self.register_json_module();

        // Register MCP Server module (MCP服务器模块)
        self.register_mcp_module();

        // Register MCP Client core module (标准库.MCP客户端)
        self.register_mcp_client_module();

        // Register string module (字符串模块)
        self.register_string_module();

        // Register new standard library modules
        self.register_regex_module();
        self.register_path_module();
        self.register_random_module();
        self.register_env_module();
        self.register_process_module();
        self.register_subprocess_module();
        self.register_config_module();
        self.register_compress_module();
        self.register_test_module();
        self.register_database_module();
        self.register_web_runtime_module();
        self.register_tls_module();
        self.register_sync_module();
        self.register_reflect_module();
        self.register_plugin_module();
        self.register_tool_control_module();
    }

    fn register_tool_control_module(&mut self) {
        let mut module = Module::new("工具控制");
        for declaration in TOOL_CONTROL_ABI {
            module.add_function(ModuleFunction::new(
                declaration.qi_name,
                declaration.runtime_name,
                declaration
                    .param_types
                    .iter()
                    .map(|ty| (*ty).to_string())
                    .collect(),
                declaration.return_type,
            ));
        }
        self.modules.insert("工具控制".to_string(), module.clone());
        self.modules.insert("标准库.工具控制".to_string(), module);
    }

    /// 运行时反射注册表（反射）：Qi 程序自省当前系统的中文函数 / 结构体 / 枚举。
    /// 元数据由 codegen 在 main 序言里 qi_reflect_register_* 灌入（见 反射.rs）。
    fn register_reflect_module(&mut self) {
        let mut m = Module::new("反射");
        // 列表：返回 数组<字符串>
        m.add_function(ModuleFunction::new(
            "函数列表",
            "qi_reflect_function_list",
            vec![],
            "字符串数组",
        ));
        m.add_function(ModuleFunction::new(
            "结构体列表",
            "qi_reflect_struct_list",
            vec![],
            "字符串数组",
        ));
        m.add_function(ModuleFunction::new(
            "枚举列表",
            "qi_reflect_enum_list",
            vec![],
            "字符串数组",
        ));
        // 描述文本
        m.add_function(ModuleFunction::new(
            "函数签名",
            "qi_reflect_function_signature",
            vec!["字符串".to_string()],
            "字符串",
        ));
        m.add_function(ModuleFunction::new(
            "结构体字段",
            "qi_reflect_struct_fields",
            vec!["字符串".to_string()],
            "字符串",
        ));
        // 判定 / 索引遍历
        // 返回整数 1/0（而非布尔）—— 运行时 FFI 按 i64 返回，避免 i1/i64 ABI 错配。
        m.add_function(ModuleFunction::new(
            "有函数",
            "qi_reflect_has_function",
            vec!["字符串".to_string()],
            "整数",
        ));
        m.add_function(ModuleFunction::new(
            "函数数量",
            "qi_reflect_function_count",
            vec![],
            "整数",
        ));
        m.add_function(ModuleFunction::new(
            "函数名",
            "qi_reflect_function_name",
            vec!["整数".to_string()],
            "字符串",
        ));
        m.add_function(ModuleFunction::new(
            "结构体数量",
            "qi_reflect_struct_count",
            vec![],
            "整数",
        ));
        m.add_function(ModuleFunction::new(
            "结构体名",
            "qi_reflect_struct_name",
            vec!["整数".to_string()],
            "字符串",
        ));
        self.modules.insert("反射".to_string(), m.clone());
        self.modules.insert("标准库.反射".to_string(), m);
    }

    /// dlopen 插件热加载（插件）：运行中加载 Qi 编的 .so/.dylib、调导出函数、卸载换新版。
    /// 句柄 / 函数指针以不透明整数（i64 位模式）表示，见 plugin_ffi.rs。
    fn register_plugin_module(&mut self) {
        let mut m = Module::new("插件");
        m.add_function(ModuleFunction::new(
            "加载",
            "qi_plugin_load",
            vec!["字符串".to_string()],
            "整数",
        ));
        m.add_function(ModuleFunction::new(
            "取函数",
            "qi_plugin_sym",
            vec!["整数".to_string(), "字符串".to_string()],
            "整数",
        ));
        m.add_function(ModuleFunction::new(
            "调用整数",
            "qi_plugin_call_i64",
            vec!["整数".to_string(), "整数".to_string()],
            "整数",
        ));
        m.add_function(ModuleFunction::new(
            "调用无参整数",
            "qi_plugin_call_i64_noarg",
            vec!["整数".to_string()],
            "整数",
        ));
        m.add_function(ModuleFunction::new(
            "调用字符串",
            "qi_plugin_call_str",
            vec!["整数".to_string(), "字符串".to_string()],
            "字符串",
        ));
        m.add_function(ModuleFunction::new(
            "卸载",
            "qi_plugin_unload",
            vec!["整数".to_string()],
            "整数",
        ));
        m.add_function(ModuleFunction::new(
            "错误",
            "qi_plugin_error",
            vec![],
            "字符串",
        ));
        self.modules.insert("插件".to_string(), m.clone());
        self.modules.insert("标准库.插件".to_string(), m);
    }

    fn register_tls_module(&mut self) {
        let mut tls_module = Module::new("TLS");

        tls_module.add_function(ModuleFunction::new(
            "创建配置",
            "qi_tls_create_config",
            vec!["字符串".to_string(), "字符串".to_string()], // 证书路径, 私钥路径
            "整数",
        ));
        tls_module.add_function(ModuleFunction::new(
            "释放配置",
            "qi_tls_free_config",
            vec!["整数".to_string()],
            "整数",
        ));
        tls_module.add_function(ModuleFunction::new(
            "TLS监听",
            "qi_tls_listen",
            vec![
                "字符串".to_string(),
                "整数".to_string(),
                "整数".to_string(),
                "整数".to_string(),
            ], // host, port, backlog, config_handle
            "整数",
        ));
        tls_module.add_function(ModuleFunction::new(
            "TLS接受连接",
            "qi_tls_accept",
            vec!["整数".to_string()],
            "整数",
        ));
        tls_module.add_function(ModuleFunction::new(
            "TLS读取",
            "qi_tls_read_string",
            vec!["整数".to_string(), "整数".to_string()],
            "字符串",
        ));
        tls_module.add_function(ModuleFunction::new(
            "TLS写入",
            "qi_tls_write_string",
            vec!["整数".to_string(), "字符串".to_string()],
            "整数",
        ));
        tls_module.add_function(ModuleFunction::new(
            "TLS关闭",
            "qi_tls_close",
            vec!["整数".to_string()],
            "整数",
        ));
        tls_module.add_function(ModuleFunction::new(
            "TLS服务器关闭",
            "qi_tls_server_close",
            vec!["整数".to_string()],
            "整数",
        ));

        self.modules.insert("TLS".to_string(), tls_module.clone());
        self.modules.insert("标准库.TLS".to_string(), tls_module);

        let mut h2_module = Module::new("HTTP2");
        h2_module.add_function(ModuleFunction::new(
            "运行服务器",
            "qi_h2_serve",
            vec![
                "字符串".to_string(), // 证书路径
                "字符串".to_string(), // 私钥路径
                "字符串".to_string(), // 主机
                "整数".to_string(),   // 端口
                "指针".to_string(),   // 处理函数指针 (处理原始请求)
                "指针".to_string(),   // 应用值指针
            ],
            "整数",
        ));
        self.modules.insert("HTTP2".to_string(), h2_module.clone());
        self.modules.insert("标准库.HTTP2".to_string(), h2_module);

        let mut bytes_module = Module::new("字节切片");
        bytes_module.add_function(ModuleFunction::new(
            "创建",
            "qi_bytes_create",
            vec![],
            "整数",
        ));
        bytes_module.add_function(ModuleFunction::new(
            "创建带容量",
            "qi_bytes_with_capacity",
            vec!["整数".to_string()],
            "整数",
        ));
        bytes_module.add_function(ModuleFunction::new(
            "从字符串",
            "qi_bytes_from_string",
            vec!["字符串".to_string()],
            "整数",
        ));
        bytes_module.add_function(ModuleFunction::new(
            "转字符串",
            "qi_bytes_to_string",
            vec!["整数".to_string()],
            "字符串",
        ));
        bytes_module.add_function(ModuleFunction::new(
            "长度",
            "qi_bytes_length",
            vec!["整数".to_string()],
            "整数",
        ));
        bytes_module.add_function(ModuleFunction::new(
            "获取",
            "qi_bytes_get",
            vec!["整数".to_string(), "整数".to_string()],
            "整数",
        ));
        bytes_module.add_function(ModuleFunction::new(
            "设置",
            "qi_bytes_set",
            vec!["整数".to_string(), "整数".to_string(), "整数".to_string()],
            "整数",
        ));
        bytes_module.add_function(ModuleFunction::new(
            "追加字节",
            "qi_bytes_push",
            vec!["整数".to_string(), "整数".to_string()],
            "整数",
        ));
        bytes_module.add_function(ModuleFunction::new(
            "追加字符串",
            "qi_bytes_push_string",
            vec!["整数".to_string(), "字符串".to_string()],
            "整数",
        ));
        bytes_module.add_function(ModuleFunction::new(
            "追加切片",
            "qi_bytes_extend",
            vec!["整数".to_string(), "整数".to_string()],
            "整数",
        ));
        bytes_module.add_function(ModuleFunction::new(
            "切片",
            "qi_bytes_slice",
            vec!["整数".to_string(), "整数".to_string(), "整数".to_string()],
            "整数",
        ));
        bytes_module.add_function(ModuleFunction::new(
            "比较",
            "qi_bytes_compare",
            vec!["整数".to_string(), "整数".to_string()],
            "整数",
        ));
        bytes_module.add_function(ModuleFunction::new(
            "查找",
            "qi_bytes_find",
            vec!["整数".to_string(), "整数".to_string()],
            "整数",
        ));
        bytes_module.add_function(ModuleFunction::new(
            "写入文件",
            "qi_bytes_write_file",
            vec!["整数".to_string(), "字符串".to_string()],
            "整数",
        ));
        bytes_module.add_function(ModuleFunction::new(
            "转十六进制",
            "qi_bytes_to_hex",
            vec!["整数".to_string()],
            "字符串",
        ));
        bytes_module.add_function(ModuleFunction::new(
            "从十六进制",
            "qi_bytes_from_hex",
            vec!["字符串".to_string()],
            "整数",
        ));
        bytes_module.add_function(ModuleFunction::new(
            "转Base64",
            "qi_bytes_to_base64",
            vec!["整数".to_string()],
            "字符串",
        ));
        bytes_module.add_function(ModuleFunction::new(
            "从Base64",
            "qi_bytes_from_base64",
            vec!["字符串".to_string()],
            "整数",
        ));
        bytes_module.add_function(ModuleFunction::new(
            "释放切片",
            "qi_bytes_free",
            vec!["整数".to_string()],
            "整数",
        ));

        self.modules
            .insert("字节切片".to_string(), bytes_module.clone());
        self.modules
            .insert("标准库.字节切片".to_string(), bytes_module);

        let mut signal_module = Module::new("信号");
        signal_module.add_function(ModuleFunction::new(
            "安装关闭处理",
            "qi_signal_install_shutdown",
            vec![],
            "整数",
        ));
        signal_module.add_function(ModuleFunction::new(
            "应关闭",
            "qi_signal_should_shutdown",
            vec![],
            "整数",
        ));
        signal_module.add_function(ModuleFunction::new(
            "重置",
            "qi_signal_reset",
            vec![],
            "整数",
        ));
        self.modules
            .insert("信号".to_string(), signal_module.clone());
        self.modules
            .insert("标准库.信号".to_string(), signal_module);

        let mut mp_module = Module::new("多部分");
        mp_module.add_function(ModuleFunction::new(
            "解析",
            "qi_multipart_parse",
            vec!["整数".to_string(), "字符串".to_string()],
            "整数",
        ));
        mp_module.add_function(ModuleFunction::new(
            "提取边界",
            "qi_multipart_extract_boundary",
            vec!["字符串".to_string()],
            "字符串",
        ));
        mp_module.add_function(ModuleFunction::new(
            "数量",
            "qi_multipart_count",
            vec!["整数".to_string()],
            "整数",
        ));
        mp_module.add_function(ModuleFunction::new(
            "字段名",
            "qi_multipart_name",
            vec!["整数".to_string(), "整数".to_string()],
            "字符串",
        ));
        mp_module.add_function(ModuleFunction::new(
            "文件名",
            "qi_multipart_filename",
            vec!["整数".to_string(), "整数".to_string()],
            "字符串",
        ));
        mp_module.add_function(ModuleFunction::new(
            "内容类型",
            "qi_multipart_content_type",
            vec!["整数".to_string(), "整数".to_string()],
            "字符串",
        ));
        mp_module.add_function(ModuleFunction::new(
            "主体",
            "qi_multipart_body",
            vec!["整数".to_string(), "整数".to_string()],
            "整数",
        ));
        mp_module.add_function(ModuleFunction::new(
            "释放部分",
            "qi_multipart_free",
            vec!["整数".to_string()],
            "整数",
        ));
        self.modules.insert("多部分".to_string(), mp_module.clone());
        self.modules.insert("标准库.多部分".to_string(), mp_module);
    }

    fn register_web_runtime_module(&mut self) {
        let mut web_module = Module::new("Web运行时");

        for declaration in WEB_RUNTIME_ABI {
            web_module.add_function(ModuleFunction::new(
                declaration.qi_name,
                declaration.runtime_name,
                declaration
                    .param_types
                    .iter()
                    .map(|ty| (*ty).to_string())
                    .collect(),
                declaration.return_type,
            ));
        }

        // 调用一个 ptr→ptr 的 handler 时用 catch_unwind 包裹；panic 时返回 0/null
        web_module.add_function(ModuleFunction::new(
            "安全调用处理器",
            "qi_web_call_handler_safe",
            vec!["指针".to_string(), "指针".to_string()], // handler 函数指针, 上下文指针
            "指针",                                       // 响应指针（panic 时为 null）
        ));

        web_module.add_function(ModuleFunction::new(
            "安全处理请求",
            "qi_web_safe_process_request",
            vec!["指针".to_string(), "指针".to_string(), "字符串".to_string()],
            "字符串", // panic 时返回预制 500 响应
        ));

        web_module.add_function(ModuleFunction::new(
            "测试_故意panic",
            "qi_web_panic_for_test",
            vec![],
            "整数",
        ));

        // 一次性 HTTP/1.1 响应序列化：避免 qi 端 8 次字符串 + 拼接 + 字节池往返
        web_module.add_function(ModuleFunction::new(
            "序列化响应",
            "qi_runtime_serialize_http_response",
            vec![
                "整数".to_string(),
                "字符串".to_string(),
                "字符串".to_string(),
                "字符串".to_string(),
            ],
            "整数", // 字节切片句柄
        ));

        // HTTP 请求解析 fast path —— 替代 qi 端 13 次 字符串::子串/查找
        web_module.add_function(ModuleFunction::new(
            "解析请求字节",
            "qi_web_parse_request_bytes",
            vec!["整数".to_string()],
            "整数", // 不透明 RequestParts 指针
        ));
        web_module.add_function(ModuleFunction::new(
            "解析请求字符串",
            "qi_web_parse_request_cstr",
            vec!["字符串".to_string()],
            "整数",
        ));
        web_module.add_function(ModuleFunction::new(
            "请求方法",
            "qi_web_request_method",
            vec!["整数".to_string()],
            "字符串",
        ));
        web_module.add_function(ModuleFunction::new(
            "请求路径",
            "qi_web_request_path",
            vec!["整数".to_string()],
            "字符串",
        ));
        web_module.add_function(ModuleFunction::new(
            "请求查询",
            "qi_web_request_query",
            vec!["整数".to_string()],
            "字符串",
        ));
        web_module.add_function(ModuleFunction::new(
            "请求头部",
            "qi_web_request_headers",
            vec!["整数".to_string()],
            "字符串",
        ));
        web_module.add_function(ModuleFunction::new(
            "请求主体",
            "qi_web_request_body",
            vec!["整数".to_string()],
            "字符串",
        ));
        web_module.add_function(ModuleFunction::new(
            "释放请求",
            "qi_web_request_parts_free",
            vec!["整数".to_string()],
            "整数", // 实际返回 0 — 用 整数 避开 void-call 赋值问题
        ));

        // 预解析的 keep-alive 标志（HTTP/1.1 默认 1 除非 Connection: close）
        web_module.add_function(ModuleFunction::new(
            "请求保持连接",
            "qi_web_request_keep_alive",
            vec!["整数".to_string()],
            "整数",
        ));

        // 一次性序列化 + 自动 Connection 头
        web_module.add_function(ModuleFunction::new(
            "序列化响应保持",
            "qi_runtime_serialize_http_response_ka",
            vec![
                "整数".to_string(),
                "字符串".to_string(),
                "字符串".to_string(),
                "字符串".to_string(),
                "整数".to_string(),
            ],
            "整数",
        ));

        // 缓存体：预构建完整 HTTP 响应（ka/close 两变体）注册为持久字节，
        // handler 用 X-Qi-Cached-Body 标记头引用 → 每请求零分配零拷贝直发。
        // (body, content_type) → 体id（>0；失败 -1）
        web_module.add_function(ModuleFunction::new(
            "缓存体注册",
            "qi_web_cache_body_register",
            vec!["字符串".to_string(), "字符串".to_string()],
            "整数",
        ));

        // 路由 Rust 镜像表 —— 注册时同步推一份；匹配走 Rust
        web_module.add_function(ModuleFunction::new(
            "路由注册",
            "qi_web_router_register",
            vec![
                "字符串".to_string(), // method
                "字符串".to_string(), // path
                "整数".to_string(),   // handler index
            ],
            "整数", // 0 ok / -1 unknown method / -2 param conflict
        ));
        web_module.add_function(ModuleFunction::new(
            "路由匹配",
            "qi_web_router_match",
            vec!["字符串".to_string(), "字符串".to_string()],
            "整数", // *mut MatchResult，0 = 路径不存在
        ));
        web_module.add_function(ModuleFunction::new(
            "匹配处理器",
            "qi_web_match_handler",
            vec!["整数".to_string()],
            "整数",
        ));
        web_module.add_function(ModuleFunction::new(
            "匹配路径命中",
            "qi_web_match_path_hit",
            vec!["整数".to_string()],
            "整数",
        ));
        web_module.add_function(ModuleFunction::new(
            "匹配参数",
            "qi_web_match_params",
            vec!["整数".to_string()],
            "字符串",
        ));
        web_module.add_function(ModuleFunction::new(
            "匹配方法掩码",
            "qi_web_match_method_mask",
            vec!["整数".to_string()],
            "整数",
        ));
        web_module.add_function(ModuleFunction::new(
            "匹配释放",
            "qi_web_match_free",
            vec!["整数".to_string()],
            "整数",
        ));

        // 一次 alloc 构建 请求标识文本（替代 prefix + "-" + int_to_string 三步链）
        web_module.add_function(ModuleFunction::new(
            "构建请求标识",
            "qi_web_build_request_id",
            vec!["字符串".to_string(), "整数".to_string()],
            "字符串",
        ));

        self.modules
            .insert("Web运行时".to_string(), web_module.clone());
        self.modules
            .insert("标准库.Web运行时".to_string(), web_module);
    }

    /// Register the crypto module
    fn register_crypto_module(&mut self) {
        let mut crypto_module = Module::new("加密");

        // MD5哈希
        crypto_module.add_function(ModuleFunction::new(
            "MD5哈希",
            "qi_crypto_md5",
            vec!["字符串".to_string()],
            "字符串",
        ));

        // SHA256哈希
        crypto_module.add_function(ModuleFunction::new(
            "SHA256哈希",
            "qi_crypto_sha256",
            vec!["字符串".to_string()],
            "字符串",
        ));

        // SHA512哈希
        crypto_module.add_function(ModuleFunction::new(
            "SHA512哈希",
            "qi_crypto_sha512",
            vec!["字符串".to_string()],
            "字符串",
        ));

        // Base64编码
        crypto_module.add_function(ModuleFunction::new(
            "Base64编码",
            "qi_crypto_base64_encode",
            vec!["字符串".to_string()],
            "字符串",
        ));

        // Base64解码
        crypto_module.add_function(ModuleFunction::new(
            "Base64解码",
            "qi_crypto_base64_decode",
            vec!["字符串".to_string()],
            "字符串",
        ));

        // HMAC_SHA256
        crypto_module.add_function(ModuleFunction::new(
            "HMAC_SHA256",
            "qi_crypto_hmac_sha256",
            vec!["字符串".to_string(), "字符串".to_string()],
            "字符串",
        ));

        // Register module with both Chinese and path formats
        self.modules
            .insert("加密".to_string(), crypto_module.clone());
        self.modules
            .insert("标准库.加密".to_string(), crypto_module);
    }

    /// Register the IO module
    fn register_io_module(&mut self) {
        let mut io_module = Module::new("输入输出");

        // 打印函数 - 也作为内置函数可用，但也支持通过模块调用
        io_module.add_function(ModuleFunction::new(
            "打印",
            "qi_runtime_print",
            vec!["字符串".to_string()],
            "i32", // qi_runtime_print returns i32
        ));

        io_module.add_function(ModuleFunction::new(
            "打印行",
            "qi_runtime_println",
            vec!["字符串".to_string()],
            "i32", // qi_runtime_println returns i32
        ));

        // 文件操作函数
        io_module.add_function(ModuleFunction::new(
            "读取文件",
            "qi_io_read_file",
            vec!["字符串".to_string()],
            "字符串",
        ));

        io_module.add_function(ModuleFunction::new(
            "写入文件",
            "qi_io_write_file",
            vec!["字符串".to_string(), "字符串".to_string()],
            "整数", // Returns 0 or 1 as i64
        ));

        io_module.add_function(ModuleFunction::new(
            "追加文件",
            "qi_io_append_file",
            vec!["字符串".to_string(), "字符串".to_string()],
            "整数", // Returns 0 or 1 as i64
        ));

        io_module.add_function(ModuleFunction::new(
            "删除文件",
            "qi_io_delete_file",
            vec!["字符串".to_string()],
            "整数", // Returns 0 or 1 as i64
        ));

        io_module.add_function(ModuleFunction::new(
            "创建文件",
            "qi_io_create_file",
            vec!["字符串".to_string()],
            "整数", // Returns 0 or 1 as i64
        ));

        io_module.add_function(ModuleFunction::new(
            "文件存在",
            "qi_io_file_exists",
            vec!["字符串".to_string()],
            "整数", // Returns 0 or 1 as i64
        ));

        io_module.add_function(ModuleFunction::new(
            "文件大小",
            "qi_io_file_size",
            vec!["字符串".to_string()],
            "整数",
        ));

        io_module.add_function(ModuleFunction::new(
            "创建符号链接",
            "qi_io_symlink",
            vec!["字符串".to_string(), "字符串".to_string()], // 目标, 链接路径
            "整数",
        ));
        io_module.add_function(ModuleFunction::new(
            "创建目录",
            "qi_io_create_dir",
            vec!["字符串".to_string()],
            "整数", // Returns 0 or 1 as i64
        ));

        io_module.add_function(ModuleFunction::new(
            "删除目录",
            "qi_io_delete_dir",
            vec!["字符串".to_string()],
            "整数", // Returns 0 or 1 as i64
        ));

        // Register module with both Chinese and path formats
        self.modules
            .insert("输入输出".to_string(), io_module.clone());
        self.modules
            .insert("标准库.输入输出".to_string(), io_module);
    }

    /// Register the network module
    fn register_network_module(&mut self) {
        let mut network_module = Module::new("网络");

        // TCP 连接函数
        network_module.add_function(ModuleFunction::new(
            "TCP连接",
            "qi_network_tcp_connect",
            vec!["字符串".to_string(), "整数".to_string(), "整数".to_string()], // 主机, 端口, 超时(毫秒)
            "整数",                                                             // 返回连接句柄
        ));

        network_module.add_function(ModuleFunction::new(
            "TCP读取",
            "qi_network_tcp_read_string",
            vec!["整数".to_string(), "整数".to_string()], // 句柄, 缓冲区大小
            "字符串",                                     // 返回读取的字符串
        ));

        network_module.add_function(ModuleFunction::new(
            "TCP写入",
            "qi_network_tcp_write_string",
            vec!["整数".to_string(), "字符串".to_string()], // 句柄, 数据字符串
            "整数",                                         // 返回写入字节数
        ));

        network_module.add_function(ModuleFunction::new(
            "TCP关闭",
            "qi_network_tcp_close",
            vec!["整数".to_string()], // 句柄
            "整数",                   // 返回成功/失败
        ));

        network_module.add_function(ModuleFunction::new(
            "TCP刷新",
            "qi_network_tcp_flush",
            vec!["整数".to_string()], // 句柄
            "整数",                   // 返回成功/失败
        ));

        network_module.add_function(ModuleFunction::new(
            "解析主机",
            "qi_network_resolve_host",
            vec!["字符串".to_string()], // 主机名
            "字符串",                   // 返回 IP 地址
        ));

        network_module.add_function(ModuleFunction::new(
            "端口可用",
            "qi_network_port_available",
            vec!["整数".to_string()], // 端口
            "整数",                   // 返回 1 可用，0 不可用
        ));

        network_module.add_function(ModuleFunction::new(
            "获取本机IP",
            "qi_network_get_local_ip",
            vec![],   // 无参数
            "字符串", // 返回本机 IP
        ));

        // TCP Server functions
        network_module.add_function(ModuleFunction::new(
            "TCP监听",
            "qi_network_tcp_listen",
            vec!["字符串".to_string(), "整数".to_string(), "整数".to_string()], // 主机, 端口, 队列大小
            "整数",                                                             // 返回服务器句柄
        ));

        network_module.add_function(ModuleFunction::new(
            "TCP接受连接",
            "qi_network_tcp_accept",
            vec!["整数".to_string()], // 服务器句柄
            "整数",                   // 返回客户端句柄
        ));

        network_module.add_function(ModuleFunction::new(
            "TCP服务器关闭",
            "qi_network_tcp_server_close",
            vec!["整数".to_string()], // 服务器句柄
            "整数",                   // 返回成功/失败
        ));

        network_module.add_function(ModuleFunction::new(
            "TCP设置非阻塞",
            "qi_network_tcp_listener_set_nonblocking",
            vec!["整数".to_string(), "整数".to_string()], // 服务器句柄, 0/1
            "整数",
        ));

        // 真 M:N 异步服务器：runtime 接管整个 accept 循环 + 每条连接的 IO。
        // 完整性检测在 Rust 侧内联，不再调 Qi 回调。
        // (服务器句柄, 处理函数, 应用值指针) -> 0 成功 / -1 错误
        network_module.add_function(ModuleFunction::new(
            "异步服务",
            "qi_runtime_async_serve",
            vec!["整数".to_string(), "ptr".to_string(), "ptr".to_string()],
            "整数",
        ));

        network_module.add_function(ModuleFunction::new(
            "TCP读取字节",
            "qi_network_tcp_read_bytes",
            vec!["整数".to_string(), "整数".to_string()],
            "整数",
        ));
        network_module.add_function(ModuleFunction::new(
            "TCP写入字节",
            "qi_network_tcp_write_bytes",
            vec!["整数".to_string(), "整数".to_string()],
            "整数",
        ));

        // 异步 TCP IO — 返回 未来<整数>，用 等待 关键字消费
        network_module.add_function(ModuleFunction::new(
            "异步TCP连接",
            "qi_network_async_tcp_connect",
            vec!["字符串".to_string(), "整数".to_string()],
            "未来<整数>",
        ));
        network_module.add_function(ModuleFunction::new(
            "异步TCP读取字节",
            "qi_network_async_tcp_read_bytes",
            vec!["整数".to_string(), "整数".to_string()],
            "未来<整数>",
        ));
        network_module.add_function(ModuleFunction::new(
            "异步TCP写入字节",
            "qi_network_async_tcp_write_bytes",
            vec!["整数".to_string(), "整数".to_string()],
            "未来<整数>",
        ));
        network_module.add_function(ModuleFunction::new(
            "异步TCP关闭",
            "qi_network_async_tcp_close",
            vec!["整数".to_string()],
            "整数",
        ));
        network_module.add_function(ModuleFunction::new(
            "异步TCP监听",
            "qi_network_async_tcp_listen",
            vec!["字符串".to_string(), "整数".to_string()],
            "未来<整数>",
        ));
        network_module.add_function(ModuleFunction::new(
            "异步TCP接受",
            "qi_network_async_tcp_accept",
            vec!["整数".to_string()],
            "未来<整数>",
        ));
        network_module.add_function(ModuleFunction::new(
            "异步TCP监听关闭",
            "qi_network_async_tcp_listener_close",
            vec!["整数".to_string()],
            "整数",
        ));

        // UDP functions
        network_module.add_function(ModuleFunction::new(
            "UDP绑定",
            "qi_network_udp_bind",
            vec!["字符串".to_string(), "整数".to_string()], // 主机, 端口
            "整数",                                         // 返回 UDP 套接字句柄
        ));

        network_module.add_function(ModuleFunction::new(
            "UDP发送到",
            "qi_network_udp_send_string",
            vec![
                "整数".to_string(),
                "字符串".to_string(),
                "字符串".to_string(),
                "整数".to_string(),
            ], // 句柄, 消息, 目标主机, 目标端口
            "整数", // 返回发送字节数
        ));

        network_module.add_function(ModuleFunction::new(
            "UDP接收",
            "qi_network_udp_recv_string",
            vec!["整数".to_string(), "整数".to_string()], // 句柄, 缓冲区大小
            "字符串",                                     // 返回接收到的数据
        ));

        network_module.add_function(ModuleFunction::new(
            "UDP关闭",
            "qi_network_udp_close",
            vec!["整数".to_string()], // 句柄
            "整数",                   // 返回成功/失败
        ));

        network_module.add_function(ModuleFunction::new(
            "UDP设置超时",
            "qi_network_udp_set_timeout",
            vec!["整数".to_string(), "整数".to_string()], // 句柄, 超时毫秒
            "整数",                                       // 返回成功/失败
        ));

        network_module.add_function(ModuleFunction::new(
            "UDP设置广播",
            "qi_network_udp_set_broadcast",
            vec!["整数".to_string(), "整数".to_string()], // 句柄, 启用(1)/禁用(0)
            "整数",                                       // 返回成功/失败
        ));

        // Register module with both Chinese and path formats
        self.modules
            .insert("网络".to_string(), network_module.clone());
        self.modules
            .insert("标准库.网络".to_string(), network_module);
    }

    /// Register the HTTP module
    fn register_http_module(&mut self) {
        let mut http_module = Module::new("HTTP");

        // 基本 HTTP 请求方法 (使用全中文函数名)
        http_module.add_function(ModuleFunction::new(
            "获取",
            "qi_http_get",
            vec!["字符串".to_string()], // URL
            "字符串",                   // 返回响应体
        ));

        http_module.add_function(ModuleFunction::new(
            "发送",
            "qi_http_post",
            vec!["字符串".to_string(), "字符串".to_string()], // URL, 请求体
            "字符串",                                         // 返回响应体
        ));

        http_module.add_function(ModuleFunction::new(
            "更新",
            "qi_http_put",
            vec!["字符串".to_string(), "字符串".to_string()], // URL, 请求体
            "字符串",                                         // 返回响应体
        ));

        http_module.add_function(ModuleFunction::new(
            "删除",
            "qi_http_delete",
            vec!["字符串".to_string()], // URL
            "字符串",                   // 返回响应体
        ));

        http_module.add_function(ModuleFunction::new(
            "请求头",
            "qi_http_head",
            vec!["字符串".to_string()], // URL
            "字符串",                   // 返回状态信息
        ));

        http_module.add_function(ModuleFunction::new(
            "修补",
            "qi_http_patch",
            vec!["字符串".to_string(), "字符串".to_string()], // URL, 请求体
            "字符串",                                         // 返回响应体
        ));

        http_module.add_function(ModuleFunction::new(
            "选项",
            "qi_http_options",
            vec!["字符串".to_string()], // URL
            "字符串",                   // 返回响应体
        ));

        http_module.add_function(ModuleFunction::new(
            "请求",
            "qi_http_request",
            vec![
                "字符串".to_string(),
                "字符串".to_string(),
                "字符串".to_string(),
                "字符串".to_string(),
            ],
            "字符串",
        ));

        // 高级请求构建器
        http_module.add_function(ModuleFunction::new(
            "创建请求",
            "qi_http_request_create",
            vec!["字符串".to_string(), "字符串".to_string()], // 方法, URL
            "整数",                                           // 返回请求句柄
        ));

        http_module.add_function(ModuleFunction::new(
            "设置请求头",
            "qi_http_request_set_header",
            vec![
                "整数".to_string(),
                "字符串".to_string(),
                "字符串".to_string(),
            ], // 句柄, 名称, 值
            "整数", // 返回成功/失败
        ));

        http_module.add_function(ModuleFunction::new(
            "设置请求体",
            "qi_http_request_set_body",
            vec!["整数".to_string(), "字符串".to_string()], // 句柄, 请求体
            "整数",                                         // 返回成功/失败
        ));

        http_module.add_function(ModuleFunction::new(
            "设置超时",
            "qi_http_request_set_timeout",
            vec!["整数".to_string(), "整数".to_string()], // 句柄, 超时(毫秒)
            "整数",                                       // 返回成功/失败
        ));

        http_module.add_function(ModuleFunction::new(
            "执行请求",
            "qi_http_request_execute",
            vec!["整数".to_string()], // 句柄
            "字符串",                 // 返回响应体
        ));

        http_module.add_function(ModuleFunction::new(
            "获取状态码",
            "qi_http_get_status",
            vec!["字符串".to_string()], // URL
            "整数",                     // 返回状态码
        ));

        // HTTP 服务器功能
        http_module.add_function(ModuleFunction::new(
            "创建服务器",
            "qi_http_server_create",
            vec!["字符串".to_string(), "整数".to_string()], // 主机, 端口
            "整数",                                         // 返回服务器句柄
        ));

        http_module.add_function(ModuleFunction::new(
            "处理请求",
            "qi_http_server_handle_request",
            vec!["整数".to_string(), "字符串".to_string(), "整数".to_string()], // 服务器句柄, 响应体, 状态码
            "字符串", // 返回请求信息 "方法|路径|请求体"
        ));

        http_module.add_function(ModuleFunction::new(
            "接受连接",
            "qi_http_server_accept",
            vec!["整数".to_string()], // 服务器句柄
            "字符串",                 // 返回完整HTTP请求
        ));

        http_module.add_function(ModuleFunction::new(
            "关闭服务器",
            "qi_http_server_close",
            vec!["整数".to_string()], // 服务器句柄
            "整数",                   // 返回成功/失败
        ));

        // Register module with both Chinese and path formats
        self.modules.insert("HTTP".to_string(), http_module.clone());
        self.modules.insert("标准库.HTTP".to_string(), http_module);
    }

    /// Register the WebSocket module
    fn register_websocket_module(&mut self) {
        let mut ws_module = Module::new("WebSocket");

        // WebSocket客户端连接
        ws_module.add_function(ModuleFunction::new(
            "连接",
            "qi_websocket_connect",
            vec!["字符串".to_string()], // URL (ws://host:port/path)
            "整数",                     // 返回连接句柄，-1表示失败
        ));

        // WebSocket服务端接受连接（升级HTTP连接为WebSocket）
        ws_module.add_function(ModuleFunction::new(
            "接受升级",
            "qi_websocket_accept",
            vec!["字符串".to_string(), "整数".to_string()], // host, port
            "整数",                                         // 返回连接句柄，-1表示失败
        ));

        // 发送文本消息
        ws_module.add_function(ModuleFunction::new(
            "发送文本",
            "qi_websocket_send_text",
            vec!["整数".to_string(), "字符串".to_string()], // 句柄, 消息
            "整数",                                         // 返回0成功，-1失败
        ));

        // 接收文本消息
        ws_module.add_function(ModuleFunction::new(
            "接收文本",
            "qi_websocket_recv_text",
            vec!["整数".to_string()], // 句柄
            "字符串",                 // 返回接收到的消息
        ));

        // 发送二进制数据
        ws_module.add_function(ModuleFunction::new(
            "发送二进制",
            "qi_websocket_send_binary",
            vec!["整数".to_string(), "字符串".to_string(), "整数".to_string()], // 句柄, 数据指针, 长度
            "整数",                                                             // 返回0成功，-1失败
        ));

        // 发送Ping帧
        ws_module.add_function(ModuleFunction::new(
            "发送心跳",
            "qi_websocket_ping",
            vec!["整数".to_string()], // 句柄
            "整数",                   // 返回0成功，-1失败
        ));

        // 关闭连接
        ws_module.add_function(ModuleFunction::new(
            "关闭",
            "qi_websocket_close",
            vec!["整数".to_string(), "整数".to_string(), "字符串".to_string()], // 句柄, 状态码, 原因
            "整数",                                                             // 返回0成功，-1失败
        ));

        // 检查连接状态
        ws_module.add_function(ModuleFunction::new(
            "已连接",
            "qi_websocket_is_connected",
            vec!["整数".to_string()], // 句柄
            "整数",                   // 返回1已连接，0未连接
        ));

        // 检查是否为WebSocket升级请求
        ws_module.add_function(ModuleFunction::new(
            "是升级请求",
            "qi_websocket_is_upgrade_request",
            vec!["字符串".to_string()], // HTTP请求头
            "整数",                     // 返回1是，0否
        ));

        // 获取客户端的WebSocket Key
        ws_module.add_function(ModuleFunction::new(
            "获取客户端密钥",
            "qi_websocket_get_client_key",
            vec!["字符串".to_string()], // HTTP请求头
            "字符串",                   // 返回Sec-WebSocket-Key
        ));

        // 创建WebSocket升级响应
        ws_module.add_function(ModuleFunction::new(
            "创建升级响应",
            "qi_websocket_create_upgrade_response",
            vec!["字符串".to_string()], // 客户端密钥
            "字符串",                   // 返回完整的HTTP升级响应
        ));

        // 释放字符串内存
        ws_module.add_function(ModuleFunction::new(
            "释放字符串",
            "qi_websocket_free_string",
            vec!["字符串".to_string()], // 字符串指针
            "空",
        ));

        // 将TCP连接注册为WebSocket连接
        ws_module.add_function(ModuleFunction::new(
            "注册TCP连接",
            "qi_websocket_register_tcp",
            vec!["整数".to_string(), "整数".to_string()], // TCP文件描述符, 是否服务器端(1/0)
            "整数",                                       // 返回WebSocket句柄
        ));

        // 注销WebSocket连接（不关闭底层TCP）
        ws_module.add_function(ModuleFunction::new(
            "注销连接",
            "qi_websocket_unregister",
            vec!["整数".to_string()], // WebSocket句柄
            "整数",                   // 返回1成功，0失败
        ));

        // Register module with both Chinese and path formats
        self.modules
            .insert("WebSocket".to_string(), ws_module.clone());
        self.modules
            .insert("标准库.WebSocket".to_string(), ws_module);
    }

    /// Register the vector module
    fn register_vector_module(&mut self) {
        let mut vector_module = Module::new("向量");

        // 签名与 qi-runtime/src/stdlib/vector_ffi.rs 一一对应（Qi 友好形态：
        // 直接收 Qi 数组指针，长度从数组头读，无出参缓冲）。
        // 参数 "数组" → ptr 直传；返回 "浮点数组" → rc=1 新数组（数组(浮点数)）。

        // 向量点积: 点积(数组, 数组) : 浮点数
        vector_module.add_function(ModuleFunction::new(
            "点积",
            "qi_vector_dot",
            vec!["数组".to_string(), "数组".to_string()],
            "浮点数",
        ));

        // 向量加法: 加(数组, 数组) : 数组（返回新数组）
        vector_module.add_function(ModuleFunction::new(
            "加",
            "qi_vector_add",
            vec!["数组".to_string(), "数组".to_string()],
            "浮点数组",
        ));

        // 向量长度(模): 长度(数组) : 浮点数
        vector_module.add_function(ModuleFunction::new(
            "长度",
            "qi_vector_magnitude",
            vec!["数组".to_string()],
            "浮点数",
        ));

        // 向量归一化: 归一化(数组) : 数组（返回新数组）
        vector_module.add_function(ModuleFunction::new(
            "归一化",
            "qi_vector_normalize",
            vec!["数组".to_string()],
            "浮点数组",
        ));

        // 余弦相似度: 余弦相似度(数组, 数组) : 浮点数
        vector_module.add_function(ModuleFunction::new(
            "余弦相似度",
            "qi_vector_cosine_similarity",
            vec!["数组".to_string(), "数组".to_string()],
            "浮点数",
        ));

        // 向量数乘: 数乘(数组, 浮点数) : 数组（返回新数组）
        vector_module.add_function(ModuleFunction::new(
            "数乘",
            "qi_vector_scale",
            vec!["数组".to_string(), "浮点数".to_string()],
            "浮点数组",
        ));

        // Register module with both Chinese and path formats
        self.modules
            .insert("向量".to_string(), vector_module.clone());
        self.modules
            .insert("标准库.向量".to_string(), vector_module);

        // 向量索引 —— 内存态增量精确 top-K 相似搜索（语义记忆下沉到 Rust，比 Qi 扫描快 ~100x）
        let mut vindex_module = Module::new("向量索引");
        vindex_module.add_function(ModuleFunction::new(
            "重置",
            "qi_vindex_reset",
            vec!["整数".to_string()], // 键（用 SQLite 库句柄）
            "整数",
        ));
        vindex_module.add_function(ModuleFunction::new(
            "添加",
            "qi_vindex_add",
            vec!["整数".to_string(), "整数".to_string(), "字符串".to_string()], // 键, id, 向量JSON
            "整数",                                                             // 返回条数
        ));
        vindex_module.add_function(ModuleFunction::new(
            "搜索",
            "qi_vindex_search",
            vec!["整数".to_string(), "字符串".to_string(), "整数".to_string()], // 键, 查询向量JSON, k
            "字符串", // 返回 [{"id":..,"score":..}] JSON
        ));
        vindex_module.add_function(ModuleFunction::new(
            "大小",
            "qi_vindex_size",
            vec!["整数".to_string()],
            "整数",
        ));
        vindex_module.add_function(ModuleFunction::new(
            "卸载",
            "qi_vindex_free",
            vec!["整数".to_string()],
            "整数",
        ));
        self.modules
            .insert("向量索引".to_string(), vindex_module.clone());
        self.modules
            .insert("标准库.向量索引".to_string(), vindex_module);

        // 词法索引 —— BM25 词法检索（中文 单字+bigram，ASCII 切词），与 向量索引 成对做混合检索
        let mut lexidx_module = Module::new("词法索引");
        lexidx_module.add_function(ModuleFunction::new(
            "重置",
            "qi_lexidx_reset",
            vec!["整数".to_string()], // 键（惯例：与向量库同句柄）
            "整数",
        ));
        lexidx_module.add_function(ModuleFunction::new(
            "添加",
            "qi_lexidx_add",
            vec!["整数".to_string(), "整数".to_string(), "字符串".to_string()], // 键, id, 文本
            "整数",                                                             // 返回文档数
        ));
        lexidx_module.add_function(ModuleFunction::new(
            "搜索",
            "qi_lexidx_search",
            vec!["整数".to_string(), "字符串".to_string(), "整数".to_string()], // 键, 查询文本, k
            "字符串", // 返回 [{"id":..,"score":..}] JSON
        ));
        lexidx_module.add_function(ModuleFunction::new(
            "大小",
            "qi_lexidx_size",
            vec!["整数".to_string()],
            "整数",
        ));
        lexidx_module.add_function(ModuleFunction::new(
            "卸载",
            "qi_lexidx_free",
            vec!["整数".to_string()],
            "整数",
        ));
        self.modules
            .insert("词法索引".to_string(), lexidx_module.clone());
        self.modules
            .insert("标准库.词法索引".to_string(), lexidx_module);

        // LLM Module - 大模型模块
        let mut llm_module = Module::new("大模型");

        // 创建会话
        llm_module.add_function(ModuleFunction::new(
            "创建会话",
            "qi_llm_create_session",
            vec![
                "字符串".to_string(),
                "字符串".to_string(),
                "字符串".to_string(),
            ], // 端点, 模型, 密钥
            "整数", // 返回会话句柄
        ));

        // 对话
        llm_module.add_function(ModuleFunction::new(
            "对话",
            "qi_llm_chat",
            vec!["整数".to_string(), "字符串".to_string()], // 会话句柄, 提示
            "字符串",                                       // 返回LLM响应
        ));

        // 文本嵌入：走会话端点 base + /embeddings（OpenAI 兼容），model 用会话模型
        // （建嵌入会话时传嵌入模型，如 text-embedding-v4）。返回 数组<浮点数>（rc=1 交出）。
        // 语言级糖 `嵌入(会话, 文本)` 脱糖到此。
        llm_module.add_function(ModuleFunction::new(
            "嵌入",
            "qi_llm_embed",
            vec!["整数".to_string(), "字符串".to_string()], // 会话句柄, 文本
            "浮点数组",                                     // 返回 数组<浮点数>
        ));

        // 多模态图像对话：文本提示 + 单张图 URL（按会话 provider 构造带图消息）
        llm_module.add_function(ModuleFunction::new(
            "对话图像",
            "qi_llm_chat_image",
            vec![
                "整数".to_string(),
                "字符串".to_string(),
                "字符串".to_string(),
            ], // 会话句柄, 提示, 图像URL
            "字符串", // 返回LLM响应
        ));

        // 设置配置（含 provider："openai"/"anthropic"/"gemini"，默认 openai）
        llm_module.add_function(ModuleFunction::new(
            "设置配置",
            "qi_llm_set_config",
            vec![
                "整数".to_string(),
                "字符串".to_string(),
                "字符串".to_string(),
            ], // 会话句柄, 键, 值
            "整数", // 返回状态
        ));

        // 清空历史
        llm_module.add_function(ModuleFunction::new(
            "清空历史",
            "qi_llm_clear_history",
            vec!["整数".to_string()], // 会话句柄
            "整数",                   // 返回状态
        ));

        // 获取历史数量
        llm_module.add_function(ModuleFunction::new(
            "历史数量",
            "qi_llm_get_history_count",
            vec!["整数".to_string()], // 会话句柄
            "整数",                   // 返回数量
        ));

        // 获取整段历史（OpenAI 消息数组 JSON 串）—— 上下文窗口管理读取口
        llm_module.add_function(ModuleFunction::new(
            "历史JSON",
            "qi_llm_get_history_json",
            vec!["整数".to_string()], // 会话句柄
            "字符串",                 // 返回消息数组 JSON
        ));

        // 用 JSON 数组整体替换历史 —— 上下文压缩写回口，返回替换后条数
        llm_module.add_function(ModuleFunction::new(
            "设置历史JSON",
            "qi_llm_set_history_json",
            vec!["整数".to_string(), "字符串".to_string()], // 会话句柄, 消息数组 JSON
            "整数",                                         // 返回条数（-1 失败）
        ));

        // 最近一次非流式请求的 token 用量：JSON {"prompt":..,"completion":..,"total":..}
        llm_module.add_function(ModuleFunction::new(
            "用量",
            "qi_llm_last_usage",
            vec!["整数".to_string()], // 会话句柄
            "字符串",                 // 返回用量 JSON
        ));

        // 会话 token 预算：设上限(0=不限)后自动累计，超限直接拒绝调用（不打 API）
        llm_module.add_function(ModuleFunction::new(
            "设置预算",
            "qi_llm_set_budget",
            vec!["整数".to_string(), "整数".to_string()], // 会话句柄, token 上限
            "整数",
        ));
        llm_module.add_function(ModuleFunction::new(
            "已用预算",
            "qi_llm_budget_used",
            vec!["整数".to_string()], // 会话句柄
            "整数",                   // 累计已用 token
        ));

        // 关闭会话
        llm_module.add_function(ModuleFunction::new(
            "关闭会话",
            "qi_llm_close_session",
            vec!["整数".to_string()], // 会话句柄
            "整数",                   // 返回状态
        ));

        // 异步对话 (返回 Future<字符串>)
        llm_module.add_function(ModuleFunction::new(
            "异步对话",
            "qi_llm_chat_async",
            vec!["整数".to_string(), "字符串".to_string()], // 会话句柄, 提示
            "未来<字符串>",                                 // 返回Future<字符串>
        ));

        // 流式对话
        llm_module.add_function(ModuleFunction::new(
            "流式对话",
            "qi_llm_stream_chat",
            vec!["整数".to_string(), "字符串".to_string()], // 会话句柄, 提示
            "整数",                                         // 返回流句柄
        ));

        // v2 可靠流：事件协议、显式取消/提交，以及基于历史版本的 CAS。
        llm_module.add_function(ModuleFunction::new(
            "打开流V2",
            "qi_llm_stream_v2_open",
            vec!["整数".to_string(), "字符串".to_string()], // 会话句柄, 请求 JSON
            "整数",
        ));
        llm_module.add_function(ModuleFunction::new(
            "读取流事件V2",
            "qi_llm_stream_v2_next_event",
            vec!["整数".to_string()],
            "字符串",
        ));
        llm_module.add_function(ModuleFunction::new(
            "限时读取流事件V2",
            "qi_llm_stream_v2_next_event_timeout",
            vec!["整数".to_string(), "整数".to_string()], // 流句柄, 最长等待毫秒
            "字符串",
        ));
        llm_module.add_function(ModuleFunction::new(
            "取消流V2",
            "qi_llm_stream_v2_cancel",
            vec!["整数".to_string(), "字符串".to_string()], // 流句柄, 原因
            "整数",
        ));
        llm_module.add_function(ModuleFunction::new(
            "流快照V2",
            "qi_llm_stream_v2_snapshot",
            vec!["整数".to_string()],
            "字符串",
        ));
        llm_module.add_function(ModuleFunction::new(
            "提交流V2",
            "qi_llm_stream_v2_commit",
            vec!["整数".to_string(), "整数".to_string()], // 流句柄, 预期历史版本
            "整数",
        ));
        llm_module.add_function(ModuleFunction::new(
            "放弃流V2",
            "qi_llm_stream_v2_abort",
            vec!["整数".to_string()],
            "整数",
        ));
        llm_module.add_function(ModuleFunction::new(
            "关闭流V2",
            "qi_llm_stream_v2_close",
            vec!["整数".to_string()],
            "整数",
        ));
        llm_module.add_function(ModuleFunction::new(
            "历史版本",
            "qi_llm_history_revision",
            vec!["整数".to_string()],
            "整数",
        ));
        llm_module.add_function(ModuleFunction::new(
            "能力版本",
            "qi_llm_runtime_capability",
            vec!["字符串".to_string()],
            "整数",
        ));

        // 读取流片段
        llm_module.add_function(ModuleFunction::new(
            "读取流",
            "qi_llm_stream_next",
            vec!["整数".to_string()], // 流句柄
            "字符串",
        ));

        // 关闭流
        llm_module.add_function(ModuleFunction::new(
            "关闭流",
            "qi_llm_stream_close",
            vec!["整数".to_string()], // 流句柄
            "整数",
        ));

        // 流式工具对话（流 + tool_calls）
        llm_module.add_function(ModuleFunction::new(
            "流式工具对话",
            "qi_llm_stream_chat_with_tools",
            vec!["整数".to_string(), "字符串".to_string()], // 会话句柄, 提示
            "整数",                                         // 返回流句柄
        ));

        // 流式继续工具对话（回写工具结果后流式续传）
        llm_module.add_function(ModuleFunction::new(
            "流式继续工具对话",
            "qi_llm_stream_continue_with_tools",
            vec!["整数".to_string()], // 会话句柄
            "整数",                   // 返回流句柄
        ));

        // 取流式 assistant 消息（含 content + tool_calls 的 JSON）
        llm_module.add_function(ModuleFunction::new(
            "流取助手消息",
            "qi_llm_stream_assistant_message",
            vec!["整数".to_string()], // 流句柄
            "字符串",
        ));

        // 注册工具
        llm_module.add_function(ModuleFunction::new(
            "注册工具",
            "qi_llm_register_tool",
            vec![
                "整数".to_string(),
                "字符串".to_string(),
                "字符串".to_string(),
                "字符串".to_string(),
            ],
            "整数",
        ));

        // 清空工具
        llm_module.add_function(ModuleFunction::new(
            "清空工具",
            "qi_llm_clear_tools",
            vec!["整数".to_string()],
            "整数",
        ));

        // 工具对话
        llm_module.add_function(ModuleFunction::new(
            "工具对话",
            "qi_llm_chat_with_tools",
            vec!["整数".to_string(), "字符串".to_string()],
            "字符串",
        ));

        // 继续工具对话
        llm_module.add_function(ModuleFunction::new(
            "继续工具对话",
            "qi_llm_continue_with_tools",
            vec!["整数".to_string()],
            "字符串",
        ));

        // 是否有工具调用
        llm_module.add_function(ModuleFunction::new(
            "有工具调用",
            "qi_llm_has_tool_call",
            vec!["字符串".to_string()],
            "整数",
        ));

        // 获取工具调用 ID
        llm_module.add_function(ModuleFunction::new(
            "工具调用ID",
            "qi_llm_get_tool_call_id",
            vec!["字符串".to_string()],
            "字符串",
        ));

        // 获取工具调用名称
        llm_module.add_function(ModuleFunction::new(
            "工具调用名称",
            "qi_llm_get_tool_call_name",
            vec!["整数".to_string(), "字符串".to_string()],
            "字符串",
        ));

        // 获取工具调用参数
        llm_module.add_function(ModuleFunction::new(
            "工具调用参数",
            "qi_llm_get_tool_call_arguments",
            vec!["字符串".to_string()],
            "字符串",
        ));

        // ===== Parallel tool_calls 支持 — 按 index 访问 =====
        llm_module.add_function(ModuleFunction::new(
            "工具调用数量",
            "qi_llm_get_tool_call_count",
            vec!["字符串".to_string()],
            "整数",
        ));
        llm_module.add_function(ModuleFunction::new(
            "工具调用ID索引",
            "qi_llm_get_tool_call_id_at",
            vec!["字符串".to_string(), "整数".to_string()],
            "字符串",
        ));
        llm_module.add_function(ModuleFunction::new(
            "工具调用名称索引",
            "qi_llm_get_tool_call_name_at",
            vec!["整数".to_string(), "字符串".to_string(), "整数".to_string()],
            "字符串",
        ));
        llm_module.add_function(ModuleFunction::new(
            "工具调用参数索引",
            "qi_llm_get_tool_call_arguments_at",
            vec!["字符串".to_string(), "整数".to_string()],
            "字符串",
        ));

        // 添加工具结果
        llm_module.add_function(ModuleFunction::new(
            "添加工具结果",
            "qi_llm_add_tool_result",
            vec![
                "整数".to_string(),
                "字符串".to_string(),
                "字符串".to_string(),
                "字符串".to_string(),
            ],
            "整数",
        ));

        // Register module with both Chinese and path formats
        self.modules
            .insert("大模型".to_string(), llm_module.clone());
        self.modules
            .insert("标准库.大模型".to_string(), llm_module.clone());
        self.modules.insert("LLM".to_string(), llm_module);

        // ===== 操作系统模块 (OS Module) =====
        let mut os_module = Module::new("操作系统");

        // 环境变量操作
        os_module.add_function(ModuleFunction::new(
            "获取环境变量",
            "qi_os_getenv",
            vec!["字符串".to_string()], // 变量名
            "字符串",                   // 返回变量值
        ));

        os_module.add_function(ModuleFunction::new(
            "设置环境变量",
            "qi_os_setenv",
            vec!["字符串".to_string(), "字符串".to_string()], // 变量名, 变量值
            "整数",                                           // 返回状态码
        ));

        os_module.add_function(ModuleFunction::new(
            "删除环境变量",
            "qi_os_unsetenv",
            vec!["字符串".to_string()], // 变量名
            "整数",                     // 返回状态码
        ));

        os_module.add_function(ModuleFunction::new(
            "所有环境变量",
            "qi_os_environ",
            vec![],   // 无参数
            "字符串", // 返回所有环境变量
        ));

        // 目录操作
        os_module.add_function(ModuleFunction::new(
            "当前目录",
            "qi_os_getcwd",
            vec![],   // 无参数
            "字符串", // 返回当前目录路径
        ));

        os_module.add_function(ModuleFunction::new(
            "切换目录",
            "qi_os_chdir",
            vec!["字符串".to_string()], // 目标路径
            "整数",                     // 返回状态码
        ));

        os_module.add_function(ModuleFunction::new(
            "用户主目录",
            "qi_os_homedir",
            vec![],   // 无参数
            "字符串", // 返回主目录路径
        ));

        os_module.add_function(ModuleFunction::new(
            "临时目录",
            "qi_os_tempdir",
            vec![],   // 无参数
            "字符串", // 返回临时目录路径
        ));

        // 系统信息
        os_module.add_function(ModuleFunction::new(
            "操作系统类型",
            "qi_os_type",
            vec![],   // 无参数
            "字符串", // 返回 windows/linux/macos
        ));

        os_module.add_function(ModuleFunction::new(
            "系统架构",
            "qi_os_arch",
            vec![],   // 无参数
            "字符串", // 返回 x86_64/aarch64
        ));

        os_module.add_function(ModuleFunction::new(
            "系统家族",
            "qi_os_family",
            vec![],   // 无参数
            "字符串", // 返回 unix/windows
        ));

        os_module.add_function(ModuleFunction::new(
            "主机名",
            "qi_os_hostname",
            vec![],   // 无参数
            "字符串", // 返回主机名
        ));

        os_module.add_function(ModuleFunction::new(
            "用户名",
            "qi_os_username",
            vec![],   // 无参数
            "字符串", // 返回用户名
        ));

        // CPU信息
        os_module.add_function(ModuleFunction::new(
            "CPU核心数",
            "qi_os_cpu_count",
            vec![], // 无参数
            "整数", // 返回CPU核心数
        ));

        // 进程信息
        os_module.add_function(ModuleFunction::new(
            "进程ID",
            "qi_os_getpid",
            vec![], // 无参数
            "整数", // 返回进程ID
        ));

        os_module.add_function(ModuleFunction::new(
            "退出程序",
            "qi_os_exit",
            vec!["整数".to_string()], // 退出码
            "void",                   // 无返回值
        ));

        // 环境变量文件加载
        os_module.add_function(ModuleFunction::new(
            "加载环境文件",
            "qi_os_load_env",
            vec!["字符串".to_string()], // .env 文件路径
            "整数",                     // 返回加载的环境变量数量
        ));

        // 目录操作
        os_module.add_function(ModuleFunction::new(
            "列出目录",
            "qi_os_list_dir",
            vec!["字符串".to_string()], // 目录路径
            "字符串",                   // 返回目录内容列表
        ));

        os_module.add_function(ModuleFunction::new(
            "是否为目录",
            "qi_os_is_dir",
            vec!["字符串".to_string()], // 路径
            "整数",                     // 返回1或0
        ));

        os_module.add_function(ModuleFunction::new(
            "是否为文件",
            "qi_os_is_file",
            vec!["字符串".to_string()], // 路径
            "整数",                     // 返回1或0
        ));

        // 内存释放
        os_module.add_function(ModuleFunction::new(
            "释放字符串",
            "qi_os_free_string",
            vec!["字符串".to_string()], // 字符串指针
            "void",                     // 无返回值
        ));

        // Register module with various names
        self.modules
            .insert("操作系统".to_string(), os_module.clone());
        self.modules
            .insert("标准库.操作系统".to_string(), os_module.clone());
        self.modules.insert("OS".to_string(), os_module);

        // ===== 命令行模块 =====
        self.register_cli_module();
    }

    /// 注册命令行参数解析模块
    fn register_cli_module(&mut self) {
        let mut cli_module = Module::new("命令行");

        // 应用创建与配置
        cli_module.add_function(ModuleFunction::new(
            "创建应用",
            "qi_cli_create_app",
            vec!["字符串".to_string()], // 应用名称
            "整数",                     // 返回应用ID
        ));

        cli_module.add_function(ModuleFunction::new(
            "设置版本",
            "qi_cli_set_version",
            vec!["整数".to_string(), "字符串".to_string()], // 应用ID, 版本号
            "整数",                                         // 成功返回1
        ));

        cli_module.add_function(ModuleFunction::new(
            "设置作者",
            "qi_cli_set_author",
            vec!["整数".to_string(), "字符串".to_string()], // 应用ID, 作者
            "整数",
        ));

        cli_module.add_function(ModuleFunction::new(
            "设置关于",
            "qi_cli_set_about",
            vec!["整数".to_string(), "字符串".to_string()], // 应用ID, 描述
            "整数",
        ));
        cli_module.add_function(ModuleFunction::new(
            "设置详细",
            "qi_cli_set_long_about",
            vec!["整数".to_string(), "字符串".to_string()],
            "整数",
        ));
        cli_module.add_function(ModuleFunction::new(
            "设置用法",
            "qi_cli_set_override_usage",
            vec!["整数".to_string(), "字符串".to_string()],
            "整数",
        ));
        cli_module.add_function(ModuleFunction::new(
            "设置尾部帮助",
            "qi_cli_set_after_help",
            vec!["整数".to_string(), "字符串".to_string()],
            "整数",
        ));

        // 参数创建与配置
        cli_module.add_function(ModuleFunction::new(
            "创建参数",
            "qi_cli_create_arg",
            vec!["字符串".to_string()], // 参数名称
            "整数",                     // 返回参数ID
        ));

        cli_module.add_function(ModuleFunction::new(
            "参数设置短名",
            "qi_cli_arg_set_short",
            vec!["整数".to_string(), "字符串".to_string()], // 参数ID, 短名
            "整数",
        ));

        cli_module.add_function(ModuleFunction::new(
            "参数设置长名",
            "qi_cli_arg_set_long",
            vec!["整数".to_string(), "字符串".to_string()], // 参数ID, 长名
            "整数",
        ));

        cli_module.add_function(ModuleFunction::new(
            "参数设置帮助",
            "qi_cli_arg_set_help",
            vec!["整数".to_string(), "字符串".to_string()], // 参数ID, 帮助文本
            "整数",
        ));

        cli_module.add_function(ModuleFunction::new(
            "参数设置必需",
            "qi_cli_arg_set_required",
            vec!["整数".to_string(), "整数".to_string()], // 参数ID, 是否必需(布尔)
            "整数",
        ));

        cli_module.add_function(ModuleFunction::new(
            "参数设置默认值",
            "qi_cli_arg_set_default",
            vec!["整数".to_string(), "字符串".to_string()], // 参数ID, 默认值
            "整数",
        ));

        cli_module.add_function(ModuleFunction::new(
            "参数设置为标志",
            "qi_cli_arg_set_flag",
            vec!["整数".to_string()], // 参数ID
            "整数",
        ));

        cli_module.add_function(ModuleFunction::new(
            "参数设置多值",
            "qi_cli_arg_set_multiple",
            vec!["整数".to_string()], // 参数ID
            "整数",
        ));

        cli_module.add_function(ModuleFunction::new(
            "参数设置环境变量",
            "qi_cli_arg_set_env",
            vec!["整数".to_string(), "字符串".to_string()], // 参数ID, 环境变量名
            "整数",
        ));

        cli_module.add_function(ModuleFunction::new(
            "参数设置全局",
            "qi_cli_arg_set_global",
            vec!["整数".to_string()], // 参数ID
            "整数",
        ));

        // 应用参数添加
        cli_module.add_function(ModuleFunction::new(
            "应用添加参数",
            "qi_cli_app_add_arg",
            vec!["整数".to_string(), "整数".to_string()], // 应用ID, 参数ID
            "整数",
        ));

        // 子命令支持
        cli_module.add_function(ModuleFunction::new(
            "创建子命令",
            "qi_cli_create_subcommand",
            vec!["字符串".to_string()], // 子命令名称
            "整数",                     // 返回子命令ID
        ));

        cli_module.add_function(ModuleFunction::new(
            "应用添加子命令",
            "qi_cli_app_add_subcommand",
            vec!["整数".to_string(), "整数".to_string()], // 应用ID, 子命令ID
            "整数",
        ));
        cli_module.add_function(ModuleFunction::new(
            "添加别名",
            "qi_cli_app_add_alias",
            vec!["整数".to_string(), "字符串".to_string()],
            "整数",
        ));
        cli_module.add_function(ModuleFunction::new(
            "显示帮助",
            "qi_cli_print_help",
            vec!["整数".to_string()],
            "整数",
        ));

        // 参数解析
        cli_module.add_function(ModuleFunction::new(
            "解析",
            "qi_cli_parse",
            vec!["整数".to_string()], // 应用ID
            "整数",                   // 返回匹配结果ID
        ));

        // 结果获取
        cli_module.add_function(ModuleFunction::new(
            "获取值",
            "qi_cli_get_value",
            vec!["整数".to_string(), "字符串".to_string()], // 匹配结果ID, 参数名
            "字符串",                                       // 返回值
        ));

        cli_module.add_function(ModuleFunction::new(
            "获取标志",
            "qi_cli_get_flag",
            vec!["整数".to_string(), "字符串".to_string()], // 匹配结果ID, 参数名
            "整数",                                         // 返回布尔值(0/1)
        ));

        cli_module.add_function(ModuleFunction::new(
            "有值",
            "qi_cli_has_value",
            vec!["整数".to_string(), "字符串".to_string()], // 匹配结果ID, 参数名
            "整数",                                         // 返回布尔值(0/1)
        ));

        cli_module.add_function(ModuleFunction::new(
            "包含子命令",
            "qi_cli_has_subcommand",
            vec!["整数".to_string(), "字符串".to_string()], // 匹配结果ID, 子命令名
            "整数",                                         // 返回布尔值(0/1)
        ));

        cli_module.add_function(ModuleFunction::new(
            "获取子命令",
            "qi_cli_get_subcommand",
            vec!["整数".to_string(), "字符串".to_string()], // 匹配结果ID, 子命令名
            "整数",                                         // 返回子命令匹配结果ID
        ));

        // 内存管理
        cli_module.add_function(ModuleFunction::new(
            "释放字符串",
            "qi_cli_free_string",
            vec!["字符串".to_string()], // 字符串指针
            "void",
        ));

        cli_module.add_function(ModuleFunction::new(
            "释放应用",
            "qi_cli_free_app",
            vec!["整数".to_string()], // 应用ID
            "整数",
        ));

        cli_module.add_function(ModuleFunction::new(
            "释放参数",
            "qi_cli_free_arg",
            vec!["整数".to_string()], // 参数ID
            "整数",
        ));

        cli_module.add_function(ModuleFunction::new(
            "释放匹配结果",
            "qi_cli_free_matches",
            vec!["整数".to_string()], // 匹配结果ID
            "整数",
        ));

        // Register module with various names
        self.modules
            .insert("命令行".to_string(), cli_module.clone());
        self.modules
            .insert("标准库.命令行".to_string(), cli_module.clone());
        self.modules.insert("CLI".to_string(), cli_module);

        // ===== 图形化模块 (GUI Module) =====
        self.register_gui_module();
    }

    /// 注册图形化窗口模块
    fn register_gui_module(&mut self) {
        let mut gui_module = Module::new("图形化");

        // 注：老 tao 自绘轨（创建窗口/窗口控制/事件回调/定时器/帧率/运行 事件循环）
        // 已随 tao 依赖一并移除。GUI 现为单轨 egui：用 应用创建 + 帧开始/帧结束 主循环，
        // 逐帧驱动控件与画布（见下方 egui 控件层与画布层）。

        // 获取版本
        gui_module.add_function(ModuleFunction::new(
            "版本",
            "qi_gui_version",
            vec![],
            "字符串",
        ));

        // 释放字符串
        gui_module.add_function(ModuleFunction::new(
            "释放字符串",
            "qi_gui_free_string",
            vec!["字符串".to_string()],
            "void",
        ));

        // 音频功能
        // 加载音频文件
        gui_module.add_function(ModuleFunction::new(
            "加载音频",
            "qi_gui_audio_load",
            vec!["字符串".to_string()],
            "整数",
        ));

        // 播放音频
        gui_module.add_function(ModuleFunction::new(
            "播放音频",
            "qi_gui_audio_play",
            vec!["整数".to_string()],
            "void",
        ));

        // 暂停音频
        gui_module.add_function(ModuleFunction::new(
            "暂停音频",
            "qi_gui_audio_pause",
            vec!["整数".to_string()],
            "void",
        ));

        // 停止音频
        gui_module.add_function(ModuleFunction::new(
            "停止音频",
            "qi_gui_audio_stop",
            vec!["整数".to_string()],
            "void",
        ));

        // 设置音量
        gui_module.add_function(ModuleFunction::new(
            "设置音量",
            "qi_gui_audio_set_volume",
            vec!["整数".to_string(), "浮点数".to_string()],
            "void",
        ));

        // 音频是否正在播放
        gui_module.add_function(ModuleFunction::new(
            "音频是否播放",
            "qi_gui_audio_is_playing",
            vec!["整数".to_string()],
            "整数",
        ));

        // 音频是否播放完成
        gui_module.add_function(ModuleFunction::new(
            "音频是否完成",
            "qi_gui_audio_is_finished",
            vec!["整数".to_string()],
            "整数",
        ));

        // 释放音频播放器
        gui_module.add_function(ModuleFunction::new(
            "释放音频",
            "qi_gui_audio_free",
            vec!["整数".to_string()],
            "void",
        ));

        // ===== egui 控件层（immediate mode GUI · 单轨架构）=====
        let s = || "字符串".to_string();
        let i = || "整数".to_string();

        // 应用创建(标题,宽,高) → 句柄
        gui_module.add_function(ModuleFunction::new(
            "应用创建",
            "qi_gui_egui_app_create",
            vec![s(), i(), i()],
            "整数",
        ));
        // 帧开始(句柄) → 整数（1=存活，0=已关闭）
        gui_module.add_function(ModuleFunction::new(
            "帧开始",
            "qi_gui_egui_frame_begin",
            vec![i()],
            "整数",
        ));
        // 帧结束(句柄)
        gui_module.add_function(ModuleFunction::new(
            "帧结束",
            "qi_gui_egui_frame_end",
            vec![i()],
            "void",
        ));
        // 关闭应用(句柄)
        gui_module.add_function(ModuleFunction::new(
            "关闭应用",
            "qi_gui_egui_app_close",
            vec![i()],
            "void",
        ));
        // 标签(文本)
        gui_module.add_function(ModuleFunction::new(
            "标签",
            "qi_gui_egui_label",
            vec![s()],
            "void",
        ));
        // 标题文本(文本)：大号标题
        gui_module.add_function(ModuleFunction::new(
            "标题文本",
            "qi_gui_egui_heading",
            vec![s()],
            "void",
        ));
        // 彩色标签(文本,r,g,b)
        gui_module.add_function(ModuleFunction::new(
            "彩色标签",
            "qi_gui_egui_colored_label",
            vec![s(), i(), i(), i()],
            "void",
        ));
        // 按钮(文本) → 整数（本帧点击 1/0）
        gui_module.add_function(ModuleFunction::new(
            "按钮",
            "qi_gui_egui_button",
            vec![s()],
            "整数",
        ));
        // 输入框(id, 当前值) → 新值字符串
        gui_module.add_function(ModuleFunction::new(
            "输入框",
            "qi_gui_egui_text_edit",
            vec![s(), s()],
            "字符串",
        ));
        // 多行输入(id, 当前值) → 新值字符串
        gui_module.add_function(ModuleFunction::new(
            "多行输入",
            "qi_gui_egui_text_edit_multiline",
            vec![s(), s()],
            "字符串",
        ));
        // 滑条(id, 当前, 最小, 最大) → 新值整数
        gui_module.add_function(ModuleFunction::new(
            "滑条",
            "qi_gui_egui_slider",
            vec![s(), i(), i(), i()],
            "整数",
        ));
        // 复选框(id, 文本, 当前) → 新值整数（1/0）
        gui_module.add_function(ModuleFunction::new(
            "复选框",
            "qi_gui_egui_checkbox",
            vec![s(), s(), i()],
            "整数",
        ));
        // 下拉选择(id, 选项CSV, 当前序号) → 新序号
        gui_module.add_function(ModuleFunction::new(
            "下拉选择",
            "qi_gui_egui_combo",
            vec![s(), s(), i()],
            "整数",
        ));
        // 分隔线()
        gui_module.add_function(ModuleFunction::new(
            "分隔线",
            "qi_gui_egui_separator",
            vec![],
            "void",
        ));
        // 空行()
        gui_module.add_function(ModuleFunction::new(
            "空行",
            "qi_gui_egui_space",
            vec![],
            "void",
        ));
        // 水平开始()
        gui_module.add_function(ModuleFunction::new(
            "水平开始",
            "qi_gui_egui_horizontal_begin",
            vec![],
            "void",
        ));
        // 水平结束()
        gui_module.add_function(ModuleFunction::new(
            "水平结束",
            "qi_gui_egui_horizontal_end",
            vec![],
            "void",
        ));
        // 分组开始(标题)
        gui_module.add_function(ModuleFunction::new(
            "分组开始",
            "qi_gui_egui_group_begin",
            vec![s()],
            "void",
        ));
        // 分组结束()
        gui_module.add_function(ModuleFunction::new(
            "分组结束",
            "qi_gui_egui_group_end",
            vec![],
            "void",
        ));
        // 进度条(0-100)
        gui_module.add_function(ModuleFunction::new(
            "进度条",
            "qi_gui_egui_progress",
            vec![i()],
            "void",
        ));
        // 折线图(id, 值CSV, 宽, 高)
        gui_module.add_function(ModuleFunction::new(
            "折线图",
            "qi_gui_egui_plot",
            vec![s(), s(), i(), i()],
            "void",
        ));
        // 消息弹窗(文本)
        gui_module.add_function(ModuleFunction::new(
            "消息弹窗",
            "qi_gui_egui_message",
            vec![s()],
            "void",
        ));

        // ── egui 第二批：容器 / 数据展示 / 外观（2026-07-17）──
        let f = || "浮点数".to_string();
        // 滚动开始(id, 高度pt) / 滚动结束()：固定高度垂直滚动视口
        gui_module.add_function(ModuleFunction::new(
            "滚动开始",
            "qi_gui_egui_scroll_begin",
            vec![s(), i()],
            "void",
        ));
        gui_module.add_function(ModuleFunction::new(
            "滚动结束",
            "qi_gui_egui_scroll_end",
            vec![],
            "void",
        ));
        // 折叠开始(标题) → 1 展开 / 0 收起；与 折叠结束() 配对
        gui_module.add_function(ModuleFunction::new(
            "折叠开始",
            "qi_gui_egui_collapse_begin",
            vec![s()],
            "整数",
        ));
        gui_module.add_function(ModuleFunction::new(
            "折叠结束",
            "qi_gui_egui_collapse_end",
            vec![],
            "void",
        ));
        // 单选(文本, 是否选中) → 1 被点击
        gui_module.add_function(ModuleFunction::new(
            "单选",
            "qi_gui_egui_radio",
            vec![s(), i()],
            "整数",
        ));
        // 选择项(文本, 是否选中) → 1 被点击（整行高亮的列表项）
        gui_module.add_function(ModuleFunction::new(
            "选择项",
            "qi_gui_egui_selectable",
            vec![s(), i()],
            "整数",
        ));
        // 数字输入(id, 当前值) → 新值（可拖拽/双击编辑）
        gui_module.add_function(ModuleFunction::new(
            "数字输入",
            "qi_gui_egui_drag_value",
            vec![s(), i()],
            "整数",
        ));
        // 浮点滑条(id, 当前, 最小, 最大) → 新值
        gui_module.add_function(ModuleFunction::new(
            "浮点滑条",
            "qi_gui_egui_slider_f64",
            vec![s(), f(), f(), f()],
            "浮点数",
        ));
        // 超链接(文本, 网址)：点击用系统浏览器打开
        gui_module.add_function(ModuleFunction::new(
            "超链接",
            "qi_gui_egui_hyperlink",
            vec![s(), s()],
            "void",
        ));
        // 悬浮标签(文本, 提示)：悬停出气泡
        gui_module.add_function(ModuleFunction::new(
            "悬浮标签",
            "qi_gui_egui_label_tip",
            vec![s(), s()],
            "void",
        ));
        // 表格(id, 表头CSV, 行数据)：行以换行分隔、列以逗号分隔，斑马纹
        gui_module.add_function(ModuleFunction::new(
            "表格",
            "qi_gui_egui_table",
            vec![s(), s(), s()],
            "void",
        ));
        // 柱状图(id, 值CSV, 宽, 高)
        gui_module.add_function(ModuleFunction::new(
            "柱状图",
            "qi_gui_egui_bar_chart",
            vec![s(), s(), i(), i()],
            "void",
        ));
        // 图片显示(路径, 宽, 高)：0=原尺寸/按比例；png/jpg，首载缓存
        gui_module.add_function(ModuleFunction::new(
            "图片显示",
            "qi_gui_egui_image",
            vec![s(), i(), i()],
            "void",
        ));
        // 设置主题(深色)：1 深 / 0 浅
        gui_module.add_function(ModuleFunction::new(
            "设置主题",
            "qi_gui_egui_set_theme",
            vec![i()],
            "void",
        ));
        // 界面缩放(百分比)：50..300
        gui_module.add_function(ModuleFunction::new(
            "界面缩放",
            "qi_gui_egui_set_zoom",
            vec![i()],
            "void",
        ));
        // 设置窗口标题(应用, 标题)
        gui_module.add_function(ModuleFunction::new(
            "设置窗口标题",
            "qi_gui_egui_set_window_title",
            vec![i(), s()],
            "void",
        ));

        // ── egui 画布层（承接老 tao 图元能力，帧循环内自绘）（2026-07-18）──
        // 画布开始(id, 宽, 高) / 画布结束()：在当前 Ui 占一块定尺寸自绘区
        gui_module.add_function(ModuleFunction::new(
            "画布开始",
            "qi_gui_egui_canvas_begin",
            vec![s(), i(), i()],
            "void",
        ));
        gui_module.add_function(ModuleFunction::new(
            "画布结束",
            "qi_gui_egui_canvas_end",
            vec![],
            "void",
        ));
        // 画布矩形(x, y, 宽, 高, r, g, b)：局部坐标填充矩形
        gui_module.add_function(ModuleFunction::new(
            "画布矩形",
            "qi_gui_egui_canvas_rect",
            vec![i(), i(), i(), i(), i(), i(), i()],
            "void",
        ));
        // 画布圆(x, y, 半径, r, g, b)：(x,y) 为圆心局部坐标
        gui_module.add_function(ModuleFunction::new(
            "画布圆",
            "qi_gui_egui_canvas_circle",
            vec![i(), i(), i(), i(), i(), i()],
            "void",
        ));
        // 画布线(x1, y1, x2, y2, 粗, r, g, b)
        gui_module.add_function(ModuleFunction::new(
            "画布线",
            "qi_gui_egui_canvas_line",
            vec![i(), i(), i(), i(), i(), i(), i(), i()],
            "void",
        ));
        // 画布文本(x, y, 文本, 字号, r, g, b)：左上对齐
        gui_module.add_function(ModuleFunction::new(
            "画布文本",
            "qi_gui_egui_canvas_text",
            vec![i(), i(), s(), i(), i(), i(), i()],
            "void",
        ));
        // 画布点击() → 整数（本帧画布被点击 1/0）
        gui_module.add_function(ModuleFunction::new(
            "画布点击",
            "qi_gui_egui_canvas_clicked",
            vec![],
            "整数",
        ));
        // 画布鼠标X() → 整数（局部 X，无悬停 -1）
        gui_module.add_function(ModuleFunction::new(
            "画布鼠标X",
            "qi_gui_egui_canvas_mouse_x",
            vec![],
            "整数",
        ));
        // 画布鼠标Y() → 整数（局部 Y，无悬停 -1）
        gui_module.add_function(ModuleFunction::new(
            "画布鼠标Y",
            "qi_gui_egui_canvas_mouse_y",
            vec![],
            "整数",
        ));

        // Register module with various names
        self.modules
            .insert("图形化".to_string(), gui_module.clone());
        self.modules
            .insert("标准库.图形化".to_string(), gui_module.clone());
        self.modules.insert("GUI".to_string(), gui_module);
    }

    /// 注册列表模块
    fn register_list_module(&mut self) {
        let mut list_module = Module::new("列表");

        // 整数列表
        list_module.add_function(ModuleFunction::new(
            "创建整数列表",
            "qi_list_int_create",
            vec![],
            "整数",
        ));
        list_module.add_function(ModuleFunction::new(
            "添加整数",
            "qi_list_int_push",
            vec!["整数".to_string(), "整数".to_string()],
            "整数",
        ));
        list_module.add_function(ModuleFunction::new(
            "获取整数",
            "qi_list_int_get",
            vec!["整数".to_string(), "整数".to_string()],
            "整数",
        ));
        list_module.add_function(ModuleFunction::new(
            "设置整数",
            "qi_list_int_set",
            vec!["整数".to_string(), "整数".to_string(), "整数".to_string()],
            "整数",
        ));
        list_module.add_function(ModuleFunction::new(
            "整数列表大小",
            "qi_list_int_size",
            vec!["整数".to_string()],
            "整数",
        ));
        list_module.add_function(ModuleFunction::new(
            "弹出整数",
            "qi_list_int_pop",
            vec!["整数".to_string()],
            "整数",
        ));
        list_module.add_function(ModuleFunction::new(
            "清空整数列表",
            "qi_list_int_clear",
            vec!["整数".to_string()],
            "整数",
        ));
        list_module.add_function(ModuleFunction::new(
            "删除整数元素",
            "qi_list_int_remove",
            vec!["整数".to_string(), "整数".to_string()],
            "整数",
        ));
        list_module.add_function(ModuleFunction::new(
            "插入整数",
            "qi_list_int_insert",
            vec!["整数".to_string(), "整数".to_string(), "整数".to_string()],
            "整数",
        ));
        list_module.add_function(ModuleFunction::new(
            "包含整数",
            "qi_list_int_contains",
            vec!["整数".to_string(), "整数".to_string()],
            "整数",
        ));
        list_module.add_function(ModuleFunction::new(
            "查找整数索引",
            "qi_list_int_index_of",
            vec!["整数".to_string(), "整数".to_string()],
            "整数",
        ));

        // 浮点数列表
        list_module.add_function(ModuleFunction::new(
            "创建浮点列表",
            "qi_list_float_create",
            vec![],
            "整数",
        ));
        list_module.add_function(ModuleFunction::new(
            "添加浮点数",
            "qi_list_float_push",
            vec!["整数".to_string(), "浮点数".to_string()],
            "整数",
        ));
        list_module.add_function(ModuleFunction::new(
            "获取浮点数",
            "qi_list_float_get",
            vec!["整数".to_string(), "整数".to_string()],
            "浮点数",
        ));
        list_module.add_function(ModuleFunction::new(
            "浮点列表大小",
            "qi_list_float_size",
            vec!["整数".to_string()],
            "整数",
        ));

        // 字符串列表
        list_module.add_function(ModuleFunction::new(
            "创建字符串列表",
            "qi_list_string_create",
            vec![],
            "整数",
        ));
        list_module.add_function(ModuleFunction::new(
            "添加字符串",
            "qi_list_string_push",
            vec!["整数".to_string(), "字符串".to_string()],
            "整数",
        ));
        list_module.add_function(ModuleFunction::new(
            "获取字符串",
            "qi_list_string_get",
            vec!["整数".to_string(), "整数".to_string()],
            "字符串",
        ));
        list_module.add_function(ModuleFunction::new(
            "字符串列表大小",
            "qi_list_string_size",
            vec!["整数".to_string()],
            "整数",
        ));

        // 指针列表
        list_module.add_function(ModuleFunction::new(
            "创建指针列表",
            "qi_list_ptr_create",
            vec![],
            "整数",
        ));
        list_module.add_function(ModuleFunction::new(
            "添加指针",
            "qi_list_ptr_push",
            vec!["整数".to_string(), "指针".to_string()],
            "整数",
        ));
        list_module.add_function(ModuleFunction::new(
            "获取指针",
            "qi_list_ptr_get",
            vec!["整数".to_string(), "整数".to_string()],
            "指针",
        ));
        list_module.add_function(ModuleFunction::new(
            "设置指针",
            "qi_list_ptr_set",
            vec!["整数".to_string(), "整数".to_string(), "指针".to_string()],
            "整数",
        ));
        list_module.add_function(ModuleFunction::new(
            "指针列表大小",
            "qi_list_ptr_size",
            vec!["整数".to_string()],
            "整数",
        ));

        // 通用操作
        list_module.add_function(ModuleFunction::new(
            "删除列表",
            "qi_list_free",
            vec!["整数".to_string()],
            "整数",
        ));

        self.modules.insert("列表".to_string(), list_module.clone());
        self.modules.insert("标准库.列表".to_string(), list_module);
    }

    /// 注册哈希表模块
    fn register_hashmap_module(&mut self) {
        let mut map_module = Module::new("哈希表");

        // 整数哈希表
        map_module.add_function(ModuleFunction::new(
            "创建整数表",
            "qi_hashmap_int_create",
            vec![],
            "整数",
        ));
        map_module.add_function(ModuleFunction::new(
            "设置整数",
            "qi_hashmap_int_set",
            vec!["整数".to_string(), "字符串".to_string(), "整数".to_string()],
            "整数",
        ));
        map_module.add_function(ModuleFunction::new(
            "获取整数",
            "qi_hashmap_int_get",
            vec!["整数".to_string(), "字符串".to_string()],
            "整数",
        ));
        // 包含键 / 删除键：通用分派（整数/浮点/字符串表统一），此前只认整数表
        map_module.add_function(ModuleFunction::new(
            "包含键",
            "qi_hashmap_contains",
            vec!["整数".to_string(), "字符串".to_string()],
            "整数",
        ));
        map_module.add_function(ModuleFunction::new(
            "删除键",
            "qi_hashmap_remove",
            vec!["整数".to_string(), "字符串".to_string()],
            "整数",
        ));
        map_module.add_function(ModuleFunction::new(
            "表大小",
            "qi_hashmap_int_size",
            vec!["整数".to_string()],
            "整数",
        ));
        map_module.add_function(ModuleFunction::new(
            "清空表",
            "qi_hashmap_int_clear",
            vec!["整数".to_string()],
            "整数",
        ));

        // 浮点数哈希表
        map_module.add_function(ModuleFunction::new(
            "创建浮点表",
            "qi_hashmap_float_create",
            vec![],
            "整数",
        ));
        map_module.add_function(ModuleFunction::new(
            "设置浮点数",
            "qi_hashmap_float_set",
            vec![
                "整数".to_string(),
                "字符串".to_string(),
                "浮点数".to_string(),
            ],
            "整数",
        ));
        map_module.add_function(ModuleFunction::new(
            "获取浮点数",
            "qi_hashmap_float_get",
            vec!["整数".to_string(), "字符串".to_string()],
            "浮点数",
        ));
        map_module.add_function(ModuleFunction::new(
            "浮点表大小",
            "qi_hashmap_float_size",
            vec!["整数".to_string()],
            "整数",
        ));

        // 字符串哈希表
        map_module.add_function(ModuleFunction::new(
            "创建字符串表",
            "qi_hashmap_string_create",
            vec![],
            "整数",
        ));
        map_module.add_function(ModuleFunction::new(
            "设置字符串",
            "qi_hashmap_string_set",
            vec![
                "整数".to_string(),
                "字符串".to_string(),
                "字符串".to_string(),
            ],
            "整数",
        ));
        map_module.add_function(ModuleFunction::new(
            "获取字符串",
            "qi_hashmap_string_get",
            vec!["整数".to_string(), "字符串".to_string()],
            "字符串",
        ));
        map_module.add_function(ModuleFunction::new(
            "字符串表大小",
            "qi_hashmap_string_size",
            vec!["整数".to_string()],
            "整数",
        ));

        // 通用操作
        map_module.add_function(ModuleFunction::new(
            "释放表",
            "qi_hashmap_free",
            vec!["整数".to_string()],
            "整数",
        ));

        self.modules
            .insert("哈希表".to_string(), map_module.clone());
        self.modules.insert("标准库.哈希表".to_string(), map_module);
    }

    /// 注册JSON模块
    fn register_json_module(&mut self) {
        let mut json_module = Module::new("JSON");

        // JSON编码
        json_module.add_function(ModuleFunction::new(
            "编码",
            "qi_json_encode",
            vec!["字符串".to_string()], // 接受任意对象的字符串表示
            "字符串",                   // 返回JSON字符串
        ));

        // JSON解码
        json_module.add_function(ModuleFunction::new(
            "解码",
            "qi_json_decode",
            vec!["字符串".to_string()], // JSON字符串
            "整数",                     // 返回JSON对象句柄
        ));

        // JSON对象操作
        json_module.add_function(ModuleFunction::new(
            "创建对象",
            "qi_json_create_object",
            vec![],
            "整数", // 返回JSON对象句柄
        ));

        json_module.add_function(ModuleFunction::new(
            "创建数组",
            "qi_json_create_array",
            vec![],
            "整数", // 返回JSON数组句柄
        ));

        // 对象字段设置
        json_module.add_function(ModuleFunction::new(
            "设置字符串",
            "qi_json_set_string",
            vec![
                "整数".to_string(),
                "字符串".to_string(),
                "字符串".to_string(),
            ], // 对象句柄, 键, 值
            "整数", // 返回状态
        ));

        json_module.add_function(ModuleFunction::new(
            "设置整数",
            "qi_json_set_int",
            vec!["整数".to_string(), "字符串".to_string(), "整数".to_string()], // 对象句柄, 键, 值
            "整数",                                                             // 返回状态
        ));

        json_module.add_function(ModuleFunction::new(
            "设置浮点数",
            "qi_json_set_float",
            vec![
                "整数".to_string(),
                "字符串".to_string(),
                "浮点数".to_string(),
            ], // 对象句柄, 键, 值
            "整数", // 返回状态
        ));

        json_module.add_function(ModuleFunction::new(
            "设置布尔",
            "qi_json_set_bool",
            vec!["整数".to_string(), "字符串".to_string(), "整数".to_string()], // 对象句柄, 键, 值(0/1)
            "整数",                                                             // 返回状态
        ));

        json_module.add_function(ModuleFunction::new(
            "设置对象",
            "qi_json_set_object",
            vec!["整数".to_string(), "字符串".to_string(), "整数".to_string()], // 对象句柄, 键, 子对象句柄
            "整数",                                                             // 返回状态
        ));

        json_module.add_function(ModuleFunction::new(
            "设置数组",
            "qi_json_set_array",
            vec!["整数".to_string(), "字符串".to_string(), "整数".to_string()], // 对象句柄, 键, 数组句柄
            "整数",                                                             // 返回状态
        ));

        // 对象字段获取
        json_module.add_function(ModuleFunction::new(
            "获取字符串",
            "qi_json_get_string",
            vec!["整数".to_string(), "字符串".to_string()], // 对象句柄, 键
            "字符串",                                       // 返回值
        ));

        json_module.add_function(ModuleFunction::new(
            "获取整数",
            "qi_json_get_int",
            vec!["整数".to_string(), "字符串".to_string()], // 对象句柄, 键
            "整数",                                         // 返回值
        ));

        json_module.add_function(ModuleFunction::new(
            "获取浮点数",
            "qi_json_get_float",
            vec!["整数".to_string(), "字符串".to_string()], // 对象句柄, 键
            "浮点数",                                       // 返回值
        ));

        json_module.add_function(ModuleFunction::new(
            "获取布尔",
            "qi_json_get_bool",
            vec!["整数".to_string(), "字符串".to_string()], // 对象句柄, 键
            "整数",                                         // 返回值(0/1)
        ));

        json_module.add_function(ModuleFunction::new(
            "获取对象",
            "qi_json_get_object",
            vec!["整数".to_string(), "字符串".to_string()], // 对象句柄, 键
            "整数",                                         // 返回子对象句柄
        ));

        json_module.add_function(ModuleFunction::new(
            "获取数组",
            "qi_json_get_array",
            vec!["整数".to_string(), "字符串".to_string()], // 对象句柄, 键
            "整数",                                         // 返回数组句柄
        ));

        // 数组操作
        json_module.add_function(ModuleFunction::new(
            "数组添加字符串",
            "qi_json_array_push_string",
            vec!["整数".to_string(), "字符串".to_string()], // 数组句柄, 值
            "整数",                                         // 返回状态
        ));

        json_module.add_function(ModuleFunction::new(
            "数组添加整数",
            "qi_json_array_push_int",
            vec!["整数".to_string(), "整数".to_string()], // 数组句柄, 值
            "整数",                                       // 返回状态
        ));

        json_module.add_function(ModuleFunction::new(
            "数组添加浮点数",
            "qi_json_array_push_float",
            vec!["整数".to_string(), "浮点数".to_string()], // 数组句柄, 值
            "整数",                                         // 返回状态
        ));

        json_module.add_function(ModuleFunction::new(
            "数组添加布尔",
            "qi_json_array_push_bool",
            vec!["整数".to_string(), "整数".to_string()], // 数组句柄, 值(0/1)
            "整数",                                       // 返回状态
        ));

        json_module.add_function(ModuleFunction::new(
            "数组添加对象",
            "qi_json_array_push_object",
            vec!["整数".to_string(), "整数".to_string()], // 数组句柄, 对象句柄
            "整数",                                       // 返回状态
        ));

        // 数组访问
        json_module.add_function(ModuleFunction::new(
            "数组获取字符串",
            "qi_json_array_get_string",
            vec!["整数".to_string(), "整数".to_string()], // 数组句柄, 索引
            "字符串",                                     // 返回值
        ));

        json_module.add_function(ModuleFunction::new(
            "数组获取整数",
            "qi_json_array_get_int",
            vec!["整数".to_string(), "整数".to_string()], // 数组句柄, 索引
            "整数",                                       // 返回值
        ));

        json_module.add_function(ModuleFunction::new(
            "数组获取浮点数",
            "qi_json_array_get_float",
            vec!["整数".to_string(), "整数".to_string()], // 数组句柄, 索引
            "浮点数",                                     // 返回值
        ));

        json_module.add_function(ModuleFunction::new(
            "数组获取布尔",
            "qi_json_array_get_bool",
            vec!["整数".to_string(), "整数".to_string()], // 数组句柄, 索引
            "整数",                                       // 返回值(0/1)
        ));

        json_module.add_function(ModuleFunction::new(
            "数组获取对象",
            "qi_json_array_get_object",
            vec!["整数".to_string(), "整数".to_string()], // 数组句柄, 索引
            "整数",                                       // 返回对象句柄
        ));

        // 工具函数
        json_module.add_function(ModuleFunction::new(
            "数组长度",
            "qi_json_array_length",
            vec!["整数".to_string()], // 数组句柄
            "整数",                   // 返回长度
        ));

        json_module.add_function(ModuleFunction::new(
            "是否包含键",
            "qi_json_has_key",
            vec!["整数".to_string(), "字符串".to_string()], // 对象句柄, 键
            "整数",                                         // 返回1或0
        ));

        json_module.add_function(ModuleFunction::new(
            "转字符串",
            "qi_json_to_string",
            vec!["整数".to_string()], // 对象或数组句柄
            "字符串",                 // 返回JSON字符串
        ));

        json_module.add_function(ModuleFunction::new(
            "格式化",
            "qi_json_to_string_pretty",
            vec!["整数".to_string()], // 对象或数组句柄
            "字符串",                 // 返回格式化的JSON字符串
        ));

        json_module.add_function(ModuleFunction::new(
            "从键值",
            "qi_json_from_pairs",
            vec!["字符串".to_string()], // 形如 "键=值;键2=值2"
            "字符串",                   // 返回JSON字符串
        ));

        json_module.add_function(ModuleFunction::new(
            "从文本",
            "qi_json_from_text",
            vec!["字符串".to_string()],
            "字符串", // 返回 {"结果":"..."}
        ));

        // 内存管理
        json_module.add_function(ModuleFunction::new(
            "删除",
            "qi_json_free",
            vec!["整数".to_string()], // JSON对象或数组句柄
            "整数",                   // 返回状态
        ));

        // 枚举序号：候选名(逗号分隔) 里找值的下标 —— 询问::<T> 反序列化枚举字段用
        json_module.add_function(ModuleFunction::new(
            "枚举序号",
            "qi_json_enum_tag",
            vec!["字符串".to_string(), "字符串".to_string()],
            "整数",
        ));

        // 对象字段(JSON数组) → Qi 原生数组（询问 数组字段反序列化；rc=1 交出）
        json_module.add_function(ModuleFunction::new(
            "字段字符串数组",
            "qi_json_field_str_array",
            vec!["整数".to_string(), "字符串".to_string()],
            "字符串数组",
        ));
        json_module.add_function(ModuleFunction::new(
            "字段整数数组",
            "qi_json_field_int_array",
            vec!["整数".to_string(), "字符串".to_string()],
            "整数数组",
        ));
        json_module.add_function(ModuleFunction::new(
            "字段浮点数组",
            "qi_json_field_float_array",
            vec!["整数".to_string(), "字符串".to_string()],
            "浮点数组",
        ));
        // n 槽指针数组分配（询问 数组of结构体反序列化用；槽零初始化，rc=1 交出。
        // 返回类型标"字符串数组"=数组(指针)——结构体元素同为指针槽，运行时布局相同）
        json_module.add_function(ModuleFunction::new(
            "分配对象数组",
            "qi_json_alloc_obj_array",
            vec!["整数".to_string()],
            "字符串数组",
        ));

        // Register module with both Chinese and path formats
        self.modules.insert("JSON".to_string(), json_module.clone());
        self.modules.insert("标准库.JSON".to_string(), json_module);
    }

    /// 注册MCP服务器模块
    fn register_mcp_module(&mut self) {
        let mut mcp_module = Module::new("MCP服务器");

        // 服务器管理
        mcp_module.add_function(ModuleFunction::new(
            "创建服务器",
            "qi_mcp_create_server",
            vec![
                "字符串".to_string(),
                "字符串".to_string(),
                "字符串".to_string(),
            ], // 名称, 版本, 描述
            "整数", // 返回服务器ID
        ));

        mcp_module.add_function(ModuleFunction::new(
            "启动服务器",
            "qi_mcp_start_server",
            vec!["整数".to_string()], // 服务器ID
            "i32",                    // 返回状态 (FFI返回i32)
        ));

        mcp_module.add_function(ModuleFunction::new(
            "停止服务器",
            "qi_mcp_stop_server",
            vec!["整数".to_string()], // 服务器ID
            "i32",                    // 返回状态 (FFI返回i32)
        ));

        mcp_module.add_function(ModuleFunction::new(
            "是否运行中",
            "qi_mcp_is_running",
            vec!["整数".to_string()], // 服务器ID
            "i32",                    // 返回1或0 (FFI返回i32)
        ));

        mcp_module.add_function(ModuleFunction::new(
            "销毁服务器",
            "qi_mcp_destroy_server",
            vec!["整数".to_string()], // 服务器ID
            "i32",                    // 返回状态 (FFI返回i32)
        ));

        mcp_module.add_function(ModuleFunction::new(
            "获取服务器信息",
            "qi_mcp_get_server_info",
            vec!["整数".to_string()], // 服务器ID
            "ptr",                    // 返回JSON字符串 (FFI返回*mut c_char)
        ));

        // 工具管理
        mcp_module.add_function(ModuleFunction::new(
            "注册工具",
            "qi_mcp_register_tool",
            vec![
                "整数".to_string(),
                "字符串".to_string(),
                "字符串".to_string(),
            ], // 服务器ID, 工具名, 描述
            "i32", // 返回状态 (FFI返回i32)
        ));

        mcp_module.add_function(ModuleFunction::new(
            "执行工具",
            "qi_mcp_call_tool",
            vec![
                "整数".to_string(),
                "字符串".to_string(),
                "字符串".to_string(),
            ], // 服务器ID, 工具名, 参数JSON
            "ptr", // 返回结果JSON (FFI返回*mut c_char)
        ));

        mcp_module.add_function(ModuleFunction::new(
            "列出工具",
            "qi_mcp_list_tools",
            vec!["整数".to_string()], // 服务器ID
            "ptr",                    // 返回JSON数组 (FFI返回*mut c_char)
        ));

        // 资源管理
        mcp_module.add_function(ModuleFunction::new(
            "注册资源",
            "qi_mcp_register_resource",
            vec![
                "整数".to_string(),
                "字符串".to_string(),
                "字符串".to_string(),
                "字符串".to_string(),
                "整数".to_string(),
            ], // 服务器ID, URI, 名称, 描述, 类型
            "i32", // 返回状态 (FFI返回i32)
        ));

        mcp_module.add_function(ModuleFunction::new(
            "列出资源",
            "qi_mcp_list_resources",
            vec!["整数".to_string()], // 服务器ID
            "ptr",                    // 返回JSON数组 (FFI返回*mut c_char)
        ));

        // 提示管理
        mcp_module.add_function(ModuleFunction::new(
            "注册提示",
            "qi_mcp_register_prompt",
            vec![
                "整数".to_string(),
                "字符串".to_string(),
                "字符串".to_string(),
                "字符串".to_string(),
            ], // 服务器ID, 名称, 描述, 模板
            "i32", // 返回状态 (FFI返回i32)
        ));

        mcp_module.add_function(ModuleFunction::new(
            "获取提示",
            "qi_mcp_get_prompt",
            vec![
                "整数".to_string(),
                "字符串".to_string(),
                "字符串".to_string(),
            ], // 服务器ID, 提示名, 参数JSON
            "ptr", // 返回填充后的文本 (FFI返回*mut c_char)
        ));

        mcp_module.add_function(ModuleFunction::new(
            "列出提示",
            "qi_mcp_list_prompts",
            vec!["整数".to_string()], // 服务器ID
            "ptr",                    // 返回JSON数组 (FFI返回*mut c_char)
        ));

        // 添加工具参数
        mcp_module.add_function(ModuleFunction::new(
            "添加工具参数",
            "qi_mcp_add_tool_parameter",
            vec![
                "整数".to_string(),
                "字符串".to_string(),
                "字符串".to_string(),
                "字符串".to_string(),
                "字符串".to_string(),
                "整数".to_string(),
            ],
            // 服务器ID, 工具名, 参数名, 参数类型, 参数描述, 是否必需
            "i32", // 返回状态 (FFI返回i32)
        ));

        // 设置工具回调
        mcp_module.add_function(ModuleFunction::new(
            "设置工具回调",
            "qi_mcp_set_tool_callback",
            vec![
                "整数".to_string(),
                "字符串".to_string(),
                "字符串".to_string(),
            ], // 服务器ID, 工具名, 回调ID
            "i32", // 返回状态 (FFI返回i32)
        ));

        // 设置工具回调闭包对象指针 (Qi 闭包对象版本)
        mcp_module.add_function(ModuleFunction::new(
            "设置工具回调指针",
            "qi_mcp_set_tool_callback_ptr",
            vec!["整数".to_string(), "字符串".to_string(), "指针".to_string()], // 服务器ID, 工具名, Qi闭包对象指针
            "i32", // 返回状态 (FFI返回i32)
        ));

        // 设置工具原始 inputSchema (完整 JSON Schema 字符串)
        mcp_module.add_function(ModuleFunction::new(
            "设置工具schema",
            "qi_mcp_set_tool_schema",
            vec![
                "整数".to_string(),
                "字符串".to_string(),
                "字符串".to_string(),
            ], // 服务器ID, 工具名, inputSchema JSON
            "i32", // 返回状态 (FFI返回i32)
        ));

        // stdio JSON-RPC 2.0 服务器主循环 (阻塞直到 stdin EOF)
        mcp_module.add_function(ModuleFunction::new(
            "运行stdio",
            "qi_mcp_serve_stdio",
            vec!["整数".to_string()], // 服务器ID
            "i32",                    // 返回状态
        ));

        // Streamable HTTP 传输主循环 (阻塞)
        mcp_module.add_function(ModuleFunction::new(
            "运行HTTP",
            "qi_mcp_serve_http",
            vec!["整数".to_string(), "字符串".to_string(), "整数".to_string()], // 服务器ID, 主机, 端口
            "i32",                                                              // 返回状态
        ));

        // 资源内容管理
        mcp_module.add_function(ModuleFunction::new(
            "设置资源文本内容",
            "qi_mcp_set_resource_text_content",
            vec![
                "整数".to_string(),
                "字符串".to_string(),
                "字符串".to_string(),
            ], // 服务器ID, URI, 内容
            "i32", // 返回状态 (FFI返回i32)
        ));

        mcp_module.add_function(ModuleFunction::new(
            "设置资源JSON内容",
            "qi_mcp_set_resource_json_content",
            vec![
                "整数".to_string(),
                "字符串".to_string(),
                "字符串".to_string(),
            ], // 服务器ID, URI, JSON内容
            "i32", // 返回状态 (FFI返回i32)
        ));

        mcp_module.add_function(ModuleFunction::new(
            "读取资源文本",
            "qi_mcp_read_resource_text",
            vec!["整数".to_string(), "字符串".to_string()], // 服务器ID, URI
            "ptr",                                          // 返回文本内容 (FFI返回*mut c_char)
        ));

        mcp_module.add_function(ModuleFunction::new(
            "读取资源JSON",
            "qi_mcp_read_resource_json",
            vec!["整数".to_string(), "字符串".to_string()], // 服务器ID, URI
            "ptr",                                          // 返回JSON内容 (FFI返回*mut c_char)
        ));

        // 内存管理
        mcp_module.add_function(ModuleFunction::new(
            "释放字符串",
            "qi_mcp_free_string",
            vec!["字符串".to_string()], // 字符串指针
            "void",
        ));

        // P2: 服务器→客户端推送通知
        mcp_module.add_function(ModuleFunction::new(
            "通知工具变更",
            "qi_mcp_notify_tools_changed",
            vec!["整数".to_string()], // 服务器ID
            "i32",                    // 返回状态
        ));

        mcp_module.add_function(ModuleFunction::new(
            "通知资源变更",
            "qi_mcp_notify_resources_changed",
            vec!["整数".to_string()], // 服务器ID
            "i32",                    // 返回状态
        ));

        mcp_module.add_function(ModuleFunction::new(
            "通知提示变更",
            "qi_mcp_notify_prompts_changed",
            vec!["整数".to_string()], // 服务器ID
            "i32",                    // 返回状态
        ));

        mcp_module.add_function(ModuleFunction::new(
            "日志消息",
            "qi_mcp_log_message",
            vec![
                "整数".to_string(),
                "字符串".to_string(),
                "字符串".to_string(),
            ], // 服务器ID, 级别, 消息
            "i32", // 返回状态
        ));

        mcp_module.add_function(ModuleFunction::new(
            "通知进度",
            "qi_mcp_notify_progress",
            vec![
                "整数".to_string(),
                "字符串".to_string(),
                "整数".to_string(),
                "整数".to_string(),
            ], // 服务器ID, token, progress, total
            "i32", // 返回状态
        ));

        // Register module with various names
        self.modules
            .insert("MCP服务器".to_string(), mcp_module.clone());
        self.modules
            .insert("标准库.MCP服务器".to_string(), mcp_module.clone());
        self.modules.insert("MCP".to_string(), mcp_module);
    }

    /// 注册 MCP 客户端核心模块（标准库.MCP客户端）
    fn register_mcp_client_module(&mut self) {
        let mut m = Module::new("MCP客户端");

        // 连接 stdio MCP server（启动子进程）
        m.add_function(ModuleFunction::new(
            "连接stdio",
            "qi_mcpc_connect_stdio",
            vec!["字符串".to_string(), "字符串".to_string()], // cmd, args_json
            "整数",                                           // conn_id (>0=成功)
        ));

        // 连接 HTTP(Streamable) MCP server
        m.add_function(ModuleFunction::new(
            "连接http",
            "qi_mcpc_connect_http",
            vec!["字符串".to_string()], // base_url
            "整数",                     // conn_id (>0=成功)
        ));

        // 发送 MCP 请求，返回 result 字段 JSON 串
        m.add_function(ModuleFunction::new(
            "请求",
            "qi_mcpc_request",
            vec![
                "整数".to_string(),
                "字符串".to_string(),
                "字符串".to_string(),
            ], // conn_id, method, params_json
            "字符串", // result JSON 串
        ));

        // 关闭连接
        m.add_function(ModuleFunction::new(
            "关闭",
            "qi_mcpc_close",
            vec!["整数".to_string()], // conn_id
            "整数",
        ));

        // P4b: server→client 双向（仅 stdio）
        // 注册 sampling/createMessage 处理器（Qi 闭包对象指针）
        m.add_function(ModuleFunction::new(
            "设置采样处理",
            "qi_mcpc_set_sampling_handler",
            vec!["整数".to_string(), "指针".to_string()], // conn_id, Qi 闭包对象指针
            "整数",
        ));

        // 注册 elicitation/create 处理器（Qi 闭包对象指针）
        m.add_function(ModuleFunction::new(
            "设置询问处理",
            "qi_mcpc_set_elicitation_handler",
            vec!["整数".to_string(), "指针".to_string()],
            "整数",
        ));

        // 设置 roots/list 返回的 roots 数组（JSON 串）
        m.add_function(ModuleFunction::new(
            "设置根目录",
            "qi_mcpc_set_roots",
            vec!["整数".to_string(), "字符串".to_string()], // conn_id, roots_json
            "整数",
        ));

        // 排空缓冲的 server→client 通知（JSON 数组串）
        m.add_function(ModuleFunction::new(
            "取通知",
            "qi_mcpc_drain_notifications",
            vec!["整数".to_string()], // conn_id
            "字符串",
        ));

        // 内存管理（Qi 通常不需要手动调用）
        m.add_function(ModuleFunction::new(
            "释放字符串",
            "qi_mcpc_free_string",
            vec!["字符串".to_string()],
            "void",
        ));

        // Register module with canonical and short names
        self.modules.insert("MCP客户端".to_string(), m.clone());
        self.modules.insert("标准库.MCP客户端".to_string(), m);
    }

    /// 注册时间模块
    fn register_datetime_module(&mut self) {
        let mut dt_module = Module::new("时间");

        // 当前时间
        dt_module.add_function(ModuleFunction::new(
            "现在",
            "qi_datetime_now",
            vec![],
            "整数",
        ));
        dt_module.add_function(ModuleFunction::new(
            "现在毫秒",
            "qi_datetime_now_millis",
            vec![],
            "整数",
        ));
        dt_module.add_function(ModuleFunction::new(
            "当前毫秒",
            "qi_datetime_now_millis",
            vec![],
            "整数",
        )); // 别名，用于 Web 框架
        dt_module.add_function(ModuleFunction::new(
            "现在微秒",
            "qi_datetime_now_micros",
            vec![],
            "整数",
        ));
        dt_module.add_function(ModuleFunction::new(
            "现在纳秒",
            "qi_datetime_now_nanos",
            vec![],
            "整数",
        ));
        dt_module.add_function(ModuleFunction::new(
            "本地时间",
            "qi_datetime_now_local",
            vec![],
            "整数",
        ));

        // 格式化
        dt_module.add_function(ModuleFunction::new(
            "格式化",
            "qi_datetime_format",
            vec!["整数".to_string(), "字符串".to_string()],
            "字符串",
        ));
        dt_module.add_function(ModuleFunction::new(
            "格式化本地",
            "qi_datetime_format_local",
            vec!["整数".to_string(), "字符串".to_string()],
            "字符串",
        ));

        // 解析
        dt_module.add_function(ModuleFunction::new(
            "解析",
            "qi_datetime_parse",
            vec!["字符串".to_string(), "字符串".to_string()],
            "整数",
        ));

        // 日期组件
        dt_module.add_function(ModuleFunction::new(
            "年",
            "qi_datetime_year",
            vec!["整数".to_string()],
            "整数",
        ));
        dt_module.add_function(ModuleFunction::new(
            "月",
            "qi_datetime_month",
            vec!["整数".to_string()],
            "整数",
        ));
        dt_module.add_function(ModuleFunction::new(
            "日",
            "qi_datetime_day",
            vec!["整数".to_string()],
            "整数",
        ));
        dt_module.add_function(ModuleFunction::new(
            "时",
            "qi_datetime_hour",
            vec!["整数".to_string()],
            "整数",
        ));
        dt_module.add_function(ModuleFunction::new(
            "分",
            "qi_datetime_minute",
            vec!["整数".to_string()],
            "整数",
        ));
        dt_module.add_function(ModuleFunction::new(
            "秒",
            "qi_datetime_second",
            vec!["整数".to_string()],
            "整数",
        ));
        dt_module.add_function(ModuleFunction::new(
            "星期几",
            "qi_datetime_weekday",
            vec!["整数".to_string()],
            "整数",
        ));
        dt_module.add_function(ModuleFunction::new(
            "季度",
            "qi_datetime_quarter",
            vec!["整数".to_string()],
            "整数",
        ));
        dt_module.add_function(ModuleFunction::new(
            "年的第几天",
            "qi_datetime_day_of_year",
            vec!["整数".to_string()],
            "整数",
        ));
        dt_module.add_function(ModuleFunction::new(
            "年的第几周",
            "qi_datetime_week_of_year",
            vec!["整数".to_string()],
            "整数",
        ));
        dt_module.add_function(ModuleFunction::new(
            "毫秒",
            "qi_datetime_millisecond",
            vec!["整数".to_string()],
            "整数",
        ));

        // 日期计算
        dt_module.add_function(ModuleFunction::new(
            "加秒",
            "qi_datetime_add_seconds",
            vec!["整数".to_string(), "整数".to_string()],
            "整数",
        ));
        dt_module.add_function(ModuleFunction::new(
            "加分钟",
            "qi_datetime_add_minutes",
            vec!["整数".to_string(), "整数".to_string()],
            "整数",
        ));
        dt_module.add_function(ModuleFunction::new(
            "加小时",
            "qi_datetime_add_hours",
            vec!["整数".to_string(), "整数".to_string()],
            "整数",
        ));
        dt_module.add_function(ModuleFunction::new(
            "加天",
            "qi_datetime_add_days",
            vec!["整数".to_string(), "整数".to_string()],
            "整数",
        ));
        dt_module.add_function(ModuleFunction::new(
            "加周",
            "qi_datetime_add_weeks",
            vec!["整数".to_string(), "整数".to_string()],
            "整数",
        ));
        dt_module.add_function(ModuleFunction::new(
            "加月",
            "qi_datetime_add_months",
            vec!["整数".to_string(), "整数".to_string()],
            "整数",
        ));
        dt_module.add_function(ModuleFunction::new(
            "加年",
            "qi_datetime_add_years",
            vec!["整数".to_string(), "整数".to_string()],
            "整数",
        ));
        dt_module.add_function(ModuleFunction::new(
            "相差天数",
            "qi_datetime_diff_days",
            vec!["整数".to_string(), "整数".to_string()],
            "整数",
        ));
        dt_module.add_function(ModuleFunction::new(
            "相差小时",
            "qi_datetime_diff_hours",
            vec!["整数".to_string(), "整数".to_string()],
            "整数",
        ));
        dt_module.add_function(ModuleFunction::new(
            "相差分钟",
            "qi_datetime_diff_minutes",
            vec!["整数".to_string(), "整数".to_string()],
            "整数",
        ));
        dt_module.add_function(ModuleFunction::new(
            "相差秒数",
            "qi_datetime_diff_seconds",
            vec!["整数".to_string(), "整数".to_string()],
            "整数",
        ));

        // 日期创建
        dt_module.add_function(ModuleFunction::new(
            "从年月日",
            "qi_datetime_from_ymd",
            vec!["整数".to_string(), "整数".to_string(), "整数".to_string()],
            "整数",
        ));
        dt_module.add_function(ModuleFunction::new(
            "从年月日时分秒",
            "qi_datetime_from_ymdhms",
            vec![
                "整数".to_string(),
                "整数".to_string(),
                "整数".to_string(),
                "整数".to_string(),
                "整数".to_string(),
                "整数".to_string(),
            ],
            "整数",
        ));

        // 工具函数
        dt_module.add_function(ModuleFunction::new(
            "是闰年",
            "qi_datetime_is_leap_year",
            vec!["整数".to_string()],
            "整数",
        ));
        dt_module.add_function(ModuleFunction::new(
            "月天数",
            "qi_datetime_days_in_month",
            vec!["整数".to_string(), "整数".to_string()],
            "整数",
        ));

        // 时间边界
        dt_module.add_function(ModuleFunction::new(
            "当天开始",
            "qi_datetime_start_of_day",
            vec!["整数".to_string()],
            "整数",
        ));
        dt_module.add_function(ModuleFunction::new(
            "当天结束",
            "qi_datetime_end_of_day",
            vec!["整数".to_string()],
            "整数",
        ));
        dt_module.add_function(ModuleFunction::new(
            "本周开始",
            "qi_datetime_start_of_week",
            vec!["整数".to_string()],
            "整数",
        ));
        dt_module.add_function(ModuleFunction::new(
            "本周结束",
            "qi_datetime_end_of_week",
            vec!["整数".to_string()],
            "整数",
        ));
        dt_module.add_function(ModuleFunction::new(
            "本月开始",
            "qi_datetime_start_of_month",
            vec!["整数".to_string()],
            "整数",
        ));
        dt_module.add_function(ModuleFunction::new(
            "本月结束",
            "qi_datetime_end_of_month",
            vec!["整数".to_string()],
            "整数",
        ));
        dt_module.add_function(ModuleFunction::new(
            "本年开始",
            "qi_datetime_start_of_year",
            vec!["整数".to_string()],
            "整数",
        ));
        dt_module.add_function(ModuleFunction::new(
            "本年结束",
            "qi_datetime_end_of_year",
            vec!["整数".to_string()],
            "整数",
        ));
        dt_module.add_function(ModuleFunction::new(
            "本季度开始",
            "qi_datetime_start_of_quarter",
            vec!["整数".to_string()],
            "整数",
        ));
        dt_module.add_function(ModuleFunction::new(
            "本季度结束",
            "qi_datetime_end_of_quarter",
            vec!["整数".to_string()],
            "整数",
        ));

        // 时间判断
        dt_module.add_function(ModuleFunction::new(
            "在范围内",
            "qi_datetime_is_between",
            vec!["整数".to_string(), "整数".to_string(), "整数".to_string()],
            "整数",
        ));
        dt_module.add_function(ModuleFunction::new(
            "是今天",
            "qi_datetime_is_today",
            vec!["整数".to_string()],
            "整数",
        ));
        dt_module.add_function(ModuleFunction::new(
            "是本周",
            "qi_datetime_is_this_week",
            vec!["整数".to_string()],
            "整数",
        ));
        dt_module.add_function(ModuleFunction::new(
            "是本月",
            "qi_datetime_is_this_month",
            vec!["整数".to_string()],
            "整数",
        ));
        dt_module.add_function(ModuleFunction::new(
            "是本年",
            "qi_datetime_is_this_year",
            vec!["整数".to_string()],
            "整数",
        ));
        dt_module.add_function(ModuleFunction::new(
            "是周末",
            "qi_datetime_is_weekend",
            vec!["整数".to_string()],
            "整数",
        ));
        dt_module.add_function(ModuleFunction::new(
            "是工作日",
            "qi_datetime_is_weekday",
            vec!["整数".to_string()],
            "整数",
        ));

        // 时间转换
        dt_module.add_function(ModuleFunction::new(
            "秒转毫秒",
            "qi_datetime_seconds_to_millis",
            vec!["整数".to_string()],
            "整数",
        ));
        dt_module.add_function(ModuleFunction::new(
            "毫秒转秒",
            "qi_datetime_millis_to_seconds",
            vec!["整数".to_string()],
            "整数",
        ));
        dt_module.add_function(ModuleFunction::new(
            "秒转微秒",
            "qi_datetime_seconds_to_micros",
            vec!["整数".to_string()],
            "整数",
        ));
        dt_module.add_function(ModuleFunction::new(
            "微秒转秒",
            "qi_datetime_micros_to_seconds",
            vec!["整数".to_string()],
            "整数",
        ));

        // 睡眠函数
        dt_module.add_function(ModuleFunction::new(
            "睡眠秒",
            "qi_datetime_sleep_seconds",
            vec!["整数".to_string()],
            "空",
        ));
        dt_module.add_function(ModuleFunction::new(
            "睡眠毫秒",
            "qi_datetime_sleep_millis",
            vec!["整数".to_string()],
            "空",
        ));
        dt_module.add_function(ModuleFunction::new(
            "睡眠微秒",
            "qi_datetime_sleep_micros",
            vec!["整数".to_string()],
            "空",
        ));
        // 异步睡眠（同步 block_in_place 版）— 仍 pin worker
        dt_module.add_function(ModuleFunction::new(
            "异步睡眠毫秒",
            "qi_datetime_async_sleep_millis",
            vec!["整数".to_string()],
            "空",
        ));
        // 异步睡眠返回 Future — 真正的 Future API。 用 等待 时间.异步睡眠未来(N) 调
        dt_module.add_function(ModuleFunction::new(
            "异步睡眠未来",
            "qi_datetime_async_sleep_future",
            vec!["整数".to_string()],
            "未来<空>",
        ));

        self.modules.insert("时间".to_string(), dt_module.clone());
        self.modules
            .insert("标准库.时间".to_string(), dt_module.clone());
        // 添加日期别名
        self.modules.insert("日期".to_string(), dt_module.clone());
        self.modules.insert("标准库.日期".to_string(), dt_module);
    }

    /// Get a module by path
    pub fn get_module(&self, path: &str) -> Option<&Module> {
        self.modules.get(path)
    }

    /// Check if a module exists
    pub fn has_module(&self, path: &str) -> bool {
        self.modules.contains_key(path)
    }

    /// Get a function from a module
    pub fn get_function(&self, module_path: &str, function_name: &str) -> Option<&ModuleFunction> {
        self.get_module(module_path)
            .and_then(|module| module.get_function(function_name))
    }

    /// Check if a function exists in a module
    pub fn has_function(&self, module_path: &str, function_name: &str) -> bool {
        self.get_function(module_path, function_name).is_some()
    }

    /// Resolve a module path from import statement
    /// e.g., ["标准库", "加密"] -> "标准库.加密"
    pub fn resolve_module_path(&self, path_parts: &[String]) -> Option<String> {
        let full_path = path_parts.join(".");

        // Try exact match first
        if self.has_module(&full_path) {
            return Some(full_path);
        }

        // Try without "标准库" prefix
        if path_parts.len() > 1 && path_parts[0] == "标准库" {
            let short_path = path_parts[1..].join(".");
            if self.has_module(&short_path) {
                return Some(short_path);
            }
        }

        None
    }

    /// Get all registered module paths
    pub fn module_paths(&self) -> Vec<&String> {
        self.modules.keys().collect()
    }

    /// 注册字符串模块
    fn register_string_module(&mut self) {
        let mut string_module = Module::new("字符串");

        // 查找子字符串位置
        string_module.add_function(ModuleFunction::new(
            "查找",
            "qi_string_find",
            vec!["字符串".to_string(), "字符串".to_string()],
            "整数",
        ));

        // 从指定位置开始查找
        string_module.add_function(ModuleFunction::new(
            "查找从位置",
            "qi_string_find_from",
            vec![
                "字符串".to_string(),
                "字符串".to_string(),
                "整数".to_string(),
            ],
            "整数",
        ));

        // 提取子字符串 (开始位置, 长度)
        string_module.add_function(ModuleFunction::new(
            "子串",
            "qi_string_substring",
            vec!["字符串".to_string(), "整数".to_string(), "整数".to_string()],
            "字符串",
        ));

        // 从位置提取到末尾
        string_module.add_function(ModuleFunction::new(
            "子串从位置",
            "qi_string_substring_from",
            vec!["字符串".to_string(), "整数".to_string()],
            "字符串",
        ));

        // 获取字符串字节长度
        // 实时页面增量下发用：公共前缀/后缀（字节数，已对齐字符边界）
        string_module.add_function(ModuleFunction::new(
            "公共前缀",
            "qi_string_common_prefix",
            vec!["字符串".to_string(), "字符串".to_string()],
            "整数",
        ));
        string_module.add_function(ModuleFunction::new(
            "公共后缀",
            "qi_string_common_suffix",
            vec![
                "字符串".to_string(),
                "字符串".to_string(),
                "整数".to_string(),
            ],
            "整数",
        ));

        string_module.add_function(ModuleFunction::new(
            "字节长度",
            "qi_string_byte_length",
            vec!["字符串".to_string()],
            "整数",
        ));

        // 获取字符串字符数量 (UTF-8)
        string_module.add_function(ModuleFunction::new(
            "字符数量",
            "qi_string_char_count",
            vec!["字符串".to_string()],
            "整数",
        ));

        // 按字符（Unicode 标量）提取子串 (起字符, 字符数)，越界钳制
        string_module.add_function(ModuleFunction::new(
            "字符子串",
            "qi_string_char_substring",
            vec!["字符串".to_string(), "整数".to_string(), "整数".to_string()],
            "字符串",
        ));

        // 按字符查找子串，返回字符索引，未找到返回 -1
        string_module.add_function(ModuleFunction::new(
            "字符查找",
            "qi_string_char_find",
            vec!["字符串".to_string(), "字符串".to_string()],
            "整数",
        ));

        // 按字符索引取单个字符（返回单字符串），越界返回空串
        string_module.add_function(ModuleFunction::new(
            "字符取",
            "qi_string_char_at",
            vec!["字符串".to_string(), "整数".to_string()],
            "字符串",
        ));

        // 按字符从指定位置取到末尾
        string_module.add_function(ModuleFunction::new(
            "字符从位置",
            "qi_string_char_from",
            vec!["字符串".to_string(), "整数".to_string()],
            "字符串",
        ));

        // 字符串分割
        string_module.add_function(ModuleFunction::new(
            "分割",
            "qi_string_split",
            vec!["字符串".to_string(), "字符串".to_string()],
            "整数", // 返回列表句柄
        ));

        // 字符串替换
        string_module.add_function(ModuleFunction::new(
            "替换",
            "qi_string_replace",
            vec![
                "字符串".to_string(),
                "字符串".to_string(),
                "字符串".to_string(),
            ],
            "字符串",
        ));

        // 去除首尾空白
        string_module.add_function(ModuleFunction::new(
            "去空白",
            "qi_string_trim",
            vec!["字符串".to_string()],
            "字符串",
        ));

        // 转大写
        string_module.add_function(ModuleFunction::new(
            "转大写",
            "qi_string_to_upper",
            vec!["字符串".to_string()],
            "字符串",
        ));

        // 转小写
        string_module.add_function(ModuleFunction::new(
            "转小写",
            "qi_string_to_lower",
            vec!["字符串".to_string()],
            "字符串",
        ));

        // 是否包含子串
        string_module.add_function(ModuleFunction::new(
            "包含",
            "qi_string_contains",
            vec!["字符串".to_string(), "字符串".to_string()],
            "整数", // 返回 1 (true) 或 0 (false)
        ));

        // 是否以某字符串开始
        string_module.add_function(ModuleFunction::new(
            "开始于",
            "qi_string_starts_with",
            vec!["字符串".to_string(), "字符串".to_string()],
            "整数",
        ));

        // 是否以某字符串结束
        string_module.add_function(ModuleFunction::new(
            "结束于",
            "qi_string_ends_with",
            vec!["字符串".to_string(), "字符串".to_string()],
            "整数",
        ));

        // 字符串相等比较
        string_module.add_function(ModuleFunction::new(
            "等于",
            "qi_string_equals",
            vec!["字符串".to_string(), "字符串".to_string()],
            "整数",
        ));

        // Note: qi_string_free is already available from future.rs, so we don't need to register it separately

        self.modules
            .insert("字符串".to_string(), string_module.clone());
        self.modules
            .insert("标准库.字符串".to_string(), string_module.clone());
        // 使用 "文本" 作为别名，因为 "字符串" 是类型关键词，无法在导入语句中使用
        self.modules
            .insert("文本".to_string(), string_module.clone());
        self.modules
            .insert("标准库.文本".to_string(), string_module);
    }

    /// 注册正则表达式模块
    fn register_regex_module(&mut self) {
        let mut regex_module = Module::new("正则");

        regex_module.add_function(ModuleFunction::new(
            "是否匹配",
            "qi_regex_is_match",
            vec!["字符串".to_string(), "字符串".to_string()],
            "i32",
        ));

        regex_module.add_function(ModuleFunction::new(
            "查找",
            "qi_regex_find",
            vec!["字符串".to_string(), "字符串".to_string()],
            "ptr",
        ));

        regex_module.add_function(ModuleFunction::new(
            "查找全部",
            "qi_regex_find_all",
            vec!["字符串".to_string(), "字符串".to_string()],
            "ptr",
        ));

        regex_module.add_function(ModuleFunction::new(
            "全部替换",
            "qi_regex_replace_all",
            vec![
                "字符串".to_string(),
                "字符串".to_string(),
                "字符串".to_string(),
            ],
            "ptr",
        ));

        regex_module.add_function(ModuleFunction::new(
            "切割",
            "qi_regex_split",
            vec!["字符串".to_string(), "字符串".to_string()],
            "ptr",
        ));

        self.modules
            .insert("正则".to_string(), regex_module.clone());
        self.modules.insert("标准库.正则".to_string(), regex_module);
    }

    /// 注册路径处理模块
    fn register_path_module(&mut self) {
        let mut path_module = Module::new("路径");

        path_module.add_function(ModuleFunction::new(
            "连接",
            "qi_path_join",
            vec!["字符串".to_string(), "字符串".to_string()],
            "ptr",
        ));

        path_module.add_function(ModuleFunction::new(
            "文件名",
            "qi_path_filename",
            vec!["字符串".to_string()],
            "ptr",
        ));

        path_module.add_function(ModuleFunction::new(
            "父目录",
            "qi_path_parent",
            vec!["字符串".to_string()],
            "ptr",
        ));

        path_module.add_function(ModuleFunction::new(
            "扩展名",
            "qi_path_extension",
            vec!["字符串".to_string()],
            "ptr",
        ));

        path_module.add_function(ModuleFunction::new(
            "绝对路径",
            "qi_path_absolute",
            vec!["字符串".to_string()],
            "ptr",
        ));

        path_module.add_function(ModuleFunction::new(
            "存在",
            "qi_path_exists",
            vec!["字符串".to_string()],
            "i32",
        ));

        path_module.add_function(ModuleFunction::new(
            "是目录",
            "qi_path_is_dir",
            vec!["字符串".to_string()],
            "i32",
        ));

        path_module.add_function(ModuleFunction::new(
            "是文件",
            "qi_path_is_file",
            vec!["字符串".to_string()],
            "i32",
        ));

        self.modules.insert("路径".to_string(), path_module.clone());
        self.modules.insert("标准库.路径".to_string(), path_module);
    }

    /// 注册随机数模块
    fn register_random_module(&mut self) {
        let mut random_module = Module::new("随机");

        random_module.add_function(ModuleFunction::new(
            "生成整数",
            "qi_random_int",
            vec!["整数".to_string(), "整数".to_string()],
            "i64",
        ));

        random_module.add_function(ModuleFunction::new(
            "生成浮点",
            "qi_random_float",
            vec!["浮点数".to_string(), "浮点数".to_string()],
            "double",
        ));

        random_module.add_function(ModuleFunction::new(
            "生成布尔",
            "qi_random_bool",
            vec![],
            "i32",
        ));

        random_module.add_function(ModuleFunction::new(
            "生成字符串",
            "qi_random_string",
            vec!["整数".to_string()],
            "ptr",
        ));

        random_module.add_function(ModuleFunction::new("UUID", "qi_random_uuid", vec![], "ptr"));

        self.modules
            .insert("随机".to_string(), random_module.clone());
        self.modules
            .insert("标准库.随机".to_string(), random_module);

        // ── 数学 模块：浮点标量数学函数（math_ffi.rs，底层 libm）──
        let mut math_module = Module::new("数学");
        let 一元 = |名: &str, ffi: &str| {
            ModuleFunction::new(名, ffi, vec!["浮点数".to_string()], "double")
        };
        let 二元 = |名: &str, ffi: &str| {
            ModuleFunction::new(
                名,
                ffi,
                vec!["浮点数".to_string(), "浮点数".to_string()],
                "double",
            )
        };
        math_module.add_function(一元("指数", "qi_math_exp")); // e^x
        math_module.add_function(一元("自然对数", "qi_math_ln"));
        math_module.add_function(一元("对数10", "qi_math_log10"));
        math_module.add_function(一元("对数2", "qi_math_log2"));
        math_module.add_function(一元("开方", "qi_math_sqrt"));
        math_module.add_function(一元("立方根", "qi_math_cbrt"));
        math_module.add_function(二元("幂", "qi_math_pow")); // 底^指
        math_module.add_function(一元("绝对值", "qi_math_abs"));
        math_module.add_function(一元("向上取整", "qi_math_ceil"));
        math_module.add_function(一元("向下取整", "qi_math_floor"));
        math_module.add_function(一元("四舍五入", "qi_math_round"));
        math_module.add_function(一元("正弦", "qi_math_sin"));
        math_module.add_function(一元("余弦", "qi_math_cos"));
        math_module.add_function(一元("正切", "qi_math_tan"));
        math_module.add_function(一元("反正弦", "qi_math_asin"));
        math_module.add_function(一元("反余弦", "qi_math_acos"));
        math_module.add_function(一元("反正切", "qi_math_atan"));
        math_module.add_function(一元("双曲正切", "qi_math_tanh")); // 神经网络激活
        math_module.add_function(二元("最大", "qi_math_max"));
        math_module.add_function(二元("最小", "qi_math_min"));
        math_module.add_function(ModuleFunction::new(
            "圆周率",
            "qi_math_pi",
            vec![],
            "double",
        ));
        math_module.add_function(ModuleFunction::new(
            "自然常数",
            "qi_math_e",
            vec![],
            "double",
        ));
        self.modules.insert("数学".to_string(), math_module.clone());
        self.modules.insert("标准库.数学".to_string(), math_module);
    }

    /// 注册环境变量模块
    fn register_env_module(&mut self) {
        let mut env_module = Module::new("环境");

        env_module.add_function(ModuleFunction::new(
            "获取",
            "qi_env_get",
            vec!["字符串".to_string()],
            "ptr",
        ));

        env_module.add_function(ModuleFunction::new(
            "设置",
            "qi_env_set",
            vec!["字符串".to_string(), "字符串".to_string()],
            "i32",
        ));

        env_module.add_function(ModuleFunction::new(
            "删除",
            "qi_env_remove",
            vec!["字符串".to_string()],
            "i32",
        ));

        env_module.add_function(ModuleFunction::new(
            "当前目录",
            "qi_env_current_dir",
            vec![],
            "ptr",
        ));

        env_module.add_function(ModuleFunction::new(
            "改变目录",
            "qi_env_set_current_dir",
            vec!["字符串".to_string()],
            "i32",
        ));

        env_module.add_function(ModuleFunction::new(
            "主目录",
            "qi_env_home_dir",
            vec![],
            "ptr",
        ));

        env_module.add_function(ModuleFunction::new("全部", "qi_env_all", vec![], "ptr"));

        self.modules.insert("环境".to_string(), env_module.clone());
        self.modules.insert("标准库.环境".to_string(), env_module);
    }

    /// 注册进程管理模块
    fn register_process_module(&mut self) {
        let mut process_module = Module::new("进程");

        process_module.add_function(ModuleFunction::new(
            "执行",
            "qi_process_execute",
            vec!["字符串".to_string(), "字符串".to_string()],
            "ptr",
        ));

        process_module.add_function(ModuleFunction::new(
            "当前ID",
            "qi_process_current_pid",
            vec![],
            "i64",
        ));

        process_module.add_function(ModuleFunction::new(
            "退出",
            "qi_process_exit",
            vec!["i32".to_string()],
            "void",
        ));

        self.modules
            .insert("进程".to_string(), process_module.clone());
        self.modules
            .insert("标准库.进程".to_string(), process_module);
    }

    /// 注册子进程模块
    fn register_subprocess_module(&mut self) {
        let mut m = Module::new("子进程");

        m.add_function(ModuleFunction::new(
            "生成",
            "qi_subprocess_spawn",
            vec!["字符串".to_string(), "字符串".to_string()], // 命令, 参数JSON
            "i64",
        ));
        m.add_function(ModuleFunction::new(
            "写入行",
            "qi_subprocess_write_line",
            vec!["i64".to_string(), "字符串".to_string()], // 句柄, 行内容
            "i32",
        ));
        m.add_function(ModuleFunction::new(
            "读取行",
            "qi_subprocess_read_line",
            vec!["i64".to_string()],
            "ptr", // 返回字符串
        ));
        m.add_function(ModuleFunction::new(
            "读取行超时",
            "qi_subprocess_read_line_timeout",
            vec!["i64".to_string(), "i64".to_string()], // 句柄, 超时毫秒
            "ptr",                                      // 返回字符串
        ));
        m.add_function(ModuleFunction::new(
            "存活",
            "qi_subprocess_is_alive",
            vec!["i64".to_string()],
            "i32",
        ));
        m.add_function(ModuleFunction::new(
            "结束",
            "qi_subprocess_terminate",
            vec!["i64".to_string()],
            "i32",
        ));

        self.modules.insert("子进程".to_string(), m.clone());
        self.modules.insert("标准库.子进程".to_string(), m);
    }

    /// 注册配置文件模块
    fn register_config_module(&mut self) {
        let mut config_module = Module::new("配置");

        config_module.add_function(ModuleFunction::new(
            "读取TOML",
            "qi_config_read_toml",
            vec!["字符串".to_string()],
            "ptr",
        ));

        config_module.add_function(ModuleFunction::new(
            "写入TOML",
            "qi_config_write_toml",
            vec!["字符串".to_string(), "字符串".to_string()],
            "i32",
        ));

        config_module.add_function(ModuleFunction::new(
            "读取INI",
            "qi_config_read_ini",
            vec!["字符串".to_string()],
            "ptr",
        ));

        config_module.add_function(ModuleFunction::new(
            "写入INI",
            "qi_config_write_ini",
            vec!["字符串".to_string(), "字符串".to_string()],
            "i32",
        ));

        self.modules
            .insert("配置".to_string(), config_module.clone());
        self.modules
            .insert("标准库.配置".to_string(), config_module);
    }

    /// 注册压缩解压模块
    fn register_compress_module(&mut self) {
        let mut compress_module = Module::new("压缩");

        compress_module.add_function(ModuleFunction::new(
            "压缩文件",
            "qi_compress_gzip_file",
            vec!["字符串".to_string(), "字符串".to_string()],
            "i32",
        ));

        compress_module.add_function(ModuleFunction::new(
            "解压文件",
            "qi_compress_gunzip_file",
            vec!["字符串".to_string(), "字符串".to_string()],
            "i32",
        ));

        compress_module.add_function(ModuleFunction::new(
            "压缩字符串",
            "qi_compress_gzip_string",
            vec!["字符串".to_string()],
            "ptr",
        ));

        compress_module.add_function(ModuleFunction::new(
            "解压字符串",
            "qi_compress_gunzip_string",
            vec!["字符串".to_string()],
            "ptr",
        ));

        // 二进制安全：直接对字节切片句柄做 gzip / gunzip，不经 base64
        compress_module.add_function(ModuleFunction::new(
            "压缩字节",
            "qi_compress_gzip_bytes",
            vec!["整数".to_string()],
            "i64",
        ));

        compress_module.add_function(ModuleFunction::new(
            "解压字节",
            "qi_compress_gunzip_bytes",
            vec!["整数".to_string()],
            "i64",
        ));

        self.modules
            .insert("压缩".to_string(), compress_module.clone());
        self.modules
            .insert("标准库.压缩".to_string(), compress_module);
    }

    /// 注册测试框架模块
    fn register_test_module(&mut self) {
        let mut test_module = Module::new("测试");

        test_module.add_function(ModuleFunction::new(
            "断言相等_整数",
            "qi_test_assert_eq_int",
            vec!["整数".to_string(), "整数".to_string(), "字符串".to_string()],
            "i32",
        ));

        test_module.add_function(ModuleFunction::new(
            "断言相等_浮点",
            "qi_test_assert_eq_float",
            vec![
                "浮点数".to_string(),
                "浮点数".to_string(),
                "字符串".to_string(),
            ],
            "i32",
        ));

        test_module.add_function(ModuleFunction::new(
            "断言相等_字符串",
            "qi_test_assert_eq_string",
            vec![
                "字符串".to_string(),
                "字符串".to_string(),
                "字符串".to_string(),
            ],
            "i32",
        ));

        test_module.add_function(ModuleFunction::new(
            "断言真",
            "qi_test_assert_true",
            vec!["i32".to_string(), "字符串".to_string()],
            "i32",
        ));

        test_module.add_function(ModuleFunction::new(
            "断言假",
            "qi_test_assert_false",
            vec!["i32".to_string(), "字符串".to_string()],
            "i32",
        ));

        test_module.add_function(ModuleFunction::new(
            "断言不等_整数",
            "qi_test_assert_ne_int",
            vec!["整数".to_string(), "整数".to_string(), "字符串".to_string()],
            "i32",
        ));

        test_module.add_function(ModuleFunction::new(
            "测试通过",
            "qi_test_pass",
            vec!["字符串".to_string()],
            "void",
        ));

        test_module.add_function(ModuleFunction::new(
            "测试失败",
            "qi_test_fail",
            vec!["字符串".to_string(), "字符串".to_string()],
            "void",
        ));

        self.modules.insert("测试".to_string(), test_module.clone());
        self.modules.insert("标准库.测试".to_string(), test_module);
    }

    /// 注册数据库模块
    fn register_database_module(&mut self) {
        let mut db_module = Module::new("数据库");

        db_module.add_function(ModuleFunction::new(
            "连接",
            "qi_db_connect",
            vec!["字符串".to_string()],
            "i64",
        ));

        db_module.add_function(ModuleFunction::new(
            "执行",
            "qi_db_execute",
            vec!["整数".to_string(), "字符串".to_string()],
            "i64",
        ));

        db_module.add_function(ModuleFunction::new(
            "查询",
            "qi_db_query",
            vec!["整数".to_string(), "字符串".to_string()],
            "ptr",
        ));

        db_module.add_function(ModuleFunction::new(
            "执行参数",
            "qi_db_execute_params",
            vec![
                "整数".to_string(),
                "字符串".to_string(),
                "字符串".to_string(),
            ],
            "ptr",
        ));

        db_module.add_function(ModuleFunction::new(
            "查询参数",
            "qi_db_query_params",
            vec![
                "整数".to_string(),
                "字符串".to_string(),
                "字符串".to_string(),
            ],
            "ptr",
        ));

        db_module.add_function(ModuleFunction::new(
            "关闭",
            "qi_db_close",
            vec!["整数".to_string()],
            "i32",
        ));

        db_module.add_function(ModuleFunction::new(
            "开始事务",
            "qi_db_begin_transaction",
            vec!["整数".to_string()],
            "i32",
        ));

        db_module.add_function(ModuleFunction::new(
            "提交",
            "qi_db_commit",
            vec!["整数".to_string()],
            "i32",
        ));

        db_module.add_function(ModuleFunction::new(
            "回滚",
            "qi_db_rollback",
            vec!["整数".to_string()],
            "i32",
        ));

        db_module.add_function(ModuleFunction::new(
            "开启事务",
            "qi_db_transaction_open",
            vec!["整数".to_string()],
            "i64",
        ));

        db_module.add_function(ModuleFunction::new(
            "事务执行参数",
            "qi_db_transaction_execute_params",
            vec![
                "整数".to_string(),
                "字符串".to_string(),
                "字符串".to_string(),
            ],
            "ptr",
        ));

        db_module.add_function(ModuleFunction::new(
            "事务查询参数",
            "qi_db_transaction_query_params",
            vec![
                "整数".to_string(),
                "字符串".to_string(),
                "字符串".to_string(),
            ],
            "ptr",
        ));

        db_module.add_function(ModuleFunction::new(
            "提交事务",
            "qi_db_transaction_commit",
            vec!["整数".to_string()],
            "i32",
        ));

        db_module.add_function(ModuleFunction::new(
            "回滚事务",
            "qi_db_transaction_rollback",
            vec!["整数".to_string()],
            "i32",
        ));

        self.modules.insert("数据库".to_string(), db_module.clone());
        self.modules.insert("标准库.数据库".to_string(), db_module);
    }

    /// 注册同步原语模块（标准库.同步）
    fn register_sync_module(&mut self) {
        let mut m = Module::new("同步");

        // ── 互斥锁 ──────────────────────────────────────────────────
        m.add_function(ModuleFunction::new(
            "创建锁",
            "qi_sync_mutex_create",
            vec![],
            "i64",
        ));
        m.add_function(ModuleFunction::new(
            "加锁",
            "qi_sync_mutex_lock",
            vec!["i64".to_string()],
            "i32",
        ));
        m.add_function(ModuleFunction::new(
            "解锁",
            "qi_sync_mutex_unlock",
            vec!["i64".to_string()],
            "i32",
        ));
        m.add_function(ModuleFunction::new(
            "尝试加锁",
            "qi_sync_mutex_trylock",
            vec!["i64".to_string()],
            "i32",
        ));
        m.add_function(ModuleFunction::new(
            "销毁锁",
            "qi_sync_mutex_destroy",
            vec!["i64".to_string()],
            "i32",
        ));

        // ── 原子整数 ─────────────────────────────────────────────────
        m.add_function(ModuleFunction::new(
            "创建原子",
            "qi_sync_atomic_create",
            vec!["i64".to_string()],
            "i64",
        ));
        m.add_function(ModuleFunction::new(
            "读原子",
            "qi_sync_atomic_load",
            vec!["i64".to_string()],
            "i64",
        ));
        m.add_function(ModuleFunction::new(
            "写原子",
            "qi_sync_atomic_store",
            vec!["i64".to_string(), "i64".to_string()],
            "i32",
        ));
        m.add_function(ModuleFunction::new(
            "原子加",
            "qi_sync_atomic_add",
            vec!["i64".to_string(), "i64".to_string()],
            "i64",
        ));
        m.add_function(ModuleFunction::new(
            "原子比较交换",
            "qi_sync_atomic_cas",
            vec!["i64".to_string(), "i64".to_string(), "i64".to_string()],
            "i32",
        ));
        m.add_function(ModuleFunction::new(
            "销毁原子",
            "qi_sync_atomic_destroy",
            vec!["i64".to_string()],
            "i32",
        ));

        // ── 协程异常 ─────────────────────────────────────────────────
        // goroutine 内未捕获的 `抛出` 进全局队列（fire-and-forget `启动`）
        // 或句柄状态（启动并等待协程）。FFI 见 qi-runtime：
        // stdlib/exception_ffi.rs + async_runtime/ffi/mod.rs。
        m.add_function(ModuleFunction::new(
            "协程异常数量",
            "qi_exc_goroutine_count",
            vec![],
            "整数",
        ));
        m.add_function(ModuleFunction::new(
            "获取协程异常",
            "qi_exc_goroutine_take",
            vec![],
            "字符串",
        ));
        m.add_function(ModuleFunction::new(
            "启动并等待协程",
            "qi_runtime_spawn_goroutine_handle",
            vec!["指针".to_string()], // 函数值（fat 闭包对象）
            "整数",                   // 协程句柄
        ));
        m.add_function(ModuleFunction::new(
            "等待协程",
            "qi_runtime_goroutine_join",
            vec!["整数".to_string()], // 句柄
            "整数",
        ));
        m.add_function(ModuleFunction::new(
            "协程有异常",
            "qi_runtime_goroutine_has_exception",
            vec!["整数".to_string()], // 句柄
            "整数",                   // 1=有 0=无 -1=未知句柄
        ));
        m.add_function(ModuleFunction::new(
            "获取协程异常句柄",
            "qi_runtime_goroutine_take_exception",
            vec!["整数".to_string()], // 句柄
            "字符串",
        ));

        self.modules.insert("同步".to_string(), m.clone());
        self.modules.insert("标准库.同步".to_string(), m);
    }
}

impl Default for ModuleRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_registry_creation() {
        let registry = ModuleRegistry::new();
        assert!(registry.has_module("加密"));
        assert!(registry.has_module("标准库.加密"));
    }

    #[test]
    fn test_crypto_module_functions() {
        let registry = ModuleRegistry::new();

        // Test MD5哈希
        assert!(registry.has_function("加密", "MD5哈希"));
        let md5 = registry.get_function("加密", "MD5哈希").unwrap();
        assert_eq!(md5.runtime_name, "qi_crypto_md5");
        assert_eq!(md5.param_types.len(), 1);
        assert_eq!(md5.return_type, "字符串");

        // Test SHA256哈希
        assert!(registry.has_function("加密", "SHA256哈希"));
        let sha256 = registry.get_function("加密", "SHA256哈希").unwrap();
        assert_eq!(sha256.runtime_name, "qi_crypto_sha256");

        // Test HMAC_SHA256
        assert!(registry.has_function("加密", "HMAC_SHA256"));
        let hmac = registry.get_function("加密", "HMAC_SHA256").unwrap();
        assert_eq!(hmac.runtime_name, "qi_crypto_hmac_sha256");
        assert_eq!(hmac.param_types.len(), 2);
    }

    #[test]
    fn test_module_path_resolution() {
        let registry = ModuleRegistry::new();

        // Test full path
        let path = registry.resolve_module_path(&["标准库".to_string(), "加密".to_string()]);
        assert!(path.is_some());
        let path_str = path.unwrap();
        assert!(path_str == "标准库.加密" || path_str == "加密");

        // Test short path
        let path = registry.resolve_module_path(&["加密".to_string()]);
        assert!(path.is_some());

        // Test non-existent module
        let path = registry.resolve_module_path(&["不存在的模块".to_string()]);
        assert!(path.is_none());
    }

    #[test]
    fn test_module_function_listing() {
        let registry = ModuleRegistry::new();
        let crypto = registry.get_module("加密").unwrap();

        let functions = crypto.function_names();
        assert!(functions.len() >= 6); // At least 6 crypto functions

        assert!(functions.contains(&&"MD5哈希".to_string()));
        assert!(functions.contains(&&"SHA256哈希".to_string()));
        assert!(functions.contains(&&"SHA512哈希".to_string()));
        assert!(functions.contains(&&"Base64编码".to_string()));
        assert!(functions.contains(&&"Base64解码".to_string()));
        assert!(functions.contains(&&"HMAC_SHA256".to_string()));
    }

    #[test]
    fn test_json_module() {
        let registry = ModuleRegistry::new();

        // Test JSON module exists
        assert!(registry.has_module("JSON"));
        assert!(registry.has_module("标准库.JSON"));

        // Test JSON object functions
        assert!(registry.has_function("JSON", "创建对象"));
        assert!(registry.has_function("JSON", "创建数组"));
        assert!(registry.has_function("JSON", "设置字符串"));
        assert!(registry.has_function("JSON", "获取字符串"));

        // Test JSON array functions
        assert!(registry.has_function("JSON", "数组添加字符串"));
        assert!(registry.has_function("JSON", "数组获取字符串"));
        assert!(registry.has_function("JSON", "数组长度"));

        // Test utility functions
        assert!(registry.has_function("JSON", "转字符串"));
        assert!(registry.has_function("JSON", "格式化"));
        assert!(registry.has_function("JSON", "是否包含键"));
        assert!(registry.has_function("JSON", "删除"));

        // Test function details
        let create_obj = registry.get_function("JSON", "创建对象").unwrap();
        assert_eq!(create_obj.runtime_name, "qi_json_create_object");
        assert_eq!(create_obj.param_types.len(), 0);
        assert_eq!(create_obj.return_type, "整数");

        let set_string = registry.get_function("JSON", "设置字符串").unwrap();
        assert_eq!(set_string.runtime_name, "qi_json_set_string");
        assert_eq!(set_string.param_types.len(), 3);
        assert_eq!(set_string.return_type, "整数");
    }
}
