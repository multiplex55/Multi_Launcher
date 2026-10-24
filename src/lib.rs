#![allow(dead_code)]
#![allow(clippy::module_inception)]
#![cfg_attr(test, allow(clippy::items_after_test_module))]
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
pub mod active_window;
pub mod annotation;
pub mod batch_rename;
pub(crate) mod batch_rename_transaction;
pub mod brightness_service;

pub mod clipboard_modify;
pub mod color;
pub mod color_pick;
pub mod commands;
pub mod common;
pub mod completion;
pub mod coordinate_tool;
pub mod dashboard;
pub mod date_arithmetic;
pub mod diff;
pub(crate) mod duplicate_finder;
pub mod file_operations;
pub mod file_search;
pub mod help_window;
pub mod history;
pub mod hotkey;
pub(crate) mod image_compare;
pub mod image_crop;
pub mod indexer;
pub mod json_transform;
pub mod launcher;
pub(crate) mod launcher_parking;
pub mod linking;
pub mod logging;
pub mod mkmacro;
pub mod mouse_gestures;
pub mod multi_manager;
pub mod note_todo_sync;
pub mod note_ui_state;
pub mod notes_markdown;
pub mod ocr;
pub mod paste_sequence;
#[cfg(feature = "perf-harness")]
pub mod perf_harness;
pub mod performance;
pub mod persistence;
pub mod pinboard;
pub mod platform;
pub mod plugin;
pub mod plugin_editor;
pub mod plugins;
pub mod plugins_builtin;
pub mod process;
pub mod qr;
pub mod radial;
pub mod regex_tester;
pub mod replace_executor;
pub mod screen_draw;
pub mod search_replace;
pub mod settings;
pub mod settings_editor;
mod settings_save;
pub mod sound;
pub mod startup;
pub mod table_utility;
pub mod table_utility_session;
pub mod target_repair;
pub mod text_transform;
pub(crate) mod thread_reaper;
pub mod toast_log;
pub mod unit_conversion;
pub mod universal_actions;
pub mod usage;
pub mod virtual_desktop;
pub mod visibility;
pub mod window_activation;
pub mod window_catalog;

pub mod global_hotkey;
pub mod graph;
pub mod gui;
pub mod hash_generator;
pub mod hotfolder;
pub mod window_manager;
pub mod windows_layout;
pub mod workspace;
pub mod xml_toolkit_engine;
pub mod xml_toolkit_session;

// The shared startup owner is compiled into the library only for its
// deterministic scanner-injection contract. Production library builds expose
// no startup-catalog testing API.
#[cfg(test)]
use crate as startup_runtime;
#[cfg(test)]
#[path = "startup_catalog.rs"]
mod startup_catalog;
#[cfg(test)]
#[path = "startup_catalog_tests.rs"]
mod startup_catalog_tests;

#[cfg(test)]
fn startup_index_coordinator_for_owner()
-> Result<indexer::coordinator::IndexCoordinator, indexer::coordinator::CoordinatorError> {
    match startup_catalog_tests::take_startup_coordinator_override() {
        Some(coordinator) => Ok(coordinator),
        None => indexer::coordinator::IndexCoordinator::new(),
    }
}
