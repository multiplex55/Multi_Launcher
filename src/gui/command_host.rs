use crate::dashboard::DashboardRefreshRequest;
use std::sync::atomic::Ordering;

use crate::commands::{
    BatchRenameCommandHost, CalendarCommandHost, ClipboardModifyCommandHost, ColorPickCommandHost,
    Command, CommandError, CommandInvocation, CommandOutcome, CropCommandHost, DataCommandHost,
    DialogCommandHost, DiffCommandHost, FavoriteLogPolicy, FileSearchCommandHost,
    HeadlessCommandHost, HistoryPolicy, JsonUtilityCommandHost, LauncherCommandHost,
    MouseGestureCommandHost, MultiManagerCommandHost, NoteCommandHost, PendingQueryPolicy,
    QueryPolicy, RadialCommandHost, ResultsPolicy, ScreenDrawCommandHost, ScreenshotCommandHost,
    ScreenshotCommandResult, ScreenshotDestination, ScreenshotMarkup, ScreenshotMode, ToastPolicy,
    TodoCommandHost, VisibilityPolicy, XmlToolkitCommandHost,
};

use super::{LauncherApp, Toast, ToastKind, ToastOptions};
use crate::performance::track_c::EventOrigin;

fn sequence_source_requirements(sequence: &crate::paste_sequence::Sequence) -> (bool, bool) {
    let mut snippets = false;
    let mut history = false;
    for step in &sequence.steps {
        match step {
            crate::paste_sequence::Step::SnippetRef { .. } => snippets = true,
            crate::paste_sequence::Step::ClipboardSnapshotOrRef { .. } => history = true,
            crate::paste_sequence::Step::LiteralText { .. } => {}
        }
    }
    (snippets, history)
}

impl LauncherCommandHost for LauncherApp {
    fn launcher_is_visible(&self) -> bool {
        self.visible_flag.load(Ordering::SeqCst)
    }

    fn open_mouse_settings(&mut self) -> Result<(), String> {
        self.focus_panel(super::Panel::MouseSettingsDialog);
        if !self.visible_flag.load(Ordering::SeqCst) {
            self.request_launcher_visibility(true);
        }
        Ok(())
    }

    fn execute_coordinate_tool_command(
        &mut self,
        command: &crate::commands::CoordinateToolCommand,
    ) -> Result<Option<String>, String> {
        match command {
            crate::commands::CoordinateToolCommand::Pick => self.begin_coordinate_pick().map(Some),
            crate::commands::CoordinateToolCommand::Cancel => Ok(self.cancel_coordinate_pick()),
            _ => self.coordinate_tool.execute(command),
        }
    }
}

fn split_virtual_desktop_outcome(
    mut completion_outcome: CommandOutcome,
) -> (CommandOutcome, CommandOutcome) {
    let immediate_outcome = CommandOutcome {
        query: std::mem::replace(&mut completion_outcome.query, QueryPolicy::Keep),
        pending_query: std::mem::replace(
            &mut completion_outcome.pending_query,
            PendingQueryPolicy::Keep,
        ),
        search: std::mem::take(&mut completion_outcome.search),
        invalidate_results: std::mem::take(&mut completion_outcome.invalidate_results),
        results: std::mem::replace(&mut completion_outcome.results, ResultsPolicy::Keep),
        visibility: std::mem::replace(&mut completion_outcome.visibility, VisibilityPolicy::Keep),
        restore: std::mem::take(&mut completion_outcome.restore),
        focus: std::mem::take(&mut completion_outcome.focus),
        move_cursor_end: std::mem::take(&mut completion_outcome.move_cursor_end),
        activate_first_result: completion_outcome.activate_first_result.take(),
        ..CommandOutcome::default()
    };
    (immediate_outcome, completion_outcome)
}

fn normalize_outcome_for_root_policy(
    outcome: CommandOutcome,
    root_policy: crate::universal_actions::RootLauncherPolicy,
) -> CommandOutcome {
    if root_policy == crate::universal_actions::RootLauncherPolicy::Legacy {
        return outcome;
    }

    // These policies all mutate the ordinary launcher surface. Preserve the
    // non-ROOT completion effects while leaving query, results, focus, and
    // visibility decisions to their existing owners.
    CommandOutcome {
        history: outcome.history,
        toasts: outcome.toasts,
        favorite_log: outcome.favorite_log,
        ..CommandOutcome::default()
    }
}

impl RadialCommandHost for LauncherApp {
    fn radial_is_enabled(&self) -> bool {
        super::radial_control_client().is_some_and(|client| client.is_enabled())
    }

    fn request_radial_control(
        &mut self,
        request: crate::radial::control::RadialControlRequest,
    ) -> Result<(), String> {
        if self.coordinate_pick_blocks_other_screen_actions()
            && matches!(
                &request,
                crate::radial::control::RadialControlRequest::Show(_)
            )
        {
            return Err("Finish or cancel coordinate picking before opening a radial menu".into());
        }
        super::radial_control_client()
            .ok_or_else(|| "radial runtime service is unavailable".to_owned())?
            .send(request)
            .map_err(|error| error.to_string())
    }

    fn open_radial_editor(&mut self, skins: bool) {
        crate::radial::acceptance_trace::emit(
            crate::radial::acceptance_trace::Event::RadialAction {
                stage: crate::radial::acceptance_trace::RadialActionStage::HostEntered,
                skins,
                editor_open: None,
                skins_selected: None,
                panel_registered: None,
            },
        );
        self.focus_panel(super::Panel::RadialEditor);
        if let Ok(mut editor) = self.radial_editor.lock() {
            if skins {
                editor.open_skins();
            } else {
                editor.open_menus();
            }
        }
        self.panel_states.radial_editor = true;
        crate::radial::acceptance_trace::emit(
            crate::radial::acceptance_trace::Event::RadialAction {
                stage: crate::radial::acceptance_trace::RadialActionStage::HostCompleted,
                skins,
                editor_open: None,
                skins_selected: None,
                panel_registered: Some(self.panel_states.radial_editor),
            },
        );
    }
}

impl ScreenDrawCommandHost for LauncherApp {
    fn execute_screen_draw_command(
        &mut self,
        command: crate::commands::ScreenDrawCommand,
    ) -> Result<(), String> {
        use crate::commands::ScreenDrawCommand;

        match command {
            ScreenDrawCommand::Start => self.start_or_focus_screen_draw().map(|_| ()),
            ScreenDrawCommand::OpenToolbar => self.focus_screen_draw_toolbar(),
            ScreenDrawCommand::NewCapture => self.request_new_screen_draw_capture(),
            ScreenDrawCommand::Ghost => self
                .screen_draw_controller
                .enter_ghost()
                .map_err(|error| error.to_string()),
            ScreenDrawCommand::Done => self
                .screen_draw_controller
                .finish()
                .map_err(|error| error.to_string()),
            ScreenDrawCommand::Clear => self
                .screen_draw_controller
                .request_clear()
                .map_err(|error| error.to_string()),
            ScreenDrawCommand::Close => self.close_screen_draw_session(),
        }
    }
}

impl DialogCommandHost for LauncherApp {
    fn open_qr_dialog(&mut self, initial_text: Option<&str>) {
        self.qr_dialog.open(initial_text);
        self.focus_panel(super::Panel::QrDialog);
    }
    fn open_regex_tester_dialog(&mut self) {
        self.regex_tester_dialog.open();
        self.focus_panel(super::Panel::RegexTesterDialog);
    }
    fn open_help_dialog(&mut self) {
        self.help_window.open = true;
    }

    fn open_timer_dialog(&mut self) {
        self.timer_dialog.open_timer();
    }

    fn open_alarm_dialog(&mut self) {
        self.timer_dialog.open_alarm();
    }

    fn open_shell_dialog(&mut self) {
        self.shell_cmd_dialog.open();
    }

    fn open_bookmark_dialog(&mut self) {
        self.add_bookmark_dialog.open();
    }

    fn open_snippet_dialog(&mut self) {
        self.snippet_dialog.open();
    }

    fn open_paste_sequence_dialog(&mut self) {
        let context = self.capture_sequence_context(crate::commands::ActivationSource::Click);
        self.paste_sequence_dialog.open_with_context(context);
        self.focus_panel(super::Panel::PasteSequenceDialog);
    }

    fn open_hash_generator_dialog(&mut self) {
        self.hash_generator_dialog.ensure_open();
        self.focus_panel(super::Panel::HashGeneratorDialog);
    }

    fn run_paste_sequence(
        &mut self,
        id: crate::paste_sequence::SequenceId,
        source: crate::commands::ActivationSource,
    ) -> Result<(), String> {
        let snapshot =
            crate::paste_sequence::SequenceStore::new(crate::paste_sequence::PASTE_SEQUENCES_FILE)
                .load()
                .map_err(|error| format!("Saved sequence could not be loaded: {error}"))?;
        let sequence = snapshot
            .sequence(id)
            .cloned()
            .ok_or_else(|| "Saved sequence no longer exists.".to_owned())?;
        let (requires_snippets, requires_history) = sequence_source_requirements(&sequence);
        let snippets = if requires_snippets {
            match crate::plugins::snippets::load_snippets_for_reload(
                crate::plugins::snippets::SNIPPETS_FILE,
            )
            .map_err(|_| "Snippet catalog could not be read.".to_owned())?
            {
                crate::common::persistence::LoadState::Missing
                | crate::common::persistence::LoadState::Empty => Vec::new(),
                crate::common::persistence::LoadState::Loaded(entries) => entries,
            }
        } else {
            Vec::new()
        };
        let history = if requires_history {
            crate::plugins::clipboard::load_history(crate::plugins::clipboard::CLIPBOARD_FILE)
                .map_err(|_| "Clipboard history could not be read.".to_owned())?
                .into_iter()
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        self.paste_sequence_dialog.set_autostart_when_ready(true);
        if let Err(error) =
            self.begin_paste_sequence_preflight(sequence, &snippets, &history, source)
        {
            self.paste_sequence_dialog.set_autostart_when_ready(false);
            return Err(format!("Paste sequence could not be prepared: {error:?}"));
        }
        if !self.snippet_prompt_dialog.is_open() {
            self.paste_sequence_dialog.set_autostart_when_ready(false);
            self.start_sequence_playback()
                .map_err(|error| error.message().to_owned())?;
        }
        Ok(())
    }

    fn start_paste_sequence(&mut self) -> Result<(), String> {
        self.start_sequence_playback()
            .map_err(|error| error.message().to_owned())
    }

    fn next_paste_sequence(&mut self) -> Result<(), String> {
        self.next_sequence_playback()
            .map_err(|error| error.message().to_owned())
    }

    fn cancel_paste_sequence(&mut self) {
        self.cancel_active_paste_sequence();
        self.paste_sequence_dialog.set_autostart_when_ready(false);
    }

    fn open_snippet_editor(&mut self, alias: &str) {
        self.snippet_dialog.open_edit(alias);
    }

    fn open_favorite_dialog(&mut self, label: &str) {
        if label.is_empty() {
            self.fav_dialog.open();
        } else {
            self.fav_dialog.open_edit(label);
        }
    }

    fn open_legacy_macro_dialog(&mut self) {
        self.macro_dialog.open();
    }

    fn open_mkmacro_dialog(&mut self) {
        self.mkmacro_dialog.open();
    }

    fn open_todo_dialog(&mut self) {
        self.todo_dialog.open();
    }

    fn open_clipboard_dialog(&mut self) {
        self.clipboard_dialog.open();
    }

    fn open_convert_dialog(&mut self) {
        self.convert_panel.open();
    }

    fn open_tempfile_dialog(&mut self) {
        self.tempfile_dialog.open();
    }

    fn open_settings_dialog(&mut self) {
        LauncherApp::open_settings_dialog(self);
    }

    fn open_dashboard_settings_dialog(&mut self) {
        let registry = self.dashboard.registry().clone();
        self.dashboard_editor.open(&self.dashboard_path, &registry);
        self.show_dashboard_editor = true;
    }

    fn open_theme_dialog(&mut self) {
        self.open_theme_settings_dialog();
    }

    fn open_volume_dialog(&mut self) {
        self.volume_dialog.open();
    }

    fn open_brightness_dialog(&mut self) {
        self.brightness_dialog.open();
    }

    fn open_cpu_list_dialog(&mut self, count: usize) {
        self.cpu_list_dialog.open(count);
    }
}

impl LauncherApp {
    pub(crate) fn handle_paste_sequence_hotkey(
        &mut self,
        action: crate::hotkey::launcher_invocation::PasteSequenceHotkeyAction,
    ) {
        use crate::hotkey::launcher_invocation::PasteSequenceHotkeyAction as HotkeyAction;
        use crate::paste_sequence::SequencePlaybackStatus as Status;

        match action {
            HotkeyAction::Start(id) => {
                if let Err(error) =
                    self.run_paste_sequence(id, crate::commands::ActivationSource::Hotkey)
                {
                    self.report_error_message("paste_sequence.hotkey", error);
                }
            }
            HotkeyAction::Next if self.sequence_playback.status() == Status::AwaitAdvance => {
                if let Err(error) = self.next_sequence_playback() {
                    self.report_error_message("paste_sequence.hotkey", error.message());
                }
            }
            HotkeyAction::Next => {}
            HotkeyAction::Cancel => {
                self.cancel_active_paste_sequence();
                self.paste_sequence_dialog.set_autostart_when_ready(false);
            }
        }
    }
}

impl CalendarCommandHost for LauncherApp {
    fn calendar_dashboard_enabled(&self) -> bool {
        self.dashboard_enabled
    }

    fn calendar_preserve_command(&self) -> bool {
        self.preserve_command
    }

    fn open_calendar_popover(&mut self, date: chrono::NaiveDate) {
        LauncherApp::open_calendar_popover(self, Some(date));
    }

    fn refresh_calendar_cache(&mut self) {
        self.dashboard_data_cache
            .request_refresh(DashboardRefreshRequest::Calendar);
    }
}

impl NoteCommandHost for LauncherApp {
    fn open_notes_dialog(&mut self) {
        self.notes_dialog.open();
    }

    fn open_note_graph_dialog(&mut self, args: Option<&str>) {
        self.note_graph_dialog.open_with_args(args);
    }

    fn open_unused_note_assets_dialog(&mut self) {
        self.unused_assets_dialog.open();
    }

    fn open_note_panel(&mut self, slug: &str, template: Option<&str>) {
        LauncherApp::open_note_panel(self, slug, template);
    }

    fn open_note_tags(&mut self) {
        LauncherApp::open_note_tags(self);
    }

    fn open_note_link(&mut self, link: &str) {
        LauncherApp::open_note_link(self, link);
    }

    fn wrap_note_plain_links(&mut self, slug: &str) {
        LauncherApp::wrap_note_plain_links(self, slug);
    }

    fn delete_note(&mut self, slug: &str) {
        LauncherApp::delete_note(self, slug);
    }
}

impl TodoCommandHost for LauncherApp {
    fn open_todo_view(&mut self) {
        self.todo_view_dialog.open();
    }

    fn open_todo_editor(&mut self, index: usize) {
        self.todo_view_dialog.open_edit(index);
    }
}

impl MouseGestureCommandHost for LauncherApp {
    fn open_mouse_gesture_dialog(&mut self) {
        self.mouse_gestures_dialog.open();
    }

    fn open_mouse_gesture_add_dialog(&mut self) {
        self.mouse_gestures_dialog.open_add();
    }

    fn open_mouse_gesture_binding_dialog(&mut self) {
        self.mouse_gestures_dialog.open_binding_editor();
    }

    fn open_mouse_gesture_focus(
        &mut self,
        args: &crate::mouse_gestures::selection::GestureFocusArgs,
    ) {
        self.mouse_gestures_dialog
            .open_focus(&args.label, &args.tokens, args.dir_mode);
    }

    fn open_mouse_gesture_settings_dialog(&mut self) {
        LauncherApp::open_mouse_gesture_settings_dialog(self);
    }

    fn set_mouse_gesture_enabled(
        &mut self,
        args: &crate::mouse_gestures::selection::GestureToggleArgs,
    ) -> Result<(), String> {
        let committed = crate::mouse_gestures::db::update_gestures(
            crate::mouse_gestures::db::GESTURES_FILE,
            |db| {
                let Some(gesture) = db.gestures.iter_mut().find(|gesture| {
                    gesture.label == args.label
                        && gesture.tokens == args.tokens
                        && gesture.dir_mode == args.dir_mode
                }) else {
                    return Ok(false);
                };
                if gesture.enabled == args.enabled {
                    return Ok(false);
                }
                gesture.enabled = args.enabled;
                Ok(true)
            },
        )
        .map_err(|error| error.to_string())?;
        crate::plugins::mouse_gestures::publish_committed_gesture_db(committed);
        self.dashboard_data_cache
            .request_refresh(DashboardRefreshRequest::Gestures);
        Ok(())
    }

    fn mouse_gesture_launcher_should_refocus(&self) -> bool {
        self.visible_flag.load(Ordering::SeqCst) && !self.any_panel_open()
    }
}
impl MultiManagerCommandHost for LauncherApp {
    fn open_multi_manager(&mut self) {
        LauncherApp::open_multi_manager(self);
    }

    fn open_multi_manager_settings(&mut self) {
        LauncherApp::open_multi_manager_settings(self);
    }

    fn multi_manager_save(&mut self) {
        LauncherApp::multi_manager_save(self);
    }

    fn multi_manager_reload(&mut self) {
        LauncherApp::multi_manager_reload(self);
    }

    fn multi_manager_send_all_home(&mut self) {
        LauncherApp::multi_manager_send_all_home(self);
    }

    fn multi_manager_start_manual_reconnect(&mut self) {
        LauncherApp::multi_manager_start_manual_reconnect(self);
    }

    fn multi_manager_save_bindings(&mut self) {
        LauncherApp::multi_manager_save_bindings(self);
    }

    fn multi_manager_restore_bindings(&mut self) {
        LauncherApp::multi_manager_restore_bindings(self);
    }

    fn multi_manager_import(&mut self) {
        LauncherApp::multi_manager_import(self);
    }

    fn multi_manager_start_recapture_all(&mut self) {
        LauncherApp::multi_manager_start_recapture_all(self);
    }

    fn multi_manager_toggle_workspace(&mut self, workspace_id: &str) {
        LauncherApp::multi_manager_toggle_workspace(self, workspace_id);
    }

    fn multi_manager_send_home(&mut self, workspace_id: &str) {
        LauncherApp::multi_manager_send_home(self, workspace_id);
    }

    fn multi_manager_send_target(&mut self, workspace_id: &str) {
        LauncherApp::multi_manager_send_target(self, workspace_id);
    }

    fn multi_manager_start_capture(&mut self, workspace_id: &str) {
        LauncherApp::multi_manager_start_capture(self, workspace_id);
    }

    fn multi_manager_set_workspace_disabled(&mut self, workspace_id: &str, disabled: bool) {
        LauncherApp::multi_manager_set_workspace_disabled(self, workspace_id, disabled);
    }

    fn multi_manager_launcher_should_refocus(&self) -> bool {
        self.visible_flag.load(Ordering::SeqCst) && !self.any_panel_open()
    }
}

impl CropCommandHost for LauncherApp {
    fn crop_image(&mut self) {
        self.handle_crop_image_action();
    }

    fn crop_screenshot(&mut self) {
        self.begin_crop_screenshot();
    }
}

impl ColorPickCommandHost for LauncherApp {
    fn start_color_pick(&mut self) -> Result<bool, String> {
        self.begin_color_pick()
    }
}
impl crate::commands::OcrCommandHost for LauncherApp {
    fn start_ocr_selection(&mut self) -> Result<bool, String> {
        self.begin_ocr_selection()
    }
}

impl JsonUtilityCommandHost for LauncherApp {
    fn open_json_utility(&mut self, intent: crate::commands::JsonUtilityIntent) {
        self.json_utility_dialog.open(intent);
        self.focus_panel(super::Panel::JsonUtilityDialog);
    }
}

impl XmlToolkitCommandHost for LauncherApp {
    fn open_xml_toolkit(&mut self) {
        self.xml_toolkit_dialog.open();
        self.focus_panel(super::Panel::XmlToolkitDialog);
    }
}

impl BatchRenameCommandHost for LauncherApp {
    fn open_batch_rename(&mut self) {
        self.batch_rename_dialog.open();
        self.focus_panel(super::Panel::BatchRenameDialog);
    }
}

impl crate::commands::SearchReplaceCommandHost for LauncherApp {
    fn open_search_replace(&mut self) {
        self.focus_panel(super::Panel::SearchReplaceDialog);
    }
}

impl FileSearchCommandHost for LauncherApp {
    fn open_file_search(&mut self) {
        self.file_search_dialog.open();
    }

    fn cancel_file_search(&mut self) {
        self.file_search_dialog
            .cancel_search(&mut self.file_search_coordinator);
    }

    fn set_file_search_mode(
        &mut self,
        payload: &crate::file_search::actions::FileSearchModePayload,
    ) {
        self.file_search_dialog
            .open_with_mode(file_search_mode(payload.search_kind()));
    }

    fn start_file_search(&mut self, payload: &crate::file_search::actions::FileSearchStartPayload) {
        self.file_search_dialog.open_and_start(
            file_search_mode(payload.search_kind()),
            payload.root_path(),
            payload.text.clone(),
            &mut self.file_search_coordinator,
        );
    }

    fn report_file_search_action_error(&mut self, message: String) {
        self.set_inline_error(message.clone());
        self.add_error_toast(message);
    }
}

impl DiffCommandHost for LauncherApp {
    fn open_diff(&mut self, payload: &crate::diff::query::DiffOpenPayload) -> Result<(), String> {
        self.diff_dialog.open_payload(payload.clone())
    }
}

impl ClipboardModifyCommandHost for LauncherApp {
    fn open_clipboard_modify(
        &mut self,
        section: crate::clipboard_modify::actions::ClipboardModifySectionPayload,
    ) {
        use crate::clipboard_modify::actions::ClipboardModifySectionPayload;
        let section = match section {
            ClipboardModifySectionPayload::Modify => super::ClipboardModifyDialogSection::Modify,
            ClipboardModifySectionPayload::Templates => {
                super::ClipboardModifyDialogSection::Templates
            }
            ClipboardModifySectionPayload::SavedPipelines => {
                super::ClipboardModifyDialogSection::SavedPipelines
            }
            ClipboardModifySectionPayload::ManageTemplates => {
                super::ClipboardModifyDialogSection::ManageTemplates
            }
            ClipboardModifySectionPayload::ManagePipelines => {
                super::ClipboardModifyDialogSection::ManagePipelines
            }
            ClipboardModifySectionPayload::Help => super::ClipboardModifyDialogSection::Help,
        };
        self.clipboard_modify_dialog.open_section(
            section,
            &crate::clipboard_modify::runtime::clipboard_service(),
        );
    }

    fn undo_clipboard_modify(&mut self) -> Result<(), String> {
        crate::clipboard_modify::runtime::undo().map_err(|error| error.to_string())?;
        self.handle_clipboard_modify_gui_event(
            super::ClipboardModifyGuiEvent::ImmediateOperationComplete,
        );
        Ok(())
    }

    fn start_clipboard_modify(
        &mut self,
        intent: crate::clipboard_modify::parser::ClipboardModifyIntent,
        mut metadata: crate::clipboard_modify::coordinator::ImmediateRequestMetadata,
    ) -> Result<(), String> {
        metadata.root_policy = self.command_root_policy;
        self.clipboard_modify_immediate
            .start(
                intent,
                self.clipboard_modify_runtime.catalog_snapshot(),
                metadata,
            )
            .map(|_| ())
            .map_err(|error| error.message)
    }

    fn clipboard_modify_hide_launcher_after_apply(&self) -> bool {
        self.clipboard_modify_hide_launcher_after_apply
    }

    fn report_clipboard_modify_action_error(&mut self, message: String) {
        let message = format!("Invalid clipboard modify action: {message}");
        self.set_inline_error(message.clone());
        self.add_error_toast(message);
    }
}

impl ScreenshotCommandHost for LauncherApp {
    fn capture_screenshot(
        &mut self,
        mode: ScreenshotMode,
        destination: ScreenshotDestination,
        markup: ScreenshotMarkup,
    ) -> Result<ScreenshotCommandResult, String> {
        let mode = match mode {
            ScreenshotMode::Window => crate::actions::screenshot::Mode::Window,
            ScreenshotMode::Region => crate::actions::screenshot::Mode::Region,
            ScreenshotMode::Desktop => crate::actions::screenshot::Mode::Desktop,
        };
        let clip = destination == ScreenshotDestination::Clipboard;
        let tool = match markup {
            ScreenshotMarkup::Rectangle => super::MarkupTool::Rectangle,
            ScreenshotMarkup::Pen => super::MarkupTool::Pen,
        };
        crate::plugins::screenshot::launch_editor(self, mode, clip, tool)
            .map(|result| match result {
                crate::plugins::screenshot::ScreenshotLaunchOutcome::Completed => {
                    ScreenshotCommandResult::Completed
                }
                crate::plugins::screenshot::ScreenshotLaunchOutcome::Cancelled => {
                    ScreenshotCommandResult::Cancelled
                }
            })
            .map_err(|error| error.to_string())
    }

    fn screenshot_launcher_should_refocus(&self) -> bool {
        self.visible_flag.load(Ordering::SeqCst) && !self.any_panel_open()
    }
}

impl DataCommandHost for LauncherApp {
    fn open_data_dialog(&mut self, focus: crate::commands::DataDialogFocus) -> Result<(), String> {
        self.data_recovery_dialog.open(focus)?;
        self.focus_panel(super::Panel::DataRecoveryDialog);
        Ok(())
    }

    fn request_data_backup(&mut self) -> Result<(), String> {
        self.data_recovery_dialog.request_backup()
    }

    fn open_data_folder(&mut self) -> Result<(), String> {
        open::that(self.data_recovery_dialog.root())
            .map_err(|error| format!("Failed to open the application data folder: {error}"))
    }

    fn stage_data_recovery(
        &mut self,
        command: &crate::commands::DataRecoveryCommand,
    ) -> Result<(), String> {
        self.data_recovery_dialog.stage_recovery(command)
    }

    fn data_launcher_should_refocus(&self) -> bool {
        self.visible_flag.load(Ordering::SeqCst) && !self.any_panel_open()
    }
}
fn file_search_mode(kind: crate::file_search::model::SearchKind) -> super::FileSearchMode {
    match kind {
        crate::file_search::model::SearchKind::Filename => super::FileSearchMode::Filename,
        crate::file_search::model::SearchKind::Content => super::FileSearchMode::Content,
    }
}

impl HeadlessCommandHost for LauncherApp {
    fn execute_headless_command(
        &mut self,
        command: &Command,
        original_action: &crate::actions::Action,
    ) -> anyhow::Result<()> {
        super::execute_parsed_action(command, original_action)
    }

    fn spawn_headless_command(
        &mut self,
        command: Command,
        original_action: crate::actions::Action,
    ) {
        std::thread::spawn(move || {
            if let Err(error) = crate::commands::headless::execute(command, &original_action) {
                tracing::error!(?error, "failed to execute asynchronous command");
            }
        });
    }

    fn spawn_virtual_desktop_command(&mut self, invocation: crate::commands::CommandInvocation) {
        let history_query = self.query.clone();
        self.spawn_virtual_desktop_command_with_history_query(invocation, history_query);
    }

    fn spawn_virtual_desktop_command_with_history_query(
        &mut self,
        mut invocation: crate::commands::CommandInvocation,
        history_query: String,
    ) {
        match &invocation.command {
            crate::commands::Command::VirtualDesktop(
                crate::commands::VirtualDesktopCommand::BindWorkspace {
                    workspace_id,
                    target,
                    cached_name,
                },
            ) => {
                let result = (|| -> Result<(), String> {
                    let binding = crate::virtual_desktop::VirtualDesktopBinding {
                        id: crate::virtual_desktop::VirtualDesktopId::parse(target)
                            .map_err(|_| "workspace binding action must contain a desktop GUID")?,
                        cached_name: cached_name.clone(),
                    };
                    self.multi_manager
                        .bind_virtual_desktop(workspace_id, binding)
                        .then_some(())
                        .ok_or_else(|| format!("MultiManager workspace not found: {workspace_id}"))
                })();
                match result {
                    Ok(()) => {
                        let outcome = crate::commands::handlers::success_outcome(self, &invocation);
                        self.apply_command_outcome_with_history_query(
                            outcome,
                            &invocation,
                            Some(&history_query),
                        );
                        self.add_success_toast("Bound MultiManager workspace to desktop");
                    }
                    Err(error) => self.report_error_message("virtual_desktop", error),
                }
                return;
            }
            crate::commands::Command::VirtualDesktop(
                crate::commands::VirtualDesktopCommand::UnbindWorkspace { workspace_id },
            ) => {
                if self.multi_manager.unbind_virtual_desktop(workspace_id) {
                    let outcome = crate::commands::handlers::success_outcome(self, &invocation);
                    self.apply_command_outcome_with_history_query(
                        outcome,
                        &invocation,
                        Some(&history_query),
                    );
                    self.add_success_toast("Cleared MultiManager desktop binding");
                } else {
                    self.report_error_message(
                        "virtual_desktop",
                        format!("MultiManager workspace not found: {workspace_id}"),
                    );
                }
                return;
            }
            _ => {}
        }
        if let crate::commands::Command::VirtualDesktop(
            crate::commands::VirtualDesktopCommand::MoveActiveWindow { target, follow },
        ) = &invocation.command
        {
            match crate::active_window::resolve_previous_active_window() {
                Ok(hwnd) => {
                    invocation.command = crate::commands::Command::VirtualDesktop(
                        crate::commands::VirtualDesktopCommand::MoveWindow {
                            hwnd,
                            target: target.clone(),
                            follow: *follow,
                        },
                    );
                }
                Err(error) => {
                    let completion_outcome = crate::commands::CommandOutcome::default();
                    let _ = self.event_tx.send(crate::gui::WatchEvent::VirtualDesktop(
                        crate::gui::VirtualDesktopGuiCompletion {
                            invocation,
                            completion_outcome,
                            history_query,
                            interaction_token: self.virtual_desktop_interaction_token,
                            expected_query: self.query.clone(),
                            expected_visible: self.visible_flag.load(Ordering::SeqCst),
                            root_policy: self.command_root_policy,
                            result: Err(error),
                        },
                    ));
                    self.egui_ctx.request_repaint();
                    return;
                }
            }
        }
        let (immediate_outcome, completion_outcome) = split_virtual_desktop_outcome(
            crate::commands::handlers::success_outcome(self, &invocation),
        );
        self.apply_command_outcome(immediate_outcome, &invocation);
        let interaction_token = self.virtual_desktop_interaction_token;
        let expected_query = self.query.clone();
        let expected_visible = self.visible_flag.load(Ordering::SeqCst);
        let root_policy = self.command_root_policy;

        #[cfg(test)]
        if self.test_defer_virtual_desktop_completion {
            self.test_defer_virtual_desktop_completion = false;
            let _ = self.event_tx.with_origin(EventOrigin::CommandHost).send(
                crate::gui::WatchEvent::VirtualDesktop(crate::gui::VirtualDesktopGuiCompletion {
                    invocation,
                    completion_outcome,
                    history_query,
                    interaction_token,
                    expected_query,
                    expected_visible,
                    root_policy,
                    result: Ok(()),
                }),
            );
            self.egui_ctx.request_repaint();
            return;
        }

        let catalog = std::sync::Arc::clone(&self.plugins.internal_services().window_catalog);
        let tx = self.event_tx.with_origin(EventOrigin::CommandHost);
        let ctx = self.egui_ctx.clone();
        std::thread::spawn(move || {
            let result = crate::commands::headless::execute_virtual_desktop_with_catalog(
                match &invocation.command {
                    crate::commands::Command::VirtualDesktop(command) => command.clone(),
                    _ => unreachable!("virtual desktop host receives virtual desktop commands"),
                },
                &invocation.original_action,
                catalog,
            )
            .map_err(|error| error.to_string());
            let _ = tx.send(crate::gui::WatchEvent::VirtualDesktop(
                crate::gui::VirtualDesktopGuiCompletion {
                    invocation,
                    completion_outcome,
                    history_query,
                    interaction_token,
                    expected_query,
                    expected_visible,
                    root_policy,
                    result,
                },
            ));
            ctx.request_repaint();
        });
    }

    fn clear_query_after_run(&self) -> bool {
        self.clear_query_after_run
    }

    fn hide_after_run(&self) -> bool {
        self.hide_after_run
    }

    fn preserve_command(&self) -> bool {
        self.preserve_command
    }

    fn current_query(&self) -> &str {
        &self.query
    }

    fn launcher_should_refocus(&self) -> bool {
        self.visible_flag.load(Ordering::SeqCst) && !self.any_panel_open()
    }

    fn copy_snippet_text(&mut self, text: &str) -> anyhow::Result<()> {
        crate::actions::clipboard::set_text(text)
    }

    fn capture_snippet_invocation_context(
        &mut self,
        source: crate::commands::ActivationSource,
        needs_clipboard: bool,
        needs_window_title: bool,
    ) -> crate::plugins::SnippetInvocationContext {
        let timestamp = chrono::Local::now().fixed_offset();
        let origin = if needs_window_title {
            select_snippet_origin_window(
                source,
                self.visible_flag.load(Ordering::SeqCst),
                crate::active_window::observe_foreground_window(),
                self.snippet_origin_window.clone(),
            )
        } else {
            None
        };
        let clipboard_text = needs_clipboard
            .then(|| crate::actions::clipboard::get_text().ok())
            .flatten();
        let (foreground_hwnd, foreground_title) = if needs_window_title {
            origin.map_or((None, None), |origin| (Some(origin.hwnd), origin.title))
        } else {
            (None, None)
        };

        crate::plugins::SnippetInvocationContext::new(
            timestamp,
            clipboard_text,
            foreground_hwnd,
            foreground_title,
        )
    }

    fn request_snippet_prompt(
        &mut self,
        intent: crate::commands::SnippetPromptIntent,
    ) -> Result<(), String> {
        self.snippet_prompt_dialog
            .begin_execution(&self.egui_ctx, intent);
        self.focus_panel(super::Panel::SnippetPromptDialog);
        Ok(())
    }

    fn snippet_root_policy(&self) -> crate::universal_actions::RootLauncherPolicy {
        self.command_root_policy
    }
}

fn select_snippet_origin_window(
    source: crate::commands::ActivationSource,
    root_visible: bool,
    observed: crate::active_window::ForegroundWindowObservation,
    cached: Option<crate::active_window::ExternalForegroundWindow>,
) -> Option<crate::active_window::ExternalForegroundWindow> {
    let radial_source = matches!(
        source,
        crate::commands::ActivationSource::RadialRelease
            | crate::commands::ActivationSource::RadialShortcut
            | crate::commands::ActivationSource::RadialHotstring
    );
    if radial_source {
        return match observed {
            crate::active_window::ForegroundWindowObservation::External(window) => Some(window),
            crate::active_window::ForegroundWindowObservation::Launcher
            | crate::active_window::ForegroundWindowObservation::Unavailable => None,
        };
    }

    match observed {
        crate::active_window::ForegroundWindowObservation::External(window) => Some(window),
        crate::active_window::ForegroundWindowObservation::Launcher if root_visible => cached,
        crate::active_window::ForegroundWindowObservation::Launcher
        | crate::active_window::ForegroundWindowObservation::Unavailable => None,
    }
}

fn select_sequence_origin_window(
    source: crate::commands::ActivationSource,
    root_visible: bool,
    observed: crate::active_window::ForegroundWindowObservation,
    cached: Option<crate::active_window::ExternalForegroundWindow>,
) -> Option<crate::active_window::ExternalForegroundWindow> {
    match observed {
        crate::active_window::ForegroundWindowObservation::External(window) => Some(window),
        crate::active_window::ForegroundWindowObservation::Launcher
            if root_visible
                && matches!(
                    source,
                    crate::commands::ActivationSource::Click
                        | crate::commands::ActivationSource::Enter
                        | crate::commands::ActivationSource::Dashboard
                        | crate::commands::ActivationSource::Hotkey
                ) =>
        {
            cached
        }
        crate::active_window::ForegroundWindowObservation::Launcher
        | crate::active_window::ForegroundWindowObservation::Unavailable => None,
    }
}

fn command_accepts_query_override(command: &Command) -> bool {
    !matches!(
        command,
        Command::Dialog(crate::commands::DialogCommand::RegexTester)
            | Command::Dialog(crate::commands::DialogCommand::Qr { .. })
            | Command::Radial(_)
            | Command::ClipboardModify(_)
            | Command::JsonUtility(_)
            | Command::XmlToolkit(_)
            | Command::BatchRename(_)
            | Command::HashGenerator(_)
            | Command::ColorPick(_)
            | Command::Ocr(_)
            | Command::CoordinateTool(_)
            | Command::FileSearch(_)
            | Command::Diff(_)
    )
}

impl LauncherApp {
    pub(crate) fn cancel_active_paste_sequence(&mut self) -> bool {
        use crate::paste_sequence::SequencePlaybackStatus as Status;

        if self.snippet_prompt_dialog.is_sequence_preflight() {
            let _ = self.cancel_snippet_prompt();
            return true;
        }
        if matches!(
            self.sequence_playback.status(),
            Status::Preparing | Status::Ready | Status::Running | Status::AwaitAdvance
        ) {
            self.cancel_sequence_playback();
            return true;
        }
        false
    }

    /// Capture every invocation input before the sequence prompt or preflight
    /// panel takes focus. The external HWND/PID/thread identity is mandatory,
    /// even when no snippet uses `sys.window_title`.
    pub(crate) fn capture_sequence_context(
        &mut self,
        source: crate::commands::ActivationSource,
    ) -> Result<
        crate::paste_sequence::SequenceInvocationContext,
        crate::paste_sequence::SequencePreflightError,
    > {
        let timestamp = chrono::Local::now().fixed_offset();
        let origin = select_sequence_origin_window(
            source,
            self.visible_flag.load(Ordering::SeqCst),
            crate::active_window::observe_foreground_window(),
            self.snippet_origin_window.clone(),
        )
        .ok_or(crate::paste_sequence::SequencePreflightError::InvalidTargetIdentity)?;
        let target = crate::paste_sequence::SequenceTarget::from_external_window(
            origin,
            std::process::id(),
        )?;
        let clipboard_text = crate::actions::clipboard::get_text().ok();
        Ok(crate::paste_sequence::SequenceInvocationContext::new(
            timestamp,
            clipboard_text,
            target,
        ))
    }

    /// Start preflight from an already inspected sequence and provider snapshots.
    /// The controller owns admission throughout a pending prompt and the ready run.
    pub(crate) fn begin_paste_sequence_preflight(
        &mut self,
        sequence: crate::paste_sequence::Sequence,
        snippets: &[crate::plugins::snippets::SnippetEntry],
        history: &[String],
        source: crate::commands::ActivationSource,
    ) -> Result<(), crate::paste_sequence::SequencePreflightError> {
        self.begin_paste_sequence_preflight_using_context(
            sequence,
            snippets,
            history,
            source,
            |app, source| app.capture_sequence_context(source),
        )
    }

    fn begin_paste_sequence_preflight_using_context(
        &mut self,
        sequence: crate::paste_sequence::Sequence,
        snippets: &[crate::plugins::snippets::SnippetEntry],
        history: &[String],
        source: crate::commands::ActivationSource,
        capture_context: impl FnOnce(
            &mut Self,
            crate::commands::ActivationSource,
        ) -> Result<
            crate::paste_sequence::SequenceInvocationContext,
            crate::paste_sequence::SequencePreflightError,
        >,
    ) -> Result<(), crate::paste_sequence::SequencePreflightError> {
        if self.snippet_prompt_dialog.is_open() {
            return Err(crate::paste_sequence::SequencePreflightError::SequenceAlreadyActive);
        }
        self.sequence_playback
            .begin_preparing()
            .map_err(|_| crate::paste_sequence::SequencePreflightError::SequenceAlreadyActive)?;
        let context = match capture_context(self, source) {
            Ok(context) => context,
            Err(error) => {
                self.sequence_playback.preparation_failed();
                return Err(error);
            }
        };
        let preflight = match crate::paste_sequence::SequencePreflight::begin(
            sequence, snippets, history, context,
        ) {
            Ok(preflight) => preflight,
            Err(error) => {
                self.sequence_playback.preparation_failed();
                return Err(error);
            }
        };
        self.finish_paste_sequence_preflight(preflight, snippets, history)
    }

    pub(crate) fn begin_paste_sequence_preflight_with_context(
        &mut self,
        sequence: crate::paste_sequence::Sequence,
        snippets: &[crate::plugins::snippets::SnippetEntry],
        history: &[String],
        context: crate::paste_sequence::SequenceInvocationContext,
    ) -> Result<(), crate::paste_sequence::SequencePreflightError> {
        if self.snippet_prompt_dialog.is_open() {
            return Err(crate::paste_sequence::SequencePreflightError::SequenceAlreadyActive);
        }
        self.sequence_playback
            .begin_preparing()
            .map_err(|_| crate::paste_sequence::SequencePreflightError::SequenceAlreadyActive)?;
        let preflight = match crate::paste_sequence::SequencePreflight::begin(
            sequence, snippets, history, context,
        ) {
            Ok(preflight) => preflight,
            Err(error) => {
                self.sequence_playback.preparation_failed();
                return Err(error);
            }
        };
        self.finish_paste_sequence_preflight(preflight, snippets, history)
    }

    pub(crate) fn begin_paste_sequence_draft_preflight_with_context(
        &mut self,
        draft: crate::paste_sequence::SequenceDraft,
        snippets: &[crate::plugins::snippets::SnippetEntry],
        history: &[String],
        context: crate::paste_sequence::SequenceInvocationContext,
    ) -> Result<(), crate::paste_sequence::SequencePreflightError> {
        if self.snippet_prompt_dialog.is_open() {
            return Err(crate::paste_sequence::SequencePreflightError::SequenceAlreadyActive);
        }
        self.sequence_playback
            .begin_preparing()
            .map_err(|_| crate::paste_sequence::SequencePreflightError::SequenceAlreadyActive)?;
        let preflight = match crate::paste_sequence::SequencePreflight::begin_draft(
            draft, snippets, history, context,
        ) {
            Ok(preflight) => preflight,
            Err(error) => {
                self.sequence_playback.preparation_failed();
                return Err(error);
            }
        };
        self.finish_paste_sequence_preflight(preflight, snippets, history)
    }

    fn finish_paste_sequence_preflight(
        &mut self,
        preflight: crate::paste_sequence::SequencePreflight,
        snippets: &[crate::plugins::snippets::SnippetEntry],
        history: &[String],
    ) -> Result<(), crate::paste_sequence::SequencePreflightError> {
        if preflight.current_prompt().is_some() {
            if self
                .snippet_prompt_dialog
                .begin_sequence_preflight(&self.egui_ctx, preflight)
                .is_err()
            {
                self.sequence_playback.preparation_failed();
                return Err(crate::paste_sequence::SequencePreflightError::InvalidSnippet);
            }
            self.focus_panel(super::Panel::SnippetPromptDialog);
        } else {
            let run = match preflight.finish(snippets, history) {
                Ok(run) => run,
                Err(error) => {
                    self.sequence_playback.preparation_failed();
                    return Err(error);
                }
            };
            if self.sequence_playback.set_ready(run).is_err() {
                self.sequence_playback.preparation_failed();
                return Err(crate::paste_sequence::SequencePreflightError::SequenceAlreadyActive);
            }
        }
        Ok(())
    }

    pub(crate) fn start_sequence_playback(
        &mut self,
    ) -> Result<(), crate::paste_sequence::SequencePlaybackError> {
        let mut platform = crate::paste_sequence::NativeSequencePlaybackPlatform::new(
            self.launcher_hwnd,
            Some(self.egui_ctx.clone()),
        );
        let result = self.sequence_playback.start(&mut platform);
        self.schedule_sequence_playback_wake(std::time::Instant::now());
        result
    }

    pub(crate) fn next_sequence_playback(
        &mut self,
    ) -> Result<(), crate::paste_sequence::SequencePlaybackError> {
        let now = std::time::Instant::now();
        let mut platform = crate::paste_sequence::NativeSequencePlaybackPlatform::new(
            self.launcher_hwnd,
            Some(self.egui_ctx.clone()),
        );
        let result = self.sequence_playback.next(&mut platform, now);
        self.schedule_sequence_playback_wake(std::time::Instant::now());
        result
    }

    pub(crate) fn cancel_sequence_playback(&mut self) {
        let now = std::time::Instant::now();
        self.sequence_playback.cancel(now);
        self.schedule_sequence_playback_wake(now);
    }

    fn schedule_sequence_playback_wake(&self, now: std::time::Instant) {
        self.egui_ctx.request_repaint();
        if let Some(delay) = self.sequence_playback.next_wake_after(now) {
            self.egui_ctx.request_repaint_after(delay);
        }
    }

    fn submit_sequence_prompt(
        &mut self,
    ) -> Result<(), super::snippet_prompt_dialog::SnippetPromptError> {
        self.submit_sequence_prompt_using(
            || {
                crate::plugins::snippets::load_snippets_for_reload(
                    crate::plugins::snippets::SNIPPETS_FILE,
                )
                .map(|state| match state {
                    crate::common::persistence::LoadState::Missing
                    | crate::common::persistence::LoadState::Empty => Vec::new(),
                    crate::common::persistence::LoadState::Loaded(entries) => entries,
                })
                .map_err(|_| ())
            },
            || {
                crate::plugins::clipboard::load_history(crate::plugins::clipboard::CLIPBOARD_FILE)
                    .map(|history| history.into_iter().collect())
                    .map_err(|_| ())
            },
        )
    }

    fn submit_sequence_prompt_using(
        &mut self,
        current_snippets: impl FnOnce() -> Result<Vec<crate::plugins::snippets::SnippetEntry>, ()>,
        current_history: impl FnOnce() -> Result<Vec<String>, ()>,
    ) -> Result<(), super::snippet_prompt_dialog::SnippetPromptError> {
        let completed = self.snippet_prompt_dialog.submit_sequence_with(
            &self.egui_ctx,
            current_snippets,
            current_history,
        )?;
        if let Some(run) = completed {
            self.panel_states.snippet_prompt_dialog = false;
            self.panel_stack
                .retain(|panel| *panel != super::Panel::SnippetPromptDialog);
            self.sequence_playback.set_ready(run).map_err(|_| {
                super::snippet_prompt_dialog::SnippetPromptError::SequencePreflightFailed
            })?;
            if self.paste_sequence_dialog.take_autostart_when_ready()
                && let Err(error) = self.start_sequence_playback()
            {
                self.report_error_message("paste_sequence.playback", error.message());
            }
        }
        Ok(())
    }

    pub(crate) fn begin_snippet_preview(
        &mut self,
        draft: crate::plugins::snippets::SnippetEntry,
    ) -> Result<(), super::snippet_prompt_dialog::SnippetPromptError> {
        let prepared = match crate::plugins::snippets::prepare_snippet_run(&draft) {
            Ok(
                crate::plugins::snippets::SnippetRunMode::SystemOnly(prepared)
                | crate::plugins::snippets::SnippetRunMode::Prompted(prepared),
            ) => prepared,
            Ok(crate::plugins::snippets::SnippetRunMode::Plain) | Err(_) => {
                return Err(super::snippet_prompt_dialog::SnippetPromptError::InvalidConfiguration);
            }
        };
        let context = prepared.parsed.has_system_variables().then(|| {
            crate::commands::HeadlessCommandHost::capture_snippet_invocation_context(
                self,
                crate::commands::ActivationSource::Click,
                prepared.parsed.uses_clipboard(),
                prepared.parsed.uses_window_title(),
            )
        });
        self.snippet_prompt_dialog.begin_preview_prepared(
            &self.egui_ctx,
            draft,
            prepared,
            context,
        )?;
        self.focus_panel(super::Panel::SnippetPromptDialog);
        Ok(())
    }

    pub(crate) fn submit_snippet_prompt(
        &mut self,
    ) -> Result<(), super::snippet_prompt_dialog::SnippetPromptError> {
        if self.snippet_prompt_dialog.is_sequence_preflight() {
            return self.submit_sequence_prompt();
        }
        self.submit_snippet_prompt_using(
            |alias| crate::plugins::snippets::resolve_snippet(alias).map_err(|_| ()),
            |text| crate::actions::clipboard::set_text(text).map_err(|_| ()),
        )
    }

    fn submit_snippet_prompt_using(
        &mut self,
        resolve_current: impl FnOnce(&str) -> Result<crate::plugins::snippets::SnippetEntry, ()>,
        copy_text: impl FnOnce(&str) -> Result<(), ()>,
    ) -> Result<(), super::snippet_prompt_dialog::SnippetPromptError> {
        let completion =
            self.snippet_prompt_dialog
                .submit_with(&self.egui_ctx, resolve_current, copy_text)?;

        self.panel_states.snippet_prompt_dialog = false;
        self.panel_stack
            .retain(|panel| *panel != super::Panel::SnippetPromptDialog);

        let invocation = CommandInvocation {
            command: Command::Storage(crate::commands::StorageCommand::SnippetRun(
                completion.alias,
            )),
            original_action: completion.safe_action,
            query_override: None,
            source: completion.source,
        };
        let outcome = crate::commands::handlers::success_outcome(self, &invocation);
        self.apply_command_outcome_with_root_policy(
            outcome,
            &invocation,
            Some(&completion.history_query),
            completion.root_policy,
        );
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn submit_snippet_prompt_with_test_backends(
        &mut self,
        resolve_current: impl FnOnce(&str) -> Result<crate::plugins::snippets::SnippetEntry, ()>,
        copy_text: impl FnOnce(&str) -> Result<(), ()>,
    ) -> Result<(), super::snippet_prompt_dialog::SnippetPromptError> {
        self.submit_snippet_prompt_using(resolve_current, copy_text)
    }

    pub(crate) fn cancel_snippet_prompt(
        &mut self,
    ) -> Option<crate::plugins::snippets::SnippetEntry> {
        let was_preview = self.snippet_prompt_dialog.is_preview_only();
        let was_sequence = self.snippet_prompt_dialog.is_sequence_preflight();
        let draft = self.snippet_prompt_dialog.cancel(&self.egui_ctx);
        if was_sequence {
            self.cancel_sequence_playback();
        }
        self.panel_states.snippet_prompt_dialog = false;
        self.panel_stack
            .retain(|panel| *panel != super::Panel::SnippetPromptDialog);
        if was_preview && self.snippet_dialog.open {
            self.focus_panel(super::Panel::SnippetDialog);
        }
        draft
    }

    pub(crate) fn dispatch_command_invocation(&mut self, invocation: CommandInvocation) {
        self.dispatch_command_invocation_with_history(invocation, None);
    }

    pub(crate) fn dispatch_command_invocation_with_history(
        &mut self,
        invocation: CommandInvocation,
        captured_history_query: Option<&str>,
    ) {
        self.virtual_desktop_interaction_token = self
            .virtual_desktop_interaction_token
            .wrapping_add(1)
            .max(1);
        if let Some(query_override) = invocation.query_override.as_ref()
            && command_accepts_query_override(&invocation.command)
        {
            self.apply_command_outcome(
                CommandOutcome {
                    query: QueryPolicy::Set(query_override.clone()),
                    search: true,
                    ..CommandOutcome::default()
                },
                &invocation,
            );
        }

        let bus = std::sync::Arc::clone(&self.command_bus);
        if let Command::Radial(command) = &invocation.command {
            let skins = match command {
                crate::commands::RadialCommand::Edit => Some(false),
                crate::commands::RadialCommand::Skins => Some(true),
                _ => None,
            };
            if let Some(skins) = skins {
                crate::radial::acceptance_trace::emit(
                    crate::radial::acceptance_trace::Event::RadialAction {
                        stage: crate::radial::acceptance_trace::RadialActionStage::Dispatched,
                        skins,
                        editor_open: None,
                        skins_selected: None,
                        panel_registered: None,
                    },
                );
            }
        }
        match bus.dispatch_with_history_query(&invocation, self, captured_history_query) {
            Ok(outcome) => self.apply_command_outcome_with_history_query(
                outcome,
                &invocation,
                captured_history_query,
            ),
            Err(error) => {
                if let Some(favorite) = error.favorite.as_ref() {
                    tracing::error!(fav = %favorite, error = %error.message, "failed to run favorite");
                }
                let message = error.message;
                self.report_error_message(error.domain, message.clone());
                if error.toast {
                    // The legacy generic failure path reported the error and also
                    // emitted its explicit failure toast.
                    self.add_error_toast(message);
                }
                if error.refocus
                    && self.visible_flag.load(Ordering::SeqCst)
                    && !self.any_panel_open()
                {
                    self.focus_input();
                }
            }
        }
    }

    pub(crate) fn apply_command_outcome(
        &mut self,
        outcome: CommandOutcome,
        invocation: &CommandInvocation,
    ) {
        self.apply_command_outcome_with_history_query(outcome, invocation, None);
    }

    pub(crate) fn apply_command_outcome_with_history_query(
        &mut self,
        outcome: CommandOutcome,
        invocation: &CommandInvocation,
        captured_history_query: Option<&str>,
    ) {
        let root_policy = self.command_root_policy;
        self.apply_command_outcome_with_root_policy(
            outcome,
            invocation,
            captured_history_query,
            root_policy,
        );
    }

    pub(crate) fn apply_command_outcome_with_root_policy(
        &mut self,
        outcome: CommandOutcome,
        invocation: &CommandInvocation,
        captured_history_query: Option<&str>,
        root_policy: crate::universal_actions::RootLauncherPolicy,
    ) {
        let outcome = normalize_outcome_for_root_policy(outcome, root_policy);
        let history_query = captured_history_query
            .map(str::to_owned)
            .unwrap_or_else(|| self.query.clone());

        let observe_query = self.track_c_query_observer.is_enabled();
        let paired_command_pending_query = if observe_query {
            match (&outcome.query, &outcome.pending_query) {
                (QueryPolicy::Set(command_query), PendingQueryPolicy::Set(pending_query))
                    if command_query == pending_query =>
                {
                    Some(command_query.clone())
                }
                _ => None,
            }
        } else {
            None
        };
        let has_command_set_query = observe_query && matches!(&outcome.query, QueryPolicy::Set(_));
        let mut command_set_sequence = None;
        if let QueryPolicy::Set(query) = outcome.query {
            command_set_sequence = self.track_c_query_observer.begin_from(
                self.track_c_query_observer.tentative_start(),
                super::track_c_query_observer::Origin::CommandSet,
                &query,
            );
            self.last_timer_query =
                query.starts_with("timer list") || query.starts_with("alarm list");
            self.query = query;
        }
        if let PendingQueryPolicy::Set(query) = outcome.pending_query {
            self.publish_pending_query(query);
            if let Some(query) = paired_command_pending_query.as_deref()
                && self.query == query
                && self.pending_query.as_deref() == Some(query)
            {
                self.track_c_query_observer
                    .arm_command_pending_handoff(command_set_sequence, query);
            }
        }
        if let ResultsPolicy::Replace(results) = outcome.results {
            let sources_before = observe_query.then(|| {
                (
                    self.plugins.search_generation(),
                    crate::radial::dynamic::MutableResultCatalogVersions::current(),
                )
            });
            if !has_command_set_query {
                self.track_c_query_observer.begin_from(
                    self.track_c_query_observer.tentative_start(),
                    super::track_c_query_observer::Origin::CommandReplacement,
                    &self.query,
                );
            }
            self.results = results;
            self.invalidate_root_list_results();
            let sources_after = sources_before.map(|_| {
                (
                    self.plugins.search_generation(),
                    crate::radial::dynamic::MutableResultCatalogVersions::current(),
                )
            });
            self.selected = None;
            self.last_search_query = self.query.clone();
            self.last_results_valid = true;
            self.last_search_provider_deferral = super::search::ProviderSearchDeferral::None;
            self.update_suggestions();
            if let (
                Some((provider_before, catalog_before)),
                Some((provider_after, catalog_after)),
            ) = (sources_before, sources_after)
            {
                self.track_c_query_observer.accept_command_replacement(
                    &self.query,
                    self.root_list_geometry.result_generation(),
                    provider_before,
                    provider_after,
                    catalog_before,
                    catalog_after,
                );
            }
        }
        if outcome.invalidate_results {
            self.track_c_query_observer.results_invalidated();
            self.last_results_valid = false;
        }
        if outcome.search {
            self.search();
        }
        if let Some(source) = outcome.activate_first_result
            && let Some(action) = self.results.first().cloned()
        {
            if allow_radial_queryexec_activation(&mut self.radial_queryexec_depth, &action) {
                self.activate_action(action, None, source);
            } else {
                self.report_error_message(
                    "radial_query",
                    "Nested queryexec stopped at the radial recursion limit",
                );
            }
        }

        if outcome.visibility != VisibilityPolicy::Keep || outcome.restore {
            self.visibility_revision.request(|| {
                let visible = match outcome.visibility {
                    VisibilityPolicy::Keep => self.visible_flag.load(Ordering::SeqCst),
                    VisibilityPolicy::Show => true,
                    VisibilityPolicy::Hide => false,
                    VisibilityPolicy::Toggle => !self.visible_flag.load(Ordering::SeqCst),
                };
                self.visible_flag.store(visible, Ordering::SeqCst);
                if outcome.restore {
                    self.restore_flag.store(true, Ordering::SeqCst);
                }
            });
        }
        if outcome.move_cursor_end {
            self.move_cursor_end = true;
        }
        if outcome.focus {
            self.focus_input();
        }
        if let FavoriteLogPolicy::Ran { label, command } = outcome.favorite_log {
            tracing::info!(fav = %label, command = %command, "ran favorite");
        }
        for toast in outcome.toasts {
            if let ToastPolicy::Error(message) = toast {
                self.add_error_toast(message);
                continue;
            }
            if self.enable_toasts {
                let (text, kind) = match toast {
                    ToastPolicy::Launched(label) => {
                        (format!("Launched {label}"), ToastKind::Success)
                    }
                    ToastPolicy::Copied(label) => (format!("Copied {label}"), ToastKind::Success),
                    ToastPolicy::Info(message) => (message, ToastKind::Info),
                    ToastPolicy::Success(message) => (message, ToastKind::Success),
                    ToastPolicy::Error(_) => unreachable!(),
                };
                self.add_toast(Toast {
                    text: text.into(),
                    kind,
                    options: ToastOptions::default()
                        .duration_in_seconds(self.toast_duration as f64),
                    style: Default::default(),
                });
            }
        }
        if outcome.history == HistoryPolicy::Record {
            self.record_history_usage(
                &invocation.original_action,
                &history_query,
                invocation.source,
            );
        }
    }
}

fn allow_radial_queryexec_activation(
    depth: &mut Option<u8>,
    action: &crate::actions::Action,
) -> bool {
    let is_queryexec = matches!(
        crate::commands::parse_action(action),
        Ok(Command::Query(
            crate::commands::QueryCommand::ExecuteFirst { .. }
        ))
    );
    if !is_queryexec {
        return true;
    }
    let Some(depth) = depth else {
        // Ordinary launcher callers keep their legacy recursive behavior.
        return true;
    };
    if *depth >= 1 {
        return false;
    }
    *depth += 1;
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paste_sequence_source_loading_is_reference_driven() {
        let literal_only = crate::paste_sequence::Sequence {
            id: crate::paste_sequence::SequenceId::from_raw(1).unwrap(),
            name: "Literal only".into(),
            hotkey: None,
            mode: crate::paste_sequence::PlaybackMode::Automatic,
            delay_ms: 0,
            steps: vec![crate::paste_sequence::Step::LiteralText {
                text: "temporary".into(),
            }],
        };
        assert_eq!(sequence_source_requirements(&literal_only), (false, false));

        let referenced = crate::paste_sequence::Sequence {
            id: crate::paste_sequence::SequenceId::from_raw(2).unwrap(),
            name: "Referenced sources".into(),
            hotkey: None,
            mode: crate::paste_sequence::PlaybackMode::Automatic,
            delay_ms: 0,
            steps: vec![
                crate::paste_sequence::Step::SnippetRef {
                    alias: "greeting".into(),
                },
                crate::paste_sequence::Step::clipboard_history_reference("history payload"),
            ],
        };
        assert_eq!(sequence_source_requirements(&referenced), (true, true));
    }

    #[test]
    fn saved_sequence_waits_for_its_prompt_before_becoming_ready_for_playback() {
        use crate::paste_sequence::{PlaybackMode, SequenceDraft, SequenceStore, Step};
        use crate::plugins::snippets::{SnippetEntry, SnippetFieldDefinition};

        let directory = tempfile::tempdir().unwrap();
        let store = SequenceStore::new(
            directory
                .path()
                .join(crate::paste_sequence::PASTE_SEQUENCES_FILE),
        );
        let history_text = "history item\n雪";
        let mut draft = SequenceDraft::new(
            "Mixed integration",
            vec![
                Step::SnippetRef {
                    alias: "private_reply".into(),
                },
                Step::SnippetRef {
                    alias: "context_values".into(),
                },
                Step::clipboard_history_reference(history_text),
                Step::LiteralText {
                    text: "One-off 🌌\nsecond line".into(),
                },
            ],
        );
        draft.mode = PlaybackMode::ManualAdvance;
        draft.delay_ms = 275;
        let mut snapshot = store.load().unwrap();
        let id = snapshot.upsert(draft).unwrap();
        store.commit(&mut snapshot).unwrap();
        let reloaded = store.load().unwrap();
        let sequence = reloaded.sequence(id).unwrap().clone();

        let snippets = vec![
            SnippetEntry {
                alias: "private_reply".into(),
                text: "Hello {{name}} at {{sys.date:%Y-%m-%d}}".into(),
                hide_contents: true,
                prompt_for_fields: true,
                fields: vec![SnippetFieldDefinition::new("name")],
                tags: Vec::new(),
            },
            SnippetEntry {
                alias: "context_values".into(),
                text: "{{sys.time:%H:%M}}|{{sys.clipboard}}|{{sys.window_title}}".into(),
                hide_contents: false,
                prompt_for_fields: false,
                fields: Vec::new(),
                tags: Vec::new(),
            },
        ];
        let history = vec![history_text.to_owned()];
        let context = crate::paste_sequence::SequenceInvocationContext::new(
            chrono::DateTime::parse_from_rfc3339("2024-01-02T03:04:05+00:00").unwrap(),
            Some("captured clipboard".into()),
            crate::paste_sequence::SequenceTarget::from_external_window(
                external_window(11, "Origin editor 🖋"),
                std::process::id(),
            )
            .unwrap(),
        );

        let mut app = inert_test_app(directory.path());
        let mut context_captures = 0;
        let provided_context = context.clone();
        app.begin_paste_sequence_preflight_using_context(
            sequence,
            &snippets,
            &history,
            crate::commands::ActivationSource::Hotkey,
            |_, source| {
                assert_eq!(source, crate::commands::ActivationSource::Hotkey);
                context_captures += 1;
                Ok(provided_context)
            },
        )
        .unwrap();

        assert_eq!(context_captures, 1);
        assert!(app.snippet_prompt_dialog.is_sequence_preflight());
        assert_eq!(
            app.sequence_playback.status(),
            crate::paste_sequence::SequencePlaybackStatus::Preparing
        );
        assert!(app.sequence_playback.prepared_preview().is_none());

        app.snippet_prompt_dialog.set_value("name", "Ada".into());
        let mut snippet_loads = 0;
        let mut history_loads = 0;
        app.submit_sequence_prompt_using(
            || {
                snippet_loads += 1;
                Ok(snippets.clone())
            },
            || {
                history_loads += 1;
                Ok(history.clone())
            },
        )
        .unwrap();

        assert_eq!(snippet_loads, 1);
        assert_eq!(history_loads, 1);
        assert!(!app.snippet_prompt_dialog.is_open());
        assert_eq!(
            app.sequence_playback.status(),
            crate::paste_sequence::SequencePlaybackStatus::Ready
        );
        assert!(!app.sequence_playback.needs_tick());
        let preview = app.sequence_playback.prepared_preview().unwrap();
        assert_eq!(preview.mode, PlaybackMode::ManualAdvance);
        assert_eq!(preview.delay_ms, 275);
        assert_eq!(preview.item_count, 4);
        assert!(preview.steps[0].hidden);
        assert!(preview.steps[0].text.is_none());
        assert_eq!(
            preview.steps[1].text.as_deref(),
            Some("03:04|captured clipboard|Origin editor 🖋")
        );
        assert_eq!(preview.steps[2].text.as_deref(), Some(history_text));
        assert_eq!(
            preview.steps[3].text.as_deref(),
            Some("One-off 🌌\nsecond line")
        );
    }

    #[test]
    fn paste_sequence_cancel_only_closes_sequence_owned_snippet_prompts() {
        use crate::hotkey::launcher_invocation::PasteSequenceHotkeyAction;

        let directory = tempfile::tempdir().unwrap();
        let mut ordinary_app = inert_test_app(directory.path());
        crate::commands::HeadlessCommandHost::request_snippet_prompt(
            &mut ordinary_app,
            prompt_intent(prompted_entry()),
        )
        .unwrap();
        ordinary_app.handle_paste_sequence_hotkey(PasteSequenceHotkeyAction::Cancel);
        assert!(ordinary_app.snippet_prompt_dialog.is_open());
        assert!(!ordinary_app.snippet_prompt_dialog.is_sequence_preflight());

        let store = crate::paste_sequence::SequenceStore::new(
            directory
                .path()
                .join(crate::paste_sequence::PASTE_SEQUENCES_FILE),
        );
        let mut snapshot = store.load().unwrap();
        let id = snapshot
            .upsert(crate::paste_sequence::SequenceDraft::new(
                "Prompted run",
                vec![crate::paste_sequence::Step::SnippetRef {
                    alias: "ticketreply".into(),
                }],
            ))
            .unwrap();
        let context = crate::paste_sequence::SequenceInvocationContext::new(
            chrono::DateTime::parse_from_rfc3339("2024-01-02T03:04:05+02:00").unwrap(),
            None,
            crate::paste_sequence::SequenceTarget::from_external_window(
                external_window(10, "origin"),
                std::process::id(),
            )
            .unwrap(),
        );
        let mut sequence_app = inert_test_app(directory.path());
        sequence_app
            .begin_paste_sequence_preflight_with_context(
                snapshot.sequence(id).unwrap().clone(),
                &[prompted_entry()],
                &[],
                context,
            )
            .unwrap();
        assert!(sequence_app.snippet_prompt_dialog.is_sequence_preflight());
        sequence_app.handle_paste_sequence_hotkey(PasteSequenceHotkeyAction::Cancel);
        assert!(!sequence_app.snippet_prompt_dialog.is_open());
        assert_eq!(
            sequence_app.sequence_playback.status(),
            crate::paste_sequence::SequencePlaybackStatus::Cancelled
        );
    }

    #[test]
    fn paste_sequence_titlebar_close_ends_the_taken_dialog_before_restore() {
        let directory = tempfile::tempdir().unwrap();
        let mut app = inert_test_app(directory.path());
        app.paste_sequence_dialog.open = true;
        app.panel_states.paste_sequence_dialog = true;
        app.panel_stack
            .push(super::super::Panel::PasteSequenceDialog);

        let mut local_dialog = std::mem::take(&mut app.paste_sequence_dialog);
        local_dialog.close_from_window(&mut app);
        app.paste_sequence_dialog = local_dialog;

        assert!(!app.paste_sequence_dialog.open);
        assert!(!app.panel_states.paste_sequence_dialog);
        assert!(
            !app.panel_stack
                .contains(&super::super::Panel::PasteSequenceDialog)
        );
    }

    fn inert_test_app(root: &std::path::Path) -> LauncherApp {
        let ctx = eframe::egui::Context::default();
        let mut settings = crate::settings::Settings::default();
        settings.hotkey = None;
        settings.quit_hotkey = None;
        settings.help_hotkey = None;
        settings.dashboard.enabled = false;
        settings.enable_toasts = false;
        settings.show_inline_errors = false;
        settings.show_error_toasts = false;
        let mut app = LauncherApp::new(
            &ctx,
            std::sync::Arc::new(Vec::new()),
            0,
            crate::plugin::PluginManager::new_inert_for_test(),
            root.join("actions.json").to_string_lossy().into_owned(),
            root.join("settings.json").to_string_lossy().into_owned(),
            settings,
            None,
            None,
            Some(std::collections::HashSet::new()),
            Some(std::collections::HashMap::new()),
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        );
        app.dashboard_initial_refresh_queued = true;
        app
    }

    fn test_app() -> LauncherApp {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.keep();
        LauncherApp::new(
            &eframe::egui::Context::default(),
            std::sync::Arc::new(Vec::new()),
            0,
            crate::plugin::PluginManager::new(),
            root.join("actions.json").to_string_lossy().into_owned(),
            root.join("settings.json").to_string_lossy().into_owned(),
            crate::settings::Settings::default(),
            None,
            None,
            None,
            None,
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true)),
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
    }

    fn prompted_entry() -> crate::plugins::snippets::SnippetEntry {
        crate::plugins::snippets::SnippetEntry {
            alias: "ticketreply".into(),
            text: "Hello {{name}}".into(),
            hide_contents: false,
            prompt_for_fields: true,
            fields: vec![crate::plugins::snippets::SnippetFieldDefinition::new(
                "name",
            )],
            tags: Vec::new(),
        }
    }

    fn prompt_intent(
        entry: crate::plugins::snippets::SnippetEntry,
    ) -> crate::commands::SnippetPromptIntent {
        let prepared = match crate::plugins::snippets::prepare_snippet_run(&entry).unwrap() {
            crate::plugins::snippets::SnippetRunMode::Prompted(prepared) => prepared,
            crate::plugins::snippets::SnippetRunMode::Plain
            | crate::plugins::snippets::SnippetRunMode::SystemOnly(_) => unreachable!(),
        };
        crate::commands::SnippetPromptIntent {
            alias: entry.alias.clone(),
            entry_snapshot: entry,
            prepared,
            safe_action: crate::actions::Action {
                label: "ticketreply".into(),
                desc: "Snippet".into(),
                action: crate::plugins::snippets::snippet_run_action("ticketreply"),
                args: None,
            },
            source: crate::commands::ActivationSource::Dashboard,
            history_query: "captured snippet query".into(),
            root_policy: crate::universal_actions::RootLauncherPolicy::PreserveOrdinaryState,
            context: None,
        }
    }

    fn external_window(hwnd: isize, title: &str) -> crate::active_window::ExternalForegroundWindow {
        crate::active_window::ExternalForegroundWindow {
            hwnd,
            process_id: 20,
            thread_id: 30,
            title: Some(title.into()),
        }
    }

    #[test]
    fn snippet_origin_prefers_a_fresh_external_foreground_for_direct_invocations() {
        let cached = external_window(10, "old window");
        for source in [
            crate::commands::ActivationSource::Macro,
            crate::commands::ActivationSource::Gesture,
            crate::commands::ActivationSource::Dashboard,
        ] {
            let selected = select_snippet_origin_window(
                source,
                true,
                crate::active_window::ForegroundWindowObservation::External(external_window(
                    20,
                    "current window",
                )),
                Some(cached.clone()),
            );
            assert_eq!(selected.map(|window| window.hwnd), Some(20));
        }
    }

    #[test]
    fn snippet_origin_uses_cached_window_only_while_visible_launcher_owns_foreground() {
        let cached = external_window(10, "session origin");
        for source in [
            crate::commands::ActivationSource::Click,
            crate::commands::ActivationSource::Enter,
        ] {
            let selected = select_snippet_origin_window(
                source,
                true,
                crate::active_window::ForegroundWindowObservation::Launcher,
                Some(cached.clone()),
            );
            assert_eq!(selected.map(|window| window.hwnd), Some(10));

            assert!(
                select_snippet_origin_window(
                    source,
                    false,
                    crate::active_window::ForegroundWindowObservation::Launcher,
                    Some(cached.clone()),
                )
                .is_none()
            );
        }

        assert!(
            select_snippet_origin_window(
                crate::commands::ActivationSource::Macro,
                true,
                crate::active_window::ForegroundWindowObservation::Unavailable,
                Some(cached.clone()),
            )
            .is_none()
        );
    }

    #[test]
    fn radial_snippet_invocations_never_fall_back_to_cached_origin() {
        let cached = external_window(10, "session origin");
        for source in [
            crate::commands::ActivationSource::RadialRelease,
            crate::commands::ActivationSource::RadialShortcut,
            crate::commands::ActivationSource::RadialHotstring,
        ] {
            assert!(
                select_snippet_origin_window(
                    source,
                    true,
                    crate::active_window::ForegroundWindowObservation::Launcher,
                    Some(cached.clone()),
                )
                .is_none()
            );

            let selected = select_snippet_origin_window(
                source,
                true,
                crate::active_window::ForegroundWindowObservation::External(external_window(
                    20,
                    "radial caller",
                )),
                Some(cached.clone()),
            );
            assert_eq!(selected.map(|window| window.hwnd), Some(20));
        }
    }

    #[test]
    fn sequence_target_requires_a_real_origin_except_for_launcher_ui_activation() {
        let cached = external_window(10, "session origin");
        let ui_target = select_sequence_origin_window(
            crate::commands::ActivationSource::Click,
            true,
            crate::active_window::ForegroundWindowObservation::Launcher,
            Some(cached.clone()),
        )
        .unwrap();
        assert_eq!(ui_target.hwnd, 10);
        assert_eq!(ui_target.process_id, 20);
        assert_eq!(ui_target.thread_id, 30);

        for source in [
            crate::commands::ActivationSource::Macro,
            crate::commands::ActivationSource::Gesture,
            crate::commands::ActivationSource::RadialShortcut,
        ] {
            assert!(
                select_sequence_origin_window(
                    source,
                    true,
                    crate::active_window::ForegroundWindowObservation::Launcher,
                    Some(cached.clone()),
                )
                .is_none()
            );
        }

        let fresh = external_window(11, "current origin");
        assert_eq!(
            select_sequence_origin_window(
                crate::commands::ActivationSource::Macro,
                true,
                crate::active_window::ForegroundWindowObservation::External(fresh),
                Some(cached),
            )
            .map(|window| window.hwnd),
            Some(11)
        );
    }

    #[test]
    fn new_sequence_preflight_is_rejected_while_an_older_run_is_ready() {
        let directory = tempfile::tempdir().unwrap();
        let store = crate::paste_sequence::SequenceStore::new(
            directory
                .path()
                .join(crate::paste_sequence::PASTE_SEQUENCES_FILE),
        );
        let mut snapshot = store.load().unwrap();
        let old_id = snapshot
            .upsert(crate::paste_sequence::SequenceDraft::new(
                "Old run",
                vec![crate::paste_sequence::Step::LiteralText {
                    text: "old payload".into(),
                }],
            ))
            .unwrap();
        let context = crate::paste_sequence::SequenceInvocationContext::new(
            chrono::DateTime::parse_from_rfc3339("2024-01-02T03:04:05+02:00").unwrap(),
            None,
            crate::paste_sequence::SequenceTarget::from_external_window(
                external_window(10, "origin"),
                std::process::id(),
            )
            .unwrap(),
        );
        let old_run = crate::paste_sequence::SequencePreflight::begin(
            snapshot.sequence(old_id).unwrap().clone(),
            &[],
            &[],
            context.clone(),
        )
        .unwrap()
        .finish(&[], &[])
        .unwrap();
        let failed_id = snapshot
            .upsert(crate::paste_sequence::SequenceDraft::new(
                "Missing snippet",
                vec![crate::paste_sequence::Step::SnippetRef {
                    alias: "missing".into(),
                }],
            ))
            .unwrap();
        let mut app = inert_test_app(directory.path());
        app.sequence_playback.begin_preparing().unwrap();
        app.sequence_playback.set_ready(old_run).unwrap();

        let result = app.begin_paste_sequence_preflight_using_context(
            snapshot.sequence(failed_id).unwrap().clone(),
            &[],
            &[],
            crate::commands::ActivationSource::Click,
            move |_, _| {
                let _ = context;
                panic!("overlapping sequence attempts must be rejected before capture")
            },
        );
        assert!(matches!(
            result,
            Err(crate::paste_sequence::SequencePreflightError::SequenceAlreadyActive)
        ));
        assert_eq!(
            app.sequence_playback.status(),
            crate::paste_sequence::SequencePlaybackStatus::Ready
        );
    }

    #[test]
    fn prompt_request_opens_and_focuses_one_panel_from_hidden_or_visible_root() {
        for initially_visible in [false, true] {
            let mut app = test_app();
            app.visible_flag
                .store(initially_visible, std::sync::atomic::Ordering::SeqCst);
            let entry = prompted_entry();
            let before = app.launcher_interaction_snapshot();
            crate::commands::HeadlessCommandHost::request_snippet_prompt(
                &mut app,
                prompt_intent(entry.clone()),
            )
            .unwrap();
            app.update_panel_stack();
            app.restore_for_new_launcher_interaction(&before);

            assert!(app.visible_flag.load(std::sync::atomic::Ordering::SeqCst));
            assert!(app.panel_states.snippet_prompt_dialog);
            assert_eq!(
                app.panel_stack.last(),
                Some(&super::super::Panel::SnippetPromptDialog)
            );
            assert_eq!(
                app.panel_stack
                    .iter()
                    .filter(|panel| **panel == super::super::Panel::SnippetPromptDialog)
                    .count(),
                1
            );
            let first_generation = app.snippet_prompt_dialog.session().unwrap().generation;
            app.snippet_prompt_dialog
                .set_value("name", "filled sentinel".into());

            crate::commands::HeadlessCommandHost::request_snippet_prompt(
                &mut app,
                prompt_intent(entry),
            )
            .unwrap();
            let session = app.snippet_prompt_dialog.session().unwrap();
            assert_ne!(session.generation, first_generation);
            assert_eq!(session.values.get("name").map(String::as_str), Some(""));
            assert_eq!(
                app.panel_stack
                    .iter()
                    .filter(|panel| **panel == super::super::Panel::SnippetPromptDialog)
                    .count(),
                1
            );

            assert!(app.close_front_dialog());
            assert!(app.snippet_prompt_dialog.session().is_none());
            assert!(!app.panel_states.snippet_prompt_dialog);
            assert!(!app.is_panel_open(super::super::Panel::SnippetPromptDialog));
        }
    }

    #[test]
    fn prompt_completion_uses_captured_history_and_root_policy_once() {
        let mut app = test_app();
        app.test_skip_history_persistence = true;
        app.query = "later ambient query".into();
        app.hide_after_run = true;
        app.clear_query_after_run = true;
        app.visible_flag
            .store(false, std::sync::atomic::Ordering::SeqCst);
        let entry = prompted_entry();
        crate::commands::HeadlessCommandHost::request_snippet_prompt(
            &mut app,
            prompt_intent(entry.clone()),
        )
        .unwrap();
        app.snippet_prompt_dialog.set_value("name", "Ada".into());
        app.command_root_policy = crate::universal_actions::RootLauncherPolicy::Legacy;

        let mut copied = Vec::new();
        app.submit_snippet_prompt_with_test_backends(
            |_| Ok(entry),
            |text| {
                copied.push(text.to_owned());
                Ok(())
            },
        )
        .unwrap();

        assert_eq!(copied, ["Hello Ada"]);
        assert_eq!(
            app.test_recorded_history_queries,
            ["captured snippet query"]
        );
        assert_eq!(app.usage.get("snippet:run:ticketreply"), Some(&1));
        assert_eq!(app.query, "later ambient query");
        assert!(!app.visible_flag.load(std::sync::atomic::Ordering::SeqCst));
        assert!(app.snippet_prompt_dialog.session().is_none());
        assert!(!app.is_panel_open(super::super::Panel::SnippetPromptDialog));

        let duplicate = app.submit_snippet_prompt_with_test_backends(
            |_| panic!("no resolver"),
            |_| panic!("no second clipboard write"),
        );
        assert_eq!(
            duplicate,
            Err(super::super::snippet_prompt_dialog::SnippetPromptError::NoActiveSession)
        );
        assert_eq!(
            app.test_recorded_history_queries,
            ["captured snippet query"]
        );
        assert_eq!(app.usage.get("snippet:run:ticketreply"), Some(&1));
    }

    #[test]
    fn qr_searched_action_opens_exact_payload_without_history_and_reopens_fresh() {
        let mut app = test_app();
        app.plugins.register(Box::new(crate::plugins::qr::QrPlugin));
        app.match_exact = true;
        app.clear_query_after_run = true;
        app.hide_after_run = true;
        app.test_skip_history_persistence = true;
        let payload = "  caf\u{e9}  \u{1f512}\n\"quoted\" \\path kind:secret id:42 ";
        let query = format!("qr {payload}");
        app.query = query.clone();
        app.search();
        let action = app
            .results
            .iter()
            .find(|action| action.action == "qr:open")
            .unwrap()
            .clone();
        app.activate_action(action, None, crate::commands::ActivationSource::Enter);
        assert!(app.qr_dialog.open);
        assert_eq!(app.qr_dialog.source, payload);
        assert_eq!(
            app.qr_dialog.error_correction,
            crate::qr::QrErrorCorrection::Medium
        );
        assert_eq!(app.panel_stack.last(), Some(&super::super::Panel::QrDialog));
        assert_eq!(app.query, query);
        assert!(app.usage.is_empty());
        assert!(app.test_recorded_history_queries.is_empty());

        app.qr_dialog
            .set_error_correction(crate::qr::QrErrorCorrection::High);
        app.qr_dialog.feedback = Some(Ok("old status".into()));
        assert!(app.close_front_dialog());
        app.query = "qr".into();
        app.last_results_valid = false;
        app.search();
        let action = app
            .results
            .iter()
            .find(|action| action.action == "qr:open")
            .unwrap()
            .clone();
        app.activate_action(action, None, crate::commands::ActivationSource::Enter);
        assert!(app.qr_dialog.open);
        assert!(app.qr_dialog.source.is_empty());
        assert_eq!(
            app.qr_dialog.error_correction,
            crate::qr::QrErrorCorrection::Medium
        );
        assert!(app.qr_dialog.feedback.is_none());
        assert_eq!(app.panel_stack.last(), Some(&super::super::Panel::QrDialog));
        assert_eq!(
            app.panel_stack
                .iter()
                .filter(|panel| **panel == super::super::Panel::QrDialog)
                .count(),
            1
        );
        assert_eq!(app.query, "qr");
        assert!(app.usage.is_empty());
        assert!(app.test_recorded_history_queries.is_empty());
    }

    #[test]
    fn qr_dispatch_ignores_query_override_and_resets_dialog() {
        let mut app = test_app();
        app.query = "keep query".into();
        app.qr_dialog.open(Some("old"));
        app.qr_dialog.error_correction = crate::qr::QrErrorCorrection::High;
        let source = "  caf\u{e9}\n\u{1f512} ";
        let invocation = crate::commands::parse_command(
            crate::actions::Action {
                label: "QR".into(),
                desc: String::new(),
                action: "qr:open".into(),
                args: Some(source.into()),
            },
            Some("override".into()),
            crate::commands::ActivationSource::Dashboard,
        )
        .unwrap();
        assert!(!command_accepts_query_override(&invocation.command));
        app.dispatch_command_invocation(invocation);
        assert_eq!(app.query, "keep query");
        assert_eq!(app.qr_dialog.source, source);
        assert_eq!(
            app.qr_dialog.error_correction,
            crate::qr::QrErrorCorrection::Medium
        );
        assert!(app.qr_dialog.open);
        assert_eq!(app.panel_stack.last(), Some(&super::super::Panel::QrDialog));
    }

    #[test]
    fn regex_tester_dispatch_ignores_query_override_and_retains_draft() {
        let mut app = test_app();
        app.query = "keep query".into();
        app.regex_tester_dialog.session.draft.pattern = "draft".into();
        let action = crate::actions::Action {
            label: "Regex".into(),
            desc: String::new(),
            action: "regex:open".into(),
            args: Some("replacement query".into()),
        };
        let invocation = crate::commands::parse_command(
            action,
            Some("override query".into()),
            crate::commands::ActivationSource::Enter,
        )
        .unwrap();
        assert!(!command_accepts_query_override(&invocation.command));
        app.dispatch_command_invocation(invocation.clone());
        app.dispatch_command_invocation(invocation);
        assert!(app.regex_tester_dialog.open);
        assert!(app.any_panel_open());
        assert_eq!(app.query, "keep query");
        assert_eq!(app.regex_tester_dialog.session.draft.pattern, "draft");
        assert_eq!(
            app.panel_stack
                .iter()
                .filter(|panel| **panel == super::super::Panel::RegexTesterDialog)
                .count(),
            1
        );
        assert!(command_accepts_query_override(&Command::Dialog(
            crate::commands::DialogCommand::Help
        )));
    }

    #[test]
    fn ocr_host_dispatch_stages_selection_only_and_rejects_query_override() {
        let mut app = test_app();
        app.last_visible = true;
        app.query = "ocr".into();
        app.selected = Some(0);
        let revision = app.visibility_revision.current();
        let invocation = crate::commands::parse_command(
            action("ocr:start"),
            Some("unrelated override".into()),
            crate::commands::ActivationSource::Enter,
        )
        .unwrap();
        assert!(!command_accepts_query_override(&invocation.command));
        let outcome = crate::commands::CommandBus
            .dispatch(&invocation, &mut app)
            .unwrap();
        assert!(app.ocr_owns_root());
        assert!(app.ocr.controller.operation().is_none());
        assert!(app.ocr.controller.confirmed().is_none());
        assert!(app.ocr.parking.is_none());
        assert!(!app.any_panel_open());
        assert_eq!(app.query, "ocr");
        assert_eq!(app.selected, Some(0));
        assert_eq!(app.visibility_revision.current(), revision);
        assert_eq!(outcome.visibility, crate::commands::VisibilityPolicy::Keep);
        app.shutdown_ocr_selection();
    }

    #[test]
    fn mouse_help_ignores_query_override_and_generic_hide_settings() {
        let mut app = test_app();
        app.query = "keep mouse query".into();
        app.clear_query_after_run = true;
        app.hide_after_run = true;
        app.visible_flag.store(true, Ordering::SeqCst);
        app.focus_query = false;
        let invocation = crate::commands::parse_command(
            action("mouse:help"),
            Some("must be ignored".into()),
            crate::commands::ActivationSource::Dashboard,
        )
        .unwrap();

        assert!(!command_accepts_query_override(&invocation.command));
        app.dispatch_command_invocation(invocation);

        assert_eq!(app.query, "keep mouse query");
        assert!(app.visible_flag.load(Ordering::SeqCst));
        assert!(!app.focus_query);
    }

    #[test]
    fn mouse_settings_command_opens_and_focuses_idle_panel() {
        let mut app = test_app();
        app.visible_flag.store(false, Ordering::SeqCst);
        app.restore_flag.store(false, Ordering::SeqCst);
        let invocation = crate::commands::parse_command(
            action("mouse:settings"),
            None,
            crate::commands::ActivationSource::Enter,
        )
        .unwrap();

        app.dispatch_command_invocation(invocation);
        assert!(app.is_panel_open(super::super::Panel::MouseSettingsDialog));
        assert_eq!(
            app.panel_stack.last(),
            Some(&super::super::Panel::MouseSettingsDialog)
        );
        assert!(app.visible_flag.load(Ordering::SeqCst));
        assert!(app.restore_flag.load(Ordering::SeqCst));
        assert!(!app.coordinate_tool.is_running());
        assert!(!app.coordinate_tool.runtime_state().hud_enabled());
        assert!(!app.coordinate_tool.runtime_state().crosshair_enabled());
        assert!(!app.coordinate_tool.capture_pending());

        let invocation = crate::commands::parse_command(
            action("mouse:settings"),
            None,
            crate::commands::ActivationSource::Enter,
        )
        .unwrap();
        app.dispatch_command_invocation(invocation);
        assert_eq!(
            app.panel_stack
                .iter()
                .filter(|panel| **panel == super::super::Panel::MouseSettingsDialog)
                .count(),
            1
        );
        app.coordinate_tool.shutdown().unwrap();
    }

    fn with_isolated_root_restore_fixture<T>(
        app: &mut LauncherApp,
        run: impl FnOnce(&mut LauncherApp) -> T,
    ) -> T {
        #[cfg(windows)]
        {
            use crate::radial::acceptance_trace::{
                self, Event, NativeActivationEdge, RootCommandKind,
            };

            assert!(
                !unsafe {
                    windows::Win32::UI::WindowsAndMessaging::IsWindow(
                        windows::Win32::Foundation::HWND(42 as *mut _),
                    )
                }
                .as_bool(),
                "the isolated ROOT fixture must not name a live OS window"
            );
            app.launcher_hwnd = Some(42);
            app.root_window_bridge.set_identity_for_test(42);
            let identity = app
                .root_window_bridge
                .qualify_simulated_wake_for_test()
                .unwrap();
            assert_eq!(identity, app.root_window_bridge.identity());
            let revision = app.visibility_revision.current();
            let (result, queued) =
                crate::window_manager::with_launcher_restore_queue_for_test(|| run(app));
            assert_eq!(app.visibility_revision.current(), revision + 1);
            assert_eq!(queued, [(revision + 1, 42)]);

            let events = acceptance_trace::take_root_activation_test_events();
            let focus = events
                .iter()
                .filter_map(|event| match event {
                    Event::RootCommand {
                        command: RootCommandKind::Focus,
                        correlation,
                    } => Some(*correlation),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let native = events
                .iter()
                .filter_map(|event| match event {
                    Event::NativeActivation {
                        edge: NativeActivationEdge::RestoreRequested,
                        hwnd,
                        correlation,
                    } => Some((*hwnd as usize, *correlation)),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(focus.len(), 1);
            assert_eq!(native.len(), 1);
            assert_eq!(native[0].0, 42);
            assert_eq!(focus[0].visibility_revision, revision + 1);
            assert_eq!(native[0].1.visibility_revision, revision + 1);
            assert_eq!(focus[0].invocation_id, native[0].1.invocation_id);
            assert!(focus[0].request_id > 0 && focus[0].request_id < native[0].1.request_id);
            assert!(!focus[0].terminal && !native[0].1.terminal);
            assert!(!events.iter().any(|event| matches!(
                event,
                Event::NativeActivation {
                    edge: NativeActivationEdge::RestoreCompleted,
                    ..
                }
            )));
            result
        }
        #[cfg(not(windows))]
        run(app)
    }

    fn action(raw: &str) -> crate::actions::Action {
        crate::actions::Action {
            label: raw.into(),
            desc: "Radial menu".into(),
            action: raw.into(),
            args: None,
        }
    }

    #[test]
    fn radial_queryexec_recursion_is_bounded_without_changing_legacy_activation() {
        let queryexec = action("queryexec:queryexec:again");
        let ordinary = action("https://example.test/");

        let mut radial_depth = Some(0);
        assert!(allow_radial_queryexec_activation(
            &mut radial_depth,
            &queryexec
        ));
        assert_eq!(radial_depth, Some(1));
        assert!(!allow_radial_queryexec_activation(
            &mut radial_depth,
            &queryexec
        ));
        assert!(allow_radial_queryexec_activation(
            &mut radial_depth,
            &ordinary
        ));

        let mut legacy_depth = None;
        assert!(allow_radial_queryexec_activation(
            &mut legacy_depth,
            &queryexec
        ));
    }

    fn virtual_desktop_create_invocation() -> CommandInvocation {
        CommandInvocation {
            command: Command::VirtualDesktop(crate::commands::VirtualDesktopCommand::Create),
            original_action: action("vd:create"),
            query_override: None,
            source: crate::commands::ActivationSource::RadialRelease,
        }
    }

    #[test]
    fn immediate_virtual_desktop_hide_does_not_mutate_preserved_launcher_root() {
        let workspace = crate::performance::track_c_workloads::TrackCWorkspace::new();
        let mut app = inert_test_app(workspace.root());
        app.hide_after_run = true;
        app.clear_query_after_run = true;
        app.command_root_policy =
            crate::universal_actions::RootLauncherPolicy::PreserveOrdinaryState;
        app.query = "keep root query".into();
        app.results = vec![action("help:show")];
        app.selected = Some(0);
        app.visible_flag.store(true, Ordering::SeqCst);
        app.restore_flag.store(false, Ordering::SeqCst);
        app.focus_query = false;

        let invocation = virtual_desktop_create_invocation();
        let success = crate::commands::handlers::success_outcome(&app, &invocation);
        assert_eq!(success.visibility, VisibilityPolicy::Hide);
        let (immediate, completion) = split_virtual_desktop_outcome(success);
        assert_eq!(immediate.visibility, VisibilityPolicy::Hide);
        assert_eq!(immediate.query, QueryPolicy::Set(String::new()));
        assert_eq!(completion.history, HistoryPolicy::Record);

        app.apply_command_outcome(immediate, &invocation);

        assert_eq!(app.query, "keep root query");
        assert_eq!(app.results, vec![action("help:show")]);
        assert_eq!(app.selected, Some(0));
        assert!(app.visible_flag.load(Ordering::SeqCst));
        assert!(!app.restore_flag.load(Ordering::SeqCst));
        assert!(!app.focus_query);
        assert!(app.test_recorded_history_queries.is_empty());
        drop(app);
        drop(workspace);
    }

    #[test]
    fn legacy_command_outcome_still_applies_query_and_hide() {
        let workspace = crate::performance::track_c_workloads::TrackCWorkspace::new();
        let mut app = inert_test_app(workspace.root());
        app.query = "before".into();
        app.visible_flag.store(true, Ordering::SeqCst);
        app.command_root_policy = crate::universal_actions::RootLauncherPolicy::Legacy;
        let invocation = virtual_desktop_create_invocation();

        app.apply_command_outcome(
            CommandOutcome {
                query: QueryPolicy::Set("after".into()),
                visibility: VisibilityPolicy::Hide,
                ..CommandOutcome::default()
            },
            &invocation,
        );

        assert_eq!(app.query, "after");
        assert!(!app.visible_flag.load(Ordering::SeqCst));
        drop(app);
        drop(workspace);
    }

    #[test]
    fn radial_virtual_desktop_dispatch_records_captured_query_after_deferred_completion_once() {
        let workspace = crate::performance::track_c_workloads::TrackCWorkspace::new();
        let mut app = inert_test_app(workspace.root());
        app.query = "ROOT query".into();
        app.hide_after_run = true;
        app.clear_query_after_run = true;
        app.command_root_policy =
            crate::universal_actions::RootLauncherPolicy::PreserveOrdinaryState;
        app.visible_flag.store(true, Ordering::SeqCst);
        app.test_defer_virtual_desktop_completion = true;

        app.dispatch_command_invocation_with_history(
            virtual_desktop_create_invocation(),
            Some("captured radial query"),
        );

        // The event is queued after the real command bus and host dispatch,
        // then reduced separately to model an asynchronous completion.
        assert_eq!(app.query, "ROOT query");
        assert!(app.visible_flag.load(Ordering::SeqCst));
        assert!(app.test_recorded_history_queries.is_empty());
        assert_eq!(app.usage.get("vd:create"), None);

        app.query = "newer ROOT query".into();
        app.request_launcher_state(Some(false), Some(false));
        app.process_watch_events();

        assert_eq!(app.query, "newer ROOT query");
        assert!(!app.visible_flag.load(Ordering::SeqCst));
        assert_eq!(app.test_recorded_history_queries, ["captured radial query"]);
        assert_eq!(app.usage.get("vd:create"), Some(&1));

        app.process_watch_events();
        assert_eq!(app.test_recorded_history_queries, ["captured radial query"]);
        assert_eq!(app.usage.get("vd:create"), Some(&1));
        drop(app);
        drop(workspace);
    }

    #[test]
    fn ordinary_virtual_desktop_dispatch_captures_root_query_before_async_completion() {
        let mut app = test_app();
        app.query = "ordinary ROOT query".into();
        app.command_root_policy = crate::universal_actions::RootLauncherPolicy::Legacy;
        app.test_defer_virtual_desktop_completion = true;

        app.dispatch_command_invocation(virtual_desktop_create_invocation());
        app.query = "ROOT changed while command ran".into();
        app.process_watch_events();

        assert_eq!(app.query, "ROOT changed while command ran");
        assert_eq!(app.test_recorded_history_queries, ["ordinary ROOT query"]);
        assert_eq!(app.usage.get("vd:create"), Some(&1));
    }

    #[test]
    fn radial_show_preserves_launcher_root_and_enqueues_once_with_one_history_record() {
        let (client, endpoint) =
            crate::radial::control::radial_control_service_with_wake(true, None);
        super::super::install_radial_control_client(client);
        let mut app = test_app();
        app.query = "keep query".into();
        app.results = vec![action("help:show")];
        app.selected = Some(0);
        app.visible_flag.store(false, Ordering::SeqCst);
        app.focus_query = false;
        app.activate_action(
            action("radial show Work Menu"),
            None,
            crate::commands::ActivationSource::Enter,
        );
        assert_eq!(app.query, "keep query");
        assert_eq!(app.results[0].action, "help:show");
        assert_eq!(app.selected, Some(0));
        assert!(!app.visible_flag.load(Ordering::SeqCst));
        assert!(!app.focus_query);
        assert_eq!(app.test_recorded_history_queries, ["keep query"]);
        assert_eq!(
            endpoint.request_rx.try_recv().unwrap(),
            crate::radial::control::RadialControlRequest::Show(
                crate::radial::control::RadialMenuSelector::IdOrName("Work Menu".into())
            )
        );
        assert!(endpoint.request_rx.try_recv().is_err());
    }

    #[test]
    fn radial_editor_commands_open_the_stable_panel_and_skins_section() {
        let mut app = test_app();
        app.visible_flag.store(false, Ordering::SeqCst);
        app.activate_action(
            action("radial edit"),
            None,
            crate::commands::ActivationSource::Click,
        );
        assert!(app.radial_editor.lock().unwrap().open);
        assert!(!app.radial_editor.lock().unwrap().is_showing_resources());
        assert!(!app.radial_editor.lock().unwrap().is_dirty());
        assert!(!app.visible_flag.load(Ordering::SeqCst));
        app.activate_action(
            action("radial skins"),
            None,
            crate::commands::ActivationSource::Click,
        );
        assert!(app.radial_editor.lock().unwrap().open);
        assert!(app.radial_editor.lock().unwrap().is_showing_resources());
        assert!(!app.radial_editor.lock().unwrap().is_dirty());
        app.activate_action(
            action("radial edit"),
            None,
            crate::commands::ActivationSource::Click,
        );
        assert!(app.radial_editor.lock().unwrap().open);
        assert!(!app.radial_editor.lock().unwrap().is_showing_resources());
    }

    #[test]
    fn launcher_host_routes_screen_draw_commands_only_through_the_controller() {
        let mut app = test_app();
        ScreenDrawCommandHost::execute_screen_draw_command(
            &mut app,
            crate::commands::ScreenDrawCommand::Start,
        )
        .unwrap();
        assert!(matches!(
            app.screen_draw_controller.state(),
            crate::screen_draw::ScreenDrawState::AwaitingLauncherParking { .. }
        ));
        assert!(!app.screen_draw_controller.toolbar_open());
        let generation = app.screen_draw_controller.state().generation();
        ScreenDrawCommandHost::execute_screen_draw_command(
            &mut app,
            crate::commands::ScreenDrawCommand::Start,
        )
        .unwrap();
        assert_eq!(app.screen_draw_controller.state().generation(), generation);
        assert!(!app.screen_draw_controller.toolbar_open());

        with_isolated_root_restore_fixture(&mut app, |app| {
            ScreenDrawCommandHost::execute_screen_draw_command(
                app,
                crate::commands::ScreenDrawCommand::Close,
            )
            .unwrap();
        });
        ScreenDrawCommandHost::execute_screen_draw_command(
            &mut app,
            crate::commands::ScreenDrawCommand::OpenToolbar,
        )
        .unwrap();
        assert_eq!(
            app.screen_draw_controller.state(),
            &crate::screen_draw::ScreenDrawState::NoSession
        );
        assert!(app.screen_draw_controller.toolbar_open());
    }

    #[test]
    fn repeated_screen_draw_start_preserves_active_session_in_drawing_ghost_and_finish() {
        for safe_mode in [None, Some(false), Some(true)] {
            let mut app = test_app();
            let generation = app.screen_draw_controller.request_start().unwrap();
            app.screen_draw_controller
                .launcher_parked(generation)
                .unwrap();
            app.screen_draw_controller
                .capture_succeeded(generation)
                .unwrap();
            app.screen_draw_controller.install_test_session_snapshot(
                generation,
                crate::mkmacro::screen::CapturedRegion {
                    image: image::RgbaImage::from_pixel(3, 2, image::Rgba([7, 8, 9, 255])),
                    origin: (-3, 4),
                },
            );
            let (commands, _) = app.screen_draw_controller.install_test_native_worker();
            match safe_mode {
                None => {}
                Some(false) => app.screen_draw_controller.enter_ghost().unwrap(),
                Some(true) => app.screen_draw_controller.finish().unwrap(),
            }
            let _ = commands.try_iter().collect::<Vec<_>>();
            let snapshot = std::sync::Arc::clone(
                app.screen_draw_controller
                    .session_snapshot()
                    .unwrap()
                    .capture(),
            );

            ScreenDrawCommandHost::execute_screen_draw_command(
                &mut app,
                crate::commands::ScreenDrawCommand::Start,
            )
            .unwrap();

            assert_eq!(
                app.screen_draw_controller.state().generation(),
                Some(generation)
            );
            assert!(app.screen_draw_controller.toolbar_open());
            assert!(std::sync::Arc::ptr_eq(
                &snapshot,
                app.screen_draw_controller
                    .session_snapshot()
                    .unwrap()
                    .capture()
            ));
            assert!(commands.try_recv().is_err());
        }
    }

    #[test]
    fn screen_draw_new_restores_before_waiting_for_native_teardown_then_restarts() {
        let mut app = test_app();
        let first = app.screen_draw_controller.request_start().unwrap();
        app.screen_draw_controller.launcher_parked(first).unwrap();
        app.screen_draw_controller.capture_succeeded(first).unwrap();
        let (commands, events) = app.screen_draw_controller.install_test_native_worker();
        let original = crate::launcher_parking::LauncherWindowRect {
            left: 15,
            top: 25,
            right: 415,
            bottom: 245,
        };
        let (mut parking, observer) = crate::launcher_parking::launcher_parking_test_fixture(
            first,
            original,
            crate::mkmacro::screen::ScreenRect::new(0, 0, 1920, 1080),
        );
        parking.commit_hidden();
        app.screen_draw_launcher_parking = Some(parking);

        with_isolated_root_restore_fixture(&mut app, |app| {
            ScreenDrawCommandHost::execute_screen_draw_command(
                app,
                crate::commands::ScreenDrawCommand::NewCapture,
            )
            .unwrap();
        });
        let replacement = app.screen_draw_controller.state().generation().unwrap();
        assert_ne!(first, replacement);
        assert!(matches!(
            app.screen_draw_controller.state(),
            crate::screen_draw::ScreenDrawState::AwaitingNativeTeardown { .. }
        ));
        assert_eq!(observer.restored_rects(), [original]);
        assert!(app.screen_draw_launcher_parking.is_none());
        assert!(matches!(
            commands.try_recv(),
            Ok(crate::screen_draw::NativeSessionCommand::Shutdown)
        ));

        events
            .send(crate::screen_draw::NativeSessionEvent::SessionClosed)
            .unwrap();
        app.screen_draw_controller
            .poll_capture(Some(1), std::sync::Arc::new(|| {}));
        assert_eq!(
            app.screen_draw_controller.state(),
            &crate::screen_draw::ScreenDrawState::AwaitingLauncherParking {
                generation: replacement
            }
        );
    }

    #[test]
    fn screen_draw_new_discards_an_already_restored_resume_snapshot() {
        let mut app = test_app();
        let generation = app.screen_draw_controller.request_start().unwrap();
        app.screen_draw_controller
            .launcher_parked(generation)
            .unwrap();
        app.screen_draw_controller
            .capture_succeeded(generation)
            .unwrap();
        let original = crate::launcher_parking::LauncherWindowRect {
            left: -500,
            top: 80,
            right: -100,
            bottom: 300,
        };
        let (mut parking, observer) = crate::launcher_parking::launcher_parking_test_fixture(
            generation,
            original,
            crate::mkmacro::screen::ScreenRect::new(-1920, 0, 1920, 1080),
        );
        parking.commit_hidden();
        parking.restore().unwrap();
        app.screen_draw_launcher_parking = Some(parking);

        with_isolated_root_restore_fixture(&mut app, |app| {
            ScreenDrawCommandHost::execute_screen_draw_command(
                app,
                crate::commands::ScreenDrawCommand::NewCapture,
            )
            .unwrap();
        });

        assert!(matches!(
            app.screen_draw_controller.state(),
            crate::screen_draw::ScreenDrawState::AwaitingLauncherParking { .. }
        ));
        assert_eq!(observer.restored_rects(), [original]);
        assert!(app.screen_draw_launcher_parking.is_none());
    }

    #[test]
    fn screen_draw_close_restores_after_disarming_drawing_ghost_and_idle() {
        for ghost in [false, true] {
            let mut app = test_app();
            let generation = app.screen_draw_controller.request_start().unwrap();
            app.screen_draw_controller
                .launcher_parked(generation)
                .unwrap();
            app.screen_draw_controller
                .capture_succeeded(generation)
                .unwrap();
            let (commands, _) = app.screen_draw_controller.install_test_native_worker();
            if ghost {
                app.screen_draw_controller.enter_ghost().unwrap();
            }
            let original = crate::launcher_parking::LauncherWindowRect {
                left: 40,
                top: 50,
                right: 440,
                bottom: 270,
            };
            let (mut parking, observer) = crate::launcher_parking::launcher_parking_test_fixture(
                generation,
                original,
                crate::mkmacro::screen::ScreenRect::new(0, 0, 1920, 1080),
            );
            parking.commit_hidden();
            app.screen_draw_launcher_parking = Some(parking);

            with_isolated_root_restore_fixture(&mut app, |app| {
                ScreenDrawCommandHost::execute_screen_draw_command(
                    app,
                    crate::commands::ScreenDrawCommand::Close,
                )
                .unwrap();
            });

            assert_eq!(
                app.screen_draw_controller.state(),
                &crate::screen_draw::ScreenDrawState::NoSession
            );
            assert_eq!(observer.restored_rects(), [original]);
            assert!(!app.screen_draw_recovery_bridge.is_active());
            assert!(matches!(
                commands.try_iter().last(),
                Some(crate::screen_draw::NativeSessionCommand::Shutdown)
            ));
        }

        let mut idle = test_app();
        with_isolated_root_restore_fixture(&mut idle, |idle| {
            ScreenDrawCommandHost::execute_screen_draw_command(
                idle,
                crate::commands::ScreenDrawCommand::Close,
            )
            .unwrap();
        });
        assert_eq!(
            idle.screen_draw_controller.state(),
            &crate::screen_draw::ScreenDrawState::NoSession
        );
        assert!(idle.visible_flag.load(Ordering::SeqCst));
    }

    #[test]
    fn clipboard_modify_commands_reject_query_override_reclassification() {
        assert!(!command_accepts_query_override(&Command::ClipboardModify(
            crate::commands::ClipboardModifyCommand::Open {
                section: crate::clipboard_modify::actions::ClipboardModifySectionPayload::Modify,
            },
        )));
    }

    #[test]
    fn json_utility_commands_reject_query_override_reclassification() {
        assert!(!command_accepts_query_override(&Command::JsonUtility(
            crate::commands::JsonUtilityCommand::Open {
                intent: crate::commands::JsonUtilityIntent::Format,
            },
        )));
    }

    #[test]
    fn hash_generator_commands_reject_query_override_reclassification() {
        assert!(!command_accepts_query_override(&Command::HashGenerator(
            crate::commands::HashGeneratorCommand::Open,
        )));
    }

    #[test]
    fn m7f_xml_toolkit_command_does_not_turn_query_override_into_source() {
        assert!(!command_accepts_query_override(&Command::XmlToolkit(
            crate::commands::XmlToolkitCommand::Open,
        )));
    }

    #[test]
    fn radial_commands_reject_query_override_reclassification() {
        assert!(!command_accepts_query_override(&Command::Radial(
            crate::commands::RadialCommand::ShowDefault,
        )));
    }

    #[test]
    fn screenshot_commands_keep_query_override_compatibility() {
        for command in [
            Command::Screenshot(crate::commands::ScreenshotCommand::Capture {
                mode: ScreenshotMode::Region,
                destination: ScreenshotDestination::Editor,
                markup: ScreenshotMarkup::Pen,
                compatibility: crate::commands::ScreenshotCompatibility::GuiOnly,
            }),
            Command::Screenshot(crate::commands::ScreenshotCommand::UnknownMode {
                raw: "future".into(),
            }),
        ] {
            assert!(command_accepts_query_override(&command));
        }
    }

    #[test]
    fn data_commands_keep_query_override_compatibility() {
        for command in [
            Command::Data(crate::commands::DataCommand::Dialog),
            Command::Data(crate::commands::DataCommand::Health),
            Command::Data(crate::commands::DataCommand::Backup),
            Command::Data(crate::commands::DataCommand::OpenFolder),
        ] {
            assert!(command_accepts_query_override(&command));
        }
    }
    #[test]
    fn color_pick_rejects_query_override_reclassification() {
        assert!(!command_accepts_query_override(&Command::ColorPick(
            crate::commands::ColorPickCommand::Pick
        )));
    }
}
