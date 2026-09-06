use super::Command;
use crate::actions::Action;
use crate::clipboard_modify::actions::ClipboardModifySectionPayload;
use crate::clipboard_modify::coordinator::ImmediateRequestMetadata;
use crate::clipboard_modify::parser::ClipboardModifyIntent;
use crate::diff::query::DiffOpenPayload;
use crate::file_search::actions::{FileSearchModePayload, FileSearchStartPayload};
use crate::mouse_gestures::selection::{GestureFocusArgs, GestureToggleArgs};
use chrono::NaiveDate;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenshotCommandResult {
    Completed,
    Cancelled,
}

pub trait LauncherCommandHost {
    fn launcher_is_visible(&self) -> bool;
}

pub trait DialogCommandHost {
    fn open_help_dialog(&mut self);
    fn open_timer_dialog(&mut self);
    fn open_alarm_dialog(&mut self);
    fn open_shell_dialog(&mut self);
    fn open_bookmark_dialog(&mut self);
    fn open_snippet_dialog(&mut self);
    fn open_snippet_editor(&mut self, alias: &str);
    fn open_favorite_dialog(&mut self, label: &str);
    fn open_legacy_macro_dialog(&mut self);
    fn open_mkmacro_dialog(&mut self);
    fn open_todo_dialog(&mut self);
    fn open_clipboard_dialog(&mut self);
    fn open_convert_dialog(&mut self);
    fn open_tempfile_dialog(&mut self);
    fn open_settings_dialog(&mut self);
    fn open_dashboard_settings_dialog(&mut self);
    fn open_theme_dialog(&mut self);
    fn open_volume_dialog(&mut self);
    fn open_brightness_dialog(&mut self);
    fn open_cpu_list_dialog(&mut self, count: usize);
}

pub trait CropCommandHost {
    fn crop_image(&mut self);
    fn crop_screenshot(&mut self);
}

pub trait CalendarCommandHost {
    fn calendar_dashboard_enabled(&self) -> bool;
    fn calendar_preserve_command(&self) -> bool;
    fn open_calendar_popover(&mut self, date: NaiveDate);
    fn refresh_calendar_cache(&mut self);
}

pub trait NoteCommandHost {
    fn open_notes_dialog(&mut self);
    fn open_note_graph_dialog(&mut self, args: Option<&str>);
    fn open_unused_note_assets_dialog(&mut self);
    fn open_note_panel(&mut self, slug: &str, template: Option<&str>);
    fn open_note_tags(&mut self);
    fn open_note_link(&mut self, link: &str);
    fn wrap_note_plain_links(&mut self, slug: &str);
    fn delete_note(&mut self, slug: &str);
}

pub trait TodoCommandHost {
    fn open_todo_view(&mut self);
    fn open_todo_editor(&mut self, index: usize);
}

pub trait MouseGestureCommandHost {
    fn open_mouse_gesture_dialog(&mut self);
    fn open_mouse_gesture_add_dialog(&mut self);
    fn open_mouse_gesture_binding_dialog(&mut self);
    fn open_mouse_gesture_focus(&mut self, args: &GestureFocusArgs);
    fn open_mouse_gesture_settings_dialog(&mut self);
    fn set_mouse_gesture_enabled(&mut self, args: &GestureToggleArgs) -> Result<(), String>;
    fn mouse_gesture_launcher_should_refocus(&self) -> bool;
}

pub trait MultiManagerCommandHost {
    fn open_multi_manager(&mut self);
    fn open_multi_manager_settings(&mut self);
    fn multi_manager_save(&mut self);
    fn multi_manager_reload(&mut self);
    fn multi_manager_send_all_home(&mut self);
    fn multi_manager_start_manual_reconnect(&mut self);
    fn multi_manager_save_bindings(&mut self);
    fn multi_manager_restore_bindings(&mut self);
    fn multi_manager_import(&mut self);
    fn multi_manager_start_recapture_all(&mut self);
    fn multi_manager_toggle_workspace(&mut self, workspace_id: &str);
    fn multi_manager_send_home(&mut self, workspace_id: &str);
    fn multi_manager_send_target(&mut self, workspace_id: &str);
    fn multi_manager_start_capture(&mut self, workspace_id: &str);
    fn multi_manager_set_workspace_disabled(&mut self, workspace_id: &str, disabled: bool);
    fn multi_manager_launcher_should_refocus(&self) -> bool;
}

pub trait FileSearchCommandHost {
    fn open_file_search(&mut self);
    fn cancel_file_search(&mut self);
    fn set_file_search_mode(&mut self, payload: &FileSearchModePayload);
    fn start_file_search(&mut self, payload: &FileSearchStartPayload);
    fn report_file_search_action_error(&mut self, message: String);
}

pub trait DiffCommandHost {
    fn open_diff(&mut self, payload: &DiffOpenPayload) -> Result<(), String>;
}

pub trait ScreenshotCommandHost {
    fn capture_screenshot(
        &mut self,
        mode: super::ScreenshotMode,
        destination: super::ScreenshotDestination,
        markup: super::ScreenshotMarkup,
    ) -> Result<ScreenshotCommandResult, String>;
    fn screenshot_launcher_should_refocus(&self) -> bool;
}

pub trait ClipboardModifyCommandHost {
    fn open_clipboard_modify(&mut self, section: ClipboardModifySectionPayload);
    fn undo_clipboard_modify(&mut self) -> Result<(), String>;
    fn start_clipboard_modify(
        &mut self,
        intent: ClipboardModifyIntent,
        metadata: ImmediateRequestMetadata,
    ) -> Result<(), String>;
    fn clipboard_modify_hide_launcher_after_apply(&self) -> bool;
    fn report_clipboard_modify_action_error(&mut self, message: String);
}

pub trait HeadlessCommandHost {
    fn execute_headless_command(
        &mut self,
        command: &Command,
        original_action: &Action,
    ) -> anyhow::Result<()>;

    fn spawn_headless_command(&mut self, command: Command, original_action: Action);
    fn clear_query_after_run(&self) -> bool;
    fn hide_after_run(&self) -> bool;
    fn preserve_command(&self) -> bool;
    fn current_query(&self) -> &str;
    fn launcher_should_refocus(&self) -> bool;
}

pub trait CommandHost:
    LauncherCommandHost
    + DialogCommandHost
    + CropCommandHost
    + CalendarCommandHost
    + NoteCommandHost
    + TodoCommandHost
    + MouseGestureCommandHost
    + MultiManagerCommandHost
    + FileSearchCommandHost
    + DiffCommandHost
    + ScreenshotCommandHost
    + ClipboardModifyCommandHost
    + HeadlessCommandHost
{
}

impl<T> CommandHost for T where
    T: LauncherCommandHost
        + DialogCommandHost
        + CropCommandHost
        + CalendarCommandHost
        + NoteCommandHost
        + TodoCommandHost
        + MouseGestureCommandHost
        + MultiManagerCommandHost
        + FileSearchCommandHost
        + DiffCommandHost
        + ScreenshotCommandHost
        + ClipboardModifyCommandHost
        + HeadlessCommandHost
{
}
