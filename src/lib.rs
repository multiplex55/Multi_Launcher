pub mod actions;
pub mod actions_editor;
pub mod add_action_dialog;
pub mod add_bookmark_dialog;
pub mod alias_dialog;
pub mod bookmark_alias_dialog;
pub mod brightness_dialog;
pub mod clipboard_dialog;
pub mod cpu_list_dialog;
pub mod help_window;
pub mod history;
pub mod hotkey;
pub mod indexer;
pub mod launcher;
pub mod logging;
pub mod notes_dialog;
pub mod plugin;
pub mod plugin_editor;
pub mod plugins;
pub mod plugins_builtin;
pub mod sound;
pub mod settings;
pub mod settings_editor;
pub mod shell_cmd_dialog;
pub mod snippet_dialog;
pub mod tempfile_alias_dialog;
pub mod tempfile_dialog;
pub mod timer_dialog;
pub mod timer_help_window;
pub mod todo_dialog;
pub mod usage;
pub mod visibility;
pub mod volume_dialog;

pub mod global_hotkey;
pub mod gui;
pub mod window_manager;
pub mod workspace;

pub fn request_hotkey_restart(_settings: settings::Settings) {
    // no-op stub for library context
}
