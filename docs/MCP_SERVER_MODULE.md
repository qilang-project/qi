# MCP 服务器模块技术文档

## 概述

MCP (Model Context Protocol) 服务器模块为 Qi 语言提供了完整的 MCP 协议支持，允许开发者创建符合 MCP 规范的服务器，为 AI 应用提供工具、资源和提示。

**协议版本**: 2025-06-18
**模块位置**: `src/runtime/stdlib/mcp.rs`
**FFI 接口**: `src/runtime/stdlib/mcp_ffi.rs`

## 架构设计

### 核心组件

```
┌─────────────────────────────────────────┐
│          Qi 语言应用层                  │
│  (示例/标准库/MCP服务器/*.qi)          │
└───────────────┬─────────────────────────┘
                │
                ▼
┌─────────────────────────────────────────┐
│         FFI 接口层                       │
│  (src/runtime/stdlib/mcp_ffi.rs)       │
│  - qi_mcp_create_server()               │
│  - qi_mcp_register_tool()               │
│  - qi_mcp_register_resource()           │
│  - qi_mcp_register_prompt()             │
│  - ...                                   │
└───────────────┬─────────────────────────┘
                │
                ▼
┌─────────────────────────────────────────┐
│         Rust 实现层                      │
│  (src/runtime/stdlib/mcp.rs)           │
│                                          │
│  ┌────────────────────────────────┐    │
│  │      MCP服务器模块             │    │
│  └────────────────────────────────┘    │
│                                          │
│  ┌────────────────────────────────┐    │
│  │      MCP服务器                 │    │
│  │  - 工具表 (HashMap)            │    │
│  │  - 资源表 (HashMap)            │    │
│  │  - 提示表 (HashMap)            │    │
│  │  - 配置信息                     │    │
│  └────────────────────────────────┘    │
│                                          │
│  ┌──────┐  ┌──────┐  ┌──────┐         │
│  │ 工具 │  │ 资源 │  │ 提示 │         │
│  └──────┘  └──────┘  └──────┘         │
└─────────────────────────────────────────┘
```

### 数据结构

#### MCP服务器 (`MCP服务器`)

主要的服务器结构，管理所有注册的能力：

```rust
pub struct MCP服务器 {
    配置: MCP服务器配置,
    工具表: HashMap<String, MCP工具>,
    资源表: HashMap<String, MCP资源>,
    提示表: HashMap<String, MCP提示>,
    运行中: bool,
}
```

#### MCP工具 (`MCP工具`)

表示一个可执行的工具：

```rust
pub struct MCP工具 {
    名称: String,
    描述: String,
    参数列表: Vec<工具参数>,
    执行函数: Option<fn(&HashMap<String, JsonValue>) -> MCP结果<JsonValue>>,
}
```

**工具参数** (`工具参数`):
```rust
pub struct 工具参数 {
    名称: String,
    类型: String,  // "string", "number", "boolean", "object", "array"
    描述: String,
    必需: bool,
    默认值: Option<JsonValue>,
}
```

#### MCP资源 (`MCP资源`)

表示一个可访问的资源：

```rust
pub struct MCP资源 {
    uri: String,
    名称: String,
    描述: String,
    类型: 资源类型,
    mime类型: Option<String>,
}
```

**资源类型** (`资源类型`):
```rust
pub enum 资源类型 {
    文本,    // Text resource
    二进制,  // Binary resource
    JSON,    // JSON resource
}
```

#### MCP提示 (`MCP提示`)

表示一个提示模板：

```rust
pub struct MCP提示 {
    名称: String,
    描述: String,
    参数列表: Vec<提示参数>,
    模板: String,  // 使用 {变量名} 作为占位符
}
```

**提示参数** (`提示参数`):
```rust
pub struct 提示参数 {
    名称: String,
    描述: String,
    必需: bool,
}
```

## FFI 接口

### 服务器管理

#### qi_mcp_create_server
创建新的 MCP 服务器。

**签名**:
```c
int64_t qi_mcp_create_server(
    const char* name,
    const char* version,
    const char* description
);
```

**参数**:
- `name`: 服务器名称
- `version`: 服务器版本
- `description`: 服务器描述 (可为空)

**返回值**: 服务器ID (>0 成功, -1 失败)

#### qi_mcp_start_server
启动 MCP 服务器。

**签名**:
```c
int32_t qi_mcp_start_server(int64_t server_id);
```

**返回值**: 0 成功, -1 失败

#### qi_mcp_stop_server
停止 MCP 服务器。

**签名**:
```c
int32_t qi_mcp_stop_server(int64_t server_id);
```

**返回值**: 0 成功, -1 失败

#### qi_mcp_is_running
检查服务器运行状态。

**签名**:
```c
int32_t qi_mcp_is_running(int64_t server_id);
```

**返回值**: 1 运行中, 0 未运行, -1 失败

#### qi_mcp_destroy_server
销毁 MCP 服务器，释放资源。

**签名**:
```c
int32_t qi_mcp_destroy_server(int64_t server_id);
```

**返回值**: 0 成功, -1 失败

### 工具管理

#### qi_mcp_register_tool
注册工具到服务器。

**签名**:
```c
int32_t qi_mcp_register_tool(
    int64_t server_id,
    const char* tool_name,
    const char* tool_description
);
```

**返回值**: 0 成功, -1 失败

#### qi_mcp_call_tool
执行已注册的工具。

**签名**:
```c
char* qi_mcp_call_tool(
    int64_t server_id,
    const char* tool_name,
    const char* params_json
);
```

**参数**:
- `params_json`: 参数的 JSON 字符串

**返回值**: 执行结果 JSON (需要用 qi_mcp_free_string 释放), NULL 失败

#### qi_mcp_list_tools
获取所有已注册工具的列表。

**签名**:
```c
char* qi_mcp_list_tools(int64_t server_id);
```

**返回值**: 工具列表 JSON (需要用 qi_mcp_free_string 释放), NULL 失败

**返回格式**:
```json
[
  {
    "name": "工具名称",
    "description": "工具描述",
    "inputSchema": {
      "type": "object",
      "properties": {
        "参数名": {
          "type": "string",
          "description": "参数描述"
        }
      },
      "required": ["必需参数名"]
    }
  }
]
```

### 资源管理

#### qi_mcp_register_resource
注册资源到服务器。

**签名**:
```c
int32_t qi_mcp_register_resource(
    int64_t server_id,
    const char* resource_uri,
    const char* resource_name,
    const char* resource_description,
    int32_t resource_type
);
```

**参数**:
- `resource_type`: 0=文本, 1=二进制, 2=JSON

**返回值**: 0 成功, -1 失败

#### qi_mcp_list_resources
获取所有已注册资源的列表。

**签名**:
```c
char* qi_mcp_list_resources(int64_t server_id);
```

**返回值**: 资源列表 JSON (需要用 qi_mcp_free_string 释放), NULL 失败

**返回格式**:
```json
[
  {
    "uri": "file:///path/to/resource",
    "name": "资源名称",
    "description": "资源描述",
    "type": "text",
    "mimeType": "text/plain"
  }
]
```

### 提示管理

#### qi_mcp_register_prompt
注册提示模板到服务器。

**签名**:
```c
int32_t qi_mcp_register_prompt(
    int64_t server_id,
    const char* prompt_name,
    const char* prompt_description,
    const char* prompt_template
);
```

**参数**:
- `prompt_template`: 模板内容，使用 `{变量名}` 作为占位符

**返回值**: 0 成功, -1 失败

#### qi_mcp_get_prompt
填充提示模板并返回结果。

**签名**:
```c
char* qi_mcp_get_prompt(
    int64_t server_id,
    const char* prompt_name,
    const char* params_json
);
```

**参数**:
- `params_json`: 参数的 JSON 字符串 (键值对)

**返回值**: 填充后的提示文本 (需要用 qi_mcp_free_string 释放), NULL 失败

#### qi_mcp_list_prompts
获取所有已注册提示的列表。

**签名**:
```c
char* qi_mcp_list_prompts(int64_t server_id);
```

**返回值**: 提示列表 JSON (需要用 qi_mcp_free_string 释放), NULL 失败

**返回格式**:
```json
[
  {
    "name": "提示名称",
    "description": "提示描述",
    "arguments": [
      {
        "name": "参数名",
        "description": "参数描述",
        "required": true
      }
    ]
  }
]
```

### 信息查询

#### qi_mcp_get_server_info
获取服务器信息。

**签名**:
```c
char* qi_mcp_get_server_info(int64_t server_id);
```

**返回值**: 服务器信息 JSON (需要用 qi_mcp_free_string 释放), NULL 失败

**返回格式**:
```json
{
  "name": "服务器名称",
  "version": "1.0.0",
  "protocolVersion": "2025-06-18",
  "description": "服务器描述",
  "capabilities": {
    "tools": true,
    "resources": true,
    "prompts": true
  }
}
```

### 内存管理

#### qi_mcp_free_string
释放由 MCP FFI 函数返回的字符串。

**签名**:
```c
void qi_mcp_free_string(char* s);
```

**注意**: 必须释放所有从 FFI 函数返回的字符串指针，否则会造成内存泄漏。

## 使用示例

### 基础服务器创建

```qi
// 创建服务器
变量 服务器ID: 整数 = 标准库.MCP服务器.创建服务器(
    "我的服务器",
    "1.0.0",
    "示例MCP服务器"
);

// 注册工具
标准库.MCP服务器.注册工具(
    服务器ID,
    "计算器",
    "执行数学计算"
);

// 启动服务器
标准库.MCP服务器.启动服务器(服务器ID);

// 获取服务器信息
变量 信息: 字符串 = 标准库.MCP服务器.获取服务器信息(服务器ID);
打印行(信息);

// 清理
标准库.MCP服务器.销毁服务器(服务器ID);
```

### 完整示例

参见 `示例/标准库/MCP服务器/` 目录下的示例文件。

## 实现细节

### 服务器池管理

使用全局静态变量管理服务器实例：

```rust
static MCP服务器池: OnceLock<Mutex<HashMap<i64, MCP服务器>>> = OnceLock::new();
static 服务器计数器: OnceLock<Mutex<i64>> = OnceLock::new();
```

- **线程安全**: 使用 `Mutex` 保护并发访问
- **ID 分配**: 使用递增计数器分配唯一服务器ID
- **生命周期**: 服务器实例存储在池中直到显式销毁

### JSON 序列化

所有工具、资源和提示的元数据都序列化为 JSON 格式，符合 MCP 协议规范。

**工具 Schema**:
```rust
pub fn 转为Schema(&self) -> JsonValue {
    serde_json::json!({
        "name": self.名称,
        "description": self.描述,
        "inputSchema": {
            "type": "object",
            "properties": properties,
            "required": required
        }
    })
}
```

### 错误处理

使用 `thiserror` 库定义错误类型：

```rust
#[derive(Debug, thiserror::Error)]
pub enum MCP错误 {
    #[error("服务器错误: {0}")]
    服务器错误(String),
    #[error("工具错误: {0}")]
    工具错误(String),
    #[error("资源错误: {0}")]
    资源错误(String),
    #[error("提示错误: {0}")]
    提示错误(String),
    // ...
}
```

## 测试

### 单元测试

位置: `src/runtime/stdlib/mcp.rs`

运行测试:
```bash
cargo test --lib runtime::stdlib::mcp::
```

测试覆盖:
- ✅ 工具创建和 Schema 生成
- ✅ 资源创建和 JSON 序列化
- ✅ 提示创建和模板填充
- ✅ MCP 服务器生命周期
- ✅ 服务器信息查询

### FFI 测试

位置: `src/runtime/stdlib/mcp_ffi.rs`

运行测试:
```bash
cargo test --lib runtime::stdlib::mcp_ffi::
```

测试覆盖:
- ✅ 服务器创建和销毁
- ✅ 工具注册
- ✅ 服务器启动和停止
- ✅ 运行状态查询

### 集成测试

运行示例程序:
```bash
cargo run -- run 示例/标准库/MCP服务器/基础示例.qi
cargo run -- run 示例/标准库/MCP服务器/工具列表示例.qi
cargo run -- run 示例/标准库/MCP服务器/资源管理示例.qi
cargo run -- run 示例/标准库/MCP服务器/提示模板示例.qi
```

## 性能考虑

### 内存使用
- 每个服务器实例约占用 1-2 KB 基础内存
- 每个工具/资源/提示约占用 100-500 字节
- 服务器池使用 HashMap，查找复杂度 O(1)

### 并发性能
- 服务器池使用 Mutex 保护，适合低并发场景
- 对于高并发场景，可考虑使用 RwLock 或无锁数据结构

### 优化建议
1. **批量注册**: 一次性注册多个工具/资源/提示
2. **缓存 JSON**: 缓存序列化后的 JSON 字符串
3. **延迟初始化**: 按需创建服务器实例

## 协议兼容性

### MCP 协议版本
当前实现基于 **MCP 2025-06-18** 规范。

### 已实现功能
- ✅ 服务器信息 (Server Info)
- ✅ 工具注册 (Tool Registration)
- ✅ 资源注册 (Resource Registration)
- ✅ 提示模板 (Prompt Templates)
- ✅ JSON-RPC 2.0 数据格式

### 待实现功能
- ⏳ 工具执行回调 (Tool Execution Callbacks)
- ⏳ 资源内容读取 (Resource Content Reading)
- ⏳ 采样支持 (Sampling)
- ⏳ 传输层实现 (stdio, HTTP+SSE)

## 安全考虑

### 输入验证
- 所有 FFI 函数都验证指针非空
- JSON 解析使用 `serde_json` 防止注入攻击
- 字符串长度没有硬限制，应在应用层控制

### 内存安全
- 使用 Rust 的所有权系统保证内存安全
- FFI 返回的字符串必须通过 `qi_mcp_free_string` 释放
- 服务器销毁时自动释放所有关联资源

### 并发安全
- 使用 Mutex 保护共享状态
- 避免死锁：Mutex 持有时间短暂
- 无竞态条件：ID 分配使用原子操作模式

## 故障排除

### 常见问题

**Q: 服务器创建返回 -1**
- 检查参数是否为空指针
- 检查内存是否充足

**Q: 工具注册失败**
- 检查工具名称是否重复
- 检查服务器ID是否有效

**Q: JSON 解析失败**
- 检查 JSON 格式是否正确
- 使用 JSON 验证工具检查语法

**Q: 内存泄漏**
- 确保调用 `qi_mcp_free_string` 释放字符串
- 确保调用 `qi_mcp_destroy_server` 销毁服务器

### 调试技巧

1. **启用日志**: 设置 `RUST_LOG=debug` 环境变量
2. **检查返回值**: 所有 FFI 函数都有明确的返回值语义
3. **使用测试**: 运行单元测试和集成测试验证功能

## 未来扩展

### 短期目标
- [ ] 实现工具执行回调机制
- [ ] 添加资源内容读取功能
- [ ] 支持工具参数动态添加

### 中期目标
- [ ] 实现完整的 JSON-RPC 2.0 协议
- [ ] 添加传输层 (stdio, HTTP+SSE)
- [ ] 支持采样 (Sampling)

### 长期目标
- [ ] 支持流式响应
- [ ] 添加认证和授权机制
- [ ] 实现服务发现和注册中心

## 参考资料

- [MCP 官方规范](https://modelcontextprotocol.io/specification/latest)
- [MCP GitHub](https://github.com/modelcontextprotocol/modelcontextprotocol)
- [JSON-RPC 2.0 规范](https://www.jsonrpc.org/specification)
- [Qi 语言文档](../README.md)

## 贡献指南

欢迎贡献！请遵循以下步骤：

1. Fork 仓库
2. 创建功能分支
3. 编写测试
4. 提交 Pull Request

代码风格：
- 使用中文命名约定 (`#![allow(non_snake_case)]`)
- 添加详细的文档注释
- 确保所有测试通过

## 许可证

MIT License

---

**文档版本**: 1.0.0
**最后更新**: 2025-11-17
**维护者**: Qi Language Team
