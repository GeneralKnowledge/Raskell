//! Diagnostics — first-class compiler errors and warnings.

use std::fmt;

/// Kind of diagnostic message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticKind {
    Error,
    Warning,
    Note,
}

/// A single diagnostic with optional source location.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub kind: DiagnosticKind,
    pub code: String,
    pub message: String,
    pub filename: String,
    pub line: usize,
    pub column: usize,
    pub span_text: Option<String>,
    pub notes: Vec<String>,
}

impl Diagnostic {
    pub fn error(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            kind: DiagnosticKind::Error,
            code: code.into(),
            message: message.into(),
            filename: String::new(),
            line: 0,
            column: 0,
            span_text: None,
            notes: Vec::new(),
        }
    }

    pub fn warning(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            kind: DiagnosticKind::Warning,
            code: code.into(),
            message: message.into(),
            filename: String::new(),
            line: 0,
            column: 0,
            span_text: None,
            notes: Vec::new(),
        }
    }

    pub fn at(mut self, filename: impl Into<String>, line: usize, column: usize) -> Self {
        self.filename = filename.into();
        self.line = line;
        self.column = column;
        self
    }

    pub fn span(mut self, text: impl Into<String>) -> Self {
        self.span_text = Some(text.into());
        self
    }

    pub fn note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    pub fn unsupported(feature: &str, filename: &str, line: usize, column: usize) -> Self {
        Self::error("E0421", format!("unsupported Rust construct: {feature}"))
            .at(filename, line, column)
            .span(feature)
            .note("Raskell cannot currently establish a safe semantic translation for this operation.")
            .note("See the README roadmap for planned support.")
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = match self.kind {
            DiagnosticKind::Error => "error",
            DiagnosticKind::Warning => "warning",
            DiagnosticKind::Note => "note",
        };
        writeln!(f, "{kind}[{}]: {}", self.code, self.message)?;
        if !self.filename.is_empty() && self.line > 0 {
            writeln!(
                f,
                " --> {}:{}:{}",
                self.filename, self.line, self.column.max(1)
            )?;
            if let Some(ref span) = self.span_text {
                writeln!(f, "  |")?;
                writeln!(f, "{:>3} |     {}", self.line, span)?;
                let caret_pad = " ".repeat(self.column.saturating_sub(1).min(40));
                let carets = "^".repeat(span.chars().count().min(40).max(1));
                writeln!(f, "  |     {caret_pad}{carets}")?;
            }
        }
        for note in &self.notes {
            writeln!(f, "  = {note}")?;
        }
        Ok(())
    }
}

/// Collection of diagnostics.
#[derive(Debug, Clone, Default)]
pub struct Diagnostics {
    pub items: Vec<Diagnostic>,
}

impl Diagnostics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, d: Diagnostic) {
        self.items.push(d);
    }

    pub fn extend(&mut self, items: impl IntoIterator<Item = Diagnostic>) {
        self.items.extend(items);
    }

    pub fn has_errors(&self) -> bool {
        self.items
            .iter()
            .any(|d| d.kind == DiagnosticKind::Error)
    }

    pub fn error_count(&self) -> usize {
        self.items
            .iter()
            .filter(|d| d.kind == DiagnosticKind::Error)
            .count()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn append(&mut self, mut other: Diagnostics) {
        self.items.append(&mut other.items);
    }
}

impl fmt::Display for Diagnostics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for d in &self.items {
            write!(f, "{d}")?;
        }
        if self.has_errors() {
            writeln!(
                f,
                "error: aborting due to {} previous error{}",
                self.error_count(),
                if self.error_count() == 1 { "" } else { "s" }
            )?;
        }
        Ok(())
    }
}

impl IntoIterator for Diagnostics {
    type Item = Diagnostic;
    type IntoIter = std::vec::IntoIter<Diagnostic>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.into_iter()
    }
}
