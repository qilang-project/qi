//! Qi 代码格式化器
//! Code formatter for Qi language (简化版本)

mod config;
mod writer;

pub use config::FormatConfig;
pub use writer::CodeWriter;

/// 格式化器
pub struct Formatter {
    config: FormatConfig,
}

impl Formatter {
    /// 创建新的格式化器
    pub fn new() -> Self {
        Self {
            config: FormatConfig::default(),
        }
    }

    /// 使用自定义配置创建格式化器
    pub fn with_config(config: FormatConfig) -> Self {
        Self { config }
    }

    /// 格式化单个文件（简化版本）
    pub fn format_file(&self, source: &str) -> Result<String, String> {
        // 当前版本：简单的格式化规则
        // TODO: 完整实现需要完整的 AST 遍历
        Ok(self.simple_format(source))
    }

    /// 简单格式化（基于文本处理）
    fn simple_format(&self, source: &str) -> String {
        // 第一步：在 { 和 } 周围添加换行符
        let mut preprocessed = String::new();
        let chars: Vec<char> = source.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            let ch = chars[i];

            match ch {
                '{' | '【' => {
                    preprocessed.push(ch);
                    preprocessed.push('\n');
                }
                '}' | '】' => {
                    // 如果前面不是换行符，先换行
                    if !preprocessed.ends_with('\n') {
                        preprocessed.push('\n');
                    }
                    preprocessed.push(ch);
                    preprocessed.push('\n');
                }
                ';' | '；' => {
                    preprocessed.push(ch);
                    preprocessed.push('\n');
                }
                _ => preprocessed.push(ch),
            }
            i += 1;
        }

        // 第二步：格式化缩进
        let mut result = String::new();
        let mut indent_level = 0;
        let mut prev_was_closing_brace = false;

        for line in preprocessed.lines() {
            let trimmed = line.trim();

            // 跳过空行
            if trimmed.is_empty() {
                continue;
            }

            // 在顶层闭合括号后添加空行（函数之间）
            if prev_was_closing_brace && indent_level == 0 {
                // 如果这是新的顶层声明（不是闭合括号）
                if !trimmed.starts_with('}') && !trimmed.starts_with('】') {
                    result.push('\n');
                }
            }

            // 在 "包" 声明后添加空行
            if trimmed.starts_with("包 ") || trimmed.starts_with("包\t") {
                let indent = self.config.indent_str().repeat(indent_level);
                result.push_str(&indent);
                result.push_str(trimmed);
                result.push('\n');
                result.push('\n'); // 额外的空行
                prev_was_closing_brace = false;
                continue;
            }

            // 减少缩进（遇到闭合括号）
            if trimmed.starts_with('}') || trimmed.starts_with('】') {
                if indent_level > 0 {
                    indent_level -= 1;
                }
                prev_was_closing_brace = true;
            } else {
                prev_was_closing_brace = false;
            }

            // 添加缩进
            let indent = self.config.indent_str().repeat(indent_level);
            result.push_str(&indent);
            result.push_str(trimmed);
            result.push('\n');

            // 增加缩进（遇到开放括号）
            if trimmed.ends_with('{') || trimmed.ends_with('【') {
                indent_level += 1;
            }
        }

        result
    }
}

impl Default for Formatter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_format() {
        let source = "包 测试;\n函数 示例() {\n变量 x = 10;\n}";
        let formatter = Formatter::new();
        let result = formatter.format_file(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_indent() {
        let source = "函数 测试() {\n如果 真 {\n打印(1);\n}\n}";
        let formatter = Formatter::new();
        let result = formatter.format_file(source).unwrap();

        // 检查缩进存在
        assert!(result.contains("    ")); // 至少有一些缩进
    }
}
