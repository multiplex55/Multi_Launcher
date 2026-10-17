mod color_pick;
mod ocr;
pub(crate) use color_pick::handle_color_pick;
pub(crate) use ocr::handle_ocr;
mod calendar;
mod clipboard_modify;
mod coordinate_tool;
mod data;
mod dialog_crop;
mod file_search_diff;
mod headless_gui;
mod json_utility;
mod hash_generator;
mod launcher_query;
mod mouse_gesture;
mod multi_manager;
mod note_link;
mod paste_sequence;
mod radial;
mod screen_draw;
mod screenshot;
mod snippet_run;
mod todo;
mod xml_toolkit;

pub(crate) use calendar::handle_calendar;
pub(crate) use clipboard_modify::{
    handle_clipboard_modify, handle_clipboard_modify_with_history_query,
};
pub(crate) use coordinate_tool::handle_coordinate_tool;
pub(crate) use data::handle_data;
pub(crate) use dialog_crop::{handle_crop, handle_simple_dialog};
pub(crate) use file_search_diff::{handle_diff, handle_file_search};
pub(crate) use headless_gui::{
    handle_headless_gui, handle_headless_gui_with_history_query, success_outcome,
};
pub(crate) use json_utility::handle_json_utility;
pub(crate) use hash_generator::handle_hash_generator;
pub(crate) use launcher_query::{handle_launcher, handle_query};
pub(crate) use mouse_gesture::handle_mouse_gesture;
pub(crate) use multi_manager::handle_multi_manager;
pub(crate) use note_link::{handle_link, handle_note};
pub(crate) use paste_sequence::handle_paste_sequence;
pub(crate) use radial::handle_radial;
pub(crate) use screen_draw::handle_screen_draw;
pub(crate) use screenshot::handle_screenshot;
pub(crate) use snippet_run::handle_snippet_run;
pub(crate) use todo::handle_todo;
pub(crate) use xml_toolkit::handle_xml_toolkit;
