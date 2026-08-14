//! Encoding-aware text loading, editing, and lossless-policy saving.
//!
//! The engine deliberately contains no UI types. UTF-8 (with or without a
//! BOM) and BOM-marked UTF-16 are editable; other undecodable data is retained
//! as a binary digest/length, rather than being reported as an I/O failure.

use crate::common::atomic_file::save_atomic;
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextEncoding {
    Utf8,
    Utf16Le,
    Utf16Be,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    None,
    Lf,
    CrLf,
    Cr,
    Mixed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileIdentity {
    pub size: u64,
    pub modified: Option<SystemTime>,
    #[cfg(unix)]
    pub device: u64,
    #[cfg(unix)]
    pub inode: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadedContent {
    Text(String),
    /// Binary bytes are not retained. Equality remains available via length
    /// and a deterministic content digest.
    Binary {
        digest: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedTextFile {
    pub display_path: PathBuf,
    pub operation_path: PathBuf,
    pub content: LoadedContent,
    pub encoding: Option<TextEncoding>,
    pub has_bom: bool,
    pub line_ending: LineEnding,
    pub trailing_newline: bool,
    pub size: u64,
    pub modified: Option<SystemTime>,
    pub read_only: bool,
    pub identity: FileIdentity,
}

impl LoadedTextFile {
    pub fn text(&self) -> Option<&str> {
        match &self.content {
            LoadedContent::Text(s) => Some(s),
            _ => None,
        }
    }
    pub fn is_binary(&self) -> bool {
        matches!(self.content, LoadedContent::Binary { .. })
    }
    pub fn binary_equal(&self, other: &Self) -> Option<bool> {
        match (&self.content, &other.content) {
            (LoadedContent::Binary { digest: a }, LoadedContent::Binary { digest: b }) => {
                Some(self.size == other.size && a == b)
            }
            _ => None,
        }
    }
    pub fn is_stale_against(&self, metadata: &fs::Metadata) -> bool {
        self.identity != identity(metadata)
    }
}

pub fn load_text_file(path: impl AsRef<Path>) -> io::Result<LoadedTextFile> {
    load_text_file_as(path.as_ref(), path.as_ref())
}

pub fn load_text_file_as(display: &Path, operation: &Path) -> io::Result<LoadedTextFile> {
    let bytes = fs::read(operation)?;
    let metadata = fs::metadata(operation)?;
    let (content, encoding, bom) = decode(&bytes);
    let (ending, trailing) = content_text_policy(&content);
    Ok(LoadedTextFile {
        display_path: display.to_owned(),
        operation_path: operation.to_owned(),
        content,
        encoding,
        has_bom: bom,
        line_ending: ending,
        trailing_newline: trailing,
        size: metadata.len(),
        modified: metadata.modified().ok(),
        read_only: metadata.permissions().readonly(),
        identity: identity(&metadata),
    })
}

fn identity(m: &fs::Metadata) -> FileIdentity {
    #[cfg(unix)]
    use std::os::unix::fs::MetadataExt;
    FileIdentity {
        size: m.len(),
        modified: m.modified().ok(),
        #[cfg(unix)]
        device: m.dev(),
        #[cfg(unix)]
        inode: m.ino(),
    }
}
fn digest(bytes: &[u8]) -> u64 {
    let mut h = DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish()
}

fn decode(bytes: &[u8]) -> (LoadedContent, Option<TextEncoding>, bool) {
    if let Some(rest) = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]) {
        return match std::str::from_utf8(rest) {
            Ok(s) => (
                LoadedContent::Text(s.into()),
                Some(TextEncoding::Utf8),
                true,
            ),
            Err(_) => (
                LoadedContent::Binary {
                    digest: digest(bytes),
                },
                None,
                true,
            ),
        };
    }
    if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
        let le = bytes.starts_with(&[0xff, 0xfe]);
        let rest = &bytes[2..];
        if !rest.len().is_multiple_of(2) {
            return (
                LoadedContent::Binary {
                    digest: digest(bytes),
                },
                None,
                true,
            );
        }
        let units: Vec<u16> = rest
            .chunks_exact(2)
            .map(|b| {
                if le {
                    u16::from_le_bytes([b[0], b[1]])
                } else {
                    u16::from_be_bytes([b[0], b[1]])
                }
            })
            .collect();
        return match String::from_utf16(&units) {
            Ok(s) => (
                LoadedContent::Text(s),
                Some(if le {
                    TextEncoding::Utf16Le
                } else {
                    TextEncoding::Utf16Be
                }),
                true,
            ),
            Err(_) => (
                LoadedContent::Binary {
                    digest: digest(bytes),
                },
                None,
                true,
            ),
        };
    }
    match std::str::from_utf8(bytes) {
        Ok(s) if !looks_binary(bytes) => (
            LoadedContent::Text(s.into()),
            Some(TextEncoding::Utf8),
            false,
        ),
        _ => (
            LoadedContent::Binary {
                digest: digest(bytes),
            },
            None,
            false,
        ),
    }
}
fn looks_binary(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return false;
    }
    let bad = bytes
        .iter()
        .filter(|&&b| b == 0 || (b < 0x20 && !matches!(b, b'\n' | b'\r' | b'\t' | 0x0c)))
        .count();
    bytes.contains(&0) || bad * 20 > bytes.len()
}
fn content_text_policy(c: &LoadedContent) -> (LineEnding, bool) {
    let LoadedContent::Text(s) = c else {
        return (LineEnding::None, false);
    };
    let mut lf = 0;
    let mut crlf = 0;
    let mut cr = 0;
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\r' {
            if b.get(i + 1) == Some(&b'\n') {
                crlf += 1;
                i += 1
            } else {
                cr += 1
            }
        } else if b[i] == b'\n' {
            lf += 1
        }
        i += 1;
    }
    let kinds = (lf > 0) as u8 + (crlf > 0) as u8 + (cr > 0) as u8;
    let e = if kinds > 1 {
        LineEnding::Mixed
    } else if crlf > 0 {
        LineEnding::CrLf
    } else if lf > 0 {
        LineEnding::Lf
    } else if cr > 0 {
        LineEnding::Cr
    } else {
        LineEnding::None
    };
    (e, s.ends_with('\n') || s.ends_with('\r'))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineEdit {
    pub start: usize,
    pub delete_count: usize,
    pub replacement: Vec<String>,
}
#[derive(Debug, Clone)]
struct UndoRecord {
    before: String,
    after: String,
}

#[derive(Debug, Clone)]
pub struct TextDocument {
    source: String,
    pub revision: u64,
    pub saved_revision: u64,
    pub read_only: bool,
    encoding: TextEncoding,
    has_bom: bool,
    line_ending: LineEnding,
    trailing_newline: bool,
    undo: Vec<UndoRecord>,
    redo: Vec<UndoRecord>,
}
impl TextDocument {
    /// Creates an editable, UTF-8 document for a side which does not exist yet.
    pub fn empty() -> Self {
        Self {
            source: String::new(),
            revision: 0,
            saved_revision: 0,
            read_only: false,
            encoding: TextEncoding::Utf8,
            has_bom: false,
            line_ending: LineEnding::Lf,
            trailing_newline: false,
            undo: vec![],
            redo: vec![],
        }
    }
    #[cfg(test)]
    pub(crate) fn from_test_text(source: impl Into<String>) -> Self {
        let mut document = Self::empty();
        document.source = source.into();
        document
    }
    pub fn from_loaded(file: &LoadedTextFile) -> Option<Self> {
        Some(Self {
            source: file.text()?.into(),
            revision: 0,
            saved_revision: 0,
            read_only: file.read_only,
            encoding: file.encoding?,
            has_bom: file.has_bom,
            line_ending: file.line_ending,
            trailing_newline: file.trailing_newline,
            undo: vec![],
            redo: vec![],
        })
    }
    pub fn source(&self) -> &str {
        &self.source
    }
    pub fn is_dirty(&self) -> bool {
        self.revision != self.saved_revision
    }
    /// Replaces the source as one undo action. UI typing may call this after an
    /// egui edit; structural merge operations deliberately call it only once.
    pub fn replace_source(&mut self, source: String) -> Result<bool, String> {
        if self.read_only {
            return Err("document is read-only".into());
        }
        if self.source == source {
            return Ok(false);
        }
        let before = std::mem::replace(&mut self.source, source);
        self.revision += 1;
        self.undo.push(UndoRecord {
            before,
            after: self.source.clone(),
        });
        self.redo.clear();
        Ok(true)
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn apply_edits(&mut self, edits: &[LineEdit]) -> Result<(), String> {
        if self.read_only {
            return Err("document is read-only".into());
        }
        let before = self.source.clone();
        let mut lines = source_lines(&self.source);
        let mut ordered = edits.to_vec();
        ordered.sort_by_key(|e| std::cmp::Reverse(e.start));
        for e in ordered {
            if e.start > lines.len() || e.start + e.delete_count > lines.len() {
                return Err("line edit is out of bounds".into());
            }
            lines.splice(e.start..e.start + e.delete_count, e.replacement);
        }
        self.source = lines.join("\n");
        if self.source == before {
            return Ok(());
        }
        self.revision += 1;
        self.undo.push(UndoRecord {
            before,
            after: self.source.clone(),
        });
        self.redo.clear();
        Ok(())
    }
    pub fn undo(&mut self) -> bool {
        let Some(r) = self.undo.pop() else {
            return false;
        };
        self.source = r.before.clone();
        self.revision += 1;
        self.redo.push(r);
        true
    }
    pub fn redo(&mut self) -> bool {
        let Some(r) = self.redo.pop() else {
            return false;
        };
        self.source = r.after.clone();
        self.revision += 1;
        self.undo.push(r);
        true
    }
    pub fn save(&mut self, path: &Path) -> anyhow::Result<()> {
        let bytes = self.encoded()?;
        save_atomic(path, &bytes)?;
        self.saved_revision = self.revision;
        Ok(())
    }
    fn encoded(&self) -> anyhow::Result<Vec<u8>> {
        let newline = match self.line_ending {
            LineEnding::CrLf => "\r\n",
            LineEnding::Cr => "\r",
            _ => "\n",
        };
        let mut normalized = source_lines(&self.source).join(newline);
        while normalized.ends_with(['\r', '\n']) {
            normalized.pop();
        }
        if self.trailing_newline {
            normalized.push_str(newline);
        }
        let mut out = vec![];
        match self.encoding {
            TextEncoding::Utf8 => {
                if self.has_bom {
                    out.extend([0xef, 0xbb, 0xbf])
                }
                out.extend(normalized.as_bytes())
            }
            TextEncoding::Utf16Le | TextEncoding::Utf16Be => {
                if self.has_bom {
                    out.extend(if self.encoding == TextEncoding::Utf16Le {
                        [0xff, 0xfe]
                    } else {
                        [0xfe, 0xff]
                    })
                }
                for u in normalized.encode_utf16() {
                    out.extend(if self.encoding == TextEncoding::Utf16Le {
                        u.to_le_bytes()
                    } else {
                        u.to_be_bytes()
                    })
                }
            }
        };
        Ok(out)
    }
}
fn source_lines(s: &str) -> Vec<String> {
    // `str::lines` treats CRLF as one delimiter (splitting on either byte
    // independently would manufacture a blank line between CR and LF).
    let mut v: Vec<_> = s.lines().map(str::to_owned).collect();
    if s.is_empty() {
        v.clear()
    }
    v
}

#[derive(Debug)]
pub struct SaveOutcome {
    pub path: PathBuf,
    pub result: anyhow::Result<()>,
}
pub fn save_all_modified(
    left: (&mut TextDocument, &Path),
    right: (&mut TextDocument, &Path),
) -> [SaveOutcome; 2] {
    fn one(d: &mut TextDocument, p: &Path) -> SaveOutcome {
        let result = if d.is_dirty() { d.save(p) } else { Ok(()) };
        SaveOutcome {
            path: p.into(),
            result,
        }
    }
    [one(left.0, left.1), one(right.0, right.1)]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn policies() {
        assert_eq!(
            content_text_policy(&LoadedContent::Text("a\r\nb\r\n".into())),
            (LineEnding::CrLf, true)
        );
        assert_eq!(
            content_text_policy(&LoadedContent::Text("a\nb\r\n".into())).0,
            LineEnding::Mixed
        )
    }
    #[test]
    fn grouped_revision() {
        let mut d = TextDocument {
            source: "a\nb".into(),
            revision: 0,
            saved_revision: 0,
            read_only: false,
            encoding: TextEncoding::Utf8,
            has_bom: false,
            line_ending: LineEnding::Lf,
            trailing_newline: false,
            undo: vec![],
            redo: vec![],
        };
        d.apply_edits(&[
            LineEdit {
                start: 0,
                delete_count: 1,
                replacement: vec!["x".into()],
            },
            LineEdit {
                start: 1,
                delete_count: 1,
                replacement: vec!["y".into()],
            },
        ])
        .unwrap();
        assert_eq!(d.revision, 1);
        assert_eq!(d.source(), "x\ny");
        assert!(d.undo());
        assert_eq!(d.source(), "a\nb")
    }
}
