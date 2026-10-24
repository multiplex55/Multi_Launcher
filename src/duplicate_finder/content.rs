use super::discovery::{DiscoveryLimits, MAX_DIAGNOSTIC_MESSAGE_CHARS, MAX_DIAGNOSTICS};
use super::model::{
    ContentProgress, DiscoveryDiagnostic, DiscoveryProgress, DiscoverySnapshot, DiscoveryStage,
    DiscoveryStatus, DuplicateTotals, FileAlias, IdentityUnverifiedMatch, KnownFileIdentity,
    VerifiedDuplicateGroup, VerifiedKnownFile,
};
use crate::file_operations::{
    self, DuplicateDiscoveryContentFile, DuplicateDiscoveryContentRequest,
    DuplicateDiscoveryRootIdentity, NativeFileIdentity, RenameScanGeneration, ScanCancellation,
};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, VecDeque};
use std::io::{self, Read};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

const CONTENT_READ_BUFFER_SIZE: usize = 64 * 1024;
const PROGRESS_MIN_INTERVAL: Duration = Duration::from_millis(250);
const PROGRESS_MIN_BYTES: u64 = 8 * 1024 * 1024;

trait ContentReadBackend {
    type Reader: Read;

    fn open(
        &self,
        request: DuplicateDiscoveryContentRequest,
        cancellation: &ScanCancellation,
    ) -> io::Result<Self::Reader>;

    fn validate(&self, reader: &Self::Reader, cancellation: &ScanCancellation) -> io::Result<()>;
}

struct NativeContentReadBackend;

impl ContentReadBackend for NativeContentReadBackend {
    type Reader = DuplicateDiscoveryContentFile;

    fn open(
        &self,
        request: DuplicateDiscoveryContentRequest,
        cancellation: &ScanCancellation,
    ) -> io::Result<Self::Reader> {
        file_operations::open_duplicate_discovery_content_file(request, cancellation)
    }

    fn validate(&self, reader: &Self::Reader, cancellation: &ScanCancellation) -> io::Result<()> {
        reader.revalidate(cancellation)
    }
}

trait DigestState {
    fn update(&mut self, bytes: &[u8]);
    fn finish(self) -> [u8; 32];
}

trait DigestFactory {
    type State: DigestState;

    fn new_state(&self) -> Self::State;
}

struct Sha256Factory;

impl DigestFactory for Sha256Factory {
    type State = Sha256;

    fn new_state(&self) -> Self::State {
        Sha256::new()
    }
}

impl DigestState for Sha256 {
    fn update(&mut self, bytes: &[u8]) {
        Digest::update(self, bytes);
    }

    fn finish(self) -> [u8; 32] {
        self.finalize().into()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CandidateMember {
    KnownFile(usize),
    UnverifiedPath(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct HashedCandidate {
    size_group_index: usize,
    member: CandidateMember,
    digest: [u8; 32],
    /// For a known identity, the immutable discovery alias that most recently
    /// passed the read lease. Unverified paths have no alias list.
    known_alias_index: Option<usize>,
}

#[derive(Default)]
struct EqualityPartition {
    representative: Option<HashedCandidate>,
    members: Vec<HashedCandidate>,
}

enum PairFailure {
    Representative(io::Error),
    Candidate(io::Error),
    Cancelled,
}

struct ComparedCandidates {
    equal: bool,
    representative: HashedCandidate,
    candidate: HashedCandidate,
}

struct ProgressReporter<'a> {
    generation: u64,
    roots_total: usize,
    roots_preflighted: usize,
    roots_completed: usize,
    counts: super::model::ScanCounts,
    stage: DiscoveryStage,
    content: ContentProgress,
    last_publish: Instant,
    bytes_at_last_publish: u64,
    progress_fn: &'a mut dyn FnMut(DiscoveryProgress),
}

impl ProgressReporter<'_> {
    fn set_stage(&mut self, stage: DiscoveryStage, content: ContentProgress) {
        self.stage = stage;
        self.content = content;
        self.publish(true);
    }

    fn add_bytes(&mut self, bytes: u64) {
        self.content.bytes_processed = self.content.bytes_processed.saturating_add(bytes);
        self.publish(false);
    }

    fn candidate_completed(&mut self) {
        self.candidates_completed(1);
    }

    fn candidates_completed(&mut self, count: usize) {
        self.content.files_completed = self.content.files_completed.saturating_add(count);
        self.publish(false);
    }

    fn publish(&mut self, force: bool) {
        let elapsed = self.last_publish.elapsed();
        let bytes_since = self
            .content
            .bytes_processed
            .saturating_sub(self.bytes_at_last_publish);
        if !force && elapsed < PROGRESS_MIN_INTERVAL && bytes_since < PROGRESS_MIN_BYTES {
            return;
        }
        (self.progress_fn)(DiscoveryProgress {
            generation: self.generation,
            stage: self.stage,
            roots_total: self.roots_total,
            roots_preflighted: self.roots_preflighted,
            roots_completed: self.roots_completed,
            current_root: None,
            counts: self.counts,
            content: Some(self.content),
        });
        self.last_publish = Instant::now();
        self.bytes_at_last_publish = self.content.bytes_processed;
    }
}

pub(super) fn verify_discovered_content(
    snapshot: &mut DiscoverySnapshot,
    limits: DiscoveryLimits,
    cancellation: &ScanCancellation,
    publish: &mut dyn FnMut(DiscoveryProgress),
) {
    verify_discovered_content_with(
        snapshot,
        limits,
        cancellation,
        publish,
        &NativeContentReadBackend,
        &Sha256Factory,
    );
}

fn verify_discovered_content_with<B: ContentReadBackend, H: DigestFactory>(
    snapshot: &mut DiscoverySnapshot,
    limits: DiscoveryLimits,
    cancellation: &ScanCancellation,
    publish: &mut dyn FnMut(DiscoveryProgress),
    backend: &B,
    digests: &H,
) {
    let mut content_files_total = 0usize;
    let mut content_bytes_total = 0u64;
    for size_group in &snapshot.size_groups {
        let candidates = size_group
            .known_files
            .len()
            .saturating_add(size_group.unverified_paths.len());
        if candidates < 2 {
            continue;
        }
        content_files_total = content_files_total.saturating_add(candidates);
        let files = size_group
            .known_files
            .len()
            .saturating_add(size_group.unverified_paths.len());
        let size = size_group.size_bytes;
        content_bytes_total = content_bytes_total
            .saturating_add(size.saturating_mul(u64::try_from(files).unwrap_or(u64::MAX)));
    }
    content_files_total = content_files_total.min(limits.max_files);

    let progress = snapshot.progress.as_ref();
    let mut reporter = ProgressReporter {
        generation: snapshot.generation,
        roots_total: progress.map_or(snapshot.roots.len(), |p| p.roots_total),
        roots_preflighted: progress.map_or(0, |p| p.roots_preflighted),
        roots_completed: progress.map_or(0, |p| p.roots_completed),
        counts: snapshot.counts,
        stage: DiscoveryStage::Hashing,
        content: ContentProgress {
            files_completed: 0,
            files_total: content_files_total,
            bytes_processed: 0,
            bytes_total: Some(content_bytes_total),
        },
        last_publish: Instant::now(),
        bytes_at_last_publish: 0,
        progress_fn: publish,
    };
    reporter.publish(true);

    let mut failures = 0usize;
    let mut hashed_by_size: Vec<(usize, BTreeMap<[u8; 32], Vec<HashedCandidate>>)> = Vec::new();
    for size_group_index in 0..snapshot.size_groups.len() {
        if cancellation.is_cancelled() {
            mark_cancelled(snapshot, &mut reporter);
            return;
        }
        let candidate_count = snapshot.size_groups[size_group_index]
            .known_files
            .len()
            .saturating_add(
                snapshot.size_groups[size_group_index]
                    .unverified_paths
                    .len(),
            );
        if candidate_count < 2 {
            continue;
        }
        let mut buckets = BTreeMap::<[u8; 32], Vec<HashedCandidate>>::new();
        let known_count = snapshot.size_groups[size_group_index].known_files.len();
        for known_file_index in 0..known_count {
            if cancellation.is_cancelled() {
                mark_cancelled(snapshot, &mut reporter);
                return;
            }
            match hash_known_file(
                snapshot,
                size_group_index,
                known_file_index,
                None,
                backend,
                digests,
                cancellation,
                &mut reporter,
            ) {
                Ok(Some(candidate)) => buckets.entry(candidate.digest).or_default().push(candidate),
                Ok(None) => failures = failures.saturating_add(1),
                Err(error) if cancellation.is_cancelled() => {
                    let _ = error;
                    mark_cancelled(snapshot, &mut reporter);
                    return;
                }
                Err(error) => {
                    failures = failures.saturating_add(1);
                    let (root_index, path) = snapshot.size_groups[size_group_index].known_files
                        [known_file_index]
                        .aliases
                        .first()
                        .map(|alias| (Some(alias.root_index), Some(alias.path.clone())))
                        .unwrap_or((None, None));
                    record_content_failure(snapshot, root_index, path, &error);
                }
            }
            reporter.candidate_completed();
        }

        let unverified_count = snapshot.size_groups[size_group_index]
            .unverified_paths
            .len();
        for unverified_path_index in 0..unverified_count {
            if cancellation.is_cancelled() {
                mark_cancelled(snapshot, &mut reporter);
                return;
            }
            let request = match request_for_unverified_path(
                snapshot,
                size_group_index,
                unverified_path_index,
            ) {
                Ok(request) => request,
                Err(error) => {
                    failures = failures.saturating_add(1);
                    let path = &snapshot.size_groups[size_group_index].unverified_paths
                        [unverified_path_index];
                    let root_index = path.root_index;
                    let path = path.path.clone();
                    record_content_failure(snapshot, Some(root_index), Some(path), &error);
                    reporter.candidate_completed();
                    continue;
                }
            };
            match hash_candidate(&request, backend, digests, cancellation, &mut reporter) {
                Ok(digest) => buckets.entry(digest).or_default().push(HashedCandidate {
                    size_group_index,
                    member: CandidateMember::UnverifiedPath(unverified_path_index),
                    digest,
                    known_alias_index: None,
                }),
                Err(error) if cancellation.is_cancelled() => {
                    let _ = error;
                    mark_cancelled(snapshot, &mut reporter);
                    return;
                }
                Err(error) => {
                    failures = failures.saturating_add(1);
                    let path = &snapshot.size_groups[size_group_index].unverified_paths
                        [unverified_path_index];
                    let root_index = path.root_index;
                    let path = path.path.clone();
                    record_content_failure(snapshot, Some(root_index), Some(path), &error);
                }
            }
            reporter.candidate_completed();
        }
        hashed_by_size.push((size_group_index, buckets));
    }

    let comparison_files_total = hashed_by_size
        .iter()
        .flat_map(|(_, buckets)| buckets.values())
        .fold(0usize, |total, candidates| {
            total.saturating_add(candidates.len())
        });
    reporter.set_stage(
        DiscoveryStage::Comparing,
        ContentProgress {
            files_completed: 0,
            files_total: comparison_files_total,
            bytes_processed: 0,
            bytes_total: None,
        },
    );

    let mut verified_groups = Vec::new();
    let mut unverified_matches = Vec::new();
    let mut totals = DuplicateTotals::default();
    for (size_group_index, buckets) in hashed_by_size {
        for candidates in buckets.into_values() {
            if cancellation.is_cancelled() {
                mark_cancelled(snapshot, &mut reporter);
                return;
            }
            let candidates_in_bucket = candidates.len();
            let partitions = match partition_digest_bucket(
                candidates,
                snapshot,
                backend,
                digests,
                cancellation,
                &mut reporter,
                &mut failures,
            ) {
                Ok(partitions) => partitions,
                Err(()) => {
                    mark_cancelled(snapshot, &mut reporter);
                    return;
                }
            };
            // Digest-bucket partitions may retry members when a representative
            // changes. Count the bucket only once it has fully resolved so the
            // progress figure cannot exceed its total after a retry.
            reporter.candidates_completed(candidates_in_bucket);
            for partition in partitions {
                let mut known_files = Vec::new();
                let mut unverified_indices = Vec::new();
                for member in partition.members {
                    match member.member {
                        CandidateMember::KnownFile(index) => {
                            if let Some(alias_index) = member.known_alias_index {
                                known_files.push(VerifiedKnownFile {
                                    known_file_index: index,
                                    alias_index,
                                });
                            }
                        }
                        CandidateMember::UnverifiedPath(index) => unverified_indices.push(index),
                    }
                }
                known_files.sort_unstable_by_key(|member| member.known_file_index);
                unverified_indices.sort_unstable();
                known_files.dedup_by_key(|member| member.known_file_index);
                unverified_indices.dedup();
                let Some(digest) = partition
                    .representative
                    .map(|representative| representative.digest)
                else {
                    continue;
                };
                let size_bytes = snapshot.size_groups[size_group_index].size_bytes;
                if known_files.len() >= 2 {
                    let duplicate_members = known_files.len().saturating_sub(1);
                    let logical_duplicate_bytes = size_bytes
                        .saturating_mul(u64::try_from(duplicate_members).unwrap_or(u64::MAX));
                    totals.verified_groups = totals.verified_groups.saturating_add(1);
                    totals.verified_files = totals.verified_files.saturating_add(known_files.len());
                    totals.logical_duplicate_bytes = totals
                        .logical_duplicate_bytes
                        .saturating_add(logical_duplicate_bytes);
                    verified_groups.push(VerifiedDuplicateGroup {
                        size_group_index,
                        sha256: digest,
                        known_files: known_files.clone(),
                        logical_duplicate_bytes,
                    });
                }
                if !unverified_indices.is_empty()
                    && known_files.len().saturating_add(unverified_indices.len()) >= 2
                {
                    totals.identity_unverified_matches =
                        totals.identity_unverified_matches.saturating_add(1);
                    unverified_matches.push(IdentityUnverifiedMatch {
                        size_group_index,
                        sha256: digest,
                        known_files,
                        unverified_path_indices: unverified_indices,
                    });
                }
            }
        }
    }

    snapshot.verified_duplicate_groups = verified_groups;
    snapshot.identity_unverified_matches = unverified_matches;
    snapshot.duplicate_totals = totals;
    if (failures > 0
        || snapshot
            .roots
            .iter()
            .any(|root| root.status == super::model::RootScanStatus::Partial))
        && snapshot.status == DiscoveryStatus::Complete
    {
        snapshot.status = DiscoveryStatus::Partial;
    }
    if cancellation.is_cancelled() {
        mark_cancelled(snapshot, &mut reporter);
        return;
    }
    reporter.set_stage(
        DiscoveryStage::Complete,
        ContentProgress {
            files_completed: comparison_files_total,
            files_total: comparison_files_total,
            bytes_processed: reporter.content.bytes_processed,
            bytes_total: reporter.content.bytes_total,
        },
    );
    snapshot.progress = Some(DiscoveryProgress {
        generation: snapshot.generation,
        stage: DiscoveryStage::Complete,
        roots_total: snapshot.roots.len(),
        roots_preflighted: snapshot
            .roots
            .iter()
            .filter(|root| root.canonical_root.is_some())
            .count(),
        roots_completed: snapshot
            .roots
            .iter()
            .filter(|root| matches!(root.status, super::model::RootScanStatus::Complete))
            .count(),
        current_root: None,
        counts: snapshot.counts,
        content: Some(reporter.content),
    });
}

fn hash_known_file<B: ContentReadBackend, H: DigestFactory>(
    snapshot: &mut DiscoverySnapshot,
    size_group_index: usize,
    known_file_index: usize,
    preferred_alias_index: Option<usize>,
    backend: &B,
    digests: &H,
    cancellation: &ScanCancellation,
    reporter: &mut ProgressReporter<'_>,
) -> io::Result<Option<HashedCandidate>> {
    let file = snapshot.size_groups[size_group_index]
        .known_files
        .get(known_file_index)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "known file is missing"))?;
    let identity = file.identity;
    let size_bytes = file.size_bytes;
    let alias_count = file.aliases.len();
    let preferred_alias_index = preferred_alias_index.filter(|index| *index < alias_count);
    let modified = file.modified;
    let Some(modified) = modified else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "file modification time was unavailable during discovery",
        ));
    };
    for attempt in 0..alias_count {
        let alias_index = match (attempt, preferred_alias_index) {
            (0, Some(preferred)) => preferred,
            (attempt, Some(preferred)) if attempt <= preferred => attempt - 1,
            (attempt, Some(_)) => attempt,
            (attempt, None) => attempt,
        };
        let alias = snapshot.size_groups[size_group_index].known_files[known_file_index].aliases
            [alias_index]
            .clone();
        let request = request_for_known_alias(snapshot, identity, size_bytes, modified, &alias);
        let result = request
            .and_then(|request| hash_candidate(&request, backend, digests, cancellation, reporter));
        match result {
            Ok(digest) => {
                return Ok(Some(HashedCandidate {
                    size_group_index,
                    member: CandidateMember::KnownFile(known_file_index),
                    digest,
                    known_alias_index: Some(alias_index),
                }));
            }
            Err(error) if cancellation.is_cancelled() => {
                return Err(error);
            }
            Err(error) => {
                record_content_failure(
                    snapshot,
                    Some(alias.root_index),
                    Some(alias.path.clone()),
                    &error,
                );
            }
        }
    }
    Ok(None)
}

fn request_for_known_alias(
    snapshot: &DiscoverySnapshot,
    identity: KnownFileIdentity,
    size_bytes: u64,
    modified: SystemTime,
    alias: &FileAlias,
) -> io::Result<DuplicateDiscoveryContentRequest> {
    let root = snapshot.roots.get(alias.root_index).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "file alias references a missing discovery root",
        )
    })?;
    let (Some(canonical_root), Some(root_identity)) = (root.canonical_root.as_ref(), root.identity)
    else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "file alias root has no retained identity evidence",
        ));
    };
    Ok(DuplicateDiscoveryContentRequest {
        root: DuplicateDiscoveryRootIdentity {
            canonical_root: canonical_root.clone(),
            identity: NativeFileIdentity::from_parts(
                root_identity.volume_serial_number,
                root_identity.file_id,
            ),
        },
        generation: RenameScanGeneration(snapshot.generation),
        path: alias.path.clone(),
        identity: NativeFileIdentity::from_parts(identity.volume_serial_number, identity.file_id),
        length: size_bytes,
        modified,
    })
}

fn request_for_unverified_path(
    snapshot: &DiscoverySnapshot,
    size_group_index: usize,
    unverified_path_index: usize,
) -> io::Result<DuplicateDiscoveryContentRequest> {
    let size_group = &snapshot.size_groups[size_group_index];
    let path = size_group
        .unverified_paths
        .get(unverified_path_index)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "unverified path is missing"))?;
    let Some(modified) = path.modified else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "file modification time was unavailable during discovery",
        ));
    };
    let root = snapshot.roots.get(path.root_index).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "unverified path references a missing discovery root",
        )
    })?;
    let (Some(canonical_root), Some(root_identity)) = (root.canonical_root.as_ref(), root.identity)
    else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "unverified path root has no retained identity evidence",
        ));
    };
    Ok(DuplicateDiscoveryContentRequest {
        root: DuplicateDiscoveryRootIdentity {
            canonical_root: canonical_root.clone(),
            identity: NativeFileIdentity::from_parts(
                root_identity.volume_serial_number,
                root_identity.file_id,
            ),
        },
        generation: RenameScanGeneration(snapshot.generation),
        path: path.path.clone(),
        identity: path.observed_identity,
        length: path.size_bytes,
        modified,
    })
}

fn hash_candidate<B: ContentReadBackend, H: DigestFactory>(
    request: &DuplicateDiscoveryContentRequest,
    backend: &B,
    digests: &H,
    cancellation: &ScanCancellation,
    reporter: &mut ProgressReporter<'_>,
) -> io::Result<[u8; 32]> {
    let mut reader = backend.open(request.clone(), cancellation)?;
    backend.validate(&reader, cancellation)?;
    let mut state = digests.new_state();
    if request.length > 0 {
        let mut buffer = [0_u8; CONTENT_READ_BUFFER_SIZE];
        let mut bytes_read = 0u64;
        loop {
            if cancellation.is_cancelled() {
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "duplicate-content hashing was cancelled",
                ));
            }
            let read = read_retrying(&mut reader, &mut buffer, cancellation)?;
            if read == 0 {
                break;
            }
            bytes_read = bytes_read.checked_add(read as u64).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "candidate byte count overflowed",
                )
            })?;
            if bytes_read > request.length {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "candidate grew while hashing",
                ));
            }
            state.update(&buffer[..read]);
            reporter.add_bytes(read as u64);
        }
        if bytes_read != request.length {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "candidate shrank while hashing",
            ));
        }
    }
    if cancellation.is_cancelled() {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "duplicate-content hashing was cancelled",
        ));
    }
    backend.validate(&reader, cancellation)?;
    Ok(state.finish())
}

fn rehash_candidate<B: ContentReadBackend, H: DigestFactory>(
    snapshot: &mut DiscoverySnapshot,
    candidate: HashedCandidate,
    backend: &B,
    digests: &H,
    cancellation: &ScanCancellation,
    reporter: &mut ProgressReporter<'_>,
) -> io::Result<Option<HashedCandidate>> {
    match candidate.member {
        CandidateMember::KnownFile(index) => hash_known_file(
            snapshot,
            candidate.size_group_index,
            index,
            candidate.known_alias_index,
            backend,
            digests,
            cancellation,
            reporter,
        ),
        CandidateMember::UnverifiedPath(index) => {
            let request = request_for_unverified_path(snapshot, candidate.size_group_index, index)?;
            let digest = hash_candidate(&request, backend, digests, cancellation, reporter)?;
            Ok(Some(HashedCandidate {
                digest,
                known_alias_index: None,
                ..candidate
            }))
        }
    }
}

fn partition_digest_bucket<B: ContentReadBackend, H: DigestFactory>(
    candidates: Vec<HashedCandidate>,
    snapshot: &mut DiscoverySnapshot,
    backend: &B,
    digests: &H,
    cancellation: &ScanCancellation,
    reporter: &mut ProgressReporter<'_>,
    failures: &mut usize,
) -> Result<Vec<EqualityPartition>, ()> {
    let mut pending = VecDeque::from(candidates);
    let mut partitions = Vec::<EqualityPartition>::new();
    while let Some(candidate) = pending.pop_front() {
        if cancellation.is_cancelled() {
            return Err(());
        }
        let mut candidate = candidate;
        let mut placed = false;
        let mut partition_index = 0usize;
        while partition_index < partitions.len() {
            let representative = partitions[partition_index]
                .representative
                .expect("nonempty equality partition has a representative");
            match compare_candidates(
                snapshot,
                representative,
                candidate,
                backend,
                digests,
                cancellation,
                reporter,
            ) {
                Ok(compared) => {
                    let partition = &mut partitions[partition_index];
                    partition.representative = Some(compared.representative);
                    replace_partition_member(partition, compared.representative);
                    candidate = compared.candidate;
                    if compared.equal {
                        partition.members.push(candidate);
                        placed = true;
                        break;
                    }
                    partition_index += 1;
                }
                Err(PairFailure::Cancelled) => return Err(()),
                Err(PairFailure::Candidate(error)) => {
                    *failures = failures.saturating_add(1);
                    record_candidate_failure(snapshot, candidate, &error);
                    match rehash_candidate(
                        snapshot,
                        representative,
                        backend,
                        digests,
                        cancellation,
                        reporter,
                    ) {
                        Ok(Some(revalidated)) => {
                            invalidate_partition(&mut partitions, partition_index, &mut pending);
                            if revalidated.digest == representative.digest {
                                // A matching digest only keeps the member in
                                // this bucket; it does not prove old equality
                                // relationships, especially under collisions.
                                // Repartition every surviving member against
                                // the freshly read representative.
                                pending.push_front(revalidated);
                            } else {
                                *failures = failures.saturating_add(1);
                                record_candidate_failure(
                                    snapshot,
                                    revalidated,
                                    &io::Error::new(
                                        io::ErrorKind::InvalidData,
                                        "representative content changed after a peer read failed",
                                    ),
                                );
                            }
                        }
                        Ok(None) => {
                            *failures = failures.saturating_add(1);
                            record_candidate_failure(
                                snapshot,
                                representative,
                                &io::Error::new(
                                    io::ErrorKind::InvalidData,
                                    "representative could not be revalidated after a peer read failed",
                                ),
                            );
                            invalidate_partition(&mut partitions, partition_index, &mut pending);
                        }
                        Err(_revalidation_error) if cancellation.is_cancelled() => return Err(()),
                        Err(revalidation_error) => {
                            *failures = failures.saturating_add(1);
                            record_candidate_failure(snapshot, representative, &revalidation_error);
                            invalidate_partition(&mut partitions, partition_index, &mut pending);
                        }
                    }
                    placed = true;
                    break;
                }
                Err(PairFailure::Representative(error)) => {
                    *failures = failures.saturating_add(1);
                    record_candidate_failure(snapshot, representative, &error);
                    invalidate_partition(&mut partitions, partition_index, &mut pending);
                    pending.push_front(candidate);
                    placed = true;
                    break;
                }
            }
        }
        if !placed {
            partitions.push(EqualityPartition {
                representative: Some(candidate),
                members: vec![candidate],
            });
        }
    }
    Ok(partitions)
}

fn replace_partition_member(partition: &mut EqualityPartition, replacement: HashedCandidate) {
    if let Some(member) = partition
        .members
        .iter_mut()
        .find(|member| member.member == replacement.member)
    {
        *member = replacement;
    }
}

fn invalidate_partition(
    partitions: &mut Vec<EqualityPartition>,
    partition_index: usize,
    pending: &mut VecDeque<HashedCandidate>,
) {
    let invalidated = partitions.remove(partition_index);
    let representative_member = invalidated.representative.map(|candidate| candidate.member);
    for member in invalidated.members {
        if Some(member.member) != representative_member {
            pending.push_back(member);
        }
    }
}

fn compare_candidates<B: ContentReadBackend, H: DigestFactory>(
    snapshot: &mut DiscoverySnapshot,
    representative: HashedCandidate,
    candidate: HashedCandidate,
    backend: &B,
    digests: &H,
    cancellation: &ScanCancellation,
    reporter: &mut ProgressReporter<'_>,
) -> Result<ComparedCandidates, PairFailure> {
    let (mut representative_reader, representative_request, representative_alias_index) =
        open_member(snapshot, representative, backend, cancellation)
            .map_err(|error| pair_error(cancellation, error, true))?;
    let (mut candidate_reader, candidate_request, candidate_alias_index) =
        open_member(snapshot, candidate, backend, cancellation)
            .map_err(|error| pair_error(cancellation, error, false))?;
    if cancellation.is_cancelled() {
        return Err(PairFailure::Cancelled);
    }
    if candidate_request.length != representative_request.length {
        return Err(PairFailure::Candidate(io::Error::new(
            io::ErrorKind::InvalidData,
            "candidate size differs from the representative",
        )));
    }

    let mut representative_digest = digests.new_state();
    let mut candidate_digest = digests.new_state();
    let mut equal = true;
    if representative_request.length > 0 {
        let mut representative_buffer = [0_u8; CONTENT_READ_BUFFER_SIZE];
        let mut candidate_buffer = [0_u8; CONTENT_READ_BUFFER_SIZE];
        let mut compared = 0u64;
        while compared < representative_request.length {
            if cancellation.is_cancelled() {
                return Err(PairFailure::Cancelled);
            }
            let target = usize::try_from(
                (representative_request.length - compared).min(CONTENT_READ_BUFFER_SIZE as u64),
            )
            .expect("comparison chunk is bounded by the fixed buffer size");
            let representative_read = read_chunk(
                &mut representative_reader,
                &mut representative_buffer[..target],
                cancellation,
            )
            .map_err(|error| pair_error(cancellation, error, true))?;
            let candidate_read = read_chunk(
                &mut candidate_reader,
                &mut candidate_buffer[..target],
                cancellation,
            )
            .map_err(|error| pair_error(cancellation, error, false))?;
            if representative_read != target {
                return Err(PairFailure::Representative(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "representative shrank during byte comparison",
                )));
            }
            if candidate_read != target {
                return Err(PairFailure::Candidate(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "candidate shrank during byte comparison",
                )));
            }
            compared = compared.checked_add(target as u64).ok_or_else(|| {
                PairFailure::Representative(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "representative byte count overflowed",
                ))
            })?;
            let representative_chunk = &representative_buffer[..target];
            let candidate_chunk = &candidate_buffer[..target];
            equal &= representative_chunk == candidate_chunk;
            representative_digest.update(representative_chunk);
            candidate_digest.update(candidate_chunk);
            reporter.add_bytes((target as u64).saturating_mul(2));
        }
        let mut extra = [0_u8; 1];
        if read_retrying(&mut representative_reader, &mut extra, cancellation)
            .map_err(|error| pair_error(cancellation, error, true))?
            != 0
        {
            return Err(PairFailure::Representative(io::Error::new(
                io::ErrorKind::InvalidData,
                "representative grew during byte comparison",
            )));
        }
        if read_retrying(&mut candidate_reader, &mut extra, cancellation)
            .map_err(|error| pair_error(cancellation, error, false))?
            != 0
        {
            return Err(PairFailure::Candidate(io::Error::new(
                io::ErrorKind::InvalidData,
                "candidate grew during byte comparison",
            )));
        }
    }
    if cancellation.is_cancelled() {
        return Err(PairFailure::Cancelled);
    }
    backend
        .validate(&representative_reader, cancellation)
        .map_err(|error| pair_error(cancellation, error, true))?;
    backend
        .validate(&candidate_reader, cancellation)
        .map_err(|error| pair_error(cancellation, error, false))?;
    if representative_digest.finish() != representative.digest {
        return Err(PairFailure::Representative(io::Error::new(
            io::ErrorKind::InvalidData,
            "representative content changed after hashing",
        )));
    }
    if candidate_digest.finish() != candidate.digest {
        return Err(PairFailure::Candidate(io::Error::new(
            io::ErrorKind::InvalidData,
            "candidate content changed after hashing",
        )));
    }
    Ok(ComparedCandidates {
        equal,
        representative: HashedCandidate {
            known_alias_index: representative_alias_index,
            ..representative
        },
        candidate: HashedCandidate {
            known_alias_index: candidate_alias_index,
            ..candidate
        },
    })
}

fn pair_error(
    cancellation: &ScanCancellation,
    error: io::Error,
    representative: bool,
) -> PairFailure {
    if cancellation.is_cancelled() {
        PairFailure::Cancelled
    } else if representative {
        PairFailure::Representative(error)
    } else {
        PairFailure::Candidate(error)
    }
}

fn open_member<B: ContentReadBackend>(
    snapshot: &mut DiscoverySnapshot,
    candidate: HashedCandidate,
    backend: &B,
    cancellation: &ScanCancellation,
) -> io::Result<(B::Reader, DuplicateDiscoveryContentRequest, Option<usize>)> {
    let size_group_index = candidate.size_group_index;
    match candidate.member {
        CandidateMember::KnownFile(index) => {
            let (identity, size_bytes, modified, alias_count) = snapshot.size_groups
                [size_group_index]
                .known_files
                .get(index)
                .map(|known| {
                    (
                        known.identity,
                        known.size_bytes,
                        known.modified,
                        known.aliases.len(),
                    )
                })
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "known file is missing")
                })?;
            let modified = modified.ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "file modification time is unavailable",
                )
            })?;
            let mut last_error = None;
            let preferred_alias_index = candidate
                .known_alias_index
                .filter(|index| *index < alias_count);
            let alias_indices = preferred_alias_index
                .into_iter()
                .chain((0..alias_count).filter(move |index| Some(*index) != preferred_alias_index));
            for alias_index in alias_indices {
                let alias = snapshot.size_groups[size_group_index].known_files[index].aliases
                    [alias_index]
                    .clone();
                let request =
                    request_for_known_alias(snapshot, identity, size_bytes, modified, &alias)?;
                match open_and_validate(backend, request.clone(), cancellation) {
                    Ok(reader) => return Ok((reader, request, Some(alias_index))),
                    Err(error) if cancellation.is_cancelled() => return Err(error),
                    Err(error) => {
                        record_content_failure(
                            snapshot,
                            Some(alias.root_index),
                            Some(alias.path),
                            &error,
                        );
                        last_error = Some(error);
                    }
                }
            }
            Err(last_error.unwrap_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "known file has no currently valid alias",
                )
            }))
        }
        CandidateMember::UnverifiedPath(index) => {
            let request = request_for_unverified_path(snapshot, size_group_index, index)?;
            let reader = open_and_validate(backend, request.clone(), cancellation)?;
            Ok((reader, request, None))
        }
    }
}

fn open_and_validate<B: ContentReadBackend>(
    backend: &B,
    request: DuplicateDiscoveryContentRequest,
    cancellation: &ScanCancellation,
) -> io::Result<B::Reader> {
    let reader = backend.open(request, cancellation)?;
    backend.validate(&reader, cancellation)?;
    Ok(reader)
}

fn record_candidate_failure(
    snapshot: &mut DiscoverySnapshot,
    candidate: HashedCandidate,
    error: &io::Error,
) {
    match candidate.member {
        CandidateMember::KnownFile(index) => {
            let known = &snapshot.size_groups[candidate.size_group_index].known_files[index];
            let alias = candidate
                .known_alias_index
                .and_then(|alias_index| known.aliases.get(alias_index))
                .or_else(|| known.aliases.first());
            let (root_index, path) = alias
                .map(|alias| (Some(alias.root_index), Some(alias.path.clone())))
                .unwrap_or((None, None));
            record_content_failure(snapshot, root_index, path, error);
        }
        CandidateMember::UnverifiedPath(index) => {
            let path = &snapshot.size_groups[candidate.size_group_index].unverified_paths[index];
            let root_index = path.root_index;
            let path = path.path.clone();
            record_content_failure(snapshot, Some(root_index), Some(path), error);
        }
    }
}

fn record_content_failure(
    snapshot: &mut DiscoverySnapshot,
    root_index: Option<usize>,
    path: Option<PathBuf>,
    error: &io::Error,
) {
    if let Some(root_index) = root_index
        && let Some(root) = snapshot.roots.get_mut(root_index)
        && root.status == super::model::RootScanStatus::Complete
    {
        root.status = super::model::RootScanStatus::Partial;
    }
    if snapshot.diagnostics.len() >= MAX_DIAGNOSTICS {
        snapshot.diagnostics_truncated = true;
        return;
    }
    snapshot.diagnostics.push(DiscoveryDiagnostic {
        root_index,
        path,
        message: error
            .to_string()
            .chars()
            .take(MAX_DIAGNOSTIC_MESSAGE_CHARS)
            .collect(),
    });
}

fn mark_cancelled(snapshot: &mut DiscoverySnapshot, reporter: &mut ProgressReporter<'_>) {
    snapshot.status = DiscoveryStatus::Cancelled;
    snapshot.verified_duplicate_groups.clear();
    snapshot.identity_unverified_matches.clear();
    snapshot.duplicate_totals = DuplicateTotals::default();
    snapshot.progress = Some(DiscoveryProgress {
        generation: snapshot.generation,
        stage: reporter.stage,
        roots_total: reporter.roots_total,
        roots_preflighted: reporter.roots_preflighted,
        roots_completed: reporter.roots_completed,
        current_root: None,
        counts: reporter.counts,
        content: Some(reporter.content),
    });
    reporter.publish(true);
}

fn read_retrying(
    reader: &mut impl Read,
    buffer: &mut [u8],
    cancellation: &ScanCancellation,
) -> io::Result<usize> {
    loop {
        if cancellation.is_cancelled() {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "duplicate-content read was cancelled",
            ));
        }
        match reader.read(buffer) {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            result => return result,
        }
    }
}

/// Fill one bounded comparison chunk despite legal short reads. Returning a
/// short count means the stream reached EOF before the requested span.
fn read_chunk(
    reader: &mut impl Read,
    buffer: &mut [u8],
    cancellation: &ScanCancellation,
) -> io::Result<usize> {
    let mut filled = 0usize;
    while filled < buffer.len() {
        let read = read_retrying(reader, &mut buffer[filled..], cancellation)?;
        if read == 0 {
            break;
        }
        filled += read;
    }
    Ok(filled)
}

#[cfg(test)]
mod tests {
    use super::{
        CONTENT_READ_BUFFER_SIZE, ContentReadBackend, DigestFactory, DigestState, DiscoveryLimits,
        Sha256Factory, verify_discovered_content_with,
    };
    #[cfg(windows)]
    use crate::duplicate_finder::discovery::{DiscoveryJob, NativeDiscoveryJob};
    use crate::duplicate_finder::model::{
        ContentProgress, DiscoveryDiagnostic, DiscoveryProgress, DiscoverySnapshot, DiscoveryStage,
        DiscoveryStatus, DuplicateTotals, FileAlias, IdentityUnverifiedPath, KnownFile,
        KnownFileIdentity, RootScan, RootScanStatus, ScanCounts, SizeGroup, VerifiedKnownFile,
    };
    use crate::file_operations::{
        DuplicateDiscoveryContentRequest, NativeFileIdentity, ScanCancellation,
    };
    use sha2::{Digest, Sha256};
    use std::collections::HashMap;
    use std::io::{self, Cursor, Read};
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use std::time::SystemTime;

    #[derive(Clone)]
    struct MemoryFile {
        identity: NativeFileIdentity,
        modified: SystemTime,
        bytes: Vec<u8>,
    }

    #[derive(Default)]
    struct MemoryState {
        files: HashMap<PathBuf, MemoryFile>,
        read_calls: usize,
        max_chunk: usize,
        max_read_request: usize,
        mutate_after_read: Option<(usize, PathBuf, MemoryFile)>,
        mutate_after_any_read: Option<(usize, PathBuf, MemoryFile)>,
        cancel_after_read: Option<(usize, ScanCancellation)>,
        read_error: Option<(PathBuf, io::ErrorKind)>,
        read_error_after_calls: Option<(usize, PathBuf, io::ErrorKind)>,
    }

    #[derive(Clone, Default)]
    struct MemoryBackend {
        state: Arc<Mutex<MemoryState>>,
    }

    impl MemoryBackend {
        fn new(max_chunk: usize) -> Self {
            Self {
                state: Arc::new(Mutex::new(MemoryState {
                    max_chunk: max_chunk.max(1),
                    ..MemoryState::default()
                })),
            }
        }

        fn insert(&self, path: PathBuf, identity: NativeFileIdentity, bytes: &[u8]) {
            self.state.lock().expect("memory state lock").files.insert(
                path,
                MemoryFile {
                    identity,
                    modified: SystemTime::UNIX_EPOCH,
                    bytes: bytes.to_vec(),
                },
            );
        }

        fn mutate_after_read(&self, read_number: usize, path: PathBuf, replacement: MemoryFile) {
            self.state
                .lock()
                .expect("memory state lock")
                .mutate_after_read = Some((read_number, path, replacement));
        }

        fn mutate_path_after_any_read(
            &self,
            read_number: usize,
            path: PathBuf,
            replacement: MemoryFile,
        ) {
            self.state
                .lock()
                .expect("memory state lock")
                .mutate_after_any_read = Some((read_number, path, replacement));
        }

        fn cancel_after_read(&self, read_number: usize, cancellation: ScanCancellation) {
            self.state
                .lock()
                .expect("memory state lock")
                .cancel_after_read = Some((read_number, cancellation));
        }

        fn fail_next_read(&self, path: PathBuf, kind: io::ErrorKind) {
            self.state.lock().expect("memory state lock").read_error = Some((path, kind));
        }

        fn fail_read_after_calls(&self, read_calls: usize, path: PathBuf, kind: io::ErrorKind) {
            self.state
                .lock()
                .expect("memory state lock")
                .read_error_after_calls = Some((read_calls, path, kind));
        }

        fn read_calls(&self) -> usize {
            self.state.lock().expect("memory state lock").read_calls
        }

        fn max_read_request(&self) -> usize {
            self.state
                .lock()
                .expect("memory state lock")
                .max_read_request
        }
    }

    struct MemoryReader {
        cursor: Cursor<Vec<u8>>,
        path: PathBuf,
        request: DuplicateDiscoveryContentRequest,
        state: Arc<Mutex<MemoryState>>,
    }

    impl Read for MemoryReader {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            let error =
                {
                    let mut state = self.state.lock().expect("memory state lock");
                    if state
                        .read_error
                        .as_ref()
                        .is_some_and(|(path, _)| path == &self.path)
                    {
                        state.read_error.take().map(|(_, kind)| kind)
                    } else if state.read_error_after_calls.as_ref().is_some_and(
                        |(after, path, _)| state.read_calls >= *after && path == &self.path,
                    ) {
                        state.read_error_after_calls.take().map(|(_, _, kind)| kind)
                    } else {
                        None
                    }
                };
            if let Some(kind) = error {
                return Err(io::Error::new(kind, "injected content read failure"));
            }
            let max_chunk = {
                let mut state = self.state.lock().expect("memory state lock");
                state.max_read_request = state.max_read_request.max(buffer.len());
                state.max_chunk
            };
            let limit = buffer.len().min(max_chunk);
            let read = self.cursor.read(&mut buffer[..limit])?;
            if read > 0 {
                let (cancel, replacement) = {
                    let mut state = self.state.lock().expect("memory state lock");
                    state.read_calls = state.read_calls.saturating_add(1);
                    let cancel = if state
                        .cancel_after_read
                        .as_ref()
                        .is_some_and(|(at, _)| *at == state.read_calls)
                    {
                        state
                            .cancel_after_read
                            .take()
                            .map(|(_, cancellation)| cancellation)
                    } else {
                        None
                    };
                    let replacement = if state
                        .mutate_after_read
                        .as_ref()
                        .is_some_and(|(at, path, _)| *at == state.read_calls && path == &self.path)
                    {
                        state
                            .mutate_after_read
                            .take()
                            .map(|(_, path, file)| (path, file))
                    } else if state
                        .mutate_after_any_read
                        .as_ref()
                        .is_some_and(|(at, _, _)| *at == state.read_calls)
                    {
                        state
                            .mutate_after_any_read
                            .take()
                            .map(|(_, path, file)| (path, file))
                    } else {
                        None
                    };
                    if let Some((path, file)) = &replacement {
                        state.files.insert(path.clone(), file.clone());
                    }
                    (cancel, replacement)
                };
                let _ = replacement;
                if let Some(cancellation) = cancel {
                    cancellation.cancel();
                }
            }
            Ok(read)
        }
    }

    impl ContentReadBackend for MemoryBackend {
        type Reader = MemoryReader;

        fn open(
            &self,
            request: DuplicateDiscoveryContentRequest,
            cancellation: &ScanCancellation,
        ) -> io::Result<Self::Reader> {
            if cancellation.is_cancelled() {
                return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
            }
            let state = self.state.lock().expect("memory state lock");
            let file = state
                .files
                .get(&request.path)
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "memory file is missing"))?;
            if file.identity != request.identity
                || file.bytes.len() as u64 != request.length
                || file.modified != request.modified
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "memory file identity or metadata changed",
                ));
            }
            Ok(MemoryReader {
                cursor: Cursor::new(file.bytes.clone()),
                path: request.path.clone(),
                request,
                state: Arc::clone(&self.state),
            })
        }

        fn validate(
            &self,
            reader: &Self::Reader,
            cancellation: &ScanCancellation,
        ) -> io::Result<()> {
            if cancellation.is_cancelled() {
                return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
            }
            let state = self.state.lock().expect("memory state lock");
            let file = state.files.get(&reader.path).ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotFound, "memory path disappeared")
            })?;
            if file.identity != reader.request.identity
                || file.bytes.len() as u64 != reader.request.length
                || file.modified != reader.request.modified
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "memory path changed during the read",
                ));
            }
            Ok(())
        }
    }

    struct ConstantDigestFactory;
    struct ConstantDigest;

    impl DigestFactory for ConstantDigestFactory {
        type State = ConstantDigest;

        fn new_state(&self) -> Self::State {
            ConstantDigest
        }
    }

    impl DigestState for ConstantDigest {
        fn update(&mut self, _bytes: &[u8]) {}

        fn finish(self) -> [u8; 32] {
            [0; 32]
        }
    }

    fn file_identity(id: u8) -> NativeFileIdentity {
        NativeFileIdentity::from_parts(17, [id; 16])
    }

    fn known_file(id: u8, paths: &[&str], size_bytes: u64) -> KnownFile {
        KnownFile {
            identity: KnownFileIdentity {
                volume_serial_number: 17,
                file_id: [id; 16],
            },
            size_bytes,
            modified: Some(SystemTime::UNIX_EPOCH),
            aliases: paths
                .iter()
                .map(|path| FileAlias {
                    root_index: 0,
                    path: PathBuf::from("root").join(path),
                })
                .collect(),
        }
    }

    fn unknown_file(path: &str, size_bytes: u64) -> IdentityUnverifiedPath {
        IdentityUnverifiedPath {
            root_index: 0,
            size_bytes,
            observed_identity: NativeFileIdentity::from_parts(17, [0; 16]),
            modified: Some(SystemTime::UNIX_EPOCH),
            path: PathBuf::from("root").join(path),
        }
    }

    fn snapshot(groups: Vec<SizeGroup>) -> DiscoverySnapshot {
        let root_path = PathBuf::from("root");
        let counts = ScanCounts::default();
        DiscoverySnapshot {
            generation: 9,
            status: DiscoveryStatus::Complete,
            progress: Some(DiscoveryProgress {
                generation: 9,
                stage: DiscoveryStage::Traversing,
                roots_total: 1,
                roots_preflighted: 1,
                roots_completed: 1,
                current_root: None,
                counts,
                content: None,
            }),
            roots: vec![RootScan {
                selected_path: root_path.clone(),
                canonical_root: Some(root_path),
                identity: Some(KnownFileIdentity {
                    volume_serial_number: 17,
                    file_id: [250; 16],
                }),
                status: RootScanStatus::Complete,
                counts,
            }],
            size_groups: groups,
            verified_duplicate_groups: Vec::new(),
            identity_unverified_matches: Vec::new(),
            duplicate_totals: DuplicateTotals::default(),
            diagnostics: Vec::<DiscoveryDiagnostic>::new(),
            diagnostics_truncated: false,
            counts,
        }
    }

    fn run_with_digest<H: DigestFactory>(
        snapshot: &mut DiscoverySnapshot,
        backend: &MemoryBackend,
        cancellation: &ScanCancellation,
        digests: &H,
        progress: &mut Vec<DiscoveryProgress>,
    ) {
        verify_discovered_content_with(
            snapshot,
            DiscoveryLimits::default(),
            cancellation,
            &mut |update| progress.push(update),
            backend,
            digests,
        );
    }

    fn install_known(backend: &MemoryBackend, path: &str, id: u8, bytes: &[u8]) {
        backend.insert(PathBuf::from("root").join(path), file_identity(id), bytes);
    }

    fn install_unknown(backend: &MemoryBackend, path: &str, bytes: &[u8]) {
        backend.insert(
            PathBuf::from("root").join(path),
            NativeFileIdentity::from_parts(17, [0; 16]),
            bytes,
        );
    }

    fn known_indices(members: &[VerifiedKnownFile]) -> Vec<usize> {
        members
            .iter()
            .map(|member| member.known_file_index)
            .collect()
    }

    #[test]
    fn dff_content_hashes_and_exactly_compares_known_and_unverified_files() {
        let backend = MemoryBackend::new(3);
        install_known(&backend, "a", 1, b"same-bytes");
        install_known(&backend, "a-hardlink", 1, b"same-bytes");
        install_known(&backend, "b", 2, b"same-bytes");
        install_known(&backend, "c", 3, b"diff-bytes");
        install_unknown(&backend, "u1", b"same-bytes");
        install_unknown(&backend, "u2", b"same-bytes");
        install_unknown(&backend, "u3", b"xyz");
        install_unknown(&backend, "u4", b"xyz");
        let mut snapshot = snapshot(vec![
            SizeGroup {
                size_bytes: 10,
                known_files: vec![
                    known_file(1, &["a", "a-hardlink"], 10),
                    known_file(2, &["b"], 10),
                    known_file(3, &["c"], 10),
                ],
                unverified_paths: vec![unknown_file("u1", 10), unknown_file("u2", 10)],
            },
            SizeGroup {
                size_bytes: 3,
                known_files: Vec::new(),
                unverified_paths: vec![unknown_file("u3", 3), unknown_file("u4", 3)],
            },
        ]);
        let mut progress = Vec::new();

        run_with_digest(
            &mut snapshot,
            &backend,
            &ScanCancellation::new(),
            &Sha256Factory,
            &mut progress,
        );

        assert_eq!(snapshot.status, DiscoveryStatus::Complete);
        assert_eq!(snapshot.verified_duplicate_groups.len(), 1);
        assert_eq!(snapshot.verified_duplicate_groups[0].size_group_index, 0);
        assert_eq!(
            known_indices(&snapshot.verified_duplicate_groups[0].known_files),
            [0, 1]
        );
        let expected_digest: [u8; 32] = Sha256::digest(b"same-bytes").into();
        assert_eq!(
            snapshot.verified_duplicate_groups[0].sha256,
            expected_digest
        );
        assert_eq!(
            snapshot.verified_duplicate_groups[0].logical_duplicate_bytes,
            10
        );
        assert_eq!(snapshot.identity_unverified_matches.len(), 2);
        assert_eq!(
            snapshot.identity_unverified_matches[0].unverified_path_indices,
            [0, 1]
        );
        assert!(
            snapshot.identity_unverified_matches[1]
                .known_files
                .is_empty()
        );
        assert_eq!(snapshot.duplicate_totals.verified_files, 2);
        assert_eq!(snapshot.duplicate_totals.logical_duplicate_bytes, 10);
        assert_eq!(snapshot.duplicate_totals.identity_unverified_matches, 2);
        let hash_progress = progress
            .iter()
            .find(|update| update.stage == DiscoveryStage::Hashing)
            .expect("hashing stage was published");
        assert_eq!(
            hash_progress.content,
            Some(ContentProgress {
                files_completed: 0,
                files_total: 7,
                bytes_processed: 0,
                bytes_total: Some(56),
            })
        );
        assert_eq!(
            snapshot.progress.as_ref().map(|update| update.stage),
            Some(DiscoveryStage::Complete)
        );
    }

    #[test]
    fn dff_content_exact_comparison_splits_forced_digest_collisions() {
        let backend = MemoryBackend::new(1);
        install_known(&backend, "a", 1, b"aa");
        install_known(&backend, "b", 2, b"bb");
        install_known(&backend, "c", 3, b"bb");
        let mut snapshot = snapshot(vec![SizeGroup {
            size_bytes: 2,
            known_files: vec![
                known_file(1, &["a"], 2),
                known_file(2, &["b"], 2),
                known_file(3, &["c"], 2),
            ],
            unverified_paths: Vec::new(),
        }]);
        let mut progress = Vec::new();

        run_with_digest(
            &mut snapshot,
            &backend,
            &ScanCancellation::new(),
            &ConstantDigestFactory,
            &mut progress,
        );

        assert_eq!(snapshot.verified_duplicate_groups.len(), 1);
        assert_eq!(
            known_indices(&snapshot.verified_duplicate_groups[0].known_files),
            [1, 2]
        );
        assert_eq!(
            snapshot.verified_duplicate_groups[0].logical_duplicate_bytes,
            2
        );
        assert_eq!(snapshot.duplicate_totals.verified_files, 2);
    }

    #[test]
    fn dff_content_skips_unique_sizes_and_hashes_empty_files_without_reads() {
        let backend = MemoryBackend::new(2);
        install_known(&backend, "unique", 1, b"one");
        install_known(&backend, "empty-a", 2, b"");
        install_known(&backend, "empty-b", 3, b"");
        let mut snapshot = snapshot(vec![
            SizeGroup {
                size_bytes: 3,
                known_files: vec![known_file(1, &["unique"], 3)],
                unverified_paths: Vec::new(),
            },
            SizeGroup {
                size_bytes: 0,
                known_files: vec![
                    known_file(2, &["empty-a"], 0),
                    known_file(3, &["empty-b"], 0),
                ],
                unverified_paths: Vec::new(),
            },
        ]);
        let mut progress = Vec::new();

        run_with_digest(
            &mut snapshot,
            &backend,
            &ScanCancellation::new(),
            &Sha256Factory,
            &mut progress,
        );

        assert_eq!(backend.read_calls(), 0);
        assert_eq!(snapshot.verified_duplicate_groups.len(), 1);
        assert_eq!(snapshot.verified_duplicate_groups[0].size_group_index, 1);
        assert_eq!(
            known_indices(&snapshot.verified_duplicate_groups[0].known_files),
            [0, 1]
        );
        let expected_digest: [u8; 32] = Sha256::digest([]).into();
        assert_eq!(
            snapshot.verified_duplicate_groups[0].sha256,
            expected_digest
        );
        let hashing_progress = progress
            .iter()
            .find(|update| update.stage == DiscoveryStage::Hashing)
            .and_then(|update| update.content)
            .expect("hashing progress is published before content work");
        assert_eq!(hashing_progress.bytes_total, Some(0));
        assert_eq!(hashing_progress.bytes_processed, 0);
        let final_progress = snapshot.progress.as_ref().unwrap().content.unwrap();
        assert_eq!(final_progress.bytes_processed, 0);
        assert_eq!(
            final_progress.bytes_total, None,
            "comparison work has no predetermined byte total"
        );
    }

    #[test]
    fn dff_content_reads_large_candidates_through_fixed_size_buffers() {
        let backend = MemoryBackend::new(usize::MAX);
        let bytes = vec![0x5a; CONTENT_READ_BUFFER_SIZE * 2 + 17];
        install_known(&backend, "large-a", 1, &bytes);
        install_known(&backend, "large-b", 2, &bytes);
        let mut snapshot = snapshot(vec![SizeGroup {
            size_bytes: bytes.len() as u64,
            known_files: vec![
                known_file(1, &["large-a"], bytes.len() as u64),
                known_file(2, &["large-b"], bytes.len() as u64),
            ],
            unverified_paths: Vec::new(),
        }]);
        let mut progress = Vec::new();

        run_with_digest(
            &mut snapshot,
            &backend,
            &ScanCancellation::new(),
            &Sha256Factory,
            &mut progress,
        );

        assert_eq!(snapshot.verified_duplicate_groups.len(), 1);
        assert_eq!(
            backend.max_read_request(),
            CONTENT_READ_BUFFER_SIZE,
            "hashing and exact comparison must stay on the fixed buffer size"
        );
        assert_eq!(
            snapshot.duplicate_totals.logical_duplicate_bytes,
            bytes.len() as u64
        );
    }

    #[test]
    fn dff_content_tries_a_surviving_hardlink_alias() {
        let backend = MemoryBackend::new(4);
        install_known(&backend, "surviving-link", 1, b"twin");
        install_known(&backend, "other", 2, b"twin");
        let mut snapshot = snapshot(vec![SizeGroup {
            size_bytes: 4,
            known_files: vec![
                known_file(1, &["stale-link", "surviving-link"], 4),
                known_file(2, &["other"], 4),
            ],
            unverified_paths: Vec::new(),
        }]);
        let mut progress = Vec::new();

        run_with_digest(
            &mut snapshot,
            &backend,
            &ScanCancellation::new(),
            &Sha256Factory,
            &mut progress,
        );

        assert_eq!(snapshot.status, DiscoveryStatus::Partial);
        assert_eq!(snapshot.verified_duplicate_groups.len(), 1);
        assert_eq!(
            known_indices(&snapshot.verified_duplicate_groups[0].known_files),
            [0, 1]
        );
        assert_eq!(
            snapshot.size_groups[0].known_files[0].aliases[0].path,
            PathBuf::from("root/stale-link"),
            "discovery aliases remain in their original immutable order"
        );
        assert_eq!(
            snapshot.verified_duplicate_groups[0].known_files[0].alias_index, 1,
            "the output identifies the alias that passed hashing and comparison"
        );
    }

    #[test]
    fn dff_content_records_the_comparison_fallback_alias_without_reordering_rows() {
        let backend = MemoryBackend::new(4);
        install_known(&backend, "first-alias", 1, b"twin");
        install_known(&backend, "fallback-alias", 1, b"twin");
        install_known(&backend, "other", 2, b"twin");
        backend.mutate_path_after_any_read(
            2,
            PathBuf::from("root/first-alias"),
            MemoryFile {
                identity: file_identity(90),
                modified: SystemTime::UNIX_EPOCH,
                bytes: b"gone".to_vec(),
            },
        );
        let mut snapshot = snapshot(vec![SizeGroup {
            size_bytes: 4,
            known_files: vec![
                known_file(1, &["first-alias", "fallback-alias"], 4),
                known_file(2, &["other"], 4),
            ],
            unverified_paths: Vec::new(),
        }]);
        let mut progress = Vec::new();

        run_with_digest(
            &mut snapshot,
            &backend,
            &ScanCancellation::new(),
            &Sha256Factory,
            &mut progress,
        );

        assert_eq!(snapshot.verified_duplicate_groups.len(), 1);
        let group = &snapshot.verified_duplicate_groups[0];
        assert_eq!(known_indices(&group.known_files), [0, 1]);
        assert_eq!(group.known_files[0].alias_index, 1);
        assert_eq!(group.known_files[1].alias_index, 0);
        assert_eq!(
            snapshot.size_groups[0].known_files[0]
                .aliases
                .iter()
                .map(|alias| alias.path.clone())
                .collect::<Vec<_>>(),
            vec![
                PathBuf::from("root/first-alias"),
                PathBuf::from("root/fallback-alias")
            ],
            "discovery keeps its original complete alias list"
        );
        let stale_alias_path = PathBuf::from("root/first-alias");
        assert!(
            snapshot.diagnostics.iter().any(|diagnostic| {
                diagnostic.path.as_deref() == Some(stale_alias_path.as_path())
            }),
            "the alias that became stale after hashing remains diagnosed"
        );
    }

    #[test]
    fn dff_content_read_failure_is_partial_and_preserves_discovery_rows() {
        let backend = MemoryBackend::new(4);
        install_known(&backend, "unreadable", 1, b"same");
        install_known(&backend, "other", 2, b"same");
        backend.fail_next_read(
            PathBuf::from("root/unreadable"),
            io::ErrorKind::PermissionDenied,
        );
        let mut snapshot = snapshot(vec![SizeGroup {
            size_bytes: 4,
            known_files: vec![
                known_file(1, &["unreadable"], 4),
                known_file(2, &["other"], 4),
            ],
            unverified_paths: Vec::new(),
        }]);
        let mut progress = Vec::new();

        run_with_digest(
            &mut snapshot,
            &backend,
            &ScanCancellation::new(),
            &Sha256Factory,
            &mut progress,
        );

        assert_eq!(snapshot.status, DiscoveryStatus::Partial);
        assert!(snapshot.verified_duplicate_groups.is_empty());
        assert_eq!(snapshot.size_groups[0].known_files.len(), 2);
        assert_eq!(
            snapshot.size_groups[0].known_files[0].aliases[0].path,
            PathBuf::from("root/unreadable")
        );
        assert_eq!(snapshot.roots[0].status, RootScanStatus::Partial);
        assert_eq!(
            snapshot.diagnostics[0].path,
            Some(PathBuf::from("root/unreadable"))
        );
    }

    #[test]
    fn dff_content_discards_a_changed_representative_and_keeps_valid_members() {
        let backend = MemoryBackend::new(8);
        install_known(&backend, "a", 1, b"aa");
        install_known(&backend, "b", 2, b"bb");
        install_known(&backend, "c", 3, b"bb");
        backend.mutate_after_read(
            4,
            PathBuf::from("root/a"),
            MemoryFile {
                identity: file_identity(99),
                modified: SystemTime::UNIX_EPOCH,
                bytes: b"zz".to_vec(),
            },
        );
        let mut snapshot = snapshot(vec![SizeGroup {
            size_bytes: 2,
            known_files: vec![
                known_file(1, &["a"], 2),
                known_file(2, &["b"], 2),
                known_file(3, &["c"], 2),
            ],
            unverified_paths: Vec::new(),
        }]);
        let mut progress = Vec::new();

        run_with_digest(
            &mut snapshot,
            &backend,
            &ScanCancellation::new(),
            &ConstantDigestFactory,
            &mut progress,
        );

        assert_eq!(snapshot.status, DiscoveryStatus::Partial);
        assert_eq!(snapshot.verified_duplicate_groups.len(), 1);
        assert_eq!(
            known_indices(&snapshot.verified_duplicate_groups[0].known_files),
            [1, 2]
        );
        assert_eq!(snapshot.diagnostics[0].path, Some(PathBuf::from("root/a")));
    }

    #[test]
    fn dff_content_revalidates_representative_when_peer_read_fails() {
        let backend = MemoryBackend::new(8);
        install_known(&backend, "a", 1, b"same");
        install_known(&backend, "b", 2, b"same");
        install_known(&backend, "c", 3, b"same");
        backend.mutate_after_read(
            6,
            PathBuf::from("root/a"),
            MemoryFile {
                identity: file_identity(99),
                modified: SystemTime::UNIX_EPOCH,
                bytes: b"else".to_vec(),
            },
        );
        // Three initial hashes and one successful A/B comparison consume five
        // reads. During the next comparison A is read sixth and changes; C's
        // peer read then fails before A can reach its normal post-read check.
        backend.fail_read_after_calls(6, PathBuf::from("root/c"), io::ErrorKind::PermissionDenied);
        let mut snapshot = snapshot(vec![SizeGroup {
            size_bytes: 4,
            known_files: vec![
                known_file(1, &["a"], 4),
                known_file(2, &["b"], 4),
                known_file(3, &["c"], 4),
            ],
            unverified_paths: Vec::new(),
        }]);
        let mut progress = Vec::new();

        run_with_digest(
            &mut snapshot,
            &backend,
            &ScanCancellation::new(),
            &Sha256Factory,
            &mut progress,
        );

        assert_eq!(snapshot.status, DiscoveryStatus::Partial);
        assert!(
            snapshot.verified_duplicate_groups.is_empty(),
            "the stale A/B partition cannot survive C's early read failure"
        );
        let diagnostic_paths = snapshot
            .diagnostics
            .iter()
            .filter_map(|diagnostic| diagnostic.path.as_deref())
            .collect::<Vec<_>>();
        let representative_path = PathBuf::from("root/a");
        let candidate_path = PathBuf::from("root/c");
        assert!(diagnostic_paths.contains(&representative_path.as_path()));
        assert!(diagnostic_paths.contains(&candidate_path.as_path()));
    }

    #[test]
    fn dff_content_repartitions_representative_after_peer_abort_under_digest_collision() {
        let backend = MemoryBackend::new(8);
        install_known(&backend, "a", 1, b"same");
        install_known(&backend, "b", 2, b"same");
        install_known(&backend, "c", 3, b"same");
        install_known(&backend, "d", 4, b"same");
        backend.mutate_path_after_any_read(
            7,
            PathBuf::from("root/a"),
            MemoryFile {
                identity: file_identity(1),
                modified: SystemTime::UNIX_EPOCH,
                bytes: b"else".to_vec(),
            },
        );
        // Four initial hashes and a successful A/B comparison consume six
        // reads. During A/C, A is read seventh and changes without changing
        // its identity, size, or timestamp. C then fails its peer read, so A
        // must be rehashed and the old A/B relationship repartitioned rather
        // than retained because the forced digest still collides.
        backend.fail_read_after_calls(7, PathBuf::from("root/c"), io::ErrorKind::PermissionDenied);
        let mut snapshot = snapshot(vec![SizeGroup {
            size_bytes: 4,
            known_files: vec![
                known_file(1, &["a"], 4),
                known_file(2, &["b"], 4),
                known_file(3, &["c"], 4),
                known_file(4, &["d"], 4),
            ],
            unverified_paths: Vec::new(),
        }]);
        let mut progress = Vec::new();

        run_with_digest(
            &mut snapshot,
            &backend,
            &ScanCancellation::new(),
            &ConstantDigestFactory,
            &mut progress,
        );

        assert_eq!(snapshot.status, DiscoveryStatus::Partial);
        assert_eq!(snapshot.verified_duplicate_groups.len(), 1);
        let members = known_indices(&snapshot.verified_duplicate_groups[0].known_files);
        assert_eq!(members, [1, 3], "only the still-equal B/D files group");
        assert!(
            !members.contains(&0),
            "changed representative A is not published"
        );
        let candidate_path = PathBuf::from("root/c");
        assert!(
            snapshot
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.path.as_ref() == Some(&candidate_path)),
            "the failed peer remains explicitly diagnosed"
        );
    }

    #[test]
    fn dff_content_cancellation_during_hashing_publishes_no_results() {
        let backend = MemoryBackend::new(2);
        install_known(&backend, "a", 1, b"abcd");
        install_known(&backend, "b", 2, b"abcd");
        let cancellation = ScanCancellation::new();
        backend.cancel_after_read(1, cancellation.clone());
        let mut snapshot = snapshot(vec![SizeGroup {
            size_bytes: 4,
            known_files: vec![known_file(1, &["a"], 4), known_file(2, &["b"], 4)],
            unverified_paths: Vec::new(),
        }]);
        let mut progress = Vec::new();

        run_with_digest(
            &mut snapshot,
            &backend,
            &cancellation,
            &Sha256Factory,
            &mut progress,
        );

        assert_eq!(snapshot.status, DiscoveryStatus::Cancelled);
        assert!(snapshot.verified_duplicate_groups.is_empty());
        assert!(snapshot.identity_unverified_matches.is_empty());
        assert_eq!(
            snapshot.progress.as_ref().unwrap().stage,
            DiscoveryStage::Hashing
        );
    }

    #[test]
    fn dff_content_cancellation_during_exact_comparison_publishes_no_results() {
        let backend = MemoryBackend::new(8);
        install_known(&backend, "a", 1, b"same");
        install_known(&backend, "b", 2, b"same");
        let cancellation = ScanCancellation::new();
        // Each nonempty hash performs one data read. The first exact
        // comparison read then cancels before the other side is read.
        backend.cancel_after_read(3, cancellation.clone());
        let mut snapshot = snapshot(vec![SizeGroup {
            size_bytes: 4,
            known_files: vec![known_file(1, &["a"], 4), known_file(2, &["b"], 4)],
            unverified_paths: Vec::new(),
        }]);
        let mut progress = Vec::new();

        run_with_digest(
            &mut snapshot,
            &backend,
            &cancellation,
            &Sha256Factory,
            &mut progress,
        );

        assert_eq!(snapshot.status, DiscoveryStatus::Cancelled);
        assert!(snapshot.verified_duplicate_groups.is_empty());
        assert_eq!(
            snapshot.progress.as_ref().unwrap().stage,
            DiscoveryStage::Comparing
        );
    }

    #[test]
    fn dff_content_changed_path_during_hash_is_partial_and_not_published() {
        let backend = MemoryBackend::new(8);
        install_known(&backend, "a", 1, b"same");
        install_known(&backend, "b", 2, b"same");
        backend.mutate_after_read(
            1,
            PathBuf::from("root/a"),
            MemoryFile {
                identity: file_identity(90),
                modified: SystemTime::UNIX_EPOCH,
                bytes: b"same".to_vec(),
            },
        );
        let mut snapshot = snapshot(vec![SizeGroup {
            size_bytes: 4,
            known_files: vec![known_file(1, &["a"], 4), known_file(2, &["b"], 4)],
            unverified_paths: Vec::new(),
        }]);
        let mut progress = Vec::new();

        run_with_digest(
            &mut snapshot,
            &backend,
            &ScanCancellation::new(),
            &Sha256Factory,
            &mut progress,
        );

        assert_eq!(snapshot.status, DiscoveryStatus::Partial);
        assert!(snapshot.verified_duplicate_groups.is_empty());
        assert_eq!(snapshot.roots[0].status, RootScanStatus::Partial);
        assert_eq!(snapshot.diagnostics[0].path, Some(PathBuf::from("root/a")));
    }

    #[cfg(windows)]
    #[test]
    fn dff_content_native_scan_reads_and_compares_only_generated_tree_files() {
        let temp = tempfile::tempdir().expect("generated content fixture directory");
        let same = temp.path().join("same.bin");
        let same_copy = temp.path().join("same-copy.bin");
        let hard_link = temp.path().join("same-hard-link.bin");
        let different = temp.path().join("different.bin");
        let empty_a = temp.path().join("empty-a.bin");
        let empty_b = temp.path().join("empty-b.bin");
        std::fs::write(&same, b"same bytes").expect("write generated source");
        std::fs::write(&same_copy, b"same bytes").expect("write generated copy");
        std::fs::hard_link(&same, &hard_link).expect("create generated hard link");
        std::fs::write(&different, b"diff bytes").expect("write same-sized different file");
        std::fs::write(&empty_a, b"").expect("write first empty file");
        std::fs::write(&empty_b, b"").expect("write second empty file");

        let cancellation = ScanCancellation::new();
        let snapshot = NativeDiscoveryJob.scan(
            vec![temp.path().to_path_buf()],
            73,
            DiscoveryLimits::default(),
            &cancellation,
            &mut |_| {},
        );

        assert_eq!(snapshot.status, DiscoveryStatus::Complete);
        assert_eq!(snapshot.duplicate_totals.verified_groups, 2);
        assert_eq!(snapshot.duplicate_totals.verified_files, 4);
        assert_eq!(
            snapshot.duplicate_totals.logical_duplicate_bytes,
            b"same bytes".len() as u64
        );

        let same_size_group = snapshot
            .size_groups
            .iter()
            .position(|group| group.size_bytes == b"same bytes".len() as u64)
            .expect("generated same-sized files are in one group");
        let verified_same = snapshot
            .verified_duplicate_groups
            .iter()
            .find(|group| group.size_group_index == same_size_group)
            .expect("native reads found the generated equal copies");
        assert_eq!(verified_same.known_files.len(), 2);
        let duplicate_aliases = verified_same
            .known_files
            .iter()
            .map(|member| {
                snapshot.size_groups[same_size_group].known_files[member.known_file_index].aliases
                    [member.alias_index]
                    .path
                    .as_path()
            })
            .collect::<Vec<_>>();
        assert!(
            duplicate_aliases.contains(&same.as_path())
                || duplicate_aliases.contains(&hard_link.as_path())
        );
        assert!(duplicate_aliases.contains(&same_copy.as_path()));
        assert!(!duplicate_aliases.contains(&different.as_path()));
        let discovered_aliases = snapshot.size_groups[same_size_group]
            .known_files
            .iter()
            .flat_map(|file| file.aliases.iter().map(|alias| alias.path.as_path()))
            .collect::<Vec<_>>();
        assert!(discovered_aliases.contains(&same.as_path()));
        assert!(discovered_aliases.contains(&hard_link.as_path()));
        assert!(discovered_aliases.contains(&same_copy.as_path()));
        assert!(discovered_aliases.contains(&different.as_path()));

        let empty_size_group = snapshot
            .size_groups
            .iter()
            .position(|group| group.size_bytes == 0)
            .expect("generated empty files are in one group");
        let verified_empty = snapshot
            .verified_duplicate_groups
            .iter()
            .find(|group| group.size_group_index == empty_size_group)
            .expect("native reads verified the generated empty copies");
        assert_eq!(verified_empty.known_files.len(), 2);
        assert_eq!(
            snapshot.progress.as_ref().map(|update| update.stage),
            Some(DiscoveryStage::Complete)
        );
    }
}
