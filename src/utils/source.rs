//! Source file management for Qi language

use std::path::PathBuf;
use std::fs;
use std::time::SystemTime;

/// Source file representation
#[derive(Debug, Clone)]
pub struct SourceFile {
    pub path: PathBuf,
    pub content: String,
    pub encoding: Encoding,
    pub line_offsets: Vec<usize>,
    pub last_modified: SystemTime,
    pub dependencies: Vec<PathBuf>,
}

/// Text encoding
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Encoding {
    Utf8,
    Unknown,
}

/// Source file error
#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    /// File not found
    #[error("文件未找到: {0}")]
    NotFound(PathBuf),

    /// I/O error
    #[error("I/O 错误: {0}")]
    Io(#[from] std::io::Error),

    /// Invalid encoding
    #[error("无效的编码: {0}")]
    InvalidEncoding(String),

    /// File too large
    #[error("文件过大: {0} 字节")]
    TooLarge(usize),
}

impl SourceFile {
    pub fn new(path: PathBuf) -> Result<Self, SourceError> {
        let content = fs::read_to_string(&path)?;
        let last_modified = fs::metadata(&path)?.modified()
            .unwrap_or_else(|_| SystemTime::now());

        Self::from_content(path, content, last_modified)
    }

    pub fn from_content(path: PathBuf, content: String, last_modified: SystemTime) -> Result<Self, SourceError> {
        // Validate UTF-8 encoding (String is always UTF-8 in Rust)
        // TODO: Add additional validation if needed

        // Check file size (10MB limit)
        if content.len() > 10 * 1024 * 1024 {
            return Err(SourceError::TooLarge(content.len()));
        }

        // Compute line offsets
        let line_offsets = Self::compute_line_offsets(&content);

        Ok(Self {
            path,
            content,
            encoding: Encoding::Utf8,
            line_offsets,
            last_modified,
            dependencies: Vec::new(),
        })
    }

    fn compute_line_offsets(content: &str) -> Vec<usize> {
        let mut offsets = vec![0]; // First line starts at byte 0

        for (byte_offset, c) in content.char_indices() {
            if c == '\n' {
                offsets.push(byte_offset + 1); // Next line starts after \n
            }
        }

        offsets
    }

    pub fn get_position(&self, byte_offset: usize) -> Option<Position> {
        if byte_offset >= self.content.len() {
            return None;
        }

        // Binary search to find line number
        let line = self.line_offsets.binary_search(&byte_offset)
            .unwrap_or_else(|idx| idx);

        let column = byte_offset - self.line_offsets[line];
        Some(Position {
            line: line + 1, // 1-based line numbers
            column: column + 1, // 1-based column numbers
        })
    }

    pub fn get_line(&self, line_number: usize) -> Option<&str> {
        if line_number == 0 || line_number > self.line_offsets.len() {
            return None;
        }

        let line_index = line_number - 1;
        let start = self.line_offsets[line_index];
        let end = if line_index + 1 < self.line_offsets.len() {
            self.line_offsets[line_index + 1]
        } else {
            self.content.len()
        };

        // Extract line without trailing newline
        let line = &self.content[start..end];
        let line = line.trim_end_matches('\n');
        Some(line)
    }

    pub fn get_line_range(&self, line_number: usize) -> Option<(usize, usize)> {
        if line_number == 0 || line_number > self.line_offsets.len() {
            return None;
        }

        let line_index = line_number - 1;
        let start = self.line_offsets[line_index];
        let end = if line_index + 1 < self.line_offsets.len() {
            self.line_offsets[line_index + 1]
        } else {
            self.content.len()
        };

        Some((start, end))
    }

    pub fn add_dependency(&mut self, path: PathBuf) {
        if !self.dependencies.contains(&path) {
            self.dependencies.push(path);
        }
    }

    pub fn get_dependencies(&self) -> &[PathBuf] {
        &self.dependencies
    }

    pub fn is_modified_since(&self, timestamp: SystemTime) -> bool {
        self.last_modified > timestamp
    }

    pub fn get_filename(&self) -> Option<&str> {
        self.path.file_name()?.to_str()
    }

    pub fn get_extension(&self) -> Option<&str> {
        self.path.extension()?.to_str()
    }

    pub fn is_qi_file(&self) -> bool {
        self.get_extension() == Some("qi")
    }
}

/// Source position (line and column)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub line: usize,
    pub column: usize,
}

impl Position {
    pub fn new(line: usize, column: usize) -> Self {
        Self { line, column }
    }
}

/// Source file manager
pub struct SourceManager {
    files: Vec<SourceFile>,
    max_files: usize,
}

impl SourceManager {
    pub fn new() -> Self {
        Self {
            files: Vec::new(),
            max_files: 1000,
        }
    }

    pub fn load_file(&mut self, path: PathBuf) -> Result<&SourceFile, SourceError> {
        // Check if file is already loaded
        if let Some(index) = self.files.iter().position(|f| f.path == path) {
            return Ok(&self.files[index]);
        }

        // Load new file
        let source_file = SourceFile::new(path)?;
        self.files.push(source_file);

        // Return reference to the loaded file
        Ok(&self.files.last().unwrap())
    }

    pub fn get_file(&self, path: &PathBuf) -> Option<&SourceFile> {
        self.files.iter().find(|f| f.path == *path)
    }

    pub fn get_files(&self) -> &[SourceFile] {
        &self.files
    }

    pub fn clear(&mut self) {
        self.files.clear();
    }

    pub fn set_max_files(&mut self, max: usize) {
        self.max_files = max;
    }
}

impl Default for SourceManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Enhanced source context for error reporting
/// 增强的错误报告源代码上下文
#[derive(Debug, Clone)]
pub struct ErrorContext {
    pub file_path: Option<PathBuf>,
    pub line_number: usize,
    pub column_number: usize,
    pub source_line: String,
    pub line_before: Option<String>,
    pub line_after: Option<String>,
    pub pointer: String,
    /// Additional context lines before and after
    /// 额外的上下文行（之前和之后）
    pub more_context_before: Vec<String>,
    pub more_context_after: Vec<String>,
    /// Error span information
    /// 错误范围信息
    pub error_span: Option<ErrorSpan>,
    /// Related code snippets
    /// 相关代码片段
    pub related_code: Vec<RelatedCode>,
}

/// Error span with start and end positions
/// 带开始和结束位置的错误范围
#[derive(Debug, Clone)]
pub struct ErrorSpan {
    pub start_line: usize,
    pub start_column: usize,
    pub end_line: usize,
    pub end_column: usize,
    pub text: String,
}

/// Related code information
/// 相关代码信息
#[derive(Debug, Clone)]
pub struct RelatedCode {
    pub file_path: Option<PathBuf>,
    pub line_number: usize,
    pub column_number: usize,
    pub description: String,
    pub code_snippet: String,
}

impl ErrorContext {
    /// Create error context for a source file
    /// 为源文件创建错误上下文
    pub fn new(source_file: &SourceFile, line_number: usize, column_number: usize) -> Self {
        let source_line = source_file.get_line(line_number)
            .unwrap_or("")
            .to_string();

        let line_before = if line_number > 1 {
            source_file.get_line(line_number - 1).map(|s| s.to_string())
        } else {
            None
        };

        let line_after = if line_number < source_file.line_offsets.len() {
            source_file.get_line(line_number + 1).map(|s| s.to_string())
        } else {
            None
        };

        // Create pointer line that shows the exact error position
        let mut pointer = String::new();
        if column_number > 0 {
            for _ in 1..column_number {
                pointer.push(' ');
            }
            pointer.push('↑');
        } else {
            pointer.push('↑');
        }

        Self {
            file_path: Some(source_file.path.clone()),
            line_number,
            column_number,
            source_line,
            line_before,
            line_after,
            pointer,
            more_context_before: Vec::new(),
            more_context_after: Vec::new(),
            error_span: None,
            related_code: Vec::new(),
        }
    }

    /// Create error context with extended context lines
    /// 创建带扩展上下文行的错误上下文
    pub fn new_with_context(
        source_file: &SourceFile,
        line_number: usize,
        column_number: usize,
        context_lines: usize,
    ) -> Self {
        let mut context = Self::new(source_file, line_number, column_number);

        // Add more context lines before
        for i in (2..=context_lines).rev() {
            if line_number > i {
                if let Some(line) = source_file.get_line(line_number - i) {
                    context.more_context_before.push(line.to_string());
                }
            }
        }

        // Add more context lines after
        for i in 2..=context_lines {
            if line_number + i - 1 < source_file.line_offsets.len() {
                if let Some(line) = source_file.get_line(line_number + i) {
                    context.more_context_after.push(line.to_string());
                }
            }
        }

        context
    }

    /// Create error context with span information
    /// 创建带范围信息的错误上下文
    pub fn new_with_span(
        source_file: &SourceFile,
        start_line: usize,
        start_column: usize,
        end_line: usize,
        end_column: usize,
        context_lines: usize,
    ) -> Self {
        let mut context = Self::new_with_context(source_file, start_line, start_column, context_lines);

        // Extract span text
        let span_text = if start_line == end_line {
            // Single line span
            if let Some(line) = source_file.get_line(start_line) {
                let start_idx = start_column.saturating_sub(1);
                let end_idx = end_column.min(line.len());
                line[start_idx..end_idx].to_string()
            } else {
                String::new()
            }
        } else {
            // Multi-line span
            let mut text = String::new();
            for line in start_line..=end_line {
                if let Some(line_content) = source_file.get_line(line) {
                    if line == start_line {
                        let start_idx = start_column.saturating_sub(1);
                        text.push_str(&line_content[start_idx..]);
                    } else if line == end_line {
                        let end_idx = end_column.min(line_content.len());
                        text.push_str(&line_content[..end_idx]);
                    } else {
                        text.push_str(line_content);
                    }
                    if line < end_line {
                        text.push('\n');
                    }
                }
            }
            text
        };

        context.error_span = Some(ErrorSpan {
            start_line,
            start_column,
            end_line,
            end_column,
            text: span_text,
        });

        context
    }

    /// Add related code snippet
    /// 添加相关代码片段
    pub fn add_related_code(&mut self, description: String, file_path: Option<PathBuf>, line: usize, column: usize, snippet: String) {
        self.related_code.push(RelatedCode {
            file_path,
            line_number: line,
            column_number: column,
            description,
            code_snippet: snippet,
        });
    }

    /// Get the total number of context lines
    /// 获取上下文行总数
    pub fn total_context_lines(&self) -> usize {
        self.more_context_before.len() + 1 + self.more_context_after.len()
            + if self.line_before.is_some() { 1 } else { 0 }
            + if self.line_after.is_some() { 1 } else { 0 }
    }

    /// Format the error context for display
    /// 格式化错误上下文以供显示
    pub fn format(&self) -> String {
        let mut result = String::new();

        if let Some(file_path) = &self.file_path {
            result.push_str(&format!("--> 文件: {}\n", file_path.display()));
        }

        result.push_str("     |\n");

        // Show extended context before
        for (i, line) in self.more_context_before.iter().rev().enumerate() {
            let line_num = self.line_number - self.more_context_before.len() + i;
            result.push_str(&format!(" {} | {}\n", line_num, line));
        }

        // Show immediate line before
        if let Some(line_before) = &self.line_before {
            result.push_str(&format!(" {} | {}\n", self.line_number - 1, line_before));
        }

        // Show error line with enhanced highlighting
        result.push_str(&format!(" {} | {}\n", self.line_number, self.source_line));

        // Enhanced pointer for span errors
        if let Some(ref span) = self.error_span {
            if span.start_line == span.end_line {
                // Single line span - show range
                let mut pointer = String::new();
                for _ in 1..span.start_column {
                    pointer.push(' ');
                }
                for _ in span.start_column..=span.end_column {
                    pointer.push('^');
                }
                result.push_str(&format!("     | {}\n", pointer));
            } else {
                // Multi-line span - show start position
                let mut pointer = String::new();
                for _ in 1..span.start_column {
                    pointer.push(' ');
                }
                pointer.push('^');
                result.push_str(&format!("     | {}\n", pointer));
            }
        } else {
            // Simple pointer
            result.push_str(&format!("     | {}\n", self.pointer));
        }

        // Show immediate line after
        if let Some(line_after) = &self.line_after {
            result.push_str(&format!(" {} | {}\n", self.line_number + 1, line_after));
        }

        // Show extended context after
        for (i, line) in self.more_context_after.iter().enumerate() {
            let line_num = self.line_number + 1 + i + if self.line_after.is_some() { 1 } else { 0 };
            result.push_str(&format!(" {} | {}\n", line_num, line));
        }

        result.push_str("     |\n");

        // Show span information if available
        if let Some(ref span) = self.error_span {
            result.push_str(&format!("     错误范围: 第{}行第{}列 到 第{}行第{}列\n",
                span.start_line, span.start_column, span.end_line, span.end_column));
            result.push_str(&format!("     错误文本: \"{}\"\n", span.text));
        }

        // Show related code if available
        if !self.related_code.is_empty() {
            result.push_str("     相关代码:\n");
            for (i, related) in self.related_code.iter().enumerate() {
                result.push_str(&format!("       {}. {}:\n", i + 1, related.description));
                if let Some(ref path) = related.file_path {
                    result.push_str(&format!("          文件: {}\n", path.display()));
                }
                result.push_str(&format!("          位置: 第{}行第{}列\n", related.line_number, related.column_number));
                result.push_str(&format!("          代码: {}\n", related.code_snippet));
                if i < self.related_code.len() - 1 {
                    result.push('\n');
                }
            }
        }

        result
    }

    /// Format a compact version for brief error reporting
    pub fn format_compact(&self) -> String {
        let mut result = String::new();

        if let Some(file_path) = &self.file_path {
            result.push_str(&format!("{}:", file_path.display()));
        }

        result.push_str(&format!("{}:{}:", self.line_number, self.column_number));

        result
    }
}

/// Create error context from source content string
/// 从源代码字符串创建错误上下文
pub fn create_error_context_from_string(
    source_code: &str,
    line_number: usize,
    column_number: usize,
) -> ErrorContext {
    let lines: Vec<&str> = source_code.lines().collect();

    let source_line = if line_number > 0 && line_number <= lines.len() {
        lines[line_number - 1].to_string()
    } else {
        String::new()
    };

    let line_before = if line_number > 1 {
        Some(lines[line_number - 2].to_string())
    } else {
        None
    };

    let line_after = if line_number < lines.len() {
        Some(lines[line_number].to_string())
    } else {
        None
    };

    // Create pointer line that shows the exact error position
    let mut pointer = String::new();
    if column_number > 0 {
        for _ in 1..column_number {
            pointer.push(' ');
        }
        pointer.push('↑');
    } else {
        pointer.push('↑');
    }

    ErrorContext {
        file_path: None,
        line_number,
        column_number,
        source_line,
        line_before,
        line_after,
        pointer,
        more_context_before: Vec::new(),
        more_context_after: Vec::new(),
        error_span: None,
        related_code: Vec::new(),
    }
}

/// Create error context with extended context from source string
/// 从源代码字符串创建带扩展上下文的错误上下文
pub fn create_error_context_from_string_with_context(
    source_code: &str,
    line_number: usize,
    column_number: usize,
    context_lines: usize,
) -> ErrorContext {
    let lines: Vec<&str> = source_code.lines().collect();
    let mut context = create_error_context_from_string(source_code, line_number, column_number);

    // Add more context lines before
    for i in (2..=context_lines).rev() {
        if line_number > i {
            if let Some(line) = lines.get(line_number - i - 1) {
                context.more_context_before.push(line.to_string());
            }
        }
    }

    // Add more context lines after
    for i in 2..=context_lines {
        if let Some(line) = lines.get(line_number + i - 1) {
            context.more_context_after.push(line.to_string());
        }
    }

    context
}

/// Create error context with span from source string
/// 从源代码字符串创建带范围的错误上下文
pub fn create_error_context_from_string_with_span(
    source_code: &str,
    start_line: usize,
    start_column: usize,
    end_line: usize,
    end_column: usize,
    context_lines: usize,
) -> ErrorContext {
    let mut context = create_error_context_from_string_with_context(
        source_code, start_line, start_column, context_lines);

    // Extract span text
    let lines: Vec<&str> = source_code.lines().collect();
    let span_text = if start_line == end_line {
        // Single line span
        if let Some(line) = lines.get(start_line - 1) {
            let start_idx = start_column.saturating_sub(1);
            let end_idx = end_column.min(line.len());
            line[start_idx..end_idx].to_string()
        } else {
            String::new()
        }
    } else {
        // Multi-line span
        let mut text = String::new();
        for line in start_line..=end_line {
            if let Some(line_content) = lines.get(line - 1) {
                if line == start_line {
                    let start_idx = start_column.saturating_sub(1);
                    text.push_str(&line_content[start_idx..]);
                } else if line == end_line {
                    let end_idx = end_column.min(line_content.len());
                    text.push_str(&line_content[..end_idx]);
                } else {
                    text.push_str(line_content);
                }
                if line < end_line {
                    text.push('\n');
                }
            }
        }
        text
    };

    context.error_span = Some(ErrorSpan {
        start_line,
        start_column,
        end_line,
        end_column,
        text: span_text,
    });

    context
}

/// Utility function to extract a code snippet around a specific location
/// 提取特定位置周围代码片段的工具函数
pub fn extract_code_snippet(
    source_file: &SourceFile,
    line_number: usize,
    before_lines: usize,
    after_lines: usize,
) -> Vec<String> {
    let mut snippet = Vec::new();

    // Add lines before
    for i in (1..=before_lines).rev() {
        if line_number > i {
            if let Some(line) = source_file.get_line(line_number - i) {
                snippet.push(line.to_string());
            }
        }
    }

    // Add target line
    if let Some(line) = source_file.get_line(line_number) {
        snippet.push(line.to_string());
    }

    // Add lines after
    for i in 1..=after_lines {
        if let Some(line) = source_file.get_line(line_number + i) {
            snippet.push(line.to_string());
        }
    }

    snippet
}

/// Create a formatted error message with source context
/// 创建带源代码上下文的格式化错误消息
pub fn format_error_with_context(
    error_code: &str,
    error_message: &str,
    suggestion: Option<&str>,
    context: &ErrorContext,
    show_colors: bool,
) -> String {
    let mut result = String::new();

    // Error header with colors
    if show_colors {
        result.push_str("\x1b[31m"); // Red
    }
    result.push_str(&format!("错误[{}]: {}\n", error_code, error_message));
    if show_colors {
        result.push_str("\x1b[0m"); // Reset
    }

    // Source context
    result.push_str(&context.format());

    // Suggestion
    if let Some(suggestion) = suggestion {
        if show_colors {
            result.push_str("\x1b[33m"); // Yellow
        }
        result.push_str(&format!("建议: {}\n", suggestion));
        if show_colors {
            result.push_str("\x1b[0m"); // Reset
        }
    }

    result
}

/// Create a formatted warning message with source context
/// 创建带源代码上下文的格式化警告消息
pub fn format_warning_with_context(
    warning_code: &str,
    warning_message: &str,
    suggestion: Option<&str>,
    context: &ErrorContext,
    show_colors: bool,
) -> String {
    let mut result = String::new();

    // Warning header with colors
    if show_colors {
        result.push_str("\x1b[33m"); // Yellow
    }
    result.push_str(&format!("警告[{}]: {}\n", warning_code, warning_message));
    if show_colors {
        result.push_str("\x1b[0m"); // Reset
    }

    // Compact source context for warnings
    result.push_str(&format!("位置: {}\n", context.format_compact()));

    // Suggestion
    if let Some(suggestion) = suggestion {
        if show_colors {
            result.push_str("\x1b[33m"); // Yellow
        }
        result.push_str(&format!("建议: {}\n", suggestion));
        if show_colors {
            result.push_str("\x1b[0m"); // Reset
        }
    }

    result
}