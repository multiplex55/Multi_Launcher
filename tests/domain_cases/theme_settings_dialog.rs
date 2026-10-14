use multi_launcher::gui::ThemeSettingsDialogState;
use multi_launcher::settings::{Settings, ThemeMode};
use tempfile::tempdir;

#[test]
fn theme_settings_dialog_save_updates_settings_and_clears_dirty_flag() {
    let dir = tempdir().unwrap();
    let settings_path = dir.path().join("settings.json");
    let settings = Settings::default();
    settings.save(settings_path.to_str().unwrap()).unwrap();

    let ctx = eframe::egui::Context::default();
    let mut state = ThemeSettingsDialogState::default();
    state.begin_session(&ctx, settings_path.to_str().unwrap());
    state.draft.mode = ThemeMode::Light;
    state.dirty = true;

    state
        .save_to_path(&ctx, settings_path.to_str().unwrap())
        .unwrap();

    let updated = Settings::load(settings_path.to_str().unwrap()).unwrap();
    assert_eq!(updated.theme.mode, ThemeMode::Light);
    assert!(!state.dirty);
}

#[test]
fn theme_settings_dialog_failed_save_keeps_draft_dirty_and_returns_before_publication() {
    let dir = tempdir().unwrap();
    let blocker = dir.path().join("not-a-directory");
    std::fs::write(&blocker, "unchanged").unwrap();
    let settings_path = blocker.join("settings.json");
    let baseline_path = dir.path().join("baseline.json");
    let mut baseline = Settings::default();
    baseline.theme.mode = ThemeMode::Dark;
    baseline.save(baseline_path.to_str().unwrap()).unwrap();

    let ctx = eframe::egui::Context::default();
    let mut state = ThemeSettingsDialogState::default();
    state.begin_session(&ctx, baseline_path.to_str().unwrap());
    state.draft.mode = ThemeMode::Light;
    state.dirty = true;

    assert!(
        state
            .save_to_path(&ctx, settings_path.to_str().unwrap())
            .is_err()
    );
    assert!(state.dirty);
    assert_eq!(state.draft.mode, ThemeMode::Light);
    assert_eq!(ctx.theme(), eframe::egui::Theme::Dark);
    assert_eq!(std::fs::read_to_string(blocker).unwrap(), "unchanged");
}

#[test]
fn theme_settings_dialog_reload_reflects_latest_saved_values() {
    let dir = tempdir().unwrap();
    let settings_path = dir.path().join("settings.json");
    let mut settings = Settings::default();
    settings.theme.mode = ThemeMode::Dark;
    settings.save(settings_path.to_str().unwrap()).unwrap();

    let ctx = eframe::egui::Context::default();
    let mut state = ThemeSettingsDialogState::default();
    state.begin_session(&ctx, settings_path.to_str().unwrap());
    assert_eq!(state.draft.mode, ThemeMode::Dark);

    settings.theme.mode = ThemeMode::Custom;
    settings.save(settings_path.to_str().unwrap()).unwrap();

    state.reload_from_path(&ctx, settings_path.to_str().unwrap());
    assert_eq!(state.draft.mode, ThemeMode::Custom);
}
