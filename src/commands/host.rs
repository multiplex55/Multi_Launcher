use super::{Command, CommandError, CommandInvocation, CommandOutcome};
use crate::actions::Action;
use chrono::NaiveDate;

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

/// Transitional bridge removed after all command families have typed handlers.
pub trait LegacyCommandHost {
    fn execute_legacy_command(
        &mut self,
        invocation: &CommandInvocation,
    ) -> Result<CommandOutcome, CommandError>;
}

pub trait CommandHost:
    LauncherCommandHost
    + DialogCommandHost
    + CropCommandHost
    + CalendarCommandHost
    + NoteCommandHost
    + TodoCommandHost
    + HeadlessCommandHost
    + LegacyCommandHost
{
}

impl<T> CommandHost for T where
    T: LauncherCommandHost
        + DialogCommandHost
        + CropCommandHost
        + CalendarCommandHost
        + NoteCommandHost
        + TodoCommandHost
        + HeadlessCommandHost
        + LegacyCommandHost
{
}
