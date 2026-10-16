//! Hashing primitives and a nonblocking worker for the Hash Generator.
//!
//! Inputs are hashed as exact bytes: text and clipboard values use their UTF-8
//! encoding without trimming or newline normalization, while files are read in
//! bounded chunks. The module has no GUI or file-writing responsibilities.

use sha1::Sha1;
use sha2::{Digest, Sha256, Sha512};
use std::error::Error;
use std::fmt;
use std::fs::File;
use std::io::{self, Read};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};

pub const HASH_BUFFER_SIZE: usize = 256 * 1024;

static NEXT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HashAlgorithm {
    Sha256,
    Sha512,
    Sha1,
}

impl HashAlgorithm {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Sha256 => "SHA-256",
            Self::Sha512 => "SHA-512",
            Self::Sha1 => "SHA-1",
        }
    }

    pub const fn digest_bytes(self) -> usize {
        match self {
            Self::Sha256 => 32,
            Self::Sha512 => 64,
            Self::Sha1 => 20,
        }
    }

    pub const fn security_notice(self) -> Option<&'static str> {
        match self {
            Self::Sha1 => Some(
                "SHA-1 is for legacy checksum compatibility and is not collision-resistant for modern security decisions.",
            ),
            Self::Sha256 | Self::Sha512 => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HashInputMode {
    Text,
    Clipboard,
    File,
}

/// The text and clipboard variants share exact UTF-8 hashing semantics.
/// Debug output never includes text or a file path.
pub enum HashInput {
    Text(String),
    Clipboard(String),
    File(PathBuf),
}

impl HashInput {
    pub const fn mode(&self) -> HashInputMode {
        match self {
            Self::Text(_) => HashInputMode::Text,
            Self::Clipboard(_) => HashInputMode::Clipboard,
            Self::File(_) => HashInputMode::File,
        }
    }
}

impl fmt::Debug for HashInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HashInput")
            .field("mode", &self.mode())
            .finish_non_exhaustive()
    }
}

/// A validated request. Its identifier travels with progress and the terminal
/// result so a caller can reject stale work after editing its input.
pub struct HashRequest {
    request_id: HashRequestId,
    algorithm: HashAlgorithm,
    input: HashInput,
    expected: Option<ExpectedDigest>,
}

impl HashRequest {
    pub fn new(
        algorithm: HashAlgorithm,
        input: HashInput,
        expected_hash: Option<&str>,
    ) -> Result<Self, HashError> {
        let expected = expected_hash
            .map(|value| parse_expected(algorithm, value))
            .transpose()?
            .flatten();
        Ok(Self {
            request_id: HashRequestId(NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed)),
            algorithm,
            input,
            expected,
        })
    }

    pub const fn request_id(&self) -> HashRequestId {
        self.request_id
    }

    pub const fn algorithm(&self) -> HashAlgorithm {
        self.algorithm
    }

    pub const fn input_mode(&self) -> HashInputMode {
        self.input.mode()
    }
}

impl fmt::Debug for HashRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HashRequest")
            .field("request_id", &self.request_id)
            .field("algorithm", &self.algorithm)
            .field("input", &self.input)
            .field("has_expected_hash", &self.expected.is_some())
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HashRequestId(u64);

impl HashRequestId {
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HashProgress {
    pub request_id: HashRequestId,
    pub bytes_processed: u64,
    pub total_bytes: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HashComparison {
    NotRequested,
    Match,
    Mismatch,
}

/// The digest string is lowercase hexadecimal. Debug intentionally omits it.
pub struct HashResult {
    pub request_id: HashRequestId,
    pub algorithm: HashAlgorithm,
    pub input_mode: HashInputMode,
    pub bytes_processed: u64,
    pub digest: String,
    pub comparison: HashComparison,
}

impl fmt::Debug for HashResult {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HashResult")
            .field("request_id", &self.request_id)
            .field("algorithm", &self.algorithm)
            .field("input_mode", &self.input_mode)
            .field("bytes_processed", &self.bytes_processed)
            .field("comparison", &self.comparison)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpectedHashError {
    WrongLength { expected: usize, actual: usize },
    NonHexadecimal,
}

impl fmt::Display for ExpectedHashError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongLength { expected, actual } => write!(
                formatter,
                "Expected a {expected}-character hexadecimal digest, received {actual} characters"
            ),
            Self::NonHexadecimal => {
                formatter.write_str("Expected digest contains non-hexadecimal characters")
            }
        }
    }
}

impl Error for ExpectedHashError {}

#[derive(Debug)]
pub enum HashError {
    Cancelled,
    InvalidExpectedHash(ExpectedHashError),
    Io(io::Error),
    Spawn(io::Error),
    WorkerDisconnected,
    WorkerPanicked,
}

impl fmt::Display for HashError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str("Hash operation cancelled"),
            Self::InvalidExpectedHash(error) => error.fmt(formatter),
            Self::Io(error) => write!(formatter, "Could not read hash input: {error}"),
            Self::Spawn(error) => write!(formatter, "Could not start hash worker: {error}"),
            Self::WorkerDisconnected => {
                formatter.write_str("Hash worker disconnected before completion")
            }
            Self::WorkerPanicked => formatter.write_str("Hash worker failed unexpectedly"),
        }
    }
}

impl Error for HashError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidExpectedHash(error) => Some(error),
            Self::Io(error) | Self::Spawn(error) => Some(error),
            Self::Cancelled | Self::WorkerDisconnected | Self::WorkerPanicked => None,
        }
    }
}

impl From<ExpectedHashError> for HashError {
    fn from(error: ExpectedHashError) -> Self {
        Self::InvalidExpectedHash(error)
    }
}

#[derive(Clone)]
pub struct HashCancellationToken(Arc<AtomicBool>);

impl HashCancellationToken {
    pub fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }

    fn checkpoint(&self) -> Result<(), HashError> {
        if self.is_cancelled() {
            Err(HashError::Cancelled)
        } else {
            Ok(())
        }
    }
}

impl Default for HashCancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for HashCancellationToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HashCancellationToken")
            .field("cancelled", &self.is_cancelled())
            .finish()
    }
}

/// ASCII whitespace around an expected digest is ignored. An empty or
/// whitespace-only value disables comparison; internal whitespace is invalid.
pub fn parse_expected_digest(
    algorithm: HashAlgorithm,
    expected: &str,
) -> Result<Option<Vec<u8>>, ExpectedHashError> {
    let bytes = expected.as_bytes();
    let start = bytes
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|byte| !byte.is_ascii_whitespace())
        .map_or(start, |index| index + 1);
    let normalized = &bytes[start..end];
    if normalized.is_empty() {
        return Ok(None);
    }

    let expected_len = algorithm.digest_bytes() * 2;
    if normalized.len() != expected_len {
        return Err(ExpectedHashError::WrongLength {
            expected: expected_len,
            actual: normalized.len(),
        });
    }
    if !normalized.iter().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ExpectedHashError::NonHexadecimal);
    }

    // ASCII validation above makes this conversion infallible and keeps the
    // accepted normalization rules explicit.
    let normalized = std::str::from_utf8(normalized).expect("validated ASCII hex");
    Ok(Some(hex::decode(normalized).expect("validated ASCII hex")))
}

/// Synchronous, cancellable engine used by the worker and deterministic tests.
/// The progress callback receives snapshots; callers may coalesce them.
pub fn hash_request(
    request: HashRequest,
    cancellation: &HashCancellationToken,
    mut publish_progress: impl FnMut(HashProgress),
) -> Result<HashResult, HashError> {
    cancellation.checkpoint()?;
    let request_id = request.request_id;
    let algorithm = request.algorithm;
    let input_mode = request.input.mode();
    let expected = request.expected;
    let mut hasher = HashState::new(algorithm);
    let mut bytes_processed = 0_u64;

    match request.input {
        HashInput::Text(text) | HashInput::Clipboard(text) => {
            let source_bytes = text.as_bytes();
            let total_bytes = u64::try_from(source_bytes.len()).ok();
            publish_progress(HashProgress {
                request_id,
                bytes_processed,
                total_bytes,
            });
            for chunk in source_bytes.chunks(HASH_BUFFER_SIZE) {
                cancellation.checkpoint()?;
                hasher.update(chunk);
                bytes_processed = bytes_processed.saturating_add(chunk.len() as u64);
                publish_progress(HashProgress {
                    request_id,
                    bytes_processed,
                    total_bytes,
                });
            }
        }
        HashInput::File(path) => {
            let mut file = File::open(path).map_err(HashError::Io)?;
            let total_bytes = file.metadata().ok().map(|metadata| metadata.len());
            publish_progress(HashProgress {
                request_id,
                bytes_processed,
                total_bytes,
            });
            hash_reader(
                &mut file,
                cancellation,
                &mut bytes_processed,
                |processed| {
                    publish_progress(HashProgress {
                        request_id,
                        bytes_processed: processed,
                        total_bytes,
                    });
                },
                &mut hasher,
            )?;
        }
    }

    cancellation.checkpoint()?;
    let digest_bytes = hasher.finish();
    let digest = hex::encode(&digest_bytes);
    let comparison = match expected {
        Some(ExpectedDigest(expected)) if expected == digest_bytes => HashComparison::Match,
        Some(_) => HashComparison::Mismatch,
        None => HashComparison::NotRequested,
    };
    Ok(HashResult {
        request_id,
        algorithm,
        input_mode,
        bytes_processed,
        digest,
        comparison,
    })
}

fn hash_reader(
    reader: &mut impl Read,
    cancellation: &HashCancellationToken,
    bytes_processed: &mut u64,
    mut publish_progress: impl FnMut(u64),
    hasher: &mut HashState,
) -> Result<(), HashError> {
    let mut buffer = vec![0_u8; HASH_BUFFER_SIZE];
    loop {
        cancellation.checkpoint()?;
        let read = match reader.read(&mut buffer) {
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(HashError::Io(error)),
        };
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        *bytes_processed = bytes_processed.saturating_add(read as u64);
        publish_progress(*bytes_processed);
    }
    cancellation.checkpoint()
}

enum HashState {
    Sha256(Sha256),
    Sha512(Sha512),
    Sha1(Sha1),
}

impl HashState {
    fn new(algorithm: HashAlgorithm) -> Self {
        match algorithm {
            HashAlgorithm::Sha256 => Self::Sha256(Sha256::new()),
            HashAlgorithm::Sha512 => Self::Sha512(Sha512::new()),
            HashAlgorithm::Sha1 => Self::Sha1(Sha1::new()),
        }
    }

    fn update(&mut self, bytes: &[u8]) {
        match self {
            Self::Sha256(hasher) => hasher.update(bytes),
            Self::Sha512(hasher) => hasher.update(bytes),
            Self::Sha1(hasher) => hasher.update(bytes),
        }
    }

    fn finish(self) -> Vec<u8> {
        match self {
            Self::Sha256(hasher) => hasher.finalize().to_vec(),
            Self::Sha512(hasher) => hasher.finalize().to_vec(),
            Self::Sha1(hasher) => hasher.finalize().to_vec(),
        }
    }
}

#[derive(Debug)]
pub struct HashJobEvent {
    pub request_id: HashRequestId,
    pub result: Result<HashResult, HashError>,
}

/// One detached worker. Progress is a single replaceable snapshot, while the
/// bounded one-item channel carries exactly one terminal result.
pub struct HashJob {
    request_id: HashRequestId,
    receiver: Option<mpsc::Receiver<HashJobEvent>>,
    cancellation: HashCancellationToken,
    progress: Arc<Mutex<HashProgress>>,
}

impl HashJob {
    pub fn start(
        request: HashRequest,
        repaint: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<Self, HashError> {
        Self::start_with_spawn(request, repaint, |worker| {
            std::thread::Builder::new()
                .name("hash-generator".into())
                .spawn(worker)
                .map(|_| ())
        })
    }

    fn start_with_spawn(
        request: HashRequest,
        repaint: Arc<dyn Fn() + Send + Sync>,
        spawn: impl FnOnce(Box<dyn FnOnce() + Send>) -> io::Result<()>,
    ) -> Result<Self, HashError> {
        let request_id = request.request_id;
        let cancellation = HashCancellationToken::new();
        let worker_cancellation = cancellation.clone();
        let progress = Arc::new(Mutex::new(HashProgress {
            request_id,
            bytes_processed: 0,
            total_bytes: None,
        }));
        let worker_progress = progress.clone();
        let (sender, receiver) = mpsc::sync_channel(1);
        let worker = Box::new(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                hash_request(request, &worker_cancellation, |snapshot| {
                    *worker_progress
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner) = snapshot;
                })
            }))
            .unwrap_or(Err(HashError::WorkerPanicked));
            let result = if worker_cancellation.is_cancelled() {
                Err(HashError::Cancelled)
            } else {
                result
            };
            if sender.send(HashJobEvent { request_id, result }).is_ok() {
                repaint();
            }
        });
        spawn(worker).map_err(HashError::Spawn)?;
        Ok(Self {
            request_id,
            receiver: Some(receiver),
            cancellation,
            progress,
        })
    }

    pub const fn request_id(&self) -> HashRequestId {
        self.request_id
    }

    pub fn progress(&self) -> HashProgress {
        *self
            .progress
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn cancel(&self) {
        self.cancellation.cancel();
    }

    pub fn poll(&mut self) -> Option<HashJobEvent> {
        let receiver = self.receiver.as_ref()?;
        let event = match receiver.try_recv() {
            Ok(event) => event,
            Err(mpsc::TryRecvError::Empty) => return None,
            Err(mpsc::TryRecvError::Disconnected) => HashJobEvent {
                request_id: self.request_id,
                result: Err(HashError::WorkerDisconnected),
            },
        };
        self.receiver = None;
        Some(event)
    }
}

impl Drop for HashJob {
    fn drop(&mut self) {
        self.cancel();
    }
}

fn parse_expected(
    algorithm: HashAlgorithm,
    expected: &str,
) -> Result<Option<ExpectedDigest>, ExpectedHashError> {
    parse_expected_digest(algorithm, expected).map(|bytes| bytes.map(ExpectedDigest))
}

struct ExpectedDigest(Vec<u8>);

impl fmt::Debug for ExpectedDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ExpectedDigest([redacted])")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn request(algorithm: HashAlgorithm, input: HashInput, expected: Option<&str>) -> HashRequest {
        HashRequest::new(algorithm, input, expected).unwrap()
    }

    fn known_answers() -> [(HashAlgorithm, &'static str, &'static str); 6] {
        [
            (
                HashAlgorithm::Sha256,
                "",
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            ),
            (
                HashAlgorithm::Sha256,
                "abc",
                "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            ),
            (
                HashAlgorithm::Sha512,
                "",
                "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e",
            ),
            (
                HashAlgorithm::Sha512,
                "abc",
                "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f",
            ),
            (
                HashAlgorithm::Sha1,
                "",
                "da39a3ee5e6b4b0d3255bfef95601890afd80709",
            ),
            (
                HashAlgorithm::Sha1,
                "abc",
                "a9993e364706816aba3e25717850c26c9cd0d89d",
            ),
        ]
    }

    fn calculate(algorithm: HashAlgorithm, text: &str) -> HashResult {
        hash_request(
            request(algorithm, HashInput::Text(text.to_owned()), None),
            &HashCancellationToken::new(),
            |_| {},
        )
        .unwrap()
    }

    #[test]
    fn hashes_published_empty_and_abc_vectors_for_all_algorithms() {
        for (algorithm, input, expected) in known_answers() {
            assert_eq!(calculate(algorithm, input).digest, expected);
        }
    }

    #[test]
    fn hashes_exact_utf8_bytes_without_trimming_or_newline_normalization() {
        let text = "é\n";
        let mut digest = Sha256::new();
        digest.update(text.as_bytes());
        let expected = hex::encode(digest.finalize());
        let result = calculate(HashAlgorithm::Sha256, text);
        assert_eq!(result.digest, expected);
        assert_eq!(result.bytes_processed, 3);
        assert_ne!(result.digest, calculate(HashAlgorithm::Sha256, "é").digest);
    }

    #[test]
    fn expected_hash_accepts_only_ascii_edge_whitespace_and_hex_case() {
        let sha256_abc = known_answers()[1].2;
        assert_eq!(
            parse_expected_digest(
                HashAlgorithm::Sha256,
                &format!(" \t{}\r\n", sha256_abc.to_uppercase())
            )
            .unwrap(),
            Some(hex::decode(sha256_abc).unwrap())
        );
        assert_eq!(
            parse_expected_digest(HashAlgorithm::Sha256, " \t\r\n").unwrap(),
            None
        );
        assert!(matches!(
            parse_expected_digest(HashAlgorithm::Sha256, "abc"),
            Err(ExpectedHashError::WrongLength { .. })
        ));
        assert!(matches!(
            parse_expected_digest(HashAlgorithm::Sha256, &format!("{}x", "0".repeat(63))),
            Err(ExpectedHashError::NonHexadecimal)
        ));
        assert!(matches!(
            parse_expected_digest(
                HashAlgorithm::Sha256,
                &format!("{} {}", "0".repeat(31), "0".repeat(32))
            ),
            Err(ExpectedHashError::NonHexadecimal)
        ));
        assert!(matches!(
            parse_expected_digest(
                HashAlgorithm::Sha256,
                &format!("\u{2003}{}", "0".repeat(64))
            ),
            Err(ExpectedHashError::WrongLength { .. })
        ));
    }

    #[test]
    fn comparison_is_optional_and_reports_match_or_mismatch() {
        let digest = known_answers()[1].2;
        let matching = hash_request(
            request(
                HashAlgorithm::Sha256,
                HashInput::Text("abc".into()),
                Some(&digest.to_uppercase()),
            ),
            &HashCancellationToken::new(),
            |_| {},
        )
        .unwrap();
        assert_eq!(matching.comparison, HashComparison::Match);
        let mismatch = hash_request(
            request(
                HashAlgorithm::Sha256,
                HashInput::Text("abc".into()),
                Some(&"0".repeat(64)),
            ),
            &HashCancellationToken::new(),
            |_| {},
        )
        .unwrap();
        assert_eq!(mismatch.comparison, HashComparison::Mismatch);
        let absent = hash_request(
            request(
                HashAlgorithm::Sha256,
                HashInput::Text("abc".into()),
                Some(" \t"),
            ),
            &HashCancellationToken::new(),
            |_| {},
        )
        .unwrap();
        assert_eq!(absent.comparison, HashComparison::NotRequested);
    }

    struct BoundedReader {
        remaining: usize,
        max_requested: usize,
        calls: usize,
    }

    impl Read for BoundedReader {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            self.max_requested = self.max_requested.max(buffer.len());
            self.calls += 1;
            let read = self.remaining.min(buffer.len());
            buffer[..read].fill(b'x');
            self.remaining -= read;
            Ok(read)
        }
    }

    #[test]
    fn streams_through_a_fixed_bounded_buffer_and_publishes_progress_snapshots() {
        let reader = BoundedReader {
            remaining: HASH_BUFFER_SIZE * 2 + 17,
            max_requested: 0,
            calls: 0,
        };
        let mut reader = reader;
        let request_id = HashRequestId(77);
        let mut bytes_processed = 0;
        let mut hasher = HashState::new(HashAlgorithm::Sha256);
        let mut progress = Vec::new();
        hash_reader(
            &mut reader,
            &HashCancellationToken::new(),
            &mut bytes_processed,
            |processed| {
                progress.push(HashProgress {
                    request_id,
                    bytes_processed: processed,
                    total_bytes: Some((HASH_BUFFER_SIZE * 2 + 17) as u64),
                })
            },
            &mut hasher,
        )
        .unwrap();
        assert_eq!(bytes_processed, (HASH_BUFFER_SIZE * 2 + 17) as u64);
        assert_eq!(reader.max_requested, HASH_BUFFER_SIZE);
        assert_eq!(reader.calls, 4);
        assert_eq!(progress.len(), 3);
        assert!(
            progress
                .windows(2)
                .all(|pair| pair[0].bytes_processed < pair[1].bytes_processed)
        );
    }

    struct CancelAfterFirstChunk {
        cancellation: HashCancellationToken,
        remaining: usize,
    }

    impl Read for CancelAfterFirstChunk {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            let read = self.remaining.min(buffer.len());
            buffer[..read].fill(b'c');
            self.remaining -= read;
            self.cancellation.cancel();
            Ok(read)
        }
    }

    #[test]
    fn cancellation_is_checked_before_work_and_between_reader_chunks() {
        let cancelled = HashCancellationToken::new();
        cancelled.cancel();
        let mut progress_calls = 0;
        assert!(matches!(
            hash_request(
                request(HashAlgorithm::Sha256, HashInput::Text("x".into()), None),
                &cancelled,
                |_| progress_calls += 1,
            ),
            Err(HashError::Cancelled)
        ));
        assert_eq!(progress_calls, 0);

        let cancellation = HashCancellationToken::new();
        let mut reader = CancelAfterFirstChunk {
            cancellation: cancellation.clone(),
            remaining: HASH_BUFFER_SIZE * 2,
        };
        let mut processed = 0;
        let mut hasher = HashState::new(HashAlgorithm::Sha256);
        let result = hash_reader(
            &mut reader,
            &cancellation,
            &mut processed,
            |_| {},
            &mut hasher,
        );
        assert!(matches!(result, Err(HashError::Cancelled)));
        assert_eq!(processed, HASH_BUFFER_SIZE as u64);
    }

    struct InterruptedOnce {
        interrupted: bool,
        bytes: &'static [u8],
        sent: bool,
    }

    impl Read for InterruptedOnce {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            if !self.interrupted {
                self.interrupted = true;
                return Err(io::Error::from(io::ErrorKind::Interrupted));
            }
            if self.sent {
                return Ok(0);
            }
            self.sent = true;
            buffer[..self.bytes.len()].copy_from_slice(self.bytes);
            Ok(self.bytes.len())
        }
    }

    struct FailingReader;

    impl Read for FailingReader {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("fixture read failure"))
        }
    }

    fn hash_custom_reader(reader: &mut impl Read) -> Result<u64, HashError> {
        let mut processed = 0;
        let mut hasher = HashState::new(HashAlgorithm::Sha256);
        hash_reader(
            reader,
            &HashCancellationToken::new(),
            &mut processed,
            |_| {},
            &mut hasher,
        )?;
        Ok(processed)
    }

    #[test]
    fn retries_interrupted_reads_and_preserves_typed_io_failures() {
        let mut reader = InterruptedOnce {
            interrupted: false,
            bytes: b"abc",
            sent: false,
        };
        assert_eq!(hash_custom_reader(&mut reader).unwrap(), 3);
        assert!(matches!(
            hash_custom_reader(&mut FailingReader),
            Err(HashError::Io(error)) if error.kind() == io::ErrorKind::Other
        ));
    }

    #[test]
    fn file_input_streams_disposable_fixture_bytes_and_returns_file_mode() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("known-bytes.bin");
        let contents = vec![0x5a; HASH_BUFFER_SIZE + 19];
        std::fs::write(&path, &contents).unwrap();
        let expected = hex::encode(Sha512::digest(&contents));
        let result = hash_request(
            request(HashAlgorithm::Sha512, HashInput::File(path), None),
            &HashCancellationToken::new(),
            |_| {},
        )
        .unwrap();
        assert_eq!(result.digest, expected);
        assert_eq!(result.input_mode, HashInputMode::File);
        assert_eq!(result.bytes_processed, contents.len() as u64);
    }

    fn terminal(job: &mut HashJob) -> HashJobEvent {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(event) = job.poll() {
                return event;
            }
            assert!(Instant::now() < deadline, "hash worker stalled");
            std::thread::yield_now();
        }
    }

    #[test]
    fn worker_returns_matching_identity_and_one_terminal_event() {
        let request = request(HashAlgorithm::Sha256, HashInput::Text("abc".into()), None);
        let request_id = request.request_id();
        let mut job = HashJob::start(request, Arc::new(|| {})).unwrap();
        assert_eq!(job.request_id(), request_id);
        let event = terminal(&mut job);
        assert_eq!(event.request_id, request_id);
        assert_eq!(event.result.unwrap().digest, known_answers()[1].2);
        assert!(job.poll().is_none());
        assert_eq!(job.progress().request_id, request_id);
    }

    #[test]
    fn worker_spawn_failure_and_disconnection_have_one_typed_terminal() {
        let rejected_request = request(HashAlgorithm::Sha256, HashInput::Text("x".into()), None);
        let error = HashJob::start_with_spawn(rejected_request, Arc::new(|| {}), |_| {
            Err(io::Error::other("spawn rejected"))
        })
        .err()
        .unwrap();
        assert!(matches!(error, HashError::Spawn(_)));

        let request = request(HashAlgorithm::Sha256, HashInput::Text("x".into()), None);
        let mut job = HashJob::start_with_spawn(request, Arc::new(|| {}), |worker| {
            drop(worker);
            Ok(())
        })
        .unwrap();
        let event = terminal(&mut job);
        assert!(matches!(event.result, Err(HashError::WorkerDisconnected)));
        assert!(job.poll().is_none());
    }

    #[test]
    fn debug_output_omits_input_paths_and_contents() {
        let text = request(
            HashAlgorithm::Sha256,
            HashInput::Text("private text marker".into()),
            Some(&"a".repeat(64)),
        );
        let debug = format!("{text:?}");
        assert!(!debug.contains("private text marker"));
        assert!(!debug.contains(&"a".repeat(64)));
        let path = request(
            HashAlgorithm::Sha256,
            HashInput::File(PathBuf::from("private-filename-marker")),
            None,
        );
        assert!(!format!("{path:?}").contains("private-filename-marker"));
    }

    #[test]
    fn sha1_is_marked_as_legacy_and_text_sources_remain_distinct() {
        assert!(HashAlgorithm::Sha1.security_notice().is_some());
        assert_eq!(HashAlgorithm::Sha256.security_notice(), None);
        assert_eq!(HashInput::Text("x".into()).mode(), HashInputMode::Text);
        assert_eq!(
            HashInput::Clipboard("x".into()).mode(),
            HashInputMode::Clipboard
        );
    }
}
