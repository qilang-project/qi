//! Unicode character handling for Qi language

/// Unicode handler for Chinese characters
pub struct UnicodeHandler {
    // No internal state needed for now
}

impl UnicodeHandler {
    pub fn new() -> Self {
        Self {}
    }

    /// Check if a character is a valid identifier start for Qi language
    pub fn is_identifier_start(&self, c: char) -> bool {
        c.is_alphabetic() || c == '_' || self.is_chinese_char(c)
    }

    /// Check if a character is a valid identifier continuation for Qi language
    pub fn is_identifier_continue(&self, c: char) -> bool {
        c.is_alphanumeric() || c == '_' || self.is_chinese_char(c)
    }

    /// Check if a character is a Chinese character
    pub fn is_chinese_char(&self, c: char) -> bool {
        let code_point = c as u32;

        // CJK Unified Ideographs
        (0x4E00..=0x9FFF).contains(&code_point) ||
        // CJK Unified Ideographs Extension A
        (0x3400..=0x4DBF).contains(&code_point) ||
        // CJK Unified Ideographs Extension B
        (0x20000..=0x2A6DF).contains(&code_point) ||
        // CJK Unified Ideographs Extension C
        (0x2A700..=0x2B73F).contains(&code_point) ||
        // CJK Unified Ideographs Extension D
        (0x2B740..=0x2B81F).contains(&code_point) ||
        // CJK Unified Ideographs Extension E
        (0x2B820..=0x2CEAF).contains(&code_point) ||
        // CJK Unified Ideographs Extension F
        (0x2CEB0..=0x2EBEF).contains(&code_point) ||
        // CJK Unified Ideographs Extension G
        (0x30000..=0x3134F).contains(&code_point) ||
        // CJK Compatibility Ideographs
        (0xF900..=0xFAFF).contains(&code_point) ||
        // CJK Compatibility Ideographs Supplement
        (0x2F800..=0x2FA1F).contains(&code_point)
    }

    /// Check if a character is valid in Qi source files
    pub fn is_valid_source_char(&self, c: char) -> bool {
        // Allow most Unicode characters except control characters
        !c.is_control() || c == '\n' || c == '\r' || c == '\t'
    }

    /// Check if a character is whitespace in Qi language
    pub fn is_whitespace(&self, c: char) -> bool {
        c.is_whitespace()
    }

    /// Get the display width of a character (for column calculations)
    pub fn char_width(&self, c: char) -> usize {
        if self.is_chinese_char(c) {
            2 // Chinese characters typically take 2 columns
        } else {
            1
        }
    }

    /// Check if a string contains only valid Qi source characters
    pub fn validate_source_string(&self, s: &str) -> bool {
        s.chars().all(|c| self.is_valid_source_char(c))
    }

    /// Normalize a string for comparison (handling different Unicode forms)
    pub fn normalize(&self, s: &str) -> String {
        // For now, just return as-is. In a full implementation,
        // we might want to handle Unicode normalization forms
        s.to_string()
    }
}
