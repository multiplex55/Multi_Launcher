pub mod actions;
pub mod actions_editor;
pub mod add_action_dialog;
pub mod alias_dialog;
pub mod bookmark_alias_dialog;
pub mod add_bookmark_dialog;
pub mod plugin_editor;
pub mod settings_editor;
pub mod settings;
pub mod launcher;
pub mod plugin;
pub mod plugins_builtin;
pub mod plugins;
pub mod indexer;
pub mod logging;
pub mod hotkey;
pub mod history;
pub mod usage;
pub mod visibility;
pub mod help_window;
pub mod timer_help_window;
pub mod timer_dialog;
pub mod shell_cmd_dialog;
pub mod snippet_dialog;
pub mod notes_dialog;
pub mod todo_dialog;
pub mod clipboard_dialog;
pub mod volume_dialog;
pub mod brightness_dialog;

pub mod window_manager;
pub mod workspace;
pub mod global_hotkey;
pub mod gui;

pub fn request_hotkey_restart(_settings: settings::Settings) {
    // no-op stub for library context
}
