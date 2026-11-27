#[path = "scoped_preferences.rs"]
mod scoped_preferences;
pub use scoped_preferences::{ScopedSetting, ScopedSettingChange, ScopedSettingValue, ScopedSettingsAcknowledgement, ScopedSettingsError};
#[path = "launcher_hotkey.rs"]
mod launcher_hotkey;
pub use launcher_hotkey::LauncherHotkeySnapshot;
#[path = "omni_preferences.rs"]
mod omni_preferences;
pub use omni_preferences::{OmniPreferencesSnapshot, OmniPreferencesAcknowledgement, OmniCommittedUnverified};

use super::Settings;
use crate::common::persistence::{LoadState, PersistenceError, load_json, save_json_atomic};
use anyhow::Context;
use once_cell::sync::Lazy;
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};
use std::sync::atomic::{AtomicU8, Ordering};

pub(super) type RawSettingsObject = BTreeMap<String, Box<serde_json::value::RawValue>>;

/// Root and optional feature preferences owned by the ordinary launcher close.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LauncherClosePreferences {
    pub(crate) window_size: (i32, i32),
    pub(crate) pinned_panels: Vec<crate::gui::Panel>,
    pub(crate) file_search_ui_preferences:
        Option<crate::file_search::settings::FileSearchUiPreferences>,
    pub(crate) calculation_pad_display:
        Option<crate::settings::CalculationPadDisplayPreferences>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsBooleanPreference {
    QueryAutocomplete,
    ShowExamples,
}

impl SettingsBooleanPreference {
    fn key(self) -> &'static str {
        match self {
            Self::QueryAutocomplete => "query_autocomplete",
            Self::ShowExamples => "show_examples",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::QueryAutocomplete => "query autocomplete",
            Self::ShowExamples => "Show examples",
        }
    }

    fn get(self, settings: &Settings) -> bool {
        match self {
            Self::QueryAutocomplete => settings.query_autocomplete,
            Self::ShowExamples => settings.show_examples,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SettingsBooleanSaveRequest {
    preference: SettingsBooleanPreference,
    expected_baseline: bool,
    desired: bool,
}

impl SettingsBooleanSaveRequest {
    pub fn new(
        preference: SettingsBooleanPreference,
        expected_baseline: bool,
        desired: bool,
    ) -> Self {
        Self {
            preference,
            expected_baseline,
            desired,
        }
    }

    pub fn preference(self) -> SettingsBooleanPreference {
        self.preference
    }

    pub fn expected_baseline(self) -> bool {
        self.expected_baseline
    }

    pub fn desired(self) -> bool {
        self.desired
    }
}

const SAVE_PENDING: u8 = 0;
const SAVE_WAITING_FOR_SETTINGS: u8 = 1;
const SAVE_CANCELED: u8 = 2;
const SAVE_COMMITTING: u8 = 3;

/// A registry-owned cancellation fence for one Settings Boolean write.
/// Cancellation wins only until the worker claims its atomic commit boundary.
#[derive(Clone, Debug)]
pub struct SettingsBooleanSaveToken {
    id: u64,
    state: Arc<AtomicU8>,
}

impl SettingsBooleanSaveToken {
    pub(crate) fn new(id: u64) -> Self {
        Self {
            id,
            state: Arc::new(AtomicU8::new(SAVE_PENDING)),
        }
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    /// Returns true when cancellation is accepted or was already accepted.
    /// False means the worker has crossed the commit boundary.
    pub fn request_cancel(&self) -> bool {
        loop {
            match self.state.load(Ordering::Acquire) {
                SAVE_PENDING | SAVE_WAITING_FOR_SETTINGS => {
                    let state = self.state.load(Ordering::Acquire);
                    if state == SAVE_PENDING || state == SAVE_WAITING_FOR_SETTINGS {
                        if self
                            .state
                            .compare_exchange(
                                state,
                                SAVE_CANCELED,
                                Ordering::AcqRel,
                                Ordering::Acquire,
                            )
                            .is_ok()
                        {
                            return true;
                        }
                    }
                }
                SAVE_CANCELED => return true,
                SAVE_COMMITTING => return false,
                _ => return false,
            }
        }
    }

    pub(crate) fn is_canceled(&self) -> bool {
        self.state.load(Ordering::Acquire) == SAVE_CANCELED
    }

    pub(crate) fn mark_waiting_for_settings(&self) -> bool {
        loop {
            match self.state.load(Ordering::Acquire) {
                SAVE_PENDING => {
                    if self
                        .state
                        .compare_exchange(
                            SAVE_PENDING,
                            SAVE_WAITING_FOR_SETTINGS,
                            Ordering::AcqRel,
                            Ordering::Acquire,
                        )
                        .is_ok()
                    {
                        return true;
                    }
                }
                SAVE_WAITING_FOR_SETTINGS => return true,
                SAVE_CANCELED | SAVE_COMMITTING => return false,
                _ => return false,
            }
        }
    }

    pub(crate) fn begin_irrevocable_commit(&self) -> bool {
        loop {
            match self.state.load(Ordering::Acquire) {
                SAVE_PENDING | SAVE_WAITING_FOR_SETTINGS => {
                    let state = self.state.load(Ordering::Acquire);
                    if state == SAVE_PENDING || state == SAVE_WAITING_FOR_SETTINGS {
                        if self
                            .state
                            .compare_exchange(
                                state,
                                SAVE_COMMITTING,
                                Ordering::AcqRel,
                                Ordering::Acquire,
                            )
                            .is_ok()
                        {
                            return true;
                        }
                    }
                }
                SAVE_CANCELED => return false,
                SAVE_COMMITTING => return true,
                _ => return false,
            }
        }
    }

    #[cfg(test)]
    fn is_waiting_for_settings(&self) -> bool {
        self.state.load(Ordering::Acquire) == SAVE_WAITING_FOR_SETTINGS
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SettingsBooleanSaveAcknowledgement {
    preference: SettingsBooleanPreference,
    value: bool,
    persisted: bool,
}

impl SettingsBooleanSaveAcknowledgement {
    pub fn preference(self) -> SettingsBooleanPreference {
        self.preference
    }

    pub fn value(self) -> bool {
        self.value
    }

    pub fn persisted(self) -> bool {
        self.persisted
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SettingsBooleanSaveError {
    Cancelled,
    Conflict {
        preference: SettingsBooleanPreference,
        expected: bool,
        actual: bool,
    },
    Store(String),
}

impl std::fmt::Display for SettingsBooleanSaveError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str("Settings Boolean save was canceled"),
            Self::Conflict {
                preference,
                expected,
                actual,
            } => write!(
                formatter,
                "{} changed from the saved baseline ({expected}) to {actual}",
                preference.label()
            ),
            Self::Store(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for SettingsBooleanSaveError {}

/// Canonical runtime capability map and the effective Search state for one provider.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchCapabilitySnapshot {
    pub capabilities: Option<HashMap<String, Vec<String>>>,
    pub search_enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchCapabilitySaveRequest {
    pub provider: String,
    /// Effective enabled non-Search capabilities used only when no row exists.
    pub inherited_enabled_siblings: Vec<String>,
    pub expected_baseline: bool,
    pub desired: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchCapabilitySaveAcknowledgement {
    pub snapshot: SearchCapabilitySnapshot,
    pub changed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SearchCapabilityError {
    Conflict {
        provider: String,
        expected: bool,
        actual: bool,
    },
    Store(String),
}

impl std::fmt::Display for SearchCapabilityError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Conflict {
                provider,
                expected,
                actual,
            } => write!(
                formatter,
                "Search capability for {provider} changed from {expected} to {actual}. Reload Settings and retry."
            ),
            Self::Store(message) => formatter.write_str(message),
        }
    }
}
impl std::error::Error for SearchCapabilityError {}

/// Missing capability maps and missing provider entries both enable Search.
pub fn provider_search_capability_enabled(
    capabilities: Option<&HashMap<String, Vec<String>>>,
    provider: &str,
) -> bool {
    provider_capability_enabled(capabilities, provider, "search")
}

/// Resolve an advertised capability against explicit settings and inherited defaults.
pub fn provider_capability_enabled(
    capabilities: Option<&HashMap<String, Vec<String>>>,
    provider: &str,
    capability: &str,
) -> bool {
    capabilities
        .and_then(|map| map.get(provider))
        .map_or_else(
            || (provider, capability) != ("folders", "show_full_path"),
            |caps| caps.iter().any(|value| value == capability),
        )
}

fn search_capability_snapshot(settings: &Settings, provider: &str) -> SearchCapabilitySnapshot {
    let capabilities = settings.enabled_capabilities.clone();
    let search_enabled = provider_search_capability_enabled(capabilities.as_ref(), provider);
    SearchCapabilitySnapshot {
        capabilities,
        search_enabled,
    }
}

fn validate_search_provider(provider: &str) -> Result<(), SearchCapabilityError> {
    if provider.is_empty() || provider.trim() != provider {
        return Err(SearchCapabilityError::Store(
            "Search capability requires a valid provider name".into(),
        ));
    }
    Ok(())
}

static SETTINGS_TRANSACTION: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

impl Settings {
    /// Mutate one utility envelope while retaining the raw values owned by
    /// other Settings writers. The candidate is validated before committing.
    pub(super) fn update_utility_envelope<T>(
        path: impl AsRef<Path>,
        field: &str,
        mutate: impl FnOnce(&mut Self) -> anyhow::Result<(bool, T)>,
        patch: impl FnOnce(
            Option<&serde_json::value::RawValue>,
            &Self,
        ) -> anyhow::Result<Box<serde_json::value::RawValue>>,
        matches: impl FnOnce(&Self, &Self) -> bool,
    ) -> anyhow::Result<T> {
        let _transaction = transaction_guard();
        let path = path.as_ref();
        let bytes = read_exact_bytes(path)?;
        let mut settings = parse_effective_settings(path, &bytes)?;
        let (changed, result) = mutate(&mut settings)?;
        if !changed {
            return Ok(result);
        }
        let mut root = match serde_json::from_slice::<RawSettingsObject>(&bytes) {
            Ok(root) => root,
            // Valid positional roots and empty files have no raw object members.
            Err(_) => raw_object(raw_value(&settings)?.as_ref())
                .context("failed to canonicalize utility Settings")?,
        };
        let envelope = patch(root.get(field).map(Box::as_ref), &settings)?;
        root.insert(field.to_owned(), envelope);
        let candidate = serde_json::to_vec(&root)
            .context("failed to serialize utility Settings transaction")?;
        let validated = parse_effective_settings(path, &candidate)?;
        anyhow::ensure!(
            matches(&settings, &validated),
            "utility Settings candidate did not preserve the intended value"
        );
        save_json_atomic(path, &root).context("failed to atomically persist utility Settings")?;
        Ok(result)
    }

    /// Load settings while preserving the generic missing/empty distinction.
    pub fn load_typed(path: impl AsRef<Path>) -> Result<LoadState<Self>, PersistenceError> {
        load_json(path)
    }

    /// Compatibility loader: missing and empty settings use defaults, while an
    /// existing malformed or unreadable file remains an error.
    pub fn load(path: &str) -> anyhow::Result<Self> {
        load_effective(path)
    }

    /// Atomically save settings under the settings-owned transaction lock.
    pub fn save(&self, path: &str) -> anyhow::Result<()> {
        self.save_typed(path).map_err(Into::into)
    }

    pub(crate) fn save_typed(&self, path: impl AsRef<Path>) -> Result<(), PersistenceError> {
        let _transaction = transaction_guard();
        save_json_atomic(path, self)
    }

    /// Serialize a complete settings read-modify-write transaction.
    ///
    /// The committed value is returned only after its atomic write succeeds, so
    /// callers can safely publish or apply that returned value.
    pub fn update(
        path: &str,
        mutate: impl FnOnce(&mut Self) -> anyhow::Result<()>,
    ) -> anyhow::Result<Self> {
        let _transaction = transaction_guard();
        let mut settings = load_effective(path)?;
        mutate(&mut settings)?;
        save_json_atomic(path, &settings)?;
        Ok(settings)
    }

    /// Apply a settings transaction and skip the disk write when its persisted
    /// representation is unchanged. The comparison and optional commit share
    /// the same transaction lock as every other settings update.
    pub(crate) fn update_if_changed<T>(
        path: &str,
        mutate: impl FnOnce(&mut Self) -> anyhow::Result<T>,
    ) -> anyhow::Result<(bool, T)> {
        let _transaction = transaction_guard();
        let mut settings = load_effective(path)?;
        let before = serde_json::to_value(&settings)
            .context("failed to serialize settings before update")?;
        let result = mutate(&mut settings)?;
        let after =
            serde_json::to_value(&settings).context("failed to serialize settings after update")?;
        if before == after {
            return Ok((false, result));
        }
        save_json_atomic(path, &settings)?;
        Ok((true, result))
    }

    /// Read the latest settings while serialized with settings updates.
    pub fn read_current(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let _transaction = transaction_guard();
        load_effective(path)
    }

    /// Keep the Settings transaction held through an exact live-owner claim.
    pub fn with_current<T>(path: impl AsRef<Path>, claim: impl FnOnce(&Self, &dyn Fn() -> Result<Self, String>) -> Result<T, String>) -> Result<T, String> {
        let _transaction = transaction_guard();
        let path = path.as_ref();
        let read = || load_effective(path).map_err(|error| error.to_string());
        let settings = read()?;
        claim(&settings, &read)
    }

    /// Persist every close-owned value in one atomic transaction while
    /// retaining raw JSON for all unrelated Settings values.
    pub(crate) fn save_launcher_close_preferences(
        path: impl AsRef<Path>,
        preferences: &LauncherClosePreferences,
    ) -> anyhow::Result<()> {
        let _transaction = transaction_guard();
        let path = path.as_ref();
        let source_bytes = read_exact_bytes(path)?;
        let source_settings = parse_effective_settings(path, &source_bytes)
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        let raw_source = if source_bytes.iter().all(u8::is_ascii_whitespace) {
            // Missing and whitespace-only stores begin from complete defaults.
            serde_json::to_vec(&source_settings)
                .context("failed to serialize default Settings for launcher close")?
        } else {
            source_bytes
        };
        let mut raw_settings = match serde_json::from_slice::<RawSettingsObject>(&raw_source) {
            Ok(raw_settings) => raw_settings,
            Err(_) => {
                // `parse_effective_settings` above already validated this
                // source. Older serde-compatible positional Settings roots
                // have no raw object siblings to preserve, so canonicalize
                // that typed value before applying the close-owned fields.
                raw_object(raw_value(&source_settings)?.as_ref())
                    .context("failed to canonicalize typed Settings for launcher close")?
            }
        };

        // These root values are always written. In particular, even an equal
        // replacement must exercise the atomic read-share-lock failure path.
        raw_settings.insert(
            "window_size".to_owned(),
            raw_value(&Some(preferences.window_size))?,
        );
        raw_settings.insert(
            "pinned_panels".to_owned(),
            raw_value(&preferences.pinned_panels)?,
        );

        if let Some(ui_preferences) = &preferences.file_search_ui_preferences {
            let mut plugin_settings = match raw_settings.get("plugin_settings") {
                Some(raw) => raw_object(raw).context("plugin_settings must be a JSON object")?,
                None => RawSettingsObject::new(),
            };
            let file_search = patch_file_search_ui_preferences(
                plugin_settings.get("file_search").map(Box::as_ref),
                source_settings.plugin_settings.get("file_search"),
                ui_preferences,
            )?;
            plugin_settings.insert("file_search".to_owned(), file_search);
            raw_settings.insert("plugin_settings".to_owned(), raw_value(&plugin_settings)?);
        }

        if let Some(display_preferences) = preferences.calculation_pad_display {
            let display = raw_object_with_replacements(
                raw_settings.get("calculation_pad_display").map(Box::as_ref),
                &display_preferences,
            )?;
            raw_settings.insert("calculation_pad_display".to_owned(), display);
        }

        let candidate_bytes = serde_json::to_vec(&raw_settings)
            .context("failed to serialize launcher close Settings transaction")?;
        parse_effective_settings(path, &candidate_bytes)
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        save_json_atomic(path, &raw_settings)
            .context("failed to atomically persist launcher close preferences")
    }

    /// Remove one panel from the latest persisted pins, preserving unrelated raw values.
    pub fn unpin_panel_checked(path: impl AsRef<Path>, panel: crate::gui::Panel) -> anyhow::Result<()> {
        let _transaction = transaction_guard();
        let path = path.as_ref();
        let source_bytes = read_exact_bytes(path)?;
        let mut settings = parse_effective_settings(path, &source_bytes)?;
        if !settings.pinned_panels.contains(&panel) {
            return Ok(());
        }
        settings.pinned_panels.retain(|pinned| *pinned != panel);

        let mut raw_settings = match serde_json::from_slice::<RawSettingsObject>(&source_bytes) {
            Ok(raw_settings) => raw_settings,
            Err(_) => raw_object(raw_value(&settings)?.as_ref())
                .context("failed to canonicalize typed Settings for unpin")?,
        };
        raw_settings.insert(
            "pinned_panels".to_owned(),
            raw_value(&settings.pinned_panels)?,
        );
        let candidate = serde_json::to_vec(&raw_settings)
            .context("failed to serialize unpin Settings transaction")?;
        parse_effective_settings(path, &candidate)?;
        save_json_atomic(path, &raw_settings).context("failed to atomically persist unpin")
    }

    /// Read the current effective Search preference and canonical map under the Settings lock.
    pub fn read_search_capability(
        path: impl AsRef<Path>,
        provider: &str,
    ) -> Result<SearchCapabilitySnapshot, SearchCapabilityError> {
        validate_search_provider(provider)?;
        let _transaction = transaction_guard();
        let path = path.as_ref();
        let bytes = read_exact_bytes(path)
            .map_err(|error| SearchCapabilityError::Store(error.to_string()))?;
        let settings = parse_effective_settings(path, &bytes)
            .map_err(|error| SearchCapabilityError::Store(error.to_string()))?;
        Ok(search_capability_snapshot(&settings, provider))
    }

    /// Compare only this provider's effective Search state, preserving raw siblings.
    pub fn save_search_capability_checked(
        path: impl AsRef<Path>,
        request: SearchCapabilitySaveRequest,
    ) -> Result<SearchCapabilitySaveAcknowledgement, SearchCapabilityError> {
        validate_search_provider(&request.provider)?;
        let _transaction = transaction_guard();
        let path = path.as_ref();
        let bytes = read_exact_bytes(path)
            .map_err(|error| SearchCapabilityError::Store(error.to_string()))?;
        let settings = parse_effective_settings(path, &bytes)
            .map_err(|error| SearchCapabilityError::Store(error.to_string()))?;
        let snapshot = search_capability_snapshot(&settings, &request.provider);
        if snapshot.search_enabled != request.expected_baseline {
            return Err(SearchCapabilityError::Conflict {
                provider: request.provider,
                expected: request.expected_baseline,
                actual: snapshot.search_enabled,
            });
        }
        if snapshot.search_enabled == request.desired {
            return Ok(SearchCapabilitySaveAcknowledgement {
                snapshot,
                changed: false,
            });
        }
        let raw_source = if bytes.iter().all(u8::is_ascii_whitespace) {
            serde_json::to_vec(&settings).map_err(|error| {
                SearchCapabilityError::Store(format!(
                    "failed to serialize default Settings: {error}"
                ))
            })?
        } else {
            bytes
        };
        let mut root: RawSettingsObject = serde_json::from_slice(&raw_source).map_err(|error| {
            SearchCapabilityError::Store(format!(
                "failed to preserve Settings at {}: {error}",
                path.display()
            ))
        })?;
        let mut raw_caps: RawSettingsObject = match root.get("enabled_capabilities") {
            None => RawSettingsObject::new(),
            Some(raw) if raw.get() == "null" => RawSettingsObject::new(),
            Some(raw) => raw_object(raw).map_err(|error| {
                SearchCapabilityError::Store(format!(
                    "invalid enabled_capabilities at {}: {error}",
                    path.display()
                ))
            })?,
        };
        let mut provider_caps = snapshot
            .capabilities
            .as_ref()
            .and_then(|map| map.get(&request.provider))
            .cloned()
            .unwrap_or_else(|| request.inherited_enabled_siblings.clone());
        provider_caps.retain(|cap| cap != "search");
        if request.desired {
            provider_caps.push("search".to_owned());
        }
        raw_caps.insert(
            request.provider.clone(),
            raw_value(&provider_caps)
                .map_err(|error| SearchCapabilityError::Store(error.to_string()))?,
        );
        root.insert(
            "enabled_capabilities".into(),
            raw_value(&raw_caps)
                .map_err(|error| SearchCapabilityError::Store(error.to_string()))?,
        );
        let candidate = serde_json::to_vec(&root).map_err(|error| {
            SearchCapabilityError::Store(format!("failed to serialize Search capability: {error}"))
        })?;
        let candidate_settings = parse_effective_settings(path, &candidate)
            .map_err(|error| SearchCapabilityError::Store(error.to_string()))?;
        let snapshot = search_capability_snapshot(&candidate_settings, &request.provider);
        if snapshot.search_enabled != request.desired {
            return Err(SearchCapabilityError::Store(
                "Search capability did not match the requested value before saving".into(),
            ));
        }
        save_json_atomic(path, &root).map_err(|error| {
            SearchCapabilityError::Store(format!("failed to save Search capability: {error}"))
        })?;
        Ok(SearchCapabilitySaveAcknowledgement {
            snapshot,
            changed: true,
        })
    }

    /// Acknowledge one GPUI-owned Boolean preference without
    /// reserializing any of the other Settings values. Both the typed
    /// baseline check and raw JSON patch share the canonical Settings lock.
    pub fn save_boolean_preference_checked(
        path: impl AsRef<Path>,
        request: SettingsBooleanSaveRequest,
    ) -> Result<SettingsBooleanSaveAcknowledgement, SettingsBooleanSaveError> {
        Self::save_boolean_preference_checked_inner(path.as_ref(), request, None)
    }

    pub(crate) fn save_boolean_preference_checked_cancellable(
        path: impl AsRef<Path>,
        request: SettingsBooleanSaveRequest,
        token: &SettingsBooleanSaveToken,
    ) -> Result<SettingsBooleanSaveAcknowledgement, SettingsBooleanSaveError> {
        if !token.mark_waiting_for_settings() {
            return Err(SettingsBooleanSaveError::Cancelled);
        }
        Self::save_boolean_preference_checked_inner(path.as_ref(), request, Some(token))
    }

    fn save_boolean_preference_checked_inner(
        path: &Path,
        request: SettingsBooleanSaveRequest,
        token: Option<&SettingsBooleanSaveToken>,
    ) -> Result<SettingsBooleanSaveAcknowledgement, SettingsBooleanSaveError> {
        let _transaction = transaction_guard();
        if token.is_some_and(SettingsBooleanSaveToken::is_canceled) {
            return Err(SettingsBooleanSaveError::Cancelled);
        }
        let source_bytes = read_exact_bytes(path)
            .map_err(|error| SettingsBooleanSaveError::Store(error.to_string()))?;
        let source_settings = parse_effective_settings(path, &source_bytes)?;
        let actual = request.preference.get(&source_settings);

        // A second writer may already have committed the requested value.
        // That state is an acknowledgement, and does not require a rewrite.
        if actual == request.desired {
            if token.is_some_and(SettingsBooleanSaveToken::is_canceled) {
                return Err(SettingsBooleanSaveError::Cancelled);
            }
            return Ok(SettingsBooleanSaveAcknowledgement {
                preference: request.preference,
                value: actual,
                persisted: false,
            });
        }
        if actual != request.expected_baseline {
            return Err(SettingsBooleanSaveError::Conflict {
                preference: request.preference,
                expected: request.expected_baseline,
                actual,
            });
        }

        let raw_source = if source_bytes.iter().all(u8::is_ascii_whitespace) {
            // A missing or empty store starts with the full effective defaults.
            // A sparse JSON object can have different deserialization defaults.
            serde_json::to_vec(&source_settings).map_err(|error| {
                SettingsBooleanSaveError::Store(format!(
                    "failed to serialize default Settings for {}: {error}",
                    path.display()
                ))
            })?
        } else {
            source_bytes
        };
        let mut raw_settings =
            serde_json::from_slice::<BTreeMap<String, Box<serde_json::value::RawValue>>>(
                &raw_source,
            )
            .map_err(|error| {
                SettingsBooleanSaveError::Store(format!(
                    "failed to preserve raw Settings values from {}: {error}",
                    path.display()
                ))
            })?;
        let raw_value = serde_json::value::RawValue::from_string(request.desired.to_string())
            .map_err(|error| SettingsBooleanSaveError::Store(error.to_string()))?;
        raw_settings.insert(request.preference.key().to_owned(), raw_value);
        if token.is_some_and(|token| !token.begin_irrevocable_commit()) {
            return Err(SettingsBooleanSaveError::Cancelled);
        }
        save_json_atomic(path, &raw_settings)
            .map_err(|error| SettingsBooleanSaveError::Store(error.to_string()))?;

        // Read the committed typed value back before returning an
        // acknowledgement to the editor.
        let committed_bytes = std::fs::read(path).map_err(|error| {
            SettingsBooleanSaveError::Store(format!(
                "failed to verify Settings write at {}: {error}",
                path.display()
            ))
        })?;
        let committed = parse_effective_settings(path, &committed_bytes)?;
        let committed_value = request.preference.get(&committed);
        if committed_value != request.desired {
            return Err(SettingsBooleanSaveError::Store(format!(
                "{} did not match the requested Settings value after writing",
                request.preference.label()
            )));
        }
        Ok(SettingsBooleanSaveAcknowledgement {
            preference: request.preference,
            value: committed_value,
            persisted: true,
        })
    }

    /// Update settings only when the raw bytes still match the caller's
    /// inspected SHA-256. The exact serialized bytes are atomically committed
    /// and read back before their digest is returned.
    pub fn update_checked(
        path: impl AsRef<Path>,
        expected_sha256: &str,
        mutate: impl FnOnce(&mut Self) -> anyhow::Result<()>,
    ) -> anyhow::Result<(Self, String)> {
        let _transaction = transaction_guard();
        let path = path.as_ref();
        let source_bytes = read_exact_bytes(path)?;
        let source_sha256 = crate::radial::package::sha256_hex(&source_bytes);
        anyhow::ensure!(
            source_sha256 == expected_sha256,
            "settings changed since the checked update was prepared"
        );
        let mut settings = if source_bytes.iter().all(u8::is_ascii_whitespace) {
            Self::default()
        } else {
            serde_json::from_slice(&source_bytes)
                .map_err(|error| anyhow::anyhow!("failed to parse settings: {error}"))?
        };
        mutate(&mut settings)?;
        let bytes = serde_json::to_vec_pretty(&settings)
            .map_err(|error| anyhow::anyhow!("failed to serialize settings: {error}"))?;
        crate::common::atomic_file::save_atomic(path, &bytes)
            .map_err(|error| anyhow::anyhow!("failed to atomically save settings: {error}"))?;
        let saved = std::fs::read(path)
            .map_err(|error| anyhow::anyhow!("failed to verify saved settings: {error}"))?;
        let saved_sha256 = crate::radial::package::sha256_hex(&saved);
        anyhow::ensure!(
            saved == bytes,
            "settings changed while verifying the checked update"
        );
        Ok((settings, saved_sha256))
    }
}

fn parse_effective_settings(
    path: &Path,
    bytes: &[u8],
) -> Result<Settings, SettingsBooleanSaveError> {
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return Ok(Settings::default());
    }
    serde_json::from_slice(bytes).map_err(|error| {
        SettingsBooleanSaveError::Store(format!(
            "failed to parse Settings from {}: {error}",
            path.display()
        ))
    })
}

pub(super) fn raw_value<T: serde::Serialize + ?Sized>(
    value: &T,
) -> anyhow::Result<Box<serde_json::value::RawValue>> {
    let value = serde_json::to_string(value).context("failed to serialize owned Settings value")?;
    serde_json::value::RawValue::from_string(value)
        .context("failed to retain serialized Settings value as raw JSON")
}

pub(super) fn raw_object(raw: &serde_json::value::RawValue) -> anyhow::Result<RawSettingsObject> {
    serde_json::from_str(raw.get()).context("expected a JSON object")
}

fn raw_object_with_replacements<T: serde::Serialize + ?Sized>(
    existing: Option<&serde_json::value::RawValue>,
    replacements: &T,
) -> anyhow::Result<Box<serde_json::value::RawValue>> {
    raw_object_with_raw_replacements(existing, raw_value(replacements)?.as_ref())
}

fn raw_object_with_raw_replacements(
    existing: Option<&serde_json::value::RawValue>,
    replacements: &serde_json::value::RawValue,
) -> anyhow::Result<Box<serde_json::value::RawValue>> {
    let mut object = existing
        .and_then(|raw| serde_json::from_str::<RawSettingsObject>(raw.get()).ok())
        .unwrap_or_default();
    let replacement_object = raw_object(replacements)?;
    object.extend(replacement_object);
    raw_value(&object)
}

fn patch_file_search_metadata_preferences(
    existing: Option<&serde_json::value::RawValue>,
    replacements: &serde_json::value::RawValue,
) -> anyhow::Result<Box<serde_json::value::RawValue>> {
    let mut metadata = existing
        .and_then(|raw| serde_json::from_str::<RawSettingsObject>(raw.get()).ok())
        .unwrap_or_default();
    let mut replacement_metadata = raw_object(replacements)?;
    let mut nested_replacements = Vec::new();
    for key in ["date", "size"] {
        if let Some(replacement) = replacement_metadata.remove(key) {
            let nested = raw_object_with_raw_replacements(
                metadata.get(key).map(Box::as_ref),
                replacement.as_ref(),
            )?;
            nested_replacements.push((key.to_owned(), nested));
        }
    }
    metadata.extend(replacement_metadata);
    metadata.extend(nested_replacements);
    raw_value(&metadata)
}

fn patch_file_search_ui_preferences_raw(
    existing: Option<&serde_json::value::RawValue>,
    ui_preferences: &crate::file_search::settings::FileSearchUiPreferences,
) -> anyhow::Result<Box<serde_json::value::RawValue>> {
    let mut replacement_ui = raw_object(raw_value(ui_preferences)?.as_ref())?;
    let mut ui = existing
        .and_then(|raw| serde_json::from_str::<RawSettingsObject>(raw.get()).ok())
        .unwrap_or_default();

    let merged_metadata = if let Some(metadata_replacement) = replacement_ui.remove("metadata") {
        let metadata = patch_file_search_metadata_preferences(
            ui.get("metadata").map(Box::as_ref),
            metadata_replacement.as_ref(),
        )?;
        Some(metadata)
    } else {
        None
    };
    ui.extend(replacement_ui);
    if let Some(metadata) = merged_metadata {
        ui.insert("metadata".to_owned(), metadata);
    }
    raw_value(&ui)
}

fn patch_file_search_ui_preferences(
    existing: Option<&serde_json::value::RawValue>,
    source_value: Option<&serde_json::Value>,
    ui_preferences: &crate::file_search::settings::FileSearchUiPreferences,
) -> anyhow::Result<Box<serde_json::value::RawValue>> {
    let Some(existing) = existing else {
        // Preserve sparse existing Settings objects. FileSearch defaults are
        // supplied by its serde(default) type.
        let mut file_search = RawSettingsObject::new();
        file_search.insert("ui_preferences".to_owned(), raw_value(ui_preferences)?);
        return raw_value(&file_search);
    };

    let existing_object = serde_json::from_str::<RawSettingsObject>(existing.get()).ok();
    // Match the previous `Settings.plugin_settings` Value deserialization:
    // duplicate object members use the last value, and serde-compatible
    // positional structs remain valid inputs.
    let typed_settings = source_value.and_then(|value| {
        serde_json::from_value::<crate::file_search::settings::FileSearchSettings>(value.clone())
            .ok()
    });
    if let Some(mut typed_settings) = typed_settings {
        if let Some(mut file_search) = existing_object {
            let ui = patch_file_search_ui_preferences_raw(
                file_search.get("ui_preferences").map(Box::as_ref),
                ui_preferences,
            )?;
            file_search.insert("ui_preferences".to_owned(), ui);
            return raw_value(&file_search);
        }

        // The previous typed `Value` path also accepted serde's positional
        // struct representation. It has no object siblings to retain, so
        // materialize the parsed backend fields and replace only its UI state.
        typed_settings.ui_preferences = ui_preferences.clone();
        return raw_value(&typed_settings);
    }

    // Match the previous typed fallback for an invalid legacy File Search
    // value, while retaining unknown raw siblings at both nested levels.
    let defaults = crate::file_search::settings::FileSearchSettings {
        ui_preferences: ui_preferences.clone(),
        ..Default::default()
    };
    let default_fields = raw_object(raw_value(&defaults)?.as_ref())?;
    let mut file_search = existing_object.unwrap_or_default();
    // FileSearchSettings accepts this legacy alias only while deserializing.
    // Once defaults are written under the canonical key, retaining both would
    // make serde reject the saved object as a duplicate field.
    file_search.remove("global_content_search_roots");
    for (key, value) in default_fields {
        if key == "ui_preferences" {
            let ui = patch_file_search_ui_preferences_raw(
                file_search.get(&key).map(Box::as_ref),
                ui_preferences,
            )?;
            file_search.insert(key, ui);
        } else {
            file_search.insert(key, value);
        }
    }
    raw_value(&file_search)
}

/// SHA-256 of the exact persisted settings bytes. Missing files hash as empty
/// bytes, matching the radial authoring store's empty-source convention.
pub fn settings_file_sha256(path: impl AsRef<Path>) -> anyhow::Result<String> {
    Ok(crate::radial::package::sha256_hex(&read_exact_bytes(
        path.as_ref(),
    )?))
}

fn read_exact_bytes(path: &Path) -> anyhow::Result<Vec<u8>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(anyhow::Error::new(error).context(format!(
            "failed to read settings bytes from {}",
            path.display()
        ))),
    }
}

fn transaction_guard() -> MutexGuard<'static, ()> {
    SETTINGS_TRANSACTION
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn load_effective(path: impl AsRef<Path>) -> anyhow::Result<Settings> {
    match Settings::load_typed(path)? {
        LoadState::Missing | LoadState::Empty => Ok(Settings::default()),
        LoadState::Loaded(settings) => Ok(settings),
    }
}

#[cfg(test)]
pub(super) fn legacy_positional_settings(settings: &Settings) -> Vec<u8> {
    use serde::de::{Deserialize, Deserializer, MapAccess, Visitor};

    struct Positional(Vec<Box<serde_json::value::RawValue>>);
    impl<'de> Deserialize<'de> for Positional {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            struct Fields;
            impl<'de> Visitor<'de> for Fields {
                type Value = Positional;

                fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                    formatter.write_str("a serialized Settings object")
                }

                fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                    let mut values = Vec::new();
                    // Model an older positional file ending at the pins.
                    let mut finished = false;
                    while let Some((key, value)) = map.next_entry::<String, _>()? {
                        if !finished {
                            values.push(value);
                            finished = key == "pinned_panels";
                        }
                    }
                    Ok(Positional(values))
                }
            }
            deserializer.deserialize_map(Fields)
        }
    }

    let object = serde_json::to_vec(settings).unwrap();
    let Positional(values) = serde_json::from_slice(&object).unwrap();
    serde_json::to_vec(&values).unwrap()
}

#[cfg(test)]
mod tests {
    use super::{
        LauncherClosePreferences, SettingsBooleanPreference,
        SettingsBooleanSaveAcknowledgement, SettingsBooleanSaveError,
        SettingsBooleanSaveRequest, SettingsBooleanSaveToken, Settings,
        settings_file_sha256, transaction_guard,
    };
    use crate::common::persistence::{LoadState, PersistenceError};
    use std::collections::BTreeMap;
    use std::sync::{Arc, Barrier};
    use std::time::{Duration, Instant};

    #[test]
    fn compatibility_load_defaults_only_missing_and_empty() {
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("missing.json");
        assert_eq!(
            Settings::load(missing.to_str().unwrap()).unwrap().hotkey,
            Settings::default().hotkey
        );

        let empty = directory.path().join("empty.json");
        std::fs::write(&empty, " \r\n\t").unwrap();
        assert_eq!(
            Settings::load(empty.to_str().unwrap()).unwrap().hotkey,
            Settings::default().hotkey
        );

        let malformed = directory.path().join("malformed.json");
        std::fs::write(&malformed, "{broken").unwrap();
        assert!(Settings::load(malformed.to_str().unwrap()).is_err());
    }

    #[test]
    fn typed_load_preserves_missing_empty_valid_and_malformed() {
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("missing.json");
        assert!(matches!(
            Settings::load_typed(&missing).unwrap(),
            LoadState::Missing
        ));

        let empty = directory.path().join("empty.json");
        std::fs::write(&empty, "\n\t").unwrap();
        assert!(matches!(
            Settings::load_typed(&empty).unwrap(),
            LoadState::Empty
        ));

        let valid = directory.path().join("valid.json");
        Settings::default().save(valid.to_str().unwrap()).unwrap();
        assert!(matches!(
            Settings::load_typed(&valid).unwrap(),
            LoadState::Loaded(_)
        ));

        let malformed = directory.path().join("malformed.json");
        std::fs::write(&malformed, "not json").unwrap();
        assert!(matches!(
            Settings::load_typed(&malformed).unwrap_err(),
            PersistenceError::MalformedJson { .. }
        ));
    }

    #[test]
    fn unreadable_existing_path_is_not_missing() {
        let directory = tempfile::tempdir().unwrap();
        assert!(matches!(
            Settings::load_typed(directory.path()).unwrap_err(),
            PersistenceError::Read { .. }
        ));
    }

    #[test]
    fn save_preserves_the_existing_pretty_json_contract() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let mut settings = Settings::default();
        settings.show_examples = true;
        let expected = serde_json::to_string_pretty(&settings).unwrap();

        settings.save(path.to_str().unwrap()).unwrap();

        assert_eq!(std::fs::read_to_string(path).unwrap(), expected);
    }

    #[test]
    fn concurrent_updates_preserve_both_logical_changes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        Settings::default().save(path.to_str().unwrap()).unwrap();
        let path = Arc::new(path.to_string_lossy().into_owned());
        let barrier = Arc::new(Barrier::new(3));

        let first = {
            let path = Arc::clone(&path);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                Settings::update(&path, |settings| {
                    settings.show_examples = true;
                    Ok(())
                })
                .unwrap();
            })
        };
        let second = {
            let path = Arc::clone(&path);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                Settings::update(&path, |settings| {
                    settings.enable_toasts = false;
                    Ok(())
                })
                .unwrap();
            })
        };

        barrier.wait();
        first.join().unwrap();
        second.join().unwrap();
        let committed = Settings::load(&path).unwrap();
        assert!(committed.show_examples);
        assert!(!committed.enable_toasts);
    }

    #[test]
    fn failed_update_returns_no_value_to_publish() {
        let directory = tempfile::tempdir().unwrap();
        let blocker = directory.path().join("not-a-directory");
        std::fs::write(&blocker, "block parent creation").unwrap();
        let path = blocker.join("settings.json");

        let result = Settings::update(path.to_str().unwrap(), |settings| {
            settings.show_examples = true;
            Ok(())
        });

        assert!(result.is_err());
        assert_eq!(
            std::fs::read_to_string(blocker).unwrap(),
            "block parent creation"
        );
    }

    #[test]
    fn checked_update_requires_exact_source_sha_and_returns_verified_target_sha() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        Settings::default().save(path.to_str().unwrap()).unwrap();
        let source_sha = settings_file_sha256(&path).unwrap();

        let (updated, target_sha) = Settings::update_checked(&path, &source_sha, |settings| {
            settings.show_examples = true;
            Ok(())
        })
        .unwrap();

        assert!(updated.show_examples);
        assert_eq!(settings_file_sha256(&path).unwrap(), target_sha);
        let before_conflict = std::fs::read(&path).unwrap();
        assert!(
            Settings::update_checked(&path, &source_sha, |settings| {
                settings.enable_toasts = false;
                Ok(())
            })
            .is_err()
        );
        assert_eq!(std::fs::read(&path).unwrap(), before_conflict);
    }

    #[test]
    fn checked_update_mutator_and_parse_failures_write_nothing() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        std::fs::write(&path, b"{broken").unwrap();
        let source = std::fs::read(&path).unwrap();
        let source_sha = crate::radial::package::sha256_hex(&source);
        assert!(Settings::update_checked(&path, &source_sha, |_| Ok(())).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), source);

        std::fs::write(&path, b"{}").unwrap();
        let source = std::fs::read(&path).unwrap();
        let source_sha = crate::radial::package::sha256_hex(&source);
        assert!(
            Settings::update_checked(&path, &source_sha, |_| { anyhow::bail!("no mutation") })
                .is_err()
        );
        assert_eq!(std::fs::read(&path).unwrap(), source);
    }

    #[test]
    fn launcher_close_patch_preserves_raw_top_level_and_nested_tokens() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let opaque = r#"{ "large": 90071992547409931234567890123456789, "nested": {"keep": [1, 2]} }"#;
        let mut source = String::from(
            r#"{"window_size":[800,600],"pinned_panels":["HelpWindow"],"future_top":"#,
        );
        source.push_str(opaque);
        source.push_str(
            r#","plugin_settings":{"file_search":{"global_content_search_roots":["legacy-root"],"max_search_results":11,"max_search_results":17,"future_search":"#,
        );
        source.push_str(opaque);
        source.push_str(r#","ui_preferences":{"whole_word":false,"future_ui":"#);
        source.push_str(opaque);
        source.push_str(r#", "metadata":{"future_metadata":"#);
        source.push_str(opaque);
        source.push_str(r#", "date":{"future_date":"#);
        source.push_str(opaque);
        source.push_str(r#"}, "size":{"future_size":"#);
        source.push_str(opaque);
        source.push_str(r#"}}}},"opaque_plugin":"#);
        source.push_str(opaque);
        source.push_str(
            r#"},"calculation_pad_display":{"number_mode":"Fixed","word_wrap":false,"future_display":"#,
        );
        source.push_str(opaque);
        source.push_str("}}");
        std::fs::write(&path, source).unwrap();

        let mut ui_preferences = crate::file_search::settings::FileSearchUiPreferences::default();
        ui_preferences.whole_word = true;
        ui_preferences.filename_sort =
            crate::file_search::settings::FileSearchFilenameSort::FilenameDescending;
        let display = crate::settings::CalculationPadDisplayPreferences {
            word_wrap: true,
            number_mode: crate::settings::CalculationPadNumberMode::Scientific,
            ..Default::default()
        };
        let preferences = LauncherClosePreferences {
            window_size: (900, 650),
            pinned_panels: vec![crate::gui::Panel::FileSearchDialog],
            file_search_ui_preferences: Some(ui_preferences),
            calculation_pad_display: Some(display),
        };

        Settings::save_launcher_close_preferences(&path, &preferences).unwrap();
        let saved = Settings::read_current(&path).unwrap();
        assert_eq!(saved.window_size, Some((900, 650)));
        assert_eq!(
            saved.pinned_panels,
            vec![crate::gui::Panel::FileSearchDialog]
        );
        assert!(saved.calculation_pad_display.word_wrap);
        assert_eq!(
            saved.calculation_pad_display.number_mode,
            crate::settings::CalculationPadNumberMode::Scientific
        );
        let typed_file_search: crate::file_search::settings::FileSearchSettings =
            serde_json::from_value(saved.plugin_settings["file_search"].clone()).unwrap();
        assert_eq!(typed_file_search.max_search_results, 17);

        let raw = serde_json::from_slice::<BTreeMap<
            String,
            Box<serde_json::value::RawValue>,
        >>(&std::fs::read(&path).unwrap())
        .unwrap();
        assert_eq!(raw["future_top"].get(), opaque);
        let plugins = super::raw_object(raw["plugin_settings"].as_ref()).unwrap();
        assert_eq!(plugins["opaque_plugin"].get(), opaque);
        let file_search = super::raw_object(plugins["file_search"].as_ref()).unwrap();
        assert_eq!(file_search["future_search"].get(), opaque);
        assert_eq!(file_search["max_search_results"].get(), "17");
        assert_eq!(
            file_search["global_content_search_roots"].get(),
            r#"["legacy-root"]"#,
            "the legacy File Search root spelling remains intact"
        );
        let ui = super::raw_object(file_search["ui_preferences"].as_ref()).unwrap();
        assert_eq!(ui["future_ui"].get(), opaque);
        assert_eq!(ui["whole_word"].get(), "true");
        assert_eq!(ui["filename_sort"].get(), r#""filename_descending""#);
        let metadata = super::raw_object(ui["metadata"].as_ref()).unwrap();
        assert_eq!(metadata["future_metadata"].get(), opaque);
        assert_eq!(
            super::raw_object(metadata["date"].as_ref()).unwrap()["future_date"].get(),
            opaque
        );
        assert_eq!(
            super::raw_object(metadata["size"].as_ref()).unwrap()["future_size"].get(),
            opaque
        );
        let display = super::raw_object(raw["calculation_pad_display"].as_ref()).unwrap();
        assert_eq!(display["future_display"].get(), opaque);
        assert_eq!(display["number_mode"].get(), r#""Scientific""#);
        assert_eq!(display["word_wrap"].get(), "true");
    }

    #[test]
    fn launcher_close_patch_keeps_typed_file_search_fallback_and_unknown_siblings() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let opaque = r#"{ "large": 90071992547409931234567890123456789 }"#;
        let mut source = String::from(
            r#"{"plugin_settings":{"file_search":{"global_content_search_roots":["legacy-invalid-root"],"max_search_results":"invalid","future_search":"#,
        );
        source.push_str(opaque);
        source.push_str(r#","ui_preferences":{"whole_word":false,"future_ui":"#);
        source.push_str(opaque);
        source.push_str("}}}}");
        std::fs::write(&path, source).unwrap();

        let ui_preferences = crate::file_search::settings::FileSearchUiPreferences {
            whole_word: true,
            ..Default::default()
        };
        let preferences = LauncherClosePreferences {
            window_size: (700, 500),
            pinned_panels: Vec::new(),
            file_search_ui_preferences: Some(ui_preferences),
            calculation_pad_display: None,
        };
        Settings::save_launcher_close_preferences(&path, &preferences).unwrap();

        let raw = serde_json::from_slice::<BTreeMap<
            String,
            Box<serde_json::value::RawValue>,
        >>(&std::fs::read(&path).unwrap())
        .unwrap();
        let plugins = super::raw_object(raw["plugin_settings"].as_ref()).unwrap();
        let file_search = super::raw_object(plugins["file_search"].as_ref()).unwrap();
        assert!(!file_search.contains_key("global_content_search_roots"));
        let expected_roots =
            crate::file_search::settings::FileSearchSettings::default().global_search_roots;
        assert_eq!(
            file_search["global_search_roots"].get(),
            serde_json::to_string(&expected_roots).unwrap()
        );
        assert_eq!(file_search["max_search_results"].get(), "500");
        assert_eq!(file_search["future_search"].get(), opaque);
        let ui = super::raw_object(file_search["ui_preferences"].as_ref()).unwrap();
        assert_eq!(ui["whole_word"].get(), "true");
        assert_eq!(ui["future_ui"].get(), opaque);
        let saved = Settings::read_current(&path).unwrap();
        let file_search: crate::file_search::settings::FileSearchSettings =
            serde_json::from_value(saved.plugin_settings["file_search"].clone()).unwrap();
        assert_eq!(file_search.global_search_roots, expected_roots);
    }

    #[test]
    fn launcher_close_patch_materializes_legacy_file_search_sequence() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let legacy_file_search = r#"[["legacy-root"],["keep-excluded"],17]"#;
        let source = format!(
            r#"{{"plugin_settings":{{"file_search":{legacy_file_search}}}}}"#
        );
        std::fs::write(&path, source).unwrap();

        let legacy_value: serde_json::Value =
            serde_json::from_str(legacy_file_search).unwrap();
        let mut expected: crate::file_search::settings::FileSearchSettings =
            serde_json::from_value(legacy_value).unwrap();
        let ui_preferences = crate::file_search::settings::FileSearchUiPreferences {
            whole_word: true,
            ..Default::default()
        };
        expected.ui_preferences = ui_preferences.clone();
        let preferences = LauncherClosePreferences {
            window_size: (700, 500),
            pinned_panels: Vec::new(),
            file_search_ui_preferences: Some(ui_preferences),
            calculation_pad_display: None,
        };

        Settings::save_launcher_close_preferences(&path, &preferences).unwrap();

        let saved = Settings::read_current(&path).unwrap();
        let actual: crate::file_search::settings::FileSearchSettings =
            serde_json::from_value(saved.plugin_settings["file_search"].clone()).unwrap();
        assert_eq!(actual, expected);
        assert_eq!(
            actual.global_search_roots,
            vec![std::path::PathBuf::from("legacy-root")]
        );
        assert_eq!(actual.excluded_directory_names, vec!["keep-excluded"]);
        assert_eq!(actual.max_search_results, 17);

        let raw = serde_json::from_slice::<super::RawSettingsObject>(
            &std::fs::read(&path).unwrap(),
        )
        .unwrap();
        let plugins = super::raw_object(raw["plugin_settings"].as_ref()).unwrap();
        let file_search = super::raw_object(plugins["file_search"].as_ref()).unwrap();
        assert_eq!(file_search["global_search_roots"].get(), r#"["legacy-root"]"#);
        assert_eq!(
            file_search["excluded_directory_names"].get(),
            r#"["keep-excluded"]"#
        );
        assert_eq!(file_search["max_search_results"].get(), "17");
    }

    #[test]
    fn launcher_close_patch_keeps_sparse_objects_and_defaults_empty_stores() {
        let directory = tempfile::tempdir().unwrap();
        let sparse = directory.path().join("sparse.json");
        let opaque = r#"{ "future": 90071992547409931234567890123456789 }"#;
        std::fs::write(&sparse, format!(r#"{{"opaque":{opaque}}}"#)).unwrap();
        let preferences = LauncherClosePreferences {
            window_size: (777, 555),
            pinned_panels: vec![crate::gui::Panel::HelpWindow],
            file_search_ui_preferences: None,
            calculation_pad_display: None,
        };
        Settings::save_launcher_close_preferences(&sparse, &preferences).unwrap();
        let saved = Settings::read_current(&sparse).unwrap();
        assert_eq!(saved.window_size, Some((777, 555)));
        assert_eq!(saved.pinned_panels, vec![crate::gui::Panel::HelpWindow]);
        let sparse_raw = serde_json::from_slice::<BTreeMap<
            String,
            Box<serde_json::value::RawValue>,
        >>(&std::fs::read(&sparse).unwrap())
        .unwrap();
        assert_eq!(sparse_raw.len(), 3);
        assert_eq!(sparse_raw["opaque"].get(), opaque);
        assert!(!sparse_raw.contains_key("plugin_settings"));
        assert!(!sparse_raw.contains_key("calculation_pad_display"));

        let sparse_feature = directory.path().join("sparse-feature.json");
        std::fs::write(
            &sparse_feature,
            format!(r#"{{"opaque":{opaque}}}"#),
        )
        .unwrap();
        let ui_preferences = crate::file_search::settings::FileSearchUiPreferences {
            whole_word: true,
            ..Default::default()
        };
        let feature_preferences = LauncherClosePreferences {
            file_search_ui_preferences: Some(ui_preferences),
            ..preferences.clone()
        };
        Settings::save_launcher_close_preferences(&sparse_feature, &feature_preferences).unwrap();
        let feature_raw = serde_json::from_slice::<BTreeMap<
            String,
            Box<serde_json::value::RawValue>,
        >>(&std::fs::read(&sparse_feature).unwrap())
        .unwrap();
        let feature_plugins = super::raw_object(feature_raw["plugin_settings"].as_ref()).unwrap();
        assert_eq!(feature_plugins.len(), 1);
        let file_search =
            super::raw_object(feature_plugins["file_search"].as_ref()).unwrap();
        assert_eq!(file_search.len(), 1);
        assert!(file_search.contains_key("ui_preferences"));

        let missing = directory.path().join("missing.json");
        let empty = directory.path().join("empty.json");
        std::fs::write(&empty, b" \r\n\t").unwrap();
        for path in [&missing, &empty] {
            Settings::save_launcher_close_preferences(path, &preferences).unwrap();
            let saved = Settings::read_current(path).unwrap();
            assert_eq!(saved.hotkey, Settings::default().hotkey);
            assert_eq!(saved.help_hotkey, Settings::default().help_hotkey);
            assert_eq!(saved.offscreen_pos, Settings::default().offscreen_pos);
            let raw = serde_json::from_slice::<BTreeMap<
                String,
                Box<serde_json::value::RawValue>,
            >>(&std::fs::read(path).unwrap())
            .unwrap();
            assert!(raw.contains_key("hotkey"));
            assert!(raw.contains_key("radial"));
            assert_eq!(raw["window_size"].get(), "[777,555]");
        }
    }

    #[test]
    fn launcher_close_rejects_malformed_settings_without_replacing_them() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let malformed = b"{broken";
        std::fs::write(&path, malformed).unwrap();
        let preferences = LauncherClosePreferences {
            window_size: (777, 555),
            pinned_panels: Vec::new(),
            file_search_ui_preferences: None,
            calculation_pad_display: None,
        };

        assert!(Settings::save_launcher_close_preferences(&path, &preferences).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), malformed);
    }

    #[test]
    fn unpin_panel_checked_preserves_latest_order_and_raw_siblings() {
        use crate::gui::Panel;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let opaque = r#"{ "large": 90071992547409931234567890123456789, "nested": [1, 2] }"#;
        let latest = format!(
            r#"{{"pinned_panels":["HelpWindow","FavDialog","FileSearchDialog","FavDialog"],"future":{opaque}}}"#
        );
        std::fs::write(&path, latest).unwrap();

        Settings::unpin_panel_checked(&path, Panel::FavDialog).unwrap();

        let bytes = std::fs::read(&path).unwrap();
        let raw: super::RawSettingsObject = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(raw["future"].get(), opaque);
        assert_eq!(
            Settings::read_current(&path).unwrap().pinned_panels,
            vec![Panel::HelpWindow, Panel::FileSearchDialog]
        );
    }

    #[test]
    fn unpin_panel_checked_acknowledges_absence_and_rejects_bad_source() {
        use crate::gui::Panel;
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("missing.json");
        Settings::unpin_panel_checked(&missing, Panel::FavDialog).unwrap();
        assert!(!missing.exists());
        let path = directory.path().join("settings.json");
        for source in [
            b" \r\n".as_slice(),
            br#"{ "pinned_panels": ["HelpWindow"] }"#.as_slice(),
        ] {
            std::fs::write(&path, source).unwrap();
            Settings::unpin_panel_checked(&path, Panel::FavDialog).unwrap();
            assert_eq!(std::fs::read(&path).unwrap(), source);
        }
        let malformed = br#"{ "pinned_panels": [42] }"#;
        std::fs::write(&path, malformed).unwrap();
        assert!(Settings::unpin_panel_checked(&path, Panel::FavDialog).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), malformed);
        assert!(Settings::unpin_panel_checked(directory.path(), Panel::FavDialog).is_err());
    }

    #[test]
    fn unpin_panel_checked_accepts_positional_settings() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let mut settings = Settings::default();
        settings.pinned_panels = vec![crate::gui::Panel::FavDialog, crate::gui::Panel::HelpWindow];
        std::fs::write(&path, super::legacy_positional_settings(&settings)).unwrap();
        Settings::unpin_panel_checked(&path, crate::gui::Panel::FavDialog).unwrap();
        assert_eq!(
            Settings::read_current(&path).unwrap().pinned_panels,
            vec![crate::gui::Panel::HelpWindow]
        );
    }

    #[cfg(windows)]
    #[test]
    fn unpin_panel_checked_reports_atomic_write_failure() {
        use std::os::windows::fs::OpenOptionsExt;
        use windows::Win32::Storage::FileSystem::FILE_SHARE_READ;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let source = br#"{"pinned_panels":["FavDialog"]}"#;
        std::fs::write(&path, source).unwrap();
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ.0)
            .open(&path)
            .unwrap();
        assert!(Settings::unpin_panel_checked(&path, crate::gui::Panel::FavDialog).is_err());
        drop(lock);
        assert_eq!(std::fs::read(&path).unwrap(), source);
    }

    #[test]
    fn search_capability_sparse_defaults_and_noop_preserve_bytes() {
        use super::SearchCapabilitySaveRequest;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let initial = Settings::read_search_capability(&path, "folders").unwrap();
        assert!(initial.search_enabled);
        assert!(initial.capabilities.is_none());
        let source = br#"{ "enabled_plugins": [], "opaque": 1234567890123456789012345 }"#;
        std::fs::write(&path, source).unwrap();
        let no_op = Settings::save_search_capability_checked(
            &path,
            SearchCapabilitySaveRequest {
                provider: "folders".into(),
                inherited_enabled_siblings: Vec::new(),
                expected_baseline: true,
                desired: true,
            },
        )
        .unwrap();
        assert!(!no_op.changed);
        assert_eq!(std::fs::read(&path).unwrap(), source);
        let saved = Settings::save_search_capability_checked(
            &path,
            SearchCapabilitySaveRequest {
                provider: "folders".into(),
                inherited_enabled_siblings: Vec::new(),
                expected_baseline: true,
                desired: false,
            },
        )
        .unwrap();
        assert!(saved.changed);
        assert_eq!(
            saved.snapshot.capabilities.unwrap()["folders"],
            Vec::<String>::new()
        );
        assert!(!saved.snapshot.search_enabled);
        let value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(value["enabled_plugins"], serde_json::json!([]));
    }

    #[test]
    fn search_capability_preserves_latest_siblings_and_other_raw_values() {
        use super::{RawSettingsObject, SearchCapabilitySaveRequest};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let source = r#"{"enabled_plugins":["other"],"enabled_capabilities":{"folders":["show_full_path","search","future","search"],"other" : [ "opaque", "search" ]},"opaque":{"huge":1234567890123456789012345}}"#;
        std::fs::write(&path, source).unwrap();
        let saved = Settings::save_search_capability_checked(
            &path,
            SearchCapabilitySaveRequest {
                provider: "folders".into(),
                inherited_enabled_siblings: Vec::new(),
                expected_baseline: true,
                desired: false,
            },
        )
        .unwrap();
        assert_eq!(
            saved.snapshot.capabilities.as_ref().unwrap()["folders"],
            ["show_full_path", "future"]
        );
        let bytes = std::fs::read(&path).unwrap();
        let root: RawSettingsObject = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(root["enabled_plugins"].get(), "[\"other\"]");
        assert_eq!(
            root["opaque"].get(),
            r#"{"huge":1234567890123456789012345}"#
        );
        let caps: RawSettingsObject =
            serde_json::from_str(root["enabled_capabilities"].get()).unwrap();
        assert_eq!(caps["other"].get(), r#"[ "opaque", "search" ]"#);
        let saved = Settings::save_search_capability_checked(
            &path,
            SearchCapabilitySaveRequest {
                provider: "folders".into(),
                inherited_enabled_siblings: Vec::new(),
                expected_baseline: false,
                desired: true,
            },
        )
        .unwrap();
        assert_eq!(
            saved.snapshot.capabilities.unwrap()["folders"],
            ["show_full_path", "future", "search"]
        );
    }

    #[test]
    fn search_capability_materializes_only_inherited_enabled_siblings() {
        use super::SearchCapabilitySaveRequest;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, br#"{"enabled_capabilities":{}}"#).unwrap();
        let saved = Settings::save_search_capability_checked(
            &path,
            SearchCapabilitySaveRequest {
                provider: "timer".into(),
                inherited_enabled_siblings: vec!["completion_dialog".into(), "search".into()],
                expected_baseline: true,
                desired: false,
            },
        ).unwrap();
        assert_eq!(saved.snapshot.capabilities.unwrap()["timer"], ["completion_dialog"]);
        let saved = Settings::save_search_capability_checked(
            &path,
            SearchCapabilitySaveRequest {
                provider: "timer".into(),
                inherited_enabled_siblings: vec!["unrelated".into()],
                expected_baseline: false,
                desired: true,
            },
        ).unwrap();
        assert_eq!(saved.snapshot.capabilities.unwrap()["timer"], ["completion_dialog", "search"]);
    }

    #[test]
    fn search_capability_rejects_stale_and_malformed_store_without_writes() {
        use super::{SearchCapabilityError, SearchCapabilitySaveRequest};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let source = br#"{"enabled_capabilities":{"web_search":[]}}"#;
        std::fs::write(&path, source).unwrap();
        let result = Settings::save_search_capability_checked(
            &path,
            SearchCapabilitySaveRequest {
                provider: "web_search".into(),
                inherited_enabled_siblings: Vec::new(),
                expected_baseline: true,
                desired: false,
            },
        );
        assert!(matches!(
            result,
            Err(SearchCapabilityError::Conflict { actual: false, .. })
        ));
        assert_eq!(std::fs::read(&path).unwrap(), source);
        for malformed in [r#"{"enabled_capabilities":{"web_search":[4]}}"#, "{broken"] {
            std::fs::write(&path, malformed).unwrap();
            assert!(Settings::read_search_capability(&path, "web_search").is_err());
            assert!(
                Settings::save_search_capability_checked(
                    &path,
                    SearchCapabilitySaveRequest {
                        provider: "web_search".into(),
                        inherited_enabled_siblings: Vec::new(),
                        expected_baseline: true,
                        desired: false,
                    }
                )
                .is_err()
            );
            assert_eq!(std::fs::read(&path).unwrap(), malformed.as_bytes());
        }
    }

    #[test]
    fn query_autocomplete_write_preserves_raw_unowned_json_values() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let opaque =
            r#"{ "signed": -9223372036854775809123, "nested": [9007199254740993123456789] }"#;
        let source = format!("{{\"query_autocomplete\":true,\"opaque_extension\":{opaque}}}");
        std::fs::write(&path, source).unwrap();

        let preference = SettingsBooleanPreference::QueryAutocomplete;
        let acknowledgement = Settings::save_boolean_preference_checked(
            &path,
            SettingsBooleanSaveRequest::new(preference, true, false),
        )
        .unwrap();

        assert_eq!(
            acknowledgement,
            SettingsBooleanSaveAcknowledgement {
                preference,
                value: false,
                persisted: true,
            }
        );
        let bytes = std::fs::read(&path).unwrap();
        let raw = serde_json::from_slice::<BTreeMap<String, Box<serde_json::value::RawValue>>>(
            &bytes,
        )
        .unwrap();
        assert_eq!(raw["opaque_extension"].get(), opaque);
        assert!(!Settings::read_current(&path).unwrap().query_autocomplete);
    }

    #[test]
    fn query_autocomplete_acknowledges_desired_value_without_rewriting() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let source = br#"{ "query_autocomplete": false, "opaque": 1234567890123456789012345 }"#;
        std::fs::write(&path, source).unwrap();

        let preference = SettingsBooleanPreference::QueryAutocomplete;
        let acknowledgement = Settings::save_boolean_preference_checked(
            &path,
            SettingsBooleanSaveRequest::new(preference, true, false),
        )
        .unwrap();

        assert_eq!(
            acknowledgement,
            SettingsBooleanSaveAcknowledgement {
                preference,
                value: false,
                persisted: false,
            }
        );
        assert_eq!(std::fs::read(&path).unwrap(), source);
    }

    #[test]
    fn query_autocomplete_write_uses_existing_missing_and_empty_defaults() {
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("missing.json");
        let empty = directory.path().join("empty.json");
        std::fs::write(&empty, b" \r\n\t").unwrap();
        let mut expected = Settings::default();
        let baseline = expected.query_autocomplete;
        let desired = !baseline;
        expected.query_autocomplete = desired;

        for path in [&missing, &empty] {
            let acknowledgement = Settings::save_boolean_preference_checked(
                path,
                SettingsBooleanSaveRequest::new(
                    SettingsBooleanPreference::QueryAutocomplete,
                    baseline,
                    desired,
                ),
            )
            .unwrap();
            assert_eq!(acknowledgement.value(), desired);
            assert!(acknowledgement.persisted());
            let reloaded = Settings::read_current(path).unwrap();
            assert_eq!(reloaded.query_autocomplete, desired);
            assert_eq!(reloaded.help_hotkey, expected.help_hotkey);
            assert_eq!(reloaded.offscreen_pos, expected.offscreen_pos);
        }

        let sparse = directory.path().join("sparse.json");
        std::fs::write(&sparse, b"{}").unwrap();
        Settings::save_boolean_preference_checked(
            &sparse,
            SettingsBooleanSaveRequest::new(
                SettingsBooleanPreference::QueryAutocomplete,
                baseline,
                desired,
            ),
        )
        .unwrap();
        let stored: serde_json::Value =
            serde_json::from_slice(&std::fs::read(sparse).unwrap()).unwrap();
        assert_eq!(stored, serde_json::json!({ "query_autocomplete": desired }));
    }

    #[test]
    fn show_examples_write_preserves_defaults_and_unowned_raw_values() {
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("missing.json");
        let empty = directory.path().join("empty.json");
        std::fs::write(&empty, b" \r\n\t").unwrap();
        let preference = SettingsBooleanPreference::ShowExamples;
        for path in [&missing, &empty] {
            let acknowledgement = Settings::save_boolean_preference_checked(
                path,
                SettingsBooleanSaveRequest::new(preference, false, true),
            )
            .unwrap();
            assert_eq!(acknowledgement.preference(), preference);
            assert!(acknowledgement.value());
            assert!(acknowledgement.persisted());
            let saved = Settings::read_current(path).unwrap();
            assert!(saved.show_examples);
            assert_eq!(saved.help_hotkey, Settings::default().help_hotkey);
            assert_eq!(saved.offscreen_pos, Settings::default().offscreen_pos);
        }

        let opaque = r#"{ "integer": 9007199254740993123456789, "nested": [true, null] }"#;
        let path = directory.path().join("opaque.json");
        std::fs::write(
            &path,
            format!(r#"{{"show_examples":false,"opaque_extension":{opaque}}}"#),
        )
        .unwrap();
        Settings::save_boolean_preference_checked(
            &path,
            SettingsBooleanSaveRequest::new(preference, false, true),
        )
        .unwrap();
        let raw = serde_json::from_slice::<BTreeMap<String, Box<serde_json::value::RawValue>>>(
            &std::fs::read(path).unwrap(),
        )
        .unwrap();
        assert_eq!(raw["opaque_extension"].get(), opaque);
    }

    #[test]
    fn concurrent_boolean_preference_saves_preserve_each_other() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let opaque = r#"{ "signed": -9223372036854775809123, "nested": [9007199254740993123456789] }"#;
        std::fs::write(
            &path,
            format!(
                r#"{{"query_autocomplete":true,"show_examples":false,"opaque_extension":{opaque}}}"#
            ),
        )
        .unwrap();
        let workers = crate::SettingsSaveWorkerRegistry::default();
        let (query_rx, _) = workers
            .begin_boolean_preference_save(
                path.to_string_lossy().into_owned(),
                SettingsBooleanSaveRequest::new(
                    SettingsBooleanPreference::QueryAutocomplete,
                    true,
                    false,
                ),
            )
            .unwrap();
        let (examples_rx, _) = workers
            .begin_boolean_preference_save(
                path.to_string_lossy().into_owned(),
                SettingsBooleanSaveRequest::new(
                    SettingsBooleanPreference::ShowExamples,
                    false,
                    true,
                ),
            )
            .unwrap();

        let query = query_rx
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap();
        let examples = examples_rx
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap();
        assert_eq!(query.preference(), SettingsBooleanPreference::QueryAutocomplete);
        assert!(!query.value());
        assert_eq!(examples.preference(), SettingsBooleanPreference::ShowExamples);
        assert!(examples.value());
        assert!(workers.join_all_checked().is_clean());

        let saved = Settings::read_current(&path).unwrap();
        assert!(!saved.query_autocomplete);
        assert!(saved.show_examples);
        let raw = serde_json::from_slice::<BTreeMap<String, Box<serde_json::value::RawValue>>>(
            &std::fs::read(&path).unwrap(),
        )
        .unwrap();
        assert_eq!(raw["opaque_extension"].get(), opaque);
    }

    #[test]
    fn pending_query_autocomplete_save_cancels_while_waiting_for_settings_lock() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let source = br#"{ "query_autocomplete": true, "opaque": 1234567890123456789012345 }"#;
        std::fs::write(&path, source).unwrap();
        let settings_lock = transaction_guard();
        let workers = crate::SettingsSaveWorkerRegistry::default();
        let (receipt_rx, token) = workers
            .begin_boolean_preference_save(
                path.to_string_lossy().into_owned(),
                SettingsBooleanSaveRequest::new(
                    SettingsBooleanPreference::QueryAutocomplete,
                    true,
                    false,
                ),
            )
            .unwrap();

        let deadline = Instant::now() + Duration::from_secs(5);
        while !token.is_waiting_for_settings() {
            assert!(Instant::now() < deadline, "writer did not wait on Settings lock");
            std::thread::yield_now();
        }
        assert!(token.request_cancel());
        drop(settings_lock);

        assert_eq!(
            receipt_rx.recv_timeout(Duration::from_secs(5)).unwrap(),
            Err(SettingsBooleanSaveError::Cancelled)
        );
        let shutdown = workers.join_all_checked();
        assert!(shutdown.is_clean(), "{:?}", shutdown.failure_summary());
        assert_eq!(std::fs::read(&path).unwrap(), source);
    }

    #[test]
    fn query_autocomplete_save_token_rejects_cancel_after_commit_claim() {
        let token = SettingsBooleanSaveToken::new(1);
        assert!(token.begin_irrevocable_commit());
        assert!(!token.request_cancel());
    }

    #[test]
    fn checked_shutdown_reports_unacknowledged_query_autocomplete_write_failure() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let source = b"{broken";
        std::fs::write(&path, source).unwrap();
        let workers = crate::SettingsSaveWorkerRegistry::default();
        let (receipt_rx, _token) = workers
            .begin_boolean_preference_save(
                path.to_string_lossy().into_owned(),
                SettingsBooleanSaveRequest::new(
                    SettingsBooleanPreference::QueryAutocomplete,
                    true,
                    false,
                ),
            )
            .unwrap();
        assert!(matches!(
            receipt_rx.recv_timeout(Duration::from_secs(5)).unwrap(),
            Err(SettingsBooleanSaveError::Store(_))
        ));

        let shutdown = workers.join_all_checked();
        let failure = shutdown.failure_summary().unwrap();
        assert!(failure.contains("Settings Boolean save request 1"));
        assert!(failure.contains("failed to parse Settings"));
        assert_eq!(std::fs::read(&path).unwrap(), source);
    }

    #[test]
    fn query_autocomplete_conflict_and_malformed_input_preserve_source_bytes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let conflicting = br#"{ "query_autocomplete": false, "opaque": 123 }"#;
        std::fs::write(&path, conflicting).unwrap();
        assert_eq!(
            Settings::save_boolean_preference_checked(
                &path,
                SettingsBooleanSaveRequest::new(
                    SettingsBooleanPreference::QueryAutocomplete,
                    true,
                    true,
                ),
            )
            .unwrap_err(),
            SettingsBooleanSaveError::Conflict {
                preference: SettingsBooleanPreference::QueryAutocomplete,
                expected: true,
                actual: false,
            }
        );
        assert_eq!(std::fs::read(&path).unwrap(), conflicting);

        let malformed = b"{broken";
        std::fs::write(&path, malformed).unwrap();
        assert!(matches!(
            Settings::save_boolean_preference_checked(
                &path,
                SettingsBooleanSaveRequest::new(
                    SettingsBooleanPreference::QueryAutocomplete,
                    true,
                    false,
                ),
            ),
            Err(SettingsBooleanSaveError::Store(_))
        ));
        assert_eq!(std::fs::read(&path).unwrap(), malformed);
    }
}
