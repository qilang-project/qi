//! Parsing error handling for Qi language
//! 解析器错误处理和恢复机制

use crate::lexer::{Token, TokenKind, Span};
use crate::parser::ast::BasicType;
use crate::utils::diagnostics::{DiagnosticManager, DiagnosticLevel};

/// Parsing errors
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    /// Unexpected token
    #[error("意外的标记: {0:?} 在第 {1} 行第 {2} 列")]
    UnexpectedToken(crate::lexer::TokenKind, usize, usize),

    /// Expected token
    #[error("期望标记 {0:?} 但找到 {1:?} 在第 {2} 行第 {3} 列")]
    ExpectedToken(crate::lexer::TokenKind, crate::lexer::TokenKind, usize, usize),

    /// Invalid syntax
    #[error("语法错误: {0} 在第 {1} 行第 {2} 列")]
    InvalidSyntax(String, usize, usize),

    /// Unterminated expression
    #[error("未终止的表达式在第 {0} 行第 {1} 列")]
    UnterminatedExpression(usize, usize),

    /// Invalid function declaration
    #[error("无效的函数声明在第 {0} 行第 {1} 列")]
    InvalidFunctionDeclaration(usize, usize),

    /// Invalid variable declaration
    #[error("无效的变量声明在第 {0} 行第 {1} 列")]
    InvalidVariableDeclaration(usize, usize),

    /// Unexpected end of input
    #[error("意外的输入结束")]
    UnexpectedEof,

    /// Generic parsing error
    #[error("解析错误: {0}")]
    General(String),

    /// Parse failed
    #[error("解析失败")]
    ParseFailed,
}

impl ParseError {
    pub fn with_span(mut self, span: Span, source: &str) -> Self {
        // Calculate line and column from span
        let (line, column) = self.calculate_position(span.start, source);
        match self {
            ParseError::UnexpectedToken(_, ref mut l, ref mut c) => {
                *l = line;
                *c = column;
            }
            ParseError::ExpectedToken(_, _, ref mut l, ref mut c) => {
                *l = line;
                *c = column;
            }
            ParseError::InvalidSyntax(_, ref mut l, ref mut c) => {
                *l = line;
                *c = column;
            }
            ParseError::UnterminatedExpression(ref mut l, ref mut c) => {
                *l = line;
                *c = column;
            }
            ParseError::InvalidFunctionDeclaration(ref mut l, ref mut c) => {
                *l = line;
                *c = column;
            }
            ParseError::InvalidVariableDeclaration(ref mut l, ref mut c) => {
                *l = line;
                *c = column;
            }
            _ => {}
        }
        self
    }

    fn calculate_position(&self, offset: usize, source: &str) -> (usize, usize) {
        let mut line = 1;
        let mut column = 1;

        for (i, c) in source.chars().enumerate() {
            if i == offset {
                break;
            }
            if c == '\n' {
                line += 1;
                column = 1;
            } else {
                column += 1;
            }
        }

        (line, column)
    }
}

// ===== Enhanced Error Recovery | 增强错误恢复 =====

/// Parser error recovery strategies
/// 解析器错误恢复策略
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryStrategy {
    /// Skip to next semicolon
    /// 跳到下一个分号
    SkipToSemicolon,
    /// Skip to next line
    /// 跳到下一行
    SkipToNextLine,
    /// Skip to next token kind (removed since TokenKind is now Copy)
    /// 跳到下一个特定类型的token (已移除，因为TokenKind现在是Copy类型)
    SkipToNextToken,
    /// Skip to matching bracket
    /// 跳到匹配的括号
    SkipToMatchingBracket,
    /// Skip to next statement
    /// 跳到下一个语句
    SkipToNextStatement,
    /// Panic mode - skip until synchronization point
    /// 恐慌模式 - 跳过直到同步点
    PanicMode,
}

/// Parser error context information
/// 解析器错误上下文信息
#[derive(Debug, Clone)]
pub struct ErrorContext {
    /// Expected token kinds
    /// 期望的token类型
    pub expected: Vec<TokenKind>,
    /// Actual token received
    /// 实际收到的token
    pub actual: Token,
    /// Current parsing rule/function
    /// 当前解析规则/函数
    pub rule: String,
    /// Additional context information
    /// 额外的上下文信息
    pub context: String,
}

/// Enhanced parser error with recovery
/// 带恢复功能的增强解析器错误
#[derive(Debug, Clone)]
pub struct ParserError {
    /// Error code
    /// 错误代码
    pub code: String,
    /// Error message
    /// 错误消息
    pub message: String,
    /// English error message
    /// 英文错误消息
    pub english_message: String,
    /// Error context
    /// 错误上下文
    pub context: ErrorContext,
    /// Suggested recovery strategy
    /// 建议的恢复策略
    pub recovery_strategy: RecoveryStrategy,
    /// Suggestion for fixing the error
    /// 修复错误的建议
    pub suggestion: String,
}

/// Parser error recovery manager
/// 解析器错误恢复管理器
pub struct ParserErrorRecovery {
    diagnostics: DiagnosticManager,
    recovery_points: Vec<usize>,
    bracket_stack: Vec<TokenKind>,
    current_line: usize,
    current_column: usize,
}

impl ParserErrorRecovery {
    /// Create a new parser error recovery manager
    /// 创建新的解析器错误恢复管理器
    pub fn new() -> Self {
        Self {
            diagnostics: DiagnosticManager::new(),
            recovery_points: Vec::new(),
            bracket_stack: Vec::new(),
            current_line: 1,
            current_column: 1,
        }
    }

    /// Get a reference to the diagnostics manager
    /// 获取诊断管理器的引用
    pub fn diagnostics(&self) -> &DiagnosticManager {
        &self.diagnostics
    }

    /// Consume the recovery manager and return diagnostics
    /// 消费恢复管理器并返回诊断信息
    pub fn into_diagnostics(self) -> DiagnosticManager {
        self.diagnostics
    }

    /// Update current position
    /// 更新当前位置
    pub fn update_position(&mut self, line: usize, column: usize) {
        self.current_line = line;
        self.current_column = column;
    }

    /// Add a recovery point
    /// 添加恢复点
    pub fn add_recovery_point(&mut self, position: usize) {
        self.recovery_points.push(position);
    }

    /// Remove the last recovery point
    /// 移除最后一个恢复点
    pub fn pop_recovery_point(&mut self) -> Option<usize> {
        self.recovery_points.pop()
    }

    /// Get the nearest recovery point
    /// 获取最近的恢复点
    pub fn get_nearest_recovery_point(&self, current_position: usize) -> Option<usize> {
        self.recovery_points
            .iter()
            .filter(|&&pos| pos >= current_position)
            .copied()
            .min()
    }

    /// Push opening bracket onto stack
    /// 将开括号推入栈中
    pub fn push_open_bracket(&mut self, bracket: TokenKind) {
        match bracket {
            TokenKind::左括号 | TokenKind::左方括号 | TokenKind::左大括号 => {
                self.bracket_stack.push(bracket);
            }
            _ => {}
        }
    }

    /// Pop closing bracket from stack
    /// 从栈中弹出闭括号
    pub fn pop_closing_bracket(&mut self, bracket: TokenKind) -> bool {
        match bracket {
            TokenKind::右括号 => {
                matches!(self.bracket_stack.pop(), Some(TokenKind::左括号))
            }
            TokenKind::右方括号 => {
                matches!(self.bracket_stack.pop(), Some(TokenKind::左方括号))
            }
            TokenKind::右大括号 => {
                matches!(self.bracket_stack.pop(), Some(TokenKind::左大括号))
            }
            _ => false,
        }
    }

    /// Get the expected closing bracket for the top of the stack
    /// 获取栈顶期望的闭括号
    pub fn get_expected_closing_bracket(&self) -> Option<TokenKind> {
        match self.bracket_stack.last()? {
            TokenKind::左括号 => Some(TokenKind::右括号),
            TokenKind::左方括号 => Some(TokenKind::右方括号),
            TokenKind::左大括号 => Some(TokenKind::右大括号),
            _ => None,
        }
    }

    /// Report a syntax error with enhanced information
    /// 报告带有增强信息的语法错误
    pub fn report_syntax_error(&mut self, context: ErrorContext) -> ParserError {
        let (code, message, english_message, suggestion) = self.generate_error_details(&context);

        let span = context.actual.span;
        let recovery_strategy = self.determine_recovery_strategy(&context);

        // Add diagnostic
        self.diagnostics.add_diagnostic({
            use crate::utils::diagnostics::Diagnostic;

            Diagnostic {
                level: DiagnosticLevel::错误,
                code: code.clone(),
                message: message.clone(),
                english_message: english_message.clone(),
                file_path: None,
                span: Some(span),
                suggestion: Some(suggestion.clone()),
                related_code: Some(context.actual.text.clone()),
            }
        });

        ParserError {
            code,
            message,
            english_message,
            context,
            recovery_strategy,
            suggestion,
        }
    }

    /// Generate error details based on context
    /// 根据上下文生成错误详情
    fn generate_error_details(&self, context: &ErrorContext) -> (String, String, String, String) {
        let expected_str = if context.expected.is_empty() {
            "未知".to_string()
        } else {
            context.expected
                .iter()
                .map(|k| format!("'{}'", self.token_kind_to_string(k)))
                .collect::<Vec<_>>()
                .join(" 或 ")
        };

        let actual_str = format!("'{}'", self.token_kind_to_string(&context.actual.kind));

        let (code, message, english_message, suggestion) = match context.context.as_str() {
            "function_declaration" => (
                "E008".to_string(),
                format!("函数声明语法错误: 期望 {}, 找到 {}", expected_str, actual_str),
                format!("Function declaration syntax error: expected {}, found {}", expected_str, actual_str),
                "检查函数声明语法是否正确: 函数名后面应该跟着括号和参数列表".to_string(),
            ),
            "variable_declaration" => (
                "E009".to_string(),
                format!("变量声明语法错误: 期望 {}, 找到 {}", expected_str, actual_str),
                format!("Variable declaration syntax error: expected {}, found {}", expected_str, actual_str),
                "检查变量声明语法: 变量名后面应该跟冒号和类型，或者直接赋值".to_string(),
            ),
            "expression" => (
                "E010".to_string(),
                format!("表达式语法错误: 期望 {}, 找到 {}", expected_str, actual_str),
                format!("Expression syntax error: expected {}, found {}", expected_str, actual_str),
                "检查表达式语法是否正确，确保操作符和操作数匹配".to_string(),
            ),
            "statement" => (
                "E011".to_string(),
                format!("语句语法错误: 期望 {}, 找到 {}", expected_str, actual_str),
                format!("Statement syntax error: expected {}, found {}", expected_str, actual_str),
                "检查语句语法是否正确，语句通常以分号或右大括号结束".to_string(),
            ),
            "type_annotation" => (
                "E012".to_string(),
                format!("类型注解语法错误: 期望 {}, 找到 {}", expected_str, actual_str),
                format!("Type annotation syntax error: expected {}, found {}", expected_str, actual_str),
                "检查类型注解语法，确保类型名称正确".to_string(),
            ),
            "function_call" => (
                "E013".to_string(),
                format!("函数调用语法错误: 期望 {}, 找到 {}", expected_str, actual_str),
                format!("Function call syntax error: expected {}, found {}", expected_str, actual_str),
                "检查函数调用语法: 函数名后面应该跟着括号和参数列表".to_string(),
            ),
            "block_statement" => (
                "E014".to_string(),
                format!("块语句语法错误: 期望 {}, 找到 {}", expected_str, actual_str),
                format!("Block statement syntax error: expected {}, found {}", expected_str, actual_str),
                "检查块语句语法: 确保左大括号和右大括号匹配".to_string(),
            ),
            "if_statement" => (
                "E015".to_string(),
                format!("如果语句语法错误: 期望 {}, 找到 {}", expected_str, actual_str),
                format!("If statement syntax error: expected {}, found {}", expected_str, actual_str),
                "检查如果语句语法: 如果 (条件) { 语句体 }".to_string(),
            ),
            "while_loop" => (
                "E016".to_string(),
                format!("当循环语法错误: 期望 {}, 找到 {}", expected_str, actual_str),
                format!("While loop syntax error: expected {}, found {}", expected_str, actual_str),
                "检查当循环语法: 当 (条件) { 循环体 }".to_string(),
            ),
            "for_loop" => (
                "E017".to_string(),
                format!("对于循环语法错误: 期望 {}, 找到 {}", expected_str, actual_str),
                format!("For loop syntax error: expected {}, found {}", expected_str, actual_str),
                "检查对于循环语法: 对于 (初始化; 条件; 更新) { 循环体 }".to_string(),
            ),
            _ => (
                "E018".to_string(),
                format!("语法错误: 期望 {}, 找到 {}", expected_str, actual_str),
                format!("Syntax error: expected {}, found {}", expected_str, actual_str),
                "检查语法是否正确，参考Qi语言语法规范".to_string(),
            ),
        };

        (code, message, english_message, suggestion)
    }

    /// Determine the best recovery strategy for the given error context
    /// 确定给定错误上下文的最佳恢复策略
    fn determine_recovery_strategy(&self, context: &ErrorContext) -> RecoveryStrategy {
        match context.context.as_str() {
            "function_declaration" | "variable_declaration" | "statement" => {
                RecoveryStrategy::SkipToSemicolon
            }
            "expression" => {
                if context.actual.kind == TokenKind::分号 {
                    RecoveryStrategy::SkipToNextStatement
                } else {
                    RecoveryStrategy::SkipToNextLine
                }
            }
            "block_statement" => {
                RecoveryStrategy::SkipToMatchingBracket
            }
            "function_call" => {
                RecoveryStrategy::SkipToNextToken
            }
            _ => {
                // Default strategy: skip to next semicolon or line end
                if context.actual.kind == TokenKind::分号 {
                    RecoveryStrategy::SkipToNextStatement
                } else {
                    RecoveryStrategy::SkipToSemicolon
                }
            }
        }
    }

    /// Convert token kind to string representation
    /// 将token类型转换为字符串表示
    fn token_kind_to_string(&self, kind: &TokenKind) -> String {
        match kind {
            TokenKind::标识符 => "标识符".to_string(),
            TokenKind::整数字面量(_) => "整数".to_string(),
            TokenKind::浮点数字面量 => "浮点数".to_string(),
            TokenKind::字符串字面量 => "字符串".to_string(),
            TokenKind::字符字面量(_) => "字符".to_string(),
            TokenKind::布尔字面量(_) => "布尔值".to_string(),
            TokenKind::左括号 => "(".to_string(),
            TokenKind::右括号 => ")".to_string(),
            TokenKind::左方括号 => "[".to_string(),
            TokenKind::右方括号 => "]".to_string(),
            TokenKind::左大括号 => "{".to_string(),
            TokenKind::右大括号 => "}".to_string(),
            TokenKind::分号 => ";".to_string(),
            TokenKind::逗号 => ",".to_string(),
            TokenKind::冒号 => ":".to_string(),
            TokenKind::点 => ".".to_string(),
            TokenKind::加 => "+".to_string(),
            TokenKind::减 => "-".to_string(),
            TokenKind::乘 => "*".to_string(),
            TokenKind::除 => "/".to_string(),
            TokenKind::取余 => "%".to_string(),
            TokenKind::赋值 => "=".to_string(),
            TokenKind::等于 => "==".to_string(),
            TokenKind::不等于 => "!=".to_string(),
            TokenKind::小于 => "<".to_string(),
            TokenKind::小于等于 => "<=".to_string(),
            TokenKind::大于 => ">".to_string(),
            TokenKind::大于等于 => ">=".to_string(),
            TokenKind::与 => "&&".to_string(),
            TokenKind::或 => "||".to_string(),
            TokenKind::非 => "!".to_string(),
            TokenKind::箭头 => "->".to_string(),
            TokenKind::函数 => "函数".to_string(),
            TokenKind::变量 => "变量".to_string(),
            TokenKind::常量 => "常量".to_string(),
            TokenKind::返回 => "返回".to_string(),
            TokenKind::如果 => "如果".to_string(),
            TokenKind::否则 => "否则".to_string(),
            TokenKind::当 => "当".to_string(),
            TokenKind::对于 => "对于".to_string(),
            TokenKind::跳出 => "跳出".to_string(),
            TokenKind::继续 => "继续".to_string(),
            TokenKind::真 => "真".to_string(),
            TokenKind::假 => "假".to_string(),
            TokenKind::空 => "空".to_string(),
            TokenKind::输入 => "输入".to_string(),
            TokenKind::长度 => "长度".to_string(),
            TokenKind::打印 => "打印".to_string(),
            TokenKind::类型 => "类型".to_string(),
            TokenKind::字符串 => "字符串".to_string(),
            TokenKind::布尔 => "布尔".to_string(),
            TokenKind::文件结束 => "文件结束".to_string(),

            // Type keywords
            TokenKind::类型关键词(basic_type) => match basic_type {
                BasicType::整数 => "整数".to_string(),
                BasicType::长整数 => "长整数".to_string(),
                BasicType::短整数 => "短整数".to_string(),
                BasicType::字节 => "字节".to_string(),
                BasicType::浮点数 => "浮点数".to_string(),
                BasicType::布尔 => "布尔".to_string(),
                BasicType::字符 => "字符".to_string(),
                BasicType::字符串 => "字符串".to_string(),
                BasicType::空 => "空".to_string(),
                BasicType::数组 => "数组".to_string(),
                BasicType::字典 => "字典".to_string(),
                BasicType::列表 => "列表".to_string(),
                BasicType::集合 => "集合".to_string(),
                BasicType::指针 => "指针".to_string(),
                BasicType::引用 => "引用".to_string(),
                BasicType::可变引用 => "可变引用".to_string(),
            },
            // Additional keywords that were missing
            TokenKind::循环 => "循环".to_string(),
            TokenKind::枚举 => "枚举".to_string(),
            TokenKind::数组 => "数组".to_string(),
            TokenKind::方法 => "方法".to_string(),
            TokenKind::自己 => "自己".to_string(),
            TokenKind::导入 => "导入".to_string(),
            TokenKind::导出 => "导出".to_string(),
            TokenKind::作为 => "作为".to_string(),
            TokenKind::在 => "在".to_string(),
            TokenKind::字符 => "字符".to_string(),
            TokenKind::参数 => "参数".to_string(),
            TokenKind::包 => "包".to_string(),
            TokenKind::模块 => "模块".to_string(),
            TokenKind::公开 => "公开".to_string(),
            TokenKind::私有 => "私有".to_string(),
            TokenKind::错误 => "错误".to_string(),
            TokenKind::自定义类型 => "自定义类型".to_string(),
        }
    }

    /// Attempt to recover from an error using the specified strategy
    /// 尝试使用指定策略从错误中恢复
    pub fn recover_from_error(&self, tokens: &[Token], current_position: usize, strategy: RecoveryStrategy) -> usize {
        match strategy {
            RecoveryStrategy::SkipToSemicolon => {
                self.skip_to_token(tokens, current_position, &[TokenKind::分号])
            }
            RecoveryStrategy::SkipToNextLine => {
                self.skip_to_next_line(tokens, current_position)
            }
            RecoveryStrategy::SkipToNextToken => {
                self.skip_to_next_token(tokens, current_position)
            }
            RecoveryStrategy::SkipToMatchingBracket => {
                self.skip_to_matching_bracket(tokens, current_position)
            }
            RecoveryStrategy::SkipToNextStatement => {
                self.skip_to_next_statement(tokens, current_position)
            }
            RecoveryStrategy::PanicMode => {
                self.panic_mode_recovery(tokens, current_position)
            }
        }
    }

    /// Skip tokens until finding one of the target kinds
    /// 跳过token直到找到目标类型之一
    fn skip_to_token(&self, tokens: &[Token], start_pos: usize, target_kinds: &[TokenKind]) -> usize {
        for i in start_pos..tokens.len().min(start_pos + 50) { // Limit lookahead to prevent infinite loops
            if target_kinds.contains(&tokens[i].kind) {
                return i + 1; // Return position after the found token
            }
        }
        tokens.len().min(start_pos + 50)
    }

    /// Skip to the next token (simple one-token skip)
    /// 跳到下一个token（简单的单token跳过）
    fn skip_to_next_token(&self, tokens: &[Token], start_pos: usize) -> usize {
        if start_pos < tokens.len() {
            start_pos + 1
        } else {
            start_pos
        }
    }

    /// Skip to the next line (look for newline in source)
    /// 跳到下一行（在源代码中查找换行符）
    fn skip_to_next_line(&self, tokens: &[Token], start_pos: usize) -> usize {
        for i in start_pos..tokens.len().min(start_pos + 50) {
            if tokens[i].line > self.current_line {
                return i;
            }
        }
        tokens.len().min(start_pos + 50)
    }

    /// Skip to matching bracket
    /// 跳到匹配的括号
    fn skip_to_matching_bracket(&self, tokens: &[Token], start_pos: usize) -> usize {
        if let Some(expected_closing) = self.get_expected_closing_bracket() {
            let mut depth = 1;

            for i in start_pos..tokens.len().min(start_pos + 100) {
                match tokens[i].kind {
                    TokenKind::左括号 | TokenKind::左方括号 | TokenKind::左大括号 => {
                        depth += 1;
                    }
                    TokenKind::右括号 | TokenKind::右方括号 | TokenKind::右大括号 => {
                        depth -= 1;
                        if depth == 0 && tokens[i].kind == expected_closing {
                            return i + 1;
                        }
                    }
                    _ => {}
                }
            }
        }
        tokens.len().min(start_pos + 50)
    }

    /// Skip to next statement (semicolon or right brace)
    /// 跳到下一个语句（分号或右大括号）
    fn skip_to_next_statement(&self, tokens: &[Token], start_pos: usize) -> usize {
        for i in start_pos..tokens.len().min(start_pos + 50) {
            match tokens[i].kind {
                TokenKind::分号 | TokenKind::右大括号 => {
                    return i + 1;
                }
                _ => {}
            }
        }
        tokens.len().min(start_pos + 50)
    }

    /// Panic mode recovery - skip to next recovery point or significant token
    /// 恢复模式 - 跳到下一个恢复点或重要token
    fn panic_mode_recovery(&self, tokens: &[Token], start_pos: usize) -> usize {
        // Try to find the nearest recovery point first
        if let Some(recovery_point) = self.get_nearest_recovery_point(start_pos) {
            return recovery_point;
        }

        // If no recovery point, skip to next significant token
        for i in start_pos..tokens.len().min(start_pos + 100) {
            match tokens[i].kind {
                TokenKind::函数 | TokenKind::变量 | TokenKind::常量 |
                TokenKind::如果 | TokenKind::当 | TokenKind::对于 |
                TokenKind::右大括号 | TokenKind::文件结束 => {
                    return i;
                }
                _ => {}
            }
        }
        tokens.len().min(start_pos + 100)
    }

    /// Get error summary statistics
    /// 获取错误摘要统计
    pub fn get_error_summary(&self) -> (usize, usize) {
        (self.diagnostics.error_count(), self.diagnostics.warning_count())
    }

    /// Check if any critical errors occurred
    /// 检查是否发生了关键错误
    pub fn has_critical_errors(&self) -> bool {
        self.diagnostics.error_count() > 0
    }

    /// Format all diagnostics as Chinese messages
    /// 将所有诊断信息格式化为中文消息
    pub fn format_diagnostics(&self) -> String {
        self.diagnostics.format_chinese_messages()
    }
}

impl Default for ParserErrorRecovery {
    fn default() -> Self {
        Self::new()
    }
}