#![allow(dead_code)]
#![allow(clippy::module_inception)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::type_complexity)]
#![allow(clippy::field_reassign_with_default)]
#![allow(clippy::if_same_then_else)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::enum_variant_names)]
#![allow(clippy::misnamed_getters)]
#![allow(clippy::suspicious_open_options)]
#![allow(clippy::ptr_arg)]
#![allow(clippy::from_str_radix_10)]
#![allow(clippy::unnecessary_sort_by)]
#![allow(clippy::question_mark)]
#![allow(clippy::manual_clamp)]
#![allow(unused_imports)]

pub mod actions;
pub mod actions_editor;

pub mod common;
pub mod dashboard;
pub mod file_search;
pub mod help_window;
pub mod history;
pub mod hotkey;
pub mod indexer;
pub mod launcher;
pub mod linking;
pub mod logging;
pub mod mouse_gestures;
pub mod multi_manager;
pub mod note_todo_sync;
pub mod note_ui_state;
pub mod notes_markdown;
pub mod platform;
pub mod plugin;
pub mod plugin_editor;
pub mod plugins;
pub mod process;
pub mod plugins_builtin;
pub mod settings;
pub mod settings_editor;
pub mod sound;
pub mod toast_log;
pub mod usage;
pub mod visibility;

pub mod global_hotkey;
pub mod graph;
pub mod gui;
pub mod window_manager;
pub mod windows_layout;
pub mod workspace;

pub fn request_hotkey_restart(_settings: settings::Settings) {
    // no-op stub for library context
}
