mod calendar;
mod dialog_crop;
mod file_search_diff;
mod headless_gui;
mod launcher_query;
mod mouse_gesture;
mod multi_manager;
mod note_link;
mod todo;

pub(crate) use calendar::handle_calendar;
pub(crate) use dialog_crop::{handle_crop, handle_simple_dialog};
pub(crate) use file_search_diff::{handle_diff, handle_file_search};
pub(crate) use headless_gui::handle_headless_gui;
pub(crate) use launcher_query::{handle_launcher, handle_query};
pub(crate) use mouse_gesture::handle_mouse_gesture;
pub(crate) use multi_manager::handle_multi_manager;
pub(crate) use note_link::{handle_link, handle_note};
pub(crate) use todo::handle_todo;
