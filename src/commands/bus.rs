use super::{Command, CommandError, CommandHost, CommandInvocation, CommandOutcome};
use crate::commands::handlers::{
    handle_calendar, handle_crop, handle_headless_gui, handle_launcher, handle_link, handle_note,
    handle_query, handle_simple_dialog,
};

#[derive(Debug, Default)]
pub struct CommandBus;

impl CommandBus {
    pub fn dispatch(
        &self,
        invocation: &CommandInvocation,
        host: &mut dyn CommandHost,
    ) -> Result<CommandOutcome, CommandError> {
        tracing::debug!(
            domain = invocation.domain(),
            kind = invocation.kind_name(),
            source = invocation.source.label(),
            "dispatching typed command"
        );
        if let Some(outcome) = handle_simple_dialog(host, &invocation.command) {
            return Ok(outcome);
        }
        match &invocation.command {
            Command::Launcher(command) => Ok(handle_launcher(host, command)),
            Command::Query(command) => Ok(handle_query(command, invocation.source)),
            Command::Crop(command) => Ok(handle_crop(host, command)),
            Command::Calendar(command) => Ok(handle_calendar(host, command)),
            Command::Note(command) => handle_note(host, command, invocation),
            Command::Link(command) => handle_link(host, command),
            // Temporary bridge: milestones 6-14 migrate the remaining enum families.
            _ => match handle_headless_gui(host, invocation) {
                Some(result) => result,
                None => host.execute_legacy_command(invocation),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::Action;
    use crate::commands::{
        ActivationSource, CalendarCommandHost, CropCommandHost, DialogCommandHost,
        HeadlessCommandHost, LauncherCommand, LauncherCommandHost, LegacyCommandHost,
        NoteCommandHost, QueryCommand, QueryPolicy, VisibilityPolicy,
    };

    #[derive(Default)]
    struct FakeHost {
        visible: bool,
        legacy_calls: usize,
        headless_calls: usize,
        dialog_calls: usize,
        crop_calls: usize,
        note_calls: usize,
    }

    impl LauncherCommandHost for FakeHost {
        fn launcher_is_visible(&self) -> bool {
            self.visible
        }
    }

    impl DialogCommandHost for FakeHost {
        fn open_help_dialog(&mut self) {
            self.dialog_calls += 1;
        }
        fn open_timer_dialog(&mut self) {
            self.dialog_calls += 1;
        }
        fn open_alarm_dialog(&mut self) {
            self.dialog_calls += 1;
        }
        fn open_shell_dialog(&mut self) {
            self.dialog_calls += 1;
        }
        fn open_bookmark_dialog(&mut self) {
            self.dialog_calls += 1;
        }
        fn open_snippet_dialog(&mut self) {
            self.dialog_calls += 1;
        }
        fn open_snippet_editor(&mut self, _: &str) {
            self.dialog_calls += 1;
        }
        fn open_favorite_dialog(&mut self, _: &str) {
            self.dialog_calls += 1;
        }
        fn open_legacy_macro_dialog(&mut self) {
            self.dialog_calls += 1;
        }
        fn open_mkmacro_dialog(&mut self) {
            self.dialog_calls += 1;
        }
        fn open_todo_dialog(&mut self) {
            self.dialog_calls += 1;
        }
        fn open_clipboard_dialog(&mut self) {
            self.dialog_calls += 1;
        }
        fn open_convert_dialog(&mut self) {
            self.dialog_calls += 1;
        }
        fn open_tempfile_dialog(&mut self) {
            self.dialog_calls += 1;
        }
        fn open_settings_dialog(&mut self) {
            self.dialog_calls += 1;
        }
        fn open_dashboard_settings_dialog(&mut self) {
            self.dialog_calls += 1;
        }
        fn open_theme_dialog(&mut self) {
            self.dialog_calls += 1;
        }
        fn open_volume_dialog(&mut self) {
            self.dialog_calls += 1;
        }
        fn open_brightness_dialog(&mut self) {
            self.dialog_calls += 1;
        }
        fn open_cpu_list_dialog(&mut self, _: usize) {
            self.dialog_calls += 1;
        }
    }

    impl CropCommandHost for FakeHost {
        fn crop_image(&mut self) {
            self.crop_calls += 1;
        }
        fn crop_screenshot(&mut self) {
            self.crop_calls += 1;
        }
    }
    impl CalendarCommandHost for FakeHost {
        fn calendar_dashboard_enabled(&self) -> bool {
            false
        }
        fn calendar_preserve_command(&self) -> bool {
            false
        }
        fn open_calendar_popover(&mut self, _: chrono::NaiveDate) {}
        fn refresh_calendar_cache(&mut self) {}
    }
    impl NoteCommandHost for FakeHost {
        fn open_notes_dialog(&mut self) {
            self.note_calls += 1;
        }
        fn open_note_graph_dialog(&mut self, _: Option<&str>) {}
        fn open_unused_note_assets_dialog(&mut self) {}
        fn open_note_panel(&mut self, _: &str, _: Option<&str>) {}
        fn open_note_tags(&mut self) {}
        fn open_note_link(&mut self, _: &str) {}
        fn wrap_note_plain_links(&mut self, _: &str) {}
        fn delete_note(&mut self, _: &str) {}
    }
    impl HeadlessCommandHost for FakeHost {
        fn execute_headless_command(&mut self, _: &Command, _: &Action) -> anyhow::Result<()> {
            self.headless_calls += 1;
            Ok(())
        }
        fn spawn_headless_command(&mut self, _: Command, _: Action) {}
        fn clear_query_after_run(&self) -> bool {
            false
        }
        fn hide_after_run(&self) -> bool {
            false
        }
        fn preserve_command(&self) -> bool {
            false
        }
        fn current_query(&self) -> &str {
            ""
        }
        fn launcher_should_refocus(&self) -> bool {
            false
        }
    }
    impl LegacyCommandHost for FakeHost {
        fn execute_legacy_command(
            &mut self,
            _: &CommandInvocation,
        ) -> Result<CommandOutcome, CommandError> {
            self.legacy_calls += 1;
            Ok(CommandOutcome::default())
        }
    }

    fn invocation(command: Command) -> CommandInvocation {
        CommandInvocation {
            command,
            original_action: Action {
                label: "x".into(),
                desc: "x".into(),
                action: "x".into(),
                args: None,
            },
            query_override: None,
            source: ActivationSource::Dashboard,
        }
    }

    #[test]
    fn command_bus_routes_launcher_and_query_without_legacy_bridge() {
        let mut host = FakeHost::default();
        let launcher = CommandBus
            .dispatch(
                &invocation(Command::Launcher(LauncherCommand::Show { query: None })),
                &mut host,
            )
            .unwrap();
        assert_eq!(launcher.visibility, VisibilityPolicy::Show);

        let query = CommandBus
            .dispatch(
                &invocation(Command::Query(QueryCommand::Set {
                    query: "abc".into(),
                    argument: None,
                })),
                &mut host,
            )
            .unwrap();
        assert_eq!(query.query, QueryPolicy::Set("abc".into()));
        assert_eq!(host.legacy_calls, 0);

        CommandBus
            .dispatch(
                &invocation(Command::Dialog(crate::commands::DialogCommand::Help)),
                &mut host,
            )
            .unwrap();
        CommandBus
            .dispatch(
                &invocation(Command::Crop(crate::commands::CropCommand::Screenshot)),
                &mut host,
            )
            .unwrap();
        assert_eq!(host.dialog_calls, 1);
        assert_eq!(host.crop_calls, 1);
        assert_eq!(host.legacy_calls, 0);

        let calendar = CommandBus
            .dispatch(
                &invocation(Command::Calendar(
                    crate::commands::CalendarCommand::Search {
                        input: "definitely-unmatched-calendar-query".into(),
                    },
                )),
                &mut host,
            )
            .unwrap();
        assert!(matches!(
            calendar.results,
            crate::commands::ResultsPolicy::Replace(_)
        ));
        assert_eq!(host.legacy_calls, 0);

        CommandBus
            .dispatch(
                &invocation(Command::Note(crate::commands::NoteCommand::Dialog)),
                &mut host,
            )
            .unwrap();
        let linked_todo = CommandBus
            .dispatch(
                &invocation(Command::Link(crate::commands::LinkCommand::Open {
                    id: "link://todo/7".into(),
                })),
                &mut host,
            )
            .unwrap();
        assert_eq!(host.note_calls, 1);
        assert_eq!(
            linked_todo.query,
            QueryPolicy::Set("todo links id:7".into())
        );
        assert_eq!(host.legacy_calls, 0);

        CommandBus
            .dispatch(
                &invocation(Command::External(crate::commands::ExternalCommand {
                    target: "tool".into(),
                    args: None,
                })),
                &mut host,
            )
            .unwrap();
        assert_eq!(host.headless_calls, 1);
        assert_eq!(host.legacy_calls, 0);
    }
}
