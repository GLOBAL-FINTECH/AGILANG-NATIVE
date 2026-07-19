//! Diagnostic system for AGILANG compiler.
//!
//! Provides:
//! - Error reporting with source locations
//! - Labels and hints for error messages
//! - Severity levels (error, warning, note, help)
//! - Multi-span diagnostics
//! - Renderable diagnostic messages

use agilang_source::{SourceFile, Span};
use std::fmt;

/// Severity level of a diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Severity {
    /// Error - compilation cannot continue
    Error,
    /// Warning - compilation can continue but may produce incorrect results
    Warning,
    /// Note - additional information
    Note,
    /// Help - suggestion for fixing the issue
    Help,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Severity::Error => write!(f, "error"),
            Severity::Warning => write!(f, "warning"),
            Severity::Note => write!(f, "note"),
            Severity::Help => write!(f, "help"),
        }
    }
}

/// A labeled span in source code.
#[derive(Debug, Clone)]
pub struct Label {
    /// The span this label applies to
    pub span: Span,
    /// Optional message for this label
    pub message: Option<String>,
}

impl Label {
    /// Create a new label.
    pub fn new(span: Span) -> Self {
        Self {
            span,
            message: None,
        }
    }

    /// Create a label with a message.
    pub fn with_message(span: Span, message: impl Into<String>) -> Self {
        Self {
            span,
            message: Some(message.into()),
        }
    }
}

/// A diagnostic message.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    /// Severity level
    pub severity: Severity,
    /// Primary error message
    pub message: String,
    /// Code associated with this diagnostic (e.g., "E0001")
    pub code: Option<String>,
    /// Labels pointing to relevant source locations
    pub labels: Vec<Label>,
    /// Additional notes
    pub notes: Vec<String>,
    /// Help suggestions
    pub help: Vec<String>,
}

impl Diagnostic {
    /// Create a new error diagnostic.
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            message: message.into(),
            code: None,
            labels: Vec::new(),
            notes: Vec::new(),
            help: Vec::new(),
        }
    }

    /// Create a new warning diagnostic.
    pub fn warning(message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Warning,
            message: message.into(),
            code: None,
            labels: Vec::new(),
            notes: Vec::new(),
            help: Vec::new(),
        }
    }

    /// Create a new note diagnostic.
    pub fn note(message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Note,
            message: message.into(),
            code: None,
            labels: Vec::new(),
            notes: Vec::new(),
            help: Vec::new(),
        }
    }

    /// Set the error code.
    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }

    /// Add a label to this diagnostic.
    pub fn with_label(mut self, label: Label) -> Self {
        self.labels.push(label);
        self
    }

    /// Add a note to this diagnostic.
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    /// Add a help suggestion to this diagnostic.
    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help.push(help.into());
        self
    }

    /// Render this diagnostic as a string.
    pub fn render(&self, source: &SourceFile) -> String {
        let mut output = String::new();

        // Severity and message
        output.push_str(&format!("{}: {}\n", self.severity, self.message));

        // Code if present
        if let Some(code) = &self.code {
            output.push_str(&format!("  [{}]\n", code));
        }

        // Labels with source snippets
        for label in &self.labels {
            if source.snippet(label.span).is_ok() {
                if let Ok(pos) = source.position(label.span.start) {
                    output.push_str(&format!(
                        "  --> {}:{}:{}\n",
                        source.path.display(),
                        pos.line,
                        pos.column
                    ));
                    output.push_str(&format!("   |\n"));
                    output.push_str(&format!("{} | {}\n", pos.line, source.line(label.span.start).unwrap_or("<error>")));
                    output.push_str(&format!("   | {}^\n", " ".repeat(pos.column - 1)));
                }
            }
        }

        // Notes
        for note in &self.notes {
            output.push_str(&format!("  note: {}\n", note));
        }

        // Help suggestions
        for help in &self.help {
            output.push_str(&format!("  help: {}\n", help));
        }

        output
    }
}

/// A collection of diagnostics from compilation.
#[derive(Debug, Clone, Default)]
pub struct Diagnostics {
    /// All diagnostics
    pub diagnostics: Vec<Diagnostic>,
}

impl Diagnostics {
    /// Create a new empty diagnostics collection.
    pub fn new() -> Self {
        Self {
            diagnostics: Vec::new(),
        }
    }

    /// Add a diagnostic.
    pub fn push(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    /// Check if there are any errors.
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }

    /// Get the number of errors.
    pub fn error_count(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .count()
    }

    /// Get the number of warnings.
    pub fn warning_count(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Warning)
            .count()
    }

    /// Render all diagnostics as a string.
    pub fn render(&self, source: &SourceFile) -> String {
        let mut output = String::new();

        for diagnostic in &self.diagnostics {
            output.push_str(&diagnostic.render(source));
            output.push('\n');
        }

        // Summary
        let errors = self.error_count();
        let warnings = self.warning_count();

        if errors > 0 || warnings > 0 {
            output.push_str(&format!(
                "error: could not compile: {} error(s), {} warning(s)\n",
                errors, warnings
            ));
        }

        output
    }
}

impl std::fmt::Display for Diagnostics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} error(s), {} warning(s)", self.error_count(), self.warning_count())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agilang_source::SourceFile;

    #[test]
    fn test_diagnostic_creation() {
        let diag = Diagnostic::error("test error")
            .with_code("E0001")
            .with_note("this is a note")
            .with_help("try this instead");

        assert_eq!(diag.severity, Severity::Error);
        assert_eq!(diag.message, "test error");
        assert_eq!(diag.code, Some("E0001".to_string()));
        assert_eq!(diag.notes.len(), 1);
        assert_eq!(diag.help.len(), 1);
    }

    #[test]
    fn test_label_creation() {
        let label = Label::with_message(Span::new(0, 10), "expected identifier");
        assert_eq!(label.span.start, 0);
        assert_eq!(label.span.end, 10);
        assert_eq!(label.message, Some("expected identifier".to_string()));
    }

    #[test]
    fn test_diagnostics_collection() {
        let mut diags = Diagnostics::new();
        diags.push(Diagnostic::error("error 1"));
        diags.push(Diagnostic::warning("warning 1"));
        diags.push(Diagnostic::error("error 2"));

        assert_eq!(diags.error_count(), 2);
        assert_eq!(diags.warning_count(), 1);
        assert!(diags.has_errors());
    }

    #[test]
    fn test_diagnostic_rendering() {
        let content = b"fn main() -> i32:\n    return 0";
        let source = SourceFile::from_bytes("test.agi".into(), content.to_vec()).unwrap();

        let diag = Diagnostic::error("unexpected token")
            .with_label(Label::new(Span::new(0, 2)))
            .with_code("E0001");

        let rendered = diag.render(&source);
        assert!(rendered.contains("error: unexpected token"));
        assert!(rendered.contains("[E0001]"));
    }
}
