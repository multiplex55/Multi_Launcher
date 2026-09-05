use super::{Command, CommandError, CommandInvocation, CommandOutcome};
use crate::actions::Action;

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
    LauncherCommandHost + DialogCommandHost + CropCommandHost + HeadlessCommandHost + LegacyCommandHost
{
}

impl<T> CommandHost for T where
    T: LauncherCommandHost
        + DialogCommandHost
        + CropCommandHost
        + HeadlessCommandHost
        + LegacyCommandHost
{
}
