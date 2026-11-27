//! Sparse compare-and-swap for the launcher hotkey and its exact raw leaf.
use super::*;
use std::io::Write;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LauncherHotkeySnapshot {
    pub value: Option<String>,
    raw: Option<String>,
    positional: bool,
}

enum RawDocument {
    Object(RawSettingsObject),
    Positional(Vec<Box<serde_json::value::RawValue>>),
}

fn source(path: &Path) -> anyhow::Result<Option<Vec<u8>>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn root(bytes: Option<&[u8]>) -> anyhow::Result<RawDocument> {
    match bytes {
        None => Ok(RawDocument::Object(RawSettingsObject::new())),
        Some(bytes) if bytes.iter().all(u8::is_ascii_whitespace) => {
            Ok(RawDocument::Object(RawSettingsObject::new()))
        }
        Some(bytes)
            if bytes
                .iter()
                .copied()
                .find(|byte| !byte.is_ascii_whitespace())
                == Some(b'[') =>
        {
            Ok(RawDocument::Positional(serde_json::from_slice(bytes)?))
        }
        Some(bytes) => Ok(RawDocument::Object(serde_json::from_slice(bytes)?)),
    }
}

fn snapshot(
    path: &Path,
    bytes: Option<&[u8]>,
    root: &RawDocument,
) -> anyhow::Result<LauncherHotkeySnapshot> {
    let settings = parse_effective_settings(path, bytes.unwrap_or_default())?;
    Ok(LauncherHotkeySnapshot {
        value: settings.hotkey,
        raw: match root {
            RawDocument::Object(root) => root.get("hotkey"),
            RawDocument::Positional(root) => root.first(),
        }
        .map(|raw| raw.get().to_owned()),
        positional: matches!(root, RawDocument::Positional(_)),
    })
}

impl Settings {
    pub fn with_exact_launcher_hotkey_snapshot<T>(
        path: impl AsRef<Path>,
        expected: &LauncherHotkeySnapshot,
        claim: impl FnOnce() -> Result<T, String>,
    ) -> Result<T, String> {
        let path = path.as_ref();
        let _transaction = transaction_guard();
        let bytes = source(path).map_err(|error| error.to_string())?;
        let root = root(bytes.as_deref()).map_err(|error| error.to_string())?;
        let observed =
            snapshot(path, bytes.as_deref(), &root).map_err(|error| error.to_string())?;
        if observed != *expected {
            return Err("The saved launcher hotkey differs from the known prior value.".into());
        }
        claim()
    }
    pub fn launcher_hotkey_snapshot(
        path: impl AsRef<Path>,
    ) -> anyhow::Result<LauncherHotkeySnapshot> {
        let path = path.as_ref();
        let _transaction = transaction_guard();
        let bytes = source(path)?;
        let root = root(bytes.as_deref())?;
        snapshot(path, bytes.as_deref(), &root)
    }

    /// Writes only `hotkey`, or restores an earlier exact leaf, if its current
    /// value and presence still match the caller's acknowledged snapshot.
    pub fn compare_exchange_launcher_hotkey(
        path: impl AsRef<Path>,
        expected: &LauncherHotkeySnapshot,
        replacement: &LauncherHotkeySnapshot,
        current: impl Fn() -> bool,
    ) -> anyhow::Result<(LauncherHotkeySnapshot, Settings)> {
        let path = path.as_ref();
        let _transaction = transaction_guard();
        let bytes = source(path)?;
        let mut root = root(bytes.as_deref())?;
        anyhow::ensure!(
            snapshot(path, bytes.as_deref(), &root)? == *expected,
            "launcher hotkey changed on disk"
        );
        anyhow::ensure!(
            expected.positional == replacement.positional,
            "launcher settings representation changed"
        );
        match &mut root {
            RawDocument::Object(root) => {
                if let Some(raw) = &replacement.raw {
                    root.insert(
                        "hotkey".into(),
                        serde_json::value::RawValue::from_string(raw.clone())?,
                    );
                } else {
                    root.remove("hotkey");
                }
            }
            RawDocument::Positional(root) => {
                let raw = replacement.raw.as_ref().ok_or_else(|| {
                    anyhow::anyhow!("positional launcher hotkey cannot be missing")
                })?;
                let first = root.first_mut().ok_or_else(|| {
                    anyhow::anyhow!("positional settings are missing the launcher hotkey")
                })?;
                *first = serde_json::value::RawValue::from_string(raw.clone())?;
            }
        }
        let candidate = match &root {
            RawDocument::Object(root) => serde_json::to_vec_pretty(root)?,
            RawDocument::Positional(root) => serde_json::to_vec_pretty(root)?,
        };
        let committed = snapshot(path, Some(&candidate), &root)?;
        anyhow::ensure!(
            committed == *replacement,
            "invalid launcher hotkey replacement"
        );
        let settings = parse_effective_settings(path, &candidate)?;
        let admissible = || current() && source(path).ok().as_ref() == Some(&bytes);
        if bytes.is_some() {
            crate::common::atomic_file::save_atomic_if(path, &candidate, admissible)?;
        } else {
            let parent = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."));
            let mut staged = tempfile::NamedTempFile::new_in(parent)?;
            staged.write_all(&candidate)?;
            staged.flush()?;
            staged.as_file().sync_all()?;
            anyhow::ensure!(
                admissible(),
                "settings changed or launcher hotkey save was cancelled"
            );
            staged
                .persist_noclobber(path)
                .map_err(|error| error.error)?;
        }
        Ok((committed, settings))
    }
}

impl LauncherHotkeySnapshot {
    pub fn replacement(&self, value: Option<String>) -> anyhow::Result<Self> {
        let raw = Some(serde_json::to_string(&value)?);
        Ok(Self {
            value,
            raw,
            positional: self.positional,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn launcher_hotkey_cas_preserves_opaque_siblings_and_missing_leaf_on_rollback() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let original = br#"{"opaque":{"large":123456789012345678901},"other":"keep"}"#;
        std::fs::write(&path, original).unwrap();
        let before = Settings::launcher_hotkey_snapshot(&path).unwrap();
        assert!(Settings::with_exact_launcher_hotkey_snapshot(&path, &before, || Ok(())).is_ok());
        let desired = before.replacement(Some("Ctrl+F8".into())).unwrap();
        let (written, _) =
            Settings::compare_exchange_launcher_hotkey(&path, &before, &desired, || true).unwrap();
        assert!(
            Settings::with_exact_launcher_hotkey_snapshot(
                &path,
                &before,
                || -> Result<(), String> { panic!("stale claim ran") }
            )
            .is_err()
        );
        let RawDocument::Object(raw) = root(Some(&std::fs::read(&path).unwrap())).unwrap() else {
            panic!("expected object")
        };
        assert_eq!(raw["opaque"].get(), "{\"large\":123456789012345678901}");
        assert!(
            Settings::compare_exchange_launcher_hotkey(&path, &before, &desired, || true).is_err()
        );
        assert!(
            Settings::compare_exchange_launcher_hotkey(&path, &written, &before, || false).is_err()
        );
        Settings::compare_exchange_launcher_hotkey(&path, &written, &before, || true).unwrap();
        let RawDocument::Object(after) = root(Some(&std::fs::read(&path).unwrap())).unwrap() else {
            panic!("expected object")
        };
        assert!(!after.contains_key("hotkey"));
        assert_eq!(after["opaque"].get(), "{\"large\":123456789012345678901}");
    }

    #[test]
    fn launcher_hotkey_cas_preserves_legacy_positional_siblings() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let mut settings = Settings::default();
        settings.paste_sequence.next_hotkey = None;
        settings.paste_sequence.cancel_hotkey = None;
        let bytes = legacy_positional_settings(&settings);
        std::fs::write(&path, &bytes).unwrap();
        let before = Settings::launcher_hotkey_snapshot(&path).unwrap();
        let replacement = before.replacement(Some("Ctrl+F8".into())).unwrap();
        let (written, _) =
            Settings::compare_exchange_launcher_hotkey(&path, &before, &replacement, || true)
                .unwrap();
        let original: Vec<Box<serde_json::value::RawValue>> =
            serde_json::from_slice(&bytes).unwrap();
        let changed: Vec<Box<serde_json::value::RawValue>> =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(original.len(), changed.len());
        assert!(
            original
                .iter()
                .skip(1)
                .zip(changed.iter().skip(1))
                .all(|(a, b)| a.get() == b.get())
        );
        Settings::compare_exchange_launcher_hotkey(&path, &written, &before, || true).unwrap();
        assert_eq!(Settings::launcher_hotkey_snapshot(&path).unwrap(), before);
    }
}
