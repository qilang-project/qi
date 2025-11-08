//! Unicode-aware lexical analysis for Qi language

pub mod keywords;
pub mod tokens;
pub mod unicode;

pub use tokens::{Token, TokenKind, Span};
pub use unicode::UnicodeHandler;

use crate::utils::diagnostics::{DiagnosticManager, DiagnosticLevel};


/// Qi language lexical analyzer
pub struct Lexer {
    source: String,
    position: usize,
    line: usize,
    column: usize,
    unicode_handler: UnicodeHandler,
    diagnostics: DiagnosticManager,
}

impl Lexer {
    /// Create a new lexer for the given source code
    pub fn new(source: String) -> Self {
        Self {
            source,
            position: 0,
            line: 1,
            column: 1,
            unicode_handler: UnicodeHandler::new(),
            diagnostics: DiagnosticManager::new(),
        }
    }

    /// Get a reference to the diagnostics manager
    pub fn diagnostics(&self) -> &DiagnosticManager {
        &self.diagnostics
    }

    /// Consume the lexer and return the diagnostics
    pub fn into_diagnostics(self) -> DiagnosticManager {
        self.diagnostics
    }

    /// Tokenize the entire source code
    pub fn tokenize(&mut self) -> Result<Vec<Token>, LexicalError> {
        let mut tokens = Vec::new();
        let mut first_error: Option<LexicalError> = None;

        while !self.is_at_end() {
            // Skip whitespace
            self.skip_whitespace();

            if self.is_at_end() {
                break;
            }

            match self.next_token() {
                Ok(Some(token)) => {
                    tokens.push(token);
                }
                Ok(None) => {
                    // None returned (e.g., for comments), just continue the loop
                }
                Err(e) => {
                    // Store first error but continue scanning for more errors
                    if first_error.is_none() {
                        first_error = Some(e);
                    }
                    // Skip the problematic character to continue scanning
                    self.advance();
                }
            }
        }

        // Add EOF token
        tokens.push(Token {
            kind: TokenKind::文件结束,
            text: String::new(),
            span: tokens::Span::new(self.position, self.position),
            line: self.line,
            column: self.column,
        });

        // Return the first error if any were encountered
        // This allows multiple errors to be collected in the diagnostics system
        // even though we return the first one to the caller
        if let Some(e) = first_error {
            Err(e)
        } else {
            Ok(tokens)
        }
    }

    /// Get the next token
    fn next_token(&mut self) -> Result<Option<Token>, LexicalError> {
        let start_pos = self.position;
        let start_line = self.line;
        let start_column = self.column;

        let c = self.current_char().ok_or(LexicalError::UnexpectedEof)?;

        match c {
            // Single-character tokens
            '(' => Ok(Some(self.make_single_char_token(TokenKind::左括号, start_pos, start_line, start_column))),
            ')' => Ok(Some(self.make_single_char_token(TokenKind::右括号, start_pos, start_line, start_column))),
            '[' => Ok(Some(self.make_single_char_token(TokenKind::左方括号, start_pos, start_line, start_column))),
            ']' => Ok(Some(self.make_single_char_token(TokenKind::右方括号, start_pos, start_line, start_column))),
            '{' => Ok(Some(self.make_single_char_token(TokenKind::左大括号, start_pos, start_line, start_column))),
            '}' => Ok(Some(self.make_single_char_token(TokenKind::右大括号, start_pos, start_line, start_column))),
            ';' => Ok(Some(self.make_single_char_token(TokenKind::分号, start_pos, start_line, start_column))),
            ',' => Ok(Some(self.make_single_char_token(TokenKind::逗号, start_pos, start_line, start_column))),
            ':' => {
                // Check for :: (double colon)
                if self.peek_char() == Some(':') {
                    Ok(Some(self.make_two_char_token(TokenKind::双冒号, start_pos, start_line, start_column)))
                } else {
                    Ok(Some(self.make_single_char_token(TokenKind::冒号, start_pos, start_line, start_column)))
                }
            }
            '.' => Ok(Some(self.make_single_char_token(TokenKind::点, start_pos, start_line, start_column))),

            // Operators and comments
            '+' => Ok(Some(self.make_single_char_token(TokenKind::加, start_pos, start_line, start_column))),
            '*' => Ok(Some(self.make_single_char_token(TokenKind::乘, start_pos, start_line, start_column))),
            '%' => Ok(Some(self.make_single_char_token(TokenKind::取余, start_pos, start_line, start_column))),
            '&' => Ok(Some(self.make_single_char_token(TokenKind::取地址, start_pos, start_line, start_column))),
            '/' => {
                if self.peek_char() == Some('/') {
                    // Check if it's a doc comment (///)
                    if self.peek_char_at_offset(2) == Some('/') {
                        // Doc line comment - skip and return None to continue main loop
                        self.skip_line_comment();
                        return Ok(None);
                    } else {
                        // Regular line comment - skip and return None to continue main loop
                        self.skip_line_comment();
                        return Ok(None);
                    }
                } else if self.peek_char() == Some('*') {
                    // Check if it's a doc block comment (/**)
                    if self.peek_char_at_offset(2) == Some('*') {
                        // Doc block comment - skip and return None to continue main loop
                        self.skip_doc_block_comment();
                        return Ok(None);
                    } else {
                        // Regular block comment - skip and return None to continue main loop
                        self.skip_block_comment();
                        return Ok(None);
                    }
                } else {
                    Ok(Some(self.make_single_char_token(TokenKind::除, start_pos, start_line, start_column)))
                }
            }

            // Assignment and comparison operators
            '=' => {
                if self.peek_char() == Some('=') {
                    self.advance();
                    Ok(Some(self.make_two_char_token(TokenKind::等于, start_pos, start_line, start_column)))
                } else {
                    Ok(Some(self.make_single_char_token(TokenKind::赋值, start_pos, start_line, start_column)))
                }
            }
            '-' => {
                if self.peek_char() == Some('>') {
                    self.advance();
                    Ok(Some(self.make_two_char_token(TokenKind::箭头, start_pos, start_line, start_column)))
                } else {
                    Ok(Some(self.make_single_char_token(TokenKind::减, start_pos, start_line, start_column)))
                }
            }
            '!' => {
                if self.peek_char() == Some('=') {
                    self.advance();
                    Ok(Some(self.make_two_char_token(TokenKind::不等于, start_pos, start_line, start_column)))
                } else {
                    self.report_invalid_character_error(c, start_pos, start_line, start_column, "可能想要使用 '!=' 表示不等于，或者检查是否有多余的字符");
                    Err(LexicalError::InvalidCharacter(c, start_line, start_column))
                }
            }
            '<' => {
                if self.peek_char() == Some('=') {
                    self.advance();
                    Ok(Some(self.make_two_char_token(TokenKind::小于等于, start_pos, start_line, start_column)))
                } else {
                    Ok(Some(self.make_single_char_token(TokenKind::小于, start_pos, start_line, start_column)))
                }
            }
            '>' => {
                if self.peek_char() == Some('=') {
                    self.advance();
                    Ok(Some(self.make_two_char_token(TokenKind::大于等于, start_pos, start_line, start_column)))
                } else {
                    Ok(Some(self.make_single_char_token(TokenKind::大于, start_pos, start_line, start_column)))
                }
            }

            // Character literals
            '\'' => self.scan_char_literal(start_pos, start_line, start_column).map(Some),

            // String literals
            '"' => self.scan_string_literal(start_pos, start_line, start_column).map(Some),

            // Numbers
            '0'..='9' => Ok(Some(self.scan_number(start_pos, start_line, start_column)?)),

            // Identifiers and keywords
            c if c.is_alphabetic() || c == '_' => {
                // Handle Chinese characters and standard identifiers
                if self.unicode_handler.is_chinese_char(c) {
                    Ok(Some(self.scan_chinese_identifier(start_pos, start_line, start_column)))
                } else {
                    Ok(Some(self.scan_identifier(start_pos, start_line, start_column)))
                }
            }

            // Chinese punctuation tokens
            '（' => Ok(Some(self.make_single_char_token(TokenKind::中文左括号, start_pos, start_line, start_column))),
            '）' => Ok(Some(self.make_single_char_token(TokenKind::中文右括号, start_pos, start_line, start_column))),
            '【' => Ok(Some(self.make_single_char_token(TokenKind::中文左大括号, start_pos, start_line, start_column))),
            '】' => Ok(Some(self.make_single_char_token(TokenKind::中文右大括号, start_pos, start_line, start_column))),
            '，' => Ok(Some(self.make_single_char_token(TokenKind::中文逗号, start_pos, start_line, start_column))),
            '；' => Ok(Some(self.make_single_char_token(TokenKind::中文分号, start_pos, start_line, start_column))),

            // Other Chinese punctuation (treat as whitespace/end of statements)
            c if "。！？：".contains(c) ||
               c == '"' || c == '"' ||
               c == '《' || c == '》' => {
                self.advance();
                return self.next_token(); // Skip Chinese punctuation
            }

            // Whitespace (should be skipped by skip_whitespace, but handle anyway)
            c if c.is_whitespace() => {
                self.advance();
                return self.next_token(); // Skip and get next token
            }

            _ => {
                self.report_invalid_character_error(c, start_pos, start_line, start_column, "检查字符是否为有效的Qi语言符号，或者删除不需要的字符");
                Err(LexicalError::InvalidCharacter(c, start_line, start_column))
            }
        }
    }

    /// Create a single character token
    fn make_single_char_token(&mut self, kind: TokenKind, start_pos: usize, start_line: usize, start_column: usize) -> Token {
        let text = self.current_char().unwrap().to_string();
        self.advance();
        Token {
            kind,
            text,
            span: tokens::Span::new(start_pos, self.position),
            line: start_line,
            column: start_column,
        }
    }

    /// Create a two character token
    fn make_two_char_token(&mut self, kind: TokenKind, start_pos: usize, start_line: usize, start_column: usize) -> Token {
        self.advance(); // Advance to include the second character
        Token {
            kind,
            text: self.source[start_pos..self.position].to_string(),
            span: tokens::Span::new(start_pos, self.position),
            line: start_line,
            column: start_column,
        }
    }

    /// Scan a string literal
    fn scan_string_literal(&mut self, start_pos: usize, start_line: usize, start_column: usize) -> Result<Token, LexicalError> {
        self.advance(); // Skip opening quote
        let start_content = self.position;

        while !self.is_at_end() && self.current_char() != Some('"') {
            if self.current_char() == Some('\\') {
                self.advance(); // Skip escape character
            }
            self.advance();
        }

        if self.is_at_end() {
            self.report_unterminated_string_error(start_pos, start_line, start_column, "字符串缺少右引号 \"，请在字符串末尾添加右引号");
            return Err(LexicalError::UnterminatedString(start_line, start_column));
        }

        let end_content = self.position;
        self.advance(); // Skip closing quote
        let end_pos = self.position;

        let _content = self.source[start_content..end_content].to_string();

        Ok(Token {
            kind: TokenKind::字符串字面量,
            text: self.source[start_pos..end_pos].to_string(),
            span: tokens::Span::new(start_pos, end_pos),
            line: start_line,
            column: start_column,
        })
    }

    /// Scan a character literal
    fn scan_char_literal(&mut self, start_pos: usize, start_line: usize, start_column: usize) -> Result<Token, LexicalError> {
        self.advance(); // Skip opening quote

        if self.is_at_end() {
            self.report_unterminated_string_error(start_pos, start_line, start_column, "字符字面量缺少右引号 '，请在字符后添加右引号");
            return Err(LexicalError::UnterminatedString(start_line, start_column));
        }

        let char_content = self.current_char().unwrap();

        // Handle escape sequences
        let final_char = if char_content == '\\' {
            self.advance(); // Skip escape character
            if self.is_at_end() {
                self.report_unterminated_string_error(start_pos, start_line, start_column, "转义字符序列不完整，检查转义字符后是否有有效字符");
                return Err(LexicalError::UnterminatedString(start_line, start_column));
            }

            let escaped_char = self.current_char().unwrap();
            match escaped_char {
                'n' => '\n',
                't' => '\t',
                'r' => '\r',
                '\\' => '\\',
                '\'' => '\'',
                '"' => '"',
                _ => escaped_char, // For other escape sequences, just use the character as-is
            }
        } else {
            char_content
        };

        self.advance(); // Skip the character

        // Expect closing quote
        if self.current_char() != Some('\'') {
            self.report_unterminated_string_error(start_pos, start_line, start_column, "字符字面量必须以右引号 ' 结尾，字符后请添加右引号");
            return Err(LexicalError::UnterminatedString(start_line, start_column));
        }

        self.advance(); // Skip closing quote
        let end_pos = self.position;

        Ok(Token {
            kind: TokenKind::字符字面量(final_char),
            text: self.source[start_pos..end_pos].to_string(),
            span: tokens::Span::new(start_pos, end_pos),
            line: start_line,
            column: start_column,
        })
    }

    /// Scan a number (integer or float)
    fn scan_number(&mut self, start_pos: usize, start_line: usize, start_column: usize) -> Result<Token, LexicalError> {
        while !self.is_at_end() && self.current_char().unwrap().is_ascii_digit() {
            self.advance();
        }

        // Check for float
        if self.current_char() == Some('.') {
            self.advance();
            while !self.is_at_end() && self.current_char().unwrap().is_ascii_digit() {
                self.advance();
            }

            let number_str = self.source[start_pos..self.position].to_string();

            // Validate float format
            if number_str.parse::<f64>().is_err() {
                self.report_invalid_number_error(start_pos, start_line, start_column, "浮点数格式无效，检查数字格式是否正确");
                return Err(LexicalError::InvalidNumber(start_line, start_column));
            }

            Ok(Token {
                kind: TokenKind::浮点数字面量,
                text: number_str.clone(),
                span: tokens::Span::new(start_pos, self.position),
                line: start_line,
                column: start_column,
            })
        } else {
            let number_str = self.source[start_pos..self.position].to_string();

            // Validate integer format
            let value = if let Ok(val) = number_str.parse::<i64>() {
                val
            } else {
                self.report_invalid_number_error(start_pos, start_line, start_column, "整数格式无效，检查数字是否在有效范围内");
                return Err(LexicalError::InvalidNumber(start_line, start_column));
            };

            Ok(Token {
                kind: TokenKind::整数字面量(value),
                text: number_str.clone(),
                span: tokens::Span::new(start_pos, self.position),
                line: start_line,
                column: start_column,
            })
        }
    }

    /// Scan a standard identifier
    fn scan_identifier(&mut self, start_pos: usize, start_line: usize, start_column: usize) -> Token {
        while !self.is_at_end() {
            let c = self.current_char().unwrap();
            // Support mixed Latin and Chinese identifiers (e.g., MD5哈希, SHA256哈希)
            if c.is_alphanumeric() || c == '_' || self.unicode_handler.is_chinese_char(c) {
                self.advance();
            } else {
                break;
            }
        }

        let text = &self.source[start_pos..self.position];

        // Check if it's a keyword
        let kind = keywords::KEYWORDS.lookup(text)
            .unwrap_or(TokenKind::标识符);

        Token {
            kind,
            text: text.to_string(),
            span: tokens::Span::new(start_pos, self.position),
            line: start_line,
            column: start_column,
        }
    }

    /// Scan a Chinese identifier or keyword
    fn scan_chinese_identifier(&mut self, start_pos: usize, start_line: usize, start_column: usize) -> Token {
        while !self.is_at_end() {
            let c = self.current_char().unwrap();
            if self.unicode_handler.is_chinese_char(c) || c.is_alphanumeric() || c == '_' {
                self.advance();
            } else {
                break;
            }
        }

        let text = &self.source[start_pos..self.position];

        // Check if it's a Chinese keyword
        let kind = keywords::KEYWORDS.lookup(text)
            .unwrap_or(TokenKind::标识符);

        Token {
            kind,
            text: text.to_string(),
            span: tokens::Span::new(start_pos, self.position),
            line: start_line,
            column: start_column,
        }
    }

    /// Advance to the next character
    fn advance(&mut self) {
        if self.is_at_end() {
            return;
        }

        let current_char = self.current_char().unwrap_or('\0');
        if current_char == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += self.unicode_handler.char_width(current_char);
        }
        self.position += current_char.len_utf8();
    }

    /// Skip whitespace characters
    fn skip_whitespace(&mut self) {
        while !self.is_at_end() {
            let current_char = self.current_char().unwrap_or('\0');
            if !current_char.is_whitespace() {
                break;
            }

            if current_char == '\n' {
                self.line += 1;
                self.column = 1;
            } else {
                self.column += self.unicode_handler.char_width(current_char);
            }
            self.position += current_char.len_utf8();
        }
    }

    /// Check if we're at the end of the source
    fn is_at_end(&self) -> bool {
        self.position >= self.source.len()
    }

    /// Get the current character
    fn current_char(&self) -> Option<char> {
        self.source[self.position..].chars().next()
    }

    /// Look ahead at the next character
    fn peek_char(&self) -> Option<char> {
        // Get current character
        let current_char = self.current_char()?;

        // Get current character length in bytes
        let char_len = current_char.len_utf8();

        // Look ahead from the position after current character
        self.source[self.position + char_len..].chars().next()
    }

    /// Look ahead at character at specific offset (character-based)
    fn peek_char_at_offset(&self, offset: usize) -> Option<char> {
        if offset == 0 {
            return self.current_char();
        }

        let mut chars = self.source[self.position..].chars();
        for _ in 0..offset {
            if chars.next().is_none() {
                return None;
            }
        }
        chars.next()
    }

    /// Skip line comment (// to end of line)
    fn skip_line_comment(&mut self) {
        // Skip both slashes
        self.advance(); // skip first '/'
        self.advance(); // skip second '/'

        // Skip until end of line or file
        while !self.is_at_end() && self.current_char() != Some('\n') {
            self.advance();
        }
    }

    /// Skip block comment (/* ... */)
    fn skip_block_comment(&mut self) {
        // Skip opening /*
        self.advance(); // skip '/'
        self.advance(); // skip '*'

        let mut depth = 1;

        while !self.is_at_end() && depth > 0 {
            match (self.current_char(), self.peek_char()) {
                (Some('/'), Some('*')) => {
                    // Found nested block comment start
                    self.advance(); // skip '/'
                    self.advance(); // skip '*'
                    depth += 1;
                }
                (Some('*'), Some('/')) => {
                    // Found block comment end
                    self.advance(); // skip '*'
                    self.advance(); // skip '/'
                    depth -= 1;
                }
                (Some('\n'), _) => {
                    // Handle line breaks properly for line counting
                    self.advance();
                }
                _ => {
                    self.advance();
                }
            }
        }

        if depth > 0 {
            // Unterminated block comment - this would be an error in a real implementation
            // For now, we'll just skip to end of file
        }
    }

    /// Skip doc block comment (/** ... */)
    fn skip_doc_block_comment(&mut self) {
        // Skip opening /**
        self.advance(); // skip '/'
        self.advance(); // skip '*'
        self.advance(); // skip '*'

        let mut depth = 1;

        while !self.is_at_end() && depth > 0 {
            match (self.current_char(), self.peek_char()) {
                (Some('/'), Some('*')) => {
                    // Found nested block comment start
                    self.advance(); // skip '/'
                    self.advance(); // skip '*'
                    depth += 1;
                }
                (Some('*'), Some('/')) => {
                    // Found block comment end
                    self.advance(); // skip '*'
                    self.advance(); // skip '/'
                    depth -= 1;
                }
                (Some('\n'), _) => {
                    // Handle line breaks properly for line counting
                    self.advance();
                }
                _ => {
                    self.advance();
                }
            }
        }

        if depth > 0 {
            // Unterminated doc block comment - this would be an error in a real implementation
            // For now, we'll just skip to end of file
        }
    }

    // ===== Enhanced Error Reporting Methods | 增强错误报告方法 =====

    /// Report invalid character error with detailed suggestions
    fn report_invalid_character_error(&mut self, char: char, start_pos: usize, _line: usize, _column: usize, suggestion: &str) {
        let span = Span::new(start_pos, start_pos + char.len_utf8());

        self.diagnostics.add_diagnostic({
            use crate::utils::diagnostics::Diagnostic;
  
            Diagnostic {
                level: DiagnosticLevel::错误,
                code: "E011".to_string(),
                message: format!("无效字符: '{}'", char),
                english_message: format!("Invalid character: '{}'", char),
                file_path: None,
                span: Some(span),
                suggestion: Some(suggestion.to_string()),
                related_code: Some(self.source[start_pos..start_pos + char.len_utf8()].to_string()),
            }
        });
    }

    /// Report unterminated string error with detailed suggestions
    fn report_unterminated_string_error(&mut self, start_pos: usize, _line: usize, _column: usize, suggestion: &str) {
        let end_pos = self.position.min(self.source.len());
        let span = Span::new(start_pos, end_pos);

        self.diagnostics.add_diagnostic({
            use crate::utils::diagnostics::Diagnostic;

            Diagnostic {
                level: DiagnosticLevel::错误,
                code: "E012".to_string(),
                message: "未终止的字符串字面量".to_string(),
                english_message: "Unterminated string literal".to_string(),
                file_path: None,
                span: Some(span),
                suggestion: Some(suggestion.to_string()),
                related_code: Some(self.source[start_pos..end_pos].to_string()),
            }
        });
    }

    /// Report invalid number error with detailed suggestions
    fn report_invalid_number_error(&mut self, start_pos: usize, _line: usize, _column: usize, suggestion: &str) {
        let end_pos = self.position.min(self.source.len());
        let span = Span::new(start_pos, end_pos);

        self.diagnostics.add_diagnostic({
            use crate::utils::diagnostics::Diagnostic;

            Diagnostic {
                level: DiagnosticLevel::错误,
                code: "E013".to_string(),
                message: "无效的数字格式".to_string(),
                english_message: "Invalid number format".to_string(),
                file_path: None,
                span: Some(span),
                suggestion: Some(suggestion.to_string()),
                related_code: Some(self.source[start_pos..end_pos].to_string()),
            }
        });
    }

    /// Report unexpected EOF error
    #[allow(dead_code)]
    fn report_unexpected_eof_error(&mut self, position: usize, suggestion: &str) {
        let span = Span::new(position, position);

        self.diagnostics.add_diagnostic({
            use crate::utils::diagnostics::Diagnostic;

            Diagnostic {
                level: DiagnosticLevel::错误,
                code: "E014".to_string(),
                message: "意外的文件结束".to_string(),
                english_message: "Unexpected end of file".to_string(),
                file_path: None,
                span: Some(span),
                suggestion: Some(suggestion.to_string()),
                related_code: None,
            }
        });
    }

    /// Get error summary statistics
    pub fn get_error_summary(&self) -> (usize, usize) {
        (self.diagnostics.error_count(), self.diagnostics.warning_count())
    }

    /// Check if any critical errors occurred
    pub fn has_critical_errors(&self) -> bool {
        self.diagnostics.error_count() > 0
    }

    /// Format all diagnostics as Chinese messages
    pub fn format_diagnostics(&self) -> String {
        self.diagnostics.format_chinese_messages()
    }
}

/// Lexical analysis errors
#[derive(Debug, thiserror::Error)]
pub enum LexicalError {
    /// Invalid character
    #[error("无效字符: '{0}' 在第 {1} 行第 {2} 列")]
    InvalidCharacter(char, usize, usize),

    /// Unterminated string literal
    #[error("未终止的字符串字面量在第 {0} 行第 {1} 列")]
    UnterminatedString(usize, usize),

    /// Invalid number format
    #[error("无效的数字格式在第 {0} 行第 {1} 列")]
    InvalidNumber(usize, usize),

    /// Unexpected end of file
    #[error("意外的文件结束")]
    UnexpectedEof,
}