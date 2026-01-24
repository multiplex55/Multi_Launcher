pub mod actions;
pub mod actions_editor;

pub mod common;
pub mod dashboard;
pub mod help_window;
pub mod history;
pub mod hotkey;
pub mod indexer;
pub mod launcher;
pub mod logging;
pub mod mouse_gestures;
pub mod plugin;
pub mod plugin_editor;
pub mod plugins;
pub mod plugins_builtin;
pub mod settings;
pub mod settings_editor;
pub mod sound;
pub mod toast_log;
pub mod usage;
pub mod visibility;

pub mod global_hotkey;
pub mod gui;
pub mod windows_layout;
pub mod window_manager;
pub mod workspace;

pub fn request_hotkey_restart(_settings: settings::Settings) {
    // no-op stub for library context
}
