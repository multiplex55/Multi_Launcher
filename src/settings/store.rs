use super::Settings;
use crate::common::persistence::{LoadState, PersistenceError, load_json, save_json_atomic};
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
    use super::Settings;
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
}
