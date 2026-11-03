//! Explicit-path, bounded plain Markdown file IO.

use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use crate::notes_markdown::MAX_MARKDOWN_SOURCE_BYTES;

pub const MAX_MARKDOWN_DOCUMENT_BYTES: usize = MAX_MARKDOWN_SOURCE_BYTES;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MarkdownFile(PathBuf);

impl MarkdownFile {
    pub fn new(path: impl AsRef<Path>) -> Result<Self, MarkdownPersistenceError> {
        let path = path.as_ref();
        if !path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
        {
            return Err(MarkdownPersistenceError::WrongExtension(path.to_path_buf()));
        }
        Ok(Self(path.to_path_buf()))
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkdownDiskConflictKind {
    Changed,
    Deleted,
}

#[derive(Debug)]
pub enum MarkdownPersistenceError {
    WrongExtension(PathBuf),
    UnsafePath(PathBuf),
    NotFound(PathBuf),
    OccupiedPath(PathBuf),
    InvalidUtf8(PathBuf),
    Oversized {
        path: PathBuf,
        max_bytes: usize,
    },
    DiskConflict {
        path: PathBuf,
        kind: MarkdownDiskConflictKind,
    },
    Io {
        operation: &'static str,
        path: PathBuf,
        source: io::Error,
    },
    AtomicWrite {
        path: PathBuf,
        source: anyhow::Error,
    },
}

impl std::fmt::Display for MarkdownPersistenceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WrongExtension(path) => write!(
                formatter,
                "Markdown documents require the .md extension: {}",
                path.display()
            ),
            Self::UnsafePath(path) => write!(
                formatter,
                "Markdown document path is not a regular file: {}",
                path.display()
            ),
            Self::NotFound(path) => {
                write!(
                    formatter,
                    "Markdown document was not found: {}",
                    path.display()
                )
            }
            Self::OccupiedPath(path) => write!(
                formatter,
                "refusing to overwrite occupied Markdown path {}",
                path.display()
            ),
            Self::InvalidUtf8(path) => write!(
                formatter,
                "Markdown document is not valid UTF-8: {}",
                path.display()
            ),
            Self::Oversized { path, max_bytes } => write!(
                formatter,
                "Markdown document at {} exceeds the {max_bytes}-byte limit",
                path.display()
            ),
            Self::DiskConflict { path, kind } => match kind {
                MarkdownDiskConflictKind::Changed => write!(
                    formatter,
                    "Markdown document changed outside this session: {}",
                    path.display()
                ),
                MarkdownDiskConflictKind::Deleted => write!(
                    formatter,
                    "Markdown document was deleted outside this session: {}",
                    path.display()
                ),
            },
            Self::Io {
                operation,
                path,
                source,
            } => write!(
                formatter,
                "failed to {operation} Markdown document {}: {source}",
                path.display()
            ),
            Self::AtomicWrite { path, source } => write!(
                formatter,
                "failed to atomically save Markdown document {}: {source}",
                path.display()
            ),
        }
    }
}

impl std::error::Error for MarkdownPersistenceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::AtomicWrite { source, .. } => Some(source.as_ref()),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub(crate) struct LoadedMarkdownDocument {
    pub source: String,
    pub disk_digest: [u8; 32],
}

/// Stateless bounded Markdown IO. Construction does not inspect the filesystem.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct MarkdownDocumentStore;

impl MarkdownDocumentStore {
    pub(crate) fn open(
        &self,
        file: &MarkdownFile,
    ) -> Result<LoadedMarkdownDocument, MarkdownPersistenceError> {
        let bytes = read_required(file.path())?;
        let disk_digest = digest(&bytes);
        let source = String::from_utf8(bytes)
            .map_err(|_| MarkdownPersistenceError::InvalidUtf8(file.path().to_path_buf()))?;
        Ok(LoadedMarkdownDocument {
            source,
            disk_digest,
        })
    }

    pub(crate) fn save_as_with_writer(
        &self,
        file: &MarkdownFile,
        source: &str,
        writer: impl FnOnce(&Path, &[u8]) -> anyhow::Result<()>,
    ) -> Result<[u8; 32], MarkdownPersistenceError> {
        validate_source_size(file.path(), source)?;
        ensure_unoccupied(file.path())?;
        writer(file.path(), source.as_bytes()).map_err(|source| {
            MarkdownPersistenceError::AtomicWrite {
                path: file.path().to_path_buf(),
                source,
            }
        })?;
        Ok(digest(source.as_bytes()))
    }

    pub(crate) fn save_existing_with_writer(
        &self,
        file: &MarkdownFile,
        expected_digest: [u8; 32],
        source: &str,
        writer: impl FnOnce(&Path, &[u8]) -> anyhow::Result<()>,
    ) -> Result<[u8; 32], MarkdownPersistenceError> {
        validate_source_size(file.path(), source)?;
        let current = match read_limited(file.path())? {
            Some(bytes) => bytes,
            None => {
                return Err(MarkdownPersistenceError::DiskConflict {
                    path: file.path().to_path_buf(),
                    kind: MarkdownDiskConflictKind::Deleted,
                });
            }
        };
        if digest(&current) != expected_digest {
            return Err(MarkdownPersistenceError::DiskConflict {
                path: file.path().to_path_buf(),
                kind: MarkdownDiskConflictKind::Changed,
            });
        }
        writer(file.path(), source.as_bytes()).map_err(|source| {
            MarkdownPersistenceError::AtomicWrite {
                path: file.path().to_path_buf(),
                source,
            }
        })?;
        Ok(digest(source.as_bytes()))
    }
}

fn validate_source_size(path: &Path, source: &str) -> Result<(), MarkdownPersistenceError> {
    if source.len() > MAX_MARKDOWN_DOCUMENT_BYTES {
        Err(MarkdownPersistenceError::Oversized {
            path: path.to_path_buf(),
            max_bytes: MAX_MARKDOWN_DOCUMENT_BYTES,
        })
    } else {
        Ok(())
    }
}

fn ensure_unoccupied(path: &Path) -> Result<(), MarkdownPersistenceError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Err(MarkdownPersistenceError::OccupiedPath(path.to_path_buf())),
        Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(MarkdownPersistenceError::Io {
            operation: "inspect",
            path: path.to_path_buf(),
            source,
        }),
    }
}

fn read_required(path: &Path) -> Result<Vec<u8>, MarkdownPersistenceError> {
    read_limited(path)?.ok_or_else(|| MarkdownPersistenceError::NotFound(path.to_path_buf()))
}

fn read_limited(path: &Path) -> Result<Option<Vec<u8>>, MarkdownPersistenceError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(MarkdownPersistenceError::Io {
                operation: "inspect",
                path: path.to_path_buf(),
                source,
            });
        }
    };
    if !metadata.file_type().is_file() {
        return Err(MarkdownPersistenceError::UnsafePath(path.to_path_buf()));
    }
    if metadata.len() > MAX_MARKDOWN_DOCUMENT_BYTES as u64 {
        return Err(MarkdownPersistenceError::Oversized {
            path: path.to_path_buf(),
            max_bytes: MAX_MARKDOWN_DOCUMENT_BYTES,
        });
    }

    let file = File::open(path).map_err(|source| MarkdownPersistenceError::Io {
        operation: "read",
        path: path.to_path_buf(),
        source,
    })?;
    let opened_metadata = file
        .metadata()
        .map_err(|source| MarkdownPersistenceError::Io {
            operation: "inspect opened file",
            path: path.to_path_buf(),
            source,
        })?;
    if !opened_metadata.file_type().is_file() {
        return Err(MarkdownPersistenceError::UnsafePath(path.to_path_buf()));
    }

    let mut bytes =
        Vec::with_capacity(metadata.len().min(MAX_MARKDOWN_DOCUMENT_BYTES as u64) as usize);
    file.take((MAX_MARKDOWN_DOCUMENT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|source| MarkdownPersistenceError::Io {
            operation: "read",
            path: path.to_path_buf(),
            source,
        })?;
    if bytes.len() > MAX_MARKDOWN_DOCUMENT_BYTES {
        return Err(MarkdownPersistenceError::Oversized {
            path: path.to_path_buf(),
            max_bytes: MAX_MARKDOWN_DOCUMENT_BYTES,
        });
    }
    Ok(Some(bytes))
}

fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}
