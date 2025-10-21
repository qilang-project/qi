//! Chinese grammar parsing for Qi language using LALRPOP

pub mod ast;
pub mod error;

// Include the generated LALRPOP parser
include!(concat!(env!("OUT_DIR"), "/parser/grammar.rs"));

pub use ast::{
    AstNode, Program, TypeNode, BasicType, LiteralValue, LiteralExpression, IdentifierExpression,
    VariableDeclaration, FunctionDeclaration, ReturnStatement, ExpressionStatement,
    IfStatement, WhileStatement, LoopStatement, ForStatement, BinaryExpression, BinaryOperator,
    AssignmentExpression, FunctionCallExpression, Parameter, ArrayAccessExpression,
    ArrayLiteralExpression, StringConcatExpression, ArrayType, StructDeclaration, StructField,
    EnumDeclaration, EnumVariant, StructType, EnumType, StructLiteralExpression,
    StructFieldValue, FieldAccessExpression
};
pub use error::ParseError;

/// Qi language parser using LALRPOP-generated parser
pub struct Parser {
    _private: (),
}

impl Parser {
    /// Create a new parser
    pub fn new() -> Self {
        Self { _private: () }
    }

    /// Parse source code directly into an AST
    pub fn parse_source(&self, source: &str) -> Result<Program, ParseError> {
        // Preprocess: strip BOM and comments to support fixtures that include them
        fn strip_comments(input: &str) -> String {
            // Strip UTF-8 BOM if present
            let s = if input.starts_with('\u{feff}') {
                &input["\u{feff}".len()..]
            } else {
                input
            };

            let bytes = s.as_bytes();
            let mut out = String::with_capacity(s.len());
            let mut i = 0;
            let n = bytes.len();

            let mut in_line_comment = false;
            let mut in_block_comment = false;
            let mut in_string = false;
            let mut in_char = false;
            let mut escape = false;

            while i < n {
                if in_line_comment {
                    // End of line ends the comment
                    if bytes[i] == b'\n' {
                        in_line_comment = false;
                        out.push('\n');
                    }
                    i += 1;
                    continue;
                }

                if in_block_comment {
                    if i + 1 < n && bytes[i] == b'*' && bytes[i + 1] == b'/' {
                        in_block_comment = false;
                        i += 2;
                    } else {
                        i += 1;
                    }
                    continue;
                }

                if in_string {
                    // Read next UTF-8 char
                    let ch = s[i..].chars().next().unwrap();
                    let len = ch.len_utf8();
                    out.push(ch);
                    if !escape {
                        if ch == '"' {
                            in_string = false;
                        } else if ch == '\\' {
                            escape = true;
                        }
                    } else {
                        // escaped char consumed
                        escape = false;
                    }
                    i += len;
                    continue;
                }

                if in_char {
                    let ch = s[i..].chars().next().unwrap();
                    let len = ch.len_utf8();
                    out.push(ch);
                    if !escape {
                        if ch == '\'' {
                            in_char = false;
                        } else if ch == '\\' {
                            escape = true;
                        }
                    } else {
                        escape = false;
                    }
                    i += len;
                    continue;
                }

                // Not inside any literal or comment
                if i + 1 < n && bytes[i] == b'/' && bytes[i + 1] == b'/' {
                    in_line_comment = true;
                    i += 2;
                    continue;
                }
                if i + 1 < n && bytes[i] == b'/' && bytes[i + 1] == b'*' {
                    in_block_comment = true;
                    i += 2;
                    continue;
                }

                // Handle start of string/char literals or just copy next char
                let ch = s[i..].chars().next().unwrap();
                let len = ch.len_utf8();
                if ch == '"' {
                    in_string = true;
                    escape = false;
                    out.push(ch);
                    i += len;
                    continue;
                }
                if ch == '\'' {
                    in_char = true;
                    escape = false;
                    out.push(ch);
                    i += len;
                    continue;
                }

                out.push(ch);
                i += len;
            }

            out
        }

        let cleaned = strip_comments(source);

        // Use LALRPOP-generated parser with cleaned string input
        use crate::parser::__parse__Program::ProgramParser;
        ProgramParser::new()
            .parse(&cleaned)
            .map_err(|_| ParseError::ParseFailed)
    }

    /// Parse tokens into an AST (legacy method - tokenizes first)
    pub fn parse(&self, tokens: Vec<crate::lexer::Token>) -> Result<Program, ParseError> {
        // Reconstruct source from tokens preserving original structure
        // Use the original span information to maintain proper spacing
        let mut source = String::new();
        let mut last_end = 0;

        for token in &tokens {
            // Preserve spacing between tokens based on original positions
            if token.span.start > last_end {
                // Add the original whitespace/newlines that were between tokens
                // For now, add a space if there was a gap
                source.push(' ');
            }
            source.push_str(&token.text);
            last_end = token.span.end;
        }

        self.parse_source(&source.trim())
    }
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}