//! String Operations Module
//!
//! This module provides string operations for the Qi runtime with both
//! legacy support and modern interface implementations.

use std::sync::Arc;
use std::sync::Mutex;
use std::collections::HashMap;
use crate::runtime::{RuntimeResult, RuntimeError};

/// Legacy string operations (deprecated)
#[derive(Debug)]
pub struct StringOperations {
    _private: (),
}

impl StringOperations {
    /// Create new string operations (deprecated)
    #[deprecated(note = "Use stdlib::StringModule instead")]
    pub fn new() -> Self {
        Self { _private: () }
    }

    /// Concatenate strings (deprecated)
    #[deprecated(note = "Use stdlib::StringModule::concat instead")]
    pub fn concat(&self, strings: &[&str]) -> RuntimeResult<String> {
        let result = strings.join("");
        Ok(result)
    }

    /// Get string length (deprecated)
    #[deprecated(note = "Use stdlib::StringModule::length instead")]
    pub fn length(&self, string: &str) -> RuntimeResult<usize> {
        Ok(string.len())
    }

    /// Check if string contains substring (deprecated)
    #[deprecated(note = "Use stdlib::StringModule::contains instead")]
    pub fn contains(&self, string: &str, substring: &str) -> RuntimeResult<bool> {
        Ok(string.contains(substring))
    }

    /// Get substring (deprecated)
    #[deprecated(note = "Use stdlib::StringModule::substring instead")]
    pub fn substring(&self, string: &str, start: usize, end: usize) -> RuntimeResult<String> {
        if start > end || end > string.len() {
            return Err(RuntimeError::internal_error("索引越界", "索引越界"));
        }
        Ok(string[start..end].to_string())
    }

    /// Convert to uppercase (deprecated)
    #[deprecated(note = "Use stdlib::StringModule::to_uppercase instead")]
    pub fn to_uppercase(&self, string: &str) -> RuntimeResult<String> {
        Ok(string.to_uppercase())
    }

    /// Convert to lowercase (deprecated)
    #[deprecated(note = "Use stdlib::StringModule::to_lowercase instead")]
    pub fn to_lowercase(&self, string: &str) -> RuntimeResult<String> {
        Ok(string.to_lowercase())
    }

    /// Trim whitespace (deprecated)
    #[deprecated(note = "Use stdlib::StringModule::trim instead")]
    pub fn trim(&self, string: &str) -> RuntimeResult<String> {
        Ok(string.trim().to_string())
    }

    /// Split string (deprecated)
    #[deprecated(note = "Use stdlib::StringModule::split instead")]
    pub fn split(&self, string: &str, delimiter: &str) -> RuntimeResult<Vec<String>> {
        let parts: Vec<String> = string.split(delimiter).map(|s| s.to_string()).collect();
        Ok(parts)
    }

    /// Join strings (deprecated)
    #[deprecated(note = "Use stdlib::StringModule::join instead")]
    pub fn join(&self, strings: &[String], separator: &str) -> RuntimeResult<String> {
        Ok(strings.join(separator))
    }

    /// Replace substring (deprecated)
    #[deprecated(note = "Use stdlib::StringModule::replace instead")]
    pub fn replace(&self, string: &str, old: &str, new: &str) -> RuntimeResult<String> {
        Ok(string.replace(old, new))
    }

    /// Check if string starts with prefix (deprecated)
    #[deprecated(note = "Use stdlib::StringModule::starts_with instead")]
    pub fn starts_with(&self, string: &str, prefix: &str) -> RuntimeResult<bool> {
        Ok(string.starts_with(prefix))
    }

    /// Check if string ends with suffix (deprecated)
    #[deprecated(note = "Use stdlib::StringModule::ends_with instead")]
    pub fn ends_with(&self, string: &str, suffix: &str) -> RuntimeResult<bool> {
        Ok(string.ends_with(suffix))
    }

    /// Check if string is empty (deprecated)
    #[deprecated(note = "Use stdlib::StringModule::is_empty instead")]
    pub fn is_empty(&self, string: &str) -> RuntimeResult<bool> {
        Ok(string.is_empty())
    }

    /// Get character at position (deprecated)
    #[deprecated(note = "Use stdlib::StringModule::char_at instead")]
    pub fn char_at(&self, string: &str, index: usize) -> RuntimeResult<char> {
        match string.chars().nth(index) {
            Some(ch) => Ok(ch),
            None => Err(RuntimeError::internal_error("索引越界", "索引越界")),
        }
    }

    /// Find substring position (deprecated)
    #[deprecated(note = "Use stdlib::StringModule::find instead")]
    pub fn find(&self, string: &str, substring: &str) -> RuntimeResult<Option<usize>> {
        Ok(string.find(substring))
    }

    /// Reverse string (deprecated)
    #[deprecated(note = "Use stdlib::StringModule::reverse instead")]
    pub fn reverse(&self, string: &str) -> RuntimeResult<String> {
        Ok(string.chars().rev().collect())
    }

    /// Get character count (Unicode aware) (deprecated)
    #[deprecated(note = "Use stdlib::StringModule::char_count instead")]
    pub fn char_count(&self, string: &str) -> RuntimeResult<usize> {
        Ok(string.chars().count())
    }

    /// Validate UTF-8 (deprecated)
    #[deprecated(note = "Use stdlib::StringModule::is_valid_utf8 instead")]
    pub fn is_valid_utf8(&self, bytes: &[u8]) -> RuntimeResult<bool> {
        Ok(std::str::from_utf8(bytes).is_ok())
    }

    /// Convert bytes to string (deprecated)
    #[deprecated(note = "Use stdlib::StringModule::from_utf8 instead")]
    pub fn from_utf8(&self, bytes: &[u8]) -> RuntimeResult<String> {
        match std::str::from_utf8(bytes) {
            Ok(string) => Ok(string.to_string()),
            Err(_) => Err(RuntimeError::internal_error("无效的UTF-8", "无效的UTF-8")),
        }
    }

    /// Convert string to bytes (deprecated)
    #[deprecated(note = "Use stdlib::StringModule::to_utf8 instead")]
    pub fn to_utf8(&self, string: &str) -> RuntimeResult<Vec<u8>> {
        Ok(string.as_bytes().to_vec())
    }
}

impl Default for StringOperations {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_legacy_string_operations() {
        let ops = StringOperations::new();

        // Test basic operations
        assert_eq!(ops.concat(&["Hello", " ", "World"]).unwrap(), "Hello World");
        assert_eq!(ops.length("Hello").unwrap(), 5);
        assert!(ops.contains("Hello World", "World").unwrap());
        assert_eq!(ops.substring("Hello World", 0, 5).unwrap(), "Hello");
        assert_eq!(ops.to_uppercase("hello").unwrap(), "HELLO");
        assert_eq!(ops.to_lowercase("HELLO").unwrap(), "hello");
        assert_eq!(ops.trim("  hello  ").unwrap(), "hello");
        assert_eq!(ops.split("a,b,c", ",").unwrap(), vec!["a", "b", "c"]);
        assert_eq!(ops.join(&["a".to_string(), "b".to_string()], ",").unwrap(), "a,b");
        assert_eq!(ops.replace("hello world", "world", "there").unwrap(), "hello there");
        assert!(ops.starts_with("hello world", "hello").unwrap());
        assert!(ops.ends_with("hello world", "world").unwrap());
        assert!(!ops.is_empty("hello").unwrap());
        assert!(ops.is_empty("").unwrap());
        assert_eq!(ops.char_at("hello", 1).unwrap(), 'e');
        assert_eq!(ops.find("hello world", "world").unwrap(), Some(6));
        assert_eq!(ops.reverse("hello").unwrap(), "olleh");
        assert_eq!(ops.char_count("你好").unwrap(), 2);
        assert!(ops.is_valid_utf8("hello".as_bytes()).unwrap());
        assert_eq!(ops.from_utf8("hello".as_bytes()).unwrap(), "hello");
        assert_eq!(ops.to_utf8("hello").unwrap(), b"hello".to_vec());
    }

    #[test]
    fn test_error_handling() {
        let ops = StringOperations::new();

        // Test index out of bounds
        assert!(ops.substring("hello", 10, 15).is_err());
        assert!(ops.char_at("hello", 10).is_err());

        // Test invalid UTF-8
        let invalid_utf8 = &[0xFF, 0xFE, 0xFD];
        assert!(ops.from_utf8(invalid_utf8).is_err());
    }
}

/// String encoding types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StringEncoding {
    /// UTF-8 encoding (default)
    Utf8,
    /// UTF-16 encoding
    Utf16,
    /// UTF-32 encoding
    Utf32,
    /// ASCII encoding
    Ascii,
    /// GBK encoding (Chinese)
    Gbk,
    /// Big5 encoding (Traditional Chinese)
    Big5,
}

impl Default for StringEncoding {
    fn default() -> Self {
        Self::Utf8
    }
}

/// Text direction for rendering
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextDirection {
    /// Left to right
    LeftToRight,
    /// Right to left
    RightToLeft,
    /// Top to bottom
    TopToBottom,
}

impl Default for TextDirection {
    fn default() -> Self {
        Self::LeftToRight
    }
}

/// Unicode normalization forms
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StringNormalization {
    /// No normalization
    None,
    /// Canonical decomposition
    NFD,
    /// Canonical composition
    NFC,
    /// Compatibility decomposition
    NFKD,
    /// Compatibility composition
    NFKC,
}

impl Default for StringNormalization {
    fn default() -> Self {
        Self::NFC
    }
}

/// Unified string interface that provides access to all string functionality
#[derive(Debug)]
pub struct StringInterface {
    /// Configuration
    config: Arc<Mutex<StringConfig>>,
    /// String statistics
    stats: Arc<Mutex<StringStats>>,
}

/// String processing configuration
#[derive(Debug, Clone)]
pub struct StringConfig {
    /// Default encoding
    pub default_encoding: StringEncoding,
    /// Default text direction
    pub text_direction: TextDirection,
    /// Case sensitivity
    pub case_sensitive: bool,
    /// Locale for operations
    pub locale: String,
    /// Maximum string length
    pub max_string_length: usize,
}

impl Default for StringConfig {
    fn default() -> Self {
        Self {
            default_encoding: StringEncoding::Utf8,
            text_direction: TextDirection::LeftToRight,
            case_sensitive: true,
            locale: "zh-CN".to_string(),
            max_string_length: 10 * 1024 * 1024, // 10MB
        }
    }
}

/// String operation statistics
#[derive(Debug, Clone, Default)]
pub struct StringStats {
    /// Total string operations performed
    pub total_operations: u64,
    /// Concatenations performed
    pub concatenations: u64,
    /// Substrings extracted
    pub substrings: u64,
    /// String comparisons
    pub comparisons: u64,
    /// Case conversions
    pub case_conversions: u64,
    /// Total characters processed
    pub total_characters: u64,
    /// Total bytes processed
    pub total_bytes: u64,
}

impl StringInterface {
    /// Create new string interface
    pub fn new() -> Self {
        let config = StringConfig::default();

        Self {
            config: Arc::new(Mutex::new(config)),
            stats: Arc::new(Mutex::new(StringStats::default())),
        }
    }

    /// Create string interface with custom configuration
    pub fn with_config(config: StringConfig) -> Self {
        Self {
            config: Arc::new(Mutex::new(config)),
            stats: Arc::new(Mutex::new(StringStats::default())),
        }
    }

    /// Concatenate strings
    pub fn concat(&self, strings: &[String]) -> RuntimeResult<String> {
        self.check_string_lengths(strings)?;

        let mut result = String::new();
        let total_chars = strings.iter().map(|s| s.chars().count()).sum();

        for string in strings {
            result.push_str(string);
        }

        self.record_operation("concat");
        self.record_characters_processed(total_chars);
        self.record_bytes_processed(result.len());

        Ok(result)
    }

    /// Extract substring
    pub fn substring(&self, text: &str, start: usize, length: usize) -> RuntimeResult<String> {
        if start >= text.len() {
            return Err(RuntimeError::validation_error(
                "字符串操作错误",
                &format!("起始位置 {} 超出字符串长度 {}", start, text.len())
            ));
        }

        let chars: Vec<char> = text.chars().collect();
        if start >= chars.len() {
            return Err(RuntimeError::validation_error(
                "字符串操作错误",
                &format!("起始位置 {} 超出字符长度 {}", start, chars.len())
            ));
        }

        let end = std::cmp::min(start + length, chars.len());
        let substring_chars: String = chars[start..end].iter().collect();

        self.record_operation("substring");
        self.record_characters_processed(end - start);
        self.record_bytes_processed(substring_chars.len());

        Ok(substring_chars)
    }

    /// Get string length (characters)
    pub fn length(&self, text: &str) -> RuntimeResult<usize> {
        let length = text.chars().count();

        self.record_operation("length");
        self.record_characters_processed(length);
        self.record_bytes_processed(text.len());

        Ok(length)
    }

    /// Compare two strings
    pub fn compare(&self, a: &str, b: &str) -> RuntimeResult<i32> {
        let config = self.config.lock().unwrap();
        let result = if config.case_sensitive {
            a.cmp(b)
        } else {
            a.to_lowercase().cmp(&b.to_lowercase())
        };

        let order = match result {
            std::cmp::Ordering::Less => -1,
            std::cmp::Ordering::Equal => 0,
            std::cmp::Ordering::Greater => 1,
        };

        self.record_operation("compare");
        self.record_characters_processed(a.chars().count() + b.chars().count());
        self.record_bytes_processed(a.len() + b.len());

        Ok(order)
    }

    /// Convert to uppercase
    pub fn to_uppercase(&self, text: &str) -> RuntimeResult<String> {
        let result = text.to_uppercase();

        self.record_operation("to_uppercase");
        self.record_case_conversion();
        self.record_characters_processed(result.chars().count());
        self.record_bytes_processed(result.len());

        Ok(result)
    }

    /// Convert to lowercase
    pub fn to_lowercase(&self, text: &str) -> RuntimeResult<String> {
        let result = text.to_lowercase();

        self.record_operation("to_lowercase");
        self.record_case_conversion();
        self.record_characters_processed(result.chars().count());
        self.record_bytes_processed(result.len());

        Ok(result)
    }

    /// Initialize the string interface
    pub fn initialize(&self) -> RuntimeResult<()> {
        // Reset statistics
        let mut stats = self.stats.lock().unwrap();
        *stats = StringStats::default();
        Ok(())
    }

    /// Get configuration
    pub fn get_config(&self) -> RuntimeResult<StringConfig> {
        let config = self.config.lock().unwrap();
        Ok(config.clone())
    }

    /// Update configuration
    pub fn update_config(&self, config: StringConfig) -> RuntimeResult<()> {
        *self.config.lock().unwrap() = config;
        Ok(())
    }

    /// Set case sensitivity
    pub fn set_case_sensitive(&self, enabled: bool) -> RuntimeResult<()> {
        self.config.lock().unwrap().case_sensitive = enabled;
        Ok(())
    }

    /// Set locale
    pub fn set_locale(&self, locale: &str) -> RuntimeResult<()> {
        self.config.lock().unwrap().locale = locale.to_string();
        Ok(())
    }

    /// Private helper methods

    fn check_string_lengths(&self, strings: &[String]) -> RuntimeResult<()> {
        let config = self.config.lock().unwrap();
        let total_length: usize = strings.iter().map(|s| s.len()).sum();

        if total_length > config.max_string_length {
            return Err(RuntimeError::validation_error(
                "字符串长度错误",
                &format!("字符串总长度 {} 超过最大限制 {}", total_length, config.max_string_length)
            ));
        }

        Ok(())
    }

    fn record_operation(&self, operation: &str) {
        let mut stats = self.stats.lock().unwrap();
        stats.total_operations += 1;

        match operation {
            "concat" => stats.concatenations += 1,
            "substring" => stats.substrings += 1,
            "compare" => stats.comparisons += 1,
            _ => {}
        }
    }

    fn record_characters_processed(&self, count: usize) {
        let mut stats = self.stats.lock().unwrap();
        stats.total_characters += count as u64;
    }

    fn record_bytes_processed(&self, count: usize) {
        let mut stats = self.stats.lock().unwrap();
        stats.total_bytes += count as u64;
    }

    fn record_case_conversion(&self) {
        let mut stats = self.stats.lock().unwrap();
        stats.case_conversions += 1;
    }
}

impl Default for StringInterface {
    fn default() -> Self {
        Self::new()
    }
}