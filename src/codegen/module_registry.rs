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

        // Register vector module (向量模块)
        self.register_vector_module();

        // Register data structure modules (数据结构模块)
        self.register_list_module();
        self.register_hashmap_module();

        // Register datetime module (日期时间模块)
        self.register_datetime_module();

        // Register JSON module (JSON模块)
        self.register_json_module();
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

        // TCP Server functions
        network_module.add_function(ModuleFunction::new(
            "TCP监听",
            "qi_network_tcp_listen",
            vec!["字符串".to_string(), "整数".to_string(), "整数".to_string()], // 主机, 端口, 队列大小
            "整数",  // 返回服务器句柄
        ));

        network_module.add_function(ModuleFunction::new(
            "TCP接受连接",
            "qi_network_tcp_accept",
            vec!["整数".to_string()], // 服务器句柄
            "整数",  // 返回客户端句柄
        ));

        network_module.add_function(ModuleFunction::new(
            "TCP服务器关闭",
            "qi_network_tcp_server_close",
            vec!["整数".to_string()], // 服务器句柄
            "整数",  // 返回成功/失败
        ));

        // UDP functions
        network_module.add_function(ModuleFunction::new(
            "UDP绑定",
            "qi_network_udp_bind",
            vec!["字符串".to_string(), "整数".to_string()], // 主机, 端口
            "整数",  // 返回 UDP 套接字句柄
        ));

        network_module.add_function(ModuleFunction::new(
            "UDP发送到",
            "qi_network_udp_send_string",
            vec!["整数".to_string(), "字符串".to_string(), "字符串".to_string(), "整数".to_string()], // 句柄, 消息, 目标主机, 目标端口
            "整数",  // 返回发送字节数
        ));

        network_module.add_function(ModuleFunction::new(
            "UDP接收",
            "qi_network_udp_recv_string",
            vec!["整数".to_string(), "整数".to_string()], // 句柄, 缓冲区大小
            "字符串",  // 返回接收到的数据
        ));

        network_module.add_function(ModuleFunction::new(
            "UDP关闭",
            "qi_network_udp_close",
            vec!["整数".to_string()], // 句柄
            "整数",  // 返回成功/失败
        ));

        network_module.add_function(ModuleFunction::new(
            "UDP设置超时",
            "qi_network_udp_set_timeout",
            vec!["整数".to_string(), "整数".to_string()], // 句柄, 超时毫秒
            "整数",  // 返回成功/失败
        ));

        network_module.add_function(ModuleFunction::new(
            "UDP设置广播",
            "qi_network_udp_set_broadcast",
            vec!["整数".to_string(), "整数".to_string()], // 句柄, 启用(1)/禁用(0)
            "整数",  // 返回成功/失败
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

        http_module.add_function(ModuleFunction::new(
            "请求头",
            "qi_http_head",
            vec!["字符串".to_string()], // URL
            "字符串",  // 返回状态信息
        ));

        http_module.add_function(ModuleFunction::new(
            "修补",
            "qi_http_patch",
            vec!["字符串".to_string(), "字符串".to_string()], // URL, 请求体
            "字符串",  // 返回响应体
        ));

        http_module.add_function(ModuleFunction::new(
            "选项",
            "qi_http_options",
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

        // HTTP 服务器功能
        http_module.add_function(ModuleFunction::new(
            "创建服务器",
            "qi_http_server_create",
            vec!["字符串".to_string(), "整数".to_string()], // 主机, 端口
            "整数",  // 返回服务器句柄
        ));

        http_module.add_function(ModuleFunction::new(
            "处理请求",
            "qi_http_server_handle_request",
            vec!["整数".to_string(), "字符串".to_string(), "整数".to_string()], // 服务器句柄, 响应体, 状态码
            "字符串",  // 返回请求信息 "方法|路径|请求体"
        ));

        http_module.add_function(ModuleFunction::new(
            "接受连接",
            "qi_http_server_accept",
            vec!["整数".to_string()], // 服务器句柄
            "字符串",  // 返回完整HTTP请求
        ));

        http_module.add_function(ModuleFunction::new(
            "关闭服务器",
            "qi_http_server_close",
            vec!["整数".to_string()], // 服务器句柄
            "整数",  // 返回成功/失败
        ));

        // Register module with both Chinese and path formats
        self.modules.insert("HTTP".to_string(), http_module.clone());
        self.modules.insert("标准库.HTTP".to_string(), http_module);
    }

    /// Register the vector module
    fn register_vector_module(&mut self) {
        let mut vector_module = Module::new("向量");

        // 向量点积 - 需要返回浮点数类型的中间结果
        // 注意：由于FFI限制，实际使用时需要传入结果指针
        vector_module.add_function(ModuleFunction::new(
            "点积",
            "qi_vector_dot",
            vec!["数组".to_string(), "整数".to_string(), "数组".to_string(), "整数".to_string()],
            "浮点数",
        ));

        // 向量加法
        vector_module.add_function(ModuleFunction::new(
            "加",
            "qi_vector_add",
            vec!["数组".to_string(), "整数".to_string(), "数组".to_string(), "整数".to_string()],
            "数组",
        ));

        // 向量长度(模)
        vector_module.add_function(ModuleFunction::new(
            "长度",
            "qi_vector_magnitude",
            vec!["数组".to_string(), "整数".to_string()],
            "浮点数",
        ));

        // 向量归一化
        vector_module.add_function(ModuleFunction::new(
            "归一化",
            "qi_vector_normalize",
            vec!["数组".to_string(), "整数".to_string()],
            "数组",
        ));

        // 余弦相似度
        vector_module.add_function(ModuleFunction::new(
            "余弦相似度",
            "qi_vector_cosine_similarity",
            vec!["数组".to_string(), "整数".to_string(), "数组".to_string(), "整数".to_string()],
            "浮点数",
        ));

        // 向量数乘
        vector_module.add_function(ModuleFunction::new(
            "数乘",
            "qi_vector_scale",
            vec!["数组".to_string(), "整数".to_string(), "浮点数".to_string()],
            "数组",
        ));

        // Register module with both Chinese and path formats
        self.modules.insert("向量".to_string(), vector_module.clone());
        self.modules.insert("标准库.向量".to_string(), vector_module);

        // LLM Module - 大模型模块
        let mut llm_module = Module::new("大模型");

        // 创建会话
        llm_module.add_function(ModuleFunction::new(
            "创建会话",
            "qi_llm_create_session",
            vec!["字符串".to_string(), "字符串".to_string(), "字符串".to_string()], // 端点, 模型, 密钥
            "整数",  // 返回会话句柄
        ));

        // 对话
        llm_module.add_function(ModuleFunction::new(
            "对话",
            "qi_llm_chat",
            vec!["整数".to_string(), "字符串".to_string()], // 会话句柄, 提示
            "字符串",  // 返回LLM响应
        ));

        // 设置配置
        llm_module.add_function(ModuleFunction::new(
            "设置配置",
            "qi_llm_set_config",
            vec!["整数".to_string(), "字符串".to_string(), "字符串".to_string()], // 会话句柄, 键, 值
            "整数",  // 返回状态
        ));

        // 清空历史
        llm_module.add_function(ModuleFunction::new(
            "清空历史",
            "qi_llm_clear_history",
            vec!["整数".to_string()], // 会话句柄
            "整数",  // 返回状态
        ));

        // 获取历史数量
        llm_module.add_function(ModuleFunction::new(
            "历史数量",
            "qi_llm_get_history_count",
            vec!["整数".to_string()], // 会话句柄
            "整数",  // 返回数量
        ));

        // 关闭会话
        llm_module.add_function(ModuleFunction::new(
            "关闭会话",
            "qi_llm_close_session",
            vec!["整数".to_string()], // 会话句柄
            "整数",  // 返回状态
        ));

        // 异步对话 (返回 Future<字符串>)
        llm_module.add_function(ModuleFunction::new(
            "异步对话",
            "qi_llm_chat_async",
            vec!["整数".to_string(), "字符串".to_string()], // 会话句柄, 提示
            "未来<字符串>",  // 返回Future<字符串>
        ));

        // Register module with both Chinese and path formats
        self.modules.insert("大模型".to_string(), llm_module.clone());
        self.modules.insert("标准库.大模型".to_string(), llm_module.clone());
        self.modules.insert("LLM".to_string(), llm_module);

        // ===== 操作系统模块 (OS Module) =====
        let mut os_module = Module::new("操作系统");

        // 环境变量操作
        os_module.add_function(ModuleFunction::new(
            "获取环境变量",
            "qi_os_getenv",
            vec!["字符串".to_string()], // 变量名
            "字符串",  // 返回变量值
        ));

        os_module.add_function(ModuleFunction::new(
            "设置环境变量",
            "qi_os_setenv",
            vec!["字符串".to_string(), "字符串".to_string()], // 变量名, 变量值
            "整数",  // 返回状态码
        ));

        os_module.add_function(ModuleFunction::new(
            "删除环境变量",
            "qi_os_unsetenv",
            vec!["字符串".to_string()], // 变量名
            "整数",  // 返回状态码
        ));

        os_module.add_function(ModuleFunction::new(
            "所有环境变量",
            "qi_os_environ",
            vec![], // 无参数
            "字符串",  // 返回所有环境变量
        ));

        // 目录操作
        os_module.add_function(ModuleFunction::new(
            "当前目录",
            "qi_os_getcwd",
            vec![], // 无参数
            "字符串",  // 返回当前目录路径
        ));

        os_module.add_function(ModuleFunction::new(
            "切换目录",
            "qi_os_chdir",
            vec!["字符串".to_string()], // 目标路径
            "整数",  // 返回状态码
        ));

        os_module.add_function(ModuleFunction::new(
            "用户主目录",
            "qi_os_homedir",
            vec![], // 无参数
            "字符串",  // 返回主目录路径
        ));

        os_module.add_function(ModuleFunction::new(
            "临时目录",
            "qi_os_tempdir",
            vec![], // 无参数
            "字符串",  // 返回临时目录路径
        ));

        // 系统信息
        os_module.add_function(ModuleFunction::new(
            "操作系统类型",
            "qi_os_type",
            vec![], // 无参数
            "字符串",  // 返回 windows/linux/macos
        ));

        os_module.add_function(ModuleFunction::new(
            "系统架构",
            "qi_os_arch",
            vec![], // 无参数
            "字符串",  // 返回 x86_64/aarch64
        ));

        os_module.add_function(ModuleFunction::new(
            "系统家族",
            "qi_os_family",
            vec![], // 无参数
            "字符串",  // 返回 unix/windows
        ));

        os_module.add_function(ModuleFunction::new(
            "主机名",
            "qi_os_hostname",
            vec![], // 无参数
            "字符串",  // 返回主机名
        ));

        os_module.add_function(ModuleFunction::new(
            "用户名",
            "qi_os_username",
            vec![], // 无参数
            "字符串",  // 返回用户名
        ));

        // CPU信息
        os_module.add_function(ModuleFunction::new(
            "CPU核心数",
            "qi_os_cpu_count",
            vec![], // 无参数
            "整数",  // 返回CPU核心数
        ));

        // 进程信息
        os_module.add_function(ModuleFunction::new(
            "进程ID",
            "qi_os_getpid",
            vec![], // 无参数
            "整数",  // 返回进程ID
        ));

        os_module.add_function(ModuleFunction::new(
            "退出程序",
            "qi_os_exit",
            vec!["整数".to_string()], // 退出码
            "void",  // 无返回值
        ));

        // 环境变量文件加载
        os_module.add_function(ModuleFunction::new(
            "加载环境文件",
            "qi_os_load_env",
            vec!["字符串".to_string()], // .env 文件路径
            "整数",  // 返回加载的环境变量数量
        ));

        // 目录操作
        os_module.add_function(ModuleFunction::new(
            "列出目录",
            "qi_os_list_dir",
            vec!["字符串".to_string()], // 目录路径
            "字符串",  // 返回目录内容列表
        ));

        os_module.add_function(ModuleFunction::new(
            "是否为目录",
            "qi_os_is_dir",
            vec!["字符串".to_string()], // 路径
            "整数",  // 返回1或0
        ));

        os_module.add_function(ModuleFunction::new(
            "是否为文件",
            "qi_os_is_file",
            vec!["字符串".to_string()], // 路径
            "整数",  // 返回1或0
        ));

        // 内存释放
        os_module.add_function(ModuleFunction::new(
            "释放字符串",
            "qi_os_free_string",
            vec!["字符串".to_string()], // 字符串指针
            "void",  // 无返回值
        ));

        // Register module with various names
        self.modules.insert("操作系统".to_string(), os_module.clone());
        self.modules.insert("标准库.操作系统".to_string(), os_module.clone());
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
            "整数",  // 返回应用ID
        ));

        cli_module.add_function(ModuleFunction::new(
            "设置版本",
            "qi_cli_set_version",
            vec!["整数".to_string(), "字符串".to_string()], // 应用ID, 版本号
            "整数",  // 成功返回1
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

        // 参数创建与配置
        cli_module.add_function(ModuleFunction::new(
            "创建参数",
            "qi_cli_create_arg",
            vec!["字符串".to_string()], // 参数名称
            "整数",  // 返回参数ID
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
            "整数",  // 返回子命令ID
        ));

        cli_module.add_function(ModuleFunction::new(
            "应用添加子命令",
            "qi_cli_app_add_subcommand",
            vec!["整数".to_string(), "整数".to_string()], // 应用ID, 子命令ID
            "整数",
        ));

        // 参数解析
        cli_module.add_function(ModuleFunction::new(
            "解析",
            "qi_cli_parse",
            vec!["整数".to_string()], // 应用ID
            "整数",  // 返回匹配结果ID
        ));

        // 结果获取
        cli_module.add_function(ModuleFunction::new(
            "获取值",
            "qi_cli_get_value",
            vec!["整数".to_string(), "字符串".to_string()], // 匹配结果ID, 参数名
            "字符串",  // 返回值
        ));

        cli_module.add_function(ModuleFunction::new(
            "获取标志",
            "qi_cli_get_flag",
            vec!["整数".to_string(), "字符串".to_string()], // 匹配结果ID, 参数名
            "整数",  // 返回布尔值(0/1)
        ));

        cli_module.add_function(ModuleFunction::new(
            "有值",
            "qi_cli_has_value",
            vec!["整数".to_string(), "字符串".to_string()], // 匹配结果ID, 参数名
            "整数",  // 返回布尔值(0/1)
        ));

        cli_module.add_function(ModuleFunction::new(
            "包含子命令",
            "qi_cli_has_subcommand",
            vec!["整数".to_string(), "字符串".to_string()], // 匹配结果ID, 子命令名
            "整数",  // 返回布尔值(0/1)
        ));

        cli_module.add_function(ModuleFunction::new(
            "获取子命令",
            "qi_cli_get_subcommand",
            vec!["整数".to_string(), "字符串".to_string()], // 匹配结果ID, 子命令名
            "整数",  // 返回子命令匹配结果ID
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
        self.modules.insert("命令行".to_string(), cli_module.clone());
        self.modules.insert("标准库.命令行".to_string(), cli_module.clone());
        self.modules.insert("CLI".to_string(), cli_module);

        // ===== 图形化模块 (GUI Module) =====
        self.register_gui_module();
    }

    /// 注册图形化窗口模块
    fn register_gui_module(&mut self) {
        let mut gui_module = Module::new("图形化");

        // 创建窗口
        gui_module.add_function(ModuleFunction::new(
            "创建窗口",
            "qi_gui_create_window",
            vec!["字符串".to_string(), "整数".to_string(), "整数".to_string()],
            "整数",
        ));

        // 销毁窗口
        gui_module.add_function(ModuleFunction::new(
            "销毁窗口",
            "qi_gui_destroy_window",
            vec!["整数".to_string()],
            "void",
        ));

        // 设置标题
        gui_module.add_function(ModuleFunction::new(
            "设置标题",
            "qi_gui_set_title",
            vec!["整数".to_string(), "字符串".to_string()],
            "void",
        ));

        // 获取标题
        gui_module.add_function(ModuleFunction::new(
            "获取标题",
            "qi_gui_get_title",
            vec!["整数".to_string()],
            "字符串",
        ));

        // 显示窗口
        gui_module.add_function(ModuleFunction::new(
            "显示窗口",
            "qi_gui_show_window",
            vec!["整数".to_string()],
            "void",
        ));

        // 隐藏窗口
        gui_module.add_function(ModuleFunction::new(
            "隐藏窗口",
            "qi_gui_hide_window",
            vec!["整数".to_string()],
            "void",
        ));

        // 是否可见
        gui_module.add_function(ModuleFunction::new(
            "是否可见",
            "qi_gui_is_visible",
            vec!["整数".to_string()],
            "整数",
        ));

        // 启用事件打印
        gui_module.add_function(ModuleFunction::new(
            "启用事件打印",
            "qi_gui_enable_event_printing",
            vec!["整数".to_string()],
            "void",
        ));

        // 获取窗口位置X
        gui_module.add_function(ModuleFunction::new(
            "获取位置X",
            "qi_gui_get_position_x",
            vec!["整数".to_string()],
            "整数",
        ));

        // 获取窗口位置Y
        gui_module.add_function(ModuleFunction::new(
            "获取位置Y",
            "qi_gui_get_position_y",
            vec!["整数".to_string()],
            "整数",
        ));

        // 设置窗口位置
        gui_module.add_function(ModuleFunction::new(
            "设置位置",
            "qi_gui_set_position",
            vec!["整数".to_string(), "整数".to_string(), "整数".to_string()],
            "void",
        ));

        // 获取窗口宽度
        gui_module.add_function(ModuleFunction::new(
            "获取宽度",
            "qi_gui_get_width",
            vec!["整数".to_string()],
            "整数",
        ));

        // 获取窗口高度
        gui_module.add_function(ModuleFunction::new(
            "获取高度",
            "qi_gui_get_height",
            vec!["整数".to_string()],
            "整数",
        ));

        // 设置窗口大小
        gui_module.add_function(ModuleFunction::new(
            "设置大小",
            "qi_gui_set_size",
            vec!["整数".to_string(), "整数".to_string(), "整数".to_string()],
            "void",
        ));

        // 运行事件循环
        gui_module.add_function(ModuleFunction::new(
            "运行",
            "qi_gui_run",
            vec![],
            "void",
        ));

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

        // Register module with various names
        self.modules.insert("图形化".to_string(), gui_module.clone());
        self.modules.insert("标准库.图形化".to_string(), gui_module.clone());
        self.modules.insert("GUI".to_string(), gui_module);
    }

    /// 注册列表模块
    fn register_list_module(&mut self) {
        let mut list_module = Module::new("列表");

        // 整数列表
        list_module.add_function(ModuleFunction::new("创建整数列表", "qi_list_int_create", vec![], "整数"));
        list_module.add_function(ModuleFunction::new("添加整数", "qi_list_int_push", vec!["整数".to_string(), "整数".to_string()], "整数"));
        list_module.add_function(ModuleFunction::new("获取整数", "qi_list_int_get", vec!["整数".to_string(), "整数".to_string()], "整数"));
        list_module.add_function(ModuleFunction::new("设置整数", "qi_list_int_set", vec!["整数".to_string(), "整数".to_string(), "整数".to_string()], "整数"));
        list_module.add_function(ModuleFunction::new("整数列表大小", "qi_list_int_size", vec!["整数".to_string()], "整数"));
        list_module.add_function(ModuleFunction::new("弹出整数", "qi_list_int_pop", vec!["整数".to_string()], "整数"));
        list_module.add_function(ModuleFunction::new("清空整数列表", "qi_list_int_clear", vec!["整数".to_string()], "整数"));
        list_module.add_function(ModuleFunction::new("删除整数元素", "qi_list_int_remove", vec!["整数".to_string(), "整数".to_string()], "整数"));
        list_module.add_function(ModuleFunction::new("插入整数", "qi_list_int_insert", vec!["整数".to_string(), "整数".to_string(), "整数".to_string()], "整数"));
        list_module.add_function(ModuleFunction::new("包含整数", "qi_list_int_contains", vec!["整数".to_string(), "整数".to_string()], "整数"));
        list_module.add_function(ModuleFunction::new("查找整数索引", "qi_list_int_index_of", vec!["整数".to_string(), "整数".to_string()], "整数"));

        // 浮点数列表
        list_module.add_function(ModuleFunction::new("创建浮点列表", "qi_list_float_create", vec![], "整数"));
        list_module.add_function(ModuleFunction::new("添加浮点数", "qi_list_float_push", vec!["整数".to_string(), "浮点数".to_string()], "整数"));
        list_module.add_function(ModuleFunction::new("获取浮点数", "qi_list_float_get", vec!["整数".to_string(), "整数".to_string()], "浮点数"));
        list_module.add_function(ModuleFunction::new("浮点列表大小", "qi_list_float_size", vec!["整数".to_string()], "整数"));

        // 字符串列表
        list_module.add_function(ModuleFunction::new("创建字符串列表", "qi_list_string_create", vec![], "整数"));
        list_module.add_function(ModuleFunction::new("添加字符串", "qi_list_string_push", vec!["整数".to_string(), "字符串".to_string()], "整数"));
        list_module.add_function(ModuleFunction::new("获取字符串", "qi_list_string_get", vec!["整数".to_string(), "整数".to_string()], "字符串"));
        list_module.add_function(ModuleFunction::new("字符串列表大小", "qi_list_string_size", vec!["整数".to_string()], "整数"));

        // 通用操作
        list_module.add_function(ModuleFunction::new("删除列表", "qi_list_free", vec!["整数".to_string()], "整数"));

        self.modules.insert("列表".to_string(), list_module.clone());
        self.modules.insert("标准库.列表".to_string(), list_module);
    }

    /// 注册哈希表模块
    fn register_hashmap_module(&mut self) {
        let mut map_module = Module::new("哈希表");

        // 整数哈希表
        map_module.add_function(ModuleFunction::new("创建整数表", "qi_hashmap_int_create", vec![], "整数"));
        map_module.add_function(ModuleFunction::new("设置整数", "qi_hashmap_int_set", vec!["整数".to_string(), "字符串".to_string(), "整数".to_string()], "整数"));
        map_module.add_function(ModuleFunction::new("获取整数", "qi_hashmap_int_get", vec!["整数".to_string(), "字符串".to_string()], "整数"));
        map_module.add_function(ModuleFunction::new("包含键", "qi_hashmap_int_contains", vec!["整数".to_string(), "字符串".to_string()], "整数"));
        map_module.add_function(ModuleFunction::new("删除键", "qi_hashmap_int_remove", vec!["整数".to_string(), "字符串".to_string()], "整数"));
        map_module.add_function(ModuleFunction::new("表大小", "qi_hashmap_int_size", vec!["整数".to_string()], "整数"));
        map_module.add_function(ModuleFunction::new("清空表", "qi_hashmap_int_clear", vec!["整数".to_string()], "整数"));

        // 浮点数哈希表
        map_module.add_function(ModuleFunction::new("创建浮点表", "qi_hashmap_float_create", vec![], "整数"));
        map_module.add_function(ModuleFunction::new("设置浮点数", "qi_hashmap_float_set", vec!["整数".to_string(), "字符串".to_string(), "浮点数".to_string()], "整数"));
        map_module.add_function(ModuleFunction::new("获取浮点数", "qi_hashmap_float_get", vec!["整数".to_string(), "字符串".to_string()], "浮点数"));
        map_module.add_function(ModuleFunction::new("浮点表大小", "qi_hashmap_float_size", vec!["整数".to_string()], "整数"));

        // 字符串哈希表
        map_module.add_function(ModuleFunction::new("创建字符串表", "qi_hashmap_string_create", vec![], "整数"));
        map_module.add_function(ModuleFunction::new("设置字符串", "qi_hashmap_string_set", vec!["整数".to_string(), "字符串".to_string(), "字符串".to_string()], "整数"));
        map_module.add_function(ModuleFunction::new("获取字符串", "qi_hashmap_string_get", vec!["整数".to_string(), "字符串".to_string()], "字符串"));
        map_module.add_function(ModuleFunction::new("字符串表大小", "qi_hashmap_string_size", vec!["整数".to_string()], "整数"));

        // 通用操作
        map_module.add_function(ModuleFunction::new("释放表", "qi_hashmap_free", vec!["整数".to_string()], "整数"));

        self.modules.insert("哈希表".to_string(), map_module.clone());
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
            "字符串",  // 返回JSON字符串
        ));

        // JSON解码
        json_module.add_function(ModuleFunction::new(
            "解码",
            "qi_json_decode",
            vec!["字符串".to_string()], // JSON字符串
            "整数",  // 返回JSON对象句柄
        ));

        // JSON对象操作
        json_module.add_function(ModuleFunction::new(
            "创建对象",
            "qi_json_create_object",
            vec![],
            "整数",  // 返回JSON对象句柄
        ));

        json_module.add_function(ModuleFunction::new(
            "创建数组",
            "qi_json_create_array",
            vec![],
            "整数",  // 返回JSON数组句柄
        ));

        // 对象字段设置
        json_module.add_function(ModuleFunction::new(
            "设置字符串",
            "qi_json_set_string",
            vec!["整数".to_string(), "字符串".to_string(), "字符串".to_string()], // 对象句柄, 键, 值
            "整数",  // 返回状态
        ));

        json_module.add_function(ModuleFunction::new(
            "设置整数",
            "qi_json_set_int",
            vec!["整数".to_string(), "字符串".to_string(), "整数".to_string()], // 对象句柄, 键, 值
            "整数",  // 返回状态
        ));

        json_module.add_function(ModuleFunction::new(
            "设置浮点数",
            "qi_json_set_float",
            vec!["整数".to_string(), "字符串".to_string(), "浮点数".to_string()], // 对象句柄, 键, 值
            "整数",  // 返回状态
        ));

        json_module.add_function(ModuleFunction::new(
            "设置布尔",
            "qi_json_set_bool",
            vec!["整数".to_string(), "字符串".to_string(), "整数".to_string()], // 对象句柄, 键, 值(0/1)
            "整数",  // 返回状态
        ));

        json_module.add_function(ModuleFunction::new(
            "设置对象",
            "qi_json_set_object",
            vec!["整数".to_string(), "字符串".to_string(), "整数".to_string()], // 对象句柄, 键, 子对象句柄
            "整数",  // 返回状态
        ));

        json_module.add_function(ModuleFunction::new(
            "设置数组",
            "qi_json_set_array",
            vec!["整数".to_string(), "字符串".to_string(), "整数".to_string()], // 对象句柄, 键, 数组句柄
            "整数",  // 返回状态
        ));

        // 对象字段获取
        json_module.add_function(ModuleFunction::new(
            "获取字符串",
            "qi_json_get_string",
            vec!["整数".to_string(), "字符串".to_string()], // 对象句柄, 键
            "字符串",  // 返回值
        ));

        json_module.add_function(ModuleFunction::new(
            "获取整数",
            "qi_json_get_int",
            vec!["整数".to_string(), "字符串".to_string()], // 对象句柄, 键
            "整数",  // 返回值
        ));

        json_module.add_function(ModuleFunction::new(
            "获取浮点数",
            "qi_json_get_float",
            vec!["整数".to_string(), "字符串".to_string()], // 对象句柄, 键
            "浮点数",  // 返回值
        ));

        json_module.add_function(ModuleFunction::new(
            "获取布尔",
            "qi_json_get_bool",
            vec!["整数".to_string(), "字符串".to_string()], // 对象句柄, 键
            "整数",  // 返回值(0/1)
        ));

        json_module.add_function(ModuleFunction::new(
            "获取对象",
            "qi_json_get_object",
            vec!["整数".to_string(), "字符串".to_string()], // 对象句柄, 键
            "整数",  // 返回子对象句柄
        ));

        json_module.add_function(ModuleFunction::new(
            "获取数组",
            "qi_json_get_array",
            vec!["整数".to_string(), "字符串".to_string()], // 对象句柄, 键
            "整数",  // 返回数组句柄
        ));

        // 数组操作
        json_module.add_function(ModuleFunction::new(
            "数组添加字符串",
            "qi_json_array_push_string",
            vec!["整数".to_string(), "字符串".to_string()], // 数组句柄, 值
            "整数",  // 返回状态
        ));

        json_module.add_function(ModuleFunction::new(
            "数组添加整数",
            "qi_json_array_push_int",
            vec!["整数".to_string(), "整数".to_string()], // 数组句柄, 值
            "整数",  // 返回状态
        ));

        json_module.add_function(ModuleFunction::new(
            "数组添加浮点数",
            "qi_json_array_push_float",
            vec!["整数".to_string(), "浮点数".to_string()], // 数组句柄, 值
            "整数",  // 返回状态
        ));

        json_module.add_function(ModuleFunction::new(
            "数组添加布尔",
            "qi_json_array_push_bool",
            vec!["整数".to_string(), "整数".to_string()], // 数组句柄, 值(0/1)
            "整数",  // 返回状态
        ));

        json_module.add_function(ModuleFunction::new(
            "数组添加对象",
            "qi_json_array_push_object",
            vec!["整数".to_string(), "整数".to_string()], // 数组句柄, 对象句柄
            "整数",  // 返回状态
        ));

        // 数组访问
        json_module.add_function(ModuleFunction::new(
            "数组获取字符串",
            "qi_json_array_get_string",
            vec!["整数".to_string(), "整数".to_string()], // 数组句柄, 索引
            "字符串",  // 返回值
        ));

        json_module.add_function(ModuleFunction::new(
            "数组获取整数",
            "qi_json_array_get_int",
            vec!["整数".to_string(), "整数".to_string()], // 数组句柄, 索引
            "整数",  // 返回值
        ));

        json_module.add_function(ModuleFunction::new(
            "数组获取浮点数",
            "qi_json_array_get_float",
            vec!["整数".to_string(), "整数".to_string()], // 数组句柄, 索引
            "浮点数",  // 返回值
        ));

        json_module.add_function(ModuleFunction::new(
            "数组获取布尔",
            "qi_json_array_get_bool",
            vec!["整数".to_string(), "整数".to_string()], // 数组句柄, 索引
            "整数",  // 返回值(0/1)
        ));

        json_module.add_function(ModuleFunction::new(
            "数组获取对象",
            "qi_json_array_get_object",
            vec!["整数".to_string(), "整数".to_string()], // 数组句柄, 索引
            "整数",  // 返回对象句柄
        ));

        // 工具函数
        json_module.add_function(ModuleFunction::new(
            "数组长度",
            "qi_json_array_length",
            vec!["整数".to_string()], // 数组句柄
            "整数",  // 返回长度
        ));

        json_module.add_function(ModuleFunction::new(
            "是否包含键",
            "qi_json_has_key",
            vec!["整数".to_string(), "字符串".to_string()], // 对象句柄, 键
            "整数",  // 返回1或0
        ));

        json_module.add_function(ModuleFunction::new(
            "转字符串",
            "qi_json_to_string",
            vec!["整数".to_string()], // 对象或数组句柄
            "字符串",  // 返回JSON字符串
        ));

        json_module.add_function(ModuleFunction::new(
            "格式化",
            "qi_json_to_string_pretty",
            vec!["整数".to_string()], // 对象或数组句柄
            "字符串",  // 返回格式化的JSON字符串
        ));

        // 内存管理
        json_module.add_function(ModuleFunction::new(
            "删除",
            "qi_json_free",
            vec!["整数".to_string()], // JSON对象或数组句柄
            "整数",  // 返回状态
        ));

        // Register module with both Chinese and path formats
        self.modules.insert("JSON".to_string(), json_module.clone());
        self.modules.insert("标准库.JSON".to_string(), json_module);
    }

    /// 注册日期时间模块
    fn register_datetime_module(&mut self) {
        let mut dt_module = Module::new("日期");

        // 当前时间
        dt_module.add_function(ModuleFunction::new("现在", "qi_datetime_now", vec![], "整数"));
        dt_module.add_function(ModuleFunction::new("现在毫秒", "qi_datetime_now_millis", vec![], "整数"));
        dt_module.add_function(ModuleFunction::new("本地时间", "qi_datetime_now_local", vec![], "整数"));

        // 格式化
        dt_module.add_function(ModuleFunction::new("格式化", "qi_datetime_format", vec!["整数".to_string(), "字符串".to_string()], "字符串"));
        dt_module.add_function(ModuleFunction::new("格式化本地", "qi_datetime_format_local", vec!["整数".to_string(), "字符串".to_string()], "字符串"));

        // 解析
        dt_module.add_function(ModuleFunction::new("解析", "qi_datetime_parse", vec!["字符串".to_string(), "字符串".to_string()], "整数"));

        // 日期组件
        dt_module.add_function(ModuleFunction::new("年", "qi_datetime_year", vec!["整数".to_string()], "整数"));
        dt_module.add_function(ModuleFunction::new("月", "qi_datetime_month", vec!["整数".to_string()], "整数"));
        dt_module.add_function(ModuleFunction::new("日", "qi_datetime_day", vec!["整数".to_string()], "整数"));
        dt_module.add_function(ModuleFunction::new("时", "qi_datetime_hour", vec!["整数".to_string()], "整数"));
        dt_module.add_function(ModuleFunction::new("分", "qi_datetime_minute", vec!["整数".to_string()], "整数"));
        dt_module.add_function(ModuleFunction::new("秒", "qi_datetime_second", vec!["整数".to_string()], "整数"));
        dt_module.add_function(ModuleFunction::new("星期几", "qi_datetime_weekday", vec!["整数".to_string()], "整数"));

        // 日期计算
        dt_module.add_function(ModuleFunction::new("加秒", "qi_datetime_add_seconds", vec!["整数".to_string(), "整数".to_string()], "整数"));
        dt_module.add_function(ModuleFunction::new("加分钟", "qi_datetime_add_minutes", vec!["整数".to_string(), "整数".to_string()], "整数"));
        dt_module.add_function(ModuleFunction::new("加小时", "qi_datetime_add_hours", vec!["整数".to_string(), "整数".to_string()], "整数"));
        dt_module.add_function(ModuleFunction::new("加天", "qi_datetime_add_days", vec!["整数".to_string(), "整数".to_string()], "整数"));
        dt_module.add_function(ModuleFunction::new("相差天数", "qi_datetime_diff_days", vec!["整数".to_string(), "整数".to_string()], "整数"));
        dt_module.add_function(ModuleFunction::new("相差小时", "qi_datetime_diff_hours", vec!["整数".to_string(), "整数".to_string()], "整数"));
        dt_module.add_function(ModuleFunction::new("相差分钟", "qi_datetime_diff_minutes", vec!["整数".to_string(), "整数".to_string()], "整数"));
        dt_module.add_function(ModuleFunction::new("相差秒数", "qi_datetime_diff_seconds", vec!["整数".to_string(), "整数".to_string()], "整数"));

        // 日期创建
        dt_module.add_function(ModuleFunction::new("从年月日", "qi_datetime_from_ymd", vec!["整数".to_string(), "整数".to_string(), "整数".to_string()], "整数"));
        dt_module.add_function(ModuleFunction::new("从年月日时分秒", "qi_datetime_from_ymdhms", vec!["整数".to_string(), "整数".to_string(), "整数".to_string(), "整数".to_string(), "整数".to_string(), "整数".to_string()], "整数"));

        // 工具函数
        dt_module.add_function(ModuleFunction::new("是闰年", "qi_datetime_is_leap_year", vec!["整数".to_string()], "整数"));
        dt_module.add_function(ModuleFunction::new("月天数", "qi_datetime_days_in_month", vec!["整数".to_string(), "整数".to_string()], "整数"));

        self.modules.insert("日期".to_string(), dt_module.clone());
        self.modules.insert("标准库.日期".to_string(), dt_module.clone());
        self.modules.insert("时间".to_string(), dt_module);
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
