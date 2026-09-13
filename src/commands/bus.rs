use super::{Command, CommandError, CommandHost, CommandInvocation, CommandOutcome};
use crate::commands::handlers::{
    handle_calendar, handle_clipboard_modify, handle_crop, handle_data, handle_diff,
    handle_file_search, handle_headless_gui, handle_launcher, handle_link, handle_mouse_gesture,
    handle_multi_manager, handle_note, handle_query, handle_screen_draw, handle_screenshot,
    handle_simple_dialog, handle_todo,
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
            Command::Todo(command) => handle_todo(host, command, invocation),
            Command::MouseGesture(command) => handle_mouse_gesture(host, command),
            Command::MultiManager(command) => Ok(handle_multi_manager(host, command)),
            Command::FileSearch(command) => Ok(handle_file_search(host, command)),
            Command::Diff(command) => handle_diff(host, command),
            Command::Screenshot(command) => handle_screenshot(host, command),
            Command::ScreenDraw(command) => handle_screen_draw(host, command),
            Command::ClipboardModify(command) => {
                Ok(handle_clipboard_modify(host, command, invocation))
            }
            Command::Data(command) => handle_data(host, command),
            Command::VirtualDesktop(super::VirtualDesktopCommand::Settings) => {
                host.open_settings_dialog();
                Ok(CommandOutcome::default())
            }
            Command::Shell(_)
            | Command::Clipboard(_)
            | Command::Calculator(_)
            | Command::Storage(_)
            | Command::Timer(_)
            | Command::System(_)
            | Command::BrowserTab(_)
            | Command::Media(_)
            | Command::Layout(_)
            | Command::Macro(_)
            | Command::VirtualDesktop(_)
            | Command::External(_) => handle_headless_gui(host, invocation),
            Command::Dialog(_) => unreachable!("dialog commands are handled before dispatch"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::Action;
    use crate::commands::{
        ActivationSource, CalendarCommandHost, CropCommandHost, DialogCommandHost,
        HeadlessCommandHost, LauncherCommand, LauncherCommandHost, MultiManagerCommandHost,
        NoteCommandHost, QueryCommand, QueryPolicy, TodoCommandHost, VisibilityPolicy,
    };

    #[derive(Default)]
    struct FakeHost {
        visible: bool,
        headless_calls: usize,
        dialog_calls: usize,
        crop_calls: usize,
        note_calls: usize,
        todo_calls: usize,
        file_search_calls: usize,
        diff_calls: usize,
        screenshot_calls: usize,
        screen_draw_calls: Vec<crate::commands::ScreenDrawCommand>,
        clipboard_modify_calls: usize,
        data_calls: Vec<&'static str>,
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
    impl TodoCommandHost for FakeHost {
        fn open_todo_view(&mut self) {
            self.todo_calls += 1;
        }
        fn open_todo_editor(&mut self, _: usize) {
            self.todo_calls += 1;
        }
    }
    impl crate::commands::MouseGestureCommandHost for FakeHost {
        fn open_mouse_gesture_dialog(&mut self) {}
        fn open_mouse_gesture_add_dialog(&mut self) {}
        fn open_mouse_gesture_binding_dialog(&mut self) {}
        fn open_mouse_gesture_focus(
            &mut self,
            _: &crate::mouse_gestures::selection::GestureFocusArgs,
        ) {
        }
        fn open_mouse_gesture_settings_dialog(&mut self) {}
        fn set_mouse_gesture_enabled(
            &mut self,
            _: &crate::mouse_gestures::selection::GestureToggleArgs,
        ) -> Result<(), String> {
            Ok(())
        }
        fn mouse_gesture_launcher_should_refocus(&self) -> bool {
            false
        }
    }
    impl MultiManagerCommandHost for FakeHost {
        fn open_multi_manager(&mut self) {}
        fn open_multi_manager_settings(&mut self) {}
        fn multi_manager_save(&mut self) {}
        fn multi_manager_reload(&mut self) {}
        fn multi_manager_send_all_home(&mut self) {}
        fn multi_manager_start_manual_reconnect(&mut self) {}
        fn multi_manager_save_bindings(&mut self) {}
        fn multi_manager_restore_bindings(&mut self) {}
        fn multi_manager_import(&mut self) {}
        fn multi_manager_start_recapture_all(&mut self) {}
        fn multi_manager_toggle_workspace(&mut self, _: &str) {}
        fn multi_manager_send_home(&mut self, _: &str) {}
        fn multi_manager_send_target(&mut self, _: &str) {}
        fn multi_manager_start_capture(&mut self, _: &str) {}
        fn multi_manager_set_workspace_disabled(&mut self, _: &str, _: bool) {}
        fn multi_manager_launcher_should_refocus(&self) -> bool {
            false
        }
    }
    impl crate::commands::FileSearchCommandHost for FakeHost {
        fn open_file_search(&mut self) {
            self.file_search_calls += 1;
        }
        fn cancel_file_search(&mut self) {
            self.file_search_calls += 1;
        }
        fn set_file_search_mode(&mut self, _: &crate::file_search::actions::FileSearchModePayload) {
            self.file_search_calls += 1;
        }
        fn start_file_search(&mut self, _: &crate::file_search::actions::FileSearchStartPayload) {
            self.file_search_calls += 1;
        }
        fn report_file_search_action_error(&mut self, _: String) {
            self.file_search_calls += 1;
        }
    }
    impl crate::commands::DiffCommandHost for FakeHost {
        fn open_diff(&mut self, _: &crate::diff::query::DiffOpenPayload) -> Result<(), String> {
            self.diff_calls += 1;
            Ok(())
        }
    }
    impl crate::commands::ScreenshotCommandHost for FakeHost {
        fn capture_screenshot(
            &mut self,
            _: crate::commands::ScreenshotMode,
            _: crate::commands::ScreenshotDestination,
            _: crate::commands::ScreenshotMarkup,
        ) -> Result<crate::commands::ScreenshotCommandResult, String> {
            self.screenshot_calls += 1;
            Ok(crate::commands::ScreenshotCommandResult::Completed)
        }
        fn screenshot_launcher_should_refocus(&self) -> bool {
            false
        }
    }
    impl crate::commands::ScreenDrawCommandHost for FakeHost {
        fn execute_screen_draw_command(
            &mut self,
            command: crate::commands::ScreenDrawCommand,
        ) -> Result<(), String> {
            self.screen_draw_calls.push(command);
            Ok(())
        }
    }
    impl crate::commands::ClipboardModifyCommandHost for FakeHost {
        fn open_clipboard_modify(
            &mut self,
            _: crate::clipboard_modify::actions::ClipboardModifySectionPayload,
        ) {
            self.clipboard_modify_calls += 1;
        }
        fn undo_clipboard_modify(&mut self) -> Result<(), String> {
            self.clipboard_modify_calls += 1;
            Ok(())
        }
        fn start_clipboard_modify(
            &mut self,
            _: crate::clipboard_modify::parser::ClipboardModifyIntent,
            _: crate::clipboard_modify::coordinator::ImmediateRequestMetadata,
        ) -> Result<(), String> {
            self.clipboard_modify_calls += 1;
            Ok(())
        }
        fn clipboard_modify_hide_launcher_after_apply(&self) -> bool {
            false
        }
        fn report_clipboard_modify_action_error(&mut self, _: String) {
            self.clipboard_modify_calls += 1;
        }
    }
    impl crate::commands::DataCommandHost for FakeHost {
        fn open_data_dialog(
            &mut self,
            focus: crate::commands::DataDialogFocus,
        ) -> Result<(), String> {
            self.data_calls.push(match focus {
                crate::commands::DataDialogFocus::Overview => "dialog",
                crate::commands::DataDialogFocus::Health => "health",
            });
            Ok(())
        }
        fn request_data_backup(&mut self) -> Result<(), String> {
            self.data_calls.push("backup");
            Ok(())
        }
        fn open_data_folder(&mut self) -> Result<(), String> {
            self.data_calls.push("folder");
            Ok(())
        }
        fn stage_data_recovery(
            &mut self,
            _: &crate::commands::DataRecoveryCommand,
        ) -> Result<(), String> {
            self.data_calls.push("recovery");
            Ok(())
        }
        fn data_launcher_should_refocus(&self) -> bool {
            false
        }
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
    fn command_bus_routes_typed_and_headless_families() {
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
        CommandBus
            .dispatch(
                &invocation(Command::Todo(crate::commands::TodoCommand::View)),
                &mut host,
            )
            .unwrap();
        assert_eq!(host.todo_calls, 1);
        assert_eq!(
            linked_todo.query,
            QueryPolicy::Set("todo links id:7".into())
        );

        CommandBus
            .dispatch(
                &invocation(Command::MouseGesture(
                    crate::commands::MouseGestureCommand::Dialog,
                )),
                &mut host,
            )
            .unwrap();

        CommandBus
            .dispatch(
                &invocation(Command::MultiManager(
                    crate::commands::MultiManagerCommand::Open,
                )),
                &mut host,
            )
            .unwrap();

        CommandBus
            .dispatch(
                &invocation(Command::FileSearch(
                    crate::commands::FileSearchCommand::Open,
                )),
                &mut host,
            )
            .unwrap();
        CommandBus
            .dispatch(
                &invocation(Command::Diff(crate::commands::DiffCommand::Open(
                    crate::diff::query::DiffOpenPayload {
                        left: None,
                        right: None,
                    },
                ))),
                &mut host,
            )
            .unwrap();
        assert_eq!(host.file_search_calls, 1);
        assert_eq!(host.diff_calls, 1);

        CommandBus
            .dispatch(
                &invocation(Command::Screenshot(
                    crate::commands::ScreenshotCommand::UnknownMode {
                        raw: "future".into(),
                    },
                )),
                &mut host,
            )
            .unwrap();
        assert_eq!(host.screenshot_calls, 1);

        let screen_draw = CommandBus
            .dispatch(
                &invocation(Command::ScreenDraw(
                    crate::commands::ScreenDrawCommand::Start,
                )),
                &mut host,
            )
            .unwrap();
        assert_eq!(
            host.screen_draw_calls,
            [crate::commands::ScreenDrawCommand::Start]
        );
        assert_eq!(screen_draw.visibility, VisibilityPolicy::Keep);
        assert_eq!(screen_draw.history, crate::commands::HistoryPolicy::Record);

        CommandBus
            .dispatch(
                &invocation(Command::ClipboardModify(
                    crate::commands::ClipboardModifyCommand::Open {
                        section:
                            crate::clipboard_modify::actions::ClipboardModifySectionPayload::Help,
                    },
                )),
                &mut host,
            )
            .unwrap();
        assert_eq!(host.clipboard_modify_calls, 1);

        CommandBus
            .dispatch(
                &invocation(Command::Data(crate::commands::DataCommand::Backup)),
                &mut host,
            )
            .unwrap();
        assert_eq!(host.data_calls, ["backup"]);

        CommandBus
            .dispatch(
                &invocation(Command::External(crate::commands::ExternalCommand {
                    target: "tool".into(),
                    args: None,
                    namespace: None,
                })),
                &mut host,
            )
            .unwrap();
        assert_eq!(host.headless_calls, 1);
    }
}
