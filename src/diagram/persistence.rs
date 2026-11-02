//! Bounded, explicit-path persistence for native Diagram Studio documents.

use super::codec::{DiagramCodecError, MAX_DIAGRAM_SESSION_BYTES};
use super::session::DiagramSession;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

pub const MAX_DIAGRAM_DOCUMENT_BYTES: usize = MAX_DIAGRAM_SESSION_BYTES;

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
    pub session: DiagramSession,
    pub disk_digest: [u8; 32],
}

/// Stateless native document IO. Construction and use do not scan directories.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct DiagramDocumentStore;

impl DiagramDocumentStore {
    pub(super) fn open(
        &self,
        path: impl AsRef<Path>,
    ) -> Result<LoadedDiagramDocument, DiagramPersistenceError> {
        let path = path.as_ref();
        let bytes = read_required(path)?;
        let session =
            DiagramSession::decode(&bytes).map_err(|source| DiagramPersistenceError::Codec {
                operation: "decode",
                path: path.to_path_buf(),
                source,
            })?;
        Ok(LoadedDiagramDocument {
            session,
            disk_digest: digest(&bytes),
        })
    }

    /// Save As refuses a path that is already occupied at the preflight check.
    /// The destination is then published using the durable shared atomic writer.
    pub(super) fn save_as_with_writer(
        &self,
        path: &Path,
        session: &DiagramSession,
        writer: impl FnOnce(&Path, &[u8]) -> anyhow::Result<()>,
    ) -> Result<[u8; 32], DiagramPersistenceError> {
        let bytes = encode_for_path(path, session)?;
        ensure_unoccupied(path)?;
        writer(path, &bytes).map_err(|source| DiagramPersistenceError::AtomicWrite {
            path: path.to_path_buf(),
            source,
        })?;
        Ok(digest(&bytes))
    }

    pub(super) fn save_existing_with_writer(
        &self,
        path: &Path,
        expected_digest: [u8; 32],
        session: &DiagramSession,
        writer: impl FnOnce(&Path, &[u8]) -> anyhow::Result<()>,
    ) -> Result<[u8; 32], DiagramPersistenceError> {
        let bytes = encode_for_path(path, session)?;
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
}

fn encode_for_path(
    path: &Path,
    session: &DiagramSession,
) -> Result<Vec<u8>, DiagramPersistenceError> {
    session
        .encode()
        .map_err(|source| DiagramPersistenceError::Codec {
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
        DiagramDocumentStore, DiagramPersistenceError, DiskConflictKind, MAX_DIAGRAM_DOCUMENT_BYTES,
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
}
