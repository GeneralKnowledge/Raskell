//! Parser — convert Rust source into the Raskell AST via `syn`.
//!
//! We intentionally reuse mature Rust parsing rather than reimplementing the grammar.
//! Unsupported constructs become `Expr::Unsupported` / diagnostics instead of silent mis-translation.

mod convert;

use crate::ast::Program;
use crate::diagnostics::{Diagnostic, Diagnostics};
use anyhow::{bail, Result};

/// Parse Rust source into a Raskell [`Program`].
pub fn parse(source: &str, filename: &str) -> Result<Program> {
    let (program, diags) = parse_with_diagnostics(source, filename)?;
    if diags.has_errors() {
        bail!("{}", diags);
    }
    Ok(program)
}

/// Parse and return diagnostics without failing on unsupported items.
pub fn parse_with_diagnostics(source: &str, filename: &str) -> Result<(Program, Diagnostics)> {
    let mut file: syn::File = syn::parse_str(source).map_err(|e| {
        let span = e.span().start();
        anyhow::anyhow!(
            "{}",
            Diagnostic::error("E0001", format!("parse error: {e}"))
                .at(filename, span.line, span.column)
        )
    })?;
    Ok(convert::convert_file(&mut file, filename))
}

pub use convert::line_col;
