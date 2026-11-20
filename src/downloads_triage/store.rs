//! Versioned app-profile preferences and exact reviewed-item marks.
use std::collections::HashSet;
use std::ffi::OsString;
use std::fs::File;
use std::io::{ErrorKind, Read};
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

use crate::common::atomic_file::save_atomic;
use crate::file_operations::ScanCancellation;
use crate::file_transfer::{EntrySnapshot, downloads_review_key, recheck_source_metadata};
use crate::platform::app_data::AppDataRoot;

const STORE_NAME: &str = "downloads_triage.json";
const VERSION: u8 = 1;
const MAX_BYTES: u64 = 4 * 1024 * 1024;
const MAX_MARKS: usize = 10_000;
const MAX_PATH_UNITS: usize = 32_766;
type ReviewKey = [u8; 32];

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct DownloadsPreferences {
    pub root: Option<PathBuf>,
    pub destination: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    version: u8,
    root: Option<Vec<u16>>,
    destination: Option<Vec<u16>>,
    reviewed: Vec<ReviewKey>,
}

impl Document {
    fn empty() -> Self {
        Self {
            version: VERSION,
            root: None,
            destination: None,
            reviewed: Vec::new(),
        }
    }

    fn validate(&self) -> Result<()> {
        ensure!(
            self.version == VERSION,
            "Unknown Downloads preferences version."
        );
        ensure!(
            self.reviewed.len() <= MAX_MARKS,
            "Too many stored Downloads review marks."
        );
        validate_path(self.root.as_deref())?;
        validate_path(self.destination.as_deref())?;
        Ok(())
    }
}

fn validate_path(units: Option<&[u16]>) -> Result<()> {
    if let Some(units) = units {
        ensure!(
            units.len() <= MAX_PATH_UNITS
                && !units.contains(&0)
                && Path::new(&OsString::from_wide(units)).is_absolute(),
            "Downloads preferences require an absolute Windows path."
        );
    }
    Ok(())
}

pub(crate) struct DownloadsStore {
    path: PathBuf,
    data: Document,
    reviewed: HashSet<ReviewKey>,
}

impl DownloadsStore {
    pub(crate) fn open(profile: &AppDataRoot) -> Result<Self> {
        let path = profile.path().join(STORE_NAME);
        let document = match File::open(&path) {
            Err(error) if error.kind() == ErrorKind::NotFound => Document::empty(),
            Err(error) => return Err(error).context("Read Downloads preferences."),
            Ok(file) => {
                let mut bytes = Vec::new();
                file.take(MAX_BYTES + 1)
                    .read_to_end(&mut bytes)
                    .context("Read Downloads preferences.")?;
                ensure!(
                    bytes.len() as u64 <= MAX_BYTES,
                    "Downloads preferences exceed the file-size limit."
                );
                serde_json::from_slice(&bytes).context("Read Downloads preferences JSON.")?
            }
        };
        document.validate()?;
        let reviewed: HashSet<_> = document.reviewed.iter().copied().collect();
        ensure!(
            reviewed.len() == document.reviewed.len(),
            "Downloads preferences contain duplicate review records."
        );
        Ok(Self {
            path,
            data: document,
            reviewed,
        })
    }

    pub(crate) fn preferences(&self) -> DownloadsPreferences {
        DownloadsPreferences {
            root: self
                .data
                .root
                .as_deref()
                .map(OsString::from_wide)
                .map(PathBuf::from),
            destination: self
                .data
                .destination
                .as_deref()
                .map(OsString::from_wide)
                .map(PathBuf::from),
        }
    }

    pub(crate) fn set_preferences(&mut self, preferences: &DownloadsPreferences) -> Result<()> {
        let mut candidate = self.data.clone();
        candidate.root = preferences
            .root
            .as_ref()
            .map(|path| path.as_os_str().encode_wide().collect());
        candidate.destination = preferences
            .destination
            .as_ref()
            .map(|path| path.as_os_str().encode_wide().collect());
        self.commit(candidate)
    }

    pub(crate) fn is_reviewed(&self, proof: &ReviewKey) -> bool {
        self.reviewed.contains(proof)
    }

    pub(crate) fn set_reviewed(
        &mut self,
        selected: &[&EntrySnapshot],
        reviewed: bool,
        cancellation: &ScanCancellation,
    ) -> Result<()> {
        ensure!(
            selected.len() <= MAX_MARKS,
            "Too many Downloads items selected."
        );
        let mut selected_keys = HashSet::with_capacity(selected.len());
        let mut keys_in_order = Vec::new();
        for entry in selected {
            recheck_source_metadata(entry, cancellation)
                .context("Review the current Downloads item.")?;
            let key = downloads_review_key(entry);
            if selected_keys.insert(key) {
                keys_in_order.push(key);
            }
        }
        ensure!(
            !cancellation.is_cancelled(),
            "Downloads review was cancelled."
        );
        let mut candidate = self.data.clone();
        if reviewed {
            candidate.reviewed.extend(
                keys_in_order
                    .into_iter()
                    .filter(|key| !self.reviewed.contains(key)),
            );
            let expired = candidate.reviewed.len().saturating_sub(MAX_MARKS);
            candidate.reviewed.drain(..expired);
        } else {
            candidate
                .reviewed
                .retain(|key| !selected_keys.contains(key));
        }
        self.commit(candidate)
    }

    fn commit(&mut self, candidate: Document) -> Result<()> {
        if self.data == candidate {
            return Ok(());
        }
        candidate.validate()?;
        let bytes = serde_json::to_vec(&candidate).context("Encode Downloads preferences.")?;
        ensure!(
            bytes.len() as u64 <= MAX_BYTES,
            "Downloads preferences exceed the file-size limit."
        );
        save_atomic(&self.path, &bytes).context("Save Downloads preferences.")?;
        self.reviewed = candidate.reviewed.iter().copied().collect();
        self.data = candidate;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_transfer::scan_downloads_directory;

    fn capture(folder: &Path, cancellation: &ScanCancellation) -> EntrySnapshot {
        scan_downloads_directory(folder, cancellation)
            .unwrap()
            .rows
            .into_iter()
            .find(|row| row.name == "fixture.zip")
            .unwrap()
            .entry
            .unwrap()
    }

    #[test]
    fn downloads_triage_review_is_durable_and_bound_to_the_exact_revision() {
        let profile = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        let app_data = AppDataRoot::from_path(profile.path());
        let cancellation = ScanCancellation::new();
        let path = root.path().join("fixture.zip");
        std::fs::write(&path, b"first occupant").unwrap();
        let first = capture(root.path(), &cancellation);

        let mut store = DownloadsStore::open(&app_data).unwrap();
        assert!(!profile.path().join(STORE_NAME).exists());
        let prefs = DownloadsPreferences {
            root: Some(root.path().to_path_buf()),
            destination: None,
        };
        store.set_preferences(&prefs).unwrap();
        store.set_reviewed(&[&first], true, &cancellation).unwrap();

        let mut reopened = DownloadsStore::open(&app_data).unwrap();
        assert_eq!(reopened.preferences(), prefs);
        assert!(reopened.is_reviewed(&downloads_review_key(&capture(root.path(), &cancellation))));
        assert_eq!(std::fs::read(&path).unwrap(), b"first occupant");

        std::fs::write(&path, b"an in-place edit of the same item").unwrap();
        let edited = capture(root.path(), &cancellation);
        assert_eq!(edited.identity(), first.identity());
        assert!(!reopened.is_reviewed(&downloads_review_key(&edited)));
        assert!(
            reopened
                .set_reviewed(&[&first], true, &cancellation)
                .is_err()
        );
        reopened
            .set_reviewed(&[&edited], true, &cancellation)
            .unwrap();

        std::fs::rename(&path, root.path().join("old.zip")).unwrap();
        std::fs::write(&path, b"an in-place edit of the same item").unwrap();
        let replacement = capture(root.path(), &cancellation);
        assert_ne!(replacement.identity(), edited.identity());
        assert!(
            !DownloadsStore::open(&app_data)
                .unwrap()
                .is_reviewed(&downloads_review_key(&replacement))
        );
    }

    #[test]
    fn downloads_triage_malformed_preferences_are_left_intact() {
        let profile = tempfile::tempdir().unwrap();
        let app_data = AppDataRoot::from_path(profile.path());
        let path = profile.path().join(STORE_NAME);
        let malformed = b"{invalid metadata";
        std::fs::write(&path, malformed).unwrap();
        assert!(DownloadsStore::open(&app_data).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), malformed);
    }
}
