use super::{BaseUnit, PanelUnit};
use crate::platform::app_data::AppDataRoot;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{self, Read};
#[cfg(test)]
use std::path::Path;
use std::path::PathBuf;

const PREFERENCES_FILE: &str = "converter_preferences.json";
const PREFERENCES_FORMAT_VERSION: u32 = 1;
const MAX_PREFERENCES_BYTES: usize = 16 * 1024;
pub(super) const MAX_FAVORITES: usize = 32;
pub(super) const MAX_DECIMAL_PLACES: u8 = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum NumberFormat {
    Smart,
    Fixed,
    Scientific,
}

impl NumberFormat {
    pub(super) const ALL: &'static [Self] = &[Self::Smart, Self::Fixed, Self::Scientific];

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Smart => "Smart",
            Self::Fixed => "Fixed",
            Self::Scientific => "Scientific",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DisplayPreferences {
    pub(super) favorites: Vec<PanelUnit>,
    pub(super) number_format: NumberFormat,
    pub(super) decimal_places: u8,
}

impl Default for DisplayPreferences {
    fn default() -> Self {
        Self {
            favorites: Vec::new(),
            number_format: NumberFormat::Smart,
            decimal_places: 3,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredPreferences {
    format_version: u32,
    favorites: Vec<StoredFavorite>,
    number_format: NumberFormat,
    decimal_places: u8,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum StoredFavorite {
    Physical { symbol: String },
    Base { radix: u8 },
}

pub(super) struct PreferencesStore {
    path: PathBuf,
}

impl PreferencesStore {
    pub(super) fn new(root: &AppDataRoot) -> Self {
        Self {
            path: root.path().join(PREFERENCES_FILE),
        }
    }

    fn with_path(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub(super) fn load(&self) -> Result<(DisplayPreferences, Option<Vec<u8>>), String> {
        let Some(bytes) = self.read_snapshot()? else {
            return Ok((DisplayPreferences::default(), None));
        };

        let stored: StoredPreferences = serde_json::from_slice(&bytes).map_err(|error| {
            format!(
                "Converter preferences are invalid at {}: {error}",
                self.path.display()
            )
        })?;
        self.validate_stored(stored)
            .map(|preferences| (preferences, Some(bytes)))
    }

    fn read_snapshot(&self) -> Result<Option<Vec<u8>>, String> {
        let metadata = match fs::symlink_metadata(&self.path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(None);
            }
            Err(error) => {
                return Err(format!(
                    "Could not inspect Converter preferences at {}: {error}",
                    self.path.display()
                ));
            }
        };
        if !metadata.file_type().is_file() {
            return Err(format!(
                "Converter preferences path is not a regular file: {}",
                self.path.display()
            ));
        }
        if metadata.len() > MAX_PREFERENCES_BYTES as u64 {
            return Err(format!(
                "Converter preferences exceed the {MAX_PREFERENCES_BYTES}-byte limit"
            ));
        }

        let file = File::open(&self.path).map_err(|error| {
            format!(
                "Could not read Converter preferences at {}: {error}",
                self.path.display()
            )
        })?;
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        file.take((MAX_PREFERENCES_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|error| {
                format!(
                    "Could not read Converter preferences at {}: {error}",
                    self.path.display()
                )
            })?;
        if bytes.len() > MAX_PREFERENCES_BYTES {
            return Err(format!(
                "Converter preferences exceed the {MAX_PREFERENCES_BYTES}-byte limit"
            ));
        }
        Ok(Some(bytes))
    }

    pub(super) fn save(
        &self,
        preferences: &DisplayPreferences,
        expected_snapshot: Option<&[u8]>,
    ) -> Result<Vec<u8>, String> {
        validate_preferences(preferences)?;
        if self.read_snapshot()?.as_deref() != expected_snapshot {
            return Err(format!(
                "Converter preferences changed on disk since they were loaded at {}. Restart the app to load the latest preferences before saving.",
                self.path.display()
            ));
        }

        let stored = StoredPreferences {
            format_version: PREFERENCES_FORMAT_VERSION,
            favorites: preferences
                .favorites
                .iter()
                .copied()
                .map(StoredFavorite::from_panel_unit)
                .collect(),
            number_format: preferences.number_format,
            decimal_places: preferences.decimal_places,
        };
        let bytes = serde_json::to_vec_pretty(&stored)
            .map_err(|error| format!("Could not encode Converter preferences: {error}"))?;
        if bytes.len() > MAX_PREFERENCES_BYTES {
            return Err(format!(
                "Converter preferences exceed the {MAX_PREFERENCES_BYTES}-byte limit"
            ));
        }
        crate::common::atomic_file::save_atomic(&self.path, &bytes).map_err(|error| {
            format!(
                "Could not save Converter preferences at {}: {error}",
                self.path.display()
            )
        })?;
        Ok(bytes)
    }

    fn validate_stored(&self, stored: StoredPreferences) -> Result<DisplayPreferences, String> {
        if stored.format_version != PREFERENCES_FORMAT_VERSION {
            return Err(format!(
                "Unsupported Converter preferences version {} (supported version is {})",
                stored.format_version, PREFERENCES_FORMAT_VERSION
            ));
        }
        if stored.favorites.len() > MAX_FAVORITES {
            return Err(format!(
                "Converter preferences contain more than {MAX_FAVORITES} favorites"
            ));
        }
        if stored.decimal_places > MAX_DECIMAL_PLACES {
            return Err(format!(
                "Converter precision must be between 0 and {MAX_DECIMAL_PLACES} decimal places"
            ));
        }

        let mut favorites = Vec::with_capacity(stored.favorites.len());
        for favorite in stored.favorites {
            let favorite = favorite.into_panel_unit()?;
            if favorites.contains(&favorite) {
                return Err("Converter preferences contain a duplicate favorite".into());
            }
            favorites.push(favorite);
        }

        Ok(DisplayPreferences {
            favorites,
            number_format: stored.number_format,
            decimal_places: stored.decimal_places,
        })
    }

    #[cfg(test)]
    fn path(&self) -> &Path {
        &self.path
    }
}

impl StoredFavorite {
    fn from_panel_unit(unit: PanelUnit) -> Self {
        match unit {
            PanelUnit::Physical(unit) => Self::Physical {
                symbol: unit.symbol().to_owned(),
            },
            PanelUnit::Base(unit) => Self::Base {
                radix: unit.radix() as u8,
            },
        }
    }

    fn into_panel_unit(self) -> Result<PanelUnit, String> {
        match self {
            Self::Physical { symbol } => {
                let unit = crate::unit_conversion::unit_by_alias(&symbol)
                    .filter(|unit| unit.symbol() == symbol)
                    .ok_or_else(|| {
                        format!("Unknown or noncanonical Converter favorite unit: {symbol}")
                    })?;
                Ok(PanelUnit::Physical(unit))
            }
            Self::Base { radix } => {
                let base = match radix {
                    2 => BaseUnit::Binary,
                    8 => BaseUnit::Octal,
                    10 => BaseUnit::Decimal,
                    16 => BaseUnit::Hexadecimal,
                    _ => {
                        return Err(format!(
                            "Unsupported Converter base favorite radix: {radix}"
                        ));
                    }
                };
                Ok(PanelUnit::Base(base))
            }
        }
    }
}

fn validate_preferences(preferences: &DisplayPreferences) -> Result<(), String> {
    if preferences.favorites.len() > MAX_FAVORITES {
        return Err(format!(
            "Converter favorites are limited to {MAX_FAVORITES} units"
        ));
    }
    if preferences.decimal_places > MAX_DECIMAL_PLACES {
        return Err(format!(
            "Converter precision must be between 0 and {MAX_DECIMAL_PLACES} decimal places"
        ));
    }
    for (index, unit) in preferences.favorites.iter().enumerate() {
        if preferences.favorites[..index].contains(unit) {
            return Err("Converter preferences contain a duplicate favorite".into());
        }
    }
    Ok(())
}

#[derive(Default)]
pub(super) struct PreferencesState {
    store: Option<PreferencesStore>,
    pub(super) current: DisplayPreferences,
    saved: DisplayPreferences,
    baseline: Option<Vec<u8>>,
    pub(super) load_error: Option<String>,
    pub(super) feedback: Option<String>,
}

impl PreferencesState {
    pub(super) fn with_data_root(root: &AppDataRoot) -> Self {
        Self::with_store(PreferencesStore::new(root))
    }

    fn with_store(store: PreferencesStore) -> Self {
        let (current, baseline, load_error) = match store.load() {
            Ok((preferences, baseline)) => (preferences, baseline, None),
            Err(error) => (DisplayPreferences::default(), None, Some(error)),
        };
        Self {
            store: Some(store),
            saved: current.clone(),
            current,
            baseline,
            load_error,
            feedback: None,
        }
    }

    pub(super) fn has_unsaved_changes(&self) -> bool {
        self.current != self.saved
    }

    pub(super) fn save_explicitly(&mut self) {
        if let Some(error) = self.load_error.as_deref() {
            self.feedback = Some(format!(
                "Cannot save Converter preferences because they could not be loaded: {error}"
            ));
            return;
        }

        let result = self.store.as_ref().map_or_else(
            || Err("Converter preference storage is not configured".to_owned()),
            |store| store.save(&self.current, self.baseline.as_deref()),
        );
        match result {
            Ok(snapshot) => {
                self.baseline = Some(snapshot);
                self.saved = self.current.clone();
                self.load_error = None;
                self.feedback = Some("Preferences saved.".into());
            }
            Err(error) => self.feedback = Some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DisplayPreferences, MAX_DECIMAL_PLACES, MAX_FAVORITES, NumberFormat, PreferencesState,
        PreferencesStore,
    };
    use crate::gui::convert_panel::{BaseUnit, PanelUnit};
    use crate::unit_conversion::Unit;

    #[test]
    fn explicitly_saved_preferences_round_trip_canonical_physical_and_tagged_base_favorites() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("converter_preferences.json");
        let store = PreferencesStore::with_path(&path);
        let (loaded, baseline) = store.load().unwrap();
        assert!(loaded.favorites.is_empty());
        assert!(baseline.is_none());

        let preferences = DisplayPreferences {
            favorites: vec![
                PanelUnit::Physical(Unit::Megabyte),
                PanelUnit::Physical(Unit::Megabit),
                PanelUnit::Base(BaseUnit::Hexadecimal),
            ],
            number_format: NumberFormat::Scientific,
            decimal_places: 5,
        };
        let saved_snapshot = store.save(&preferences, baseline.as_deref()).unwrap();

        let json = std::fs::read_to_string(&path).unwrap();
        assert!(json.contains("\"symbol\": \"mb\""));
        assert!(json.contains("\"symbol\": \"mbit\""));
        assert!(!json.contains("\"symbol\": \"MB\""));
        assert!(!json.contains("\"symbol\": \"Mb\""));
        assert!(json.contains("\"radix\": 16"));
        let (reloaded, baseline) = store.load().unwrap();
        assert_eq!(reloaded, preferences);
        assert_eq!(baseline.as_deref(), Some(saved_snapshot.as_slice()));
    }

    #[test]
    fn invalid_or_oversized_preferences_are_rejected_without_rewriting_existing_bytes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("converter_preferences.json");
        let store = PreferencesStore::with_path(&path);
        let invalid =
            br#"{"format_version":99,"favorites":[],"number_format":"smart","decimal_places":3}"#;
        std::fs::write(&path, invalid).unwrap();
        assert!(
            store
                .load()
                .unwrap_err()
                .contains("Unsupported Converter preferences version")
        );
        assert_eq!(std::fs::read(&path).unwrap(), invalid);

        std::fs::write(&path, vec![b'x'; 16 * 1024 + 1]).unwrap();
        assert!(store.load().unwrap_err().contains("byte limit"));
        assert_eq!(
            std::fs::metadata(&path).unwrap().len(),
            (16 * 1024 + 1) as u64
        );
    }

    #[test]
    fn failed_load_blocks_explicit_save_and_preserves_future_version_bytes_and_diagnostics() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("converter_preferences.json");
        let bytes =
            br#"{"format_version":99,"favorites":[],"number_format":"smart","decimal_places":3}"#;
        std::fs::write(&path, bytes).unwrap();
        let mut state = PreferencesState::with_store(PreferencesStore::with_path(&path));
        let load_error = state
            .load_error
            .clone()
            .expect("future version is diagnosed");
        state.current.number_format = NumberFormat::Fixed;
        state.current.decimal_places = 4;
        let working = state.current.clone();

        state.save_explicitly();

        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert_eq!(state.current, working);
        assert_eq!(state.load_error.as_deref(), Some(load_error.as_str()));
        assert!(state.has_unsaved_changes());
        assert!(
            state
                .feedback
                .as_deref()
                .is_some_and(|message| message.contains("could not be loaded"))
        );
    }

    #[test]
    fn external_preference_changes_deletions_and_unreadable_paths_block_explicit_save() {
        for mutation in ["changed", "created", "deleted", "unreadable"] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("converter_preferences.json");
            let store = PreferencesStore::with_path(&path);
            if mutation != "created" {
                store
                    .save(&DisplayPreferences::default(), None)
                    .expect("create a valid baseline");
            }
            let mut state = PreferencesState::with_store(PreferencesStore::with_path(&path));
            state.current.number_format = NumberFormat::Scientific;
            let external_bytes = b"external preferences";
            match mutation {
                "changed" => std::fs::write(&path, external_bytes).unwrap(),
                "created" => std::fs::write(&path, external_bytes).unwrap(),
                "deleted" => std::fs::remove_file(&path).unwrap(),
                "unreadable" => {
                    std::fs::remove_file(&path).unwrap();
                    std::fs::create_dir(&path).unwrap();
                }
                _ => unreachable!(),
            }

            state.save_explicitly();

            assert!(state.has_unsaved_changes(), "{mutation}");
            assert_eq!(state.current.number_format, NumberFormat::Scientific);
            assert!(state.feedback.is_some(), "{mutation}");
            match mutation {
                "changed" => assert_eq!(std::fs::read(&path).unwrap(), external_bytes),
                "created" => assert_eq!(std::fs::read(&path).unwrap(), external_bytes),
                "deleted" => assert!(!path.exists()),
                "unreadable" => assert!(std::fs::metadata(&path).unwrap().is_dir()),
                _ => unreachable!(),
            }
        }
    }

    #[test]
    fn failed_preference_load_keeps_working_and_saved_state_separate() {
        let directory = tempfile::tempdir().unwrap();
        let blocker = directory.path().join("not-a-directory");
        std::fs::write(&blocker, b"occupant").unwrap();
        let store = PreferencesStore::with_path(blocker.join("converter_preferences.json"));
        let mut state = PreferencesState::with_store(store);
        state.current.number_format = NumberFormat::Fixed;
        state.current.decimal_places = MAX_DECIMAL_PLACES;
        state
            .current
            .favorites
            .push(PanelUnit::Base(BaseUnit::Binary));

        state.save_explicitly();

        assert!(state.has_unsaved_changes());
        assert!(
            state
                .feedback
                .as_deref()
                .is_some_and(|message| message.contains("save"))
        );
        assert_eq!(state.current.number_format, NumberFormat::Fixed);
        assert_eq!(state.current.decimal_places, MAX_DECIMAL_PLACES);
        assert_eq!(
            state.current.favorites,
            vec![PanelUnit::Base(BaseUnit::Binary)]
        );
    }

    #[test]
    fn defaults_are_bounded_and_empty_without_any_filesystem_configuration() {
        let state = PreferencesState::default();
        assert!(!state.has_unsaved_changes());
        assert!(state.load_error.is_none());
        assert!(state.feedback.is_none());
        assert_eq!(state.current.number_format, NumberFormat::Smart);
        assert_eq!(state.current.decimal_places, 3);
        assert!(state.current.favorites.len() <= MAX_FAVORITES);
        assert!(state.current.decimal_places <= MAX_DECIMAL_PLACES);
        assert_eq!(state.current, DisplayPreferences::default());
    }
}
