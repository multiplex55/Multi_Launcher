//! Handle-bound review and publication for one explicitly chosen PDF file.
use super::MAX_OUTPUT_BYTES;
use crate::archive::{ArchiveLimits, EntryKind, SafeRelativePath};
use std::{
    fmt,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PdfTargetKind {
    New,
    Replace,
}
#[derive(Debug)]
pub enum PdfPublishError {
    InvalidDestination,
    Unsupported,
    Cancelled,
    Stale,
    ProtectedSource,
    Io(String),
    Recovery { message: String, backup: PathBuf },
}
impl fmt::Display for PdfPublishError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDestination => write!(
                f,
                "Choose an absolute local PDF file with a safe literal name."
            ),
            Self::Unsupported => write!(
                f,
                "Guarded PDF publication is unavailable on this platform."
            ),
            Self::Cancelled => write!(f, "PDF publication cancelled."),
            Self::Stale => write!(f, "The reviewed PDF destination or source changed."),
            Self::ProtectedSource => write!(f, "The PDF destination is the active source file."),
            Self::Io(message) => write!(f, "{message}"),
            Self::Recovery { message, backup } => {
                write!(f, "{message}; the original remains at {}", backup.display())
            }
        }
    }
}
impl std::error::Error for PdfPublishError {}
#[derive(Debug)]
pub struct PdfPublishOutcome {
    pub backup_retained: Option<PathBuf>,
}

#[cfg(windows)]
mod windows_impl {
    use super::*;
    use crate::archive::print_publication::{Directory, Revision};
    use crate::file_operations::NativeFileIdentity;
    use std::{
        ffi::OsString,
        os::windows::ffi::OsStrExt,
        sync::atomic::{AtomicU64, Ordering},
    };
    static NEXT_BACKUP: AtomicU64 = AtomicU64::new(1);
    #[derive(Debug)]
    pub struct PdfDestinationReview {
        parent: PathBuf,
        leaf: OsString,
        parent_id: NativeFileIdentity,
        target: Option<TargetObservation>,
        source: Option<SourceObservation>,
    }
    #[derive(Debug)]
    struct TargetObservation {
        revision: Revision,
        digest: [u8; 32],
    }
    #[derive(Debug)]
    struct SourceObservation {
        parent: PathBuf,
        leaf: OsString,
        parent_id: NativeFileIdentity,
        identity: Option<NativeFileIdentity>,
    }
    #[derive(Debug)]
    pub struct PdfPublishPermit(PdfDestinationReview);
    impl PdfDestinationReview {
        pub fn kind(&self) -> PdfTargetKind {
            if self.target.is_some() {
                PdfTargetKind::Replace
            } else {
                PdfTargetKind::New
            }
        }
        pub fn path(&self) -> PathBuf {
            self.parent.join(&self.leaf)
        }
        /// Call only after any required explicit overwrite confirmation.
        pub fn authorize(self) -> PdfPublishPermit {
            PdfPublishPermit(self)
        }
    }
    fn io(error: std::io::Error) -> PdfPublishError {
        if error.kind() == std::io::ErrorKind::Interrupted {
            PdfPublishError::Cancelled
        } else {
            PdfPublishError::Io(error.to_string())
        }
    }
    fn source_observation(
        source: Option<&Path>,
    ) -> Result<Option<SourceObservation>, PdfPublishError> {
        source
            .map(|path| {
                let absolute = std::path::absolute(path).map_err(io)?;
                let parent = absolute
                    .parent()
                    .ok_or(PdfPublishError::InvalidDestination)?
                    .to_owned();
                let leaf = absolute
                    .file_name()
                    .ok_or(PdfPublishError::InvalidDestination)?
                    .to_os_string();
                let directory = Directory::open(&parent, false).map_err(io)?;
                let parent_id = directory.identity().map_err(io)?;
                let identity = directory
                    .source_leaf(&leaf, false)
                    .map_err(io)?
                    .map(|leaf| {
                        leaf.revision()
                            .map(|revision| revision.identity)
                            .map_err(io)
                    })
                    .transpose()?;
                Ok(SourceObservation {
                    parent,
                    leaf,
                    parent_id,
                    identity,
                })
            })
            .transpose()
    }
    fn validate(path: &Path) -> Result<(PathBuf, OsString), PdfPublishError> {
        if !path.is_absolute() {
            return Err(PdfPublishError::InvalidDestination);
        }
        let raw: Vec<u16> = path.as_os_str().encode_wide().collect();
        let terminal = raw
            .rsplit(|unit| *unit == b'\\' as u16 || *unit == b'/' as u16)
            .next()
            .unwrap_or_default();
        let parsed: Vec<u16> = path
            .file_name()
            .ok_or(PdfPublishError::InvalidDestination)?
            .encode_wide()
            .collect();
        if terminal.is_empty() || terminal != parsed {
            return Err(PdfPublishError::InvalidDestination);
        }
        let parent = path
            .parent()
            .ok_or(PdfPublishError::InvalidDestination)?
            .to_owned();
        let leaf = path
            .file_name()
            .and_then(|v| v.to_str())
            .ok_or(PdfPublishError::InvalidDestination)?;
        let safe = SafeRelativePath::validate(leaf, EntryKind::File, &ArchiveLimits::default())
            .map_err(|_| PdfPublishError::InvalidDestination)?;
        if safe.component_count() != 1
            || !leaf
                .rsplit_once('.')
                .is_some_and(|(name, ext)| !name.is_empty() && ext.eq_ignore_ascii_case("pdf"))
        {
            return Err(PdfPublishError::InvalidDestination);
        }
        Ok((parent, OsString::from(leaf)))
    }
    pub fn review_pdf_destination(
        path: &Path,
        source: Option<&Path>,
    ) -> Result<PdfDestinationReview, PdfPublishError> {
        review_pdf_destination_with_cancel(path, source, || false)
    }

    pub fn review_pdf_destination_with_cancel(
        path: &Path,
        source: Option<&Path>,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<PdfDestinationReview, PdfPublishError> {
        if cancelled() {
            return Err(PdfPublishError::Cancelled);
        }
        let (parent, leaf) = validate(path)?;
        let directory = Directory::open(&parent, false).map_err(io)?;
        let parent_id = directory.identity().map_err(io)?;
        let source = source_observation(source)?;
        let target_meta = directory.source_leaf(&leaf, false).map_err(io)?;
        let target_revision = target_meta
            .as_ref()
            .map(|leaf| leaf.revision().map_err(io))
            .transpose()?;
        if source.as_ref().is_some_and(|source| {
            (source.parent_id == parent_id
                && crate::batch_rename::compare_paths_windows_ordinal(
                    Path::new(&source.leaf),
                    Path::new(&leaf),
                )
                .is_eq())
                || source
                    .identity
                    .zip(target_revision.as_ref().map(|target| target.identity))
                    .is_some_and(|(a, b)| a == b)
        }) {
            return Err(PdfPublishError::ProtectedSource);
        }
        if let Some(target) = &target_revision {
            crate::archive::print_publication::validate_pdf_attributes(target.attributes)
                .map_err(io)?;
        }
        drop(target_meta);
        let target_leaf = if target_revision.is_some() {
            directory.leaf(&leaf, false).map_err(io)?
        } else {
            None
        };
        if target_revision.is_some() != target_leaf.is_some() {
            return Err(PdfPublishError::Stale);
        }
        let target = target_leaf
            .map(|leaf| {
                leaf.validate_security().map_err(io)?;
                let (revision, digest) = leaf.fingerprint(&mut cancelled).map_err(io)?;
                if Some(&revision) != target_revision.as_ref() {
                    return Err(PdfPublishError::Stale);
                }
                Ok(TargetObservation { revision, digest })
            })
            .transpose()?;
        if cancelled() {
            return Err(PdfPublishError::Cancelled);
        }
        Ok(PdfDestinationReview {
            parent,
            leaf,
            parent_id,
            target,
            source,
        })
    }
    fn abort(
        stage: crate::archive::print_publication::Stage,
        parent: &Path,
        primary: PdfPublishError,
    ) -> PdfPublishError {
        let stage_name = stage.name();
        let identity = stage.identity();
        match stage.abort() {
            Ok(()) => primary,
            Err(error) => PdfPublishError::Io(format!(
                "{primary}; stage cleanup failed: {error}; owned stage {} (identity {identity:?})",
                parent.join(stage_name).display()
            )),
        }
    }
    fn stage_publish_error(
        error: std::io::Error,
        retained: Option<(OsString, NativeFileIdentity)>,
        parent: &Path,
    ) -> PdfPublishError {
        if error.kind() == std::io::ErrorKind::Interrupted && retained.is_none() {
            return PdfPublishError::Cancelled;
        }
        let message = if let Some((leaf, identity)) = retained {
            format!(
                "Could not publish PDF: {error}; owned stage {} (identity {identity:?}) was retained",
                parent.join(leaf).display()
            )
        } else {
            format!("Could not publish PDF: {error}")
        };
        PdfPublishError::Io(message)
    }
    pub fn publish_pdf(
        permit: PdfPublishPermit,
        bytes: &[u8],
        cancelled: impl FnMut() -> bool,
        source_current: impl FnMut() -> bool,
    ) -> Result<PdfPublishOutcome, PdfPublishError> {
        publish_pdf_with_admission(permit, bytes, cancelled, source_current, || true)
    }

    pub fn publish_pdf_with_admission(
        permit: PdfPublishPermit,
        bytes: &[u8],
        cancelled: impl FnMut() -> bool,
        source_current: impl FnMut() -> bool,
        admit: impl FnOnce() -> bool,
    ) -> Result<PdfPublishOutcome, PdfPublishError> {
        publish_inner(permit, bytes, cancelled, source_current, admit, |_| {})
    }

    fn publish_inner(
        permit: PdfPublishPermit,
        bytes: &[u8],
        mut cancelled: impl FnMut() -> bool,
        mut source_current: impl FnMut() -> bool,
        admit: impl FnOnce() -> bool,
        after_backup: impl FnOnce(&Path),
    ) -> Result<PdfPublishOutcome, PdfPublishError> {
        if bytes.len() > MAX_OUTPUT_BYTES {
            return Err(PdfPublishError::Io(
                "PDF output exceeds the size limit.".into(),
            ));
        }
        if cancelled() {
            return Err(PdfPublishError::Cancelled);
        }
        if !source_current() {
            return Err(PdfPublishError::Stale);
        }
        let review = permit.0;
        let directory = Directory::open(&review.parent, true).map_err(io)?;
        if directory.identity().map_err(io)? != review.parent_id {
            return Err(PdfPublishError::Stale);
        }
        let mut source_directory = None;
        let mut source_guard = None;
        if let Some(source) = &review.source {
            let dir = Directory::open(&source.parent, false).map_err(io)?;
            if dir.identity().map_err(io)? != source.parent_id {
                return Err(PdfPublishError::Stale);
            }
            let held = dir.source_leaf(&source.leaf, true).map_err(io)?;
            let identity = held
                .as_ref()
                .map(|leaf| leaf.revision().map(|r| r.identity).map_err(io))
                .transpose()?;
            if identity != source.identity {
                return Err(PdfPublishError::Stale);
            }
            source_guard = held;
            source_directory = Some(dir);
        }
        let _source_authority = (source_directory, source_guard);
        let leaf = if review.target.is_some() {
            directory.leaf(&review.leaf, true).map_err(io)?
        } else {
            None
        };
        match (&review.target, &leaf) {
            (None, None) => {}
            (Some(expected), Some(original)) => {
                original.validate_security().map_err(io)?;
                let (revision, digest) = original.fingerprint(&mut cancelled).map_err(io)?;
                if revision != expected.revision || digest != expected.digest {
                    return Err(PdfPublishError::Stale);
                }
            }
            _ => return Err(PdfPublishError::Stale),
        }
        if cancelled() {
            return Err(PdfPublishError::Cancelled);
        }
        if !source_current() {
            return Err(PdfPublishError::Stale);
        }
        let stage = directory.stage(review.target.is_some()).map_err(io)?;
        if let Some(original) = leaf.as_ref() {
            if let Err(error) = stage.copy_security(original) {
                return Err(abort(stage, &review.parent, io(error)));
            }
        }
        if let Err(error) = stage.write(bytes, &mut cancelled) {
            let reason = if error.kind() == std::io::ErrorKind::Interrupted {
                PdfPublishError::Cancelled
            } else {
                io(error)
            };
            return Err(abort(stage, &review.parent, reason));
        }
        if cancelled() {
            return Err(abort(stage, &review.parent, PdfPublishError::Cancelled));
        }
        if !source_current() {
            return Err(abort(stage, &review.parent, PdfPublishError::Stale));
        }
        match (review.target, leaf) {
            (None, None) => {
                if cancelled() {
                    return Err(abort(stage, &review.parent, PdfPublishError::Cancelled));
                }
                if !source_current() {
                    return Err(abort(stage, &review.parent, PdfPublishError::Stale));
                }
                stage
                    .publish(&review.leaf, || !cancelled() && source_current() && admit())
                    .map_err(|(error, retained)| {
                        stage_publish_error(error, retained, &review.parent)
                    })?;
                Ok(PdfPublishOutcome {
                    backup_retained: None,
                })
            }
            (Some(expected), Some(original)) => {
                match original.fingerprint(&mut cancelled) {
                    Ok((revision, digest))
                        if revision == expected.revision && digest == expected.digest => {}
                    Ok(_) => return Err(abort(stage, &review.parent, PdfPublishError::Stale)),
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {
                        return Err(abort(stage, &review.parent, PdfPublishError::Cancelled));
                    }
                    Err(error) => return Err(abort(stage, &review.parent, io(error))),
                }
                if cancelled() {
                    return Err(abort(stage, &review.parent, PdfPublishError::Cancelled));
                }
                if !source_current() {
                    return Err(abort(stage, &review.parent, PdfPublishError::Stale));
                }
                let mut backup = None;
                for _ in 0..64 {
                    if cancelled() {
                        return Err(abort(stage, &review.parent, PdfPublishError::Cancelled));
                    }
                    if !source_current() {
                        return Err(abort(stage, &review.parent, PdfPublishError::Stale));
                    }
                    let serial = NEXT_BACKUP.fetch_add(1, Ordering::Relaxed);
                    let name = OsString::from(format!(
                        ".multi-launcher-pdf-{}-{serial}.backup",
                        std::process::id()
                    ));
                    match directory.rename(&original, &name) {
                        Ok(()) => {
                            backup = Some(name);
                            break;
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                        Err(error) => return Err(abort(stage, &review.parent, io(error))),
                    }
                }
                let Some(backup) = backup else {
                    return Err(abort(
                        stage,
                        &review.parent,
                        PdfPublishError::Io("Could not reserve a recovery backup name.".into()),
                    ));
                };
                after_backup(&review.parent.join(&review.leaf));
                let revoked = cancelled();
                let current = source_current();
                if revoked || !current {
                    let reason = if revoked {
                        PdfPublishError::Cancelled
                    } else {
                        PdfPublishError::Stale
                    };
                    return match directory.rename(&original, &review.leaf) {
                        Ok(()) => Err(abort(stage, &review.parent, reason)),
                        Err(error) => {
                            let reason = abort(stage, &review.parent, reason);
                            Err(PdfPublishError::Recovery {
                                message: format!("{reason}; restoring original failed: {error}"),
                                backup: review.parent.join(&backup),
                            })
                        }
                    };
                }
                if let Err((error, retained)) =
                    stage.publish(&review.leaf, || !cancelled() && source_current() && admit())
                {
                    let message = stage_publish_error(error, retained, &review.parent);
                    return match directory.rename(&original, &review.leaf) {
                        Ok(()) => Err(message),
                        Err(restore) => Err(PdfPublishError::Recovery {
                            message: format!("{message}; restoring original failed: {restore}"),
                            backup: review.parent.join(backup),
                        }),
                    };
                }
                let retained = if original.dispose().is_err() {
                    Some(review.parent.join(backup))
                } else {
                    None
                };
                Ok(PdfPublishOutcome {
                    backup_retained: retained,
                })
            }
            _ => Err(abort(stage, &review.parent, PdfPublishError::Stale)),
        }
    }
    #[cfg(test)]
    mod tests {
        use super::*;
        use std::fs;
        fn publish(
            permit: PdfPublishPermit,
            data: &[u8],
        ) -> Result<PdfPublishOutcome, PdfPublishError> {
            publish_pdf(permit, data, || false, || true)
        }
        #[test]
        fn pdf_publication_denied_final_admission_restores_existing_target() {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("draft.pdf");
            let permit = review_pdf_destination(&path, None).unwrap().authorize();
            let result = publish_pdf_with_admission(permit, b"draft", || false, || true, || false);
            assert!(matches!(result, Err(PdfPublishError::Cancelled)));
            assert!(
                std::fs::read_dir(directory.path())
                    .unwrap()
                    .next()
                    .is_none()
            );

            std::fs::write(&path, b"original").unwrap();
            let permit = review_pdf_destination(&path, None).unwrap().authorize();
            let result = publish_pdf_with_admission(permit, b"draft", || false, || true, || false);
            assert!(matches!(result, Err(PdfPublishError::Cancelled)));
            assert_eq!(std::fs::read(&path).unwrap(), b"original");
            assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
        }

        #[test]
        fn pdf_publication_new_replace_and_stale_revision() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("report.pdf");
            let review = review_pdf_destination(&path, None).unwrap();
            assert_eq!(review.kind(), PdfTargetKind::New);
            assert!(
                publish(review.authorize(), b"first")
                    .unwrap()
                    .backup_retained
                    .is_none()
            );
            assert_eq!(fs::read(&path).unwrap(), b"first");
            let review = review_pdf_destination(&path, None).unwrap();
            assert_eq!(review.kind(), PdfTargetKind::Replace);
            assert!(
                publish(review.authorize(), b"second")
                    .unwrap()
                    .backup_retained
                    .is_none()
            );
            assert_eq!(fs::read(&path).unwrap(), b"second");
            let review = review_pdf_destination(&path, None).unwrap();
            fs::write(&path, b"changed revision").unwrap();
            assert!(matches!(
                publish(review.authorize(), b"bad"),
                Err(PdfPublishError::Stale)
            ));
            assert_eq!(fs::read(&path).unwrap(), b"changed revision");
        }
        #[test]
        fn pdf_publication_late_collision_retains_foreign_and_exact_backup() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("report.pdf");
            fs::write(&path, b"original").unwrap();
            let permit = review_pdf_destination(&path, None).unwrap().authorize();
            let result = publish_inner(
                permit,
                b"new",
                || false,
                || true,
                || true,
                |path| fs::write(path, b"intruder").unwrap(),
            );
            let Err(PdfPublishError::Recovery { backup, .. }) = result else {
                panic!("expected recovery backup")
            };
            assert_eq!(fs::read(&path).unwrap(), b"intruder");
            assert_eq!(fs::read(backup).unwrap(), b"original");
        }
        #[test]
        fn pdf_publication_rejects_source_hardlink_parent_change_and_cancel() {
            let dir = tempfile::tempdir().unwrap();
            let source = dir.path().join("source.md");
            fs::write(&source, b"source").unwrap();
            let alias = dir.path().join("alias.pdf");
            fs::hard_link(&source, &alias).unwrap();
            assert!(matches!(
                review_pdf_destination(&alias, Some(&source)),
                Err(PdfPublishError::ProtectedSource)
            ));
            let target = dir.path().join("cancel.pdf");
            let permit = review_pdf_destination(&target, Some(&source))
                .unwrap()
                .authorize();
            assert!(matches!(
                publish_pdf(permit, b"pdf", || true, || true),
                Err(PdfPublishError::Cancelled)
            ));
            assert!(!target.exists());
            let child = dir.path().join("parent");
            fs::create_dir(&child).unwrap();
            let dest = child.join("new.pdf");
            let permit = review_pdf_destination(&dest, None).unwrap().authorize();
            fs::rename(&child, dir.path().join("old-parent")).unwrap();
            fs::create_dir(&child).unwrap();
            assert!(matches!(
                publish(permit, b"pdf"),
                Err(PdfPublishError::Stale)
            ));
            assert!(!dest.exists());
        }
        #[test]
        fn pdf_publication_missing_source_literal_suffix_and_late_occupant() {
            let dir = tempfile::tempdir().unwrap();
            let source = dir.path().join("missing.md");
            let target = dir.path().join("out.pdf");
            let permit = review_pdf_destination(&target, Some(&source))
                .unwrap()
                .authorize();
            assert!(publish(permit, b"PDF").is_ok());
            let missing = dir.path().join("missing.pdf");
            assert!(matches!(
                review_pdf_destination(&missing, Some(&missing)),
                Err(PdfPublishError::ProtectedSource)
            ));
            let trailing = PathBuf::from(format!("{}\\", target.display()));
            assert!(matches!(
                review_pdf_destination(&trailing, None),
                Err(PdfPublishError::InvalidDestination)
            ));
            let dotted = PathBuf::from(format!("{}\\.", target.display()));
            assert!(matches!(
                review_pdf_destination(&dotted, None),
                Err(PdfPublishError::InvalidDestination)
            ));
            let new = dir.path().join("late.pdf");
            let permit = review_pdf_destination(&new, None).unwrap().authorize();
            fs::write(&new, b"foreign").unwrap();
            assert!(publish(permit, b"draft").is_err());
            assert_eq!(fs::read(new).unwrap(), b"foreign");
        }
        #[test]
        fn pdf_publication_refuses_unsupported_target_before_staging_bytes() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("protected.pdf");
            fs::write(&path, b"original").unwrap();
            let mut permissions = fs::metadata(&path).unwrap().permissions();
            permissions.set_readonly(true);
            fs::set_permissions(&path, permissions).unwrap();
            assert!(review_pdf_destination(&path, None).is_err());
            assert_eq!(fs::read(&path).unwrap(), b"original");
            assert!(fs::read_dir(dir.path()).unwrap().all(|entry| {
                !entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .ends_with(".stage")
            }));
            let mut permissions = fs::metadata(&path).unwrap().permissions();
            permissions.set_readonly(false);
            fs::set_permissions(&path, permissions).unwrap();
        }
        #[test]
        fn pdf_publication_stage_denies_early_read_and_cancel_stops_fingerprint() {
            let dir = tempfile::tempdir().unwrap();
            let directory = Directory::open(dir.path(), true).unwrap();
            let stage = directory.stage(false).unwrap();
            let path = dir.path().join(stage.name());
            assert!(fs::File::open(&path).is_err());
            stage.abort().unwrap();
            assert!(!path.exists());
            let target = dir.path().join("existing.pdf");
            fs::write(&target, b"abc").unwrap();
            let leaf = directory
                .leaf(target.file_name().unwrap(), true)
                .unwrap()
                .unwrap();
            assert_eq!(
                leaf.fingerprint(&mut || true).unwrap_err().kind(),
                std::io::ErrorKind::Interrupted
            );
        }
        #[test]
        fn pdf_publication_rejects_named_streams_when_supported() {
            let dir = tempfile::tempdir().unwrap();
            let target = dir.path().join("existing.pdf");
            fs::write(&target, b"abc").unwrap();
            let stream = PathBuf::from(format!("{}:private", target.display()));
            if fs::write(&stream, b"secret").is_ok() {
                assert!(review_pdf_destination(&target, None).is_err());
            }
        }
        #[test]
        fn pdf_publication_cancel_after_backup_restores_original() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("cancel.pdf");
            fs::write(&path, b"original").unwrap();
            let permit = review_pdf_destination(&path, None).unwrap().authorize();
            let cancel = std::cell::Cell::new(false);
            let result = publish_inner(
                permit,
                b"draft",
                || cancel.get(),
                || true,
                || true,
                |_| cancel.set(true),
            );
            assert!(matches!(result, Err(PdfPublishError::Cancelled)));
            assert_eq!(fs::read(&path).unwrap(), b"original");
        }
        #[test]
        fn pdf_publication_rejects_unsafe_names_and_occupied_nonfiles() {
            let dir = tempfile::tempdir().unwrap();
            for leaf in ["CON.pdf", "COM¹.pdf", "x:stream.pdf", "x.txt", "x?.pdf"] {
                assert!(matches!(
                    review_pdf_destination(&dir.path().join(leaf), None),
                    Err(PdfPublishError::InvalidDestination)
                ));
            }
            let folder = dir.path().join("folder.pdf");
            fs::create_dir(&folder).unwrap();
            assert!(review_pdf_destination(&folder, None).is_err());
            let real = dir.path().join("real.pdf");
            fs::write(&real, b"original").unwrap();
            let link = dir.path().join("link.pdf");
            if std::os::windows::fs::symlink_file(&real, &link).is_ok() {
                assert!(review_pdf_destination(&link, None).is_err());
            }
        }
    }
}
#[cfg(windows)]
pub use windows_impl::{
    PdfDestinationReview, PdfPublishPermit, publish_pdf, publish_pdf_with_admission,
    review_pdf_destination, review_pdf_destination_with_cancel,
};
#[cfg(not(windows))]
#[derive(Debug)]
pub enum PdfDestinationReview {}
#[cfg(not(windows))]
#[derive(Debug)]
pub struct PdfPublishPermit;
#[cfg(not(windows))]
impl PdfDestinationReview {
    pub fn kind(&self) -> PdfTargetKind {
        match *self {}
    }
    pub fn path(&self) -> PathBuf {
        match *self {}
    }
    pub fn authorize(self) -> PdfPublishPermit {
        match self {}
    }
}
#[cfg(not(windows))]
pub fn review_pdf_destination(
    _path: &Path,
    _source: Option<&Path>,
) -> Result<PdfDestinationReview, PdfPublishError> {
    Err(PdfPublishError::Unsupported)
}
#[cfg(not(windows))]
pub fn review_pdf_destination_with_cancel(
    _path: &Path,
    _source: Option<&Path>,
    _cancelled: impl FnMut() -> bool,
) -> Result<PdfDestinationReview, PdfPublishError> {
    Err(PdfPublishError::Unsupported)
}
#[cfg(not(windows))]
pub fn publish_pdf(
    _permit: PdfPublishPermit,
    _bytes: &[u8],
    _cancelled: impl FnMut() -> bool,
    _source_current: impl FnMut() -> bool,
) -> Result<PdfPublishOutcome, PdfPublishError> {
    Err(PdfPublishError::Unsupported)
}

#[cfg(not(windows))]
pub fn publish_pdf_with_admission(
    _permit: PdfPublishPermit,
    _bytes: &[u8],
    _cancelled: impl FnMut() -> bool,
    _source_current: impl FnMut() -> bool,
    _admit: impl FnOnce() -> bool,
) -> Result<PdfPublishOutcome, PdfPublishError> {
    Err(PdfPublishError::Unsupported)
}
