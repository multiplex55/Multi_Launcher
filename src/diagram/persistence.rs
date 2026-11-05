//! Bounded, explicit-path persistence for native Diagram Studio documents.

use super::codec::{
    DiagramCodecError, DiagramFileDocument, DiagramFileDocumentRef, MAX_DIAGRAM_SESSION_BYTES,
    decode_document, encode_document,
};
use super::export::DiagramExportFormat;
use super::mermaid::MAX_MERMAID_SOURCE_BYTES;
use super::session::DiagramSession;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

pub const MAX_DIAGRAM_DOCUMENT_BYTES: usize = MAX_DIAGRAM_SESSION_BYTES;

#[derive(Debug)]
pub enum DiagramExportPublicationError {
    WrongExtension {
        path: PathBuf,
        expected: &'static str,
    },
    OccupiedPath(PathBuf),
    Io {
        path: PathBuf,
        source: io::Error,
    },
    AtomicWrite {
        path: PathBuf,
        source: anyhow::Error,
    },
}

impl std::fmt::Display for DiagramExportPublicationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WrongExtension { path, expected } => write!(
                formatter,
                "Diagram export requires the .{expected} extension: {}",
                path.display()
            ),
            Self::OccupiedPath(path) => write!(
                formatter,
                "refusing to overwrite occupied Diagram export path {}",
                path.display()
            ),
            Self::Io { path, source } => write!(
                formatter,
                "failed to inspect Diagram export destination {}: {source}",
                path.display()
            ),
            Self::AtomicWrite { path, source } => write!(
                formatter,
                "failed to atomically publish Diagram export {}: {source}",
                path.display()
            ),
        }
    }
}

impl std::error::Error for DiagramExportPublicationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::AtomicWrite { source, .. } => Some(source.as_ref()),
            Self::WrongExtension { .. } | Self::OccupiedPath(_) => None,
        }
    }
}

pub(super) fn publish_export_unoccupied(
    path: &Path,
    format: DiagramExportFormat,
    bytes: &[u8],
    writer: impl FnOnce(&Path, &[u8]) -> anyhow::Result<()>,
) -> Result<(), DiagramExportPublicationError> {
    if !path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case(format.extension()))
    {
        return Err(DiagramExportPublicationError::WrongExtension {
            path: path.to_path_buf(),
            expected: format.extension(),
        });
    }
    match fs::symlink_metadata(path) {
        Ok(_) => {
            return Err(DiagramExportPublicationError::OccupiedPath(
                path.to_path_buf(),
            ));
        }
        Err(source) if source.kind() == io::ErrorKind::NotFound => {}
        Err(source) => {
            return Err(DiagramExportPublicationError::Io {
                path: path.to_path_buf(),
                source,
            });
        }
    }
    // Keep the destination check directly adjacent to the atomic writer. This
    // refuses normal overwrite attempts but does not claim filesystem CAS.
    writer(path, bytes).map_err(|source| DiagramExportPublicationError::AtomicWrite {
        path: path.to_path_buf(),
        source,
    })
}

/// An explicitly selected UTF-8 Mermaid source file with the `.mmd` extension.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MermaidFile(PathBuf);

impl MermaidFile {
    pub fn new(path: impl AsRef<Path>) -> Result<Self, MermaidImportError> {
        let path = path.as_ref();
        if !path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("mmd"))
        {
            return Err(MermaidImportError::WrongExtension(path.to_path_buf()));
        }
        Ok(Self(path.to_path_buf()))
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

#[derive(Debug)]
pub enum MermaidImportError {
    WrongExtension(PathBuf),
    UnsafePath(PathBuf),
    NotFound(PathBuf),
    Oversized {
        path: PathBuf,
        max_bytes: usize,
    },
    InvalidUtf8(PathBuf),
    Io {
        operation: &'static str,
        path: PathBuf,
        source: io::Error,
    },
}

impl std::fmt::Display for MermaidImportError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WrongExtension(path) => write!(
                formatter,
                "Mermaid import requires a .mmd file: {}",
                path.display()
            ),
            Self::UnsafePath(path) => write!(
                formatter,
                "Mermaid source is not a regular file: {}",
                path.display()
            ),
            Self::NotFound(path) => {
                write!(
                    formatter,
                    "Mermaid source was not found: {}",
                    path.display()
                )
            }
            Self::Oversized { path, max_bytes } => write!(
                formatter,
                "Mermaid source at {} exceeds the {max_bytes}-byte limit",
                path.display()
            ),
            Self::InvalidUtf8(path) => write!(
                formatter,
                "Mermaid source is not valid UTF-8: {}",
                path.display()
            ),
            Self::Io {
                operation,
                path,
                source,
            } => write!(
                formatter,
                "failed to {operation} Mermaid source {}: {source}",
                path.display()
            ),
        }
    }
}

impl std::error::Error for MermaidImportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiskConflictKind {
    Changed,
    Deleted,
}

#[derive(Debug)]
pub enum DiagramPersistenceError {
    Io {
        operation: &'static str,
        path: PathBuf,
        source: io::Error,
    },
    UnsafePath(PathBuf),
    NotFound(PathBuf),
    OccupiedPath(PathBuf),
    DiskConflict {
        path: PathBuf,
        kind: DiskConflictKind,
    },
    Oversized {
        path: PathBuf,
        max_bytes: usize,
    },
    Codec {
        operation: &'static str,
        path: PathBuf,
        source: DiagramCodecError,
    },
    AtomicWrite {
        path: PathBuf,
        source: anyhow::Error,
    },
}

impl std::fmt::Display for DiagramPersistenceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io {
                operation,
                path,
                source,
            } => write!(
                formatter,
                "failed to {operation} {}: {source}",
                path.display()
            ),
            Self::UnsafePath(path) => write!(
                formatter,
                "native diagram path is not a regular file: {}",
                path.display()
            ),
            Self::NotFound(path) => {
                write!(
                    formatter,
                    "native diagram was not found: {}",
                    path.display()
                )
            }
            Self::OccupiedPath(path) => write!(
                formatter,
                "refusing to overwrite occupied native diagram path {}",
                path.display()
            ),
            Self::DiskConflict { path, kind } => match kind {
                DiskConflictKind::Changed => write!(
                    formatter,
                    "native diagram changed outside this session: {}",
                    path.display()
                ),
                DiskConflictKind::Deleted => write!(
                    formatter,
                    "native diagram was deleted outside this session: {}",
                    path.display()
                ),
            },
            Self::Oversized { path, max_bytes } => write!(
                formatter,
                "native diagram at {} exceeds the {max_bytes}-byte limit",
                path.display()
            ),
            Self::Codec {
                operation,
                path,
                source,
            } => write!(
                formatter,
                "failed to {operation} native diagram at {}: {source}",
                path.display()
            ),
            Self::AtomicWrite { path, source } => write!(
                formatter,
                "failed to atomically save native diagram {}: {source}",
                path.display()
            ),
        }
    }
}

impl std::error::Error for DiagramPersistenceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Codec { source, .. } => Some(source),
            Self::AtomicWrite { source, .. } => Some(source.as_ref()),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub(super) struct LoadedDiagramDocument {
    pub document: DiagramFileDocument,
    pub disk_digest: [u8; 32],
}

/// Stateless native document IO. Construction and use do not scan directories.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct DiagramDocumentStore;

impl DiagramDocumentStore {
    pub(super) fn read_mermaid(&self, file: &MermaidFile) -> Result<String, MermaidImportError> {
        let path = file.path();
        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(source) if source.kind() == io::ErrorKind::NotFound => {
                return Err(MermaidImportError::NotFound(path.to_path_buf()));
            }
            Err(source) => {
                return Err(MermaidImportError::Io {
                    operation: "inspect",
                    path: path.to_path_buf(),
                    source,
                });
            }
        };
        if !metadata.file_type().is_file() {
            return Err(MermaidImportError::UnsafePath(path.to_path_buf()));
        }
        if metadata.len() > MAX_MERMAID_SOURCE_BYTES as u64 {
            return Err(MermaidImportError::Oversized {
                path: path.to_path_buf(),
                max_bytes: MAX_MERMAID_SOURCE_BYTES,
            });
        }

        let file_handle = File::open(path).map_err(|source| MermaidImportError::Io {
            operation: "read",
            path: path.to_path_buf(),
            source,
        })?;
        let opened_metadata = file_handle
            .metadata()
            .map_err(|source| MermaidImportError::Io {
                operation: "inspect opened file",
                path: path.to_path_buf(),
                source,
            })?;
        if !opened_metadata.file_type().is_file() {
            return Err(MermaidImportError::UnsafePath(path.to_path_buf()));
        }

        let mut bytes =
            Vec::with_capacity(metadata.len().min(MAX_MERMAID_SOURCE_BYTES as u64) as usize);
        file_handle
            .take((MAX_MERMAID_SOURCE_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|source| MermaidImportError::Io {
                operation: "read",
                path: path.to_path_buf(),
                source,
            })?;
        if bytes.len() > MAX_MERMAID_SOURCE_BYTES {
            return Err(MermaidImportError::Oversized {
                path: path.to_path_buf(),
                max_bytes: MAX_MERMAID_SOURCE_BYTES,
            });
        }
        String::from_utf8(bytes).map_err(|_| MermaidImportError::InvalidUtf8(path.to_path_buf()))
    }

    pub(super) fn open(
        &self,
        path: impl AsRef<Path>,
    ) -> Result<LoadedDiagramDocument, DiagramPersistenceError> {
        let path = path.as_ref();
        let bytes = read_required(path)?;
        let document =
            decode_document(&bytes).map_err(|source| DiagramPersistenceError::Codec {
                operation: "decode",
                path: path.to_path_buf(),
                source,
            })?;
        Ok(LoadedDiagramDocument {
            document,
            disk_digest: digest(&bytes),
        })
    }

    pub(super) fn save_document_as_with_writer(
        &self,
        path: &Path,
        document: DiagramFileDocumentRef<'_>,
        writer: impl FnOnce(&Path, &[u8]) -> anyhow::Result<()>,
    ) -> Result<[u8; 32], DiagramPersistenceError> {
        let bytes = encode_document_for_path(path, document)?;
        ensure_unoccupied(path)?;
        writer(path, &bytes).map_err(|source| DiagramPersistenceError::AtomicWrite {
            path: path.to_path_buf(),
            source,
        })?;
        Ok(digest(&bytes))
    }

    pub(super) fn save_document_existing_with_writer(
        &self,
        path: &Path,
        expected_digest: [u8; 32],
        document: DiagramFileDocumentRef<'_>,
        writer: impl FnOnce(&Path, &[u8]) -> anyhow::Result<()>,
    ) -> Result<[u8; 32], DiagramPersistenceError> {
        let bytes = encode_document_for_path(path, document)?;
        let current = match read_limited(path)? {
            Some(bytes) => bytes,
            None => {
                return Err(DiagramPersistenceError::DiskConflict {
                    path: path.to_path_buf(),
                    kind: DiskConflictKind::Deleted,
                });
            }
        };
        if digest(&current) != expected_digest {
            return Err(DiagramPersistenceError::DiskConflict {
                path: path.to_path_buf(),
                kind: DiskConflictKind::Changed,
            });
        }
        writer(path, &bytes).map_err(|source| DiagramPersistenceError::AtomicWrite {
            path: path.to_path_buf(),
            source,
        })?;
        Ok(digest(&bytes))
    }

    /// Save As refuses a path that is already occupied at the preflight check.
    /// The destination is then published using the durable shared atomic writer.
    #[cfg(test)]
    pub(super) fn save_as_with_writer(
        &self,
        path: &Path,
        session: &DiagramSession,
        writer: impl FnOnce(&Path, &[u8]) -> anyhow::Result<()>,
    ) -> Result<[u8; 32], DiagramPersistenceError> {
        self.save_document_as_with_writer(path, DiagramFileDocumentRef::Mermaid(session), writer)
    }

    #[cfg(test)]
    pub(super) fn save_existing_with_writer(
        &self,
        path: &Path,
        expected_digest: [u8; 32],
        session: &DiagramSession,
        writer: impl FnOnce(&Path, &[u8]) -> anyhow::Result<()>,
    ) -> Result<[u8; 32], DiagramPersistenceError> {
        self.save_document_existing_with_writer(
            path,
            expected_digest,
            DiagramFileDocumentRef::Mermaid(session),
            writer,
        )
    }
}

fn encode_document_for_path(
    path: &Path,
    document: DiagramFileDocumentRef<'_>,
) -> Result<Vec<u8>, DiagramPersistenceError> {
    encode_document(document).map_err(|source| DiagramPersistenceError::Codec {
        operation: "encode",
        path: path.to_path_buf(),
        source,
    })
}

fn ensure_unoccupied(path: &Path) -> Result<(), DiagramPersistenceError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Err(DiagramPersistenceError::OccupiedPath(path.to_path_buf())),
        Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(DiagramPersistenceError::Io {
            operation: "inspect",
            path: path.to_path_buf(),
            source,
        }),
    }
}

fn read_required(path: &Path) -> Result<Vec<u8>, DiagramPersistenceError> {
    read_limited(path)?.ok_or_else(|| DiagramPersistenceError::NotFound(path.to_path_buf()))
}

fn read_limited(path: &Path) -> Result<Option<Vec<u8>>, DiagramPersistenceError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(DiagramPersistenceError::Io {
                operation: "inspect",
                path: path.to_path_buf(),
                source,
            });
        }
    };
    if !metadata.file_type().is_file() {
        return Err(DiagramPersistenceError::UnsafePath(path.to_path_buf()));
    }
    if metadata.len() > MAX_DIAGRAM_DOCUMENT_BYTES as u64 {
        return Err(DiagramPersistenceError::Oversized {
            path: path.to_path_buf(),
            max_bytes: MAX_DIAGRAM_DOCUMENT_BYTES,
        });
    }

    let file = File::open(path).map_err(|source| DiagramPersistenceError::Io {
        operation: "read",
        path: path.to_path_buf(),
        source,
    })?;
    let opened_metadata = file
        .metadata()
        .map_err(|source| DiagramPersistenceError::Io {
            operation: "inspect opened file",
            path: path.to_path_buf(),
            source,
        })?;
    if !opened_metadata.file_type().is_file() {
        return Err(DiagramPersistenceError::UnsafePath(path.to_path_buf()));
    }

    let mut bytes =
        Vec::with_capacity(metadata.len().min(MAX_DIAGRAM_DOCUMENT_BYTES as u64) as usize);
    file.take((MAX_DIAGRAM_DOCUMENT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|source| DiagramPersistenceError::Io {
            operation: "read",
            path: path.to_path_buf(),
            source,
        })?;
    if bytes.len() > MAX_DIAGRAM_DOCUMENT_BYTES {
        return Err(DiagramPersistenceError::Oversized {
            path: path.to_path_buf(),
            max_bytes: MAX_DIAGRAM_DOCUMENT_BYTES,
        });
    }
    Ok(Some(bytes))
}

fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

#[cfg(test)]
mod tests {
    use super::{
        DiagramDocumentStore, DiagramPersistenceError, DiskConflictKind,
        MAX_DIAGRAM_DOCUMENT_BYTES, MAX_MERMAID_SOURCE_BYTES, MermaidFile, MermaidImportError,
    };
    use crate::common::atomic_file::save_atomic;
    use crate::diagram::DiagramSession;
    use std::fs;

    #[test]
    fn diagram_persistence_reads_are_bounded_and_require_regular_files() {
        let directory = tempfile::tempdir().unwrap();
        let store = DiagramDocumentStore;
        let missing = directory.path().join("missing.diagram");
        assert!(matches!(
            store.open(&missing),
            Err(DiagramPersistenceError::NotFound(path)) if path == missing
        ));

        let oversized = directory.path().join("oversized.diagram");
        fs::write(&oversized, vec![b' '; MAX_DIAGRAM_DOCUMENT_BYTES + 1]).unwrap();
        assert!(matches!(
            store.open(&oversized),
            Err(DiagramPersistenceError::Oversized { max_bytes, .. })
                if max_bytes == MAX_DIAGRAM_DOCUMENT_BYTES
        ));

        assert!(matches!(
            store.open(directory.path()),
            Err(DiagramPersistenceError::UnsafePath(_))
        ));
    }

    #[test]
    fn diagram_persistence_detects_changed_deleted_and_occupied_paths() {
        let directory = tempfile::tempdir().unwrap();
        let store = DiagramDocumentStore;
        let path = directory.path().join("native.diagram");
        let session = DiagramSession::new();
        let baseline = store
            .save_as_with_writer(&path, &session, save_atomic)
            .unwrap();

        let occupied_bytes = fs::read(&path).unwrap();
        assert!(matches!(
            store.save_as_with_writer(&path, &session, save_atomic),
            Err(DiagramPersistenceError::OccupiedPath(_))
        ));
        assert_eq!(fs::read(&path).unwrap(), occupied_bytes);

        fs::write(&path, b"external replacement").unwrap();
        assert!(matches!(
            store.save_existing_with_writer(&path, baseline, &session, save_atomic),
            Err(DiagramPersistenceError::DiskConflict {
                kind: DiskConflictKind::Changed,
                ..
            })
        ));

        fs::remove_file(&path).unwrap();
        assert!(matches!(
            store.save_existing_with_writer(&path, baseline, &session, save_atomic),
            Err(DiagramPersistenceError::DiskConflict {
                kind: DiskConflictKind::Deleted,
                ..
            })
        ));
    }

    #[test]
    fn diagram_persistence_write_failure_preserves_destination_bytes() {
        let directory = tempfile::tempdir().unwrap();
        let store = DiagramDocumentStore;
        let path = directory.path().join("native.diagram");
        let session = DiagramSession::new();
        let baseline = store
            .save_as_with_writer(&path, &session, save_atomic)
            .unwrap();
        let original = fs::read(&path).unwrap();

        let result = store.save_existing_with_writer(&path, baseline, &session, |_path, _bytes| {
            anyhow::bail!("injected write failure")
        });

        assert!(matches!(
            result,
            Err(DiagramPersistenceError::AtomicWrite { .. })
        ));
        assert_eq!(fs::read(&path).unwrap(), original);
    }

    #[test]
    fn diagram_mermaid_import_reads_exact_utf8_and_rejects_bad_paths_and_bounds() {
        let directory = tempfile::tempdir().unwrap();
        let store = DiagramDocumentStore;
        let source = "\u{feff}%% preserve BOM and CRLF\r\nflowchart TD\r\nA --> B\r\n";
        let path = directory.path().join("draft.mmd");
        fs::write(&path, source.as_bytes()).unwrap();
        let file = MermaidFile::new(&path).unwrap();
        assert_eq!(store.read_mermaid(&file).unwrap(), source);

        assert!(matches!(
            MermaidFile::new(directory.path().join("draft.txt")),
            Err(MermaidImportError::WrongExtension(_))
        ));

        let invalid_utf8 = directory.path().join("invalid.mmd");
        fs::write(&invalid_utf8, [0x66_u8, 0x6c, 0xff]).unwrap();
        assert!(matches!(
            store.read_mermaid(&MermaidFile::new(&invalid_utf8).unwrap()),
            Err(MermaidImportError::InvalidUtf8(path)) if path == invalid_utf8
        ));

        let oversized = directory.path().join("oversized.mmd");
        fs::write(&oversized, vec![b'x'; MAX_MERMAID_SOURCE_BYTES + 1]).unwrap();
        assert!(matches!(
            store.read_mermaid(&MermaidFile::new(&oversized).unwrap()),
            Err(MermaidImportError::Oversized { max_bytes, .. })
                if max_bytes == MAX_MERMAID_SOURCE_BYTES
        ));

        let directory_path = directory.path().join("folder.mmd");
        fs::create_dir(&directory_path).unwrap();
        assert!(matches!(
            store.read_mermaid(&MermaidFile::new(&directory_path).unwrap()),
            Err(MermaidImportError::UnsafePath(_))
        ));
    }
}
