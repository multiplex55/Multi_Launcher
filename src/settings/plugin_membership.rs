//! Checked persistence for the currently supported stateless plugin membership edits.
use super::*;
use crate::plugin::BuiltinIdentity;
use std::collections::HashSet;
use std::io::Write;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatelessPlugin {
    DateArithmetic,
    BaseConvert,
    UnitConvert,
    Google,
    Wikipedia,
    Youtube,
    Reddit,
    Runescape,
    Weather,
}

impl StatelessPlugin {
    pub fn id(self) -> &'static str {
        match self {
            Self::DateArithmetic => {
                <crate::plugins::date_arithmetic::DateArithmeticPlugin as BuiltinIdentity>::ID
            }
            Self::BaseConvert => {
                <crate::plugins::base_convert::BaseConvertPlugin as BuiltinIdentity>::ID
            }
            Self::UnitConvert => {
                <crate::plugins::unit_convert::UnitConvertPlugin as BuiltinIdentity>::ID
            }
            Self::Google => <crate::plugins_builtin::WebSearchPlugin as BuiltinIdentity>::ID,
            Self::Wikipedia => <crate::plugins::wikipedia::WikipediaPlugin as BuiltinIdentity>::ID,
            Self::Youtube => <crate::plugins::youtube::YoutubePlugin as BuiltinIdentity>::ID,
            Self::Reddit => <crate::plugins::reddit::RedditPlugin as BuiltinIdentity>::ID,
            Self::Runescape => <crate::plugins::runescape::RunescapeSearchPlugin as BuiltinIdentity>::ID,
            Self::Weather => <crate::plugins::weather::WeatherPlugin as BuiltinIdentity>::ID,
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        [Self::DateArithmetic, Self::BaseConvert, Self::UnitConvert, Self::Google,
         Self::Wikipedia, Self::Youtube, Self::Reddit, Self::Runescape, Self::Weather]
            .into_iter().find(|plugin| plugin.id() == id)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginMembershipSnapshot {
    pub enabled: Option<HashSet<String>>,
    pub plugin_dirs: Option<Vec<String>>,
    path: std::path::PathBuf,
    raw: Option<String>,
    positional: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PluginMembershipRequest {
    pub plugin: StatelessPlugin,
    pub enabled: bool,
    /// The user reviewed conversion of implicit all-enabled membership to an explicit set.
    pub reviewed_all_to_explicit: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PluginMembershipError {
    ReviewRequired,
    PluginDirectoriesConfigured,
    InvalidRequest,
    Conflict,
    Canceled,
    Store(String),
    CommittedUnverified(String),
}
impl std::fmt::Display for PluginMembershipError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReviewRequired => {
                f.write_str("Review conversion to an explicit plugin list before saving")
            }
            Self::PluginDirectoriesConfigured => {
                f.write_str("Plugin directories are configured; membership cannot be edited here")
            }
            Self::InvalidRequest => {
                f.write_str("Plugin membership is already in the requested state")
            }
            Self::Conflict => {
                f.write_str("Plugin membership or Settings changed; reload before saving")
            }
            Self::Canceled => f.write_str("Plugin membership save was canceled"),
            Self::Store(error) => f.write_str(error),
            Self::CommittedUnverified(error) => write!(
                f,
                "Plugin membership was written but could not be verified: {error}"
            ),
        }
    }
}
impl std::error::Error for PluginMembershipError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginMembershipAcknowledgement {
    pub snapshot: PluginMembershipSnapshot,
}

impl PluginMembershipSnapshot {
    /// Preview the exact new set without reading or writing Settings.
    pub fn propose(
        &self,
        request: PluginMembershipRequest,
    ) -> Result<HashSet<String>, PluginMembershipError> {
        if self
            .plugin_dirs
            .as_ref()
            .is_some_and(|dirs| !dirs.is_empty())
        {
            return Err(PluginMembershipError::PluginDirectoriesConfigured);
        }
        let id = request.plugin.id();
        let mut enabled = match &self.enabled {
            Some(enabled) => enabled.clone(),
            None if request.enabled => return Err(PluginMembershipError::InvalidRequest),
            None if !request.reviewed_all_to_explicit => {
                return Err(PluginMembershipError::ReviewRequired);
            }
            None => crate::plugin::builtin_plugin_ids()
                .iter()
                .map(|id| (*id).to_owned())
                .collect(),
        };
        let changed = if request.enabled {
            enabled.insert(id.to_owned())
        } else {
            enabled.remove(id)
        };
        if !changed {
            return Err(PluginMembershipError::InvalidRequest);
        }
        Ok(enabled)
    }
}

enum Root {
    Object(RawSettingsObject),
    Positional(Vec<Box<serde_json::value::RawValue>>),
}

fn source(path: &Path) -> std::io::Result<Option<Vec<u8>>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}
fn root(bytes: Option<&[u8]>) -> anyhow::Result<Root> {
    match bytes {
        None => Ok(Root::Object(RawSettingsObject::new())),
        Some(bytes) if bytes.iter().all(u8::is_ascii_whitespace) => {
            Ok(Root::Object(RawSettingsObject::new()))
        }
        Some(bytes)
            if bytes
                .iter()
                .copied()
                .find(|byte| !byte.is_ascii_whitespace())
                == Some(b'[') =>
        {
            Ok(Root::Positional(serde_json::from_slice(bytes)?))
        }
        Some(bytes) => Ok(Root::Object(super::omni_preferences::unique_object(
            std::str::from_utf8(bytes)?,
        )?)),
    }
}
fn inspect(
    path: &Path,
    bytes: Option<&[u8]>,
    root: &Root,
) -> anyhow::Result<(
    PluginMembershipSnapshot,
    usize,
    Vec<(String, Box<serde_json::value::RawValue>)>,
)> {
    let settings = parse_effective_settings(path, bytes.unwrap_or_default())?;
    let fields = super::omni_preferences::ordered_fields(&settings)?;
    let position = fields
        .iter()
        .position(|(name, _)| name == "enabled_plugins")
        .ok_or_else(|| anyhow::anyhow!("Settings schema has no plugin membership"))?;
    let raw = match root {
        Root::Object(root) => root.get("enabled_plugins"),
        Root::Positional(root) => root.get(position),
    }
    .map(|value| value.get().to_owned());
    Ok((
        PluginMembershipSnapshot {
            enabled: settings.enabled_plugins,
            plugin_dirs: settings.plugin_dirs,
            path: path.to_owned(),
            raw,
            positional: matches!(root, Root::Positional(_)),
        },
        position,
        fields,
    ))
}

impl Settings {
    pub fn plugin_membership_snapshot(
        path: impl AsRef<Path>,
    ) -> Result<PluginMembershipSnapshot, PluginMembershipError> {
        let path = path.as_ref();
        let _guard = transaction_guard();
        let bytes =
            source(path).map_err(|error| PluginMembershipError::Store(error.to_string()))?;
        let parsed = root(bytes.as_deref())
            .map_err(|error| PluginMembershipError::Store(error.to_string()))?;
        inspect(path, bytes.as_deref(), &parsed)
            .map(|(snapshot, _, _)| snapshot)
            .map_err(|error| PluginMembershipError::Store(error.to_string()))
    }

    pub fn save_plugin_membership_checked(
        path: impl AsRef<Path>,
        baseline: &PluginMembershipSnapshot,
        request: PluginMembershipRequest,
        current: impl Fn() -> bool,
    ) -> Result<PluginMembershipAcknowledgement, PluginMembershipError> {
        save_inner(path.as_ref(), baseline, request, None, current, source)
    }

    pub(crate) fn save_plugin_membership_cancellable(
        path: &Path,
        baseline: &PluginMembershipSnapshot,
        request: PluginMembershipRequest,
        token: &SettingsBooleanSaveToken,
        current: impl Fn() -> bool,
    ) -> Result<PluginMembershipAcknowledgement, PluginMembershipError> {
        if !token.mark_waiting_for_settings() {
            return Err(PluginMembershipError::Canceled);
        }
        save_inner(path, baseline, request, Some(token), current, source)
    }
}

fn save_inner(
    path: &Path,
    baseline: &PluginMembershipSnapshot,
    request: PluginMembershipRequest,
    token: Option<&SettingsBooleanSaveToken>,
    current: impl Fn() -> bool,
    readback: impl FnOnce(&Path) -> std::io::Result<Option<Vec<u8>>>,
) -> Result<PluginMembershipAcknowledgement, PluginMembershipError> {
    let _guard = transaction_guard();
    if token.is_some_and(SettingsBooleanSaveToken::is_canceled) || !current() {
        return Err(PluginMembershipError::Canceled);
    }
    let bytes = source(path).map_err(|error| PluginMembershipError::Store(error.to_string()))?;
    let mut parsed =
        root(bytes.as_deref()).map_err(|error| PluginMembershipError::Store(error.to_string()))?;
    let (actual, position, mut fields) = inspect(path, bytes.as_deref(), &parsed)
        .map_err(|error| PluginMembershipError::Store(error.to_string()))?;
    if actual != *baseline {
        return Err(PluginMembershipError::Conflict);
    }
    let desired = actual.propose(request)?;
    let mut sorted: Vec<_> = desired.iter().cloned().collect();
    sorted.sort();
    let raw = serde_json::value::to_raw_value(&sorted)
        .map_err(|error| PluginMembershipError::Store(error.to_string()))?;
    match &mut parsed {
        Root::Object(root) => {
            root.insert("enabled_plugins".into(), raw);
        }
        Root::Positional(root) => {
            while root.len() <= position {
                let (_, value) = &mut fields[root.len()];
                root.push(value.clone());
            }
            root[position] = raw;
        }
    }
    let candidate = match &parsed {
        Root::Object(root) => serde_json::to_vec(root),
        Root::Positional(root) => serde_json::to_vec(root),
    }
    .map_err(|error| PluginMembershipError::Store(error.to_string()))?;
    let (expected, _, _) = inspect(path, Some(&candidate), &parsed)
        .map_err(|error| PluginMembershipError::Store(error.to_string()))?;
    if expected.enabled.as_ref() != Some(&desired) {
        return Err(PluginMembershipError::Store(
            "Plugin membership candidate did not round trip".into(),
        ));
    }
    let claimed = std::cell::Cell::new(false);
    let changed = std::cell::Cell::new(false);
    let authority_lost = std::cell::Cell::new(false);
    let final_check = || {
        if source(path).ok().as_ref() != Some(&bytes) {
            changed.set(true);
            return false;
        }
        if !current() || token.is_some_and(SettingsBooleanSaveToken::is_canceled) {
            authority_lost.set(true);
            return false;
        }
        if !claimed.get() && token.is_some_and(|token| !token.begin_irrevocable_commit()) {
            authority_lost.set(true);
            return false;
        }
        claimed.set(true);
        if !current() || token.is_some_and(SettingsBooleanSaveToken::is_canceled) {
            authority_lost.set(true);
            return false;
        }
        if source(path).ok().as_ref() != Some(&bytes) {
            changed.set(true);
            return false;
        }
        true
    };
    let result: anyhow::Result<()> = if bytes.is_some() {
        crate::common::atomic_file::save_atomic_if(path, &candidate, final_check)
    } else {
        (|| {
            let parent = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."));
            let mut staged = tempfile::NamedTempFile::new_in(parent)?;
            staged.write_all(&candidate)?;
            staged.flush()?;
            staged.as_file().sync_all()?;
            anyhow::ensure!(final_check(), "Plugin membership save superseded");
            staged
                .persist_noclobber(path)
                .map_err(|error| error.error)?;
            Ok(())
        })()
    };
    if let Err(error) = result {
        if authority_lost.get() {
            return Err(PluginMembershipError::Canceled);
        }
        if changed.get() || source(path).ok().as_ref() != Some(&bytes) {
            return Err(PluginMembershipError::Conflict);
        }
        if !current() || token.is_some_and(SettingsBooleanSaveToken::is_canceled) {
            return Err(PluginMembershipError::Canceled);
        }
        return Err(PluginMembershipError::Store(error.to_string()));
    }
    let committed = readback(path)
        .map_err(|error| PluginMembershipError::CommittedUnverified(error.to_string()))?
        .ok_or_else(|| PluginMembershipError::CommittedUnverified("Settings disappeared".into()))?;
    if committed != candidate {
        return Err(PluginMembershipError::CommittedUnverified(
            "Settings changed after installation".into(),
        ));
    }
    let reread_root = root(Some(&committed))
        .map_err(|error| PluginMembershipError::CommittedUnverified(error.to_string()))?;
    let (readback, _, _) = inspect(path, Some(&committed), &reread_root)
        .map_err(|error| PluginMembershipError::CommittedUnverified(error.to_string()))?;
    if readback != expected {
        return Err(PluginMembershipError::CommittedUnverified(
            "membership receipt differs".into(),
        ));
    }
    Ok(PluginMembershipAcknowledgement { snapshot: readback })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disable(plugin: StatelessPlugin, reviewed: bool) -> PluginMembershipRequest {
        PluginMembershipRequest {
            plugin,
            enabled: false,
            reviewed_all_to_explicit: reviewed,
        }
    }

    #[test]
    fn reviewed_implicit_conversion_uses_full_catalog_and_rejects_noops() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let baseline = Settings::plugin_membership_snapshot(&path).unwrap();
        assert_eq!(
            baseline.propose(disable(StatelessPlugin::DateArithmetic, false)),
            Err(PluginMembershipError::ReviewRequired)
        );
        let proposed = baseline
            .propose(disable(StatelessPlugin::DateArithmetic, true))
            .unwrap();
        assert_eq!(
            proposed.len(),
            crate::plugin::builtin_plugin_ids().len() - 1
        );
        assert!(!proposed.contains(StatelessPlugin::DateArithmetic.id()));
        assert!(proposed.contains(StatelessPlugin::UnitConvert.id()));
        assert_eq!(
            baseline.propose(PluginMembershipRequest {
                plugin: StatelessPlugin::UnitConvert,
                enabled: true,
                reviewed_all_to_explicit: true
            }),
            Err(PluginMembershipError::InvalidRequest)
        );
        assert!(!path.exists());
    }

    #[test]
    fn explicit_membership_preserves_unknown_siblings_and_refuses_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let original = br#"{"enabled_plugins":["date_arithmetic","custom_plugin"],"opaque":{"a":1,"a":2},"plugin_settings":{"x":{"n":999999999999999999999999}}}"#;
        std::fs::write(&path, original).unwrap();
        let before = Settings::plugin_membership_snapshot(&path).unwrap();
        let ack = Settings::save_plugin_membership_checked(
            &path,
            &before,
            disable(StatelessPlugin::DateArithmetic, false),
            || true,
        )
        .unwrap();
        assert_eq!(
            ack.snapshot.enabled,
            Some(HashSet::from(["custom_plugin".into()]))
        );
        let saved = std::fs::read_to_string(&path).unwrap();
        assert!(saved.contains(r#"{"a":1,"a":2}"#));
        assert!(saved.contains("999999999999999999999999"));
        assert_eq!(
            Settings::save_plugin_membership_checked(
                &path,
                &before,
                disable(StatelessPlugin::DateArithmetic, false),
                || true
            ),
            Err(PluginMembershipError::Conflict)
        );
        std::fs::write(
            &path,
            br#"{"plugin_dirs":["plugins"],"enabled_plugins":[]}"#,
        )
        .unwrap();
        let blocked = Settings::plugin_membership_snapshot(&path).unwrap();
        assert_eq!(
            blocked.propose(PluginMembershipRequest {
                plugin: StatelessPlugin::BaseConvert,
                enabled: true,
                reviewed_all_to_explicit: false
            }),
            Err(PluginMembershipError::PluginDirectoriesConfigured)
        );
    }

    #[test]
    fn membership_no_clobber_cancel_and_readback_are_distinct() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let baseline = Settings::plugin_membership_snapshot(&path).unwrap();
        let token = SettingsBooleanSaveToken::new(7);
        assert!(token.request_cancel());
        assert_eq!(
            Settings::save_plugin_membership_cancellable(
                &path,
                &baseline,
                disable(StatelessPlugin::BaseConvert, true),
                &token,
                || true
            ),
            Err(PluginMembershipError::Canceled)
        );
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let result = Settings::save_plugin_membership_checked(
            &path,
            &baseline,
            disable(StatelessPlugin::BaseConvert, true),
            || {
                if calls.fetch_add(1, Ordering::SeqCst) == 2 {
                    std::fs::write(&path, b"{}").unwrap();
                }
                true
            },
        );
        assert_eq!(result, Err(PluginMembershipError::Conflict));
        assert_eq!(std::fs::read(&path).unwrap(), b"{}");
        let current = Settings::plugin_membership_snapshot(&path).unwrap();
        let result = save_inner(
            &path,
            &current,
            disable(StatelessPlugin::UnitConvert, true),
            None,
            || true,
            |_| Err(std::io::Error::other("readback failed")),
        );
        assert!(matches!(
            result,
            Err(PluginMembershipError::CommittedUnverified(_))
        ));
        assert!(
            !Settings::plugin_membership_snapshot(&path)
                .unwrap()
                .enabled
                .unwrap()
                .contains(StatelessPlugin::UnitConvert.id())
        );
        std::fs::write(&path, b"{broken").unwrap();
        assert!(Settings::plugin_membership_snapshot(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"{broken");
    }

    #[test]
    fn positional_settings_keep_all_unowned_fields() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let fields = super::super::omni_preferences::ordered_fields(&Settings::default()).unwrap();
        let position = fields
            .iter()
            .position(|(name, _)| name == "enabled_plugins")
            .unwrap();
        let raw: Vec<_> = fields.into_iter().map(|(_, raw)| raw).collect();
        std::fs::write(&path, serde_json::to_vec(&raw).unwrap()).unwrap();
        let baseline = Settings::plugin_membership_snapshot(&path).unwrap();
        Settings::save_plugin_membership_checked(
            &path,
            &baseline,
            disable(StatelessPlugin::DateArithmetic, true),
            || true,
        )
        .unwrap();
        let saved: Vec<Box<serde_json::value::RawValue>> =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved.len(), raw.len());
        for index in 0..raw.len() {
            if index != position {
                assert_eq!(saved[index].get(), raw[index].get());
            }
        }
    }

    #[test]
    fn membership_last_source_check_preserves_late_and_unrelated_changes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(
            &path,
            br#"{"enabled_plugins":["date_arithmetic"],"opaque_note":"old"}"#,
        )
        .unwrap();
        let baseline = Settings::plugin_membership_snapshot(&path).unwrap();
        std::fs::write(
            &path,
            br#"{"enabled_plugins":["date_arithmetic"],"opaque_note":"new"}"#,
        )
        .unwrap();
        Settings::save_plugin_membership_checked(
            &path,
            &baseline,
            disable(StatelessPlugin::DateArithmetic, false),
            || true,
        )
        .unwrap();
        let saved: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved["opaque_note"], "new");
        let baseline = Settings::plugin_membership_snapshot(&path).unwrap();
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let competitor = br#"{"enabled_plugins":[],"opaque_note":"competitor"}"#;
        let result = Settings::save_plugin_membership_checked(
            &path,
            &baseline,
            PluginMembershipRequest {
                plugin: StatelessPlugin::BaseConvert,
                enabled: true,
                reviewed_all_to_explicit: false,
            },
            || {
                if calls.fetch_add(1, Ordering::SeqCst) == 2 {
                    std::fs::write(&path, competitor).unwrap();
                }
                true
            },
        );
        assert_eq!(result, Err(PluginMembershipError::Conflict));
        assert_eq!(std::fs::read(&path).unwrap(), competitor);
        let other = dir.path().join("other.json");
        std::fs::write(&other, competitor).unwrap();
        assert_eq!(
            Settings::save_plugin_membership_checked(
                &other,
                &baseline,
                PluginMembershipRequest {
                    plugin: StatelessPlugin::BaseConvert,
                    enabled: true,
                    reviewed_all_to_explicit: false
                },
                || true
            ),
            Err(PluginMembershipError::Conflict)
        );
    }
}
