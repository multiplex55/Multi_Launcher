use crate::commands::{
    ClipboardCommand, Command, CommandOutcome, CropCommand, CropCommandHost, DialogCommand,
    DialogCommandHost, HeadlessCommandHost, MacroCommand, ShellCommand, StorageCommand,
    SystemCommand, TimerCommand, TodoCommand,
};

pub(crate) fn handle_simple_dialog<H>(host: &mut H, command: &Command) -> Option<CommandOutcome>
where
    H: DialogCommandHost + HeadlessCommandHost + ?Sized,
{
    match command {
        Command::Dialog(DialogCommand::Help) => host.open_help_dialog(),
        Command::Dialog(DialogCommand::Convert) => host.open_convert_dialog(),
        Command::Dialog(DialogCommand::Settings) => host.open_settings_dialog(),
        Command::Dialog(DialogCommand::DashboardSettings) => host.open_dashboard_settings_dialog(),
        Command::Dialog(DialogCommand::Theme) => host.open_theme_dialog(),
        Command::Timer(TimerCommand::TimerDialog) => host.open_timer_dialog(),
        Command::Timer(TimerCommand::AlarmDialog) => host.open_alarm_dialog(),
        Command::Shell(ShellCommand::Dialog) => host.open_shell_dialog(),
        Command::Clipboard(ClipboardCommand::Dialog) => host.open_clipboard_dialog(),
        Command::Storage(StorageCommand::BookmarkDialog) => host.open_bookmark_dialog(),
        Command::Storage(StorageCommand::SnippetDialog) => host.open_snippet_dialog(),
        Command::Storage(StorageCommand::SnippetEdit(alias)) => host.open_snippet_editor(alias),
        Command::Storage(StorageCommand::FavoriteDialog(label)) => host.open_favorite_dialog(label),
        Command::Storage(StorageCommand::TempfileDialog) => host.open_tempfile_dialog(),
        Command::Macro(MacroCommand::LegacyDialog) => host.open_legacy_macro_dialog(),
        Command::Macro(MacroCommand::MkDialog) => host.open_mkmacro_dialog(),
        Command::System(SystemCommand::VolumeDialog) => host.open_volume_dialog(),
        Command::System(SystemCommand::BrightnessDialog) => host.open_brightness_dialog(),
        Command::System(SystemCommand::CpuList(count)) => host.open_cpu_list_dialog(*count),
        Command::Todo(TodoCommand::Dialog) => host.open_todo_dialog(),
        _ => return None,
    }
    Some(post_open_policy(host))
}

pub(crate) fn handle_crop<H>(host: &mut H, command: &CropCommand) -> CommandOutcome
where
    H: CropCommandHost + HeadlessCommandHost + ?Sized,
{
    match command {
        CropCommand::Image => host.crop_image(),
        CropCommand::Screenshot => host.crop_screenshot(),
    }
    post_open_policy(host)
}

fn post_open_policy<H: HeadlessCommandHost + ?Sized>(host: &H) -> CommandOutcome {
    CommandOutcome {
        focus: host.launcher_should_refocus(),
        ..CommandOutcome::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::Action;

    #[derive(Default)]
    struct Host {
        opened: Option<String>,
        clear: bool,
        hide: bool,
        refocus: bool,
    }

    impl Host {
        fn mark(&mut self, name: impl Into<String>) {
            self.opened = Some(name.into());
        }
    }

    impl DialogCommandHost for Host {
        fn open_help_dialog(&mut self) {
            self.mark("help");
        }
        fn open_timer_dialog(&mut self) {
            self.mark("timer");
        }
        fn open_alarm_dialog(&mut self) {
            self.mark("alarm");
        }
        fn open_shell_dialog(&mut self) {
            self.mark("shell");
        }
        fn open_bookmark_dialog(&mut self) {
            self.mark("bookmark");
        }
        fn open_snippet_dialog(&mut self) {
            self.mark("snippet");
        }
        fn open_snippet_editor(&mut self, alias: &str) {
            self.mark(format!("snippet:{alias}"));
        }
        fn open_favorite_dialog(&mut self, label: &str) {
            self.mark(format!("favorite:{label}"));
        }
        fn open_legacy_macro_dialog(&mut self) {
            self.mark("macro");
        }
        fn open_mkmacro_dialog(&mut self) {
            self.mark("mkmacro");
        }
        fn open_todo_dialog(&mut self) {
            self.mark("todo");
        }
        fn open_clipboard_dialog(&mut self) {
            self.mark("clipboard");
        }
        fn open_convert_dialog(&mut self) {
            self.mark("convert");
        }
        fn open_tempfile_dialog(&mut self) {
            self.mark("tempfile");
        }
        fn open_settings_dialog(&mut self) {
            self.mark("settings");
        }
        fn open_dashboard_settings_dialog(&mut self) {
            self.mark("dashboard_settings");
        }
        fn open_theme_dialog(&mut self) {
            self.mark("theme");
        }
        fn open_volume_dialog(&mut self) {
            self.mark("volume");
        }
        fn open_brightness_dialog(&mut self) {
            self.mark("brightness");
        }
        fn open_cpu_list_dialog(&mut self, count: usize) {
            self.mark(format!("cpu:{count}"));
        }
    }

    impl CropCommandHost for Host {
        fn crop_image(&mut self) {
            self.mark("crop_image");
        }
        fn crop_screenshot(&mut self) {
            self.mark("crop_screenshot");
        }
    }

    impl HeadlessCommandHost for Host {
        fn execute_headless_command(&mut self, _: &Command, _: &Action) -> anyhow::Result<()> {
            unreachable!()
        }
        fn spawn_headless_command(&mut self, _: Command, _: Action) {
            unreachable!()
        }
        fn clear_query_after_run(&self) -> bool {
            self.clear
        }
        fn hide_after_run(&self) -> bool {
            self.hide
        }
        fn preserve_command(&self) -> bool {
            false
        }
        fn current_query(&self) -> &str {
            ""
        }
        fn launcher_should_refocus(&self) -> bool {
            self.refocus
        }
    }

    #[test]
    fn every_simple_dialog_routes_through_the_typed_host() {
        let cases = [
            (Command::Dialog(DialogCommand::Help), "help"),
            (Command::Timer(TimerCommand::TimerDialog), "timer"),
            (Command::Timer(TimerCommand::AlarmDialog), "alarm"),
            (Command::Shell(ShellCommand::Dialog), "shell"),
            (Command::Storage(StorageCommand::BookmarkDialog), "bookmark"),
            (Command::Storage(StorageCommand::SnippetDialog), "snippet"),
            (
                Command::Storage(StorageCommand::SnippetEdit("a".into())),
                "snippet:a",
            ),
            (
                Command::Storage(StorageCommand::FavoriteDialog("f".into())),
                "favorite:f",
            ),
            (Command::Macro(MacroCommand::LegacyDialog), "macro"),
            (Command::Macro(MacroCommand::MkDialog), "mkmacro"),
            (Command::Todo(TodoCommand::Dialog), "todo"),
            (Command::Clipboard(ClipboardCommand::Dialog), "clipboard"),
            (Command::Dialog(DialogCommand::Convert), "convert"),
            (Command::Storage(StorageCommand::TempfileDialog), "tempfile"),
            (Command::Dialog(DialogCommand::Settings), "settings"),
            (
                Command::Dialog(DialogCommand::DashboardSettings),
                "dashboard_settings",
            ),
            (Command::Dialog(DialogCommand::Theme), "theme"),
            (Command::System(SystemCommand::VolumeDialog), "volume"),
            (
                Command::System(SystemCommand::BrightnessDialog),
                "brightness",
            ),
            (Command::System(SystemCommand::CpuList(12)), "cpu:12"),
        ];
        for (command, expected) in cases {
            let mut host = Host::default();
            let outcome = handle_simple_dialog(&mut host, &command).unwrap();
            assert_eq!(host.opened.as_deref(), Some(expected));
            assert_eq!(outcome, CommandOutcome::default());
        }
    }

    #[test]
    fn crop_routes_through_the_typed_host_and_keeps_history_skipped() {
        let mut host = Host::default();
        let outcome = handle_crop(&mut host, &CropCommand::Screenshot);
        assert_eq!(host.opened.as_deref(), Some("crop_screenshot"));
        assert_eq!(outcome.history, crate::commands::HistoryPolicy::Skip);
    }

    #[test]
    fn interactive_commands_ignore_generic_clear_hide_and_refocus_after_host_work() {
        let mut host = Host {
            clear: true,
            hide: true,
            refocus: true,
            ..Host::default()
        };
        let outcome =
            handle_simple_dialog(&mut host, &Command::Dialog(DialogCommand::Settings)).unwrap();
        assert_eq!(outcome.query, crate::commands::QueryPolicy::Keep);
        assert!(!outcome.search && !outcome.invalidate_results);
        assert_eq!(outcome.visibility, crate::commands::VisibilityPolicy::Keep);
        assert!(outcome.focus);

        host.refocus = false;
        assert!(!handle_crop(&mut host, &CropCommand::Image).focus);
    }

    #[test]
    fn non_dialog_variants_are_left_for_their_domain_handlers() {
        let mut host = Host::default();
        assert!(handle_simple_dialog(&mut host, &Command::Todo(TodoCommand::View)).is_none());
        assert!(
            handle_simple_dialog(&mut host, &Command::Timer(TimerCommand::Cancel(1))).is_none()
        );
    }
}
