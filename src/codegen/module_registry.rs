//! Module Registry for Qi Language
//!
//! This module provides a registry system for managing standard library modules
//! and their functions, enabling modular imports and namespace resolution.

use std::collections::HashMap;

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
        self.modules.insert("加密".to_string(), crypto_module.clone());
        self.modules.insert("标准库.加密".to_string(), crypto_module);
    }

    /// Register the IO module
    fn register_io_module(&mut self) {
        let mut io_module = Module::new("io");

        // Note: 打印 and 打印行 are built-in functions and NOT part of the io module
        // They are always available without import

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
            "整数",  // Returns 0 or 1 as i64
        ));

        io_module.add_function(ModuleFunction::new(
            "追加文件",
            "qi_io_append_file",
            vec!["字符串".to_string(), "字符串".to_string()],
            "整数",  // Returns 0 or 1 as i64
        ));

        io_module.add_function(ModuleFunction::new(
            "删除文件",
            "qi_io_delete_file",
            vec!["字符串".to_string()],
            "整数",  // Returns 0 or 1 as i64
        ));

        io_module.add_function(ModuleFunction::new(
            "创建文件",
            "qi_io_create_file",
            vec!["字符串".to_string()],
            "整数",  // Returns 0 or 1 as i64
        ));

        io_module.add_function(ModuleFunction::new(
            "文件存在",
            "qi_io_file_exists",
            vec!["字符串".to_string()],
            "整数",  // Returns 0 or 1 as i64
        ));

        io_module.add_function(ModuleFunction::new(
            "文件大小",
            "qi_io_file_size",
            vec!["字符串".to_string()],
            "整数",
        ));

        io_module.add_function(ModuleFunction::new(
            "创建目录",
            "qi_io_create_dir",
            vec!["字符串".to_string()],
            "整数",  // Returns 0 or 1 as i64
        ));

        io_module.add_function(ModuleFunction::new(
            "删除目录",
            "qi_io_delete_dir",
            vec!["字符串".to_string()],
            "整数",  // Returns 0 or 1 as i64
        ));

        // Register module with both Chinese and path formats
        self.modules.insert("io".to_string(), io_module.clone());
        self.modules.insert("标准库.io".to_string(), io_module);
    }

    /// Register the network module
    fn register_network_module(&mut self) {
        let mut network_module = Module::new("网络");

        // TCP 连接函数
        network_module.add_function(ModuleFunction::new(
            "TCP连接",
            "qi_network_tcp_connect",
            vec!["字符串".to_string(), "整数".to_string(), "整数".to_string()], // 主机, 端口, 超时(毫秒)
            "整数",  // 返回连接句柄
        ));

        network_module.add_function(ModuleFunction::new(
            "TCP读取",
            "qi_network_tcp_read",
            vec!["整数".to_string(), "整数".to_string(), "整数".to_string()], // 句柄, 缓冲区指针, 大小
            "整数",  // 返回读取字节数
        ));

        network_module.add_function(ModuleFunction::new(
            "TCP写入",
            "qi_network_tcp_write",
            vec!["整数".to_string(), "整数".to_string(), "整数".to_string()], // 句柄, 数据指针, 大小
            "整数",  // 返回写入字节数
        ));

        network_module.add_function(ModuleFunction::new(
            "TCP关闭",
            "qi_network_tcp_close",
            vec!["整数".to_string()], // 句柄
            "整数",  // 返回成功/失败
        ));

        network_module.add_function(ModuleFunction::new(
            "TCP刷新",
            "qi_network_tcp_flush",
            vec!["整数".to_string()], // 句柄
            "整数",  // 返回成功/失败
        ));

        network_module.add_function(ModuleFunction::new(
            "解析主机",
            "qi_network_resolve_host",
            vec!["字符串".to_string()], // 主机名
            "字符串",  // 返回 IP 地址
        ));

        network_module.add_function(ModuleFunction::new(
            "端口可用",
            "qi_network_port_available",
            vec!["整数".to_string()], // 端口
            "整数",  // 返回 1 可用，0 不可用
        ));

        network_module.add_function(ModuleFunction::new(
            "获取本机IP",
            "qi_network_get_local_ip",
            vec![], // 无参数
            "字符串",  // 返回本机 IP
        ));

        // Register module with both Chinese and path formats
        self.modules.insert("网络".to_string(), network_module.clone());
        self.modules.insert("标准库.网络".to_string(), network_module);
    }

    /// Register the HTTP module
    fn register_http_module(&mut self) {
        let mut http_module = Module::new("HTTP");

        // 基本 HTTP 请求方法 (使用全中文函数名)
        http_module.add_function(ModuleFunction::new(
            "获取",
            "qi_http_get",
            vec!["字符串".to_string()], // URL
            "字符串",  // 返回响应体
        ));

        http_module.add_function(ModuleFunction::new(
            "发送",
            "qi_http_post",
            vec!["字符串".to_string(), "字符串".to_string()], // URL, 请求体
            "字符串",  // 返回响应体
        ));

        http_module.add_function(ModuleFunction::new(
            "更新",
            "qi_http_put",
            vec!["字符串".to_string(), "字符串".to_string()], // URL, 请求体
            "字符串",  // 返回响应体
        ));

        http_module.add_function(ModuleFunction::new(
            "删除",
            "qi_http_delete",
            vec!["字符串".to_string()], // URL
            "字符串",  // 返回响应体
        ));

        // 高级请求构建器
        http_module.add_function(ModuleFunction::new(
            "创建请求",
            "qi_http_request_create",
            vec!["字符串".to_string(), "字符串".to_string()], // 方法, URL
            "整数",  // 返回请求句柄
        ));

        http_module.add_function(ModuleFunction::new(
            "设置请求头",
            "qi_http_request_set_header",
            vec!["整数".to_string(), "字符串".to_string(), "字符串".to_string()], // 句柄, 名称, 值
            "整数",  // 返回成功/失败
        ));

        http_module.add_function(ModuleFunction::new(
            "设置请求体",
            "qi_http_request_set_body",
            vec!["整数".to_string(), "字符串".to_string()], // 句柄, 请求体
            "整数",  // 返回成功/失败
        ));

        http_module.add_function(ModuleFunction::new(
            "设置超时",
            "qi_http_request_set_timeout",
            vec!["整数".to_string(), "整数".to_string()], // 句柄, 超时(毫秒)
            "整数",  // 返回成功/失败
        ));

        http_module.add_function(ModuleFunction::new(
            "执行请求",
            "qi_http_request_execute",
            vec!["整数".to_string()], // 句柄
            "字符串",  // 返回响应体
        ));

        http_module.add_function(ModuleFunction::new(
            "获取状态码",
            "qi_http_get_status",
            vec!["字符串".to_string()], // URL
            "整数",  // 返回状态码
        ));

        // Register module with both Chinese and path formats (use lowercase 'http' for compatibility)
        self.modules.insert("http".to_string(), http_module.clone());
        self.modules.insert("标准库.http".to_string(), http_module);
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
}
