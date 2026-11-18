//! Native copy of one approved file into a reviewed vacant destination.

use std::fmt;
use std::fs::File;
use std::io;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_ARCHIVE, FILE_ATTRIBUTE_NORMAL, FILE_BASIC_INFO, FILE_STREAM_INFO,
    FileStreamInfo, GetFileInformationByHandleEx, GetVolumeInformationByHandleW,
};
use windows::Win32::System::SystemServices::FILE_NAMED_STREAMS;

use crate::archive::transfer_native::{
    NativeArchiveError, TransferFile, TransferIo, TransferRelocation, TransferStage,
    TransferStageCreateFailure,
};
use crate::file_operations::{NativeFileIdentity, ScanCancellation, set_replace_basic_info};

use super::{
    AdmissionError, ApprovedTarget, ApprovedTransferEntry, DirectoryAccess, EntryKind,
    EntryMetadata, EntrySnapshot, HeldParent, TargetOccupancy, TargetSnapshot, checked_final_path,
    observe_entry_metadata,
};

const COPY_CHUNK_BYTES: usize = 64 * 1024;
const STREAM_INFO_WORDS: usize = 8 * 1024;
const MAX_ZONE_IDENTIFIER_BYTES: u64 = 64 * 1024;
const SUPPORTED_ATTRIBUTES: u32 = FILE_ATTRIBUTE_ARCHIVE.0 | FILE_ATTRIBUTE_NORMAL.0;

#[derive(Clone)]
pub struct CopyControl {
    cancellation: ScanCancellation,
    io: TransferIo,
    pause: std::sync::Arc<super::control::PauseGate>,
}

impl Default for CopyControl {
    fn default() -> Self {
        let cancellation = ScanCancellation::new();
        Self {
            io: TransferIo::new(cancellation.clone()),
            cancellation,
            pause: std::sync::Arc::new(super::control::PauseGate::default()),
        }
    }
}

impl CopyControl {
    pub fn cancel(&self) {
        self.pause.cancel();
        self.io.cancel();
    }

    pub fn is_cancelled(&self) -> bool {
        self.io.is_cancelled()
    }

    pub(super) fn cancellation(&self) -> &ScanCancellation {
        &self.cancellation
    }

    pub fn request_pause(&self) -> bool {
        self.pause.request_pause()
    }
    pub fn resume(&self) -> bool {
        self.pause.resume()
    }
    pub fn pause_state(&self) -> super::control::CopyPauseState {
        self.pause.state()
    }
    pub(super) fn admission_epoch(&self) -> Result<Option<u64>, CopyFailure> {
        self.pause.epoch()
    }
    pub(super) fn try_claim(
        &self,
        epoch: u64,
    ) -> Result<Option<super::control::MutationClaim>, CopyFailure> {
        self.pause.try_claim(epoch)
    }
    pub(super) fn check(&self) -> Result<(), CopyFailure> {
        self.pause.checkpoint()
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
    Verified {
        sha256: [u8; 32],
        zone_sha256: Option<[u8; 32]>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplacementState {
    Retained,
    Restored,
    Uncertain,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplacementBackup {
    pub original: PathBuf,
    pub backup: PathBuf,
    pub parent: NativeFileIdentity,
    pub identity: NativeFileIdentity,
    pub state: ReplacementState,
}

impl ReplacementBackup {
    pub fn retained_or_uncertain(&self) -> bool {
        self.state != ReplacementState::Restored
    }
}

#[derive(Debug)]
pub struct CopiedFile {
    pub source: NativeFileIdentity,
    pub output: CopyArtifact,
    pub bytes: u64,
    pub zone_bytes: Option<u64>,
    pub verification: CopyVerification,
    pub replacement: Option<ReplacementBackup>,
}

#[derive(Debug)]
pub enum CopyFailure {
    UnsupportedPlan,
    Cancelled,
    DestinationOccupied,
    UnsupportedStreams,
    UnsupportedMetadata,
    ContentMismatch,
    Journal(io::Error),
    Admission(AdmissionError),
    Native(io::Error),
}

impl fmt::Display for CopyFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::UnsupportedPlan => "The copy engine cannot run this approved operation.",
            Self::Cancelled => "Copy cancelled.",
            Self::DestinationOccupied => "A file or folder now occupies the reviewed destination.",
            Self::UnsupportedStreams => {
                "Windows could not verify support for the file's data and Zone Identifier streams."
            }
            Self::UnsupportedMetadata => {
                "This copy cannot preserve the file's attributes or timestamps."
            }
            Self::ContentMismatch => "The copied bytes did not match the source.",
            Self::Journal(error) => return write!(f, "Transfer journal failed ({error})."),
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
    Relocation(Box<super::relocate::MoveEffect>),
    CrossVolume(Box<CrossMoveEffect>),
    UncertainPublication {
        stage: PathBuf,
        target: PathBuf,
        parent: NativeFileIdentity,
        identity: NativeFileIdentity,
    },
    UnprovenStage {
        path: PathBuf,
        parent: NativeFileIdentity,
    },
    UnprovenDirectory {
        path: PathBuf,
        parent: NativeFileIdentity,
        creation_observed: bool,
    },
}

#[derive(Debug)]
pub struct CopyError {
    pub failure: CopyFailure,
    pub artifact: Option<CopyRecoveryArtifact>,
    pub recovery_error: Option<String>,
    pub replacement: Option<ReplacementBackup>,
}

impl CopyError {
    pub(super) fn has_retained_effect(&self) -> bool {
        self.artifact.is_some()
            || self
                .replacement
                .as_ref()
                .is_some_and(ReplacementBackup::retained_or_uncertain)
    }
}

impl From<CopyFailure> for CopyError {
    fn from(failure: CopyFailure) -> Self {
        Self {
            failure,
            artifact: None,
            recovery_error: None,
            replacement: None,
        }
    }
}

impl fmt::Display for CopyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.failure.fmt(f)
    }
}

impl std::error::Error for CopyError {}

mod cross_move;

pub use cross_move::{CrossMoveEffect, CrossMoveSourceState, CrossVolumeMovedFile};
pub(super) use cross_move::{CrossSourceAction, move_across_volumes};

struct HeldZone {
    file: TransferFile,
    length: u64,
}

struct GuardedCopy<'a> {
    source: &'a EntrySnapshot,
    source_parent: HeldParent,
    source_file: TransferFile,
    source_relocation: Option<TransferRelocation>,
    source_zone: Option<HeldZone>,
    target: &'a TargetSnapshot,
    target_parent: HeldParent,
    length: u64,
}

impl<'a> GuardedCopy<'a> {
    fn open(
        source: &'a EntrySnapshot,
        target: &'a TargetSnapshot,
        control: &CopyControl,
        replacement: bool,
    ) -> Result<Self, CopyFailure> {
        Self::open_scoped(source, target, control, replacement, false)
    }

    fn open_cross_volume(
        source: &'a EntrySnapshot,
        target: &'a TargetSnapshot,
        control: &CopyControl,
    ) -> Result<Self, CopyFailure> {
        Self::open_scoped(source, target, control, false, true)
    }

    fn open_scoped(
        source: &'a EntrySnapshot,
        target: &'a TargetSnapshot,
        control: &CopyControl,
        replacement: bool,
        cross_volume: bool,
    ) -> Result<Self, CopyFailure> {
        control.check()?;
        let metadata = source.metadata();
        if metadata.kind != EntryKind::File
            || !matches!(
                (target.occupancy(), replacement),
                (TargetOccupancy::Vacant, false) | (TargetOccupancy::Occupied(_), true)
            )
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
        if cross_volume
            && source_parent.snapshot.identity().volume_serial_number()
                == target_parent.snapshot.identity().volume_serial_number()
        {
            return Err(CopyFailure::UnsupportedPlan);
        }
        let source_relocation = if cross_volume {
            let held = source_parent
                .directory()
                .open_cross_volume_move_source(source.leaf().as_os_str(), source.identity())?;
            held.require_no_extended_attributes()?;
            Some(held)
        } else {
            None
        };
        let source_file = if let Some(held) = &source_relocation {
            held.data_file()
                .ok_or(CopyFailure::UnsupportedPlan)?
                .clone()
        } else {
            source_parent
                .directory()
                .open_read(source.leaf().as_os_str())?
        };
        let current = source_parent.capture_entry(source_file.file(), source.leaf())?;
        if current != *source || source_file.identity() != source.identity() {
            return Err(AdmissionError::StaleEntry.into());
        }
        let inventory = stream_inventory(source_file.file(), length)?;
        let source_zone = inventory
            .zone
            .map(|length| {
                let file = if cross_volume {
                    source_parent
                        .directory()
                        .open_move_source_zone(source.leaf().as_os_str(), source.identity())?
                } else {
                    source_parent
                        .directory()
                        .open_source_zone(source.leaf().as_os_str(), source.identity())?
                };
                if file.file().metadata()?.len() != length {
                    return Err(CopyFailure::ContentMismatch);
                }
                Ok(HeldZone { file, length })
            })
            .transpose()?;
        let guarded = Self {
            source,
            source_parent,
            source_file,
            source_relocation,
            source_zone,
            target,
            target_parent,
            length,
        };
        guarded.check_source(control)?;
        guarded.check_target(control)?;
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
        if let Some(held) = &self.source_relocation {
            held.require_no_extended_attributes()?;
        }
        if stream_inventory(self.source_file.file(), self.length)? != self.streams() {
            return Err(AdmissionError::StaleEntry.into());
        }
        control.check()
    }

    fn streams(&self) -> StreamInventory {
        StreamInventory {
            data: self.length,
            zone: self.source_zone.as_ref().map(|zone| zone.length),
        }
    }

    fn check_vacancy_after_displacement(&self, control: &CopyControl) -> Result<(), CopyFailure> {
        match self
            .target_parent
            .inspect_leaf(self.target.requested_leaf(), &control.cancellation)?
        {
            TargetOccupancy::Vacant => Ok(()),
            TargetOccupancy::Occupied(_) => Err(CopyFailure::DestinationOccupied),
        }
    }

    fn check_target(&self, control: &CopyControl) -> Result<(), CopyFailure> {
        let current = self
            .target_parent
            .inspect_leaf(self.target.requested_leaf(), &control.cancellation)?;
        let reviewed = self.target.occupancy();
        if &current == reviewed {
            return Ok(());
        }
        match reviewed {
            TargetOccupancy::Vacant => Err(CopyFailure::DestinationOccupied),
            TargetOccupancy::Occupied(_) => Err(AdmissionError::StaleTarget.into()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct StreamInventory {
    pub(super) data: u64,
    pub(super) zone: Option<u64>,
}

fn query_stream_records(file: &File) -> windows::core::Result<Vec<u64>> {
    let mut words = vec![0_u64; STREAM_INFO_WORDS];
    // SAFETY: the live file is borrowed and the aligned buffer is writable.
    unsafe {
        GetFileInformationByHandleEx(
            HANDLE(file.as_raw_handle()),
            FileStreamInfo,
            words.as_mut_ptr().cast(),
            std::mem::size_of_val(words.as_slice()) as u32,
        )
    }?;
    Ok(words)
}

fn volume_has_no_named_streams(file: &File) -> bool {
    let mut capabilities = 0;
    // SAFETY: the handle is live and capabilities points to a writable DWORD.
    let volume = unsafe {
        GetVolumeInformationByHandleW(
            HANDLE(file.as_raw_handle()),
            None,
            None,
            None,
            Some(&mut capabilities),
            None,
        )
    };
    volume.is_ok() && capabilities & FILE_NAMED_STREAMS == 0
}

pub(super) fn stream_inventory(file: &File, length: u64) -> Result<StreamInventory, CopyFailure> {
    match query_stream_records(file) {
        Ok(words) => parse_stream_inventory(&words, length),
        Err(_) if volume_has_no_named_streams(file) => Ok(StreamInventory {
            data: length,
            zone: None,
        }),
        Err(_) => Err(CopyFailure::UnsupportedStreams),
    }
}

pub(super) fn require_no_directory_streams(file: &File) -> Result<(), CopyFailure> {
    use windows::Win32::Foundation::ERROR_HANDLE_EOF;
    use windows::core::HRESULT;

    match query_stream_records(file) {
        Err(error)
            if error.code() == HRESULT::from_win32(ERROR_HANDLE_EOF.0)
                || volume_has_no_named_streams(file) =>
        {
            Ok(())
        }
        _ => Err(CopyFailure::UnsupportedStreams),
    }
}

fn parse_stream_inventory(words: &[u64], length: u64) -> Result<StreamInventory, CopyFailure> {
    let size = std::mem::size_of_val(words);
    let header = std::mem::offset_of!(FILE_STREAM_INFO, StreamName);
    let mut offset = 0_usize;
    let mut data = None;
    let mut zone = None;
    loop {
        let name_start = offset
            .checked_add(header)
            .filter(|&end| end <= size)
            .ok_or(CopyFailure::UnsupportedStreams)?;
        // SAFETY: the aligned record starts within the buffer and all of the
        // fixed fields before StreamName are inside the verified header.
        let (next, bytes, stream_size) = unsafe {
            let record = words
                .as_ptr()
                .cast::<u8>()
                .add(offset)
                .cast::<FILE_STREAM_INFO>();
            (
                (*record).NextEntryOffset as usize,
                (*record).StreamNameLength as usize,
                (*record).StreamSize,
            )
        };
        if bytes == 0 || !bytes.is_multiple_of(2) {
            return Err(CopyFailure::UnsupportedStreams);
        }
        let name_end = name_start
            .checked_add(bytes)
            .filter(|&end| end <= size)
            .ok_or(CopyFailure::UnsupportedStreams)?;
        let next_offset = if next != 0 {
            let next_offset = offset
                .checked_add(next)
                .filter(|&end| {
                    end >= name_end
                        && end <= size
                        && end.is_multiple_of(std::mem::align_of::<FILE_STREAM_INFO>())
                })
                .ok_or(CopyFailure::UnsupportedStreams)?;
            Some(next_offset)
        } else {
            None
        };
        // SAFETY: the checked UTF-16 name range lies within the aligned buffer.
        let name = unsafe {
            std::slice::from_raw_parts(
                words.as_ptr().cast::<u8>().add(name_start).cast::<u16>(),
                bytes / 2,
            )
        };
        let name = String::from_utf16(name).map_err(|_| CopyFailure::UnsupportedStreams)?;
        let stream_size =
            u64::try_from(stream_size).map_err(|_| CopyFailure::UnsupportedStreams)?;
        if name.eq_ignore_ascii_case("::$DATA") && data.is_none() {
            data = Some(stream_size);
        } else if name.eq_ignore_ascii_case(":Zone.Identifier:$DATA")
            && zone.is_none()
            && stream_size <= MAX_ZONE_IDENTIFIER_BYTES
        {
            zone = Some(stream_size);
        } else {
            return Err(CopyFailure::UnsupportedStreams);
        }
        let Some(next_offset) = next_offset else {
            break;
        };
        offset = next_offset;
    }
    if data != Some(length) {
        return Err(CopyFailure::ContentMismatch);
    }
    Ok(StreamInventory { data: length, zone })
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
    control: &CopyControl,
    offset: u64,
    bytes: &[u8],
    mut write: impl FnMut(u64, &[u8]) -> Result<usize, CopyFailure>,
) -> Result<(), CopyFailure> {
    let mut written = 0;
    while written < bytes.len() {
        control.check()?;
        let progress = write(offset + written as u64, &bytes[written..])?;
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

fn check_output(
    file: &File,
    source: EntryMetadata,
    streams: StreamInventory,
) -> Result<(), CopyFailure> {
    let copied = observe_entry_metadata(file)?;
    if copied.kind != EntryKind::File || copied.byte_len != Some(streams.data) {
        return Err(CopyFailure::ContentMismatch);
    }
    if copied.created != source.created
        || copied.modified != source.modified
        || copied.attributes & !FILE_ATTRIBUTE_NORMAL.0
            != source.attributes & !FILE_ATTRIBUTE_NORMAL.0
    {
        return Err(CopyFailure::UnsupportedMetadata);
    }
    if stream_inventory(file, streams.data)? != streams {
        return Err(CopyFailure::ContentMismatch);
    }
    Ok(())
}

struct StagedCopy {
    data_hash: Option<[u8; 32]>,
    zone: Option<StagedZone>,
}

struct StagedZone {
    file: TransferFile,
    hash: [u8; 32],
    length: u64,
}

fn fill_stage(
    stage: &TransferStage,
    copy: &GuardedCopy<'_>,
    verify: bool,
    control: &CopyControl,
    mut progress: impl FnMut(u64),
) -> Result<StagedCopy, CopyFailure> {
    let mut hash = verify.then(Sha256::new);
    read_chunks(&copy.source_file, copy.length, control, |offset, bytes| {
        write_chunk(control, offset, bytes, |at, chunk| {
            stage
                .write(&control.io, at, chunk)
                .map_err(CopyFailure::from)
        })?;
        if let Some(hash) = &mut hash {
            hash.update(bytes);
        }
        progress(offset + bytes.len() as u64);
        Ok(())
    })?;
    let zone = copy
        .source_zone
        .as_ref()
        .map(|source| copy_zone(stage, source, control))
        .transpose()?;
    control.check()?;
    apply_metadata(stage, copy.source.metadata())?;
    control.check()?;
    stage.flush()?;
    check_output(stage.file(), copy.source.metadata(), copy.streams())?;
    copy.check_source(control)?;
    copy.check_target(control)?;
    Ok(StagedCopy {
        data_hash: hash.map(|hash| hash.finalize().into()),
        zone,
    })
}

fn copy_zone(
    stage: &TransferStage,
    source: &HeldZone,
    control: &CopyControl,
) -> Result<StagedZone, CopyFailure> {
    let claim = loop {
        control.check()?;
        let Some(epoch) = control.admission_epoch()? else {
            continue;
        };
        if let Some(claim) = control.try_claim(epoch)? {
            break claim;
        }
    };
    let output_result = stage.create_zone();
    drop(claim);
    let output = output_result?;
    let mut hash = Sha256::new();
    read_chunks(&source.file, source.length, control, |offset, bytes| {
        write_chunk(control, offset, bytes, |at, chunk| {
            control
                .io
                .write_at(&output, at, chunk.into())
                .map_err(CopyFailure::from)
        })?;
        hash.update(bytes);
        Ok(())
    })?;
    control.check()?;
    output.flush()?;
    let expected: [u8; 32] = hash.finalize().into();
    if hash_file(&output, source.length, control)? != expected {
        return Err(CopyFailure::ContentMismatch);
    }
    Ok(StagedZone {
        file: output,
        hash: expected,
        length: source.length,
    })
}

/// Publish an ordinary regular file to its approved vacant target. The
/// caller owns the surrounding job and any later move or recovery steps.
pub fn copy_to_vacancy(
    entry: &ApprovedTransferEntry,
    control: &CopyControl,
    progress: impl FnMut(u64),
) -> Result<CopiedFile, CopyError> {
    copy_with_publication_hooks(entry, control, progress, |_| {}, || {})
}

fn copy_with_publication_hooks(
    entry: &ApprovedTransferEntry,
    control: &CopyControl,
    progress: impl FnMut(u64),
    before_publication: impl FnOnce(&Path),
    after_publication: impl FnOnce(),
) -> Result<CopiedFile, CopyError> {
    let ApprovedTarget::CreateAt(target) = entry.destination() else {
        return Err(CopyFailure::UnsupportedPlan.into());
    };
    copy_file_bound(
        entry.source(),
        target,
        entry.verify_content(),
        control,
        progress,
        (before_publication, after_publication),
        |_| {},
    )
}

pub(super) fn copy_to_vacancy_reporting(
    entry: &ApprovedTransferEntry,
    control: &CopyControl,
    progress: impl FnMut(u64),
    verification: impl FnOnce(bool),
) -> Result<CopiedFile, CopyError> {
    let ApprovedTarget::CreateAt(target) = entry.destination() else {
        return Err(CopyFailure::UnsupportedPlan.into());
    };
    copy_file_bound(
        entry.source(),
        target,
        entry.verify_content(),
        control,
        progress,
        (|_| {}, || {}),
        verification,
    )
}

pub(super) fn copy_to_created_parent(
    entry: &ApprovedTransferEntry,
    target: &TargetSnapshot,
    control: &CopyControl,
    progress: impl FnMut(u64),
    verification: impl FnOnce(bool),
) -> Result<CopiedFile, CopyError> {
    copy_file_bound(
        entry.source(),
        target,
        entry.verify_content(),
        control,
        progress,
        (|_| {}, || {}),
        verification,
    )
}

fn copy_file_bound(
    source: &EntrySnapshot,
    target: &TargetSnapshot,
    verify_content: bool,
    control: &CopyControl,
    progress: impl FnMut(u64),
    hooks: (impl FnOnce(&Path), impl FnOnce()),
    verification_progress: impl FnOnce(bool),
) -> Result<CopiedFile, CopyError> {
    let copy = GuardedCopy::open(source, target, control, false)?;
    Ok(publish_vacant(
        copy,
        verify_content,
        control,
        progress,
        hooks,
        verification_progress,
        None,
    )?
    .copied)
}

struct VerifiedPublication<'a> {
    copy: GuardedCopy<'a>,
    published: TransferFile,
    staged: StagedCopy,
    copied: CopiedFile,
    revision: EntryMetadata,
}

fn publish_vacant<'a>(
    copy: GuardedCopy<'a>,
    verify_content: bool,
    control: &CopyControl,
    progress: impl FnMut(u64),
    hooks: (impl FnOnce(&Path), impl FnOnce()),
    verification_progress: impl FnOnce(bool),
    mut journal: Option<&mut dyn super::journal::TransferJournal>,
) -> Result<VerifiedPublication<'a>, CopyError> {
    use super::journal::TransferJournalEvent as Event;
    use rand::TryRng;
    if copy.source_relocation.is_some() && (!verify_content || journal.is_none()) {
        return Err(CopyFailure::UnsupportedPlan.into());
    }
    let (before_publication, after_publication) = hooks;
    let candidate = if let Some(recorder) = journal.as_deref_mut() {
        let number = rand::rngs::SysRng
            .try_next_u64()
            .map_err(|error| CopyFailure::Native(io::Error::other(error.to_string())))?;
        let name = format!(".multi-launcher-{number:016x}.stage");
        let leaf =
            super::TransferLeaf::new(std::ffi::OsStr::new(&name)).map_err(CopyFailure::from)?;
        super::check_target_bounds(copy.target.parent(), &leaf).map_err(CopyFailure::from)?;
        recorder
            .record(Event::StageIntent {
                path: copy.target.parent().path().join(leaf.as_os_str()),
                parent: copy.target.parent().identity(),
            })
            .map_err(CopyFailure::Journal)?;
        Some(leaf)
    } else {
        None
    };
    let (stage, stage_artifact) = start_guarded_stage(&copy, control, candidate.as_ref())?;
    if let Some(recorder) = journal.as_deref_mut() {
        if let Err(error) = recorder.record(Event::StageCreated {
            path: stage_artifact.path.clone(),
            parent: stage_artifact.parent,
            identity: stage_artifact.identity,
        }) {
            return Err(abort_stage(
                stage,
                stage_artifact,
                CopyFailure::Journal(error),
            ));
        }
    }
    let staged = match fill_stage(&stage, &copy, verify_content, control, progress) {
        Ok(hash) => hash,
        Err(failure) => return Err(abort_stage(stage, stage_artifact, failure)),
    };
    if let Some(recorder) = journal.as_deref_mut() {
        if let Err(error) = recorder.record(Event::PublishIntent {
            stage: stage_artifact.path.clone(),
            target: copy
                .target
                .parent()
                .path()
                .join(copy.target.requested_leaf().as_os_str()),
            parent: stage_artifact.parent,
            identity: stage_artifact.identity,
        }) {
            return Err(abort_stage(
                stage,
                stage_artifact,
                CopyFailure::Journal(error),
            ));
        }
    }
    let mut refused = None;
    let mut claim = None;
    let published_result = stage.publish(copy.target.requested_leaf().as_os_str(), || {
        before_publication(&stage_artifact.path);
        let recheck = (|| loop {
            control.check()?;
            let Some(epoch) = control.admission_epoch()? else {
                continue;
            };
            copy.check_source(control)?;
            copy.target_parent
                .check_chain(&control.cancellation)
                .map_err(CopyFailure::from)?;
            if let Some(acquired) = control.try_claim(epoch)? {
                claim = Some(acquired);
                return Ok(());
            }
        })();
        refused = recheck.err();
        refused.is_none()
    });
    drop(claim);
    let published = published_result.map_err(|error| {
        let artifact = if error.unconfirmed_name_effect() {
            Some(CopyRecoveryArtifact::UncertainPublication {
                stage: stage_artifact.path.clone(),
                target: copy
                    .target
                    .parent()
                    .path()
                    .join(copy.target.requested_leaf().as_os_str()),
                parent: stage_artifact.parent,
                identity: stage_artifact.identity,
            })
        } else {
            error
                .cleanup_failed()
                .then_some(CopyRecoveryArtifact::Owned(stage_artifact))
        };
        let recovery_error = artifact.as_ref().map(|_| error.to_string());
        let failure = refused.unwrap_or_else(|| {
            if error.kind() == io::ErrorKind::AlreadyExists {
                CopyFailure::DestinationOccupied
            } else {
                error.into()
            }
        });
        CopyError {
            failure,
            artifact,
            recovery_error,
            replacement: None,
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
    let named_revision = observe_entry_metadata(published.file()).map_err(CopyFailure::from);
    if let Some(recorder) = journal.as_deref_mut() {
        if let Err(error) = recorder.record(Event::PublicationObserved {
            target: artifact.path.clone(),
            parent: artifact.parent,
            identity: artifact.identity,
        }) {
            return Err(CopyError {
                failure: CopyFailure::Journal(error),
                artifact: Some(CopyRecoveryArtifact::Owned(artifact)),
                recovery_error: None,
                replacement: None,
            });
        }
    }
    after_publication();
    verification_progress(staged.data_hash.is_some() || staged.zone.is_some());
    let (verification, revision) = named_revision
        .and_then(|revision| {
            finish_publication(&copy, &published, &artifact, &staged, control, revision)
        })
        .map_err(|failure| CopyError {
            failure,
            artifact: Some(CopyRecoveryArtifact::Owned(artifact.clone())),
            recovery_error: None,
            replacement: None,
        })?;
    if let Some(recorder) = journal.as_deref_mut() {
        recorder
            .record(Event::Verified {
                target: artifact.path.clone(),
                parent: artifact.parent,
                identity: artifact.identity,
                verification,
                bytes: copy.length,
                revision,
            })
            .map_err(|error| CopyError {
                failure: CopyFailure::Journal(error),
                artifact: Some(CopyRecoveryArtifact::Owned(artifact.clone())),
                recovery_error: None,
                replacement: None,
            })?;
    }
    let copied = CopiedFile {
        source: copy.source.identity(),
        output: artifact,
        bytes: copy.length,
        zone_bytes: copy.streams().zone,
        verification,
        replacement: None,
    };
    Ok(VerifiedPublication {
        copy,
        published,
        staged,
        copied,
        revision,
    })
}

fn start_guarded_stage(
    copy: &GuardedCopy<'_>,
    control: &CopyControl,
    exact_candidate: Option<&super::TransferLeaf>,
) -> Result<(TransferStage, CopyArtifact), CopyError> {
    let claim = loop {
        control.check()?;
        let Some(epoch) = control.admission_epoch()? else {
            continue;
        };
        copy.check_source(control)?;
        copy.check_target(control)?;
        if let Some(claim) = control.try_claim(epoch)? {
            break claim;
        }
    };
    let directory = copy.target_parent.directory();
    let final_leaf = copy.target.requested_leaf().as_os_str();
    let result = if let Some(candidate) = exact_candidate {
        directory.create_stage_exact(final_leaf, candidate.as_os_str())
    } else {
        directory.create_stage(final_leaf)
    };
    drop(claim);
    let stage = result.map_err(|error| stage_create_error(copy.target, error))?;
    let artifact = CopyArtifact {
        path: copy.target.parent().path().join(stage.leaf()),
        parent: copy.target.parent().identity(),
        identity: stage.identity(),
        state: CopyArtifactState::StageNeedsReconciliation,
    };
    Ok((stage, artifact))
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
        replacement: None,
    }
}

fn finish_publication(
    copy: &GuardedCopy<'_>,
    published: &TransferFile,
    artifact: &CopyArtifact,
    staged: &StagedCopy,
    control: &CopyControl,
    named_revision: EntryMetadata,
) -> Result<(CopyVerification, EntryMetadata), CopyFailure> {
    let file = published.file();
    let source = copy.source.metadata();
    let output_revision = loop {
        control.check()?;
        let Some(epoch) = control.admission_epoch()? else {
            continue;
        };
        copy.target_parent.check_chain(control.cancellation())?;
        checked_final_path(file, &artifact.path)?;
        let current = observe_entry_metadata(file)?;
        if current != named_revision {
            return Err(AdmissionError::StaleEntry.into());
        }
        if current.modified != source.modified {
            return Err(CopyFailure::UnsupportedMetadata);
        }
        if current.created == source.created {
            break current;
        }
        // NTFS can inherit the former occupant's creation time on rename.
        // Restore the approved creation time on the held output.
        if let Some(claim) = control.try_claim(epoch)? {
            let result = set_replace_basic_info(
                file,
                FILE_BASIC_INFO {
                    CreationTime: source.created,
                    ..Default::default()
                },
            );
            drop(claim);
            result?;
            break observe_entry_metadata(file)?;
        }
    };
    file.sync_all()?;
    let streams = copy.streams();
    check_output(file, source, streams)?;
    let zone_sha256 = staged
        .zone
        .as_ref()
        .map(|zone| {
            if zone.file.identity() != artifact.identity
                || hash_file(&zone.file, zone.length, control)? != zone.hash
            {
                return Err(CopyFailure::ContentMismatch);
            }
            Ok(zone.hash)
        })
        .transpose()?;
    let verification = if let Some(expected_data) = staged.data_hash {
        let sha256 = hash_file(published, copy.length, control)?;
        if sha256 != expected_data {
            return Err(CopyFailure::ContentMismatch);
        }
        CopyVerification::Verified {
            sha256,
            zone_sha256,
        }
    } else {
        CopyVerification::NotRequested
    };
    copy.check_source(control)?;
    copy.target_parent.check_chain(control.cancellation())?;
    check_output(file, source, streams)?;
    if observe_entry_metadata(file)? != output_revision {
        return Err(AdmissionError::StaleEntry.into());
    }
    control.check()?;
    Ok((verification, output_revision))
}

pub(super) fn hash_file(
    file: &TransferFile,
    length: u64,
    control: &CopyControl,
) -> Result<[u8; 32], CopyFailure> {
    let mut hash = Sha256::new();
    read_chunks(file, length, control, |_, bytes| {
        hash.update(bytes);
        Ok(())
    })?;
    Ok(hash.finalize().into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_transfer::{
        ApprovedTransferPlan, TransferAction, TransferDraft, inspect_source_tree, review_transfer,
    };
    use std::fs;
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
                        sha256: Sha256::digest(bytes).into(),
                        zone_sha256: None,
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
        let error = copy_with_publication_hooks(
            &plan.entries()[0],
            &CopyControl::default(),
            |_| {},
            |_| {
                fs::write(&target, b"foreign winner").unwrap();
            },
            || {},
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
    fn transfer_copy_native_stream_visible_at_final_admission_blocks_publication() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source.txt");
        let dest = temp.path().join("output");
        fs::create_dir(&dest).unwrap();
        fs::write(&source, b"source payload").unwrap();
        let plan = approve_file(&source, &dest, true);
        let mut ads = source.clone().into_os_string();
        ads.push(":LateExtra:$DATA");
        let error = copy_with_publication_hooks(
            &plan.entries()[0],
            &CopyControl::default(),
            |_| {},
            |_| fs::write(Path::new(&ads), b"late stream").unwrap(),
            || {},
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
        assert!(error.artifact.is_none());
        assert_eq!(fs::read_dir(dest).unwrap().count(), 0);
        assert_eq!(fs::read(Path::new(&ads)).unwrap(), b"late stream");
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
        let error = copy_with_publication_hooks(
            &plan.entries()[0],
            &CopyControl::default(),
            |_| {},
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
    fn transfer_copy_native_zone_identifier_survives_copy_and_independent_verification() {
        let temp = tempfile::tempdir().unwrap();
        let dest = temp.path().join("output");
        fs::create_dir(&dest).unwrap();
        let data = b"ordinary downloaded file";
        let zone = b"[ZoneTransfer]\r\nZoneId=3\r\nHostUrl=https://example.invalid/file\r\n";
        let zone_path = |base: &Path| {
            let mut path = base.as_os_str().to_owned();
            path.push(":Zone.Identifier:$DATA");
            PathBuf::from(path)
        };

        for (name, verify) in [("verify.txt", true), ("copy.txt", false)] {
            let source = temp.path().join(name);
            fs::write(&source, data).unwrap();
            fs::write(zone_path(&source), zone).unwrap();
            let plan = approve_file(&source, &dest, verify);
            let receipt = copy_with_publication_hooks(
                &plan.entries()[0],
                &CopyControl::default(),
                |_| {},
                |staged| {
                    let writer = fs::OpenOptions::new().write(true).open(zone_path(staged));
                    assert_eq!(writer.unwrap_err().raw_os_error(), Some(32));
                },
                || {},
            )
            .unwrap();
            assert_eq!(receipt.zone_bytes, Some(zone.len() as u64));
            assert_eq!(receipt.bytes, data.len() as u64);
            assert_eq!(fs::read(&receipt.output.path).unwrap(), data);
            assert_eq!(fs::read(zone_path(&receipt.output.path)).unwrap(), zone);
            assert_eq!(fs::read(zone_path(&source)).unwrap(), zone);
            let recorded = plan.entries()[0].source().metadata();
            let observed = crate::file_transfer::inspect_source(
                &receipt.output.path,
                &ScanCancellation::new(),
            )
            .unwrap()
            .metadata();
            assert_eq!(observed.created, recorded.created);
            assert_eq!(observed.modified, recorded.modified);
            assert_eq!(
                receipt.verification,
                if verify {
                    CopyVerification::Verified {
                        sha256: Sha256::digest(data).into(),
                        zone_sha256: Some(Sha256::digest(zone).into()),
                    }
                } else {
                    CopyVerification::NotRequested
                },
            );
        }
        assert_eq!(fs::read_dir(dest).unwrap().count(), 2);
    }

    #[test]
    fn transfer_copy_native_stream_inventory_rejects_records_past_the_buffer() {
        let mut words = [0_u64; 8];
        words[0] = (0x200_u64 << 32) | 8;
        assert!(matches!(
            parse_stream_inventory(&words, 0),
            Err(CopyFailure::UnsupportedStreams)
        ));

        words[0] = (2_u64 << 32) | 0x1_0000;
        assert!(matches!(
            parse_stream_inventory(&words, 0),
            Err(CopyFailure::UnsupportedStreams)
        ));
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

// Only an explicitly approved occupied target may enter this path. The journal
// records each namespace transition before its corresponding native mutation.
pub(super) fn copy_to_replacement_reporting(
    entry: &ApprovedTransferEntry,
    control: &CopyControl,
    progress: impl FnMut(u64),
    verification_progress: impl FnOnce(bool),
    journal: &mut dyn super::journal::TransferJournal,
) -> Result<CopiedFile, CopyError> {
    use super::TransferLeaf;
    use super::journal::TransferJournalEvent as Event;
    use rand::TryRng;

    let ApprovedTarget::Replace(target) = entry.destination() else {
        return Err(CopyFailure::UnsupportedPlan.into());
    };
    let TargetOccupancy::Occupied(incumbent) = target.occupancy() else {
        return Err(CopyFailure::UnsupportedPlan.into());
    };
    if incumbent.metadata().kind != EntryKind::File {
        return Err(CopyFailure::UnsupportedPlan.into());
    }
    let copy = GuardedCopy::open(entry.source(), target, control, true)?;
    let parent = target.parent().identity();
    let original = incumbent.path();
    let final_path = target
        .parent()
        .path()
        .join(target.requested_leaf().as_os_str());
    let mut random = rand::rngs::SysRng;
    let stage_number = random
        .try_next_u64()
        .map_err(|e| CopyFailure::Native(io::Error::other(e.to_string())))?;
    let backup_number = random
        .try_next_u64()
        .map_err(|e| CopyFailure::Native(io::Error::other(e.to_string())))?;
    let stage_leaf = TransferLeaf::new(std::ffi::OsStr::new(&format!(
        ".multi-launcher-{stage_number:016x}.stage"
    )))
    .map_err(CopyFailure::from)?;
    let backup_leaf = TransferLeaf::new(std::ffi::OsStr::new(&format!(
        ".multi-launcher-{backup_number:016x}.backup"
    )))
    .map_err(CopyFailure::from)?;
    super::check_target_bounds(target.parent(), &stage_leaf).map_err(CopyFailure::from)?;
    super::check_target_bounds(target.parent(), &backup_leaf).map_err(CopyFailure::from)?;
    let stage_path = target.parent().path().join(stage_leaf.as_os_str());
    let backup_path = target.parent().path().join(backup_leaf.as_os_str());
    journal
        .record(Event::StageIntent {
            path: stage_path.clone(),
            parent,
        })
        .map_err(CopyFailure::Journal)?;
    let (stage, stage_artifact) = start_guarded_stage(&copy, control, Some(&stage_leaf))?;
    if let Err(error) = journal.record(Event::StageCreated {
        path: stage_path.clone(),
        parent,
        identity: stage.identity(),
    }) {
        return Err(abort_stage(
            stage,
            stage_artifact,
            CopyFailure::Journal(error),
        ));
    }
    let staged = match fill_stage(&stage, &copy, entry.verify_content(), control, progress) {
        Ok(staged) => staged,
        Err(failure) => return Err(abort_stage(stage, stage_artifact, failure)),
    };
    let held = match copy
        .target_parent
        .directory()
        .open_relocation(incumbent.leaf().as_os_str(), incumbent.identity())
    {
        Ok(held) => held,
        Err(error) => return Err(abort_stage(stage, stage_artifact, error.into())),
    };
    let preflight = (|| -> Result<(), CopyFailure> {
        if copy
            .target_parent
            .capture_entry(held.file(), incumbent.leaf())?
            != *incumbent
            || !exact_held_leaf(held.file(), &original)
        {
            return Err(AdmissionError::StaleEntry.into());
        }
        control.check()?;
        copy.check_source(control)?;
        copy.check_target(control)?;
        Ok(())
    })();
    if let Err(failure) = preflight {
        return Err(abort_stage(stage, stage_artifact, failure));
    }
    let mut backup = ReplacementBackup {
        original: original.clone(),
        backup: backup_path.clone(),
        parent,
        identity: incumbent.identity(),
        state: ReplacementState::Uncertain,
    };
    if let Err(error) = journal.record(Event::DisplacementIntent {
        original: original.clone(),
        backup: backup_path.clone(),
        parent,
        identity: incumbent.identity(),
    }) {
        return Err(abort_stage(
            stage,
            stage_artifact,
            CopyFailure::Journal(error),
        ));
    }
    let claim = loop {
        let check = (|| -> Result<Option<_>, CopyFailure> {
            control.check()?;
            let Some(epoch) = control.admission_epoch()? else {
                return Ok(None);
            };
            copy.check_source(control)?;
            copy.check_target(control)?;
            if copy
                .target_parent
                .capture_entry(held.file(), incumbent.leaf())?
                != *incumbent
                || !exact_held_leaf(held.file(), &original)
            {
                return Err(AdmissionError::StaleEntry.into());
            }
            control.try_claim(epoch)
        })();
        match check {
            Ok(Some(claim)) => break claim,
            Ok(None) => continue,
            Err(failure) => return Err(abort_stage(stage, stage_artifact, failure)),
        }
    };
    let displaced = held.rename_no_replace(copy.target_parent.directory(), backup_leaf.as_os_str());
    drop(claim);
    if let Err(error) = displaced {
        let original_proven = exact_held_leaf(held.file(), &original);
        let backup_proven = exact_held_leaf(held.file(), &backup_path);
        if original_proven {
            return Err(abort_stage(stage, stage_artifact, error.into()));
        }
        backup.state = if backup_proven {
            ReplacementState::Retained
        } else {
            ReplacementState::Uncertain
        };
        let mut failure = abort_stage(stage, stage_artifact, error.into());
        failure.replacement = Some(backup);
        if backup_proven {
            match journal.record(Event::DisplacementObserved {
                original: original.clone(),
                backup: backup_path.clone(),
                parent,
                identity: incumbent.identity(),
            }) {
                Ok(()) => restore_incumbent(&copy, &held, journal, &mut failure),
                Err(error) => {
                    append_recovery_diagnostic(&mut failure.recovery_error, &failure.failure);
                    failure.failure = CopyFailure::Journal(error);
                }
            }
        }
        return Err(failure);
    }
    if !exact_held_leaf(held.file(), &backup_path) {
        let mut failure = abort_stage(stage, stage_artifact, CopyFailure::ContentMismatch);
        failure.replacement = Some(backup);
        return Err(failure);
    }
    backup.state = ReplacementState::Retained;
    if let Err(error) = journal.record(Event::DisplacementObserved {
        original: original.clone(),
        backup: backup_path.clone(),
        parent,
        identity: incumbent.identity(),
    }) {
        let mut failure = abort_stage(stage, stage_artifact, CopyFailure::Journal(error));
        failure.replacement = Some(backup);
        return Err(failure);
    }
    if let Err(error) = journal.record(Event::PublishIntent {
        stage: stage_path.clone(),
        target: final_path.clone(),
        parent,
        identity: stage_artifact.identity,
    }) {
        let mut failure = abort_stage(stage, stage_artifact, CopyFailure::Journal(error));
        failure.replacement = Some(backup);
        return Err(failure);
    }
    let mut refused = None;
    let mut claim = None;
    let result = stage.publish(target.requested_leaf().as_os_str(), || {
        let check = (|| loop {
            control.check()?;
            let Some(epoch) = control.admission_epoch()? else {
                continue;
            };
            copy.check_source(control)?;
            copy.check_vacancy_after_displacement(control)?;
            if let Some(acquired) = control.try_claim(epoch)? {
                claim = Some(acquired);
                return Ok(());
            }
        })();
        refused = check.err();
        refused.is_none()
    });
    drop(claim);
    let published = match result {
        Ok(file) => file,
        Err(error) => {
            let ambiguous = error.unconfirmed_name_effect();
            let artifact = if ambiguous {
                Some(CopyRecoveryArtifact::UncertainPublication {
                    stage: stage_path,
                    target: final_path.clone(),
                    parent,
                    identity: stage_artifact.identity,
                })
            } else if error.cleanup_failed() {
                Some(CopyRecoveryArtifact::Owned(stage_artifact))
            } else {
                None
            };
            let recovery_error = artifact.as_ref().map(|_| error.to_string());
            let failure = refused.unwrap_or_else(|| {
                if error.kind() == io::ErrorKind::AlreadyExists {
                    CopyFailure::DestinationOccupied
                } else {
                    error.into()
                }
            });
            let mut outcome = CopyError {
                failure,
                artifact,
                recovery_error,
                replacement: Some(backup),
            };
            if !ambiguous {
                restore_incumbent(&copy, &held, journal, &mut outcome);
            }
            return Err(outcome);
        }
    };
    let artifact = CopyArtifact {
        path: final_path.clone(),
        parent,
        identity: published.identity(),
        state: CopyArtifactState::Published,
    };
    let named_revision = observe_entry_metadata(published.file()).map_err(CopyFailure::from);
    if let Err(error) = journal.record(Event::PublicationObserved {
        target: final_path.clone(),
        parent,
        identity: artifact.identity,
    }) {
        return Err(CopyError {
            failure: CopyFailure::Journal(error),
            artifact: Some(CopyRecoveryArtifact::Owned(artifact)),
            recovery_error: None,
            replacement: Some(backup),
        });
    }
    verification_progress(staged.data_hash.is_some() || staged.zone.is_some());
    let (verification, revision) = named_revision
        .and_then(|revision| {
            finish_publication(&copy, &published, &artifact, &staged, control, revision)
        })
        .map_err(|failure| CopyError {
            failure,
            artifact: Some(CopyRecoveryArtifact::Owned(artifact.clone())),
            recovery_error: None,
            replacement: Some(backup.clone()),
        })?;
    journal
        .record(Event::Verified {
            target: final_path,
            parent,
            identity: artifact.identity,
            verification,
            bytes: copy.length,
            revision,
        })
        .map_err(|error| CopyError {
            failure: CopyFailure::Journal(error),
            artifact: Some(CopyRecoveryArtifact::Owned(artifact.clone())),
            recovery_error: None,
            replacement: Some(backup.clone()),
        })?;
    Ok(CopiedFile {
        source: copy.source.identity(),
        output: artifact,
        bytes: copy.length,
        zone_bytes: copy.streams().zone,
        verification,
        replacement: Some(backup),
    })
}

pub(super) fn exact_held_leaf(file: &File, expected: &Path) -> bool {
    checked_final_path(file, expected)
        .is_ok_and(|actual| actual.file_name() == expected.file_name())
}

fn abort_stage(stage: TransferStage, artifact: CopyArtifact, failure: CopyFailure) -> CopyError {
    match stage.abort() {
        Ok(()) => failure.into(),
        Err(error) => CopyError {
            failure,
            artifact: Some(CopyRecoveryArtifact::Owned(artifact)),
            recovery_error: Some(error.to_string()),
            replacement: None,
        },
    }
}

fn append_recovery_diagnostic(detail: &mut Option<String>, error: impl fmt::Display) {
    match detail {
        Some(detail) => {
            use std::fmt::Write;
            let _ = write!(detail, "\n{error}");
        }
        None => *detail = Some(error.to_string()),
    }
}

fn restore_incumbent(
    copy: &GuardedCopy<'_>,
    held: &crate::archive::transfer_native::TransferRelocation,
    journal: &mut dyn super::journal::TransferJournal,
    outcome: &mut CopyError,
) {
    use super::journal::TransferJournalEvent as Event;
    let Some(backup) = outcome.replacement.as_mut() else {
        return;
    };
    let fresh = ScanCancellation::new();
    if !exact_held_leaf(held.file(), &backup.backup)
        || !matches!(
            copy.target_parent
                .inspect_leaf(copy.target.requested_leaf(), &fresh),
            Ok(TargetOccupancy::Vacant)
        )
    {
        return;
    }
    let event = || Event::RestoreIntent {
        original: backup.original.clone(),
        backup: backup.backup.clone(),
        parent: backup.parent,
        identity: backup.identity,
    };
    if let Err(error) = journal.record(event()) {
        append_recovery_diagnostic(&mut outcome.recovery_error, &outcome.failure);
        outcome.failure = CopyFailure::Journal(error);
        return;
    }
    if !exact_held_leaf(held.file(), &backup.backup) {
        backup.state = ReplacementState::Uncertain;
        return;
    }
    if !matches!(
        copy.target_parent
            .inspect_leaf(copy.target.requested_leaf(), &fresh),
        Ok(TargetOccupancy::Vacant)
    ) {
        return;
    }
    // No-replace protects a winner that appears after the vacancy check.
    let result = held.rename_no_replace(
        copy.target_parent.directory(),
        backup.original.file_name().unwrap_or_default(),
    );
    if exact_held_leaf(held.file(), &backup.original) {
        backup.state = ReplacementState::Restored;
        if let Err(error) = journal.record(Event::RestoreObserved {
            original: backup.original.clone(),
            backup: backup.backup.clone(),
            parent: backup.parent,
            identity: backup.identity,
        }) {
            append_recovery_diagnostic(&mut outcome.recovery_error, &outcome.failure);
            outcome.failure = CopyFailure::Journal(error);
        }
    } else if !exact_held_leaf(held.file(), &backup.backup) {
        backup.state = ReplacementState::Uncertain;
    }
    if let Err(error) = result {
        append_recovery_diagnostic(&mut outcome.recovery_error, error);
    }
}
