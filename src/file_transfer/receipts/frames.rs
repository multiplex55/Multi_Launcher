//! Bounded, checksummed append-only receipt frames. Payloads are opaque here.
use sha2::{Digest, Sha256};
use std::fs::File;
#[cfg(test)]
use std::fs::OpenOptions;
use std::io::{self, Read, Seek, SeekFrom, Write};
#[cfg(test)]
use std::os::windows::fs::OpenOptionsExt;
#[cfg(test)]
use std::path::Path;
#[cfg(test)]
use windows::Win32::Storage::FileSystem::{FILE_FLAG_WRITE_THROUGH, FILE_SHARE_READ};

pub(super) const MAX_FRAME_BYTES: usize = 512 * 1024;
pub(super) const MAX_RECEIPT_BYTES: u64 = 32 * 1024 * 1024;
const MAGIC: &[u8; 8] = b"MLRCPT\r\n";
const VERSION: u32 = 1;
const FILE_HEADER: u64 = 12;
const FRAME_HEADER: u64 = 12;
const DIGEST_BYTES: u64 = 32;

#[derive(Debug)]
pub(super) enum FrameWriteError {
    InvalidCapacity,
    FrameTooLarge,
    Capacity,
    SequenceExhausted,
    Poisoned,
    Io(io::Error),
}

pub(super) struct FrameWriter {
    file: File,
    capacity: u64,
    written: u64,
    next: Option<u64>,
    poisoned: bool,
}

impl FrameWriter {
    /// The caller owns the exact target path; an existing file is never opened.
    #[cfg(test)]
    pub(super) fn create(path: &Path, capacity: u64) -> Result<Self, FrameWriteError> {
        if !(FILE_HEADER..=MAX_RECEIPT_BYTES).contains(&capacity) {
            return Err(FrameWriteError::InvalidCapacity);
        }
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .share_mode(FILE_SHARE_READ.0)
            .custom_flags(FILE_FLAG_WRITE_THROUGH.0)
            .open(path)
            .map_err(FrameWriteError::Io)?;
        Self::from_new_file(file, capacity)
    }

    /// Takes an already-held, newly created synchronous file. On header failure
    /// its bytes are left in place for recovery rather than removed.
    pub(super) fn from_new_file(mut file: File, capacity: u64) -> Result<Self, FrameWriteError> {
        if !(FILE_HEADER..=MAX_RECEIPT_BYTES).contains(&capacity) {
            return Err(FrameWriteError::InvalidCapacity);
        }
        file.write_all(MAGIC)
            .and_then(|()| file.write_all(&VERSION.to_le_bytes()))
            .and_then(|()| file.flush())
            .and_then(|()| file.sync_all())
            .map_err(FrameWriteError::Io)?;
        Ok(Self {
            file,
            capacity,
            written: FILE_HEADER,
            next: Some(0),
            poisoned: false,
        })
    }

    pub(super) fn remaining_bytes(&self) -> u64 {
        self.capacity.saturating_sub(self.written)
    }

    pub(super) fn frame_size(payload_len: usize) -> Option<u64> {
        if payload_len > MAX_FRAME_BYTES {
            return None;
        }
        FRAME_HEADER
            .checked_add(u64::try_from(payload_len).ok()?)?
            .checked_add(DIGEST_BYTES)
    }

    pub(super) fn append(&mut self, payload: &[u8]) -> Result<(), FrameWriteError> {
        if self.poisoned {
            return Err(FrameWriteError::Poisoned);
        }
        if payload.len() > MAX_FRAME_BYTES {
            return Err(FrameWriteError::FrameTooLarge);
        }
        let sequence = self.next.ok_or(FrameWriteError::SequenceExhausted)?;
        let length = u32::try_from(payload.len()).map_err(|_| FrameWriteError::FrameTooLarge)?;
        let frame_size = FRAME_HEADER + u64::from(length) + DIGEST_BYTES;
        let end = self
            .written
            .checked_add(frame_size)
            .ok_or(FrameWriteError::Capacity)?;
        if end > self.capacity {
            return Err(FrameWriteError::Capacity);
        }
        let seq_bytes = sequence.to_le_bytes();
        let len_bytes = length.to_le_bytes();
        let mut hash = Sha256::new();
        hash.update(seq_bytes);
        hash.update(len_bytes);
        hash.update(payload);
        let checksum = hash.finalize();
        let write = self
            .file
            .write_all(&seq_bytes)
            .and_then(|()| self.file.write_all(&len_bytes))
            .and_then(|()| self.file.write_all(payload))
            .and_then(|()| self.file.write_all(&checksum))
            .and_then(|()| self.file.flush())
            .and_then(|()| self.file.sync_all());
        if let Err(error) = write {
            self.poisoned = true;
            return Err(FrameWriteError::Io(error));
        }
        self.written = end;
        self.next = sequence.checked_add(1);
        Ok(())
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum FrameProblem {
    Clean,
    WrongMagic,
    UnsupportedVersion(u32),
    Truncated {
        offset: u64,
    },
    Sequence {
        offset: u64,
        expected: u64,
        actual: u64,
    },
    OversizedFrame {
        offset: u64,
        length: u32,
    },
    Corrupt {
        offset: u64,
    },
    Limit {
        offset: u64,
    },
}

#[derive(Debug)]
pub(super) struct FrameRead {
    pub records: Vec<Vec<u8>>,
    pub problem: FrameProblem,
}

/// Reads only a valid prefix and never alters the receipt. Bounds apply to the
/// whole file, including header and checksums, before any frame allocation.
#[cfg(test)]
pub(super) fn read_prefix(path: &Path, capacity: u64) -> io::Result<FrameRead> {
    let mut file = File::open(path)?;
    read_prefix_from_file(&mut file, capacity)
}

pub(super) fn read_prefix_from_file(file: &mut File, capacity: u64) -> io::Result<FrameRead> {
    if !(FILE_HEADER..=MAX_RECEIPT_BYTES).contains(&capacity) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid receipt capacity",
        ));
    }
    file.seek(SeekFrom::Start(0))?;
    let mut result = FrameRead {
        records: Vec::new(),
        problem: FrameProblem::Clean,
    };
    let mut header = [0u8; FILE_HEADER as usize];
    if !read_complete(file, &mut header)? {
        result.problem = FrameProblem::Truncated { offset: 0 };
        return Ok(result);
    }
    if &header[..8] != MAGIC {
        result.problem = FrameProblem::WrongMagic;
        return Ok(result);
    }
    let version = u32::from_le_bytes(header[8..12].try_into().unwrap());
    if version != VERSION {
        result.problem = FrameProblem::UnsupportedVersion(version);
        return Ok(result);
    }
    let mut offset = FILE_HEADER;
    let mut next = Some(0u64);
    loop {
        let mut frame_header = [0u8; FRAME_HEADER as usize];
        let first = file.read(&mut frame_header[..1])?;
        if first == 0 {
            return Ok(result);
        }
        if capacity.saturating_sub(offset) < FRAME_HEADER + DIGEST_BYTES {
            result.problem = FrameProblem::Limit { offset };
            return Ok(result);
        }
        if !read_complete(file, &mut frame_header[1..])? {
            result.problem = FrameProblem::Truncated { offset };
            return Ok(result);
        }
        let sequence = u64::from_le_bytes(frame_header[..8].try_into().unwrap());
        let length = u32::from_le_bytes(frame_header[8..].try_into().unwrap());
        let Some(expected) = next else {
            result.problem = FrameProblem::Limit { offset };
            return Ok(result);
        };
        if sequence != expected {
            result.problem = FrameProblem::Sequence {
                offset,
                expected,
                actual: sequence,
            };
            return Ok(result);
        }
        if length as usize > MAX_FRAME_BYTES {
            result.problem = FrameProblem::OversizedFrame { offset, length };
            return Ok(result);
        }
        let Some(end) = offset.checked_add(FRAME_HEADER + u64::from(length) + DIGEST_BYTES) else {
            result.problem = FrameProblem::Limit { offset };
            return Ok(result);
        };
        if end > capacity {
            result.problem = FrameProblem::Limit { offset };
            return Ok(result);
        }
        let mut payload = vec![0; length as usize];
        let mut actual = [0u8; DIGEST_BYTES as usize];
        if !read_complete(file, &mut payload)? || !read_complete(file, &mut actual)? {
            result.problem = FrameProblem::Truncated { offset };
            return Ok(result);
        }
        let mut hash = Sha256::new();
        hash.update(frame_header);
        hash.update(&payload);
        if hash.finalize().as_slice() != actual {
            result.problem = FrameProblem::Corrupt { offset };
            return Ok(result);
        }
        result.records.push(payload);
        offset = end;
        next = expected.checked_add(1);
    }
}

fn read_complete(file: &mut File, bytes: &mut [u8]) -> io::Result<bool> {
    match file.read_exact(bytes) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => Ok(false),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn receipt_frames_use_preopened_files_and_count_capacity() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("preopened");
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        let mut writer = FrameWriter::from_new_file(file, 128).unwrap();
        let before = writer.remaining_bytes();
        let size = FrameWriter::frame_size(3).unwrap();
        writer.append(b"abc").unwrap();
        assert_eq!(writer.remaining_bytes(), before - size);
        assert!(FrameWriter::frame_size(MAX_FRAME_BYTES + 1).is_none());
        drop(writer);
        let mut file = File::open(path).unwrap();
        let result = read_prefix_from_file(&mut file, 128).unwrap();
        assert_eq!(result.records, [b"abc".to_vec()]);
        assert_eq!(result.problem, FrameProblem::Clean);
    }

    #[test]
    fn receipt_frames_roundtrip_and_create_never_clobbers() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("receipt");
        let mut writer = FrameWriter::create(&path, 4096).unwrap();
        writer.append(b"header").unwrap();
        writer.append(b"event").unwrap();
        assert!(
            matches!(FrameWriter::create(&path, 4096), Err(FrameWriteError::Io(error)) if error.kind() == io::ErrorKind::AlreadyExists)
        );
        let read = read_prefix(&path, 4096).unwrap();
        assert_eq!(read.records, [b"header".to_vec(), b"event".to_vec()]);
        assert_eq!(read.problem, FrameProblem::Clean);
    }

    #[test]
    fn receipt_frames_preserve_prefix_for_corruption_sequence_and_truncation() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("receipt");
        let mut writer = FrameWriter::create(&path, 4096).unwrap();
        writer.append(b"first").unwrap();
        writer.append(b"second").unwrap();
        writer.append(b"third after the invalid frame").unwrap();
        drop(writer);
        let bytes = fs::read(&path).unwrap();
        let second = FILE_HEADER as usize + FRAME_HEADER as usize + 5 + DIGEST_BYTES as usize;
        let mut changed = bytes.clone();
        changed[second] = 7;
        fs::write(&path, &changed).unwrap();
        let read = read_prefix(&path, 4096).unwrap();
        assert_eq!(read.records, [b"first".to_vec()]);
        assert!(matches!(
            read.problem,
            FrameProblem::Sequence {
                expected: 1,
                actual: 7,
                ..
            }
        ));
        let mut changed = bytes.clone();
        changed[second + FRAME_HEADER as usize] ^= 1;
        fs::write(&path, &changed).unwrap();
        let read = read_prefix(&path, 4096).unwrap();
        assert_eq!(read.records, [b"first".to_vec()]);
        assert!(matches!(read.problem, FrameProblem::Corrupt { .. }));
        let truncated_second = second + FRAME_HEADER as usize + 6 + DIGEST_BYTES as usize - 1;
        fs::write(&path, &bytes[..truncated_second]).unwrap();
        let read = read_prefix(&path, 4096).unwrap();
        assert_eq!(read.records, [b"first".to_vec()]);
        assert!(matches!(read.problem, FrameProblem::Truncated { .. }));
    }

    #[test]
    fn receipt_frames_capacity_rejection_has_no_write() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("receipt");
        let exact = FILE_HEADER + FRAME_HEADER + DIGEST_BYTES + 3;
        let mut writer = FrameWriter::create(&path, exact).unwrap();
        writer.append(b"abc").unwrap();
        let before = fs::read(&path).unwrap();
        assert!(matches!(writer.append(b""), Err(FrameWriteError::Capacity)));
        assert!(matches!(
            writer.append(&vec![0; MAX_FRAME_BYTES + 1]),
            Err(FrameWriteError::FrameTooLarge)
        ));
        assert_eq!(fs::read(&path).unwrap(), before);
        assert_eq!(
            read_prefix(&path, exact).unwrap().problem,
            FrameProblem::Clean
        );
    }

    #[test]
    fn receipt_frames_active_writer_denies_external_mutation_and_rename() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("active");
        let other = dir.path().join("moved");
        let mut writer = FrameWriter::create(&path, 4096).unwrap();
        writer.append(b"intent").unwrap();
        assert!(fs::write(&path, b"foreign write").is_err());
        assert!(fs::rename(&path, &other).is_err());
        assert!(fs::remove_file(&path).is_err());
        assert_eq!(
            read_prefix(&path, 4096).unwrap().records,
            [b"intent".to_vec()]
        );
        drop(writer);
        fs::rename(&path, &other).unwrap();
        assert_eq!(
            read_prefix(&other, 4096).unwrap().records,
            [b"intent".to_vec()]
        );
    }

    #[test]
    fn receipt_frames_short_tail_cannot_exceed_reader_capacity() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("receipt");
        let mut writer = FrameWriter::create(&path, 4096).unwrap();
        writer.append(b"record").unwrap();
        drop(writer);
        let mut bytes = fs::read(&path).unwrap();
        let offset = bytes.len() as u64;
        bytes.push(0);
        fs::write(&path, bytes).unwrap();
        let read = read_prefix(&path, offset + 1).unwrap();
        assert_eq!(read.records, [b"record".to_vec()]);
        assert_eq!(read.problem, FrameProblem::Limit { offset });
    }

    #[test]
    fn receipt_frames_reject_bad_file_header() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("receipt");
        fs::write(&path, [0u8; FILE_HEADER as usize]).unwrap();
        assert_eq!(
            read_prefix(&path, 4096).unwrap().problem,
            FrameProblem::WrongMagic
        );
        let mut header = Vec::from(MAGIC.as_slice());
        header.extend_from_slice(&2u32.to_le_bytes());
        fs::write(&path, header).unwrap();
        assert_eq!(
            read_prefix(&path, 4096).unwrap().problem,
            FrameProblem::UnsupportedVersion(2)
        );
    }
}
