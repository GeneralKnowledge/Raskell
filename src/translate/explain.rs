//! `raskell explain` — human-readable translation decisions.

use crate::ir::{self, Module};
use crate::semantic::AnalyzedProgram;
use crate::translate::lower::camel;
use std::collections::HashSet;

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
                out.push_str(&format!("  • {d}\n"));
            }
        }
        if !self.translation.is_empty() {
            out.push_str("Translation:\n");
            for t in &self.translation {
                out.push_str(&format!("  → {t}\n"));
            }
        }
        out.push_str("Generated shape:\n");
        for line in self.generated.lines() {
            out.push_str(&format!("  {line}\n"));
        }
        out
    }
}

pub fn explain_module(analyzed: &AnalyzedProgram, ir: &Module) -> Vec<Explanation> {
    let _ = analyzed;
    let mut seen = HashSet::new();
    let mut out = Vec::new();

    for n in &ir.explanations {
        seen.insert(n.function.clone());
        let hs_name = camel(&n.function);
        let snippet = function_snippet(ir, &hs_name).unwrap_or_else(|| n.generated_summary.clone());
        out.push(Explanation {
            function: n.function.clone(),
            detected: n.detected.clone(),
            translation: n.translation.clone(),
            generated: snippet,
        });
    }

    for d in &ir.decls {
        if let ir::Decl::Func(f) = d {
            // Skip if already covered under the original Rust name
            let rust_name = ir
                .explanations
                .iter()
                .find(|n| camel(&n.function) == f.name)
                .map(|n| n.function.clone());
            if rust_name.is_some() {
                continue;
            }
            if seen.contains(&f.name) {
                continue;
            }
            seen.insert(f.name.clone());
            let snippet = function_snippet(ir, &f.name)
                .unwrap_or_else(|| format!("{} …", f.name));
            out.push(Explanation {
                function: f.name.clone(),
                detected: vec!["direct translation".into()],
                translation: vec!["expression lowering".into()],
                generated: snippet,
            });
        }
    }
    out
}

fn function_snippet(ir: &Module, hs_name: &str) -> Option<String> {
    for d in &ir.decls {
        if let ir::Decl::Func(f) = d {
            if f.name == hs_name {
                let params: Vec<_> = f.params.iter().map(|(n, _)| n.as_str()).collect();
                let param_s = params.join(" ");
                let body = truncate(&format!("{:?}", f.body), 120);
                return Some(format!("{hs_name} {param_s} = {body}"));
            }
        }
    }
    None
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max])
    }
}
