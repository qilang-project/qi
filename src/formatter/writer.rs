//! 代码写入器
//! Code writer for formatted output

use super::config::FormatConfig;

/// 代码写入器
pub struct CodeWriter {
    buffer: String,
    indent_level: usize,
    config: FormatConfig,
    at_line_start: bool,
    current_line_length: usize,
}

impl CodeWriter {
    /// 创建新的写入器
    pub fn new(config: FormatConfig) -> Self {
        Self {
            buffer: String::new(),
            indent_level: 0,
            config,
            at_line_start: true,
            current_line_length: 0,
        }
    }

    /// 写入文本
    pub fn write(&mut self, text: &str) -> Result<(), String> {
        if self.at_line_start && !text.is_empty() && text != "\n" {
            self.write_indent()?;
            self.at_line_start = false;
        }

        self.buffer.push_str(text);
        self.current_line_length += text.len();

        Ok(())
    }

    /// 写入缩进
    fn write_indent(&mut self) -> Result<(), String> {
        let indent = self.config.indent_str().repeat(self.indent_level);
        self.buffer.push_str(&indent);
        self.current_line_length += indent.len();
        Ok(())
    }

    /// 换行
    pub fn newline(&mut self) -> Result<(), String> {
        self.buffer.push('\n');
        self.at_line_start = true;
        self.current_line_length = 0;
        Ok(())
    }

    /// 空行
    pub fn blank_line(&mut self) -> Result<(), String> {
        if !self.at_line_start {
            self.newline()?;
        }
        self.buffer.push('\n');
        self.at_line_start = true;
        self.current_line_length = 0;
        Ok(())
    }

    /// 增加缩进
    pub fn indent(&mut self) {
        self.indent_level += 1;
    }

    /// 减少缩进
    pub fn dedent(&mut self) {
        if self.indent_level > 0 {
            self.indent_level -= 1;
        }
    }

    /// 完成写入并返回结果
    pub fn finish(self) -> String {
        self.buffer
    }

    /// 获取当前行长度
    pub fn current_line_len(&self) -> usize {
        self.current_line_length
    }

    /// 检查是否会超过最大行长
    pub fn would_exceed_max_width(&self, additional: usize) -> bool {
        self.current_line_length + additional > self.config.max_width
    }

    /// 写入格式化的列表
    pub fn write_list<T, F>(
        &mut self,
        items: &[T],
        separator: &str,
        writer_fn: F,
    ) -> Result<(), String>
    where
        F: Fn(&mut Self, &T) -> Result<(), String>,
    {
        for (i, item) in items.iter().enumerate() {
            if i > 0 {
                self.write(separator)?;
            }
            writer_fn(self, item)?;
        }
        Ok(())
    }

    /// 写入可能需要换行的列表
    pub fn write_multiline_list<T, F>(
        &mut self,
        items: &[T],
        separator: &str,
        force_multiline: bool,
        writer_fn: F,
    ) -> Result<(), String>
    where
        F: Fn(&mut Self, &T) -> Result<(), String>,
    {
        if items.is_empty() {
            return Ok(());
        }

        let multiline = force_multiline || items.len() > 5;

        if multiline {
            self.newline()?;
            self.indent();
            for (i, item) in items.iter().enumerate() {
                writer_fn(self, item)?;
                if i < items.len() - 1 || self.config.trailing_comma {
                    self.write(separator)?;
                }
                self.newline()?;
            }
            self.dedent();
        } else {
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    self.write(separator)?;
                    self.write(" ")?;
                }
                writer_fn(self, item)?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_write() {
        let mut writer = CodeWriter::new(FormatConfig::default());
        writer.write("测试").unwrap();
        assert_eq!(writer.finish(), "测试");
    }

    #[test]
    fn test_indentation() {
        let mut writer = CodeWriter::new(FormatConfig::default());
        writer.write("第一行").unwrap();
        writer.newline().unwrap();
        writer.indent();
        writer.write("缩进行").unwrap();
        writer.newline().unwrap();
        writer.dedent();
        writer.write("正常行").unwrap();

        let result = writer.finish();
        assert!(result.contains("第一行"));
        assert!(result.contains("    缩进行"));
        assert!(result.contains("正常行"));
    }

    #[test]
    fn test_blank_line() {
        let mut writer = CodeWriter::new(FormatConfig::default());
        writer.write("行1").unwrap();
        writer.newline().unwrap();
        writer.blank_line().unwrap();
        writer.write("行2").unwrap();

        let result = writer.finish();
        assert_eq!(result, "行1\n\n行2");
    }

    #[test]
    fn test_line_length_tracking() {
        let mut writer = CodeWriter::new(FormatConfig::default());
        writer.write("短文本").unwrap();
        assert!(writer.current_line_len() > 0);

        writer.newline().unwrap();
        assert_eq!(writer.current_line_len(), 0);
    }

    #[test]
    fn test_max_width_check() {
        let mut config = FormatConfig::default();
        config.max_width = 10;

        let mut writer = CodeWriter::new(config);
        writer.write("12345").unwrap();
        assert!(!writer.would_exceed_max_width(3)); // 5 + 3 = 8 < 10
        assert!(writer.would_exceed_max_width(10)); // 5 + 10 = 15 > 10
    }

    #[test]
    fn test_write_list() {
        let mut writer = CodeWriter::new(FormatConfig::default());
        let items = vec!["a", "b", "c"];

        writer
            .write_list(&items, ", ", |w, item| w.write(item))
            .unwrap();

        assert_eq!(writer.finish(), "a, b, c");
    }
}
