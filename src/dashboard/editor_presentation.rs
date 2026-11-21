//! User presentation preferences for the Dashboard Editor.
//!
//! These collapse states are scoped to the active settings profile and the
//! dashboard file. They are deliberately stored beside application data,
//! never inside `DashboardConfig` or its edit history.

use crate::dashboard::config::SlotConfig;
use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::{error::Error, fmt};

const CURRENT_VERSION: u16 = 1;
const MAX_FILE_BYTES: usize = 256 * 1024;
const MAX_DASHBOARDS: usize = 32;
const MAX_SLOTS_PER_DASHBOARD: usize = 256;
const MAX_DASHBOARD_PATH_BYTES: usize = 4096;
const MAX_SLOT_ID_BYTES: usize = 128;
const DEFAULT_EXPANDED: bool = true;

static PRESENTATION_FILE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn file_lock() -> MutexGuard<'static, ()> {
    PRESENTATION_FILE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[derive(Clone, Debug)]
pub(crate) struct DashboardEditorPresentationStore {
    path: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PresentationFile {
    version: u16,
    #[serde(default)]
    dashboards: BTreeMap<String, Vec<StoredSlotPresentation>>,
}

impl Default for PresentationFile {
    fn default() -> Self {
        Self {
            version: CURRENT_VERSION,
            dashboards: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredSlotPresentation {
    #[serde(default)]
    slot_id: Option<String>,
    widget: String,
    fingerprint: u64,
    expanded: bool,
}

#[derive(Debug)]
enum PresentationFileLoadError {
    Invalid(String),
    Unavailable(anyhow::Error),
}

impl fmt::Display for PresentationFileLoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(message) => formatter.write_str(message),
            Self::Unavailable(error) => write!(formatter, "{error:#}"),
        }
    }
}

impl Error for PresentationFileLoadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Invalid(_) => None,
            Self::Unavailable(error) => Some(error.as_ref()),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct DashboardSlotPresentation {
    pub(crate) id: u64,
    pub(crate) expanded: bool,
}

/// The dashboard snapshot associated with a runtime presentation identity.
/// It remains the base file snapshot while the editor contains unsaved edits,
/// allowing an ordinary close to persist folds without persisting the draft.
#[derive(Clone, Debug)]
pub(crate) struct DashboardSlotPresentationBase {
    pub(crate) id: u64,
    pub(crate) slot: SlotConfig,
}

impl DashboardEditorPresentationStore {
    pub(crate) fn for_profile(profile_root: &Path) -> Self {
        Self {
            path: profile_root.join("dashboard-editor-presentation.json"),
        }
    }

    pub(crate) fn load(
        &self,
        dashboard_path: &Path,
        slots: &[SlotConfig],
    ) -> anyhow::Result<Vec<bool>> {
        let path_identity = dashboard_path_identity(dashboard_path)?;
        let _guard = file_lock();
        let file = load_file(&self.path).map_err(anyhow::Error::new)?;
        let stored = file.dashboards.get(&path_identity);
        Ok(reconcile_slots(
            stored.map(Vec::as_slice).unwrap_or_default(),
            slots,
        ))
    }

    pub(crate) fn save(
        &self,
        dashboard_path: &Path,
        slots: &[(SlotConfig, bool)],
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            slots.len() <= MAX_SLOTS_PER_DASHBOARD,
            "Dashboard Editor presentation has too many slots"
        );
        let path_identity = dashboard_path_identity(dashboard_path)?;
        let mut stored = slots
            .iter()
            .map(|(slot, expanded)| StoredSlotPresentation {
                slot_id: safe_slot_id(slot.id.as_deref()),
                widget: slot.widget.clone(),
                fingerprint: slot_fingerprint(slot),
                expanded: *expanded,
            })
            .collect::<Vec<_>>();
        if stored
            .iter()
            .any(|slot| slot.widget.len() > MAX_SLOT_ID_BYTES)
        {
            bail!("Dashboard Editor widget identity exceeds its preference bound");
        }
        stored.shrink_to_fit();

        let _guard = file_lock();
        let mut file = match load_file(&self.path) {
            Ok(file) => file,
            Err(PresentationFileLoadError::Invalid(_)) => PresentationFile::default(),
            Err(PresentationFileLoadError::Unavailable(error)) => return Err(error),
        };
        file.dashboards.insert(path_identity.clone(), stored);
        if file.dashboards.len() > MAX_DASHBOARDS {
            bail!("Dashboard Editor preference file contains too many dashboards");
        }
        validate_file(&file)?;
        let bytes = serde_json::to_vec_pretty(&file)
            .context("serialize Dashboard Editor presentation preferences")?;
        anyhow::ensure!(
            bytes.len() <= MAX_FILE_BYTES,
            "Dashboard Editor presentation preferences exceed their file bound"
        );
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create preference directory {}", parent.display()))?;
        }
        crate::common::atomic_file::save_atomic_replaceable(&self.path, &bytes).with_context(|| {
            format!(
                "save Dashboard Editor preferences to {}",
                self.path.display()
            )
        })
    }
}

fn load_file(path: &Path) -> Result<PresentationFile, PresentationFileLoadError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(PresentationFile::default());
        }
        Err(error) => {
            return Err(PresentationFileLoadError::Unavailable(
                anyhow::Error::from(error).context(format!("inspect {}", path.display())),
            ));
        }
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(PresentationFileLoadError::Unavailable(anyhow::anyhow!(
            "Dashboard Editor preference path is not a regular file"
        )));
    }
    if metadata.len() > MAX_FILE_BYTES as u64 {
        return Err(PresentationFileLoadError::Invalid(
            "Dashboard Editor preference file exceeds its size bound".into(),
        ));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    File::open(path)
        .map_err(|error| {
            PresentationFileLoadError::Unavailable(
                anyhow::Error::from(error).context(format!("open {}", path.display())),
            )
        })?
        .take((MAX_FILE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| {
            PresentationFileLoadError::Unavailable(
                anyhow::Error::from(error).context(format!("read {}", path.display())),
            )
        })?;
    if bytes.len() > MAX_FILE_BYTES {
        return Err(PresentationFileLoadError::Invalid(
            "Dashboard Editor preference file exceeds its size bound".into(),
        ));
    }
    let file: PresentationFile = serde_json::from_slice(&bytes).map_err(|error| {
        PresentationFileLoadError::Invalid(format!(
            "failed to parse Dashboard Editor preference data from {}: {error}",
            path.display()
        ))
    })?;
    validate_file(&file).map_err(|error| PresentationFileLoadError::Invalid(error.to_string()))?;
    Ok(file)
}

fn validate_file(file: &PresentationFile) -> anyhow::Result<()> {
    anyhow::ensure!(
        file.version == CURRENT_VERSION,
        "unsupported Dashboard Editor preference version {}",
        file.version
    );
    anyhow::ensure!(
        file.dashboards.len() <= MAX_DASHBOARDS,
        "Dashboard Editor preference file contains too many dashboards"
    );
    for (path, slots) in &file.dashboards {
        anyhow::ensure!(
            !path.is_empty() && path.len() <= MAX_DASHBOARD_PATH_BYTES,
            "Dashboard Editor preference path is invalid"
        );
        anyhow::ensure!(
            slots.len() <= MAX_SLOTS_PER_DASHBOARD,
            "Dashboard Editor preference contains too many slots"
        );
        anyhow::ensure!(
            slots.iter().all(|slot| {
                slot.widget.len() <= MAX_SLOT_ID_BYTES
                    && slot
                        .slot_id
                        .as_ref()
                        .is_none_or(|id| !id.is_empty() && id.len() <= MAX_SLOT_ID_BYTES)
            }),
            "Dashboard Editor preference contains an invalid slot identity"
        );
    }
    Ok(())
}

fn safe_slot_id(slot_id: Option<&str>) -> Option<String> {
    slot_id
        .filter(|slot_id| !slot_id.is_empty() && slot_id.len() <= MAX_SLOT_ID_BYTES)
        .map(str::to_owned)
}

fn slot_fingerprint(slot: &SlotConfig) -> u64 {
    let settings = serde_json::to_vec(&slot.settings).unwrap_or_default();
    let mut hash = 0xcbf29ce484222325_u64;
    let mut add = |bytes: &[u8]| {
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash ^= 0xff;
        hash = hash.wrapping_mul(0x100000001b3);
    };
    add(slot.widget.as_bytes());
    add(slot.row.to_string().as_bytes());
    add(slot.col.to_string().as_bytes());
    add(slot.row_span.to_string().as_bytes());
    add(slot.col_span.to_string().as_bytes());
    add(slot.overflow.as_str().as_bytes());
    add(&settings);
    hash
}

fn reconcile_slots(stored: &[StoredSlotPresentation], slots: &[SlotConfig]) -> Vec<bool> {
    let stored_ids = unique_slot_id_indices(stored.iter().map(|entry| entry.slot_id.as_deref()));
    let current_ids = unique_slot_id_indices(slots.iter().map(|slot| slot.id.as_deref()));
    let stored_fingerprints =
        unique_fingerprint_indices(stored.iter().map(|entry| entry.fingerprint));
    let current_fingerprints = unique_fingerprint_indices(slots.iter().map(slot_fingerprint));
    let mut result = vec![DEFAULT_EXPANDED; slots.len()];
    let mut claimed = HashMap::<usize, usize>::new();

    for (slot_id, &current_index) in &current_ids {
        if let Some(&stored_index) = stored_ids.get(slot_id)
            && stored[stored_index].widget == slots[current_index].widget
            && stored[stored_index].fingerprint == slot_fingerprint(&slots[current_index])
        {
            claimed.insert(current_index, stored_index);
        }
    }
    for (&fingerprint, &current_index) in &current_fingerprints {
        if claimed.contains_key(&current_index) {
            continue;
        }
        if let Some(&stored_index) = stored_fingerprints.get(&fingerprint) {
            claimed.insert(current_index, stored_index);
        }
    }
    for (current_index, stored_index) in claimed {
        result[current_index] = stored[stored_index].expanded;
    }
    result
}

fn unique_slot_id_indices<'a>(
    slot_ids: impl Iterator<Item = Option<&'a str>>,
) -> HashMap<&'a str, usize> {
    let mut counts = HashMap::<&'a str, usize>::new();
    let ids = slot_ids.collect::<Vec<_>>();
    for slot_id in ids
        .iter()
        .flatten()
        .filter(|id| !id.is_empty() && id.len() <= MAX_SLOT_ID_BYTES)
    {
        *counts.entry(slot_id).or_default() += 1;
    }
    ids.into_iter()
        .enumerate()
        .filter_map(|(index, slot_id)| {
            let slot_id = slot_id.filter(|id| !id.is_empty() && id.len() <= MAX_SLOT_ID_BYTES)?;
            (counts.get(slot_id) == Some(&1)).then_some((slot_id, index))
        })
        .collect()
}

fn unique_fingerprint_indices(fingerprints: impl Iterator<Item = u64>) -> HashMap<u64, usize> {
    let fingerprints = fingerprints.collect::<Vec<_>>();
    let mut counts = HashMap::<u64, usize>::new();
    for fingerprint in &fingerprints {
        *counts.entry(*fingerprint).or_default() += 1;
    }
    fingerprints
        .into_iter()
        .enumerate()
        .filter_map(|(index, fingerprint)| {
            (counts.get(&fingerprint) == Some(&1)).then_some((fingerprint, index))
        })
        .collect()
}

fn dashboard_path_identity(path: &Path) -> anyhow::Result<String> {
    let current_dir =
        std::env::current_dir().context("resolve Dashboard Editor preference path")?;
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        current_dir.join(path)
    };
    let identity_path =
        fs::canonicalize(&absolute).unwrap_or_else(|_| lexical_normalize(&absolute));
    let identity = crate::platform::app_data::normalize_windows_identity_path(&identity_path);
    anyhow::ensure!(
        !identity.is_empty() && identity.len() <= MAX_DASHBOARD_PATH_BYTES,
        "dashboard path exceeds the preference key bound"
    );
    Ok(identity)
}

fn lexical_normalize(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if result.file_name().is_some() {
                    result.pop();
                }
            }
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                result.push(component.as_os_str());
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_EXPANDED, DashboardEditorPresentationStore, PresentationFile,
        StoredSlotPresentation, reconcile_slots, slot_fingerprint,
    };
    use crate::dashboard::config::SlotConfig;
    use std::collections::BTreeMap;

    fn slot(id: Option<&str>, widget: &str, row: i32) -> SlotConfig {
        let mut slot = SlotConfig::with_widget(widget, row, 0);
        slot.id = id.map(str::to_owned);
        slot
    }

    fn stored(slot: &SlotConfig, expanded: bool) -> StoredSlotPresentation {
        StoredSlotPresentation {
            slot_id: slot.id.clone(),
            widget: slot.widget.clone(),
            fingerprint: slot_fingerprint(slot),
            expanded,
        }
    }

    #[test]
    fn reconciliation_follows_unique_ids_and_renames_but_resets_reused_ids() {
        let saved = [
            slot(Some("alpha"), "weather_site", 0),
            slot(Some("beta"), "quick_tools", 0),
        ];
        let persisted = vec![stored(&saved[0], false), stored(&saved[1], true)];
        let reordered_and_renamed = vec![
            slot(Some("beta-renamed"), "quick_tools", 0),
            saved[0].clone(),
        ];
        assert_eq!(
            reconcile_slots(&persisted, &reordered_and_renamed),
            vec![true, false]
        );

        let reused = vec![slot(Some("alpha"), "quick_tools", 4)];
        assert_eq!(
            reconcile_slots(&persisted, &reused),
            vec![DEFAULT_EXPANDED],
            "a reused ID with incompatible slot content is not trusted"
        );
    }

    #[test]
    fn reconciliation_never_guesses_duplicate_or_ambiguous_missing_ids() {
        let duplicate = vec![
            slot(Some("same"), "weather_site", 0),
            slot(Some("same"), "weather_site", 0),
        ];
        let persisted = vec![stored(&duplicate[0], false), stored(&duplicate[1], false)];
        assert_eq!(
            reconcile_slots(&persisted, &duplicate),
            vec![DEFAULT_EXPANDED, DEFAULT_EXPANDED]
        );

        let identical_without_ids =
            vec![slot(None, "weather_site", 0), slot(None, "weather_site", 0)];
        assert_eq!(
            reconcile_slots(&persisted, &identical_without_ids),
            vec![DEFAULT_EXPANDED, DEFAULT_EXPANDED]
        );
    }

    #[test]
    fn profile_and_dashboard_path_preferences_round_trip_independently() {
        let directory = tempfile::tempdir().unwrap();
        let store = DashboardEditorPresentationStore::for_profile(directory.path());
        let dashboard_a = directory.path().join("a.json");
        let dashboard_b = directory.path().join("b.json");
        let slots = vec![
            slot(Some("a"), "weather_site", 0),
            slot(None, "quick_tools", 0),
        ];
        store
            .save(
                &dashboard_a,
                &[(slots[0].clone(), false), (slots[1].clone(), true)],
            )
            .unwrap();
        store
            .save(&dashboard_b, &[(slot(Some("b"), "weather_site", 0), true)])
            .unwrap();
        assert_eq!(store.load(&dashboard_a, &slots).unwrap(), vec![false, true]);
        assert_eq!(
            store
                .load(&dashboard_b, &[slot(Some("b"), "weather_site", 0)])
                .unwrap(),
            vec![true]
        );
    }

    #[test]
    fn dashboard_path_identity_is_stable_when_a_missing_file_is_created() {
        let directory = tempfile::tempdir().unwrap();
        let dashboard = directory.path().join("dashboard.json");
        let before = super::dashboard_path_identity(&dashboard).unwrap();
        std::fs::write(&dashboard, b"{}").unwrap();
        let after = super::dashboard_path_identity(&dashboard).unwrap();
        assert_eq!(before, after);
    }

    #[test]
    fn malformed_or_oversized_preference_data_fails_closed() {
        let directory = tempfile::tempdir().unwrap();
        let store = DashboardEditorPresentationStore::for_profile(directory.path());
        std::fs::write(
            directory.path().join("dashboard-editor-presentation.json"),
            b"{broken",
        )
        .unwrap();
        assert!(
            store
                .load(&directory.path().join("dash.json"), &[])
                .is_err()
        );

        let large = vec![b' '; super::MAX_FILE_BYTES + 1];
        std::fs::write(
            directory.path().join("dashboard-editor-presentation.json"),
            large,
        )
        .unwrap();
        assert!(
            store
                .load(&directory.path().join("dash.json"), &[])
                .is_err()
        );

        let unavailable_directory = tempfile::tempdir().unwrap();
        let unavailable_store =
            DashboardEditorPresentationStore::for_profile(unavailable_directory.path());
        let preference_path = unavailable_directory
            .path()
            .join("dashboard-editor-presentation.json");
        std::fs::create_dir(&preference_path).unwrap();
        assert!(
            unavailable_store
                .load(&unavailable_directory.path().join("dash.json"), &[])
                .is_err()
        );
        assert!(
            unavailable_store
                .save(&unavailable_directory.path().join("dash.json"), &[])
                .is_err()
        );
        assert!(
            preference_path.is_dir(),
            "an unsafe preference path is never replaced"
        );
    }

    #[test]
    fn preference_envelope_has_an_explicit_version() {
        let file = PresentationFile {
            version: super::CURRENT_VERSION,
            dashboards: BTreeMap::new(),
        };
        assert_eq!(
            serde_json::to_value(file).unwrap()["version"],
            super::CURRENT_VERSION
        );
    }
}
