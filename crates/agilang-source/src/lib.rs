//! Source files, byte spans, and line/column mapping for the native AGILANG compiler.

use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

/// File identifier carried by every [`Span`]. `0` means "the file the caller
/// is rendering against" (the entry file); registered files get ids >= 1.
pub type FileId = u32;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize,
)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    #[serde(default)]
    pub file: FileId,
}

impl Span {
    pub const fn new(start: usize, end: usize) -> Self {
        Self {
            start,
            end,
            file: 0,
        }
    }
    pub const fn empty(at: usize) -> Self {
        Self {
            start: at,
            end: at,
            file: 0,
        }
    }
    /// Returns this span attributed to `file`.
    pub const fn in_file(self, file: FileId) -> Self {
        Self {
            start: self.start,
            end: self.end,
            file,
        }
    }
    pub const fn len(self) -> usize {
        self.end.saturating_sub(self.start)
    }
    pub const fn is_empty(self) -> bool {
        self.start >= self.end
    }
    pub const fn join(self, other: Self) -> Self {
        Self {
            start: if self.start < other.start {
                self.start
            } else {
                other.start
            },
            end: if self.end > other.end {
                self.end
            } else {
                other.end
            },
            file: self.file,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone)]
pub struct SourceFile {
    path: PathBuf,
    text: String,
    line_starts: Vec<usize>,
    file_id: FileId,
}

/// Path -> id and id -> file maps for sources registered for diagnostics.
type Registry = (HashMap<PathBuf, FileId>, HashMap<FileId, SourceFile>);

fn registry() -> &'static Mutex<Registry> {
    static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new((HashMap::new(), HashMap::new())))
}

impl SourceFile {
    pub fn load(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let bytes = fs::read(&path)?;
        let text =
            String::from_utf8(bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        Ok(Self::new(path, text))
    }

    pub fn new(path: impl Into<PathBuf>, text: impl Into<String>) -> Self {
        let text = text.into();
        let mut line_starts = vec![0];
        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i + 1);
            }
        }
        Self {
            path: path.into(),
            text,
            line_starts,
            file_id: 0,
        }
    }

    /// Identifier stamped on spans produced from this file.
    pub fn file_id(&self) -> FileId {
        self.file_id
    }

    /// Registers this file so diagnostics whose spans point into it can be
    /// rendered with the right path and source line, even when reported
    /// against a different (entry) file. Re-registering the same path reuses
    /// its id and refreshes the stored text.
    pub fn registered(mut self) -> Self {
        let mut guard = registry().lock().unwrap_or_else(|e| e.into_inner());
        let (ids, files) = &mut *guard;
        let next = ids.len() as FileId + 1;
        let id = *ids.entry(self.path.clone()).or_insert(next);
        self.file_id = id;
        files.insert(id, self.clone());
        self
    }

    /// Looks up a registered file by id.
    pub fn lookup(id: FileId) -> Option<SourceFile> {
        if id == 0 {
            return None;
        }
        let guard = registry().lock().unwrap_or_else(|e| e.into_inner());
        guard.1.get(&id).cloned()
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn len(&self) -> usize {
        self.text.len()
    }
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }
    pub fn slice(&self, span: Span) -> Option<&str> {
        self.text.get(span.start..span.end)
    }

    pub fn position(&self, offset: usize) -> Position {
        let offset = offset.min(self.text.len());
        let line_index = self
            .line_starts
            .partition_point(|&start| start <= offset)
            .saturating_sub(1);
        Position {
            line: line_index + 1,
            column: offset.saturating_sub(self.line_starts[line_index]) + 1,
        }
    }

    pub fn line_text(&self, line: usize) -> Option<&str> {
        if line == 0 || line > self.line_starts.len() {
            return None;
        }
        let start = self.line_starts[line - 1];
        let end = self
            .line_starts
            .get(line)
            .copied()
            .unwrap_or(self.text.len());
        Some(self.text[start..end].trim_end_matches(['\r', '\n']))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn maps_crlf_and_lf_positions() {
        let source = SourceFile::new("test.agi", "fn main():\r\n    return 0\n");
        assert_eq!(source.position(0), Position { line: 1, column: 1 });
        assert_eq!(source.position(16), Position { line: 2, column: 5 });
    }
}
