//! Raskell — a Rust-to-Haskell transpiler.
//!
//! Architecture:
//! ```text
//! Rust source → Lexer/Parser → Rust AST → Semantic analysis
//!   → Translation IR → Haskell lowering → Haskell AST → .hs
//! ```
//!
//! Raskell does **not** translate Rust into Rust. The target is idiomatic Haskell.

pub mod ast;
pub mod cli;
pub mod diagnostics;
pub mod haskell;
pub mod ir;
pub mod lexer;
pub mod parser;
pub mod semantic;
pub mod translate;

pub use diagnostics::{Diagnostic, DiagnosticKind, Diagnostics};
pub use haskell::pretty::pretty_print;
pub use translate::explain::Explanation;

use anyhow::{Context, Result};
use std::path::Path;

/// Pipeline result after translating a Rust source file.
#[derive(Debug, Clone)]
pub struct TranslationResult {
    pub haskell_source: String,
    pub explanations: Vec<Explanation>,
    pub diagnostics: Diagnostics,
    pub haskell_ast: haskell::ast::HsModule,
}

/// Check whether `source` is a supported Raskell program without emitting Haskell.
pub fn check(source: &str, filename: &str) -> Result<Diagnostics> {
    let (rust_ast, mut diags) = parser::parse_with_diagnostics(source, filename)?;
    if diags.has_errors() {
        return Ok(diags);
    }
    let (analyzed, sem_diags) = semantic::analyze(&rust_ast)?;
    diags.append(sem_diags);
    if diags.has_errors() {
        return Ok(diags);
    }
    let (ir, ir_diags) = translate::to_ir(&analyzed)?;
    diags.append(ir_diags);
    let (_hs, lower_diags) = haskell::lower::lower_module(&ir)?;
    diags.append(lower_diags);
    Ok(diags)
}

/// Translate Rust source into Haskell.
pub fn translate(source: &str, filename: &str) -> Result<TranslationResult> {
    let (rust_ast, mut diags) = parser::parse_with_diagnostics(source, filename)?;
    if diags.has_errors() {
        return Ok(TranslationResult {
            haskell_source: String::new(),
            explanations: Vec::new(),
            diagnostics: diags,
            haskell_ast: haskell::ast::HsModule::empty("Main"),
        });
    }
    let (analyzed, sem_diags) = semantic::analyze(&rust_ast)?;
    diags.append(sem_diags);
    if diags.has_errors() {
        return Ok(TranslationResult {
            haskell_source: String::new(),
            explanations: Vec::new(),
            diagnostics: diags,
            haskell_ast: haskell::ast::HsModule::empty("Main"),
        });
    }
    let (ir, ir_diags) = translate::to_ir(&analyzed)?;
    diags.append(ir_diags);
    if diags.has_errors() {
        return Ok(TranslationResult {
            haskell_source: String::new(),
            explanations: Vec::new(),
            diagnostics: diags,
            haskell_ast: haskell::ast::HsModule::empty("Main"),
        });
    }
    let explanations = translate::explain::explain_module(&analyzed, &ir);
    let (hs_ast, lower_diags) = haskell::lower::lower_module(&ir)?;
    diags.append(lower_diags);
    let haskell_source = pretty_print(&hs_ast);
    Ok(TranslationResult {
        haskell_source,
        explanations,
        diagnostics: diags,
        haskell_ast: hs_ast,
    })
}

/// Translate a file on disk.
pub fn translate_file(path: &Path) -> Result<TranslationResult> {
    let source = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let filename = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown.rs");
    translate(&source, filename)
}

/// Check a file on disk.
pub fn check_file(path: &Path) -> Result<Diagnostics> {
    let source = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let filename = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown.rs");
    check(&source, filename)
}

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
