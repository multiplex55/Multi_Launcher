//! Bounded profile-local record of completed extraction coverage.
use super::native::ArchiveSourceStamp;
use crate::{
    common::atomic_file::save_atomic,
    file_transfer::{EntryKind, EntrySnapshot},
    platform::app_data::AppDataRoot,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::{ErrorKind, Read},
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

const NAME: &str = "archive_extraction_provenance.json";
const VERSION: u8 = 1;
const MAX_BYTES: u64 = 1024 * 1024;
const MAX_RECORDS: usize = 4096;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum ArchiveCoverage {
    Full,
    Selection,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    volume: u64,
    identity: [u8; 16],
    length: u64,
    created: i64,
    modified: i64,
    changed: i64,
    attributes: u32,
    coverage: ArchiveCoverage,
    run: u64,
    completed_unix: u64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    version: u8,
    records: Vec<Record>,
}
pub(crate) struct ArchiveProvenance {
    path: PathBuf,
    records: Vec<Record>,
}
impl ArchiveProvenance {
    pub(crate) fn read(profile: &AppDataRoot) -> Result<Self> {
        let path = profile.path().join(NAME);
        let document = match File::open(&path) {
            Err(e) if e.kind() == ErrorKind::NotFound => Document {
                version: VERSION,
                records: Vec::new(),
            },
            Err(e) => return Err(e).context("Read archive extraction history"),
            Ok(file) => {
                let mut bytes = Vec::new();
                file.take(MAX_BYTES + 1).read_to_end(&mut bytes)?;
                ensure!(
                    bytes.len() as u64 <= MAX_BYTES,
                    "Archive extraction history is too large"
                );
                serde_json::from_slice(&bytes).context("Decode archive extraction history")?
            }
        };
        ensure!(
            document.version == VERSION && document.records.len() <= MAX_RECORDS,
            "Unsupported archive extraction history"
        );
        ensure!(
            document
                .records
                .iter()
                .all(|record| record.identity != [0; 16]),
            "Invalid archive extraction identity"
        );
        Ok(Self {
            path,
            records: document.records,
        })
    }
    pub(crate) fn coverage_for(&self, entry: &EntrySnapshot) -> Option<ArchiveCoverage> {
        let meta = entry.metadata();
        if meta.kind != EntryKind::File {
            return None;
        }
        self.records
            .iter()
            .rev()
            .find(|record| {
                record.volume == entry.identity().volume_serial_number()
                    && record.identity == entry.identity().file_id()
                    && Some(record.length) == meta.byte_len
                    && record.created == meta.created
                    && record.modified == meta.modified
                    && record.changed == meta.changed
                    && record.attributes == meta.attributes
            })
            .map(|record| record.coverage)
    }
    pub(super) fn record(
        profile: &AppDataRoot,
        stamp: ArchiveSourceStamp,
        coverage: ArchiveCoverage,
        run: u64,
    ) -> Result<()> {
        let mut store = Self::read(profile)?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("Archive completion time")?
            .as_secs();
        let previous_full = store.records.iter().any(|r| {
            r.volume == stamp.identity.volume_serial_number()
                && r.identity == stamp.identity.file_id()
                && r.length == stamp.length
                && r.created == stamp.created
                && r.modified == stamp.modified
                && r.changed == stamp.changed
                && r.attributes == stamp.attributes
                && r.coverage == ArchiveCoverage::Full
        });
        store.records.retain(|r| {
            !(r.volume == stamp.identity.volume_serial_number()
                && r.identity == stamp.identity.file_id()
                && r.length == stamp.length
                && r.created == stamp.created
                && r.modified == stamp.modified
                && r.changed == stamp.changed
                && r.attributes == stamp.attributes)
        });
        store.records.push(Record {
            volume: stamp.identity.volume_serial_number(),
            identity: stamp.identity.file_id(),
            length: stamp.length,
            created: stamp.created,
            modified: stamp.modified,
            changed: stamp.changed,
            attributes: stamp.attributes,
            coverage: if previous_full {
                ArchiveCoverage::Full
            } else {
                coverage
            },
            run,
            completed_unix: now,
        });
        let bytes = encode_bounded(store.records)?;
        save_atomic(&store.path, &bytes).context("Save archive extraction history")?;
        Ok(())
    }
}

fn encode_bounded(mut records: Vec<Record>) -> Result<Vec<u8>> {
    let excess = records.len().saturating_sub(MAX_RECORDS);
    records.drain(..excess);
    let mut document = Document {
        version: VERSION,
        records,
    };
    let mut bytes = serde_json::to_vec(&document)?;
    if bytes.len() as u64 > MAX_BYTES {
        let deficit = bytes.len() - MAX_BYTES as usize;
        let mut removed = 0;
        let mut boundary = 0;
        for record in &document.records {
            removed += serde_json::to_vec(record)?.len() + 1;
            boundary += 1;
            if removed >= deficit {
                break;
            }
        }
        document.records.drain(..boundary);
        bytes = serde_json::to_vec(&document)?;
    }
    ensure!(
        bytes.len() as u64 <= MAX_BYTES,
        "Archive extraction history is too large"
    );
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        archive::{ArchiveLimits, ArchiveRevision, native::NativeArchiveSource},
        file_operations::ScanCancellation,
        file_transfer::inspect_source,
    };
    #[test]
    fn archive_provenance_reopens_and_rejects_new_revision() {
        let temp = tempfile::tempdir().unwrap();
        let profile = AppDataRoot::from_path(temp.path());
        let path = temp.path().join("input.zip");
        std::fs::write(&path, b"first").unwrap();
        let entry = inspect_source(&path, &ScanCancellation::new()).unwrap();
        let source = NativeArchiveSource::open_exact(
            &entry,
            ArchiveRevision::new(1),
            ArchiveLimits::default(),
        )
        .unwrap();
        ArchiveProvenance::record(
            &profile,
            source.recheck_source_stamp().unwrap(),
            ArchiveCoverage::Selection,
            1,
        )
        .unwrap();
        assert_eq!(
            ArchiveProvenance::read(&profile)
                .unwrap()
                .coverage_for(&entry),
            Some(ArchiveCoverage::Selection)
        );
        drop(source);
        std::fs::write(&path, b"different length").unwrap();
        let changed = inspect_source(&path, &ScanCancellation::new()).unwrap();
        assert_eq!(
            ArchiveProvenance::read(&profile)
                .unwrap()
                .coverage_for(&changed),
            None
        );
    }
    #[test]
    fn archive_provenance_keeps_accepting_records_at_the_byte_limit() {
        use crate::file_operations::NativeFileIdentity;

        let profile_dir = tempfile::tempdir().unwrap();
        let profile = AppDataRoot::from_path(profile_dir.path());
        let records: Vec<_> = (0..MAX_RECORDS as u64)
            .map(|run| {
                let mut identity = [244; 16];
                identity[..8].copy_from_slice(&(u64::MAX - run).to_le_bytes());
                Record {
                    volume: u64::MAX - 1,
                    identity,
                    length: 321_654_987,
                    created: 134_080_692_000_123_456,
                    modified: 134_080_692_000_123_457,
                    changed: 134_080_692_000_123_458,
                    attributes: 32,
                    coverage: ArchiveCoverage::Selection,
                    run,
                    completed_unix: 1_795_025_000,
                }
            })
            .collect();
        let bytes = encode_bounded(records).unwrap();
        assert!(bytes.len() as u64 <= MAX_BYTES);
        let path = profile.path().join(NAME);
        std::fs::write(&path, bytes).unwrap();
        let before = ArchiveProvenance::read(&profile).unwrap();
        assert!(before.records.len() < MAX_RECORDS);
        let earliest = before.records.first().unwrap().run;

        ArchiveProvenance::record(
            &profile,
            ArchiveSourceStamp {
                identity: NativeFileIdentity::from_parts(u64::MAX - 1, [255; 16]),
                length: 321_654_987,
                created: 134_080_692_000_123_456,
                modified: 134_080_692_000_123_457,
                changed: 134_080_692_000_123_458,
                attributes: 32,
            },
            ArchiveCoverage::Selection,
            u64::MAX,
        )
        .unwrap();
        let after = ArchiveProvenance::read(&profile).unwrap();
        assert!(after.records.first().unwrap().run > earliest);
        assert_eq!(after.records.last().unwrap().run, u64::MAX);
        assert!(std::fs::metadata(path).unwrap().len() <= MAX_BYTES);
    }

    #[test]
    fn archive_provenance_preserves_malformed_history() {
        let temp = tempfile::tempdir().unwrap();
        let profile = AppDataRoot::from_path(temp.path());
        let path = temp.path().join(NAME);
        std::fs::write(&path, b"not json").unwrap();
        assert!(ArchiveProvenance::read(&profile).is_err());
        assert_eq!(std::fs::read(path).unwrap(), b"not json");
    }
}
