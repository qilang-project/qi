好的，设计语言服务器协议 (LSP) 和代码高亮支持是构建现代编程语言工具链的关键一步。这将极大提升 Qi 语言在 VS Code、Sublime Text、Vim/Neovim 和 JetBrains IDEs 等主流编辑器中的开发体验。

我们将设计两个核心组件：

1.  **`qi-lsp`**：一个语言服务器，负责提供“智能”支持（如自动补全、错误检查）。
2.  **TextMate 语法**：一个 `.tmLanguage.json` 文件，负责提供“静态”的语法高亮。

---

### 1\. 语言服务器协议 (LSP) 设计 (qi-lsp)

`qi-lsp` 将是一个独立的二进制文件，它在后台运行，并通过标准输入/输出 (stdin/stdout) 与编辑器插件（如 VS Code 扩展）进行 JSON-RPC 通信。

#### 1.1 核心架构

`qi-lsp` 将重用 `compiler`（编译器） crate 中的词法分析器 (Lexer) 和语法分析器 (Parser)。它不会执行完整的代码生成，而是专注于生成和查询抽象语法树 (AST) 和语义信息。

1.  **文档管理器 (DocumentManager)**：在内存中维护所有打开的 `.qi` 文件的内容。当编辑器发送 `textDocument/didChange` 通知时，它会更新对应文件的内容。
2.  **编译器前端集成**：当需要分析时（例如在保存或更改后），`qi-lsp` 会调用编译器前端对文档内容进行词法分析和语法分析，生成 AST。
3.  **语义分析器 (SemanticAnalyzer)**：(可选的，更高级的特性) 遍历 AST，解析类型、变量作用域和引用，构建一个符号表（Symbol Table）。

#### 1.2 支持的核心 LSP 功能

我们将根据 Qi 语言的特性，实现以下关键 LSP 功能：

##### a. 诊断 (Diagnostics - `textDocument/publishDiagnostics`)

- **触发时机**：文件打开时 (`didOpen`)、内容更改时 (`didChange`)、保存时 (`didSave`)。
- **实现**：
  1.  调用 `compiler::lexer` 和 `compiler::parser` 分析文档内容。
  2.  捕获词法错误（例如无效字符）和语法错误（例如缺少分号、关键字错用）。
  3.  将这些错误转换为 LSP `Diagnostic` 对象（包含错误消息、代码范围 `Range` 和严重级别 `Severity`），并通过 `publishDiagnostics` 通知发送给客户端。

##### b. 代码补全 (Completion - `textDocument/completion`)

- **触发时机**：用户输入触发字符（如 `.`、`:`）或手动触发（Ctrl+Space）。
- **实现**：
  - **关键字补全**：提供完整的 Qi 关键字列表（例如 `函数`, `结构体`, `如果`, `对于` 等）。
  - **变量/函数补全**：(需要语义分析) 分析当前光标位置的作用域，从符号表中查找可见的变量、函数和类型，并提供补全。
  - **成员访问补全**：当用户输入 `实例.` 时，分析 `实例` 的类型（必须是 `结构体`），并列出该结构体的所有字段和方法（`实现` 块中的函数）。
  - **包/模块补全**：当用户输入 `导入 ` 时，查找项目目录下的其他 `.qi` 文件或标准库模块。

##### c. 悬停提示 (Hover - `textDocument/hover`)

- **触发时机**：鼠标悬停在代码中的某个符号上。
- **实现**：
  1.  (需要语义分析) 根据光标位置在 AST 和符号表中查找对应的符号。
  2.  返回该符号的详细信息：
      - **变量/不可变**：`变量 名称: 类型`
      - **函数**：`函数 签名(参数: 类型) -> 返回类型`
      - **结构体/枚举**：`结构体 名称 { ... }`
      - **关键字**：显示该关键字的简短说明。

##### d. 定义跳转 (Go to Definition - `textDocument/definition`)

- **触发时机**：用户在符号上按下 F12 或 Ctrl+点击。
- **实现**：
  1.  (需要语义分析) 查找光标下符号的定义位置。
  2.  例如，如果光标在函数调用 `计算总和()` 上，LSP 将返回 `函数 整数 计算总和(...) { ... }` 定义的位置（文件 URI 和代码范围 `Range`）。
  3.  如果光标在变量 `计数器` 上，LSP 将返回 `变量 计数器 = 0;` 的位置。

##### e. 查找引用 (Find References - `textDocument/references`)

- **触发时机**：用户在符号上右键菜单选择“查找所有引用”。
- **实现**：
  1.  (需要高级索引) 遍历所有项目文件（或缓存的 AST），查找所有使用该符号（变量、函数、结构体）的位置。
  2.  返回一个位置列表（`Location[]`）。

---

### 2\. 语法高亮 (TextMate 语法)

语法高亮是通过一个 JSON 文件（通常是 `.tmLanguage.json`）来实现的，它使用正则表达式来匹配代码中的不同部分，并为它们分配“作用域 (scopes)”。编辑器主题 (Theme) 再根据这些作用域来应用颜色。

这是 Qi 语言 (`qi.tmLanguage.json`) 的一个基础设计：

```json
{
  "$schema": "https://raw.githubusercontent.com/martinring/tmlanguage/master/tmlanguage.json",
  "name": "Qi",
  "scopeName": "source.qi",
  "patterns": [
    { "include": "#comments" },
    { "include": "#keywords" },
    { "include": "#storage-types" },
    { "include": "#operators" },
    { "include": "#strings" },
    { "include": "#numbers" },
    { "include": "#entities" },
    { "include": "#punctuation" },
    { "include": "#constants" }
  ],
  "repository": {
    "comments": {
      "patterns": [
        {
          "name": "comment.line.double-slash.qi",
          "match": "//.*"
        },
        {
          "name": "comment.block.qi",
          "begin": "/\\*",
          "end": "\\*/"
        }
      ]
    },
    "keywords": {
      "patterns": [
        {
          "name": "keyword.control.qi",
          "match": "\\b(如果|否则|匹配|循环|当|对于|中断|继续|返回|跳转|异步|等待|抛出|捕获|尝试)\\b"
        },
        {
          "name": "keyword.declaration.qi",
          "match": "\\b(函数|结构体|枚举|联合体|特性|实现|类型|常量|静态|包|导入|公开|私有|作为|内联)\\b"
        },
        {
          "name": "keyword.memory.qi",
          "match": "\\b(拥有|借用|移动|克隆|释放|新建|自我|自身|指针|引用|可变引用)\\b"
        },
        {
          "name": "keyword.operator.chinese.qi",
          "match": "\\b(加|减|乘|除|取余|等于|不等于|大于|小于|大于等于|小于等于|与|或|非)\\b"
        }
      ]
    },
    "storage-types": {
      "patterns": [
        {
          "name": "storage.type.primitive.qi",
          "match": "\\b(整数|长整数|短整数|字节|浮点数|布尔|字符|字符串|空|结果|选项|数组|字典|列表|集合)\\b"
        },
        {
          "name": "storage.modifier.qi",
          "match": "\\b(变量|不可变)\\b"
        }
      ]
    },
    "operators": {
      "patterns": [
        {
          "name": "keyword.operator.assignment.qi",
          "match": "="
        },
        {
          "name": "keyword.operator.arrow.qi",
          "match": "->"
        },
        {
          "name": "keyword.operator.arithmetic.qi",
          "match": "(\\+|-|\\*|/|%)"
        },
        {
          "name": "keyword.operator.comparison.qi",
          "match": "(==|!=|<=|>=|<|>)"
        },
        {
          "name": "keyword.operator.logical.qi",
          "match": "(&&|\\|\\||!)"
        }
      ]
    },
    "strings": {
      "name": "string.quoted.double.qi",
      "begin": "\"",
      "end": "\"",
      "patterns": [
        {
          "name": "constant.character.escape.qi",
          "match": "\\\\."
        },
        {
          "name": "variable.interpolation.qi",
          "match": "\\{\\}"
        }
      ]
    },
    "numbers": {
      "patterns": [
        {
          "name": "constant.numeric.float.qi",
          "match": "\\b[0-9]+\\.[0-9]+\\b"
        },
        {
          "name": "constant.numeric.integer.qi",
          "match": "\\b[0-9]+\\b"
        }
      ]
    },
    "entities": {
      "patterns": [
        {
          "name": "entity.name.function.qi",
          "match": "\\b(函数|异步 函数)\\s+([a-zA-Z_\\u4e00-\\u9fa5][a-zA-Z0-9_\\u4e00-\\u9fa5]*)"
        },
        {
          "name": "entity.name.type.struct.qi",
          "match": "\\b(结构体|枚举|特性)\\s+([a-zA-Z_\\u4e00-\\u9fa5][a-zA-Z0-9_\\u4e00-\\u9fa5]*)"
        }
      ]
    },
    "punctuation": {
      "patterns": [
        {
          "name": "punctuation.terminator.statement.qi",
          "match": ";"
        },
        {
          "name": "punctuation.separator.comma.qi",
          "match": ","
        },
        {
          "name": "punctuation.accessor.qi",
          "match": "\\."
        },
        {
          "name": "punctuation.definition.generic.begin.qi",
          "match": "<"
        },
        {
          "name": "punctuation.definition.generic.end.qi",
          "match": ">"
        }
      ]
    },
    "constants": {
      "patterns": [
        {
          "name": "constant.language.boolean.qi",
          "match": "\\b(真|假)\\b"
        },
        {
          "name": "constant.language.null.qi",
          "match": "\\b(空指针)\\b"
        },
        {
          "name": "support.function.builtin.qi",
          "match": "\\b(打印|输入|长度|包含)\\b"
        }
      ]
    }
  }
}
```

### 3\. 如何集成 (项目结构)

基于我们之前的项目结构，`qi-lsp` 和 VS Code 扩展将这样组织：

```
qi/
├── ... (compiler, runtime, etc.)
│
├── tools/
│   ├── qi-lsp/             # 语言服务器 (Rust Crate)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs     # LSP 主循环和 JSON-RPC 通信
│   │       ├── server.rs   # 实现 LSP 功能 (e.g., on_completion, on_hover)
│   │       └── analysis.rs # 与 compiler crate 交互, 管理 AST
│   │
│   └── vscode-qi/          # VS Code 扩展
│       ├── package.json    # 扩展定义
│       ├── client/         # 客户端 (TypeScript)
│       │   └── src/
│       │       └── extension.ts # 启动和管理 LSP 客户端
│       └── server/         # (空, 我们使用外部的 qi-lsp 二进制文件)
│       └── syntaxes/
│           └── qi.tmLanguage.json # 语法高亮文件 (上面设计的)
│
└── ... (stdlib, tests, etc.)
```

这个设计为 Qi 语言提供了从基础高亮到智能代码辅助的完整 IDE 支持。
