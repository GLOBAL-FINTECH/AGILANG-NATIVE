//! Source file management for AGILANG compiler.
//!
//! Provides:
//! - UTF-8 source file loading
//! - Byte offsets and line/column calculations
//! - Span tracking
//! - Line maps for error reporting
//! - CRLF/LF handling

use std::path::Path;
use std::sync::Arc;

/// A span in source code, represented as byte offsets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    /// Start byte offset (inclusive)
    pub start: usize,
    /// End byte offset (exclusive)
    pub end: usize,
}

impl Span {
    /// Create a new span from start and end offsets.
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// Create a span at a single position.
    pub fn point(pos: usize) -> Self {
        Self { start: pos, end: pos }
    }

    /// Check if this span contains a byte offset.
    pub fn contains(&self, offset: usize) -> bool {
        offset >= self.start && offset < self.end
    }

    /// Get the length of this span in bytes.
    pub fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }

    /// Check if this span is empty.
    pub fn is_empty(&self) -> bool {
        self.start >= self.end
    }
}

/// A position in source code (line and column).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Position {
    /// Line number (1-based)
    pub line: usize,
    /// Column number (1-based, in UTF-8 characters)
    pub column: usize,
}

impl Position {
    /// Create a new position.
    pub fn new(line: usize, column: usize) -> Self {
        Self { line, column }
    }
}

/// A source file with its contents and line map.
#[derive(Debug, Clone)]
pub struct SourceFile {
    /// The file path
    pub path: Arc<Path>,
    /// The source content as UTF-8 bytes
    pub content: Arc<Vec<u8>>,
    /// Line start offsets (byte offsets where each line begins)
    pub line_starts: Arc<Vec<usize>>,
}

impl SourceFile {
    /// Load a source file from disk.
    pub fn from_path(path: &Path) -> Result<Self, SourceError> {
        let content = std::fs::read(path)
            .map_err(|e| SourceError::Io(path.to_path_buf(), e))?;

        Self::from_bytes(path.to_path_buf(), content)
    }

    /// Create a source file from raw bytes.
    pub fn from_bytes(path: std::path::PathBuf, content: Vec<u8>) -> Result<Self, SourceError> {
        // Validate UTF-8
        let _ = std::str::from_utf8(&content)
            .map_err(|e| SourceError::InvalidUtf8(path.clone(), e))?;

        // Build line map
        let line_starts = Self::build_line_map(&content);

        Ok(Self {
            path: Arc::from(path.as_path()),
            content: Arc::new(content),
            line_starts: Arc::new(line_starts),
        })
    }

    /// Build a map of line start offsets from source content.
    fn build_line_map(content: &[u8]) -> Vec<usize> {
        let mut line_starts = vec![0];
        let mut i = 0;

        while i < content.len() {
            if content[i] == b'\n' {
                line_starts.push(i + 1);
            }
            i += 1;
        }

        line_starts
    }

    /// Get the content as a string slice.
    pub fn as_str(&self) -> &str {
        // Safe because we validated UTF-8 in from_bytes
        unsafe { std::str::from_utf8_unchecked(&self.content) }
    }

    /// Convert a byte offset to a line/column position.
    pub fn position(&self, offset: usize) -> Result<Position, SourceError> {
        if offset > self.content.len() {
            return Err(SourceError::OffsetOutOfBounds(offset, self.content.len()));
        }

        // Binary search for the line
        let line_idx = match self.line_starts.binary_search(&offset) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        };

        let line_start = self.line_starts[line_idx];
        let line_content = &self.content[line_start..offset];
        
        // Count UTF-8 characters for column
        let column = String::from_utf8_lossy(line_content).chars().count() + 1;

        Ok(Position::new(line_idx + 1, column))
    }

    /// Get the line containing a byte offset.
    pub fn line(&self, offset: usize) -> Result<&str, SourceError> {
        let pos = self.position(offset)?;
        let line_idx = pos.line - 1;

        if line_idx >= self.line_starts.len() {
            return Err(SourceError::LineOutOfBounds(pos.line));
        }

        let line_start = self.line_starts[line_idx];
        let line_end = if line_idx + 1 < self.line_starts.len() {
            self.line_starts[line_idx + 1]
        } else {
            self.content.len()
        };

        let mut line_bytes = &self.content[line_start..line_end];
        if line_bytes.ends_with(b"\n") {
            line_bytes = &line_bytes[..line_bytes.len() - 1];
        }
        if line_bytes.ends_with(b"\r") {
            line_bytes = &line_bytes[..line_bytes.len() - 1];
        }
        Ok(std::str::from_utf8(line_bytes).unwrap_or("<invalid utf-8>"))
    }

    /// Get a snippet of source code around a span.
    pub fn snippet(&self, span: Span) -> Result<&str, SourceError> {
        if span.start > self.content.len() || span.end > self.content.len() {
            return Err(SourceError::SpanOutOfBounds(span, self.content.len()));
        }

        let bytes = &self.content[span.start..span.end];
        Ok(std::str::from_utf8(bytes).unwrap_or("<invalid utf-8>"))
    }

    /// Get the total number of lines.
    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }
}

/// Errors that can occur when working with source files.
#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    #[error("IO error reading {0}: {1}")]
    Io(std::path::PathBuf, #[source] std::io::Error),

    #[error("Invalid UTF-8 in {0}: {1}")]
    InvalidUtf8(std::path::PathBuf, #[source] std::str::Utf8Error),

    #[error("Offset {0} out of bounds (content length: {1})")]
    OffsetOutOfBounds(usize, usize),

    #[error("Line {0} out of bounds")]
    LineOutOfBounds(usize),

    #[error("Span {0:?} out of bounds (content length: {1})")]
    SpanOutOfBounds(Span, usize),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_span_creation() {
        let span = Span::new(10, 20);
        assert_eq!(span.start, 10);
        assert_eq!(span.end, 20);
        assert_eq!(span.len(), 10);
        assert!(!span.is_empty());
    }

    #[test]
    fn test_span_point() {
        let span = Span::point(15);
        assert_eq!(span.start, 15);
        assert_eq!(span.end, 15);
        assert!(span.is_empty());
    }

    #[test]
    fn test_span_contains() {
        let span = Span::new(10, 20);
        assert!(span.contains(10));
        assert!(span.contains(15));
        assert!(!span.contains(20));
        assert!(!span.contains(5));
    }

    #[test]
    fn test_line_map_lf() {
        let content = b"line1\nline2\nline3";
        let line_starts = SourceFile::build_line_map(content);
        assert_eq!(line_starts, vec![0, 6, 12]);
    }

    #[test]
    fn test_line_map_crlf() {
        let content = b"line1\r\nline2\r\nline3";
        let line_starts = SourceFile::build_line_map(content);
        assert_eq!(line_starts, vec![0, 7, 14]);
    }

    #[test]
    fn test_position_calculation() {
        let content = b"line1\nline2\nline3";
        let source = SourceFile::from_bytes("test.agi".into(), content.to_vec()).unwrap();

        let pos = source.position(0).unwrap();
        assert_eq!(pos.line, 1);
        assert_eq!(pos.column, 1);

        let pos = source.position(5).unwrap();
        assert_eq!(pos.line, 1);
        assert_eq!(pos.column, 6);

        let pos = source.position(6).unwrap();
        assert_eq!(pos.line, 2);
        assert_eq!(pos.column, 1);
    }

    #[test]
    fn test_line_retrieval() {
        let content = b"line1\nline2\nline3";
        let source = SourceFile::from_bytes("test.agi".into(), content.to_vec()).unwrap();

        assert_eq!(source.line(0).unwrap(), "line1");
        assert_eq!(source.line(6).unwrap(), "line2");
        assert_eq!(source.line(12).unwrap(), "line3");
    }

    #[test]
    fn test_snippet() {
        let content = b"hello world";
        let source = SourceFile::from_bytes("test.agi".into(), content.to_vec()).unwrap();

        let snippet = source.snippet(Span::new(0, 5)).unwrap();
        assert_eq!(snippet, "hello");

        let snippet = source.snippet(Span::new(6, 11)).unwrap();
        assert_eq!(snippet, "world");
    }

    #[test]
    fn test_invalid_utf8() {
        let content = vec![0xFF, 0xFE];
        let result = SourceFile::from_bytes("test.agi".into(), content);
        assert!(result.is_err());
    }
}
