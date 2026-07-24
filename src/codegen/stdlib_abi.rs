//! Compiler-owned declarations for standard-library runtime ABIs.
//!
//! Implementations live in the canonical `qi-runtime` repository. Keep declarations here so
//! module registration and ABI parity tests consume the same source of truth.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StdlibAbiFunction {
    pub qi_name: &'static str,
    pub runtime_name: &'static str,
    pub param_types: &'static [&'static str],
    pub return_type: &'static str,
    pub c_param_types: &'static [&'static str],
    pub c_return_type: &'static str,
}

macro_rules! llm_abi {
    ($qi:literal, $runtime:literal, [$($param:literal),*], $ret:literal, [$($c_param:literal),*], $c_ret:literal) => {
        StdlibAbiFunction {
            qi_name: $qi,
            runtime_name: $runtime,
            param_types: &[$($param),*],
            return_type: $ret,
            c_param_types: &[$($c_param),*],
            c_return_type: $c_ret,
        }
    };
}

macro_rules! tool_control_abi {
    ($qi:literal, $runtime:literal, [$($param:literal),*], $ret:literal, [$($c_param:literal),*], $c_ret:literal) => {
        StdlibAbiFunction {
            qi_name: $qi,
            runtime_name: $runtime,
            param_types: &[$($param),*],
            return_type: $ret,
            c_param_types: &[$($c_param),*],
            c_return_type: $c_ret,
        }
    };
}

macro_rules! web_runtime_abi {
    ($qi:literal, $runtime:literal, [$($param:literal),*], $ret:literal, [$($c_param:literal),*], $c_ret:literal) => {
        StdlibAbiFunction {
            qi_name: $qi,
            runtime_name: $runtime,
            param_types: &[$($param),*],
            return_type: $ret,
            c_param_types: &[$($c_param),*],
            c_return_type: $c_ret,
        }
    };
}

pub const WEB_RUNTIME_ABI: &[StdlibAbiFunction] = &[web_runtime_abi!(
    "请求主体超过上限",
    "qi_web_request_body_exceeds_limit",
    ["整数", "整数"],
    "整数",
    ["i64", "i64"],
    "i64"
)];

pub const LLM_ABI: &[StdlibAbiFunction] = &[
    llm_abi!(
        "创建会话",
        "qi_llm_create_session",
        ["字符串", "字符串", "字符串"],
        "整数",
        ["*const c_char", "*const c_char", "*const c_char"],
        "i64"
    ),
    llm_abi!(
        "对话",
        "qi_llm_chat",
        ["整数", "字符串"],
        "字符串",
        ["i64", "*const c_char"],
        "*mut c_char"
    ),
    llm_abi!(
        "嵌入",
        "qi_llm_embed",
        ["整数", "字符串"],
        "浮点数组",
        ["i64", "*const c_char"],
        "*mut u8"
    ),
    llm_abi!(
        "对话图像",
        "qi_llm_chat_image",
        ["整数", "字符串", "字符串"],
        "字符串",
        ["i64", "*const c_char", "*const c_char"],
        "*mut c_char"
    ),
    llm_abi!(
        "设置配置",
        "qi_llm_set_config",
        ["整数", "字符串", "字符串"],
        "整数",
        ["i64", "*const c_char", "*const c_char"],
        "i64"
    ),
    llm_abi!(
        "清空历史",
        "qi_llm_clear_history",
        ["整数"],
        "整数",
        ["i64"],
        "i64"
    ),
    llm_abi!(
        "历史数量",
        "qi_llm_get_history_count",
        ["整数"],
        "整数",
        ["i64"],
        "i64"
    ),
    llm_abi!(
        "历史JSON",
        "qi_llm_get_history_json",
        ["整数"],
        "字符串",
        ["i64"],
        "*mut c_char"
    ),
    llm_abi!(
        "设置历史JSON",
        "qi_llm_set_history_json",
        ["整数", "字符串"],
        "整数",
        ["i64", "*const c_char"],
        "i64"
    ),
    llm_abi!(
        "用量",
        "qi_llm_last_usage",
        ["整数"],
        "字符串",
        ["i64"],
        "*mut c_char"
    ),
    llm_abi!(
        "设置预算",
        "qi_llm_set_budget",
        ["整数", "整数"],
        "整数",
        ["i64", "i64"],
        "i64"
    ),
    llm_abi!(
        "已用预算",
        "qi_llm_budget_used",
        ["整数"],
        "整数",
        ["i64"],
        "i64"
    ),
    llm_abi!(
        "关闭会话",
        "qi_llm_close_session",
        ["整数"],
        "整数",
        ["i64"],
        "i64"
    ),
    llm_abi!(
        "异步对话",
        "qi_llm_chat_async",
        ["整数", "字符串"],
        "未来<字符串>",
        ["i64", "*const c_char"],
        "*mut Future"
    ),
    llm_abi!(
        "流式对话",
        "qi_llm_stream_chat",
        ["整数", "字符串"],
        "整数",
        ["i64", "*const c_char"],
        "i64"
    ),
    llm_abi!(
        "打开流V2",
        "qi_llm_stream_v2_open",
        ["整数", "字符串"],
        "整数",
        ["i64", "*const c_char"],
        "i64"
    ),
    llm_abi!(
        "读取流事件V2",
        "qi_llm_stream_v2_next_event",
        ["整数"],
        "字符串",
        ["i64"],
        "*mut c_char"
    ),
    llm_abi!(
        "限时读取流事件V2",
        "qi_llm_stream_v2_next_event_timeout",
        ["整数", "整数"],
        "字符串",
        ["i64", "i64"],
        "*mut c_char"
    ),
    llm_abi!(
        "取消流V2",
        "qi_llm_stream_v2_cancel",
        ["整数", "字符串"],
        "整数",
        ["i64", "*const c_char"],
        "i64"
    ),
    llm_abi!(
        "流快照V2",
        "qi_llm_stream_v2_snapshot",
        ["整数"],
        "字符串",
        ["i64"],
        "*mut c_char"
    ),
    llm_abi!(
        "提交流V2",
        "qi_llm_stream_v2_commit",
        ["整数", "整数"],
        "整数",
        ["i64", "i64"],
        "i64"
    ),
    llm_abi!(
        "放弃流V2",
        "qi_llm_stream_v2_abort",
        ["整数"],
        "整数",
        ["i64"],
        "i64"
    ),
    llm_abi!(
        "关闭流V2",
        "qi_llm_stream_v2_close",
        ["整数"],
        "整数",
        ["i64"],
        "i64"
    ),
    llm_abi!(
        "历史版本",
        "qi_llm_history_revision",
        ["整数"],
        "整数",
        ["i64"],
        "i64"
    ),
    llm_abi!(
        "能力版本",
        "qi_llm_runtime_capability",
        ["字符串"],
        "整数",
        ["*const c_char"],
        "i64"
    ),
    llm_abi!(
        "读取流",
        "qi_llm_stream_next",
        ["整数"],
        "字符串",
        ["i64"],
        "*mut c_char"
    ),
    llm_abi!(
        "关闭流",
        "qi_llm_stream_close",
        ["整数"],
        "整数",
        ["i64"],
        "i64"
    ),
    llm_abi!(
        "流式工具对话",
        "qi_llm_stream_chat_with_tools",
        ["整数", "字符串"],
        "整数",
        ["i64", "*const c_char"],
        "i64"
    ),
    llm_abi!(
        "流式继续工具对话",
        "qi_llm_stream_continue_with_tools",
        ["整数"],
        "整数",
        ["i64"],
        "i64"
    ),
    llm_abi!(
        "流取助手消息",
        "qi_llm_stream_assistant_message",
        ["整数"],
        "字符串",
        ["i64"],
        "*mut c_char"
    ),
    llm_abi!(
        "注册工具",
        "qi_llm_register_tool",
        ["整数", "字符串", "字符串", "字符串"],
        "整数",
        ["i64", "*const c_char", "*const c_char", "*const c_char"],
        "i64"
    ),
    llm_abi!(
        "清空工具",
        "qi_llm_clear_tools",
        ["整数"],
        "整数",
        ["i64"],
        "i64"
    ),
    llm_abi!(
        "工具对话",
        "qi_llm_chat_with_tools",
        ["整数", "字符串"],
        "字符串",
        ["i64", "*const c_char"],
        "*mut c_char"
    ),
    llm_abi!(
        "继续工具对话",
        "qi_llm_continue_with_tools",
        ["整数"],
        "字符串",
        ["i64"],
        "*mut c_char"
    ),
    llm_abi!(
        "有工具调用",
        "qi_llm_has_tool_call",
        ["字符串"],
        "整数",
        ["*const c_char"],
        "i64"
    ),
    llm_abi!(
        "工具调用ID",
        "qi_llm_get_tool_call_id",
        ["字符串"],
        "字符串",
        ["*const c_char"],
        "*mut c_char"
    ),
    llm_abi!(
        "工具调用名称",
        "qi_llm_get_tool_call_name",
        ["整数", "字符串"],
        "字符串",
        ["i64", "*const c_char"],
        "*mut c_char"
    ),
    llm_abi!(
        "工具调用参数",
        "qi_llm_get_tool_call_arguments",
        ["字符串"],
        "字符串",
        ["*const c_char"],
        "*mut c_char"
    ),
    llm_abi!(
        "工具调用数量",
        "qi_llm_get_tool_call_count",
        ["字符串"],
        "整数",
        ["*const c_char"],
        "i64"
    ),
    llm_abi!(
        "工具调用ID索引",
        "qi_llm_get_tool_call_id_at",
        ["字符串", "整数"],
        "字符串",
        ["*const c_char", "i64"],
        "*mut c_char"
    ),
    llm_abi!(
        "工具调用名称索引",
        "qi_llm_get_tool_call_name_at",
        ["整数", "字符串", "整数"],
        "字符串",
        ["i64", "*const c_char", "i64"],
        "*mut c_char"
    ),
    llm_abi!(
        "工具调用参数索引",
        "qi_llm_get_tool_call_arguments_at",
        ["字符串", "整数"],
        "字符串",
        ["*const c_char", "i64"],
        "*mut c_char"
    ),
    llm_abi!(
        "添加工具结果",
        "qi_llm_add_tool_result",
        ["整数", "字符串", "字符串", "字符串"],
        "整数",
        ["i64", "*const c_char", "*const c_char", "*const c_char"],
        "i64"
    ),
];

pub const TOOL_CONTROL_ABI: &[StdlibAbiFunction] = &[
    tool_control_abi!(
        "创建",
        "qi_tool_control_create",
        ["整数", "整数"],
        "整数",
        ["i64", "i64"],
        "i64"
    ),
    tool_control_abi!(
        "释放控制",
        "qi_tool_control_release",
        ["整数"],
        "整数",
        ["i64"],
        "i32"
    ),
    tool_control_abi!(
        "取消",
        "qi_tool_control_cancel",
        ["整数", "字符串"],
        "整数",
        ["i64", "*const c_char"],
        "i32"
    ),
    tool_control_abi!(
        "是否已取消",
        "qi_tool_control_is_cancelled",
        ["整数"],
        "整数",
        ["i64"],
        "i32"
    ),
    tool_control_abi!(
        "取消原因",
        "qi_tool_control_cancel_reason",
        ["整数"],
        "字符串",
        ["i64"],
        "*mut c_char"
    ),
    tool_control_abi!(
        "剩余毫秒",
        "qi_tool_control_remaining_ms",
        ["整数"],
        "整数",
        ["i64"],
        "i64"
    ),
    tool_control_abi!(
        "发送进度",
        "qi_tool_control_progress_push",
        ["整数", "字符串"],
        "整数",
        ["i64", "*const c_char"],
        "i32"
    ),
    tool_control_abi!(
        "下一进度",
        "qi_tool_control_progress_pop",
        ["整数"],
        "字符串",
        ["i64"],
        "*mut c_char"
    ),
    tool_control_abi!(
        "已丢弃进度",
        "qi_tool_control_progress_dropped",
        ["整数"],
        "整数",
        ["i64"],
        "i64"
    ),
    tool_control_abi!(
        "完成",
        "qi_tool_control_finish",
        ["整数", "整数", "字符串"],
        "整数",
        ["i64", "i64", "*const c_char"],
        "i32"
    ),
    tool_control_abi!(
        "是否已完成",
        "qi_tool_control_is_finished",
        ["整数"],
        "整数",
        ["i64"],
        "i32"
    ),
    tool_control_abi!(
        "完成代码",
        "qi_tool_control_finish_code",
        ["整数"],
        "整数",
        ["i64"],
        "i64"
    ),
    tool_control_abi!(
        "完成结果",
        "qi_tool_control_finish_result",
        ["整数"],
        "字符串",
        ["i64"],
        "*mut c_char"
    ),
    tool_control_abi!(
        "等待状态",
        "qi_tool_control_wait",
        ["整数", "整数"],
        "整数",
        ["i64", "i64"],
        "i32"
    ),
    tool_control_abi!(
        "释放字符串",
        "qi_tool_control_free_string",
        ["字符串"],
        "空",
        ["*mut c_char"],
        "()"
    ),
];
