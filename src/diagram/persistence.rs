//! Bounded, explicit-path persistence for native Diagram Studio documents.

use super::codec::{
    DiagramCodecError, DiagramFileDocument, DiagramFileDocumentRef, MAX_DIAGRAM_SESSION_BYTES,
    decode_document, encode_document,
};
use super::drawio::MAX_DRAWIO_XML_BYTES;
use super::export::DiagramExportFormat;
use super::mermaid::MAX_MERMAID_SOURCE_BYTES;
use super::session::DiagramSession;
use super::visual::VisualDiagramDocument;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{self, Read, Write};
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

/// A synced same-directory temporary file that can be atomically published
/// later without replacing a destination that became occupied in the meantime.
pub(crate) struct StagedUnoccupiedFile {
    destination: PathBuf,
    display_path: PathBuf,
    verified_parent: Option<(PathBuf, PathBuf)>,
    temporary: tempfile::NamedTempFile,
    digest: [u8; 32],
}

/// A user-selected export path pinned to its current resolved parent before
/// encoding starts. The path is rechecked both when staging and publishing.
pub(crate) struct ResolvedExportDestination {
    display_path: PathBuf,
    destination: PathBuf,
    requested_parent: PathBuf,
    resolved_parent: PathBuf,
    format: DiagramExportFormat,
}

impl StagedUnoccupiedFile {
    pub(crate) fn destination(&self) -> &Path {
        &self.display_path
    }

    #[cfg(test)]
    pub(crate) fn temporary_path_for_test(&self) -> &Path {
        self.temporary.path()
    }

    /// Consume the staged file, publishing it only if the selected destination
    /// is still unoccupied. Dropping an unpublished stage removes its temporary.
    pub(crate) fn publish(self) -> Result<[u8; 32], DiagramPersistenceError> {
        if let Some((requested_parent, expected_parent)) = &self.verified_parent {
            verify_export_parent(&self.display_path, requested_parent, expected_parent)?;
        }
        ensure_unoccupied(&self.destination).map_err(|error| match error {
            DiagramPersistenceError::OccupiedPath(_) => {
                DiagramPersistenceError::OccupiedPath(self.display_path.clone())
            }
            DiagramPersistenceError::Io {
                operation, source, ..
            } => DiagramPersistenceError::Io {
                operation,
                path: self.display_path.clone(),
                source,
            },
            error => error,
        })?;
        self.temporary
            .persist_noclobber(&self.destination)
            .map_err(|error| {
                if error.error.kind() == io::ErrorKind::AlreadyExists {
                    DiagramPersistenceError::OccupiedPath(self.display_path.clone())
                } else {
                    DiagramPersistenceError::AtomicWrite {
                        path: self.display_path.clone(),
                        source: anyhow::anyhow!(error.error),
                    }
                }
            })?;
        Ok(self.digest)
    }
}

/// Stage a format-checked export in a synced temporary adjacent to its final
/// destination. The caller can move this value across a worker boundary and
/// publish it only after its application-level identity checks pass.
pub(crate) fn resolve_export_destination(
    path: &Path,
    format: DiagramExportFormat,
) -> Result<ResolvedExportDestination, DiagramExportPublicationError> {
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
    let destination =
        resolve_export_parent(path).map_err(|error| export_stage_error(path, error))?;
    ensure_unoccupied(&destination.destination).map_err(|error| export_stage_error(path, error))?;
    Ok(ResolvedExportDestination {
        display_path: path.to_path_buf(),
        destination: destination.destination,
        requested_parent: destination.requested_parent,
        resolved_parent: destination.resolved_parent,
        format,
    })
}

pub(crate) fn stage_export_unoccupied(
    destination: ResolvedExportDestination,
    format: DiagramExportFormat,
    bytes: &[u8],
) -> Result<StagedUnoccupiedFile, DiagramExportPublicationError> {
    if destination.format != format {
        return Err(DiagramExportPublicationError::WrongExtension {
            path: destination.display_path,
            expected: format.extension(),
        });
    }
    let display_path = destination.display_path.clone();
    stage_resolved_export_bytes(destination, bytes)
        .map_err(|error| export_stage_error(&display_path, error))
}

struct ResolvedExportParent {
    destination: PathBuf,
    requested_parent: PathBuf,
    resolved_parent: PathBuf,
}

fn resolve_export_parent(path: &Path) -> Result<ResolvedExportParent, DiagramPersistenceError> {
    let display_path = path.to_path_buf();
    let file_name = path
        .file_name()
        .ok_or_else(|| DiagramPersistenceError::UnsafePath(display_path.clone()))?;
    let requested_parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let requested_parent = if requested_parent.is_absolute() {
        requested_parent.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|source| DiagramPersistenceError::Io {
                operation: "resolve export destination",
                path: display_path.clone(),
                source,
            })?
            .join(requested_parent)
    };
    let resolved_parent =
        fs::canonicalize(&requested_parent).map_err(|source| DiagramPersistenceError::Io {
            operation: "resolve export destination",
            path: display_path,
            source,
        })?;
    Ok(ResolvedExportParent {
        destination: resolved_parent.join(file_name),
        requested_parent,
        resolved_parent,
    })
}

pub(super) fn export_stage_error(
    fallback_path: &Path,
    error: DiagramPersistenceError,
) -> DiagramExportPublicationError {
    match error {
        DiagramPersistenceError::OccupiedPath(_) => {
            DiagramExportPublicationError::OccupiedPath(fallback_path.to_path_buf())
        }
        DiagramPersistenceError::Io {
            operation: "inspect",
            path: _,
            source,
        } => DiagramExportPublicationError::Io {
            path: fallback_path.to_path_buf(),
            source,
        },
        DiagramPersistenceError::Io { source, .. } => DiagramExportPublicationError::AtomicWrite {
            path: fallback_path.to_path_buf(),
            source: anyhow::anyhow!(source),
        },
        DiagramPersistenceError::AtomicWrite { source, .. } => {
            DiagramExportPublicationError::AtomicWrite {
                path: fallback_path.to_path_buf(),
                source,
            }
        }
        error => DiagramExportPublicationError::AtomicWrite {
            path: fallback_path.to_path_buf(),
            source: anyhow::anyhow!(error),
        },
    }
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
            Self::UnsafePath(path) => {
                write!(
                    formatter,
                    "native diagram path is not a regular file: {}",
                    path.display()
                )
            }
            Self::NotFound(path) => write!(
                formatter,
                "native diagram was not found: {}",
                path.display()
            ),
            Self::OccupiedPath(path) => write!(
                formatter,
                "refusing to overwrite occupied output path {}",
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

    pub(super) fn save_document_as(
        &self,
        path: &Path,
        document: DiagramFileDocumentRef<'_>,
        after_stage_sync: impl FnOnce(&Path) -> anyhow::Result<()>,
    ) -> Result<[u8; 32], DiagramPersistenceError> {
        let bytes = encode_document_for_path(path, document)?;
        ensure_unoccupied(path)?;
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        fs::create_dir_all(parent).map_err(|source| DiagramPersistenceError::Io {
            operation: "create Save As parent directory",
            path: path.to_path_buf(),
            source,
        })?;
        publish_unoccupied_bytes_after_stage_sync(path, &bytes, after_stage_sync)
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

    /// Save As refuses a path that is already occupied and publishes through
    /// the same-directory durable no-clobber staging path.
    #[cfg(test)]
    pub(super) fn save_as(
        &self,
        path: &Path,
        session: &DiagramSession,
    ) -> Result<[u8; 32], DiagramPersistenceError> {
        self.save_document_as(path, DiagramFileDocumentRef::Mermaid(session), |_| Ok(()))
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

/// Publish a newly converted visual document without replacing any existing
/// destination. The returned digest is for the exact bytes atomically copied.
pub(crate) fn publish_visual_document_copy(
    path: &Path,
    document: &VisualDiagramDocument,
) -> Result<[u8; 32], DiagramPersistenceError> {
    ensure_mld_extension(path)?;
    let bytes = encode_document(DiagramFileDocumentRef::Visual(document)).map_err(|source| {
        DiagramPersistenceError::Codec {
            operation: "encode",
            path: path.to_path_buf(),
            source,
        }
    })?;
    publish_unoccupied_bytes(path, &bytes)
}

#[derive(Debug)]
pub(crate) enum DrawioFileError {
    WrongExtension(PathBuf),
    Persistence(DiagramPersistenceError),
}

impl std::fmt::Display for DrawioFileError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WrongExtension(path) => write!(
                formatter,
                "draw.io file must use the .drawio extension: {}",
                path.display()
            ),
            Self::Persistence(error) => match error {
                DiagramPersistenceError::NotFound(path) => {
                    write!(formatter, "draw.io file was not found: {}", path.display())
                }
                DiagramPersistenceError::UnsafePath(path) => {
                    write!(
                        formatter,
                        "draw.io path is not a regular file: {}",
                        path.display()
                    )
                }
                DiagramPersistenceError::Oversized { path, max_bytes } => write!(
                    formatter,
                    "draw.io file at {} exceeds the {max_bytes}-byte limit",
                    path.display()
                ),
                DiagramPersistenceError::OccupiedPath(path) => write!(
                    formatter,
                    "refusing to overwrite occupied draw.io path {}",
                    path.display()
                ),
                DiagramPersistenceError::Io {
                    operation,
                    path,
                    source,
                } => write!(
                    formatter,
                    "failed to {operation} draw.io file {}: {source}",
                    path.display()
                ),
                other => write!(formatter, "draw.io file operation failed: {other}"),
            },
        }
    }
}

impl std::error::Error for DrawioFileError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Persistence(error) => Some(error),
            _ => None,
        }
    }
}

/// Read a bounded, regular `.drawio` source file without interpreting it on
/// the UI thread. The XML decoder applies its own structural limits to these
/// bytes after the worker receives them.
pub(crate) fn read_drawio_xml_source(path: &Path) -> Result<Vec<u8>, DrawioFileError> {
    if !has_extension(path, "drawio") {
        return Err(DrawioFileError::WrongExtension(path.to_path_buf()));
    }
    read_limited_with_limit(path, MAX_DRAWIO_XML_BYTES)
        .map_err(DrawioFileError::Persistence)?
        .ok_or_else(|| {
            DrawioFileError::Persistence(DiagramPersistenceError::NotFound(path.to_path_buf()))
        })
}

/// Publish already-reviewed draw.io bytes without replacing an existing file.
pub(crate) fn publish_drawio_export(
    path: &Path,
    bytes: &[u8],
) -> Result<[u8; 32], DrawioFileError> {
    if !has_extension(path, "drawio") {
        return Err(DrawioFileError::WrongExtension(path.to_path_buf()));
    }
    if bytes.len() > MAX_DRAWIO_XML_BYTES {
        return Err(DrawioFileError::Persistence(
            DiagramPersistenceError::Oversized {
                path: path.to_path_buf(),
                max_bytes: MAX_DRAWIO_XML_BYTES,
            },
        ));
    }
    publish_unoccupied_bytes(path, bytes).map_err(DrawioFileError::Persistence)
}

/// Copy an explicitly chosen saved Diagram file byte-for-byte. Decode first
/// so malformed/future documents cannot be propagated as a duplicate.
pub(crate) fn duplicate_saved_document(
    source: &Path,
    destination: &Path,
) -> Result<[u8; 32], DiagramPersistenceError> {
    ensure_mld_extension(destination)?;
    let bytes = read_required(source)?;
    decode_document(&bytes).map_err(|source_error| DiagramPersistenceError::Codec {
        operation: "validate for copy",
        path: source.to_path_buf(),
        source: source_error,
    })?;
    publish_unoccupied_bytes(destination, &bytes)
}

fn ensure_mld_extension(path: &Path) -> Result<(), DiagramPersistenceError> {
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("mld"))
    {
        Ok(())
    } else {
        Err(DiagramPersistenceError::Codec {
            operation: "publish copy to a path without .mld extension",
            path: path.to_path_buf(),
            source: DiagramCodecError::InvalidState(
                "a native Diagram copy must use the .mld extension".into(),
            ),
        })
    }
}

fn has_extension(path: &Path, expected: &'static str) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case(expected))
}

fn publish_unoccupied_bytes(
    path: &Path,
    bytes: &[u8],
) -> Result<[u8; 32], DiagramPersistenceError> {
    publish_unoccupied_bytes_after_stage_sync(path, bytes, |_| Ok(()))
}

fn publish_unoccupied_bytes_after_stage_sync(
    path: &Path,
    bytes: &[u8],
    after_stage_sync: impl FnOnce(&Path) -> anyhow::Result<()>,
) -> Result<[u8; 32], DiagramPersistenceError> {
    let stage = stage_unoccupied_file(path.to_path_buf(), path.to_path_buf(), None, bytes)?;
    after_stage_sync(path).map_err(|source| DiagramPersistenceError::AtomicWrite {
        path: path.to_path_buf(),
        source,
    })?;
    stage.publish()
}

fn stage_unoccupied_file(
    destination: PathBuf,
    display_path: PathBuf,
    verified_parent: Option<(PathBuf, PathBuf)>,
    bytes: &[u8],
) -> Result<StagedUnoccupiedFile, DiagramPersistenceError> {
    if let Some((requested, expected)) = &verified_parent {
        verify_export_parent(&display_path, requested, expected)?;
    }
    ensure_unoccupied(&destination)?;
    let parent = destination
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temporary = tempfile::Builder::new()
        .prefix(".diagram-copy-")
        .tempfile_in(parent)
        .map_err(|source| DiagramPersistenceError::Io {
            operation: "create copy temporary file",
            path: display_path.clone(),
            source,
        })?;
    temporary
        .write_all(bytes)
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|source| DiagramPersistenceError::Io {
            operation: "write copy temporary file",
            path: display_path.clone(),
            source,
        })?;
    Ok(StagedUnoccupiedFile {
        destination,
        display_path,
        verified_parent,
        temporary,
        digest: digest(bytes),
    })
}

fn stage_resolved_export_bytes(
    destination: ResolvedExportDestination,
    bytes: &[u8],
) -> Result<StagedUnoccupiedFile, DiagramPersistenceError> {
    stage_unoccupied_file(
        destination.destination,
        destination.display_path,
        Some((destination.requested_parent, destination.resolved_parent)),
        bytes,
    )
}

fn verify_export_parent(
    display_path: &Path,
    requested_parent: &Path,
    expected_parent: &Path,
) -> Result<(), DiagramPersistenceError> {
    let resolved =
        fs::canonicalize(requested_parent).map_err(|source| DiagramPersistenceError::Io {
            operation: "verify export destination",
            path: display_path.to_path_buf(),
            source,
        })?;
    if resolved != expected_parent {
        return Err(DiagramPersistenceError::Io {
            operation: "verify export destination",
            path: display_path.to_path_buf(),
            source: io::Error::other("the selected directory changed after export selection"),
        });
    }
    Ok(())
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
    read_limited_with_limit(path, MAX_DIAGRAM_DOCUMENT_BYTES)
}

fn read_limited_with_limit(
    path: &Path,
    max_bytes: usize,
) -> Result<Option<Vec<u8>>, DiagramPersistenceError> {
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
    if metadata.len() > max_bytes as u64 {
        return Err(DiagramPersistenceError::Oversized {
            path: path.to_path_buf(),
            max_bytes,
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

    let mut bytes = Vec::with_capacity(metadata.len().min(max_bytes as u64) as usize);
    file.take((max_bytes + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|source| DiagramPersistenceError::Io {
            operation: "read",
            path: path.to_path_buf(),
            source,
        })?;
    if bytes.len() > max_bytes {
        return Err(DiagramPersistenceError::Oversized {
            path: path.to_path_buf(),
            max_bytes,
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
        DiagramDocumentStore, DiagramPersistenceError, DiskConflictKind, DrawioFileError,
        MAX_DIAGRAM_DOCUMENT_BYTES, MAX_MERMAID_SOURCE_BYTES, MermaidFile, MermaidImportError,
        duplicate_saved_document, publish_drawio_export, publish_visual_document_copy,
        read_drawio_xml_source,
    };
    use super::{
        DiagramExportPublicationError, resolve_export_destination, stage_export_unoccupied,
    };
    use crate::common::atomic_file::save_atomic;
    use crate::diagram::DiagramExportFormat;
    use crate::diagram::{
        DiagramDocumentController, DiagramSession, MAX_DRAWIO_XML_BYTES, VisualDiagramDocument,
    };
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
        let baseline = store.save_as(&path, &session).unwrap();

        let occupied_bytes = fs::read(&path).unwrap();
        assert!(matches!(
            store.save_as(&path, &session),
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
    fn mermaid_visual_copy_persistence_is_atomic_no_clobber_and_preserves_saved_bytes() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source.mld");
        let duplicate = directory.path().join("duplicate.mld");
        let visual_copy = directory.path().join("visual.mld");

        let mut source_document = DiagramDocumentController::new();
        source_document
            .replace_source("flowchart TD\nA[\"Saved\"]\n")
            .unwrap();
        source_document.save_as(&source).unwrap();
        let original_bytes = fs::read(&source).unwrap();

        duplicate_saved_document(&source, &duplicate).expect("publish exact validated copy");
        assert_eq!(fs::read(&duplicate).unwrap(), original_bytes);
        assert!(matches!(
            duplicate_saved_document(&source, &duplicate),
            Err(DiagramPersistenceError::OccupiedPath(path)) if path == duplicate
        ));

        publish_visual_document_copy(&visual_copy, &VisualDiagramDocument::new())
            .expect("publish a new visual document");
        let loaded = DiagramDocumentStore.open(&visual_copy).unwrap();
        assert!(matches!(
            loaded.document,
            super::super::codec::DiagramFileDocument::Visual(_)
        ));
        assert!(matches!(
            publish_visual_document_copy(&visual_copy, &VisualDiagramDocument::new()),
            Err(DiagramPersistenceError::OccupiedPath(path)) if path == visual_copy
        ));
    }

    #[test]
    fn diagram_drawio_workflow_persistence_is_bounded_and_no_clobber() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("candidate.drawio");
        let output = directory.path().join("export.drawio");
        let bytes = b"<mxfile><diagram/></mxfile>";
        fs::write(&source, bytes).unwrap();
        assert_eq!(read_drawio_xml_source(&source).unwrap(), bytes);
        assert!(matches!(
            read_drawio_xml_source(&directory.path().join("missing.drawio")),
            Err(DrawioFileError::Persistence(
                DiagramPersistenceError::NotFound(_)
            ))
        ));
        assert!(matches!(
            read_drawio_xml_source(&directory.path().join("wrong.xml")),
            Err(DrawioFileError::WrongExtension(_))
        ));
        let directory_with_suffix = directory.path().join("folder.drawio");
        fs::create_dir(&directory_with_suffix).unwrap();
        assert!(matches!(
            read_drawio_xml_source(&directory_with_suffix),
            Err(DrawioFileError::Persistence(
                DiagramPersistenceError::UnsafePath(_)
            ))
        ));

        let too_large = directory.path().join("large.drawio");
        fs::write(&too_large, vec![b' '; MAX_DRAWIO_XML_BYTES + 1]).unwrap();
        assert!(matches!(
            read_drawio_xml_source(&too_large),
            Err(DrawioFileError::Persistence(DiagramPersistenceError::Oversized { max_bytes, .. }))
                if max_bytes == MAX_DRAWIO_XML_BYTES
        ));

        publish_drawio_export(&output, bytes).unwrap();
        let published = fs::read(&output).unwrap();
        assert_eq!(published, bytes);
        assert!(matches!(
            publish_drawio_export(&output, b"replacement"),
            Err(DrawioFileError::Persistence(DiagramPersistenceError::OccupiedPath(path))) if path == output
        ));
        assert_eq!(fs::read(&output).unwrap(), published);
        assert!(matches!(
            publish_drawio_export(&directory.path().join("wrong.svg"), bytes),
            Err(DrawioFileError::WrongExtension(_))
        ));
        let too_large_output = directory.path().join("oversized-output.drawio");
        assert!(matches!(
            publish_drawio_export(&too_large_output, &vec![b' '; MAX_DRAWIO_XML_BYTES + 1]),
            Err(DrawioFileError::Persistence(DiagramPersistenceError::Oversized { max_bytes, .. }))
                if max_bytes == MAX_DRAWIO_XML_BYTES
        ));
        assert!(!too_large_output.exists());
    }

    #[test]
    fn diagram_visual_export_publication_stage_is_destination_bound_and_no_clobber() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("diagram.svg");
        let resolved = resolve_export_destination(&destination, DiagramExportFormat::Svg).unwrap();
        let stage =
            stage_export_unoccupied(resolved, DiagramExportFormat::Svg, b"<svg>staged</svg>")
                .unwrap();
        assert_eq!(stage.destination(), destination);
        let temporary = stage.temporary.path().to_path_buf();
        assert!(temporary.exists());

        let occupied = b"preserve late destination";
        fs::write(&destination, occupied).unwrap();
        assert!(matches!(
            stage.publish(),
            Err(DiagramPersistenceError::OccupiedPath(path)) if path == destination
        ));
        assert_eq!(fs::read(&destination).unwrap(), occupied);
        assert!(
            !temporary.exists(),
            "dropping a rejected stage removes its temp"
        );
    }

    #[test]
    fn diagram_visual_export_publication_rejects_changed_resolved_parent() {
        let directory = tempfile::tempdir().unwrap();
        let original_parent = directory.path().join("original");
        let replacement_parent = directory.path().join("replacement");
        fs::create_dir(&original_parent).unwrap();
        fs::create_dir(&replacement_parent).unwrap();
        let destination = original_parent.join("diagram.png");
        let mut resolved =
            resolve_export_destination(&destination, DiagramExportFormat::Png).unwrap();
        assert_eq!(
            resolved.resolved_parent,
            std::fs::canonicalize(&original_parent).unwrap()
        );
        // Model a selected parent resolving elsewhere before worker staging;
        // this check is deterministic and does not depend on symlink privileges.
        resolved.requested_parent = replacement_parent.clone();

        assert!(matches!(
            stage_export_unoccupied(resolved, DiagramExportFormat::Png, b"png bytes"),
            Err(DiagramExportPublicationError::AtomicWrite { path, .. }) if path == destination
        ));
        assert!(!original_parent.join("diagram.png").exists());
        assert!(!replacement_parent.join("diagram.png").exists());
        let resolved = resolve_export_destination(&destination, DiagramExportFormat::Png).unwrap();
        let mut stage =
            stage_export_unoccupied(resolved, DiagramExportFormat::Png, b"staged png").unwrap();
        let temporary = stage.temporary_path_for_test().to_path_buf();
        stage.verified_parent.as_mut().unwrap().0 = replacement_parent.clone();
        assert!(stage.publish().is_err());
        assert!(!temporary.exists());
        assert!(!original_parent.join("diagram.png").exists());
        assert!(!replacement_parent.join("diagram.png").exists());
    }

    #[test]
    fn diagram_persistence_write_failure_preserves_destination_bytes() {
        let directory = tempfile::tempdir().unwrap();
        let store = DiagramDocumentStore;
        let path = directory.path().join("native.diagram");
        let session = DiagramSession::new();
        let baseline = store.save_as(&path, &session).unwrap();
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
