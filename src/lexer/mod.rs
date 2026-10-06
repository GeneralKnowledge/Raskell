//! Lexer module.
//!
//! Raskell uses `syn` for parsing, which includes its own lexer. This module
//! exists for the architectural stage list and for future hand-written lexing
//! of Rust-like dialects or incremental reparsing.

/// Token kinds (reserved for a future hand-written lexer).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    Ident,
    Int,
    Float,
    String,
    Char,
    Keyword,
    Punct,
    Eof,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub text: String,
    pub line: usize,
    pub column: usize,
}

/// Placeholder — production parsing goes through `raskell::parser` / syn.
pub fn tokenize(_source: &str) -> Vec<Token> {
    Vec::new()
}
