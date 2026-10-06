//! `raskell explain` — human-readable translation decisions.

use crate::haskell::pretty::pretty_print;
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
                out.push_str(&format!("  {d}\n"));
            }
        }
        if !self.translation.is_empty() {
            out.push_str("Semantic transformation:\n");
            for t in &self.translation {
                out.push_str(&format!("  {t}\n"));
            }
        }
        out.push_str("Generated:\n");
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

    // Pretty-print once so snippets can be extracted as real Haskell.
    let hs = crate::haskell::lower_module(ir)
        .ok()
        .map(|(m, _)| pretty_print(&m));

    for n in &ir.explanations {
        seen.insert(n.function.clone());
        let hs_name = camel(&n.function);
        let snippet = hs
            .as_ref()
            .and_then(|src| haskell_function_snippet(src, &hs_name))
            .or_else(|| function_snippet(ir, &hs_name))
            .unwrap_or_else(|| n.generated_summary.clone());
        out.push(Explanation {
            function: n.function.clone(),
            detected: n.detected.clone(),
            translation: n.translation.clone(),
            generated: snippet,
        });
    }

    for d in &ir.decls {
        if let ir::Decl::Func(f) = d {
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
            let snippet = hs
                .as_ref()
                .and_then(|src| haskell_function_snippet(src, &f.name))
                .or_else(|| function_snippet(ir, &f.name))
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

fn haskell_function_snippet(src: &str, hs_name: &str) -> Option<String> {
    let mut lines = Vec::new();
    let mut capturing = false;
    for line in src.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with(&format!("{hs_name} ::")) {
            continue;
        }
        if trimmed.starts_with(&format!("{hs_name} ")) || trimmed == hs_name {
            capturing = true;
            lines.push(line.to_string());
            continue;
        }
        if capturing {
            // Stop at next top-level decl
            if !line.is_empty()
                && !line.starts_with(' ')
                && !line.starts_with('\t')
                && !trimmed.starts_with('|')
            {
                break;
            }
            lines.push(line.to_string());
        }
    }
    if lines.is_empty() {
        None
    } else {
        Some(truncate(&lines.join("\n"), 400))
    }
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
