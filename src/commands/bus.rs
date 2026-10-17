use super::{Command, CommandError, CommandHost, CommandInvocation, CommandOutcome};
use crate::commands::handlers::{
    handle_batch_rename, handle_calendar, handle_clipboard_modify_with_history_query,
    handle_color_pick, handle_coordinate_tool, handle_crop, handle_data, handle_diff,
    handle_file_search, handle_hash_generator, handle_headless_gui_with_history_query,
    handle_json_utility, handle_launcher, handle_link, handle_mouse_gesture, handle_multi_manager,
    handle_note, handle_ocr, handle_paste_sequence, handle_query, handle_radial,
    handle_search_replace,
    handle_hotfolder, handle_screen_draw, handle_screenshot, handle_simple_dialog, handle_snippet_run, handle_todo,
    handle_pinboard, handle_xml_toolkit,
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
            Command::XmlToolkit(command) => Ok(handle_xml_toolkit(host, *command)),
            Command::BatchRename(command) => Ok(handle_batch_rename(host, *command)),
            Command::SearchReplace(command) => Ok(handle_search_replace(host, *command)),
            Command::HotFolder(command) => handle_hotfolder(host, *command),
            Command::HashGenerator(command) => Ok(handle_hash_generator(host, command)),
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
            Command::Pinboard(command) => handle_pinboard(host, command),
            Command::ScreenDraw(command) => handle_screen_draw(host, command),
            Command::CoordinateTool(command) => handle_coordinate_tool(host, command),
            Command::ClipboardModify(command) => Ok(handle_clipboard_modify_with_history_query(
                host,
                command,
                invocation,
                captured_history_query,
            )),
            Command::Data(command) => handle_data(host, command),
            Command::PasteSequence(command) => {
                handle_paste_sequence(host, *command, invocation.source)
            }
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
        PinboardCommandHost, RadialCommandHost, TodoCommandHost, VisibilityPolicy,
    };
    use crate::gui::hotfolder_dispatch::{
        LauncherHotFolderActionAdapter, invoke_registered_action_from_catalog,
        pump_launcher_completions_with,
    };
    use crate::hotfolder::{
        ActionEnablement, HotFolderCandidateId,
        HotFolderExternalActionRequest, HotFolderExternalEnableAuthorization,
        HotFolderExternalFileEvidence, HotFolderExecutionPhase, HotFolderTerminalStatus,
        RuleAction, RuleId, bind_registered_launcher_action,
    };
    use crate::universal_actions::{ActionTarget, ResolvedActionTarget};
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::{Duration, SystemTime};

    #[derive(Default)]
    struct FakeHost {
        visible: bool,
        refocus: bool,
        headless_calls: usize,
        headless_failure: bool,
        headless_commands: Vec<(Command, Action)>,
        dialog_calls: usize,
        qr_calls: Vec<Option<String>>,
        crop_calls: usize,
        note_calls: usize,
        todo_calls: usize,
        file_search_calls: usize,
        diff_calls: usize,
        screenshot_calls: usize,
        pinboard_calls: Vec<&'static str>,
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
        xml_toolkit_opens: usize,
        batch_rename_opens: usize,
        search_replace_opens: usize,
        hotfolder_commands: Vec<crate::commands::HotFolderCommand>,
        hash_generator_opens: usize,
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
        fn open_hash_generator_dialog(&mut self) {
            self.hash_generator_opens += 1;
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
    impl PinboardCommandHost for FakeHost {
        fn start_pinboard_capture(&mut self) -> Result<bool, String> {
            self.pinboard_calls.push("capture");
            Ok(true)
        }

        fn open_pinboard_list(&mut self) -> Result<(), String> {
            self.pinboard_calls.push("list");
            Ok(())
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
    impl crate::commands::XmlToolkitCommandHost for FakeHost {
        fn open_xml_toolkit(&mut self) {
            self.xml_toolkit_opens += 1;
        }
    }
    impl crate::commands::BatchRenameCommandHost for FakeHost {
        fn open_batch_rename(&mut self) {
            self.batch_rename_opens += 1;
        }
    }
    impl crate::commands::SearchReplaceCommandHost for FakeHost {
        fn open_search_replace(&mut self) {
            self.search_replace_opens += 1;
        }
    }
    impl crate::commands::HotFolderCommandHost for FakeHost {
        fn execute_hotfolder_command(
            &mut self,
            command: crate::commands::HotFolderCommand,
        ) -> Result<(), String> {
            self.hotfolder_commands.push(command);
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
        fn execute_headless_command(
            &mut self,
            command: &Command,
            original_action: &Action,
        ) -> anyhow::Result<()> {
            self.headless_calls += 1;
            self.headless_commands
                .push((command.clone(), original_action.clone()));
            if self.headless_failure {
                anyhow::bail!("injected headless handler failure");
            }
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
    fn pinboard_commands_dispatch_to_the_typed_gui_host() {
        let mut host = FakeHost::default();
        for command in [
            crate::commands::PinboardCommand::Capture,
            crate::commands::PinboardCommand::List,
        ] {
            let outcome = CommandBus
                .dispatch(&invocation(Command::Pinboard(command)), &mut host)
                .unwrap();
            assert_eq!(outcome.visibility, crate::commands::VisibilityPolicy::Keep);
            assert!(!outcome.focus && !outcome.restore && !outcome.search);
        }
        assert_eq!(host.pinboard_calls, ["capture", "list"]);
        assert_eq!(host.headless_calls, 0);
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
    fn hash_generator_dispatch_uses_the_dialog_host_without_other_side_effects() {
        let mut host = FakeHost::default();
        let invocation = invocation(Command::HashGenerator(
            crate::commands::HashGeneratorCommand::Open,
        ));

        let outcome = CommandBus.dispatch(&invocation, &mut host).unwrap();

        assert_eq!(host.hash_generator_opens, 1);
        assert_eq!(host.headless_calls, 0);
        assert_eq!(outcome.query, QueryPolicy::Keep);
        assert_eq!(outcome.results, crate::commands::ResultsPolicy::Keep);
        assert_eq!(outcome.visibility, VisibilityPolicy::Keep);
        assert_eq!(outcome.history, HistoryPolicy::Skip);
    }

    #[test]
    fn xml_toolkit_plugin_command_routes_once_through_the_typed_bus() {
        let mut manager = crate::plugin::PluginManager::new();
        manager.reload_from_dirs(
            &[],
            10,
            crate::settings::NetUnit::Auto,
            false,
            &std::collections::HashMap::new(),
            std::sync::Arc::new(Vec::new()),
        );

        let actions = manager.search_filtered("xml", None, None);
        let action = actions
            .into_iter()
            .find(|action| action.action == "xml:open")
            .expect("normal plugin construction registers the XML provider");
        assert!(
            action.args.is_none(),
            "the XML command carries no source payload"
        );
        assert!(
            manager
                .commands()
                .iter()
                .any(|command| command.action == "query:xml")
        );
        assert!(
            !manager
                .search_filtered("xmlfoo", None, None)
                .iter()
                .any(|action| action.action == "xml:open")
        );

        let invocation = crate::commands::parse_command(
            action,
            Some("must not become XML input".into()),
            crate::commands::ActivationSource::Dashboard,
        )
        .expect("the plugin wire action has a typed command");
        assert_eq!(
            invocation.command,
            Command::XmlToolkit(crate::commands::XmlToolkitCommand::Open)
        );
        let mut host = FakeHost::default();
        let outcome = CommandBus.dispatch(&invocation, &mut host).unwrap();
        assert_eq!(host.xml_toolkit_opens, 1);
        assert_eq!(host.headless_calls, 0);
        assert_eq!(outcome.query, QueryPolicy::Keep);
        assert_eq!(
            outcome.pending_query,
            crate::commands::PendingQueryPolicy::Keep
        );
        assert_eq!(outcome.results, crate::commands::ResultsPolicy::Keep);
        assert_eq!(outcome.visibility, VisibilityPolicy::Keep);
        assert_eq!(outcome.history, HistoryPolicy::Skip);
    }

    #[test]
    fn m8c_batch_rename_plugin_opens_through_the_typed_bus_without_a_root_payload() {
        let mut manager = crate::plugin::PluginManager::new_inert_for_test();
        manager.reload_from_dirs(
            &[],
            10,
            crate::settings::NetUnit::Auto,
            false,
            &std::collections::HashMap::new(),
            std::sync::Arc::new(Vec::new()),
        );

        let action = manager
            .search_filtered("rename", None, None)
            .into_iter()
            .find(|action| action.action == "rename:open")
            .expect("normal plugin construction registers the batch renamer");
        assert!(
            action.args.is_none(),
            "the command does not carry a root path"
        );
        assert!(action.desc.contains("extensions are protected by default"));
        let plugin_description = manager
            .plugin_infos()
            .into_iter()
            .find(|(name, _, _)| name == "batch_rename")
            .map(|(_, description, _)| description)
            .expect("the built-in plugin is discoverable in plugin metadata");
        assert!(plugin_description.contains("explicit folder"));
        assert!(plugin_description.contains("extensions are protected by default"));
        let help = manager
            .commands()
            .into_iter()
            .find(|command| command.action == "query:rename")
            .expect("the plugin contributes a searchable help entry");
        assert!(help.desc.contains("explicitly chosen folder"));
        assert!(help.desc.contains("extensions are protected by default"));
        assert!(
            !manager
                .search_filtered("rename-me", None, None)
                .iter()
                .any(|action| action.action == "rename:open")
        );

        let invocation = crate::commands::parse_command(
            action,
            Some("must not become a directory root".into()),
            crate::commands::ActivationSource::Dashboard,
        )
        .expect("the plugin wire action has a typed command");
        assert_eq!(
            invocation.command,
            Command::BatchRename(crate::commands::BatchRenameCommand::Open)
        );
        let mut host = FakeHost::default();
        let outcome = CommandBus.dispatch(&invocation, &mut host).unwrap();
        assert_eq!(host.batch_rename_opens, 1);
        assert_eq!(host.headless_calls, 0);
        assert_eq!(outcome.query, QueryPolicy::Keep);
        assert_eq!(outcome.results, crate::commands::ResultsPolicy::Keep);
        assert_eq!(outcome.visibility, VisibilityPolicy::Keep);
        assert_eq!(outcome.history, HistoryPolicy::Skip);
    }

    #[test]
    fn search_replace_plugin_opens_once_through_the_typed_bus_without_a_path_payload() {
        let mut manager = crate::plugin::PluginManager::new_inert_for_test();
        manager.reload_from_dirs(
            &[],
            10,
            crate::settings::NetUnit::Auto,
            false,
            &std::collections::HashMap::new(),
            std::sync::Arc::new(Vec::new()),
        );

        let action = manager
            .search_filtered("replace", None, None)
            .into_iter()
            .find(|action| action.action == "replace:open")
            .expect("normal plugin construction registers Search and Replace");
        assert!(action.args.is_none(), "the command carries no path or pattern");
        assert!(manager
            .commands()
            .iter()
            .any(|command| command.action == "query:replace"));
        assert!(manager
            .plugin_names()
            .iter()
            .any(|name| name == "search_replace"));
        assert!(manager
            .search_filtered("replacefoo", None, None)
            .iter()
            .all(|action| action.action != "replace:open"));

        let invocation = crate::commands::parse_command(
            action,
            Some("must not become file content".into()),
            crate::commands::ActivationSource::Dashboard,
        )
        .expect("the plugin wire action has a typed command");
        assert_eq!(
            invocation.command,
            Command::SearchReplace(crate::commands::SearchReplaceCommand::Open)
        );
        let mut host = FakeHost::default();
        let outcome = CommandBus.dispatch(&invocation, &mut host).unwrap();
        assert_eq!(host.search_replace_opens, 1);
        assert_eq!(host.headless_calls, 0);
        assert_eq!(outcome.query, QueryPolicy::Keep);
        assert_eq!(outcome.results, crate::commands::ResultsPolicy::Keep);
        assert_eq!(outcome.visibility, VisibilityPolicy::Keep);
        assert_eq!(outcome.history, HistoryPolicy::Skip);
    }

    #[test]
    fn hotfolder_provider_and_payload_free_controls_use_the_typed_bus() {
        let mut manager = crate::plugin::PluginManager::new_inert_for_test();
        manager.reload_from_dirs(
            &[],
            10,
            crate::settings::NetUnit::Auto,
            false,
            &std::collections::HashMap::new(),
            std::sync::Arc::new(Vec::new()),
        );
        let open = manager
            .search_filtered("hotfolder", None, None)
            .into_iter()
            .find(|action| action.action == "hotfolder:open")
            .expect("production registration exposes the manager action");
        assert!(open.args.is_none());
        let help = manager
            .commands()
            .into_iter()
            .find(|action| action.action == "query:hotfolder")
            .expect("the provider contributes an in-app help entry");
        let normalized_help = help.desc.to_ascii_lowercase();
        for requirement in [
            "rules start disabled",
            "startup never scans or backfills",
            "test rule reads sample metadata only",
            "zero execution effects",
        ] {
            assert!(normalized_help.contains(requirement));
        }

        let mut host = FakeHost::default();
        let open_invocation = crate::commands::parse_command(
            open,
            None,
            ActivationSource::Enter,
        )
        .unwrap();
        assert_eq!(open_invocation.command, Command::HotFolder(crate::commands::HotFolderCommand::Open));
        let outcome = CommandBus.dispatch(&open_invocation, &mut host).unwrap();
        assert_eq!(outcome.visibility, VisibilityPolicy::Show);
        assert_eq!(host.hotfolder_commands, vec![crate::commands::HotFolderCommand::Open]);

        for raw in ["hotfolder:pause", "hotfolder:resume"] {
            let invocation = crate::commands::parse_command(
                Action { label: raw.into(), desc: String::new(), action: raw.into(), args: None },
                None,
                ActivationSource::Enter,
            )
            .unwrap();
            CommandBus.dispatch(&invocation, &mut host).unwrap();
        }
        assert_eq!(
            host.hotfolder_commands,
            vec![
                crate::commands::HotFolderCommand::Open,
                crate::commands::HotFolderCommand::Pause,
                crate::commands::HotFolderCommand::Resume,
            ]
        );
        assert_eq!(host.headless_calls, 0);
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

    fn registered_launcher_fixture() -> (ResolvedActionTarget, RuleAction) {
        let selected = Action {
            label: "Éditeur".into(),
            desc: "Application".into(),
            action: r"C:\Program Files\Éditeur\editor.exe".into(),
            args: Some("--open".into()),
        };
        let target = ResolvedActionTarget {
            target: ActionTarget::Generic {
                action: selected.clone(),
            },
            selected_action: selected,
            custom_action_index: None,
        };
        let binding =
            bind_registered_launcher_action(&target).expect("stable exact launcher binding");
        (target, binding)
    }

    fn launcher_request(action: RuleAction, operation_id: u128) -> HotFolderExternalActionRequest {
        let rule_fingerprint = match &action {
            RuleAction::RegisteredLauncherAction {
                configuration_fingerprint,
                ..
            }
            | RuleAction::MkMacro {
                configuration_fingerprint,
                ..
            } => configuration_fingerprint.clone(),
            RuleAction::Copy { .. } | RuleAction::Move { .. } | RuleAction::Rename { .. } => {
                unreachable!("fixture requests must be external")
            }
        };
        HotFolderExternalActionRequest {
            operation_id,
            candidate_id: HotFolderCandidateId::from_persisted(operation_id as u64),
            rule_id: RuleId::parse(format!("{operation_id:032x}")).expect("valid rule ID"),
            rule_fingerprint: rule_fingerprint.clone(),
            action,
            authorization: HotFolderExternalEnableAuthorization {
                store_revision: 1,
                epoch: 1,
                rule_fingerprint,
                enablement: ActionEnablement::StrongConfirmationRequired,
            },
            file: HotFolderExternalFileEvidence {
                path: PathBuf::from(r"C:\watch\résumé.txt"),
                root_identity: crate::file_operations::NativeFileIdentity::default(),
                parent_identity: crate::file_operations::NativeFileIdentity::default(),
                file_identity: crate::file_operations::NativeFileIdentity::default(),
                length: 19,
                modified: SystemTime::UNIX_EPOCH,
                sha256: None,
            },
        }
    }

    #[test]
    fn hotfolder_launcher_queue_revalidates_exact_target_and_uses_typed_bus() {
        let macro_store = Arc::new(crate::mkmacro::MkMacroStore::empty_for_test(
            std::env::temp_dir(),
        ));
        let (adapter, pump) = LauncherHotFolderActionAdapter::new(
            eframe::egui::Context::default(),
            macro_store,
        );
        let (target, action) = registered_launcher_fixture();
        let entries = vec![target.clone()];
        let request = launcher_request(action.clone(), 144);

        let cancelled = adapter
            .enqueue_launcher_request(launcher_request(action.clone(), 145))
            .expect("pending launcher operation is queued");
        assert!(cancelled.cancel_if_pending());
        let mut cancelled_invocations = 0;
        pump_launcher_completions_with(&pump, |_| {
            cancelled_invocations += 1;
            unreachable!("a pending cancellation must not reach the launcher")
        });
        let cancelled_outcome = cancelled
            .receive_timeout(Duration::from_millis(1))
            .expect("pump completes the revoked request");
        assert_eq!(cancelled_outcome.status, HotFolderTerminalStatus::Cancelled);
        assert_eq!(cancelled_outcome.phase, HotFolderExecutionPhase::Admission);
        assert_eq!(cancelled_invocations, 0);

        let queued = adapter
            .enqueue_launcher_request(request.clone())
            .expect("exact launcher operation is queued");
        let mut host = FakeHost::default();
        let mut command_outcome = None;
        pump_launcher_completions_with(&pump, |pending| {
            invoke_registered_action_from_catalog(pending, &entries, |invocation| {
                assert_eq!(invocation.source, ActivationSource::HotFolder);
                assert_eq!(invocation.query_override, None);
                let outcome = CommandBus.dispatch(invocation, &mut host);
                command_outcome = outcome.as_ref().ok().cloned();
                outcome
            })
        });
        let completed = queued
            .receive_timeout(Duration::from_millis(1))
            .expect("typed bus result completes the queued request");
        assert_eq!(completed.operation_id, request.operation_id);
        assert_eq!(completed.status, HotFolderTerminalStatus::Success);
        assert_eq!(host.headless_calls, 1);
        assert_eq!(
            host.headless_commands,
            vec![(
                Command::External(crate::commands::ExternalCommand {
                    target: r"C:\Program Files\Éditeur\editor.exe".into(),
                    args: Some("--open".into()),
                    namespace: None,
                }),
                target.selected_action.clone(),
            )]
        );
        let command_outcome = command_outcome.expect("canonical CommandBus returned an outcome");
        assert_eq!(command_outcome.query, QueryPolicy::Keep);
        assert_eq!(command_outcome.visibility, VisibilityPolicy::Keep);

        let deleted = invoke_registered_action_from_catalog(&request, &[], |_| {
            unreachable!("a deleted target must be refused before typed dispatch")
        });
        assert_eq!(deleted.status, HotFolderTerminalStatus::Failure);
        assert_eq!(host.headless_calls, 1);

        let mut failing_host = FakeHost {
            headless_failure: true,
            ..FakeHost::default()
        };
        let failed = invoke_registered_action_from_catalog(&request, &entries, |invocation| {
            CommandBus.dispatch(invocation, &mut failing_host)
        });
        assert_eq!(failed.status, HotFolderTerminalStatus::Failure);
        assert_eq!(failing_host.headless_calls, 1);
        assert_eq!(failing_host.headless_commands, host.headless_commands);
        assert_eq!(request.file.path, PathBuf::from(r"C:\watch\résumé.txt"));
    }
}
