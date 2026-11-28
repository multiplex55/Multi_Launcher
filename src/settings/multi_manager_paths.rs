//! Checked Settings transaction for MultiManager's two source paths.
use super::*;
use std::io::Write;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MultiManagerPaths {
    pub workspaces: String,
    pub bindings: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MultiManagerPathsSnapshot {
    pub paths: MultiManagerPaths,
    pub auto_reconnect_on_load: bool,
    path: PathBuf,
    owned: [Option<String>; 3],
    positional: bool,
    group_positional: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MultiManagerPathsAcknowledgement {
    pub snapshot: MultiManagerPathsSnapshot,
    pub persisted: bool,
}
#[derive(Debug)]
pub enum MultiManagerPathsError {
    Conflict,
    SourceChanged,
    Canceled,
    Store(String),
    CommittedUnverified(String),
}
impl std::fmt::Display for MultiManagerPathsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Conflict => {
                f.write_str("MultiManager paths or Settings changed; reload before saving")
            }
            Self::SourceChanged => {
                f.write_str("MultiManager source changed during validation; retry saving")
            }
            Self::Canceled => f.write_str("MultiManager paths save was canceled"),
            Self::Store(e) => f.write_str(e),
            Self::CommittedUnverified(e) => write!(
                f,
                "MultiManager paths were written but could not be verified: {e}"
            ),
        }
    }
}
impl std::error::Error for MultiManagerPathsError {}

enum Root {
    Object(RawSettingsObject),
    Positional(Vec<Box<serde_json::value::RawValue>>),
}
fn source(path: &Path) -> std::io::Result<Option<Vec<u8>>> {
    match std::fs::read(path) {
        Ok(v) => Ok(Some(v)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}
fn resolved_identity_path(path: &Path) -> anyhow::Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut suffix = Vec::new();
    let mut ancestor = absolute.as_path();
    while !ancestor.exists() {
        suffix.push(
            ancestor
                .file_name()
                .ok_or_else(|| anyhow::anyhow!("Cannot resolve source path"))?
                .to_owned(),
        );
        ancestor = ancestor
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Cannot resolve source parent"))?;
    }
    let mut resolved = std::fs::canonicalize(ancestor)?;
    for component in suffix.into_iter().rev() {
        resolved.push(component);
    }
    Ok(resolved)
}
fn aliases(a: &Path, b: &Path) -> anyhow::Result<bool> {
    let a = resolved_identity_path(a)?;
    let b = resolved_identity_path(b)?;
    #[cfg(windows)]
    if windows_path_equal(&a, &b) {
        return Ok(true);
    }
    #[cfg(not(windows))]
    if a == b {
        return Ok(true);
    }
    if let (Ok(left), Ok(right)) = (std::fs::File::open(a), std::fs::File::open(b)) {
        #[cfg(windows)]
        {
            return Ok(crate::file_operations::native_file_identity(&left)?
                == crate::file_operations::native_file_identity(&right)?);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let left = left.metadata()?;
            let right = right.metadata()?;
            return Ok(left.dev() == right.dev() && left.ino() == right.ino());
        }
    }
    Ok(false)
}
#[cfg(windows)]
fn windows_path_equal(a: &Path, b: &Path) -> bool {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Foundation::BOOL;
    use windows::Win32::Globalization::{CSTR_EQUAL, CompareStringOrdinal};
    let normalize = |path: &Path| {
        let mut result = Vec::new();
        for component in path.components() {
            let mut part: Vec<u16> = component.as_os_str().encode_wide().collect();
            while part
                .last()
                .is_some_and(|c| *c == b' ' as u16 || *c == b'.' as u16)
            {
                part.pop();
            }
            if !result.is_empty() {
                result.push(b'\\' as u16);
            }
            result.extend(part);
        }
        result
    };
    let left = normalize(a);
    let right = normalize(b);
    // SAFETY: the API only reads the supplied UTF-16 slices.
    unsafe { CompareStringOrdinal(&left, &right, BOOL(1)) == CSTR_EQUAL }
}
fn distinct_paths(settings: &Path, workspace: &Path, bindings: &Path) -> anyhow::Result<()> {
    anyhow::ensure!(
        !aliases(workspace, bindings)?
            && !aliases(settings, workspace)?
            && !aliases(settings, bindings)?,
        "Settings, workspaces and bindings must use distinct files"
    );
    Ok(())
}
fn root(bytes: Option<&[u8]>) -> anyhow::Result<Root> {
    match bytes {
        None => Ok(Root::Object(RawSettingsObject::new())),
        Some(b) if b.iter().all(u8::is_ascii_whitespace) => {
            Ok(Root::Object(RawSettingsObject::new()))
        }
        Some(b) if b.iter().copied().find(|x| !x.is_ascii_whitespace()) == Some(b'[') => {
            Ok(Root::Positional(serde_json::from_slice(b)?))
        }
        Some(b) => Ok(Root::Object(super::omni_preferences::unique_object(
            std::str::from_utf8(b)?,
        )?)),
    }
}
fn inspect(
    path: &Path,
    bytes: Option<&[u8]>,
    root: &Root,
) -> anyhow::Result<(
    MultiManagerPathsSnapshot,
    usize,
    Vec<(String, Box<serde_json::value::RawValue>)>,
)> {
    let settings = parse_effective_settings(path, bytes.unwrap_or_default())?;
    let fields = super::omni_preferences::ordered_fields(&settings)?;
    let position = fields
        .iter()
        .position(|(name, _)| name == "multi_manager")
        .ok_or_else(|| anyhow::anyhow!("Settings schema has no MultiManager settings"))?;
    let raw = match root {
        Root::Object(o) => o.get("multi_manager"),
        Root::Positional(a) => a.get(position),
    };
    let owned = match raw {
        None => [None, None, None],
        Some(raw) if raw.get().trim_start().starts_with('[') => {
            let a: Vec<Box<serde_json::value::RawValue>> = serde_json::from_str(raw.get())?;
            [
                a.get(1).map(|v| v.get().to_owned()),
                a.get(2).map(|v| v.get().to_owned()),
                a.get(8).map(|v| v.get().to_owned()),
            ]
        }
        Some(raw) => {
            let o = super::omni_preferences::unique_object(raw.get())?;
            [
                o.get("workspaces_path").map(|v| v.get().to_owned()),
                o.get("bindings_path").map(|v| v.get().to_owned()),
                o.get("auto_reconnect_on_load").map(|v| v.get().to_owned()),
            ]
        }
    };
    Ok((
        MultiManagerPathsSnapshot {
            paths: MultiManagerPaths {
                workspaces: settings.multi_manager.workspaces_path,
                bindings: settings.multi_manager.bindings_path,
            },
            auto_reconnect_on_load: settings.multi_manager.auto_reconnect_on_load,
            path: path.to_owned(),
            owned,
            positional: matches!(root, Root::Positional(_)),
            group_positional: raw.is_some_and(|v| v.get().trim_start().starts_with('[')),
        },
        position,
        fields,
    ))
}
impl Settings {
    pub fn multi_manager_paths_snapshot(
        path: impl AsRef<Path>,
    ) -> Result<MultiManagerPathsSnapshot, MultiManagerPathsError> {
        let path = path.as_ref();
        let _guard = transaction_guard();
        let bytes = source(path).map_err(|e| MultiManagerPathsError::Store(e.to_string()))?;
        let parsed =
            root(bytes.as_deref()).map_err(|e| MultiManagerPathsError::Store(e.to_string()))?;
        inspect(path, bytes.as_deref(), &parsed)
            .map(|v| v.0)
            .map_err(|e| MultiManagerPathsError::Store(e.to_string()))
    }
    pub fn save_multi_manager_paths_checked(
        path: impl AsRef<Path>,
        baseline: &MultiManagerPathsSnapshot,
        desired: &MultiManagerPaths,
        current: impl Fn() -> bool,
    ) -> Result<MultiManagerPathsAcknowledgement, MultiManagerPathsError> {
        save(path.as_ref(), baseline, desired, None, None, current)
    }
    pub fn save_multi_manager_settings_checked(
        path: impl AsRef<Path>,
        baseline: &MultiManagerPathsSnapshot,
        desired: &MultiManagerPaths,
        auto_reconnect_on_load: bool,
        current: impl Fn() -> bool,
    ) -> Result<MultiManagerPathsAcknowledgement, MultiManagerPathsError> {
        save(path.as_ref(), baseline, desired, Some(auto_reconnect_on_load), None, current)
    }
    pub(crate) fn save_multi_manager_paths_cancellable(
        path: &Path,
        baseline: &MultiManagerPathsSnapshot,
        desired: &MultiManagerPaths,
        auto_reconnect_on_load: Option<bool>,
        token: &SettingsBooleanSaveToken,
        current: impl Fn() -> bool,
    ) -> Result<MultiManagerPathsAcknowledgement, MultiManagerPathsError> {
        if !token.mark_waiting_for_settings() {
            return Err(MultiManagerPathsError::Canceled);
        }
        save(path, baseline, desired, auto_reconnect_on_load, Some(token), current)
    }
}
fn save(
    path: &Path,
    baseline: &MultiManagerPathsSnapshot,
    desired: &MultiManagerPaths,
    auto_reconnect_on_load: Option<bool>,
    token: Option<&SettingsBooleanSaveToken>,
    current: impl Fn() -> bool,
) -> Result<MultiManagerPathsAcknowledgement, MultiManagerPathsError> {
    let _guard = transaction_guard();
    let active = || current() && !token.is_some_and(SettingsBooleanSaveToken::is_canceled);
    if !active() {
        return Err(MultiManagerPathsError::Canceled);
    }
    let bytes = source(path).map_err(|e| MultiManagerPathsError::Store(e.to_string()))?;
    let mut parsed =
        root(bytes.as_deref()).map_err(|e| MultiManagerPathsError::Store(e.to_string()))?;
    let (actual, position, fields) = inspect(path, bytes.as_deref(), &parsed)
        .map_err(|e| MultiManagerPathsError::Store(e.to_string()))?;
    let paths_match = actual.path == baseline.path
        && actual.paths == baseline.paths
        && actual.owned[..2] == baseline.owned[..2]
        && actual.positional == baseline.positional
        && actual.group_positional == baseline.group_positional;
    if !paths_match || (auto_reconnect_on_load.is_some() && actual != *baseline) {
        return Err(MultiManagerPathsError::Conflict);
    }
    let auto_reconnect_on_load = auto_reconnect_on_load.unwrap_or(actual.auto_reconnect_on_load);
    let base = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let workspace = crate::multi_manager::state::resolve_relative_to(base, &desired.workspaces);
    let bindings = crate::multi_manager::state::resolve_relative_to(base, &desired.bindings);
    let paths_changed = actual.paths != *desired;
    let bool_changed = actual.auto_reconnect_on_load != auto_reconnect_on_load;
    let source_basis = if paths_changed || !bool_changed {
        if desired.workspaces.trim().is_empty() || desired.bindings.trim().is_empty() {
            return Err(MultiManagerPathsError::Store("Source paths cannot be empty".into()));
        }
        let workspace_permit = crate::common::repair_watch::require_path_read(&workspace)
            .map_err(|e| MultiManagerPathsError::Store(e.to_string()))?;
        let bindings_permit = crate::common::repair_watch::require_path_read(&bindings)
            .map_err(|e| MultiManagerPathsError::Store(e.to_string()))?;
        distinct_paths(path, &workspace, &bindings)
            .map_err(|e| MultiManagerPathsError::Store(e.to_string()))?;
        let (workspace_basis, binding_basis) =
            crate::multi_manager::checked::load_editor_source_basis(&workspace, &bindings, &active)
                .map_err(|e| if !active() { MultiManagerPathsError::Canceled }
                    else { MultiManagerPathsError::Store(format!("MultiManager source: {e:#}")) })?;
        Some((workspace_permit, bindings_permit, workspace_basis, binding_basis))
    } else {
        None
    };
    let sources_current = || {
        source_basis.as_ref().is_none_or(|(_, _, workspace_basis, binding_basis)| {
            crate::multi_manager::store::checked_workspace_source_is_current(workspace_basis)
                && source(&bindings).ok().as_ref() == Some(binding_basis)
        })
    };
    if !active() {
        return Err(MultiManagerPathsError::Canceled);
    }
    if source(path).ok().as_ref() != Some(&bytes) {
        return Err(MultiManagerPathsError::Conflict);
    }
    if !paths_changed && !bool_changed {
        if !active() {
            return Err(MultiManagerPathsError::Canceled);
        }
        if !sources_current() {
            return Err(MultiManagerPathsError::SourceChanged);
        }
        if !active() {
            return Err(MultiManagerPathsError::Canceled);
        }
        if source(path).ok().as_ref() != Some(&bytes) {
            return Err(MultiManagerPathsError::Conflict);
        }
        return Ok(MultiManagerPathsAcknowledgement {
            snapshot: actual,
            persisted: false,
        });
    }
    let old = match &parsed {
        Root::Object(o) => o.get("multi_manager"),
        Root::Positional(a) => a.get(position),
    };
    let workspace_raw = serde_json::value::to_raw_value(&desired.workspaces)
        .map_err(|e| MultiManagerPathsError::Store(e.to_string()))?;
    let bindings_raw = serde_json::value::to_raw_value(&desired.bindings)
        .map_err(|e| MultiManagerPathsError::Store(e.to_string()))?;
    let auto_raw = serde_json::value::to_raw_value(&auto_reconnect_on_load)
        .map_err(|e| MultiManagerPathsError::Store(e.to_string()))?;
    let new_group = if old.is_some_and(|v| v.get().trim_start().starts_with('[')) {
        let mut a: Vec<Box<serde_json::value::RawValue>> = serde_json::from_str(old.unwrap().get())
            .map_err(|e| MultiManagerPathsError::Store(e.to_string()))?;
        let required = if bool_changed {
            9
        } else if desired.bindings != actual.paths.bindings {
            3
        } else {
            2
        };
        while a.len() < required {
            let value = match a.len() {
                0 => serde_json::value::to_raw_value(&true),
                1 => serde_json::value::to_raw_value(&actual.paths.workspaces),
                2 => serde_json::value::to_raw_value(&actual.paths.bindings),
                3 | 4 => serde_json::value::to_raw_value(&true),
                5 | 6 => serde_json::value::to_raw_value(&false),
                7 => serde_json::value::to_raw_value(&crate::settings::MultiManagerSettings::default().hotkey_poll_ms),
                _ => serde_json::value::to_raw_value(&actual.auto_reconnect_on_load),
            }
            .map_err(|e| MultiManagerPathsError::Store(e.to_string()))?;
            a.push(value);
        }
        if desired.workspaces != actual.paths.workspaces {
            a[1] = workspace_raw;
        }
        if desired.bindings != actual.paths.bindings {
            a[2] = bindings_raw;
        }
        if bool_changed {
            a[8] = auto_raw;
        }
        serde_json::value::to_raw_value(&a)
    } else {
        let mut o = match old {
            Some(v) => super::omni_preferences::unique_object(v.get())
                .map_err(|e| MultiManagerPathsError::Store(e.to_string()))?,
            None => RawSettingsObject::new(),
        };
        if desired.workspaces != actual.paths.workspaces {
            o.insert("workspaces_path".into(), workspace_raw);
        }
        if desired.bindings != actual.paths.bindings {
            o.insert("bindings_path".into(), bindings_raw);
        }
        if bool_changed {
            o.insert("auto_reconnect_on_load".into(), auto_raw);
        }
        serde_json::value::to_raw_value(&o)
    }
    .map_err(|e| MultiManagerPathsError::Store(e.to_string()))?;
    match &mut parsed {
        Root::Object(o) => {
            o.insert("multi_manager".into(), new_group);
        }
        Root::Positional(a) => {
            while a.len() <= position {
                a.push(fields[a.len()].1.clone());
            }
            a[position] = new_group;
        }
    }
    let candidate = match &parsed {
        Root::Object(o) => serde_json::to_vec(o),
        Root::Positional(a) => serde_json::to_vec(a),
    }
    .map_err(|e| MultiManagerPathsError::Store(e.to_string()))?;
    let (expected, _, _) = inspect(path, Some(&candidate), &parsed)
        .map_err(|e| MultiManagerPathsError::Store(e.to_string()))?;
    if expected.paths != *desired || expected.auto_reconnect_on_load != auto_reconnect_on_load {
        return Err(MultiManagerPathsError::Store(
            "MultiManager paths failed round trip".into(),
        ));
    }
    let claimed = std::cell::Cell::new(false);
    let canceled = std::cell::Cell::new(false);
    let conflict = std::cell::Cell::new(false);
    let source_changed = std::cell::Cell::new(false);
    let final_check = || {
        if !active() || (!claimed.get() && token.is_some_and(|t| !t.begin_irrevocable_commit())) {
            canceled.set(true);
            return false;
        }
        claimed.set(true);
        if !active() {
            canceled.set(true);
            return false;
        }
        if !sources_current() {
            source_changed.set(true);
            return false;
        }
        if source_basis.is_some() && distinct_paths(path, &workspace, &bindings).is_err() {
            conflict.set(true);
            return false;
        }
        if !active() {
            canceled.set(true);
            return false;
        }
        if source(path).ok().as_ref() != Some(&bytes) {
            conflict.set(true);
            return false;
        }
        true
    };
    let result: anyhow::Result<()> = if bytes.is_some() {
        crate::common::atomic_file::save_atomic_if(path, &candidate, final_check)
    } else {
        (|| {
            let mut staged = tempfile::NamedTempFile::new_in(base)?;
            staged.write_all(&candidate)?;
            staged.flush()?;
            staged.as_file().sync_all()?;
            anyhow::ensure!(final_check(), "MultiManager paths superseded");
            staged.persist_noclobber(path).map_err(|e| e.error)?;
            Ok(())
        })()
    };
    if let Err(e) = result {
        if canceled.get() {
            return Err(MultiManagerPathsError::Canceled);
        }
        if source_changed.get() {
            return Err(MultiManagerPathsError::SourceChanged);
        }
        if conflict.get() || source(path).ok().as_ref() != Some(&bytes) {
            return Err(MultiManagerPathsError::Conflict);
        }
        return Err(MultiManagerPathsError::Store(e.to_string()));
    }
    if source(path).ok().flatten().as_deref() != Some(candidate.as_slice()) {
        return Err(MultiManagerPathsError::CommittedUnverified(
            "Settings readback differs or is unavailable".into(),
        ));
    }
    Ok(MultiManagerPathsAcknowledgement {
        snapshot: expected,
        persisted: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn path_only_save_preserves_external_auto_change() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, br#"{"multi_manager":{"workspaces_path":"old.json","bindings_path":"bindings.json","auto_reconnect_on_load":true,"opaque":7}}"#).unwrap();
        let baseline = Settings::multi_manager_paths_snapshot(&path).unwrap();
        std::fs::write(&path, br#"{"multi_manager":{"workspaces_path":"old.json","bindings_path":"bindings.json","auto_reconnect_on_load":false,"opaque":7}}"#).unwrap();
        let desired = MultiManagerPaths {
            workspaces: "new.json".into(),
            bindings: "bindings.json".into(),
        };
        let ack = Settings::save_multi_manager_paths_checked(&path, &baseline, &desired, || true).unwrap();
        assert_eq!(ack.snapshot.paths, desired);
        assert!(!ack.snapshot.auto_reconnect_on_load);
        let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(saved["multi_manager"]["auto_reconnect_on_load"], false);
        assert_eq!(saved["multi_manager"]["opaque"], 7);
    }

    #[test]
    fn path_save_rechecks_authority_after_source_validation() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, b"{}").unwrap();
        let baseline = Settings::multi_manager_paths_snapshot(&path).unwrap();
        let desired = MultiManagerPaths {
            workspaces: "new.json".into(),
            bindings: "bindings.json".into(),
        };
        let calls = std::cell::Cell::new(0);
        let result = Settings::save_multi_manager_paths_checked(&path, &baseline, &desired, || {
            calls.set(calls.get() + 1);
            calls.get() < 8
        });
        assert!(matches!(result, Err(MultiManagerPathsError::Canceled)));
        assert_eq!(calls.get(), 8);
        assert_eq!(std::fs::read(path).unwrap(), b"{}");
    }
    #[test]
    fn bool_only_preserves_paths_and_opaque_fields_without_reading_stores() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let original = br#"{"opaque":{"x":1},"multi_manager":{"workspaces_path":"bad.json","bindings_path":"also-bad.json","extra":7}}"#;
        std::fs::write(&path, original).unwrap();
        std::fs::write(dir.path().join("bad.json"), b"broken").unwrap();
        std::fs::create_dir(dir.path().join("also-bad.json")).unwrap();
        let baseline = Settings::multi_manager_paths_snapshot(&path).unwrap();
        assert!(baseline.auto_reconnect_on_load);
        let ack = Settings::save_multi_manager_settings_checked(&path, &baseline, &baseline.paths, false, || true).unwrap();
        assert!(!ack.snapshot.auto_reconnect_on_load);
        assert_eq!(ack.snapshot.paths, baseline.paths);
        let value: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(value["multi_manager"]["extra"], 7);
        assert_eq!(value["multi_manager"]["workspaces_path"], "bad.json");
        assert_eq!(value["opaque"]["x"], 1);
        assert!(matches!(Settings::save_multi_manager_settings_checked(&path, &baseline, &baseline.paths, false, || true), Err(MultiManagerPathsError::Conflict)));
        assert_eq!(std::fs::read(dir.path().join("bad.json")).unwrap(), b"broken");
    }

    #[test]
    fn positional_group_updates_only_auto_reconnect_slot() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let defaults = crate::settings::MultiManagerSettings::default();
        let group = serde_json::json!([true, "broken.json", "missing.json", true, true, false, false, defaults.hotkey_poll_ms, true, false, true]);
        std::fs::write(&path, serde_json::to_vec(&serde_json::json!({"multi_manager": group.clone(), "opaque": 9})).unwrap()).unwrap();
        std::fs::write(dir.path().join("broken.json"), b"broken").unwrap();
        let baseline = Settings::multi_manager_paths_snapshot(&path).unwrap();
        let ack = Settings::save_multi_manager_settings_checked(&path, &baseline, &baseline.paths, false, || true).unwrap();
        assert!(!ack.snapshot.auto_reconnect_on_load);
        let result: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        for index in 0..11 {
            if index == 8 { assert_eq!(result["multi_manager"][index], false); }
            else { assert_eq!(result["multi_manager"][index], group[index]); }
        }
        assert_eq!(result["opaque"], 9);
    }

    #[test]
    fn bool_only_late_settings_change_is_not_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, b"{}").unwrap();
        let baseline = Settings::multi_manager_paths_snapshot(&path).unwrap();
        let calls = std::cell::Cell::new(0);
        let result = Settings::save_multi_manager_settings_checked(&path, &baseline, &baseline.paths, false, || {
            calls.set(calls.get() + 1);
            if calls.get() == 4 { std::fs::write(&path, br#"{"opaque":7}"#).unwrap(); }
            true
        });
        assert!(matches!(result, Err(MultiManagerPathsError::Conflict)));
        assert_eq!(std::fs::read(path).unwrap(), br#"{"opaque":7}"#);
    }

    #[test]
    fn combined_paths_and_auto_reconnect_acknowledge_together() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let baseline = Settings::multi_manager_paths_snapshot(&path).unwrap();
        let desired = MultiManagerPaths { workspaces: "new-workspaces.json".into(), bindings: "new-bindings.json".into() };
        let ack = Settings::save_multi_manager_settings_checked(&path, &baseline, &desired, false, || true).unwrap();
        assert_eq!(ack.snapshot.paths, desired);
        assert!(!ack.snapshot.auto_reconnect_on_load);
        assert!(!dir.path().join("new-workspaces.json").exists());
        assert!(!dir.path().join("new-bindings.json").exists());
    }
    #[test]
    fn saves_relative_paths_preserving_opaque_fields_and_rejects_stale() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, br#"{"opaque":{"nested":42},"multi_manager":{"workspaces_path":"old.json","bindings_path":"bindings.json","extra":{"n":9}}}"#).unwrap();
        let baseline = Settings::multi_manager_paths_snapshot(&path).unwrap();
        let desired = MultiManagerPaths {
            workspaces: "new.json".into(),
            bindings: "other.json".into(),
        };
        let ack = Settings::save_multi_manager_paths_checked(&path, &baseline, &desired, || true)
            .unwrap();
        assert!(ack.persisted);
        assert_eq!(ack.snapshot.paths, desired);
        let json: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(json["opaque"]["nested"], 42);
        assert_eq!(json["multi_manager"]["extra"]["n"], 9);
        assert!(!dir.path().join("new.json").exists());
        assert!(!dir.path().join("other.json").exists());
        assert!(matches!(
            Settings::save_multi_manager_paths_checked(&path, &baseline, &desired, || true),
            Err(MultiManagerPathsError::Conflict)
        ));
    }
    #[test]
    fn rejects_aliases_corrupt_stores_and_cancellation_without_writes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let baseline = Settings::multi_manager_paths_snapshot(&path).unwrap();
        let same = MultiManagerPaths {
            workspaces: "same.json".into(),
            bindings: "./same.json".into(),
        };
        assert!(matches!(
            Settings::save_multi_manager_paths_checked(&path, &baseline, &same, || true),
            Err(MultiManagerPathsError::Store(_))
        ));
        let desired = MultiManagerPaths {
            workspaces: "workspace.json".into(),
            bindings: "bindings.json".into(),
        };
        std::fs::write(dir.path().join("workspace.json"), b"broken").unwrap();
        assert!(matches!(
            Settings::save_multi_manager_paths_checked(&path, &baseline, &desired, || true),
            Err(MultiManagerPathsError::Store(_))
        ));
        assert!(matches!(
            Settings::save_multi_manager_paths_checked(&path, &baseline, &desired, || false),
            Err(MultiManagerPathsError::Canceled)
        ));
        assert!(!path.exists());
    }

    #[test]
    fn short_positional_settings_are_extended_without_rewriting_prefix() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let fields = super::super::omni_preferences::ordered_fields(&Settings::default()).unwrap();
        let position = fields
            .iter()
            .position(|(name, _)| name == "multi_manager")
            .unwrap();
        let original: Vec<_> = fields
            .iter()
            .take(position)
            .map(|(_, value)| value.clone())
            .collect();
        std::fs::write(&path, serde_json::to_vec(&original).unwrap()).unwrap();
        assert!(parse_effective_settings(&path, &serde_json::to_vec(&original).unwrap()).is_ok());
        let baseline = Settings::multi_manager_paths_snapshot(&path).unwrap();
        let desired = MultiManagerPaths {
            workspaces: "workspace.json".into(),
            bindings: "bindings.json".into(),
        };
        let ack = Settings::save_multi_manager_paths_checked(&path, &baseline, &desired, || true)
            .unwrap();
        assert_eq!(ack.snapshot.paths, desired);
        let result: Vec<Box<serde_json::value::RawValue>> =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(result[0].get(), original[0].get());
        assert_eq!(result[1].get(), original[1].get());
    }

    #[test]
    fn late_source_replacement_refuses_settings_commit() {
        for replacement in [b"[]".as_slice(), b"broken".as_slice()] {
            let dir = tempfile::tempdir().unwrap();
            let settings = dir.path().join("settings.json");
            std::fs::write(&settings, b"{}").unwrap();
            let candidate = dir.path().join("workspace.json");
            let baseline = Settings::multi_manager_paths_snapshot(&settings).unwrap();
            let desired = MultiManagerPaths {
                workspaces: "workspace.json".into(),
                bindings: "bindings.json".into(),
            };
            let calls = std::cell::Cell::new(0);
            let result =
                Settings::save_multi_manager_paths_checked(&settings, &baseline, &desired, || {
                    calls.set(calls.get() + 1);
                    if calls.get() == 7 {
                        std::fs::write(&candidate, replacement).unwrap();
                    }
                    true
                });
            assert!(matches!(result, Err(MultiManagerPathsError::SourceChanged)));
            assert_eq!(std::fs::read(&settings).unwrap(), b"{}");
            assert_eq!(std::fs::read(candidate).unwrap(), replacement);
        }
    }

    #[test]
    fn invalid_utf8_in_opaque_bindings_is_rejected_before_save() {
        let dir = tempfile::tempdir().unwrap();
        let settings = dir.path().join("settings.json");
        let baseline = Settings::multi_manager_paths_snapshot(&settings).unwrap();
        let desired = MultiManagerPaths { workspaces: "workspace.json".into(), bindings: "bindings.json".into() };
        std::fs::write(dir.path().join("bindings.json"), b"[{\"workspace_name\":\"x\",\"opaque\":\"\xff\"}]").unwrap();
        assert!(matches!(Settings::save_multi_manager_paths_checked(&settings, &baseline, &desired, || true), Err(MultiManagerPathsError::Store(_))));
        assert!(!settings.exists());
    }

    #[test]
    fn existing_hardlinks_and_settings_self_alias_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let settings = dir.path().join("settings.json");
        std::fs::write(&settings, b"{}").unwrap();
        let workspace = dir.path().join("workspace.json");
        std::fs::write(&workspace, b"[]").unwrap();
        let link = dir.path().join("link.json");
        std::fs::hard_link(&workspace, &link).unwrap();
        assert!(distinct_paths(&settings, &workspace, &link).is_err());
        assert!(distinct_paths(&settings, &settings, &link).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn missing_windows_names_reject_ordinal_and_win32_aliases() {
        let dir = tempfile::tempdir().unwrap();
        assert!(aliases(&dir.path().join("Ä.json"), &dir.path().join("ä.json")).unwrap());
        assert!(
            aliases(
                &dir.path().join("name.json"),
                &dir.path().join("name.json. ")
            )
            .unwrap()
        );
    }
}
