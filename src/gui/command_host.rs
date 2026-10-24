use crate::dashboard::DashboardRefreshRequest;
use std::sync::atomic::Ordering;

use crate::commands::{
    BatchRenameCommandHost, CalendarCommandHost, ClipboardModifyCommandHost, ColorPickCommandHost,
    Command, CommandError, CommandInvocation, CommandOutcome, CropCommandHost, DataCommandHost,
    DialogCommandHost, DiffCommandHost, FavoriteLogPolicy, FileSearchCommandHost,
    HeadlessCommandHost, HistoryPolicy, JsonUtilityCommandHost, LauncherCommandHost,
    MouseGestureCommandHost, MultiManagerCommandHost, NoteCommandHost, PendingQueryPolicy,
    PinboardCommandHost, QueryPolicy, RadialCommandHost, ResultsPolicy, ScreenDrawCommandHost,
    ScreenshotCommandHost, ScreenshotCommandResult, ScreenshotDelay, ScreenshotDestination,
    ScreenshotMarkup, ScreenshotMode, ToastPolicy, TodoCommandHost, VisibilityPolicy,
    XmlToolkitCommandHost,
};

use super::{LauncherApp, Toast, ToastKind, ToastOptions};
use crate::performance::track_c::EventOrigin;

fn choose_paste_open_context(
    use_empty_qa_context: bool,
    capture_context: impl FnOnce() -> Result<
        crate::paste_sequence::SequenceInvocationContext,
        crate::paste_sequence::SequencePreflightError,
    >,
) -> Result<
    crate::paste_sequence::SequenceInvocationContext,
    crate::paste_sequence::SequencePreflightError,
> {
    if use_empty_qa_context {
        Ok(crate::paste_sequence::SequenceInvocationContext::new(
            chrono::Local::now().fixed_offset(),
            None,
            None,
        ))
    } else {
        capture_context()
    }
}

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

impl LauncherApp {
    pub(super) fn paste_sequence_invocation_context_for_open(
        &mut self,
    ) -> Result<
        crate::paste_sequence::SequenceInvocationContext,
        crate::paste_sequence::SequencePreflightError,
    > {
        #[cfg(feature = "qa-framebuffer-capture")]
        let use_empty_qa_context = self.qa_framebuffer_use_empty_paste_invocation_context();
        #[cfg(not(feature = "qa-framebuffer-capture"))]
        let use_empty_qa_context = false;
        choose_paste_open_context(use_empty_qa_context, || {
            self.capture_sequence_context(crate::commands::ActivationSource::Click)
        })
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

    fn open_image_compare_dialog(&mut self) {
        self.focus_panel(super::Panel::ImageCompareDialog);
    }

    fn open_duplicate_finder_dialog(&mut self) {
        self.focus_panel(super::Panel::DuplicateFinderDialog);
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
        let snapshot = crate::paste_sequence::SequenceStore::new(
            self.paste_sequence_dialog.sequence_store_path(),
        )
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
        let presentation_sequence = sequence.clone();
        if let Err(error) = self.begin_paste_sequence_preflight_using_context_after_capture(
            sequence,
            &snippets,
            &history,
            source,
            |app, source| app.capture_sequence_context(source),
            move |app, context| {
                if app.should_keep_paste_sequence_dialog_visible_during_playback() {
                    app.present_saved_paste_sequence_run(&presentation_sequence, context.clone());
                }
            },
        ) {
            self.paste_sequence_dialog.set_autostart_when_ready(false);
            return Err(format!("Paste sequence could not be prepared: {error:?}"));
        }
        self.paste_sequence_dialog.set_autostart_when_ready(true);
        if !self.snippet_prompt_dialog.is_open() {
            self.paste_sequence_dialog.set_autostart_when_ready(false);
            self.start_sequence_playback()
                .map_err(|error| error.message().to_owned())?;
            self.paste_sequence_dialog.note_playback_started();
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
        self.settings_editor.set_search_query_from_command("");
    }

    fn open_settings_search(&mut self, query: &str) {
        LauncherApp::open_settings_dialog(self);
        self.settings_editor.set_search_query_from_command(query);
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
        self.open_brightness_settings_dialog();
    }

    fn open_cpu_list_dialog(&mut self, count: usize) {
        self.cpu_list_dialog.open(count);
    }
}

impl LauncherApp {
    fn sequence_escape_requested(&self) -> bool {
        self.sequence_routes.as_ref().map_or(
            self.settings_editor
                .baseline_settings
                .paste_sequence
                .escape_cancel_enabled,
            |routes| routes.escape_cancel_enabled,
        )
    }

    fn sequence_input_lease(
        owner: u64,
        queue_live: bool,
        enabled: bool,
        escape: bool,
        run: crate::paste_sequence::StartedSequence,
    ) -> Result<
        Option<crate::paste_sequence::SessionInputLease>,
        crate::paste_sequence::SequencePlaybackError,
    > {
        use crate::paste_sequence::admission::{SessionCapabilities, physical_paste_bridge};
        use crate::paste_sequence::{PlaybackMode, SequencePlaybackError, SessionInputLease};
        let paste = run.mode == PlaybackMode::PasteToAdvance;
        if !queue_live || !enabled {
            return if paste {
                Err(SequencePlaybackError::ObserverUnavailable)
            } else {
                Ok(None)
            };
        }
        let bridge = physical_paste_bridge();
        bridge
            .configure_session(owner, run.generation, SessionCapabilities { paste, escape })
            .map_err(|_| SequencePlaybackError::ObserverUnavailable)?;
        Ok(Some(SessionInputLease {
            owner,
            generation: run.generation,
            bridge: std::sync::Arc::clone(bridge),
        }))
    }

    fn execute_physical_paste_using(
        &mut self,
        admission: crate::paste_sequence::admission::PhysicalPasteAdmission,
        bridge: std::sync::Arc<crate::paste_sequence::admission::PasteAdmissionBridge>,
        platform: &mut impl crate::paste_sequence::SequencePlaybackPlatform,
    ) -> Result<(), crate::paste_sequence::SequencePlaybackError> {
        if admission.owner != self.event_sink.owner_id()
            || !bridge.valid(&admission)
            || self.sequence_exit_requested
        {
            return Err(crate::paste_sequence::SequencePlaybackError::InvalidState);
        }
        self.sequence_playback
            .physical_paste(admission, bridge, platform)
    }

    pub(crate) fn sync_physical_paste_admission(
        &mut self,
    ) -> Result<(), crate::paste_sequence::SequencePlaybackError> {
        let enabled = self
            .enabled_plugins
            .as_ref()
            .is_none_or(|plugins| plugins.contains("paste_sequence"));
        let bridge = crate::paste_sequence::admission::physical_paste_bridge();
        let owner = self.event_sink.owner_id();
        if (!enabled || !self.event_sink.owns_live_queue())
            && self.sequence_playback.has_input_lease()
        {
            self.cancel_sequence_playback();
            return Ok(());
        }
        if let Some(run) = self.sequence_playback.started_run() {
            if bridge.cancellation_for(owner, run.generation) {
                self.cancel_sequence_playback();
                return Ok(());
            }
            match Self::sequence_input_lease(
                owner,
                self.event_sink.owns_live_queue(),
                enabled,
                self.sequence_escape_requested(),
                run,
            ) {
                Ok(lease) => self.sequence_playback.set_input_lease(lease),
                Err(error) => {
                    self.cancel_sequence_playback();
                    return Err(error);
                }
            }
        } else {
            bridge.publish_session(owner, None, Default::default());
        }
        Ok(())
    }

    pub(crate) fn handle_physical_paste_notice(
        &mut self,
        notice: crate::paste_sequence::admission::PhysicalPasteNotice,
    ) {
        use crate::paste_sequence::admission::{PhysicalPasteNotice, physical_paste_bridge};
        match notice {
            PhysicalPasteNotice::Paste(admission) => {
                if admission.owner != self.event_sink.owner_id()
                    || !physical_paste_bridge().valid(&admission)
                {
                    return;
                }
                let mut platform =
                    crate::paste_sequence::NativeSequencePlaybackPlatform::new(self.launcher_hwnd);
                if let Err(error) = self.execute_physical_paste_using(
                    admission,
                    std::sync::Arc::clone(physical_paste_bridge()),
                    &mut platform,
                ) {
                    physical_paste_bridge().delivery_failed();
                    self.cancel_active_paste_sequence();
                    self.report_error_message("paste_sequence.physical", error.message());
                }
                self.schedule_sequence_playback_wake(std::time::Instant::now());
            }
            PhysicalPasteNotice::Busy { owner, generation } => {
                if owner == self.event_sink.owner_id()
                    && self.sequence_playback.physical_paste_generation() == Some(generation)
                {
                    self.report_error_message(
                        "paste_sequence.physical",
                        "The previous paste is still settling. Release V and retry.",
                    );
                }
            }
            PhysicalPasteNotice::Cancel { owner, generation } => {
                if owner == self.event_sink.owner_id()
                    && self
                        .sequence_playback
                        .started_run()
                        .is_some_and(|run| run.generation == generation)
                {
                    self.cancel_active_paste_sequence();
                }
            }
            PhysicalPasteNotice::Failed {
                owner,
                generation,
                reason,
            } => {
                if owner == self.event_sink.owner_id()
                    && self
                        .sequence_playback
                        .started_run()
                        .is_some_and(|run| run.generation == generation)
                {
                    self.cancel_active_paste_sequence();
                    self.report_error_message("paste_sequence.physical", reason);
                }
            }
        }
    }

    pub(crate) fn handle_paste_sequence_hotkey(
        &mut self,
        action: crate::hotkey::launcher_invocation::PasteSequenceHotkeyAction,
    ) {
        self.handle_paste_sequence_hotkey_using(action, Self::next_sequence_playback);
    }

    fn handle_paste_sequence_hotkey_using(
        &mut self,
        action: crate::hotkey::launcher_invocation::PasteSequenceHotkeyAction,
        next: impl FnOnce(&mut Self) -> Result<(), crate::paste_sequence::SequencePlaybackError>,
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
                if let Err(error) = next(self) {
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

impl crate::commands::TableUtilityCommandHost for LauncherApp {
    fn open_table_utility(&mut self, profile: crate::table_utility::TableUtilityProfile) {
        self.table_utility_dialog.open_with_profile(profile);
        self.focus_panel(super::Panel::TableUtilityDialog);
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

impl crate::commands::HotFolderCommandHost for LauncherApp {
    fn execute_hotfolder_command(
        &mut self,
        command: crate::commands::HotFolderCommand,
    ) -> Result<(), String> {
        match command {
            crate::commands::HotFolderCommand::Open => self.open_hotfolder_manager(),
            crate::commands::HotFolderCommand::Pause => {
                if !self.plugin_enabled("hotfolder") {
                    return Err("Hot Folder is disabled in Plugins".into());
                }
                self.hotfolder_runtime
                    .pause()
                    .map(|_| ())
                    .map_err(str::to_owned)?;
            }
            crate::commands::HotFolderCommand::Resume => {
                if !self.plugin_enabled("hotfolder") {
                    return Err("Hot Folder is disabled in Plugins".into());
                }
                self.hotfolder_runtime
                    .resume(self.hotfolder_capability_inputs())
                    .map(|_| ())
                    .map_err(str::to_owned)?;
            }
        }
        Ok(())
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

    fn schedule_delayed_screenshot(
        &mut self,
        delay: ScreenshotDelay,
        mode: ScreenshotMode,
        destination: ScreenshotDestination,
        markup: ScreenshotMarkup,
        invocation: &CommandInvocation,
        captured_history_query: Option<&str>,
    ) -> Result<(), String> {
        self.ensure_delayed_screenshot_admitted()?;
        let history_query = captured_history_query.unwrap_or(&self.query).to_owned();
        #[cfg(feature = "qa-framebuffer-capture")]
        let expected_window_target = self.qa_framebuffer_screenshot_expected_window();
        #[cfg(not(feature = "qa-framebuffer-capture"))]
        let expected_window_target = None;
        self.screenshot_capture.schedule_for_window_target(
            delay,
            mode,
            destination,
            markup,
            invocation,
            &history_query,
            self.command_root_policy,
            std::time::Instant::now(),
            &self.egui_ctx,
            expected_window_target,
        )?;
        let snapshot = self
            .screenshot_capture
            .snapshot()
            .ok_or_else(|| "screenshot request was not retained".to_owned())?;
        if let Err(error) = self
            .screenshot_countdown
            .begin(snapshot, std::time::Instant::now())
        {
            self.screenshot_capture.cancel();
            return Err(error);
        }
        #[cfg(feature = "qa-framebuffer-capture")]
        self.qa_framebuffer_record_screenshot_scheduled(
            &snapshot,
            crate::history::get_history().len(),
        );
        self.egui_ctx
            .request_repaint_of(eframe::egui::ViewportId::ROOT);
        Ok(())
    }

    fn cancel_delayed_screenshot(&mut self) -> Result<(), String> {
        #[cfg(feature = "qa-framebuffer-capture")]
        let capture_request_id = self
            .screenshot_capture
            .snapshot()
            .map(|snapshot| snapshot.request_id);
        let countdown_request_id = self.screenshot_countdown.request_id();
        if self.screenshot_capture.cancel() {
            if let Some(request_id) = countdown_request_id {
                let _ = self
                    .screenshot_countdown
                    .request_close(request_id, &self.egui_ctx);
            }
            #[cfg(feature = "qa-framebuffer-capture")]
            if let Some(request_id) = capture_request_id {
                self.qa_framebuffer_cancel_screenshot_observation(
                    request_id,
                    "cancelled",
                    crate::history::get_history().len(),
                );
            }
            self.egui_ctx
                .request_repaint_of(eframe::egui::ViewportId::ROOT);
        }
        Ok(())
    }
}

impl LauncherApp {
    pub(crate) fn finish_screenshot_capture(
        &mut self,
        completion: super::screenshot_capture::ScreenshotCaptureCompletion,
    ) {
        let Some(image) = completion.image else {
            #[cfg(feature = "qa-framebuffer-capture")]
            self.qa_framebuffer_finish_screenshot_observation(
                completion.request_id,
                completion.capture_finished_at_unix_ms,
                completion.acquisition_count,
                completion.acquired_window_target,
                if completion.error.is_some() {
                    "failed"
                } else {
                    "cancelled"
                },
                crate::history::get_history().len(),
                None,
                false,
                None,
                None,
            );
            if let Some(error) = completion.error {
                self.report_error_message("launcher", format!("Failed: {error}"));
            }
            return;
        };
        #[cfg(feature = "qa-framebuffer-capture")]
        let previous_editor_count = self.screenshot_editors.len();
        let clip = completion.destination == ScreenshotDestination::Clipboard;
        let tool = match completion.markup {
            ScreenshotMarkup::Rectangle => super::MarkupTool::Rectangle,
            ScreenshotMarkup::Pen => super::MarkupTool::Pen,
        };
        match self.deliver_screenshot_capture(image, clip, tool) {
            Ok(crate::plugins::screenshot::ScreenshotLaunchOutcome::Completed) => {
                self.apply_command_outcome_with_root_policy(
                    CommandOutcome {
                        focus: self.screenshot_launcher_should_refocus(),
                        history: HistoryPolicy::Record,
                        ..CommandOutcome::default()
                    },
                    &completion.invocation,
                    Some(&completion.history_query),
                    completion.root_policy,
                );
                #[cfg(feature = "qa-framebuffer-capture")]
                {
                    let mut editor_request_id = None;
                    let mut editor_is_open = false;
                    let mut artifact = None;
                    let mut artifact_error = None;
                    let export_path = self.qa_framebuffer_screenshot_export_path();
                    if self.screenshot_editors.len() > previous_editor_count {
                        if let Some(editor) = self.screenshot_editors.last_mut() {
                            editor.qa_mark_capture_request(completion.request_id.get());
                            editor_request_id = editor.qa_capture_request_id();
                            editor_is_open = editor.open;
                            if let Some(path) = export_path.as_ref().filter(|_| editor.open) {
                                match editor.qa_write_applied_png(path) {
                                    Ok((sha256, width, height)) => {
                                        let file = path
                                            .file_name()
                                            .map(|name| name.to_string_lossy().into_owned())
                                            .unwrap_or_else(|| {
                                                "screenshot-editor-output.png".into()
                                            });
                                        artifact = Some((file, sha256, width, height));
                                    }
                                    Err(error) => artifact_error = Some(error.to_string()),
                                }
                            } else if export_path.is_some() {
                                artifact_error =
                                    Some("the current screenshot editor is not open".into());
                            }
                        }
                    } else if export_path.is_some() {
                        artifact_error = Some(
                            "the completed capture did not open a new screenshot editor".into(),
                        );
                    }
                    self.qa_framebuffer_finish_screenshot_observation(
                        completion.request_id,
                        completion.capture_finished_at_unix_ms,
                        completion.acquisition_count,
                        completion.acquired_window_target,
                        "completed",
                        crate::history::get_history().len(),
                        editor_request_id,
                        editor_is_open,
                        artifact,
                        artifact_error,
                    );
                }
            }
            Ok(crate::plugins::screenshot::ScreenshotLaunchOutcome::Cancelled) => {
                #[cfg(feature = "qa-framebuffer-capture")]
                self.qa_framebuffer_finish_screenshot_observation(
                    completion.request_id,
                    completion.capture_finished_at_unix_ms,
                    completion.acquisition_count,
                    completion.acquired_window_target,
                    "cancelled",
                    crate::history::get_history().len(),
                    None,
                    false,
                    None,
                    None,
                );
            }
            Err(error) => {
                #[cfg(feature = "qa-framebuffer-capture")]
                self.qa_framebuffer_finish_screenshot_observation(
                    completion.request_id,
                    completion.capture_finished_at_unix_ms,
                    completion.acquisition_count,
                    completion.acquired_window_target,
                    "failed",
                    crate::history::get_history().len(),
                    None,
                    false,
                    None,
                    Some(error.clone()),
                );
                self.report_error_message("launcher", format!("Failed: {error}"));
            }
        }
    }

    fn deliver_screenshot_capture(
        &mut self,
        image: image::RgbaImage,
        clip: bool,
        tool: super::MarkupTool,
    ) -> Result<crate::plugins::screenshot::ScreenshotLaunchOutcome, String> {
        #[cfg(test)]
        if let Some(deliver) = self.test_screenshot_delivery.take() {
            return deliver(image, clip, tool);
        }

        crate::plugins::screenshot::deliver_captured_image(self, image, clip, tool)
            .map_err(|error| error.to_string())
    }
}

impl PinboardCommandHost for LauncherApp {
    fn start_pinboard_capture(&mut self) -> Result<bool, String> {
        let started = self.begin_pinboard_capture()?;
        if started {
            self.egui_ctx
                .request_repaint_of(eframe::egui::ViewportId::ROOT);
        }
        Ok(started)
    }

    fn start_pinboard_live_capture(&mut self) -> Result<bool, String> {
        let started = self.begin_pinboard_live_capture()?;
        if started {
            self.egui_ctx
                .request_repaint_of(eframe::egui::ViewportId::ROOT);
        }
        Ok(started)
    }

    fn open_pinboard_list(&mut self) -> Result<(), String> {
        self.pin_list_open = true;
        self.focus_query = false;
        self.pinboard_capture.request_pin_list_focus();
        if self.visible_flag.load(Ordering::SeqCst) {
            self.egui_ctx
                .request_repaint_of(eframe::egui::ViewportId::ROOT);
        } else {
            self.request_launcher_state(Some(true), Some(true));
        }
        Ok(())
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
            | Command::HotFolder(_)
            | Command::HashGenerator(_)
            | Command::Pinboard(_)
            | Command::ColorPick(_)
            | Command::Ocr(_)
            | Command::CoordinateTool(_)
            | Command::FileSearch(_)
            | Command::Diff(_)
    )
}

impl LauncherApp {
    fn present_saved_paste_sequence_run(
        &mut self,
        sequence: &crate::paste_sequence::Sequence,
        context: crate::paste_sequence::SequenceInvocationContext,
    ) {
        self.paste_sequence_dialog
            .present_saved_run(sequence, context);
        self.panel_states.paste_sequence_dialog = true;
        if !self
            .panel_stack
            .contains(&super::Panel::PasteSequenceDialog)
        {
            self.panel_stack.push(super::Panel::PasteSequenceDialog);
        }
        if !self.visible_flag.load(Ordering::SeqCst) {
            self.request_launcher_visibility(true);
        }
    }

    pub(crate) fn cancel_active_paste_sequence(&mut self) -> bool {
        use crate::paste_sequence::SequencePlaybackStatus as Status;

        if self.snippet_prompt_dialog.is_sequence_preflight() {
            let _ = self.cancel_snippet_prompt();
            return true;
        }
        if matches!(
            self.sequence_playback.status(),
            Status::Preparing
                | Status::Ready
                | Status::Running
                | Status::AwaitAdvance
                | Status::AwaitTarget
                | Status::AwaitPaste
        ) {
            self.cancel_sequence_playback();
            return true;
        }
        false
    }

    /// Capture every invocation input before the sequence prompt or preflight
    /// panel takes focus. An optional external origin supplies template values;
    /// playback independently captures the current target at each step.
    pub(crate) fn capture_sequence_context(
        &mut self,
        source: crate::commands::ActivationSource,
    ) -> Result<
        crate::paste_sequence::SequenceInvocationContext,
        crate::paste_sequence::SequencePreflightError,
    > {
        #[cfg(test)]
        {
            self.test_sequence_context_capture_calls =
                self.test_sequence_context_capture_calls.saturating_add(1);
            assert!(
                !self.test_sequence_context_capture_forbidden,
                "test forbids capture_sequence_context before any OS clipboard access"
            );
        }
        let timestamp = chrono::Local::now().fixed_offset();
        let origin = select_sequence_origin_window(
            source,
            self.visible_flag.load(Ordering::SeqCst),
            crate::active_window::observe_foreground_window(),
            self.snippet_origin_window.clone(),
        );
        let target = origin
            .map(|origin| {
                crate::paste_sequence::SequenceTarget::from_external_window(
                    origin,
                    std::process::id(),
                )
            })
            .transpose()?;
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
        self.begin_paste_sequence_preflight_using_context_after_capture(
            sequence,
            snippets,
            history,
            source,
            capture_context,
            |_, _| {},
        )
    }

    fn begin_paste_sequence_preflight_using_context_after_capture(
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
        after_capture: impl FnOnce(&mut Self, &crate::paste_sequence::SequenceInvocationContext),
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
        after_capture(self, &context);
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
        let mut platform =
            crate::paste_sequence::NativeSequencePlaybackPlatform::new(self.launcher_hwnd);
        self.start_sequence_playback_using(&mut platform)
    }

    pub(super) fn start_sequence_playback_using(
        &mut self,
        platform: &mut impl crate::paste_sequence::SequencePlaybackPlatform,
    ) -> Result<(), crate::paste_sequence::SequencePlaybackError> {
        if self.sequence_exit_requested {
            return Err(crate::paste_sequence::SequencePlaybackError::Busy);
        }
        if self.sequence_playback.prepared_mode()
            == Some(crate::paste_sequence::PlaybackMode::PasteToAdvance)
            && let Some(reason) = crate::paste_sequence::admission::physical_paste_bridge()
                .route_availability()
                .reason()
        {
            self.report_error_message("paste_sequence.physical", &reason);
            return Err(crate::paste_sequence::SequencePlaybackError::RouteConflict);
        }
        let owner = self.event_sink.owner_id();
        let queue_live = self.event_sink.owns_live_queue();
        let enabled = self.plugin_enabled("paste_sequence");
        let escape = self.sequence_escape_requested();
        let result = self
            .sequence_playback
            .start_with_admission(platform, |run| {
                Self::sequence_input_lease(owner, queue_live, enabled, escape, run)
            });
        let result = if self.sequence_playback.status()
            == crate::paste_sequence::SequencePlaybackStatus::Cancelled
        {
            Ok(())
        } else {
            result
        };
        let result = result.and_then(|_| self.sync_physical_paste_admission());
        if result.is_ok()
            && escape
            && self.sequence_playback.started_run().is_some()
            && !self.sequence_playback.escape_cancel_available()
        {
            self.report_error_message("paste_sequence.escape", "Session Escape cancellation is unavailable. Use the configured Cancel shortcut or Cancel Run.");
        }
        if result.is_ok()
            && self.sequence_playback.status()
                != crate::paste_sequence::SequencePlaybackStatus::Cancelled
            && !self.should_keep_paste_sequence_dialog_visible_during_playback()
        {
            self.paste_sequence_dialog.suspend_for_playback();
            self.panel_states.paste_sequence_dialog = false;
            self.panel_stack
                .retain(|panel| *panel != super::Panel::PasteSequenceDialog);
            self.visibility_revision.request(|| {
                self.visible_flag.store(false, Ordering::SeqCst);
                self.restore_flag.store(false, Ordering::SeqCst);
            });
        }
        self.schedule_sequence_playback_wake(platform.now());
        result
    }

    pub(crate) fn next_sequence_playback(
        &mut self,
    ) -> Result<(), crate::paste_sequence::SequencePlaybackError> {
        let now = std::time::Instant::now();
        let mut platform =
            crate::paste_sequence::NativeSequencePlaybackPlatform::new(self.launcher_hwnd);
        self.next_sequence_playback_using(&mut platform, now)
    }

    fn next_sequence_playback_using(
        &mut self,
        platform: &mut impl crate::paste_sequence::SequencePlaybackPlatform,
        now: std::time::Instant,
    ) -> Result<(), crate::paste_sequence::SequencePlaybackError> {
        let result = self.sequence_playback.next(platform, now);
        let result = if self.sequence_playback.status()
            == crate::paste_sequence::SequencePlaybackStatus::Cancelled
        {
            Ok(())
        } else {
            result
        };
        self.schedule_sequence_playback_wake(platform.now());
        result
    }

    pub(crate) fn cancel_sequence_playback(&mut self) {
        crate::paste_sequence::admission::physical_paste_bridge()
            .invalidate_owner(self.event_sink.owner_id());
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
            if self.paste_sequence_dialog.take_autostart_when_ready() {
                match self.start_sequence_playback() {
                    Ok(()) => self.paste_sequence_dialog.note_playback_started(),
                    Err(error) => {
                        self.paste_sequence_dialog
                            .note_playback_start_error(error.message());
                        self.report_error_message("paste_sequence.playback", error.message());
                    }
                }
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
pub(in crate::gui) mod tests {
    use super::*;

    #[cfg(feature = "qa-framebuffer-capture")]
    #[test]
    fn phase2_paste_context_suppression_is_explicit_and_scoped_to_qa_request() {
        let mut capture_called = false;
        let empty = choose_paste_open_context(true, || {
            capture_called = true;
            Err(crate::paste_sequence::SequencePreflightError::InvalidSequence)
        })
        .unwrap();
        assert!(
            !capture_called,
            "QA Paste open must bypass the OS clipboard read"
        );
        assert!(!empty.has_clipboard_text());

        let normal_context = crate::paste_sequence::SequenceInvocationContext::new(
            chrono::Local::now().fixed_offset(),
            Some("test-only sentinel".into()),
            None,
        );
        let normal = choose_paste_open_context(false, || {
            capture_called = true;
            Ok(normal_context)
        })
        .unwrap();
        assert!(
            capture_called,
            "unrequested Paste open keeps the production path"
        );
        assert!(normal.has_clipboard_text());
    }

    pub(crate) struct AppPastePlatform {
        now: std::time::Instant,
        publications: usize,
        text: String,
        target: crate::paste_sequence::SequenceTarget,
        pastes: usize,
        input_eligible: bool,
        target_error: Option<crate::paste_sequence::SequencePlaybackError>,
        pasted: Vec<(isize, String)>,
    }
    impl crate::paste_sequence::SequencePlaybackPlatform for AppPastePlatform {
        fn input_is_eligible(&mut self) -> bool {
            self.input_eligible
        }
        fn now(&mut self) -> std::time::Instant {
            self.now
        }
        fn current_target(
            &mut self,
        ) -> Result<
            crate::paste_sequence::SequenceTarget,
            crate::paste_sequence::SequencePlaybackError,
        > {
            self.target_error
                .map_or_else(|| Ok(self.target.clone()), Err)
        }
        fn observe_target(
            &mut self,
            target: &crate::paste_sequence::SequenceTarget,
        ) -> crate::paste_sequence::TargetObservation {
            if target.hwnd() == self.target.hwnd()
                && target.process_id() == self.target.process_id()
                && target.thread_id() == self.target.thread_id()
            {
                crate::paste_sequence::TargetObservation::Target {
                    password_control: false,
                }
            } else {
                crate::paste_sequence::TargetObservation::Other
            }
        }
        fn publish_text(
            &mut self,
            text: &str,
        ) -> Result<(), crate::paste_sequence::SequencePlaybackError> {
            self.publications += 1;
            self.text = text.into();
            Ok(())
        }
        fn paste_chord(&mut self) -> Result<(), crate::paste_sequence::SequencePlaybackError> {
            self.pastes += 1;
            self.pasted.push((self.target.hwnd(), self.text.clone()));
            Ok(())
        }
    }

    impl AppPastePlatform {
        pub(crate) fn new(target: crate::paste_sequence::SequenceTarget) -> Self {
            Self {
                now: std::time::Instant::now(),
                publications: 0,
                text: "original".into(),
                target,
                pastes: 0,
                input_eligible: true,
                target_error: None,
                pasted: Vec::new(),
            }
        }

        pub(crate) fn pasted_values(&self) -> &[(isize, String)] {
            &self.pasted
        }
    }

    fn playback_settings_fixture(app: &LauncherApp) -> Vec<u8> {
        let settings = app.settings_editor.baseline_settings.clone();
        settings.save(&app.settings_path).unwrap();
        std::fs::read(&app.settings_path).unwrap()
    }

    fn started_physical_app(root: &std::path::Path) -> (LauncherApp, AppPastePlatform) {
        started_session_app(root, crate::paste_sequence::PlaybackMode::PasteToAdvance)
    }

    fn started_session_app(
        root: &std::path::Path,
        mode: crate::paste_sequence::PlaybackMode,
    ) -> (LauncherApp, AppPastePlatform) {
        use crate::paste_sequence::*;
        let mut app = inert_test_app(root);
        app.enabled_plugins = None;
        let target = SequenceTarget::from_external_window(
            crate::active_window::ExternalForegroundWindow {
                hwnd: 91,
                process_id: 92,
                thread_id: 93,
                title: None,
            },
            std::process::id(),
        )
        .unwrap();
        let context = SequenceInvocationContext::new(
            chrono::DateTime::parse_from_rfc3339("2024-01-02T03:04:05+00:00").unwrap(),
            None,
            target.clone(),
        );
        let run = SequencePreflight::begin(
            Sequence {
                id: SequenceId::from_raw(1).unwrap(),
                name: "Physical app fixture".into(),
                hotkey: None,
                mode,
                delay_ms: 0,
                steps: vec![
                    Step::LiteralText { text: "one".into() };
                    if mode == PlaybackMode::PasteToAdvance {
                        1
                    } else {
                        2
                    }
                ],
            },
            &[],
            &[],
            context,
        )
        .unwrap()
        .finish(&[], &[])
        .unwrap();
        app.sequence_playback.begin_preparing().unwrap();
        app.sequence_playback.set_ready(run).unwrap();
        let mut platform = AppPastePlatform::new(target);
        if mode == PlaybackMode::PasteToAdvance {
            app.sequence_playback.start(&mut platform).unwrap();
        } else {
            app.start_sequence_playback_using(&mut platform).unwrap();
        }
        (app, platform)
    }

    #[test]
    fn session_escape_app_uses_persisted_then_acknowledged_preferences_and_exact_cancel() {
        use crate::paste_sequence::admission::*;
        use crate::paste_sequence::*;
        let _guard = crate::gui::TEST_MUTEX.lock().unwrap();
        let root = tempfile::tempdir().unwrap();
        let bridge = physical_paste_bridge();
        let (wake, _rx) = std::sync::mpsc::channel();
        bridge.install_wake(wake);
        bridge.allow_test_routes();
        bridge.observer_live(true);
        let (mut app, _) = started_physical_app(root.path());
        app.settings_editor
            .baseline_settings
            .paste_sequence
            .escape_cancel_enabled = false;
        app.sync_physical_paste_admission().unwrap();
        assert!(!app.sequence_playback.escape_cancel_available());
        app.sequence_routes = Some(SequenceRoutePresentation {
            generation: 9,
            physical_paste: Default::default(),
            next: SequenceShortcutPresentation::Disabled,
            cancel: SequenceShortcutPresentation::Disabled,
            starts: vec![],
            escape_cancel_enabled: true,
            keep_dialog_visible_during_playback: false,
        });
        app.sync_physical_paste_admission().unwrap();
        assert!(app.sequence_playback.escape_cancel_available());
        let generation = app.sequence_playback.started_run().unwrap().generation;
        let owner = app.event_sink.owner_id();
        for (owner, generation) in [(owner + 1, generation), (owner, generation.wrapping_sub(1))] {
            app.handle_physical_paste_notice(PhysicalPasteNotice::Cancel { owner, generation });
            assert_eq!(
                app.sequence_playback.status(),
                SequencePlaybackStatus::AwaitPaste
            );
        }
        app.handle_physical_paste_notice(PhysicalPasteNotice::Cancel { owner, generation });
        assert_eq!(
            app.sequence_playback.status(),
            SequencePlaybackStatus::Cancelled
        );
        assert!(!app.sequence_playback.escape_cancel_available());
        bridge.observer_live(false);
    }

    #[test]
    fn session_escape_app_optional_observer_fallback_preserves_configured_cancel() {
        use crate::paste_sequence::*;
        let _guard = crate::gui::TEST_MUTEX.lock().unwrap();
        let root = tempfile::tempdir().unwrap();
        let bridge = admission::physical_paste_bridge();
        bridge.observer_live(false);
        let (mut app, platform) = started_session_app(root.path(), PlaybackMode::ManualAdvance);
        app.sync_physical_paste_admission().unwrap();
        assert_eq!(platform.pastes, 1);
        assert_eq!(
            app.sequence_playback.status(),
            SequencePlaybackStatus::AwaitAdvance
        );
        assert!(!app.sequence_playback.escape_cancel_available());
        app.settings_editor
            .baseline_settings
            .paste_sequence
            .escape_cancel_enabled = false;
        app.handle_paste_sequence_hotkey_using(
            crate::hotkey::launcher_invocation::PasteSequenceHotkeyAction::Cancel,
            |_| panic!("Cancel must not invoke Next"),
        );
        assert_eq!(
            app.sequence_playback.status(),
            SequencePlaybackStatus::Cancelled
        );
    }

    #[test]
    fn paste_to_advance_app_requires_live_observer_then_executes_exact_owner_permit() {
        use crate::hotkey::launcher_invocation::{InputProvenance, KeyEvent, KeyTransition};
        use crate::paste_sequence::admission::*;
        let _guard = crate::gui::TEST_MUTEX.lock().unwrap();
        let root = tempfile::tempdir().unwrap();
        let bridge = physical_paste_bridge();
        bridge.observer_live(false);
        let (mut unavailable, platform) = started_physical_app(root.path());
        assert_eq!(
            unavailable.sync_physical_paste_admission(),
            Err(crate::paste_sequence::SequencePlaybackError::ObserverUnavailable)
        );
        assert_eq!(platform.pastes, 0);
        assert_eq!(
            unavailable.sequence_playback.status(),
            crate::paste_sequence::SequencePlaybackStatus::Cancelled
        );
        drop(unavailable);
        let (mut app, mut platform) = started_physical_app(root.path());
        let (wake, _rx) = std::sync::mpsc::channel();
        bridge.install_wake(wake);
        bridge.allow_test_routes();
        bridge.observer_live(true);
        app.sync_physical_paste_admission().unwrap();
        let mut reducer = PhysicalPasteReducer::new([false; 9]);
        let focus = PasteFocus {
            hwnd: 91,
            process_id: 92,
            thread_id: 93,
        };
        for vk in [0xA2, 0x56] {
            reducer.process(
                KeyEvent {
                    vk,
                    transition: KeyTransition::Down,
                    provenance: InputProvenance::Physical,
                    at: 0,
                },
                Some(focus),
                std::process::id(),
                true,
                bridge,
            );
        }
        let PhysicalPasteNotice::Paste(admission) = bridge.take_notice().unwrap() else {
            panic!("expected paste")
        };
        let wrong_root = tempfile::tempdir().unwrap();
        let (mut wrong_owner, _) = started_physical_app(wrong_root.path());
        assert_eq!(
            wrong_owner.execute_physical_paste_using(
                admission.clone(),
                std::sync::Arc::clone(bridge),
                &mut platform
            ),
            Err(crate::paste_sequence::SequencePlaybackError::InvalidState)
        );
        assert_eq!(platform.pastes, 0);
        app.execute_physical_paste_using(
            admission.clone(),
            std::sync::Arc::clone(bridge),
            &mut platform,
        )
        .unwrap();
        assert_eq!(platform.pastes, 1);
        assert!(bridge.valid(&admission));
        platform.now += std::time::Duration::from_millis(180);
        let now = platform.now;
        app.sequence_playback.tick(&mut platform, now);
        app.sync_physical_paste_admission().unwrap();
        assert!(!bridge.valid(&admission));
        assert_eq!(platform.text, "one");
        assert_eq!(
            app.sequence_playback.status(),
            crate::paste_sequence::SequencePlaybackStatus::Completed
        );
        bridge.observer_live(false);
    }

    #[test]
    fn saved_paste_to_advance_run_repeats_three_times_through_app_and_physical_admission() {
        use crate::hotkey::launcher_invocation::{InputProvenance, KeyEvent, KeyTransition};
        use crate::paste_sequence::admission::{
            PasteAdmissionBridge, PhysicalPasteAdmission, PhysicalPasteNotice,
            PhysicalPasteReducer, physical_paste_bridge,
        };
        use crate::paste_sequence::{
            PlaybackMode, SequenceDraft, SequenceId, SequenceRoutePresentation,
            SequenceShortcutPresentation, SequenceStore, SequenceTarget, Step,
        };
        use std::time::Duration;

        struct ObserverReset(std::sync::Arc<PasteAdmissionBridge>);
        impl Drop for ObserverReset {
            fn drop(&mut self) {
                self.0.observer_live(false);
            }
        }

        fn physical_paste(
            app: &mut LauncherApp,
            reducer: &mut PhysicalPasteReducer,
            bridge: &std::sync::Arc<PasteAdmissionBridge>,
            platform: &mut AppPastePlatform,
            generation: u64,
        ) -> PhysicalPasteAdmission {
            let focus = crate::paste_sequence::admission::PasteFocus {
                hwnd: platform.target.hwnd(),
                process_id: platform.target.process_id(),
                thread_id: platform.target.thread_id(),
            };
            for (vk, transition, consumed) in [
                (0xA2, KeyTransition::Down, false),
                (0x56, KeyTransition::Down, true),
            ] {
                assert_eq!(
                    reducer.process(
                        KeyEvent {
                            vk,
                            transition,
                            provenance: InputProvenance::Physical,
                            at: 0,
                        },
                        Some(focus),
                        std::process::id(),
                        true,
                        bridge,
                    ),
                    consumed
                );
            }
            let PhysicalPasteNotice::Paste(admission) = bridge
                .take_notice()
                .expect("eligible physical Ctrl+V creates one admission")
            else {
                panic!("expected physical paste admission");
            };
            assert_eq!(admission.owner, app.event_sink.owner_id());
            assert_eq!(admission.generation, generation);
            assert!(bridge.valid(&admission));
            for (vk, transition, consumed) in [
                (0x56, KeyTransition::Up, true),
                (0xA2, KeyTransition::Up, false),
            ] {
                assert_eq!(
                    reducer.process(
                        KeyEvent {
                            vk,
                            transition,
                            provenance: InputProvenance::Physical,
                            at: 0,
                        },
                        Some(focus),
                        std::process::id(),
                        true,
                        bridge,
                    ),
                    consumed
                );
            }
            app.execute_physical_paste_using(
                admission.clone(),
                std::sync::Arc::clone(bridge),
                platform,
            )
            .unwrap();
            admission
        }

        let _guard = crate::gui::TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let store = SequenceStore::new(
            directory
                .path()
                .join(crate::paste_sequence::PASTE_SEQUENCES_FILE),
        );
        let mut saved = store.load().unwrap();
        let mut draft = SequenceDraft::new(
            "Repeatable saved run",
            vec![
                Step::LiteralText {
                    text: "first run step".into(),
                },
                Step::LiteralText {
                    text: "final run step".into(),
                },
            ],
        );
        draft.mode = PlaybackMode::PasteToAdvance;
        let id = saved.upsert(draft).unwrap();
        store.commit(&mut saved).unwrap();

        let bridge = physical_paste_bridge();
        bridge.observer_live(false);
        bridge.allow_test_routes();
        let (wake, _wake_rx) = std::sync::mpsc::channel();
        bridge.install_wake(wake);
        bridge.observer_live(true);
        let _observer_reset = ObserverReset(std::sync::Arc::clone(bridge));

        for keep_visible in [false, true] {
            let mut app = inert_test_app(directory.path());
            app.enabled_plugins = None;
            app.paste_sequence_dialog
                .use_test_data_sources(store.path().to_path_buf());
            app.sequence_routes = Some(SequenceRoutePresentation {
                generation: 1,
                physical_paste: Default::default(),
                next: SequenceShortcutPresentation::Disabled,
                cancel: SequenceShortcutPresentation::Disabled,
                starts: Vec::new(),
                escape_cancel_enabled: true,
                keep_dialog_visible_during_playback: keep_visible,
            });
            app.settings_editor
                .baseline_settings
                .paste_sequence
                .keep_dialog_visible_during_playback = !keep_visible;
            app.visible_flag.store(true, Ordering::SeqCst);
            let owner = app.event_sink.owner_id();
            let target = SequenceTarget::from_external_window(
                crate::active_window::ExternalForegroundWindow {
                    hwnd: 91,
                    process_id: 92,
                    thread_id: 93,
                    title: None,
                },
                std::process::id(),
            )
            .unwrap();
            let mut platform = AppPastePlatform::new(target);
            let mut reducer = PhysicalPasteReducer::new([false; 9]);
            let mut previous_generation = None;
            let invocation = CommandInvocation {
                command: Command::PasteSequence(crate::commands::PasteSequenceCommand::Run(id)),
                original_action: crate::actions::Action {
                    label: format!("Run saved sequence {}", id.get()),
                    desc: "Paste sequence".into(),
                    action: format!("paste:run:{}", id.get()),
                    args: None,
                },
                query_override: None,
                source: crate::commands::ActivationSource::Hotkey,
            };

            for run_index in 0..3 {
                app.dispatch_command_invocation(invocation.clone());

                let generation = app
                    .sequence_playback
                    .started_run()
                    .expect("saved Run starts a physical session")
                    .generation;
                assert_eq!(
                    app.sequence_playback.status(),
                    crate::paste_sequence::SequencePlaybackStatus::AwaitPaste,
                    "run {} starts ready for its first physical paste",
                    run_index + 1
                );
                assert_eq!(
                    app.sequence_playback.current_step_number(),
                    Some((1, 2)),
                    "every saved Run restarts at Step 1"
                );
                assert!(previous_generation.is_none_or(|previous| generation > previous));
                previous_generation = Some(generation);
                assert!(bridge.session_execution_valid(owner, generation));
                assert_eq!(app.paste_sequence_dialog.open, keep_visible);
                assert_eq!(app.panel_states.paste_sequence_dialog, keep_visible);
                assert_eq!(app.visible_flag.load(Ordering::SeqCst), keep_visible);
                assert_eq!(platform.publications, run_index * 2);

                let mut terminal_admission = None;
                for step in 0..2 {
                    let admission =
                        physical_paste(&mut app, &mut reducer, bridge, &mut platform, generation);
                    assert_eq!(platform.publications, run_index * 2 + step + 1);
                    assert_eq!(platform.pastes, run_index * 2 + step + 1);
                    assert_eq!(
                        platform.text,
                        if step == 0 {
                            "first run step"
                        } else {
                            "final run step"
                        }
                    );
                    terminal_admission = Some(admission);

                    platform.now += Duration::from_millis(180);
                    let now = platform.now;
                    app.sequence_playback.tick(&mut platform, now);
                    app.sync_physical_paste_admission().unwrap();
                    if step == 0 {
                        assert_eq!(
                            app.sequence_playback.status(),
                            crate::paste_sequence::SequencePlaybackStatus::AwaitPaste
                        );
                        assert_eq!(app.sequence_playback.current_step_number(), Some((2, 2)));
                    }
                }

                assert_eq!(
                    app.sequence_playback.status(),
                    crate::paste_sequence::SequencePlaybackStatus::Completed
                );
                assert!(app.sequence_playback.started_run().is_none());
                assert!(app.sequence_playback.current_step_number().is_none());
                assert!(!app.sequence_playback.has_input_lease());
                assert!(!app.sequence_playback.needs_tick());
                assert_eq!(platform.text, "final run step");
                let terminal_admission = terminal_admission.unwrap();
                assert!(!bridge.valid(&terminal_admission));
                assert!(!bridge.session_execution_valid(owner, generation));
                assert!(bridge.take_notice().is_none());
            }

            assert_eq!(platform.publications, 6);
            assert_eq!(platform.pastes, 6);
        }
    }

    #[test]
    fn physical_paste_start_conflict_refuses_before_clipboard_and_keeps_ordinary_routes() {
        use crate::paste_sequence::{admission::*, *};
        let _guard = crate::gui::TEST_MUTEX.lock().unwrap();
        let root = tempfile::tempdir().unwrap();
        let mut app = inert_test_app(root.path());
        app.enabled_plugins = None;
        let target = SequenceTarget::from_external_window(
            crate::active_window::ExternalForegroundWindow {
                hwnd: 91,
                process_id: std::process::id() + 1,
                thread_id: 93,
                title: None,
            },
            std::process::id(),
        )
        .unwrap();
        let run = SequencePreflight::begin(
            Sequence {
                id: SequenceId::from_raw(1).unwrap(),
                name: "Conflict fixture".into(),
                hotkey: None,
                mode: PlaybackMode::PasteToAdvance,
                delay_ms: 0,
                steps: vec![Step::LiteralText {
                    text: "owned dummy".into(),
                }],
            },
            &[],
            &[],
            SequenceInvocationContext::new(
                chrono::Local::now().fixed_offset(),
                None,
                target.clone(),
            ),
        )
        .unwrap()
        .finish(&[], &[])
        .unwrap();
        app.sequence_playback.begin_preparing().unwrap();
        app.sequence_playback.set_ready(run).unwrap();
        let mut platform = AppPastePlatform::new(target);
        let bridge = physical_paste_bridge();
        bridge.invalidate_route_policy();
        assert_eq!(
            app.start_sequence_playback_using(&mut platform),
            Err(SequencePlaybackError::RouteConflict)
        );
        assert_eq!(platform.publications, 0);
        assert!(bridge.acknowledge_route_policy(&PhysicalPastePolicy {
            revision: bridge.route_revision(),
            availability: PhysicalPasteAvailability::Conflict {
                owner: "Paste Sequence Cancel".into()
            }
        }));
        assert_eq!(
            app.start_sequence_playback_using(&mut platform),
            Err(SequencePlaybackError::RouteConflict)
        );
        assert_eq!(platform.publications, 0);
        assert_eq!(platform.pastes, 0);
        assert_eq!(platform.text, "original");
        assert_eq!(
            app.sequence_playback.status(),
            SequencePlaybackStatus::Ready
        );
        assert!(!app.sequence_playback.has_input_lease());
        assert!(app.sequence_playback.started_run().is_none());
        assert!(app.sequence_playback.current_step_number().is_none());
        assert!(
            bridge
                .route_availability()
                .reason()
                .unwrap()
                .contains("Cancel")
        );
    }

    #[test]
    fn paste_sequence_acceptance_prompted_three_steps_across_apps_and_fourth_normal_paste() {
        use crate::hotkey::launcher_invocation::{InputProvenance, KeyEvent, KeyTransition};
        use crate::paste_sequence::{admission::*, *};
        use crate::plugins::snippets::{SnippetEntry, SnippetFieldDefinition};
        use std::time::Duration;
        let _guard = crate::gui::TEST_MUTEX.lock().unwrap();
        let root = tempfile::tempdir().unwrap();
        let history_text = "history \u{96ea}\nsecond line";
        let temporary_text = "temporary \u{1f30c}\nfinal line";
        let store = SequenceStore::new(root.path().join(PASTE_SEQUENCES_FILE));
        let mut saved = store.load().unwrap();
        let mut draft = SequenceDraft::new(
            "Three mixed steps",
            vec![
                Step::SnippetRef {
                    alias: "private_reply".into(),
                },
                Step::clipboard_history_reference(history_text),
                Step::LiteralText {
                    text: temporary_text.into(),
                },
            ],
        );
        draft.mode = PlaybackMode::PasteToAdvance;
        let id = saved.upsert(draft).unwrap();
        store.commit(&mut saved).unwrap();
        let saved_bytes = std::fs::read(store.path()).unwrap();
        let sequence = store.load().unwrap().sequence(id).unwrap().clone();
        let mut snippets = vec![SnippetEntry {
            alias: "private_reply".into(),
            text: "Hello {{name}} at {{sys.time:%H:%M}}|{{sys.clipboard}}|{{sys.window_title}}"
                .into(),
            hide_contents: true,
            prompt_for_fields: true,
            fields: vec![SnippetFieldDefinition::new("name")],
            tags: Vec::new(),
        }];
        let mut history = vec![history_text.to_owned()];
        let origin = SequenceTarget::from_external_window(
            crate::active_window::ExternalForegroundWindow {
                hwnd: 91,
                process_id: std::process::id() + 1,
                thread_id: 93,
                title: Some("Frozen origin".into()),
            },
            std::process::id(),
        )
        .unwrap();
        let context = SequenceInvocationContext::new(
            chrono::DateTime::parse_from_rfc3339("2024-01-02T03:04:05+00:00").unwrap(),
            Some("invocation clipboard".into()),
            origin,
        );
        let mut app = inert_test_app_with_paste_shortcuts_disabled(root.path());
        app.enabled_plugins = None;
        app.sequence_routes = Some(SequenceRoutePresentation {
            generation: 7,
            physical_paste: Default::default(),
            next: SequenceShortcutPresentation::Disabled,
            cancel: SequenceShortcutPresentation::Active("Ctrl+F9".into()),
            starts: vec![],
            escape_cancel_enabled: true,
            keep_dialog_visible_during_playback: false,
        });
        app.begin_paste_sequence_preflight_using_context(
            sequence,
            &snippets,
            &history,
            crate::commands::ActivationSource::Hotkey,
            |_, _| Ok(context),
        )
        .unwrap();
        assert!(app.snippet_prompt_dialog.is_sequence_preflight());
        assert!(app.sequence_playback.started_run().is_none());
        app.snippet_prompt_dialog.set_value("name", "Ada".into());
        app.submit_sequence_prompt_using(|| Ok(snippets.clone()), || Ok(history.clone()))
            .unwrap();
        assert_eq!(
            app.sequence_playback.status(),
            SequencePlaybackStatus::Ready
        );
        let preview = app.sequence_playback.prepared_preview().unwrap();
        assert!(preview.steps[0].hidden && preview.steps[0].text.is_none());
        // Future source changes and foreground titles cannot alter this run.
        snippets[0].text = "changed after preflight".into();
        history.clear();
        let make_target = |hwnd, process_id, thread_id, title: &str| {
            SequenceTarget::from_external_window(
                crate::active_window::ExternalForegroundWindow {
                    hwnd,
                    process_id,
                    thread_id,
                    title: Some(title.into()),
                },
                std::process::id(),
            )
            .unwrap()
        };
        let target_a = make_target(101, std::process::id() + 2, 103, "Current A");
        let target_b = make_target(201, std::process::id() + 3, 203, "Current B");
        let mut platform = AppPastePlatform::new(target_a);
        let bridge = physical_paste_bridge();
        struct ObserverReset(std::sync::Arc<PasteAdmissionBridge>);
        impl Drop for ObserverReset {
            fn drop(&mut self) {
                self.0.observer_live(false);
            }
        }
        bridge.observer_live(false);
        let _reset = ObserverReset(std::sync::Arc::clone(bridge));
        let (wake, _receive) = std::sync::mpsc::channel();
        bridge.install_wake(wake);
        bridge.allow_test_routes();
        bridge.observer_live(true);
        app.start_sequence_playback_using(&mut platform).unwrap();
        assert_eq!(platform.pastes, 0);
        assert_eq!(
            platform.publications, 0,
            "Start never publishes Step 1 ahead of a press"
        );
        assert_eq!(platform.text, "original");
        assert!(app.sequence_playback.has_input_lease());
        assert!(app.sequence_playback.escape_cancel_available());
        let mut reducer = PhysicalPasteReducer::new([false; 9]);
        fn input(
            reducer: &mut PhysicalPasteReducer,
            bridge: &std::sync::Arc<PasteAdmissionBridge>,
            platform: &AppPastePlatform,
            vk: u32,
            transition: KeyTransition,
            provenance: InputProvenance,
        ) -> bool {
            reducer.process(
                KeyEvent {
                    vk,
                    transition,
                    provenance,
                    at: 0,
                },
                Some(PasteFocus {
                    hwnd: platform.target.hwnd(),
                    process_id: platform.target.process_id(),
                    thread_id: platform.target.thread_id(),
                }),
                std::process::id(),
                true,
                bridge,
            )
        }
        assert!(!input(
            &mut reducer,
            bridge,
            &platform,
            0xA2,
            KeyTransition::Down,
            InputProvenance::Physical
        ));
        let expected = [
            "Hello Ada at 03:04|invocation clipboard|Frozen origin",
            history_text,
            temporary_text,
        ];
        let total_steps = expected.len();
        for (index, expected) in expected.iter().enumerate() {
            if index == 1 {
                platform.target = target_b.clone();
            }
            let before = platform.publications;
            for transition in [
                KeyTransition::Down,
                KeyTransition::Down,
                KeyTransition::Repeat,
            ] {
                assert!(input(
                    &mut reducer,
                    bridge,
                    &platform,
                    0x56,
                    transition,
                    InputProvenance::Physical
                ));
            }
            let PhysicalPasteNotice::Paste(admission) = bridge.take_notice().unwrap() else {
                panic!("expected one admitted paste");
            };
            assert!(bridge.take_notice().is_none(), "held V produces one permit");
            assert!(input(
                &mut reducer,
                bridge,
                &platform,
                0x56,
                KeyTransition::Up,
                InputProvenance::Physical
            ));
            app.execute_physical_paste_using(
                admission.clone(),
                std::sync::Arc::clone(bridge),
                &mut platform,
            )
            .unwrap();
            assert_eq!(platform.publications, before + 1);
            assert_eq!(platform.text, *expected);
            assert_eq!(platform.pastes, index + 1);
            assert_eq!(
                app.sequence_playback.current_step_number(),
                Some((index + 1, total_steps))
            );
            let metadata = format!(
                "{:?}",
                (
                    app.sequence_playback.status(),
                    app.sequence_playback.started_run(),
                    app.sequence_playback.current_step_number(),
                )
            );
            assert!(
                !metadata.contains("Ada")
                    && !metadata.contains("invocation clipboard")
                    && !metadata.contains("Frozen origin")
            );
            for provenance in [
                InputProvenance::SelfInjected,
                InputProvenance::ExternalInjected,
            ] {
                for (vk, transition) in [
                    (0xA2, KeyTransition::Down),
                    (0x56, KeyTransition::Down),
                    (0x56, KeyTransition::Up),
                    (0xA2, KeyTransition::Up),
                ] {
                    assert!(!input(
                        &mut reducer,
                        bridge,
                        &platform,
                        vk,
                        transition,
                        provenance
                    ));
                }
            }
            assert!(
                bridge.take_notice().is_none(),
                "injected replay cannot reenter playback"
            );
            // External modifier injection invalidates the physical baseline.
            // A real release and fresh press reestablish it before retrying.
            for transition in [KeyTransition::Up, KeyTransition::Down] {
                assert!(!input(
                    &mut reducer,
                    bridge,
                    &platform,
                    0xA2,
                    transition,
                    InputProvenance::Physical
                ));
            }
            for transition in [KeyTransition::Down, KeyTransition::Up] {
                assert!(input(
                    &mut reducer,
                    bridge,
                    &platform,
                    0x56,
                    transition,
                    InputProvenance::Physical
                ));
            }
            let busy = bridge.take_notice().unwrap();
            assert!(matches!(busy, PhysicalPasteNotice::Busy { .. }));
            app.handle_physical_paste_notice(busy);
            let dispatched = platform.now;
            platform.now = dispatched + Duration::from_millis(179);
            let now = platform.now;
            app.sequence_playback.tick(&mut platform, now);
            assert_eq!(
                platform.publications,
                before + 1,
                "later text is not published during the current step settle window"
            );
            assert_eq!(platform.pastes, index + 1);
            assert!(bridge.valid(&admission));
            platform.now = dispatched + Duration::from_millis(180);
            let now = platform.now;
            app.sequence_playback.tick(&mut platform, now);
            app.sync_physical_paste_admission().unwrap();
            let expected_next_step = if index + 1 < total_steps {
                Some((index + 2, total_steps))
            } else {
                None
            };
            assert_eq!(
                app.sequence_playback.current_step_number(),
                expected_next_step
            );
            assert!(!bridge.valid(&admission));
        }
        assert_eq!(
            app.sequence_playback.status(),
            SequencePlaybackStatus::Completed
        );
        assert!(app.sequence_playback.started_run().is_none());
        assert!(!app.sequence_playback.has_input_lease());
        assert!(!app.sequence_playback.escape_cancel_available());
        assert!(app.sequence_playback.current_step_number().is_none());
        assert!(bridge.take_notice().is_none());
        assert!(!app.sequence_playback.needs_tick());
        assert_eq!(platform.publications, 3);
        for transition in [KeyTransition::Down, KeyTransition::Up] {
            assert!(!input(
                &mut reducer,
                bridge,
                &platform,
                0x56,
                transition,
                InputProvenance::Physical
            ));
        }
        assert!(bridge.take_notice().is_none());
        // The unclaimed fourth human chord follows the fake ordinary OS path,
        // consuming the unchanged final clipboard instead of a sequence step.
        SequencePlaybackPlatform::paste_chord(&mut platform).unwrap();
        assert_eq!(
            platform.pasted,
            vec![
                (101, expected[0].into()),
                (201, history_text.into()),
                (201, temporary_text.into()),
                (201, temporary_text.into())
            ]
        );
        assert_eq!(platform.publications, 3);
        assert_eq!(platform.text, temporary_text);
        assert!(!input(
            &mut reducer,
            bridge,
            &platform,
            0xA2,
            KeyTransition::Up,
            InputProvenance::Physical
        ));
        assert_eq!(std::fs::read(store.path()).unwrap(), saved_bytes);
        assert!(!root.path().join("settings.json").exists());
        assert!(!root.path().join("actions.json").exists());
    }

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
    fn saved_run_prompt_continuation_opens_closed_grid_route_and_keeps_root_visible() {
        use crate::paste_sequence::{
            PlaybackMode, Sequence, SequenceId, SequenceInvocationContext,
            SequenceRoutePresentation, SequenceShortcutPresentation, SequenceTarget, Step,
            admission::physical_paste_bridge,
        };
        use std::sync::atomic::Ordering;

        let _guard = crate::gui::TEST_MUTEX.lock().unwrap();
        let bridge = physical_paste_bridge();
        struct ObserverReset(
            std::sync::Arc<crate::paste_sequence::admission::PasteAdmissionBridge>,
        );
        impl Drop for ObserverReset {
            fn drop(&mut self) {
                self.0.observer_live(false);
            }
        }
        bridge.observer_live(false);
        let _reset = ObserverReset(std::sync::Arc::clone(bridge));
        let (wake, _receive) = std::sync::mpsc::channel();
        bridge.install_wake(wake);
        bridge.allow_test_routes();
        bridge.observer_live(true);

        let baseline_directory = tempfile::tempdir().unwrap();
        let mut baseline_app = inert_test_app(baseline_directory.path());
        baseline_app
            .settings_editor
            .baseline_settings
            .paste_sequence
            .keep_dialog_visible_during_playback = true;
        assert!(baseline_app.should_keep_paste_sequence_dialog_visible_during_playback());
        assert!(!baseline_app.paste_sequence_dialog.open);
        assert!(!baseline_app.panel_states.paste_sequence_dialog);

        for editor_open in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let mut app = inert_test_app(directory.path());
            app.enabled_plugins = None;
            app.paste_sequence_dialog.use_test_data_sources(
                directory
                    .path()
                    .join(crate::paste_sequence::PASTE_SEQUENCES_FILE),
            );
            app.visible_flag.store(editor_open, Ordering::SeqCst);
            app.settings_editor
                .paste_sequence
                .keep_dialog_visible_during_playback = false;
            app.sequence_routes = Some(SequenceRoutePresentation {
                generation: 3,
                physical_paste: Default::default(),
                next: SequenceShortcutPresentation::Disabled,
                cancel: SequenceShortcutPresentation::Disabled,
                starts: Vec::new(),
                escape_cancel_enabled: false,
                keep_dialog_visible_during_playback: true,
            });
            if editor_open {
                app.panel_states.paste_sequence_dialog = true;
                app.panel_stack
                    .push(super::super::Panel::PasteSequenceDialog);
                app.paste_sequence_dialog.open = true;
            }
            app.paste_sequence_dialog.set_autostart_when_ready(true);

            let sequence = Sequence {
                id: SequenceId::from_raw(1).unwrap(),
                name: "Prompted visibility".into(),
                hotkey: None,
                mode: PlaybackMode::PasteToAdvance,
                delay_ms: 0,
                steps: vec![Step::SnippetRef {
                    alias: "ticketreply".into(),
                }],
            };
            let snippets = vec![prompted_entry()];
            let target = SequenceTarget::from_external_window(
                crate::active_window::ExternalForegroundWindow {
                    hwnd: 91,
                    process_id: std::process::id() + 1,
                    thread_id: 93,
                    title: Some("External target".into()),
                },
                std::process::id(),
            )
            .unwrap();
            let context = SequenceInvocationContext::new(
                chrono::DateTime::parse_from_rfc3339("2024-01-02T03:04:05+00:00").unwrap(),
                None,
                Some(target),
            );
            assert!(app.should_keep_paste_sequence_dialog_visible_during_playback());
            let presentation_sequence = sequence.clone();
            app.begin_paste_sequence_preflight_using_context_after_capture(
                sequence,
                &snippets,
                &[],
                crate::commands::ActivationSource::Click,
                |_, _| Ok(context.clone()),
                move |app, context| {
                    app.present_saved_paste_sequence_run(&presentation_sequence, context.clone());
                },
            )
            .unwrap();
            app.paste_sequence_dialog.set_autostart_when_ready(true);
            assert!(app.visible_flag.load(Ordering::SeqCst));
            assert!(app.paste_sequence_dialog.open);
            assert!(app.panel_states.paste_sequence_dialog);
            assert!(
                app.panel_stack
                    .contains(&super::super::Panel::PasteSequenceDialog)
            );
            assert!(app.snippet_prompt_dialog.is_sequence_preflight());
            app.snippet_prompt_dialog.set_value("name", "Ada".into());
            app.submit_sequence_prompt_using(|| Ok(snippets.clone()), || Ok(Vec::new()))
                .unwrap();

            assert_eq!(
                app.sequence_playback.status(),
                crate::paste_sequence::SequencePlaybackStatus::AwaitPaste
            );
            assert!(app.visible_flag.load(Ordering::SeqCst));
            assert!(app.panel_states.paste_sequence_dialog);
            assert!(app.paste_sequence_dialog.open);
            assert!(
                app.panel_stack
                    .contains(&super::super::Panel::PasteSequenceDialog)
            );
            let invocation = crate::commands::CommandInvocation {
                command: crate::commands::Command::PasteSequence(
                    crate::commands::PasteSequenceCommand::Run(SequenceId::from_raw(1).unwrap()),
                ),
                original_action: crate::actions::Action {
                    label: "Prompted visibility".into(),
                    desc: "Paste sequence".into(),
                    action: "paste:run:1".into(),
                    args: None,
                },
                query_override: None,
                source: crate::commands::ActivationSource::Click,
            };
            app.apply_command_outcome(crate::commands::CommandOutcome::default(), &invocation);
            assert!(app.visible_flag.load(Ordering::SeqCst));
            app.cancel_sequence_playback();
        }
    }

    #[test]
    fn paste_sequence_modes_saved_prompt_then_configured_next_uses_current_app() {
        let _guard = crate::gui::TEST_MUTEX.lock().unwrap();
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
        let settings_bytes = playback_settings_fixture(&app);
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

        let saved_bytes = std::fs::read(store.path()).unwrap();
        let target_a = crate::paste_sequence::SequenceTarget::from_external_window(
            external_window(41, "App A"),
            std::process::id(),
        )
        .unwrap();
        let target_b = crate::paste_sequence::SequenceTarget::from_external_window(
            crate::active_window::ExternalForegroundWindow {
                process_id: 52,
                thread_id: 53,
                ..external_window(51, "App B")
            },
            std::process::id(),
        )
        .unwrap();
        let mut platform = AppPastePlatform::new(target_a);
        let started = platform.now;
        app.start_sequence_playback_using(&mut platform).unwrap();
        assert_eq!(platform.pasted, [(41, "Hello Ada at 2024-01-02".into())]);
        app.open_paste_sequence_settings_dialog();
        assert!(app.show_settings);
        assert_eq!(
            app.settings_editor
                .paste_sequence_preferences()
                .next_hotkey
                .as_deref(),
            Some("Ctrl+Alt+Shift+PageDown")
        );
        assert_eq!(app.sequence_playback.current_step_number(), Some((2, 4)));
        assert_eq!(
            app.sequence_playback.status(),
            crate::paste_sequence::SequencePlaybackStatus::AwaitAdvance
        );

        // Actual routed Next dispatch waits while the configured chord's
        // Ctrl+Alt+Shift modifiers remain held, then uses the user's new app.
        platform.now = started + std::time::Duration::from_millis(275);
        platform.input_eligible = false;
        let now = platform.now;
        app.handle_paste_sequence_hotkey_using(
            crate::hotkey::launcher_invocation::PasteSequenceHotkeyAction::Next,
            |app| app.next_sequence_playback_using(&mut platform, now),
        );
        assert_eq!(
            app.sequence_playback.status(),
            crate::paste_sequence::SequencePlaybackStatus::AwaitTarget
        );
        assert_eq!(platform.pastes, 1);
        assert_eq!(app.sequence_playback.current_step_number(), Some((2, 4)));
        platform.target = target_b;
        platform.input_eligible = true;
        platform.now += std::time::Duration::from_millis(100);
        let now = platform.now;
        app.sequence_playback.tick(&mut platform, now);
        assert_eq!(
            platform.pasted[1],
            (
                51,
                "03:04|captured clipboard|Origin editor \u{1f58b}".into()
            )
        );
        assert_eq!(
            app.sequence_playback.status(),
            crate::paste_sequence::SequencePlaybackStatus::AwaitAdvance
        );
        for expected in [history_text, "One-off \u{1f30c}\nsecond line"] {
            platform.now += std::time::Duration::from_millis(275);
            let now = platform.now;
            app.handle_paste_sequence_hotkey_using(
                crate::hotkey::launcher_invocation::PasteSequenceHotkeyAction::Next,
                |app| app.next_sequence_playback_using(&mut platform, now),
            );
            assert_eq!(platform.pasted.last().unwrap(), &(51, expected.into()));
        }
        assert_eq!(platform.pastes, 4);
        assert_eq!(
            app.sequence_playback.status(),
            crate::paste_sequence::SequencePlaybackStatus::Completed
        );
        platform.now += std::time::Duration::from_millis(180);
        let now = platform.now;
        app.sequence_playback.tick(&mut platform, now);
        assert_eq!(platform.text, "One-off \u{1f30c}\nsecond line");
        assert_eq!(std::fs::read(store.path()).unwrap(), saved_bytes);
        assert_eq!(std::fs::read(&app.settings_path).unwrap(), settings_bytes);
    }

    #[test]
    fn paste_sequence_modes_run_once_auto_wait_cancel_cleanup_and_restart_preserve_saved_bytes() {
        use crate::paste_sequence::{
            PlaybackMode, SequenceDraft, SequenceInvocationContext, SequencePlaybackError,
            SequencePlaybackStatus as Status, SequencePreflightError, SequenceStore,
            SequenceTarget, Step,
        };
        let _guard = crate::gui::TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let store = SequenceStore::new(
            directory
                .path()
                .join(crate::paste_sequence::PASTE_SEQUENCES_FILE),
        );
        let mut saved = store.load().unwrap();
        saved
            .upsert(SequenceDraft::new(
                "Saved remains unchanged",
                vec![Step::LiteralText {
                    text: "saved".into(),
                }],
            ))
            .unwrap();
        store.commit(&mut saved).unwrap();
        let saved_bytes = std::fs::read(store.path()).unwrap();
        let context = SequenceInvocationContext::new(
            chrono::DateTime::parse_from_rfc3339("2024-01-02T03:04:05+00:00").unwrap(),
            None,
            None,
        );
        let mut draft = SequenceDraft::new(
            "Memory-only",
            ["one", "two", "never"]
                .into_iter()
                .map(|text| Step::LiteralText { text: text.into() })
                .collect(),
        );
        draft.mode = PlaybackMode::Automatic;
        draft.delay_ms = 250;
        let mut app = inert_test_app(directory.path());
        let settings_bytes = playback_settings_fixture(&app);
        app.begin_paste_sequence_draft_preflight_with_context(
            draft.clone(),
            &[],
            &[],
            context.clone(),
        )
        .unwrap();
        assert_eq!(app.sequence_playback.status(), Status::Ready);
        assert_eq!(std::fs::read(store.path()).unwrap(), saved_bytes);
        let mut platform = AppPastePlatform::new(
            SequenceTarget::from_external_window(external_window(61, "A"), std::process::id())
                .unwrap(),
        );
        let started = platform.now;
        app.start_sequence_playback_using(&mut platform).unwrap();
        assert_eq!(platform.pasted, [(61, "one".into())]);
        app.open_paste_sequence_settings_dialog();
        assert!(app.show_settings);
        assert_eq!(app.sequence_playback.status(), Status::Running);
        platform.now = started + std::time::Duration::from_millis(249);
        let now = platform.now;
        app.sequence_playback.tick(&mut platform, now);
        assert_eq!(platform.pastes, 1);
        // Showing/focusing the launcher at the timer boundary cannot receive
        // the next payload or consume its index.
        platform.target_error = Some(SequencePlaybackError::FocusChanged);
        platform.now += std::time::Duration::from_millis(1);
        let now = platform.now;
        app.sequence_playback.tick(&mut platform, now);
        assert_eq!(app.sequence_playback.status(), Status::AwaitTarget);
        assert_eq!(app.sequence_playback.current_step_number(), Some((2, 3)));
        assert_eq!(platform.text, "one");
        platform.target = SequenceTarget::from_external_window(
            crate::active_window::ExternalForegroundWindow {
                process_id: 72,
                thread_id: 73,
                ..external_window(71, "B")
            },
            std::process::id(),
        )
        .unwrap();
        platform.target_error = None;
        platform.now += std::time::Duration::from_millis(100);
        let now = platform.now;
        app.sequence_playback.tick(&mut platform, now);
        assert_eq!(platform.pasted, [(61, "one".into()), (71, "two".into())]);
        app.handle_paste_sequence_hotkey(
            crate::hotkey::launcher_invocation::PasteSequenceHotkeyAction::Cancel,
        );
        assert_eq!(app.sequence_playback.status(), Status::Cancelled);
        assert_eq!(
            app.begin_paste_sequence_draft_preflight_with_context(draft, &[], &[], context.clone()),
            Err(SequencePreflightError::SequenceAlreadyActive)
        );
        platform.now += std::time::Duration::from_millis(180);
        let now = platform.now;
        app.sequence_playback.tick(&mut platform, now);
        assert_eq!(platform.pastes, 2);
        assert_eq!(platform.text, "two");
        let mut restart = SequenceDraft::new(
            "Restart",
            vec![Step::LiteralText {
                text: "restart".into(),
            }],
        );
        restart.mode = PlaybackMode::ManualAdvance;
        app.begin_paste_sequence_draft_preflight_with_context(
            restart.clone(),
            &[],
            &[],
            context.clone(),
        )
        .unwrap();
        app.start_sequence_playback_using(&mut platform).unwrap();
        assert_eq!(platform.pasted.last().unwrap(), &(71, "restart".into()));
        assert_eq!(app.sequence_playback.status(), Status::Completed);
        platform.now += std::time::Duration::from_millis(180);
        let now = platform.now;
        app.sequence_playback.tick(&mut platform, now);
        for refusal in [
            SequencePlaybackError::InvalidTarget,
            SequencePlaybackError::PasswordField,
        ] {
            platform.target_error = Some(refusal);
            app.begin_paste_sequence_draft_preflight_with_context(
                restart.clone(),
                &[],
                &[],
                context.clone(),
            )
            .unwrap();
            assert_eq!(
                app.start_sequence_playback_using(&mut platform),
                Err(refusal)
            );
            assert_eq!(app.sequence_playback.status(), Status::Failed(refusal));
            assert_eq!(platform.pastes, 3);
            assert_eq!(platform.text, "restart");
        }
        assert_eq!(std::fs::read(store.path()).unwrap(), saved_bytes);
        assert_eq!(std::fs::read(&app.settings_path).unwrap(), settings_bytes);
        assert!(!directory.path().join("actions.json").exists());
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

    pub(in crate::gui) fn inert_test_app(root: &std::path::Path) -> LauncherApp {
        inert_test_app_with_settings(root, crate::settings::Settings::default())
    }

    fn inert_test_app_with_paste_shortcuts_disabled(root: &std::path::Path) -> LauncherApp {
        let mut settings = crate::settings::Settings::default();
        settings.paste_sequence.next_hotkey = None;
        settings.paste_sequence.cancel_hotkey = None;
        inert_test_app_with_settings(root, settings)
    }

    fn inert_test_app_with_settings(
        root: &std::path::Path,
        mut settings: crate::settings::Settings,
    ) -> LauncherApp {
        let ctx = eframe::egui::Context::default();
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
    fn sequence_template_origin_uses_fresh_foreground_or_launcher_ui_cache_only() {
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
    fn settings_search_entry_reaches_gui_host_losslessly_without_replacing_draft() {
        use crate::plugin::Plugin;

        let _guard = crate::gui::TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let mut app = inert_test_app_with_paste_shortcuts_disabled(directory.path());
        app.show_settings = true;

        let baseline_hotkey = app.settings_editor.baseline_settings.hotkey.clone();
        app.settings_editor.hotkey = "Ctrl+Shift+F10".into();
        app.settings_editor.settings_search_match_query = "prior query".into();
        app.settings_editor.settings_search_match_revision = Some(42);
        app.settings_editor.settings_search_match_ids = vec!["launcher.always_on_top".into()];
        app.settings_editor.settings_search_selected_id = Some("launcher.always_on_top".into());
        app.settings_editor.settings_search_page = 3;
        app.settings_editor.settings_search_response_owner_instance = Some(7);
        app.settings_editor.settings_search_query_response_id =
            Some(eframe::egui::Id::new("stale-settings-query"));
        app.settings_editor.settings_search_result_response_ids =
            vec![eframe::egui::Id::new("stale-settings-result")];
        app.settings_editor.request_control_navigation(
            "launcher.always_on_top",
            None,
            &app.plugins,
        );
        assert!(app.settings_editor.pending_settings_navigation.is_some());

        let expected_query = "  font: \"Δelta\"\tlauncher  ";
        let plugin_query = "  Settings   font: \"Δelta\"\tlauncher  ";
        let search_action = crate::plugins::settings::SettingsPlugin
            .search(plugin_query)
            .into_iter()
            .find(|action| action.action == "settings:search")
            .expect("Settings plugin exposes a typed search action");
        assert_eq!(search_action.args.as_deref(), Some(expected_query));

        let invocation = crate::commands::parse_command(
            search_action,
            None,
            crate::commands::ActivationSource::Enter,
        )
        .unwrap();
        assert_eq!(
            invocation.command,
            crate::commands::Command::Dialog(crate::commands::DialogCommand::SettingsSearch {
                query: expected_query.to_owned()
            })
        );
        let outcome = crate::commands::CommandBus
            .dispatch(&invocation, &mut app)
            .unwrap();

        assert!(app.show_settings);
        assert_eq!(app.settings_editor.settings_search_query, expected_query);
        assert!(app.settings_editor.settings_search_match_query.is_empty());
        assert_eq!(app.settings_editor.settings_search_match_revision, None);
        assert!(app.settings_editor.settings_search_match_ids.is_empty());
        assert_eq!(app.settings_editor.settings_search_selected_id, None);
        assert_eq!(app.settings_editor.settings_search_page, 0);
        assert_eq!(app.settings_editor.pending_settings_navigation, None);
        assert_eq!(
            app.settings_editor.settings_search_response_owner_instance,
            None
        );
        assert_eq!(app.settings_editor.settings_search_query_response_id, None);
        assert!(
            app.settings_editor
                .settings_search_result_response_ids
                .is_empty()
        );
        assert_eq!(app.settings_editor.hotkey, "Ctrl+Shift+F10");
        assert_eq!(
            app.settings_editor.baseline_settings.hotkey,
            baseline_hotkey
        );
        assert!(!directory.path().join("settings.json").exists());
        assert_eq!(outcome.query, crate::commands::QueryPolicy::Keep);

        let empty_invocation = crate::commands::parse_command(
            crate::actions::Action {
                label: "Search Settings".into(),
                desc: String::new(),
                action: "settings:search".into(),
                args: Some(" \t".into()),
            },
            None,
            crate::commands::ActivationSource::Enter,
        )
        .unwrap();
        assert_eq!(
            empty_invocation.command,
            crate::commands::Command::Dialog(crate::commands::DialogCommand::SettingsSearch {
                query: " \t".into()
            })
        );
        let empty_outcome = crate::commands::CommandBus
            .dispatch(&empty_invocation, &mut app)
            .unwrap();
        assert!(app.settings_editor.settings_search_query.is_empty());
        assert!(app.settings_editor.settings_search_match_query.is_empty());
        assert_eq!(app.settings_editor.settings_search_match_revision, None);
        assert!(app.settings_editor.settings_search_match_ids.is_empty());
        assert_eq!(app.settings_editor.settings_search_selected_id, None);
        assert_eq!(app.settings_editor.settings_search_page, 0);
        assert_eq!(app.settings_editor.pending_settings_navigation, None);
        assert_eq!(
            app.settings_editor.settings_search_response_owner_instance,
            None
        );
        assert_eq!(app.settings_editor.settings_search_query_response_id, None);
        assert!(
            app.settings_editor
                .settings_search_result_response_ids
                .is_empty()
        );
        assert_eq!(app.settings_editor.hotkey, "Ctrl+Shift+F10");
        assert_eq!(
            app.settings_editor.baseline_settings.hotkey,
            baseline_hotkey
        );
        assert!(!directory.path().join("settings.json").exists());
        assert_eq!(empty_outcome.query, crate::commands::QueryPolicy::Keep);

        let repeated_query = crate::commands::CommandBus
            .dispatch(&invocation, &mut app)
            .unwrap();
        assert_eq!(app.settings_editor.settings_search_query, expected_query);
        assert_eq!(repeated_query.query, crate::commands::QueryPolicy::Keep);
        app.settings_editor.settings_search_match_query = "stale result query".into();
        app.settings_editor.settings_search_match_revision = Some(43);
        app.settings_editor.settings_search_match_ids = vec!["launcher.always_on_top".into()];
        app.settings_editor.settings_search_selected_id = Some("launcher.always_on_top".into());
        app.settings_editor.settings_search_page = 2;
        app.settings_editor.settings_search_response_owner_instance = Some(8);
        app.settings_editor.settings_search_query_response_id =
            Some(eframe::egui::Id::new("stale-settings-query-again"));
        app.settings_editor.settings_search_result_response_ids =
            vec![eframe::egui::Id::new("stale-settings-result-again")];
        app.settings_editor.search_enter_consumption_pending = true;
        app.settings_editor.request_control_navigation(
            "launcher.always_on_top",
            None,
            &app.plugins,
        );
        assert!(app.settings_editor.pending_settings_navigation.is_some());
        let legacy_settings = crate::commands::parse_command(
            crate::actions::Action {
                label: "Settings".into(),
                desc: String::new(),
                action: "settings:dialog".into(),
                args: None,
            },
            None,
            crate::commands::ActivationSource::Enter,
        )
        .unwrap();
        let legacy_outcome = crate::commands::CommandBus
            .dispatch(&legacy_settings, &mut app)
            .unwrap();
        assert!(app.settings_editor.settings_search_query.is_empty());
        assert!(app.settings_editor.settings_search_match_query.is_empty());
        assert_eq!(app.settings_editor.settings_search_match_revision, None);
        assert!(app.settings_editor.settings_search_match_ids.is_empty());
        assert_eq!(app.settings_editor.settings_search_selected_id, None);
        assert_eq!(app.settings_editor.settings_search_page, 0);
        assert_eq!(app.settings_editor.pending_settings_navigation, None);
        assert!(!app.settings_editor.search_enter_consumption_pending);
        assert_eq!(
            app.settings_editor.settings_search_response_owner_instance,
            None
        );
        assert_eq!(app.settings_editor.settings_search_query_response_id, None);
        assert!(
            app.settings_editor
                .settings_search_result_response_ids
                .is_empty()
        );
        assert_eq!(app.settings_editor.hotkey, "Ctrl+Shift+F10");
        assert_eq!(
            app.settings_editor.baseline_settings.hotkey,
            baseline_hotkey
        );
        assert!(!directory.path().join("settings.json").exists());
        assert_eq!(legacy_outcome.query, crate::commands::QueryPolicy::Keep);
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
