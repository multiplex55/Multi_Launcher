/// Save only the MultiManager settings block to the full settings file.
pub fn save_multi_manager_settings(
    settings_path: &str,
    multi_manager: crate::settings::MultiManagerSettings,
) -> anyhow::Result<()> {
    let mut settings = crate::settings::Settings::load(settings_path)?;
    settings.multi_manager = multi_manager;
    settings.save(settings_path)
}

#[cfg(test)]
mod tests {
    use super::save_multi_manager_settings;
    use crate::settings::{MultiManagerSettings, Settings};

    #[test]
    fn save_multi_manager_settings_preserves_unrelated_settings() {
        let dir = tempfile::tempdir().expect("tempdir");
        let settings_path = dir.path().join("settings.json");
        let settings_path = settings_path.to_string_lossy().to_string();

        let mut original = Settings::default();
        original.hotkey = Some("Ctrl+Alt+M".to_string());
        original.enable_toasts = false;
        original.plugin_settings.insert(
            "mouse_gestures".to_string(),
            serde_json::json!({ "enabled": true, "debug": true }),
        );
        original
            .save(&settings_path)
            .expect("save original settings");

        let updated = MultiManagerSettings {
            enabled: false,
            workspaces_path: "updated_workspaces.json".to_string(),
            bindings_path: "updated_bindings.json".to_string(),
            auto_save: false,
            save_on_exit: false,
            developer_debugging: true,
            show_force_recapture_prompt: true,
            hotkey_poll_ms: 250,
            auto_reconnect_on_load: false,
            hide_launcher_before_toggle: true,
            ignore_launcher_window_on_capture: false,
        };

        save_multi_manager_settings(&settings_path, updated.clone())
            .expect("save multi manager settings");

        let restored = Settings::load(&settings_path).expect("reload settings");
        assert_eq!(restored.hotkey, Some("Ctrl+Alt+M".to_string()));
        assert!(!restored.enable_toasts);
        assert_eq!(
            restored.plugin_settings.get("mouse_gestures"),
            Some(&serde_json::json!({ "enabled": true, "debug": true }))
        );
        assert_eq!(restored.multi_manager, updated);
    }

    #[test]
    fn save_multi_manager_settings_omits_obsolete_periodic_reconnect_fields() {
        let dir = tempfile::tempdir().expect("tempdir");
        let settings_path = dir.path().join("settings.json");
        std::fs::write(
            &settings_path,
            r#"{
                "multi_manager": {
                    "periodic_reconnect_enabled": true,
                    "reconnect_interval_ms": 1000,
                    "auto_reconnect_period_ms": 2500
                }
            }"#,
        )
        .expect("write legacy settings");
        let settings_path = settings_path.to_string_lossy().to_string();

        let updated = MultiManagerSettings {
            auto_reconnect_on_load: false,
            ..Default::default()
        };
        save_multi_manager_settings(&settings_path, updated.clone())
            .expect("save multi manager settings");

        let saved: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&settings_path).expect("read settings"))
                .expect("parse saved settings");
        let multi_manager = saved
            .get("multi_manager")
            .and_then(serde_json::Value::as_object)
            .expect("multi manager object");
        assert_eq!(
            multi_manager.get("auto_reconnect_on_load"),
            Some(&serde_json::Value::Bool(false))
        );
        assert!(!multi_manager.contains_key("periodic_reconnect_enabled"));
        assert!(!multi_manager.contains_key("reconnect_interval_ms"));
        assert!(!multi_manager.contains_key("auto_reconnect_period_ms"));
        assert_eq!(
            Settings::load(&settings_path).unwrap().multi_manager,
            updated
        );
    }
}
