//! 格式化配置
//! Formatting configuration

use serde::{Deserialize, Serialize};

/// 格式化配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormatConfig {
    /// 缩进空格数
    #[serde(default = "default_indent")]
    pub indent_size: usize,

    /// 最大行长度
    #[serde(default = "default_max_width")]
    pub max_width: usize,

    /// 是否使用制表符
    #[serde(default)]
    pub use_tabs: bool,

    /// 尾随逗号
    #[serde(default = "default_true")]
    pub trailing_comma: bool,

    /// 运算符周围空格
    #[serde(default = "default_true")]
    pub space_around_operators: bool,

    /// 逗号后空格
    #[serde(default = "default_true")]
    pub space_after_comma: bool,

    /// 冒号后空格
    #[serde(default = "default_true")]
    pub space_after_colon: bool,

    /// 保留空行
    #[serde(default = "default_true")]
    pub preserve_blank_lines: bool,

    /// 导入排序
    #[serde(default = "default_true")]
    pub sort_imports: bool,

    /// 导入分组
    #[serde(default = "default_true")]
    pub group_imports: bool,
}

fn default_indent() -> usize {
    4
}

fn default_max_width() -> usize {
    100
}

fn default_true() -> bool {
    true
}

impl Default for FormatConfig {
    fn default() -> Self {
        Self {
            indent_size: default_indent(),
            max_width: default_max_width(),
            use_tabs: false,
            trailing_comma: true,
            space_around_operators: true,
            space_after_comma: true,
            space_after_colon: true,
            preserve_blank_lines: true,
            sort_imports: true,
            group_imports: true,
        }
    }
}

impl FormatConfig {
    /// 从 TOML 文件加载配置
    pub fn from_file(path: &std::path::Path) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("无法读取配置文件: {}", e))?;

        toml::from_str(&content)
            .map_err(|e| format!("配置文件格式错误: {}", e))
    }

    /// 保存配置到文件
    pub fn save_to_file(&self, path: &std::path::Path) -> Result<(), String> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| format!("序列化配置失败: {}", e))?;

        std::fs::write(path, content)
            .map_err(|e| format!("写入配置文件失败: {}", e))
    }

    /// 获取缩进字符串
    pub fn indent_str(&self) -> String {
        if self.use_tabs {
            "\t".to_string()
        } else {
            " ".repeat(self.indent_size)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = FormatConfig::default();
        assert_eq!(config.indent_size, 4);
        assert_eq!(config.max_width, 100);
        assert!(!config.use_tabs);
        assert!(config.trailing_comma);
    }

    #[test]
    fn test_indent_str() {
        let config = FormatConfig::default();
        assert_eq!(config.indent_str(), "    ");

        let mut tab_config = FormatConfig::default();
        tab_config.use_tabs = true;
        assert_eq!(tab_config.indent_str(), "\t");
    }

    #[test]
    fn test_toml_serialization() {
        let config = FormatConfig::default();
        let toml_str = toml::to_string(&config).unwrap();
        assert!(toml_str.contains("indent_size"));
        assert!(toml_str.contains("max_width"));
    }
}
