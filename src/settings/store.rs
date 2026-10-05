use super::Settings;
use crate::common::persistence::{LoadState, PersistenceError, load_json, save_json_atomic};
use anyhow::Context;
use once_cell::sync::Lazy;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

static SETTINGS_TRANSACTION: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

impl Settings {
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
mod tests {
    use super::{Settings, settings_file_sha256};
    use crate::common::persistence::{LoadState, PersistenceError};
    use std::sync::{Arc, Barrier};

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
}
