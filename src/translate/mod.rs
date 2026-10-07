//! Semantic translation: Rust AST → Translation IR.
//!
//! This is where Raskell recognises patterns (mutation, iterators, accumulators)
//! and lowers them into idiomatic functional IR.

pub mod explain;
pub mod loop_analysis;
mod lower;
pub mod patterns;

use crate::diagnostics::Diagnostics;
use crate::ir::Module;
use crate::semantic::AnalyzedProgram;

pub use lower::to_ir;
pub use patterns::PatternMatch;

/// Translate an analyzed program into IR.
pub fn translate(analyzed: &AnalyzedProgram) -> anyhow::Result<(Module, Diagnostics)> {
    to_ir(analyzed)
}
