pub mod actions;
pub mod actions_editor;
pub mod add_action_dialog;
pub mod settings_editor;
pub mod settings;
pub mod launcher;
pub mod plugin;
pub mod plugins_builtin;
pub mod plugins;
pub mod indexer;
pub mod logging;
pub mod hotkey;
pub mod visibility;

pub mod window_manager;
pub mod workspace;
pub mod global_hotkey;
pub mod gui;

pub fn request_hotkey_restart(_settings: settings::Settings) {
    // no-op stub for library context
}
