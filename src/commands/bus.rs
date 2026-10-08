use super::{Command, CommandError, CommandHost, CommandInvocation, CommandOutcome};
use crate::commands::handlers::{
    handle_calendar, handle_clipboard_modify_with_history_query, handle_color_pick,
    handle_coordinate_tool, handle_crop, handle_data, handle_diff, handle_file_search,
    handle_headless_gui_with_history_query, handle_json_utility, handle_launcher, handle_link,
    handle_mouse_gesture, handle_multi_manager, handle_note, handle_ocr, handle_query,
    handle_radial, handle_screen_draw, handle_screenshot, handle_simple_dialog, handle_snippet_run,
    handle_todo,
};

#[derive(Debug, Default)]
pub struct CommandBus;

impl CommandBus {
    pub fn dispatch(
        &self,
        invocation: &CommandInvocation,
        host: &mut dyn CommandHost,
    ) -> Result<CommandOutcome, CommandError> {
        self.dispatch_with_history_query(invocation, host, None)
    }

    pub fn dispatch_with_history_query(
        &self,
        invocation: &CommandInvocation,
        host: &mut dyn CommandHost,
        captured_history_query: Option<&str>,
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
            Command::Radial(command) => handle_radial(host, command),
            Command::Query(command) => Ok(handle_query(command, invocation.source)),
            Command::Crop(command) => Ok(handle_crop(host, command)),
            Command::JsonUtility(command) => Ok(handle_json_utility(host, command)),
            Command::ColorPick(command) => handle_color_pick(host, command),
            Command::Ocr(command) => handle_ocr(host, command),
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
            Command::CoordinateTool(command) => handle_coordinate_tool(host, command),
            Command::ClipboardModify(command) => Ok(handle_clipboard_modify_with_history_query(
                host,
                command,
                invocation,
                captured_history_query,
            )),
            Command::Data(command) => handle_data(host, command),
            Command::Storage(
                super::StorageCommand::SnippetRun(_) | super::StorageCommand::InvalidSnippetRun,
            ) => handle_snippet_run(host, invocation, captured_history_query),
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
            | Command::External(_) => {
                handle_headless_gui_with_history_query(host, invocation, captured_history_query)
            }
            Command::Dialog(_) => unreachable!("dialog commands are handled before dispatch"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::Action;
    use crate::commands::{
        ActivationSource, CalendarCommandHost, CoordinateToolCommand, CropCommandHost,
        DialogCommandHost, HeadlessCommandHost, HistoryPolicy, LauncherCommand,
        LauncherCommandHost, MultiManagerCommandHost, NoteCommandHost, QueryCommand, QueryPolicy,
        RadialCommandHost, TodoCommandHost, VisibilityPolicy,
    };

    #[derive(Default)]
    struct FakeHost {
        visible: bool,
        refocus: bool,
        headless_calls: usize,
        dialog_calls: usize,
        qr_calls: Vec<Option<String>>,
        crop_calls: usize,
        note_calls: usize,
        todo_calls: usize,
        file_search_calls: usize,
        diff_calls: usize,
        screenshot_calls: usize,
        screen_draw_calls: Vec<crate::commands::ScreenDrawCommand>,
        coordinate_tool_calls: Vec<CoordinateToolCommand>,
        mouse_settings_calls: usize,
        clipboard_modify_calls: usize,
        clipboard_modify_metadata:
            Option<crate::clipboard_modify::coordinator::ImmediateRequestMetadata>,
        data_calls: Vec<&'static str>,
        color_pick_requests: usize,
        ocr_requests: usize,
        json_utility_intents: Vec<crate::commands::JsonUtilityIntent>,
        radial_calls: usize,
    }

    impl LauncherCommandHost for FakeHost {
        fn launcher_is_visible(&self) -> bool {
            self.visible
        }

        fn open_mouse_settings(&mut self) -> Result<(), String> {
            self.mouse_settings_calls += 1;
            Ok(())
        }

        fn execute_coordinate_tool_command(
            &mut self,
            command: &CoordinateToolCommand,
        ) -> Result<Option<String>, String> {
            self.coordinate_tool_calls.push(command.clone());
            Ok(None)
        }
    }

    impl RadialCommandHost for FakeHost {
        fn radial_is_enabled(&self) -> bool {
            true
        }
        fn request_radial_control(
            &mut self,
            _: crate::radial::control::RadialControlRequest,
        ) -> Result<(), String> {
            self.radial_calls += 1;
            Ok(())
        }
        fn open_radial_editor(&mut self, _: bool) {}
    }

    impl DialogCommandHost for FakeHost {
        fn open_qr_dialog(&mut self, initial_text: Option<&str>) {
            self.qr_calls.push(initial_text.map(str::to_owned));
        }
        fn open_regex_tester_dialog(&mut self) {}
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
    impl crate::commands::ColorPickCommandHost for FakeHost {
        fn start_color_pick(&mut self) -> Result<bool, String> {
            self.color_pick_requests += 1;
            Ok(true)
        }
    }
    impl crate::commands::OcrCommandHost for FakeHost {
        fn start_ocr_selection(&mut self) -> Result<bool, String> {
            self.ocr_requests += 1;
            Ok(true)
        }
    }
    impl crate::commands::JsonUtilityCommandHost for FakeHost {
        fn open_json_utility(&mut self, intent: crate::commands::JsonUtilityIntent) {
            self.json_utility_intents.push(intent);
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
            metadata: crate::clipboard_modify::coordinator::ImmediateRequestMetadata,
        ) -> Result<(), String> {
            self.clipboard_modify_calls += 1;
            self.clipboard_modify_metadata = Some(metadata);
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
            self.refocus
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
    fn qr_bus_opens_once_with_exact_payload_and_skips_history() {
        for visible in [false, true] {
            let mut host = FakeHost {
                visible,
                refocus: visible,
                ..FakeHost::default()
            };
            let source = "  caf\u{e9}\n\u{1f512} ";
            let command = Command::Dialog(crate::commands::DialogCommand::Qr {
                initial_text: Some(source.into()),
            });
            let outcome = CommandBus
                .dispatch(&invocation(command), &mut host)
                .unwrap();
            assert_eq!(host.qr_calls, vec![Some(source.to_owned())]);
            assert_eq!(host.headless_calls, 0);
            assert_eq!(outcome.history, crate::commands::HistoryPolicy::Skip);
            assert_eq!(outcome.focus, visible);
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

        let radial = CommandBus
            .dispatch(
                &invocation(Command::Radial(crate::commands::RadialCommand::ShowDefault)),
                &mut host,
            )
            .unwrap();
        assert_eq!(
            radial,
            CommandOutcome {
                history: HistoryPolicy::Record,
                ..CommandOutcome::default()
            }
        );
        assert_eq!(host.radial_calls, 1);

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

        let coordinate = CommandBus
            .dispatch(
                &invocation(Command::CoordinateTool(CoordinateToolCommand::ToggleHud)),
                &mut host,
            )
            .unwrap();
        assert_eq!(
            host.coordinate_tool_calls,
            [CoordinateToolCommand::ToggleHud]
        );
        assert_eq!(coordinate, CommandOutcome::default());

        let settings = CommandBus
            .dispatch(
                &invocation(Command::CoordinateTool(CoordinateToolCommand::Settings)),
                &mut host,
            )
            .unwrap();
        assert_eq!(host.mouse_settings_calls, 1);
        assert_eq!(
            host.coordinate_tool_calls,
            [CoordinateToolCommand::ToggleHud]
        );
        assert_eq!(settings, CommandOutcome::default());

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

    #[test]
    fn invalid_snippet_run_uses_the_typed_handler_without_headless_fallback() {
        let mut host = FakeHost::default();
        let error = CommandBus
            .dispatch(
                &invocation(Command::Storage(
                    super::super::StorageCommand::InvalidSnippetRun,
                )),
                &mut host,
            )
            .unwrap_err();

        assert!(error.message.contains("invalid snippet run action"));
        assert_eq!(host.headless_calls, 0);
    }

    #[test]
    fn json_utility_dispatch_opens_the_requested_mode_without_other_side_effects() {
        let mut host = FakeHost::default();
        let invocation = invocation(Command::JsonUtility(
            crate::commands::JsonUtilityCommand::Open {
                intent: crate::commands::JsonUtilityIntent::Minify,
            },
        ));

        let outcome = CommandBus.dispatch(&invocation, &mut host).unwrap();

        assert_eq!(
            host.json_utility_intents,
            [crate::commands::JsonUtilityIntent::Minify]
        );
        assert_eq!(host.clipboard_modify_calls, 0);
        assert!(host.data_calls.is_empty());
        assert_eq!(outcome.query, QueryPolicy::Keep);
        assert_eq!(outcome.results, crate::commands::ResultsPolicy::Keep);
        assert_eq!(outcome.visibility, VisibilityPolicy::Keep);
        assert_eq!(outcome.history, HistoryPolicy::Skip);
    }

    #[test]
    fn radial_captured_query_reaches_async_clipboard_modify_metadata() {
        let mut host = FakeHost::default();
        let invocation = invocation(Command::ClipboardModify(
            crate::commands::ClipboardModifyCommand::Execute {
                payload: Some(
                    crate::clipboard_modify::actions::ClipboardModifyActionPayload::ExecuteTemplate {
                        canonical_command: "cm template uppercase".into(),
                        name: "uppercase".into(),
                    },
                ),
                raw_argument: None,
                payload_error: None,
            },
        ));
        CommandBus
            .dispatch_with_history_query(&invocation, &mut host, Some("radial saved query"))
            .unwrap();

        let metadata = host
            .clipboard_modify_metadata
            .expect("async metadata reaches the clipboard host");
        assert_eq!(
            metadata.history_query.as_deref(),
            Some("radial saved query")
        );
        assert_eq!(metadata.query, "cm template uppercase");
        assert_eq!(metadata.source, ActivationSource::Dashboard);
        assert_eq!(
            metadata.root_policy,
            crate::universal_actions::RootLauncherPolicy::Legacy
        );
    }
    #[test]
    fn color_pick_dispatch_routes_once_without_clipboard_or_visibility_effects() {
        let mut host = FakeHost::default();
        let request = invocation(Command::ColorPick(crate::commands::ColorPickCommand::Pick));
        let outcome = CommandBus.dispatch(&request, &mut host).unwrap();
        assert_eq!(host.color_pick_requests, 1);
        assert_eq!(host.clipboard_modify_calls, 0);
        assert_eq!(outcome.query, QueryPolicy::Keep);
        assert_eq!(outcome.visibility, VisibilityPolicy::Keep);
        assert_eq!(outcome.history, HistoryPolicy::Record);
    }
    #[test]
    fn ocr_dispatch_routes_once_to_gui_host_without_headless_or_clipboard_effects() {
        let mut host = FakeHost::default();
        let request = invocation(Command::Ocr(crate::commands::OcrCommand::Start));
        let outcome = CommandBus.dispatch(&request, &mut host).unwrap();
        assert_eq!(host.ocr_requests, 1);
        assert_eq!(host.headless_calls, 0);
        assert_eq!(host.clipboard_modify_calls, 0);
        assert_eq!(outcome.query, QueryPolicy::Keep);
        assert_eq!(outcome.visibility, VisibilityPolicy::Keep);
        assert_eq!(outcome.history, HistoryPolicy::Record);
    }
}
