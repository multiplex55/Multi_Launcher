//! Native copy of one approved file into a reviewed vacant destination.

use std::fmt;
use std::fs::File;
use std::io;
use std::os::windows::io::AsRawHandle;
use std::path::PathBuf;

use sha2::{Digest, Sha256};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_ARCHIVE, FILE_ATTRIBUTE_NORMAL, FILE_BASIC_INFO, FILE_STREAM_INFO,
    FileStreamInfo, GetFileInformationByHandleEx,
};

use crate::archive::transfer_native::{
    NativeArchiveError, TransferFile, TransferIo, TransferStage, TransferStageCreateFailure,
};
use crate::file_operations::{NativeFileIdentity, ScanCancellation, set_replace_basic_info};

use super::{
    AdmissionError, ApprovedTarget, ApprovedTransferEntry, DirectoryAccess, EntryKind,
    EntryMetadata, EntrySnapshot, HeldParent, TargetOccupancy, TargetSnapshot, checked_final_path,
    observe_entry_metadata,
};

const COPY_CHUNK_BYTES: usize = 64 * 1024;
const STREAM_INFO_WORDS: usize = 8 * 1024;
const SUPPORTED_ATTRIBUTES: u32 = FILE_ATTRIBUTE_ARCHIVE.0 | FILE_ATTRIBUTE_NORMAL.0;

#[derive(Clone)]
pub struct CopyControl {
    cancellation: ScanCancellation,
    io: TransferIo,
}

impl Default for CopyControl {
    fn default() -> Self {
        let cancellation = ScanCancellation::new();
        Self {
            io: TransferIo::new(cancellation.clone()),
            cancellation,
        }
    }
}

impl CopyControl {
    pub fn cancel(&self) {
        self.io.cancel();
    }

    pub fn is_cancelled(&self) -> bool {
        self.io.is_cancelled()
    }

    fn check(&self) -> Result<(), CopyFailure> {
        if self.is_cancelled() {
            return Err(CopyFailure::Cancelled);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CopyArtifactState {
    StageNeedsReconciliation,
    Published,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CopyArtifact {
    pub path: PathBuf,
    pub parent: NativeFileIdentity,
    pub identity: NativeFileIdentity,
    pub state: CopyArtifactState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CopyVerification {
    NotRequested,
    Verified { sha256: [u8; 32] },
}

#[derive(Debug)]
pub struct CopiedFile {
    pub source: NativeFileIdentity,
    pub output: CopyArtifact,
    pub bytes: u64,
    pub verification: CopyVerification,
}

#[derive(Debug)]
pub enum CopyFailure {
    UnsupportedPlan,
    Cancelled,
    DestinationOccupied,
    UnsupportedStreams,
    UnsupportedMetadata,
    ContentMismatch,
    Admission(AdmissionError),
    Native(io::Error),
}

impl fmt::Display for CopyFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::UnsupportedPlan => {
                "Choose an approved regular file and an exact vacant destination."
            }
            Self::Cancelled => "Copy cancelled.",
            Self::DestinationOccupied => "A file or folder now occupies the reviewed destination.",
            Self::UnsupportedStreams => {
                "Windows could not prove that this file contains only the supported data stream."
            }
            Self::UnsupportedMetadata => {
                "This copy cannot preserve the file's attributes or timestamps."
            }
            Self::ContentMismatch => "The copied bytes did not match the source.",
            Self::Admission(error) => return write!(f, "{error}"),
            Self::Native(error) => return write!(f, "Windows copy failed ({error})."),
        };
        f.write_str(message)
    }
}

impl std::error::Error for CopyFailure {}

impl From<AdmissionError> for CopyFailure {
    fn from(error: AdmissionError) -> Self {
        match error {
            AdmissionError::Cancelled => Self::Cancelled,
            error => Self::Admission(error),
        }
    }
}

impl From<io::Error> for CopyFailure {
    fn from(error: io::Error) -> Self {
        match error.kind() {
            io::ErrorKind::Interrupted => Self::Cancelled,
            _ => Self::Native(error),
        }
    }
}

impl From<NativeArchiveError> for CopyFailure {
    fn from(error: NativeArchiveError) -> Self {
        io::Error::new(error.kind(), error.to_string()).into()
    }
}

#[derive(Debug)]
pub enum CopyRecoveryArtifact {
    Owned(CopyArtifact),
    UnprovenStage {
        path: PathBuf,
        parent: NativeFileIdentity,
    },
}

#[derive(Debug)]
pub struct CopyError {
    pub failure: CopyFailure,
    pub artifact: Option<CopyRecoveryArtifact>,
    pub recovery_error: Option<String>,
}

impl From<CopyFailure> for CopyError {
    fn from(failure: CopyFailure) -> Self {
        Self {
            failure,
            artifact: None,
            recovery_error: None,
        }
    }
}

impl fmt::Display for CopyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.failure.fmt(f)
    }
}

impl std::error::Error for CopyError {}

struct GuardedCopy<'a> {
    source: &'a EntrySnapshot,
    source_parent: HeldParent,
    source_file: TransferFile,
    target: &'a TargetSnapshot,
    target_parent: HeldParent,
    length: u64,
}

impl<'a> GuardedCopy<'a> {
    fn open(entry: &'a ApprovedTransferEntry, control: &CopyControl) -> Result<Self, CopyFailure> {
        control.check()?;
        let source = entry.source();
        let ApprovedTarget::CreateAt(target) = entry.destination() else {
            return Err(CopyFailure::UnsupportedPlan);
        };
        let metadata = source.metadata();
        if metadata.kind != EntryKind::File
            || !matches!(target.occupancy(), TargetOccupancy::Vacant)
        {
            return Err(CopyFailure::UnsupportedPlan);
        }
        if metadata.attributes & !SUPPORTED_ATTRIBUTES != 0 {
            return Err(CopyFailure::UnsupportedMetadata);
        }
        let length = metadata.byte_len.ok_or(CopyFailure::UnsupportedPlan)?;

        let source_parent = HeldParent::open(source.parent().path(), &control.cancellation)?;
        let target_parent = HeldParent::open_with_access(
            target.parent().path(),
            &control.cancellation,
            DirectoryAccess::CreateFile,
        )?;
        if source_parent.snapshot != *source.parent() || target_parent.snapshot != *target.parent()
        {
            return Err(AdmissionError::StaleParent.into());
        }
        let source_file = source_parent
            .directory()
            .open_read(source.leaf().as_os_str())?;
        let guarded = Self {
            source,
            source_parent,
            source_file,
            target,
            target_parent,
            length,
        };
        guarded.check_source(control)?;
        guarded.check_vacancy(control)?;
        Ok(guarded)
    }

    fn check_source(&self, control: &CopyControl) -> Result<(), CopyFailure> {
        self.source_parent.check_chain(&control.cancellation)?;
        let current = self
            .source_parent
            .capture_entry(self.source_file.file(), self.source.leaf())?;
        if current != *self.source || self.source_file.identity() != self.source.identity() {
            return Err(AdmissionError::StaleEntry.into());
        }
        require_default_stream(self.source_file.file(), self.length)?;
        control.check()
    }

    fn check_vacancy(&self, control: &CopyControl) -> Result<(), CopyFailure> {
        match self
            .target_parent
            .inspect_leaf(self.target.requested_leaf(), &control.cancellation)?
        {
            TargetOccupancy::Vacant => Ok(()),
            TargetOccupancy::Occupied(_) => Err(CopyFailure::DestinationOccupied),
        }
    }
}

fn require_default_stream(file: &File, length: u64) -> Result<(), CopyFailure> {
    let mut words = vec![0_u64; STREAM_INFO_WORDS];
    // SAFETY: file is a live held file. The u64 buffer provides the record's
    // required alignment, and its full byte length is passed to Windows.
    unsafe {
        GetFileInformationByHandleEx(
            HANDLE(file.as_raw_handle()),
            FileStreamInfo,
            words.as_mut_ptr().cast(),
            std::mem::size_of_val(words.as_slice()) as u32,
        )
    }
    .map_err(|_| CopyFailure::UnsupportedStreams)?;

    let record = words.as_ptr().cast::<FILE_STREAM_INFO>();
    // SAFETY: the zero-initialized buffer contains a complete fixed header
    // after the successful query. Read only fixed fields before checking name.
    let (next, bytes, size) = unsafe {
        (
            (*record).NextEntryOffset,
            (*record).StreamNameLength as usize,
            (*record).StreamSize,
        )
    };
    let header = std::mem::offset_of!(FILE_STREAM_INFO, StreamName);
    if next != 0
        || bytes == 0
        || !bytes.is_multiple_of(2)
        || bytes > std::mem::size_of_val(words.as_slice()) - header
    {
        return Err(CopyFailure::UnsupportedStreams);
    }
    // SAFETY: the checked range stays inside the aligned record buffer.
    let name = unsafe {
        std::slice::from_raw_parts(
            words.as_ptr().cast::<u8>().add(header).cast::<u16>(),
            bytes / 2,
        )
    };
    if !name.iter().copied().eq("::$DATA".encode_utf16()) {
        return Err(CopyFailure::UnsupportedStreams);
    }
    if u64::try_from(size).ok() != Some(length) {
        return Err(CopyFailure::ContentMismatch);
    }
    Ok(())
}

fn read_chunks(
    file: &TransferFile,
    length: u64,
    control: &CopyControl,
    mut consume: impl FnMut(u64, &[u8]) -> Result<(), CopyFailure>,
) -> Result<(), CopyFailure> {
    let mut offset = 0;
    while offset < length {
        control.check()?;
        let amount = (length - offset).min(COPY_CHUNK_BYTES as u64) as usize;
        let (buffer, transferred) = control.io.read_at(file, offset, amount)?;
        if transferred == 0 || transferred > amount {
            return Err(CopyFailure::ContentMismatch);
        }
        consume(offset, &buffer[..transferred])?;
        offset += transferred as u64;
    }
    control.check()
}

fn write_chunk(
    stage: &TransferStage,
    control: &CopyControl,
    offset: u64,
    bytes: &[u8],
) -> Result<(), CopyFailure> {
    let mut written = 0;
    while written < bytes.len() {
        control.check()?;
        let progress = stage.write(&control.io, offset + written as u64, &bytes[written..])?;
        if progress == 0 || progress > bytes.len() - written {
            return Err(
                io::Error::new(io::ErrorKind::WriteZero, "invalid staging write progress").into(),
            );
        }
        written += progress;
    }
    Ok(())
}

fn apply_metadata(stage: &TransferStage, source: EntryMetadata) -> Result<(), CopyFailure> {
    let attributes = if source.attributes & FILE_ATTRIBUTE_ARCHIVE.0 != 0 {
        FILE_ATTRIBUTE_ARCHIVE.0
    } else {
        FILE_ATTRIBUTE_NORMAL.0
    };
    set_replace_basic_info(
        stage.file(),
        FILE_BASIC_INFO {
            CreationTime: source.created,
            LastWriteTime: source.modified,
            FileAttributes: attributes,
            ..Default::default()
        },
    )?;
    Ok(())
}

fn check_output(file: &File, source: EntryMetadata, length: u64) -> Result<(), CopyFailure> {
    let copied = observe_entry_metadata(file)?;
    if copied.kind != EntryKind::File || copied.byte_len != Some(length) {
        return Err(CopyFailure::ContentMismatch);
    }
    if copied.created != source.created
        || copied.modified != source.modified
        || copied.attributes & !FILE_ATTRIBUTE_NORMAL.0
            != source.attributes & !FILE_ATTRIBUTE_NORMAL.0
    {
        return Err(CopyFailure::UnsupportedMetadata);
    }
    require_default_stream(file, length)
}

fn fill_stage(
    stage: &TransferStage,
    copy: &GuardedCopy<'_>,
    verify: bool,
    control: &CopyControl,
    mut progress: impl FnMut(u64),
) -> Result<Option<[u8; 32]>, CopyFailure> {
    let mut hash = verify.then(Sha256::new);
    read_chunks(&copy.source_file, copy.length, control, |offset, bytes| {
        write_chunk(stage, control, offset, bytes)?;
        if let Some(hash) = &mut hash {
            hash.update(bytes);
        }
        progress(offset + bytes.len() as u64);
        Ok(())
    })?;
    apply_metadata(stage, copy.source.metadata())?;
    stage.flush()?;
    check_output(stage.file(), copy.source.metadata(), copy.length)?;
    copy.check_source(control)?;
    copy.check_vacancy(control)?;
    Ok(hash.map(|hash| hash.finalize().into()))
}

/// Publish an ordinary regular file to its approved vacant target. The
/// caller owns the surrounding job and any later move or recovery steps.
pub fn copy_to_vacancy(
    entry: &ApprovedTransferEntry,
    control: &CopyControl,
    progress: impl FnMut(u64),
) -> Result<CopiedFile, CopyError> {
    copy_with_publication_hook(entry, control, progress, || {})
}

fn copy_with_publication_hook(
    entry: &ApprovedTransferEntry,
    control: &CopyControl,
    progress: impl FnMut(u64),
    before_publication: impl FnOnce(),
) -> Result<CopiedFile, CopyError> {
    let copy = GuardedCopy::open(entry, control)?;
    let stage = copy
        .target_parent
        .directory()
        .create_stage(copy.target.requested_leaf().as_os_str())
        .map_err(|error| stage_create_error(copy.target, error))?;
    let stage_artifact = CopyRecoveryArtifact::Owned(CopyArtifact {
        path: copy.target.parent().path().join(stage.leaf()),
        parent: copy.target.parent().identity(),
        identity: stage.identity(),
        state: CopyArtifactState::StageNeedsReconciliation,
    });
    let expected_hash = match fill_stage(&stage, &copy, entry.verify_content(), control, progress) {
        Ok(hash) => hash,
        Err(failure) => match stage.abort() {
            Ok(()) => return Err(failure.into()),
            Err(error) => {
                return Err(CopyError {
                    failure,
                    artifact: Some(stage_artifact),
                    recovery_error: Some(error.to_string()),
                });
            }
        },
    };
    let published = stage
        .publish(copy.target.requested_leaf().as_os_str(), || {
            before_publication();
            !control.is_cancelled()
        })
        .map_err(|error| {
            let artifact = error.cleanup_failed().then_some(stage_artifact);
            let recovery_error = artifact.as_ref().map(|_| error.to_string());
            let failure = if error.kind() == io::ErrorKind::AlreadyExists {
                CopyFailure::DestinationOccupied
            } else {
                error.into()
            };
            CopyError {
                failure,
                artifact,
                recovery_error,
            }
        })?;

    let artifact = CopyArtifact {
        path: copy
            .target
            .parent()
            .path()
            .join(copy.target.requested_leaf().as_os_str()),
        parent: copy.target.parent().identity(),
        identity: published.identity(),
        state: CopyArtifactState::Published,
    };
    let verification = finish_publication(&copy, &published, &artifact, expected_hash, control)
        .map_err(|failure| CopyError {
            failure,
            artifact: Some(CopyRecoveryArtifact::Owned(artifact.clone())),
            recovery_error: None,
        })?;
    Ok(CopiedFile {
        source: copy.source.identity(),
        output: artifact,
        bytes: copy.length,
        verification,
    })
}

fn stage_create_error(target: &TargetSnapshot, failed: TransferStageCreateFailure) -> CopyError {
    let artifact = failed.unresolved.map(|attempt| {
        let path = target.parent().path().join(attempt.leaf);
        let parent = target.parent().identity();
        match attempt.identity {
            Some(identity) => CopyRecoveryArtifact::Owned(CopyArtifact {
                path,
                parent,
                identity,
                state: CopyArtifactState::StageNeedsReconciliation,
            }),
            None => CopyRecoveryArtifact::UnprovenStage { path, parent },
        }
    });
    let recovery_error = artifact.as_ref().map(|_| failed.error.to_string());
    CopyError {
        failure: failed.error.into(),
        artifact,
        recovery_error,
    }
}

fn finish_publication(
    copy: &GuardedCopy<'_>,
    published: &TransferFile,
    artifact: &CopyArtifact,
    expected_hash: Option<[u8; 32]>,
    control: &CopyControl,
) -> Result<CopyVerification, CopyFailure> {
    published.file().sync_all()?;
    copy.target_parent.check_chain(&control.cancellation)?;
    checked_final_path(published.file(), &artifact.path)?;
    check_output(published.file(), copy.source.metadata(), copy.length)?;
    let verification = if let Some(expected) = expected_hash {
        let mut hash = Sha256::new();
        read_chunks(published, copy.length, control, |_, bytes| {
            hash.update(bytes);
            Ok(())
        })?;
        let actual: [u8; 32] = hash.finalize().into();
        if actual != expected {
            return Err(CopyFailure::ContentMismatch);
        }
        CopyVerification::Verified { sha256: actual }
    } else {
        CopyVerification::NotRequested
    };
    copy.check_source(control)?;
    copy.target_parent.check_chain(&control.cancellation)?;
    check_output(published.file(), copy.source.metadata(), copy.length)?;
    control.check()?;
    Ok(verification)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_transfer::{
        ApprovedTransferPlan, TransferAction, TransferDraft, inspect_source_tree, review_transfer,
    };
    use std::fs;
    use std::path::Path;
    use std::sync::Arc;

    fn approve_file(source: &Path, destination: &Path, verify: bool) -> ApprovedTransferPlan {
        let cancellation = ScanCancellation::new();
        let tree = Arc::new(inspect_source_tree(&[source.to_path_buf()], &cancellation).unwrap());
        let review = review_transfer(
            tree,
            TransferDraft {
                action: TransferAction::CopyTo(destination.to_path_buf()),
                verify,
            },
            &cancellation,
        )
        .unwrap();
        review.approve(review.generation(), &cancellation).unwrap()
    }

    #[test]
    fn transfer_copy_native_multichunk_and_empty_files_preserve_bytes_and_times() {
        let temp = tempfile::tempdir().unwrap();
        let dest = temp.path().join("output");
        fs::create_dir(&dest).unwrap();
        let data = vec![b'x'; COPY_CHUNK_BYTES * 2 + 31];
        for (name, bytes, verify) in [
            ("multi.bin", data.as_slice(), true),
            ("empty.bin", &[], false),
        ] {
            let source = temp.path().join(name);
            fs::write(&source, bytes).unwrap();
            let plan = approve_file(&source, &dest, verify);
            let mut progress = Vec::new();
            let copied = copy_to_vacancy(&plan.entries()[0], &CopyControl::default(), |count| {
                progress.push(count)
            })
            .unwrap();
            assert_eq!(copied.output.path, dest.join(name));
            assert_eq!(copied.bytes, bytes.len() as u64);
            assert_eq!(fs::read(&copied.output.path).unwrap(), bytes);
            assert_eq!(fs::read(&source).unwrap(), bytes);
            let current_output_identity =
                crate::file_transfer::inspect_source(&copied.output.path, &ScanCancellation::new())
                    .unwrap();
            assert_eq!(current_output_identity.identity(), copied.output.identity);
            assert_eq!(
                current_output_identity.metadata().created,
                plan.entries()[0].source().metadata().created
            );
            assert_eq!(
                current_output_identity.metadata().modified,
                plan.entries()[0].source().metadata().modified
            );
            if verify {
                assert_eq!(
                    copied.verification,
                    CopyVerification::Verified {
                        sha256: Sha256::digest(bytes).into()
                    },
                );
                assert_eq!(
                    progress,
                    [
                        COPY_CHUNK_BYTES as u64,
                        (COPY_CHUNK_BYTES * 2) as u64,
                        bytes.len() as u64
                    ]
                );
            } else {
                assert_eq!(copied.verification, CopyVerification::NotRequested);
                assert!(progress.is_empty());
            }
        }
        assert_eq!(fs::read_dir(dest).unwrap().count(), 2);
    }

    #[test]
    fn transfer_copy_native_late_winner_survives_no_clobber_and_owned_stage_is_removed() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source.txt");
        let dest = temp.path().join("output");
        let target = dest.join("source.txt");
        fs::create_dir(&dest).unwrap();
        fs::write(&source, b"original source").unwrap();
        let plan = approve_file(&source, &dest, true);
        let error = copy_with_publication_hook(
            &plan.entries()[0],
            &CopyControl::default(),
            |_| {},
            || {
                fs::write(&target, b"foreign winner").unwrap();
            },
        )
        .unwrap_err();
        assert!(
            matches!(error.failure, CopyFailure::DestinationOccupied),
            "{error:?}"
        );
        assert!(error.artifact.is_none());
        assert_eq!(fs::read(&target).unwrap(), b"foreign winner");
        assert_eq!(fs::read(&source).unwrap(), b"original source");
        assert_eq!(fs::read_dir(&dest).unwrap().count(), 1);
    }

    #[test]
    fn transfer_copy_native_stale_source_or_parent_is_refused_before_staging() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source.txt");
        let dest = temp.path().join("output");
        fs::create_dir(&dest).unwrap();
        fs::write(&source, b"original").unwrap();
        let plan = approve_file(&source, &dest, false);
        fs::rename(&source, temp.path().join("original.txt")).unwrap();
        fs::write(&source, b"new data").unwrap();
        let error =
            copy_to_vacancy(&plan.entries()[0], &CopyControl::default(), |_| {}).unwrap_err();
        assert!(matches!(
            error.failure,
            CopyFailure::Admission(AdmissionError::StaleEntry)
        ));
        assert_eq!(fs::read_dir(&dest).unwrap().count(), 0);

        let plan = approve_file(&source, &dest, false);
        let original_dest = temp.path().join("earlier-output");
        fs::rename(&dest, &original_dest).unwrap();
        fs::create_dir(&dest).unwrap();
        let error =
            copy_to_vacancy(&plan.entries()[0], &CopyControl::default(), |_| {}).unwrap_err();
        assert!(matches!(
            error.failure,
            CopyFailure::Admission(AdmissionError::StaleParent)
        ));
        assert_eq!(fs::read_dir(&dest).unwrap().count(), 0);
        assert_eq!(fs::read_dir(&original_dest).unwrap().count(), 0);
    }

    #[test]
    fn transfer_copy_native_cancellation_releases_the_stage_and_keeps_source() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source.bin");
        let dest = temp.path().join("output");
        fs::create_dir(&dest).unwrap();
        let contents = vec![17; COPY_CHUNK_BYTES * 2];
        fs::write(&source, &contents).unwrap();
        let plan = approve_file(&source, &dest, true);
        let control = CopyControl::default();
        let error =
            copy_to_vacancy(&plan.entries()[0], &control, |_| control.cancel()).unwrap_err();
        assert!(matches!(error.failure, CopyFailure::Cancelled), "{error:?}");
        assert!(error.artifact.is_none());
        assert_eq!(fs::read(&source).unwrap(), contents);
        assert_eq!(fs::read_dir(&dest).unwrap().count(), 0);
    }

    #[test]
    fn transfer_copy_native_late_source_stream_change_reports_the_published_file() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source.txt");
        let dest = temp.path().join("output");
        let target = dest.join("source.txt");
        fs::create_dir(&dest).unwrap();
        fs::write(&source, b"source payload").unwrap();
        let plan = approve_file(&source, &dest, true);
        let mut ads = source.clone().into_os_string();
        ads.push(":LateExtra:$DATA");
        let error = copy_with_publication_hook(
            &plan.entries()[0],
            &CopyControl::default(),
            |_| {},
            || fs::write(Path::new(&ads), b"late stream").unwrap(),
        )
        .unwrap_err();
        assert!(
            matches!(
                error.failure,
                CopyFailure::UnsupportedStreams
                    | CopyFailure::Admission(AdmissionError::StaleEntry)
            ),
            "{error:?}"
        );
        let Some(CopyRecoveryArtifact::Owned(artifact)) = error.artifact else {
            panic!("the published file must remain identifiable");
        };
        assert_eq!(artifact.state, CopyArtifactState::Published);
        assert_eq!(artifact.path, target);
        assert_eq!(fs::read(target).unwrap(), b"source payload");
        assert_eq!(fs::read(Path::new(&ads)).unwrap(), b"late stream");
    }

    #[test]
    fn transfer_copy_native_unknown_ads_requires_explicit_refusal() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source.txt");
        let dest = temp.path().join("output");
        fs::create_dir(&dest).unwrap();
        fs::write(&source, b"base payload").unwrap();
        let mut ads = source.clone().into_os_string();
        ads.push(":AdditionalData:$DATA");
        fs::write(Path::new(&ads), b"other stream").unwrap();
        let plan = approve_file(&source, &dest, false);
        let error =
            copy_to_vacancy(&plan.entries()[0], &CopyControl::default(), |_| {}).unwrap_err();
        assert!(
            matches!(error.failure, CopyFailure::UnsupportedStreams),
            "{error:?}"
        );
        assert!(error.artifact.is_none());
        assert_eq!(fs::read(Path::new(&ads)).unwrap(), b"other stream");
        assert_eq!(fs::read_dir(&dest).unwrap().count(), 0);
    }

    #[test]
    fn transfer_copy_native_existing_writer_prevents_file_guard_admission() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source.txt");
        let dest = temp.path().join("output");
        fs::create_dir(&dest).unwrap();
        fs::write(&source, b"payload").unwrap();
        let plan = approve_file(&source, &dest, false);
        let writer = fs::OpenOptions::new().write(true).open(&source).unwrap();

        let error =
            copy_to_vacancy(&plan.entries()[0], &CopyControl::default(), |_| {}).unwrap_err();
        assert!(matches!(error.failure, CopyFailure::Native(_)), "{error:?}");
        assert_eq!(fs::read_dir(&dest).unwrap().count(), 0);
        drop(writer);
    }
}
