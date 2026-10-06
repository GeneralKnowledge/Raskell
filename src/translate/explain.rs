//! `raskell explain` — human-readable translation decisions.

use crate::ir::{self, Module};
use crate::semantic::AnalyzedProgram;

#[derive(Debug, Clone)]
pub struct Explanation {
    pub function: String,
    pub detected: Vec<String>,
    pub translation: Vec<String>,
    pub generated: String,
}

impl Explanation {
    pub fn format(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("Function: {}\n", self.function));
        if !self.detected.is_empty() {
            out.push_str("Detected:\n");
            for d in &self.detected {
                out.push_str(&format!("  {d}\n"));
            }
        }
        if !self.translation.is_empty() {
            out.push_str("Translation:\n");
            for t in &self.translation {
                out.push_str(&format!("  {t}\n"));
            }
        }
        out.push_str("Generated:\n");
        out.push_str(&format!("  {}\n", self.generated));
        out
    }
}

pub fn explain_module(analyzed: &AnalyzedProgram, ir: &Module) -> Vec<Explanation> {
    let _ = analyzed;
    ir.explanations
        .iter()
        .map(|n| Explanation {
            function: n.function.clone(),
            detected: n.detected.clone(),
            translation: n.translation.clone(),
            generated: n.generated_summary.clone(),
        })
        .chain(ir.decls.iter().filter_map(|d| match d {
            ir::Decl::Func(f) if f.notes.is_empty() => Some(Explanation {
                function: f.name.clone(),
                detected: vec!["direct translation".into()],
                translation: vec!["expression lowering".into()],
                generated: format!("{} ...", f.name),
            }),
            _ => None,
        }))
        .collect()
}
