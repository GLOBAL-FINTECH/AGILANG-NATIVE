//! Structured compiler diagnostics shared by lexer, parser, and semantic stages.

use agilang_source::{SourceFile, Span};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Note,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: &'static str,
    pub severity: Severity,
    pub message: String,
    pub span: Span,
    pub hint: Option<String>,
}

impl Diagnostic {
    pub fn error(code: &'static str, message: impl Into<String>, span: Span) -> Self {
        Self {
            code,
            severity: Severity::Error,
            message: message.into(),
            span,
            hint: None,
        }
    }
    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
    pub fn render(&self, source: &SourceFile) -> String {
        let pos = source.position(self.span.start);
        let line = source.line_text(pos.line).unwrap_or("");
        let marker_width = self.span.len().max(1);
        let marker = format!(
            "{}{}",
            " ".repeat(pos.column.saturating_sub(1)),
            "^".repeat(marker_width.min(80))
        );
        let mut out = format!(
            "{}[{}] at {}:{}:{}: {}\n  {}\n  {}",
            self.severity,
            self.code,
            source.path().display(),
            pos.line,
            pos.column,
            self.message,
            line,
            marker
        );
        if let Some(hint) = &self.hint {
            out.push_str(&format!("\n  hint: {hint}"));
        }
        out
    }
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Note => "note",
        })
    }
}
