use crate::common::persistence::{LoadState, PersistenceError};
use crate::persistence::{RecoveryStartupResult, apply_pending_recovery};
use crate::platform::app_data::AppDataRoot;
use crate::settings::Settings;
use std::path::Path;

#[derive(Debug)]
pub enum SettingsStartupDiagnostic {
    LoadFailed(PersistenceError),
    MigrationSaveFailed(PersistenceError),
}

impl SettingsStartupDiagnostic {
    pub fn error(&self) -> &PersistenceError {
        match self {
            Self::LoadFailed(error) | Self::MigrationSaveFailed(error) => error,
        }
    }
}

#[derive(Debug)]
pub struct StartupSettings {
    pub settings: Settings,
    pub diagnostic: Option<SettingsStartupDiagnostic>,
}

#[derive(Debug)]
pub struct StartupPreload {
    pub recovery: RecoveryStartupResult,
    pub settings: StartupSettings,
}

/// Run the only persistence work permitted immediately after single-instance
/// acquisition. Recovery must precede the normal settings load so restored or
/// reset settings are the bytes observed by the rest of startup.
pub fn load_startup_preload(root: &AppDataRoot, settings_path: impl AsRef<Path>) -> StartupPreload {
    preload_with(
        || apply_pending_recovery(root),
        || load_startup_settings(settings_path),
    )
}

fn preload_with(
    recovery: impl FnOnce() -> RecoveryStartupResult,
    settings: impl FnOnce() -> StartupSettings,
) -> StartupPreload {
    let recovery = recovery();
    let settings = settings();
    StartupPreload { recovery, settings }
}

/// Load effective startup settings and own startup-only settings migration.
///
/// Existing invalid data is retained and produces temporary in-memory defaults.
/// A migration candidate is published only after it is durably persisted.
pub fn load_startup_settings(path: impl AsRef<Path>) -> StartupSettings {
    let path = path.as_ref();
    let original = match Settings::load_typed(path) {
        Ok(LoadState::Missing | LoadState::Empty) => Settings::default(),
        Ok(LoadState::Loaded(settings)) => settings,
        Err(error) => {
            return StartupSettings {
                settings: Settings::default(),
                diagnostic: Some(SettingsStartupDiagnostic::LoadFailed(error)),
            };
        }
    };

    let mut candidate = original.clone();
    if !crate::plugins::clipboard_modify::migrate_enablement(&mut candidate) {
        return StartupSettings {
            settings: original,
            diagnostic: None,
        };
    }

    match candidate.save_typed(path) {
        Ok(()) => StartupSettings {
            settings: candidate,
            diagnostic: None,
        },
        Err(error) => StartupSettings {
            settings: original,
            diagnostic: Some(SettingsStartupDiagnostic::MigrationSaveFailed(error)),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{SettingsStartupDiagnostic, load_startup_settings, preload_with};
    use crate::common::persistence::PersistenceError;
    use crate::persistence::RecoveryStartupResult;
    use crate::settings::Settings;

    fn has_clipboard_modify(settings: &Settings) -> bool {
        settings.plugin_settings.contains_key("clipboard_modify")
    }

    #[test]
    fn missing_settings_migration_persists() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");

        let startup = load_startup_settings(&path);

        assert!(startup.diagnostic.is_none());
        assert!(has_clipboard_modify(&startup.settings));
        assert!(has_clipboard_modify(
            &Settings::load(path.to_str().unwrap()).unwrap()
        ));
    }

    #[test]
    fn empty_settings_migration_persists() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        std::fs::write(&path, " \n\t").unwrap();

        let startup = load_startup_settings(&path);

        assert!(startup.diagnostic.is_none());
        assert!(has_clipboard_modify(&startup.settings));
        assert!(has_clipboard_modify(
            &Settings::load(path.to_str().unwrap()).unwrap()
        ));
    }

    #[test]
    fn valid_settings_load_without_rewrite_when_already_migrated() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let mut settings = Settings::default();
        crate::plugins::clipboard_modify::migrate_enablement(&mut settings);
        settings.show_examples = true;
        settings.save(path.to_str().unwrap()).unwrap();
        let before = std::fs::read(&path).unwrap();

        let startup = load_startup_settings(&path);

        assert!(startup.diagnostic.is_none());
        assert!(startup.settings.show_examples);
        assert_eq!(std::fs::read(path).unwrap(), before);
    }

    #[test]
    fn valid_settings_migration_persists() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let mut settings = Settings::default();
        settings.show_examples = true;
        settings.save(path.to_str().unwrap()).unwrap();

        let startup = load_startup_settings(&path);

        assert!(startup.diagnostic.is_none());
        assert!(startup.settings.show_examples);
        assert!(has_clipboard_modify(
            &Settings::load(path.to_str().unwrap()).unwrap()
        ));
    }

    #[test]
    fn malformed_settings_are_unchanged_and_migration_is_not_applied() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let invalid = b"{ this is not settings JSON";
        std::fs::write(&path, invalid).unwrap();

        let startup = load_startup_settings(&path);

        assert!(matches!(
            startup.diagnostic,
            Some(SettingsStartupDiagnostic::LoadFailed(
                PersistenceError::MalformedJson { .. }
            ))
        ));
        assert!(!has_clipboard_modify(&startup.settings));
        assert_eq!(std::fs::read(path).unwrap(), invalid);
    }

    #[test]
    fn unreadable_settings_use_temporary_defaults_with_diagnostic() {
        let directory = tempfile::tempdir().unwrap();

        let startup = load_startup_settings(directory.path());

        assert!(matches!(
            startup.diagnostic,
            Some(SettingsStartupDiagnostic::LoadFailed(
                PersistenceError::Read { .. }
            ))
        ));
        assert!(!has_clipboard_modify(&startup.settings));
        assert!(directory.path().is_dir());
    }

    #[test]
    fn failed_migration_save_does_not_publish_candidate() {
        let directory = tempfile::tempdir().unwrap();
        let blocker = directory.path().join("not-a-directory");
        std::fs::write(&blocker, "unchanged").unwrap();
        let path = blocker.join("settings.json");

        let startup = load_startup_settings(&path);

        assert!(matches!(
            startup.diagnostic,
            Some(SettingsStartupDiagnostic::MigrationSaveFailed(_))
        ));
        assert!(!has_clipboard_modify(&startup.settings));
        assert_eq!(std::fs::read_to_string(blocker).unwrap(), "unchanged");
    }

    #[test]
    fn recovery_is_instrumented_before_settings_load() {
        let events = std::cell::RefCell::new(Vec::new());
        let startup = preload_with(
            || {
                events.borrow_mut().push("recovery");
                RecoveryStartupResult::default()
            },
            || {
                events.borrow_mut().push("settings");
                super::StartupSettings {
                    settings: Settings::default(),
                    diagnostic: None,
                }
            },
        );

        assert!(startup.recovery.diagnostic.is_none());
        assert_eq!(&*events.borrow(), &["recovery", "settings"]);
    }
}
