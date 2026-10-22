use crate::paste_sequence::{
    HotkeyChord, PASTE_SEQUENCES_FILE, PlaybackMode, PreparedSequencePreview, SequenceDraft,
    SequenceHotkeyBinding, SequenceId, SequenceInvocationContext, SequenceSnapshot, SequenceStore,
    SequenceStoreError, Step,
};
use crate::plugins::snippets::SnippetEntry;
use eframe::egui;
use std::path::PathBuf;

const SNIPPETS_FILE: &str = crate::plugins::snippets::SNIPPETS_FILE;
const CLIPBOARD_FILE: &str = crate::plugins::clipboard::CLIPBOARD_FILE;

fn playback_mode_label(mode: PlaybackMode) -> &'static str {
    match mode {
        PlaybackMode::Automatic => "Automatic",
        PlaybackMode::ManualAdvance => "Manual advance",
        PlaybackMode::PasteToAdvance => "Paste-to-Advance (Ctrl+V)",
    }
}

fn saved_run_status_text(saved_run: Option<&SavedRunPresentation>, status: String) -> String {
    saved_run.map_or(status.clone(), |run| {
        format!(
            "Saved sequence ‘{}’ (ID {}) · {status}",
            run.name,
            run.id.get()
        )
    })
}

fn load_sequence_snapshot_and_notify(
    store: &SequenceStore,
    notifier: &super::PasteSequenceDocumentRevisionNotifier,
) -> Option<SequenceSnapshot> {
    let snapshot = store.load().ok()?;
    notifier.note_revision(snapshot.revision());
    Some(snapshot)
}

fn run_status_text(
    status: crate::paste_sequence::SequencePlaybackStatus,
    progress: Option<(usize, usize)>,
) -> String {
    use crate::paste_sequence::{SequencePlaybackError, SequencePlaybackStatus as Status};
    let label = match status {
        Status::Idle => "Idle".to_owned(),
        Status::Preparing => "Preparing steps and prompts".into(),
        Status::Ready => "Ready; no text has been pasted".into(),
        Status::Running => "Running".into(),
        Status::AwaitAdvance => "Waiting for Next".into(),
        Status::AwaitTarget => {
            "Release shortcut keys; focus an eligible application (30 second limit)".into()
        }
        Status::AwaitPaste => "Waiting for Ctrl+V in an eligible application".into(),
        Status::Cancelled => "Cancelled".into(),
        Status::Completed => "Completed".into(),
        Status::Failed(error) => format!("Failed: {}", error.message()),
    };
    if matches!(
        status,
        Status::Running | Status::AwaitAdvance | Status::AwaitTarget | Status::AwaitPaste
    ) && let Some((current, total)) = progress
    {
        format!("{label} · step {current} of {total}")
    } else {
        format!("Run status: {label}")
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug)]
struct DialogLayout {
    window: egui::Rect,
    body: egui::Rect,
    body_content: egui::Vec2,
    body_id: Option<egui::Id>,
    body_offset: f32,
    steps: egui::Rect,
    steps_content: egui::Vec2,
    steps_id: Option<egui::Id>,
    steps_offset: f32,
    footer: egui::Rect,
}

#[cfg(test)]
impl Default for DialogLayout {
    fn default() -> Self {
        Self {
            window: egui::Rect::NOTHING,
            body: egui::Rect::NOTHING,
            body_content: egui::Vec2::ZERO,
            body_id: None,
            body_offset: 0.0,
            steps: egui::Rect::NOTHING,
            steps_content: egui::Vec2::ZERO,
            steps_id: None,
            steps_offset: 0.0,
            footer: egui::Rect::NOTHING,
        }
    }
}

#[derive(Clone)]
enum SequenceEditorUpdate {
    Saved { id: SequenceId },
    Removed { id: SequenceId },
}

struct HotkeyUpdateCompletion {
    result: Result<SequenceCommitOutcome, String>,
    snapshot: Option<SequenceSnapshot>,
    editor_update: SequenceEditorUpdate,
}

#[derive(Debug)]
enum SequenceCommitOutcome {
    ContentOnly,
    BindingApplied {
        plugin_enabled: bool,
    },
    ContentSavedBindingRejected {
        message: String,
        routes_uncertain: bool,
    },
}

fn binding_update_status(update: &SequenceEditorUpdate, plugin_enabled: bool) -> &'static str {
    match (update, plugin_enabled) {
        (SequenceEditorUpdate::Saved { .. }, true) => {
            "Sequence saved; start shortcut routes are active. Global playback settings were unchanged."
        }
        (SequenceEditorUpdate::Saved { .. }, false) => {
            "Sequence saved; shortcuts are inactive while the Paste Sequences plugin is disabled."
        }
        (SequenceEditorUpdate::Removed { .. }, true) => {
            "Sequence removed; its start shortcut was removed. Remaining routes are active."
        }
        (SequenceEditorUpdate::Removed { .. }, false) => {
            "Sequence removed; its start shortcut was removed. Shortcuts remain inactive while the Paste Sequences plugin is disabled."
        }
    }
}

enum StartShortcutUpdate {
    Keep,
    Set(Option<HotkeyChord>),
    Invalid,
}

#[derive(Debug)]
struct RouteUpdateFailure {
    message: String,
    uncertain: bool,
}

enum RouteActivationFailure {
    Confirmed(String),
    Uncertain(String),
}

struct SavedRunPresentation {
    id: SequenceId,
    name: String,
}

pub(crate) struct PasteSequenceDialog {
    suspended_for_playback: bool,
    pub(crate) open: bool,
    #[cfg(test)]
    layout: DialogLayout,
    #[cfg(test)]
    sources_are_test_owned: bool,
    store_path: PathBuf,
    snapshot: Option<SequenceSnapshot>,
    draft: SequenceDraft,
    selected_id: Option<SequenceId>,
    selected_step: Option<usize>,
    reveal_step: Option<usize>,
    remove_confirmation: Option<SequenceId>,
    snippets: Vec<SnippetEntry>,
    history: Vec<String>,
    context: Option<SequenceInvocationContext>,
    hotkey_text: String,
    inline_error: Option<String>,
    status: Option<String>,
    snippets_error: Option<String>,
    history_error: Option<String>,
    autostart_when_ready: bool,
    hotkey_update: Option<std::sync::mpsc::Receiver<HotkeyUpdateCompletion>>,
    saved_run: Option<SavedRunPresentation>,
}

#[cfg(feature = "qa-framebuffer-capture")]
pub(crate) struct PasteSequenceDialogQaObservation {
    pub(crate) open: bool,
    pub(crate) invocation_context_present: bool,
    pub(crate) invocation_context_has_clipboard_text: bool,
    pub(crate) saved_sequence_ids: Vec<u64>,
    pub(crate) selected_sequence_id: Option<u64>,
    pub(crate) draft_step_count: usize,
    pub(crate) remove_confirmation_open: bool,
}

impl Default for PasteSequenceDialog {
    fn default() -> Self {
        Self {
            suspended_for_playback: false,
            open: false,
            #[cfg(test)]
            layout: DialogLayout::default(),
            #[cfg(test)]
            sources_are_test_owned: false,
            store_path: PathBuf::from(PASTE_SEQUENCES_FILE),
            snapshot: None,
            draft: SequenceDraft::new("New sequence", Vec::new()),
            selected_id: None,
            selected_step: None,
            reveal_step: None,
            remove_confirmation: None,
            snippets: Vec::new(),
            history: Vec::new(),
            context: None,
            hotkey_text: String::new(),
            inline_error: None,
            status: None,
            snippets_error: None,
            history_error: None,
            autostart_when_ready: false,
            hotkey_update: None,
            saved_run: None,
        }
    }
}

impl PasteSequenceDialog {
    #[cfg(feature = "qa-framebuffer-capture")]
    pub(crate) fn qa_observation(&self) -> PasteSequenceDialogQaObservation {
        PasteSequenceDialogQaObservation {
            open: self.open,
            invocation_context_present: self.context.is_some(),
            invocation_context_has_clipboard_text: self
                .context
                .as_ref()
                .is_some_and(SequenceInvocationContext::has_clipboard_text),
            saved_sequence_ids: self.snapshot.as_ref().map_or_else(Vec::new, |snapshot| {
                snapshot
                    .sequences()
                    .iter()
                    .map(|sequence| sequence.id.get())
                    .collect()
            }),
            selected_sequence_id: self.selected_id.map(SequenceId::get),
            draft_step_count: self.draft.steps.len(),
            remove_confirmation_open: self.remove_confirmation.is_some(),
        }
    }

    pub(crate) fn sequence_store_path(&self) -> &std::path::Path {
        self.store_path.as_path()
    }

    pub(crate) fn open_with_context(
        &mut self,
        context: Result<SequenceInvocationContext, crate::paste_sequence::SequencePreflightError>,
    ) {
        if !self.open && !self.suspended_for_playback {
            self.begin_session();
        }
        self.suspended_for_playback = false;
        match context {
            Ok(context) => {
                self.context = Some(context);
                self.status = Some(
                    "Invocation context captured; each step uses the current eligible application."
                        .into(),
                );
            }
            Err(error) => {
                self.context = None;
                self.inline_error = Some(preflight_error_text(error).into());
            }
        }
        self.open = true;
    }

    pub(crate) fn present_saved_run(
        &mut self,
        sequence: &crate::paste_sequence::Sequence,
        context: SequenceInvocationContext,
    ) {
        if !self.open && !self.suspended_for_playback {
            self.begin_session();
            self.select_saved_sequence(sequence);
        }
        self.suspended_for_playback = false;
        self.context = Some(context);
        self.saved_run = Some(SavedRunPresentation {
            id: sequence.id,
            name: sequence.name.clone(),
        });
        self.status = Some("Saved sequence is preparing to run.".into());
        self.open = true;
    }

    fn begin_session(&mut self) {
        let store_error = match SequenceStore::new(&self.store_path).load() {
            Ok(snapshot) => {
                self.snapshot = Some(snapshot);
                None
            }
            Err(error) => {
                self.snapshot = None;
                Some(format!("Saved sequences could not be loaded: {error}"))
            }
        };
        self.load_sources();
        self.draft = SequenceDraft::new("New sequence", Vec::new());
        self.selected_id = None;
        self.selected_step = None;
        self.reveal_step = None;
        self.remove_confirmation = None;
        self.hotkey_text.clear();
        self.inline_error = store_error;
        self.status = None;
        self.autostart_when_ready = false;
        self.hotkey_update = None;
        self.saved_run = None;
        self.open = true;
    }

    fn load_sources(&mut self) {
        // UI tests exercise real preparation with an owned catalog, never the
        // user's snippet/history files or native clipboard/input.
        #[cfg(test)]
        if self.sources_are_test_owned {
            return;
        }
        match crate::plugins::snippets::load_snippets_for_reload(SNIPPETS_FILE) {
            Ok(crate::common::persistence::LoadState::Missing)
            | Ok(crate::common::persistence::LoadState::Empty) => {
                self.snippets.clear();
                self.snippets_error = None;
            }
            Ok(crate::common::persistence::LoadState::Loaded(entries)) => {
                self.snippets = entries;
                self.snippets_error = None;
            }
            Err(error) => self.snippets_error = Some(error.to_string()),
        }
        match crate::plugins::clipboard::load_history(CLIPBOARD_FILE) {
            Ok(history) => {
                self.history = history.into_iter().collect();
                self.history_error = None;
            }
            Err(error) => {
                self.history.clear();
                self.history_error = Some(error.to_string());
            }
        }
    }

    pub(crate) fn suspend_for_playback(&mut self) {
        if self.open {
            self.open = false;
            self.suspended_for_playback = true;
        }
        self.autostart_when_ready = false;
    }

    pub(crate) fn end_session(&mut self) {
        self.suspended_for_playback = false;
        self.open = false;
        self.snapshot = None;
        self.draft = SequenceDraft::new("New sequence", Vec::new());
        self.selected_id = None;
        self.selected_step = None;
        self.reveal_step = None;
        self.remove_confirmation = None;
        self.snippets.clear();
        self.history.clear();
        self.context = None;
        self.hotkey_text.clear();
        self.inline_error = None;
        self.status = None;
        self.snippets_error = None;
        self.history_error = None;
        self.autostart_when_ready = false;
        self.hotkey_update = None;
        self.saved_run = None;
    }

    pub(crate) fn close_from_window(&mut self, app: &mut super::LauncherApp) {
        self.end_session();
        app.close_paste_sequence_dialog();
    }

    pub(crate) fn take_autostart_when_ready(&mut self) -> bool {
        std::mem::take(&mut self.autostart_when_ready)
    }

    pub(crate) fn set_autostart_when_ready(&mut self, value: bool) {
        self.autostart_when_ready = value;
    }

    pub(crate) fn note_playback_started(&mut self) {
        if self.open {
            self.status = Some(if self.saved_run.is_some() {
                "Saved sequence started.".into()
            } else {
                "Sequence started.".into()
            });
        }
    }

    pub(crate) fn note_playback_start_error(&mut self, message: &str) {
        if self.open {
            self.inline_error = Some(message.into());
            self.autostart_when_ready = false;
        }
    }

    pub(crate) fn ui(&mut self, ctx: &egui::Context, app: &mut super::LauncherApp) {
        self.ui_with_jump_mode(ctx, app, None);
    }

    pub(crate) fn ui_with_jump_mode(
        &mut self,
        ctx: &egui::Context,
        app: &mut super::LauncherApp,
        jump_scope: Option<crate::gui::jump_mode::ui::JumpModeScope>,
    ) {
        if !self.open {
            return;
        }
        self.poll_hotkey_update(ctx);
        let mut jump_visual_order = 0;
        let initial_confirmation_id = self.remove_confirmation;
        let initial_dialog_scope = self.current_dialog_jump_scope(jump_scope.as_ref());
        let initial_confirmation_scope = self.current_confirmation_jump_scope(jump_scope.as_ref());
        let mut body_dialog_scope_for_footer = None;
        let mut window_open = self.open;
        let window = egui::Window::new("Paste Sequences")
            .default_size((760.0, 640.0))
            .min_width(520.0)
            .min_height(420.0)
            .resizable(true)
            .open(&mut window_open)
            .show(ctx, |ui| {
                // Reserve two action rows and the run status before the body requests space.
                // A list-only cap cannot bound the sum of controls, preview and saved rows.
                let row_height = ui
                    .spacing()
                    .interact_size
                    .y
                    .max(ui.text_style_height(&egui::TextStyle::Body));
                let footer_height = row_height * 3.0 + ui.spacing().item_spacing.y * 4.0;
                let body_height = (ui.available_height() - footer_height).max(0.0);
                let step_height = (body_height * 0.6).min(260.0);
                let body = egui::ScrollArea::vertical()
                    .id_salt("paste_sequence_body")
                    .min_scrolled_height(0.0)
                    .max_height(body_height)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                        ui.horizontal_wrapped(|ui| {
                            ui.label("Saved sequences");
                            let playback_settings = ui
                                .button("Playback Settings")
                                .on_hover_text(app.paste_sequence_shortcut_summary());
                            register_paste_sequence_jump_target(
                                initial_dialog_scope.as_ref(),
                                ui,
                                &playback_settings,
                                "playback-settings",
                                "Playback Settings",
                                crate::gui::jump_mode::TargetKind::Button,
                                &mut jump_visual_order,
                            );
                            if playback_settings.clicked() {
                                app.open_paste_sequence_settings_dialog();
                            }
                            let new_sequence = ui.button("New");
                            register_paste_sequence_jump_target(
                                initial_dialog_scope.as_ref(),
                                ui,
                                &new_sequence,
                                "new-sequence",
                                "New",
                                crate::gui::jump_mode::TargetKind::Button,
                                &mut jump_visual_order,
                            );
                            if new_sequence.clicked() {
                                self.selected_id = None;
                                self.selected_step = None;
                                self.reveal_step = None;
                                self.remove_confirmation = None;
                                self.draft = SequenceDraft::new("New sequence", Vec::new());
                                self.hotkey_text.clear();
                                self.inline_error = None;
                            }
                            let remove_saved = ui.add_enabled(
                                self.selected_id.is_some() && self.hotkey_update.is_none(),
                                egui::Button::new("Remove Saved"),
                            );
                            register_paste_sequence_jump_target(
                                initial_dialog_scope.as_ref(),
                                ui,
                                &remove_saved,
                                "remove-saved",
                                "Remove Saved",
                                crate::gui::jump_mode::TargetKind::Button,
                                &mut jump_visual_order,
                            );
                            if remove_saved.clicked() {
                                self.remove_confirmation = self.selected_id;
                            }
                        });
                        // The header may have opened or closed the inline
                        // confirmation. Select scopes from the live state so
                        // the remainder of this same UI pass belongs only to
                        // its actual foreground surface.
                        let dialog_scope = self.current_dialog_jump_scope(jump_scope.as_ref());
                        body_dialog_scope_for_footer = dialog_scope.clone();
                        let confirmation_scope =
                            self.current_confirmation_jump_scope(jump_scope.as_ref());
                        self.saved_sequence_list(ui, dialog_scope.as_ref(), &mut jump_visual_order);
                        self.saved_sequence_remove_confirmation(
                            ui,
                            ctx,
                            app,
                            confirmation_scope.as_ref(),
                            &mut jump_visual_order,
                        );
                        ui.separator();
                        self.editor(
                            ui,
                            app,
                            step_height,
                            dialog_scope.as_ref(),
                            &mut jump_visual_order,
                        );
                    });
                #[cfg(test)]
                {
                    self.layout.body = body.inner_rect;
                    self.layout.body_content = body.content_size;
                    self.layout.body_id = Some(body.id);
                    self.layout.body_offset = body.state.offset.y;
                }
                #[cfg(not(test))]
                let _ = body;
                ui.separator();
                let footer = ui.scope(|ui| {
                    self.editor_footer(
                        ui,
                        ctx,
                        app,
                        body_dialog_scope_for_footer.as_ref(),
                        &mut jump_visual_order,
                    )
                });
                #[cfg(test)]
                {
                    self.layout.footer = footer.response.rect;
                }
                #[cfg(not(test))]
                let _ = footer;
            });
        #[cfg(test)]
        if let Some(window) = &window {
            self.layout.window = window.response.rect;
        }
        #[cfg(not(test))]
        let _ = window;
        if self.open && !window_open {
            self.close_from_window(app);
        }
        if initial_confirmation_id.is_some()
            && self.remove_confirmation.is_none()
            && let Some(scope) = initial_confirmation_scope.as_ref()
        {
            scope.discard_registered_targets();
        }
    }

    fn current_dialog_jump_scope(
        &self,
        scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
    ) -> Option<crate::gui::jump_mode::ui::JumpModeScope> {
        if self.remove_confirmation.is_some() {
            return None;
        }
        scope.cloned()
    }

    fn current_confirmation_jump_scope(
        &self,
        scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
    ) -> Option<crate::gui::jump_mode::ui::JumpModeScope> {
        let id = self.remove_confirmation?;
        Some(
            scope?
                .child_popup_scope("remove-saved-confirm")
                .child_namespace(format!("sequence-{}", id.get())),
        )
    }

    fn saved_sequence_list(
        &mut self,
        ui: &mut egui::Ui,
        jump_scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
        jump_visual_order: &mut u64,
    ) {
        let sequences = self
            .snapshot
            .as_ref()
            .map(|snapshot| snapshot.sequences().to_vec())
            .unwrap_or_default();
        egui::ScrollArea::vertical()
            .id_salt("paste_sequence_saved_list")
            .max_height(118.0)
            .show(ui, |ui| {
                if sequences.is_empty() {
                    ui.weak("No saved sequences yet.");
                }
                for sequence in sequences {
                    let selected = self.selected_id == Some(sequence.id);
                    let row_id = ui.id().with(("saved-sequence-row", sequence.id.get()));
                    let response = ui.scope_builder(egui::UiBuilder::new().id(row_id), |ui| {
                        ui.selectable_label(
                            selected,
                            format!("{}  ·  {} steps", sequence.name, sequence.steps.len()),
                        )
                    });
                    let response = response.inner;
                    register_paste_sequence_jump_target(
                        jump_scope,
                        ui,
                        &response,
                        format!("saved-sequence-{}", sequence.id.get()),
                        format!("{}  ·  {} steps", sequence.name, sequence.steps.len()),
                        crate::gui::jump_mode::TargetKind::Button,
                        jump_visual_order,
                    );
                    if response.clicked() {
                        self.select_saved_sequence(&sequence);
                        self.inline_error = None;
                    }
                }
            });
    }

    fn select_saved_sequence(&mut self, sequence: &crate::paste_sequence::Sequence) {
        self.draft = SequenceDraft::from_sequence(sequence);
        self.selected_id = Some(sequence.id);
        self.selected_step = None;
        self.reveal_step = None;
        self.remove_confirmation = None;
        self.hotkey_text = sequence
            .hotkey
            .as_ref()
            .map(|chord| chord.as_str().to_owned())
            .unwrap_or_default();
    }

    fn saved_sequence_remove_confirmation(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        app: &mut super::LauncherApp,
        jump_scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
        jump_visual_order: &mut u64,
    ) {
        let Some(id) = self.remove_confirmation else {
            return;
        };
        let name = self
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.sequence(id))
            .map(|sequence| sequence.name.clone());
        let Some(name) = name else {
            self.remove_confirmation = None;
            return;
        };
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(format!("Remove saved sequence ‘{name}’?")).strong());
            let confirm_remove = ui.button("Confirm Remove");
            register_paste_sequence_jump_target(
                jump_scope,
                ui,
                &confirm_remove,
                "confirm-remove",
                "Confirm Remove",
                crate::gui::jump_mode::TargetKind::Button,
                jump_visual_order,
            );
            if confirm_remove.clicked() {
                self.remove_saved_sequence(ctx, app, id);
            }
            let keep = ui.button("Keep");
            register_paste_sequence_jump_target(
                jump_scope,
                ui,
                &keep,
                "keep",
                "Keep",
                crate::gui::jump_mode::TargetKind::Button,
                jump_visual_order,
            );
            if keep.clicked() {
                self.remove_confirmation = None;
            }
        });
    }

    fn remove_saved_sequence(
        &mut self,
        ctx: &egui::Context,
        app: &mut super::LauncherApp,
        id: SequenceId,
    ) {
        let Some(previous) = self.snapshot.as_ref().cloned() else {
            self.inline_error = Some(
                "Saved sequences are unavailable because the store could not be loaded.".into(),
            );
            return;
        };
        let mut candidate = previous.clone();
        if !candidate.remove(id) {
            self.remove_confirmation = None;
            self.inline_error = Some("That saved sequence no longer exists.".into());
            return;
        }
        self.remove_confirmation = None;
        self.inline_error = None;
        self.status = Some("Preparing sequence removal and shortcut update…".into());
        self.begin_hotkey_update(
            ctx,
            app,
            previous,
            candidate,
            SequenceEditorUpdate::Removed { id },
            StartShortcutUpdate::Keep,
        );
    }

    fn editor(
        &mut self,
        ui: &mut egui::Ui,
        app: &mut super::LauncherApp,
        step_height: f32,
        jump_scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
        jump_visual_order: &mut u64,
    ) {
        if let Some(origin) = self
            .context
            .as_ref()
            .and_then(crate::paste_sequence::SequenceInvocationContext::origin)
        {
            let label = origin
                .title()
                .filter(|title| !title.trim().is_empty())
                .unwrap_or("External window");
            ui.weak(format!("Template context captured from: {label}"));
        } else {
            ui.weak("No origin needed; start playback, then focus an eligible application.");
        }
        let mut name_changed = false;
        ui.horizontal(|ui| {
            ui.label("Name");
            let name = ui.text_edit_singleline(&mut self.draft.name);
            register_paste_sequence_jump_target(
                jump_scope,
                ui,
                &name,
                "name",
                "Name",
                crate::gui::jump_mode::TargetKind::TextEntry,
                jump_visual_order,
            );
            name_changed = name.changed();
        });
        if name_changed {
            self.inline_error = None;
        }
        ui.horizontal(|ui| {
            ui.label("Optional start hotkey");
            let hotkey =
                ui.add(egui::TextEdit::singleline(&mut self.hotkey_text).desired_width(180.0));
            register_paste_sequence_jump_target(
                jump_scope,
                ui,
                &hotkey,
                "start-hotkey",
                "Optional start hotkey",
                crate::gui::jump_mode::TargetKind::TextEntry,
                jump_visual_order,
            );
        });
        ui.weak("Uses the application's existing chord syntax");
        ui.horizontal_wrapped(|ui| {
            ui.label("Playback");
            let automatic =
                ui.selectable_value(&mut self.draft.mode, PlaybackMode::Automatic, "Automatic");
            register_paste_sequence_jump_target(
                jump_scope,
                ui,
                &automatic,
                "mode-automatic",
                "Automatic playback",
                crate::gui::jump_mode::TargetKind::Button,
                jump_visual_order,
            );
            let manual = ui.selectable_value(
                &mut self.draft.mode,
                PlaybackMode::ManualAdvance,
                "Manual advance",
            );
            register_paste_sequence_jump_target(
                jump_scope,
                ui,
                &manual,
                "mode-manual",
                "Manual advance playback",
                crate::gui::jump_mode::TargetKind::Button,
                jump_visual_order,
            );
            let paste_to_advance = ui.selectable_value(
                &mut self.draft.mode,
                PlaybackMode::PasteToAdvance,
                "Paste-to-Advance (Ctrl+V)",
            );
            register_paste_sequence_jump_target(
                jump_scope,
                ui,
                &paste_to_advance,
                "mode-paste-to-advance",
                "Paste-to-Advance playback",
                crate::gui::jump_mode::TargetKind::Button,
                jump_visual_order,
            );
        });
        ui.add_enabled_ui(self.draft.mode != PlaybackMode::PasteToAdvance, |ui| {
            ui.horizontal(|ui| {
                ui.label("Delay between steps");
                ui.add(egui::Slider::new(&mut self.draft.delay_ms, 0..=60_000).suffix(" ms"));
            })
        });
        ui.weak(match self.draft.mode {
            PlaybackMode::Automatic => "Each step pastes into the current eligible application after the delay.",
            PlaybackMode::ManualAdvance => "Next pastes into the current eligible application; the delay is the minimum cooldown between steps.",
            PlaybackMode::PasteToAdvance => "One real Ctrl+V pastes one step into the current eligible application. Wait for it to settle, then release V and press again. The last item stays on the clipboard; the next Ctrl+V is an ordinary paste. The stored delay is unused in this mode.",
        });
        if app.sequence_playback.started_run().is_some() {
            ui.weak(if app.sequence_playback.escape_cancel_available() {
                "End early with Esc, configured Cancel, or Cancel Run."
            } else {
                "End early with configured Cancel or Cancel Run."
            });
        } else {
            ui.weak("Cancel Run or configured Cancel ends early. Optional Esc cancellation is in Playback Settings.");
        }

        ui.horizontal_wrapped(|ui| {
            ui.menu_button("Add Snippet", |ui| {
                if let Some(error) = &self.snippets_error {
                    ui.label(format!("Snippet catalog unavailable: {error}"));
                } else if self.snippets.is_empty() {
                    ui.weak("No saved snippets are available.");
                }
                for snippet in &self.snippets {
                    let label = if snippet.hide_contents {
                        format!("{}  ·  hidden", snippet.alias)
                    } else {
                        snippet.alias.clone()
                    };
                    if ui.button(label).clicked() {
                        self.draft.steps.push(Step::SnippetRef {
                            alias: snippet.alias.clone(),
                        });
                        self.selected_step = self.draft.steps.len().checked_sub(1);
                        self.reveal_step = self.selected_step;
                        ui.close();
                    }
                }
            });
            ui.menu_button("Add Clipboard Item", |ui| {
                if let Some(error) = &self.history_error {
                    ui.label(format!("Clipboard history unavailable: {error}"));
                }
                if self.history.is_empty() {
                    ui.weak("Clipboard history items can expire or be removed.");
                }
                for index in 0..self.history.len() {
                    if ui.button(format!("History item {}", index + 1)).clicked() {
                        self.draft.steps.push(Step::clipboard_history_reference(
                            self.history
                                .get(index)
                                .map(String::as_str)
                                .unwrap_or_default(),
                        ));
                        self.status = Some(
                            "Clipboard steps refer to history and can expire before a run.".into(),
                        );
                        self.selected_step = self.draft.steps.len().checked_sub(1);
                        self.reveal_step = self.selected_step;
                        ui.close();
                    }
                }
            });
            let add_temporary_text = ui.button("Add Temporary Text");
            register_paste_sequence_jump_target(
                jump_scope,
                ui,
                &add_temporary_text,
                "add-temporary-text",
                "Add Temporary Text",
                crate::gui::jump_mode::TargetKind::Button,
                jump_visual_order,
            );
            if add_temporary_text.clicked() {
                self.draft.steps.push(Step::LiteralText {
                    text: String::new(),
                });
                self.selected_step = self.draft.steps.len().checked_sub(1);
                self.reveal_step = self.selected_step;
            }
            let move_up = ui.button("Move Up");
            register_paste_sequence_jump_target(
                jump_scope,
                ui,
                &move_up,
                "move-up",
                "Move Up",
                crate::gui::jump_mode::TargetKind::Button,
                jump_visual_order,
            );
            if move_up.clicked()
                && let Some(index) = self.selected_step
                && index > 0
            {
                self.draft.steps.swap(index, index - 1);
                self.selected_step = Some(index - 1);
                self.reveal_step = self.selected_step;
            }
            let move_down = ui.button("Move Down");
            register_paste_sequence_jump_target(
                jump_scope,
                ui,
                &move_down,
                "move-down",
                "Move Down",
                crate::gui::jump_mode::TargetKind::Button,
                jump_visual_order,
            );
            if move_down.clicked()
                && let Some(index) = self.selected_step
                && index + 1 < self.draft.steps.len()
            {
                self.draft.steps.swap(index, index + 1);
                self.selected_step = Some(index + 1);
                self.reveal_step = self.selected_step;
            }
            let remove_step =
                ui.add_enabled(self.selected_step.is_some(), egui::Button::new("Remove"));
            register_paste_sequence_jump_target(
                jump_scope,
                ui,
                &remove_step,
                "remove-step",
                "Remove selected step",
                crate::gui::jump_mode::TargetKind::Button,
                jump_visual_order,
            );
            if remove_step.clicked()
                && let Some(index) = self.selected_step.take()
                && index < self.draft.steps.len()
            {
                self.draft.steps.remove(index);
            }
        });
        ui.label("Ordered steps");
        self.step_editor(ui, step_height);

        if let Some(error) = &self.inline_error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        if let Some(status) = &self.status {
            ui.weak(status);
        }
        if let Some(preview) = app.sequence_playback.prepared_preview() {
            self.show_preview(ui, &preview);
        }
    }

    fn editor_footer(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        app: &mut super::LauncherApp,
        jump_scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
        jump_visual_order: &mut u64,
    ) {
        let state = app.sequence_playback.status();
        ui.horizontal(|ui| {
            let preview =
                ui.add_enabled(!self.draft.steps.is_empty(), egui::Button::new("Preview"));
            register_paste_sequence_jump_target(
                jump_scope,
                ui,
                &preview,
                "preview",
                "Preview",
                crate::gui::jump_mode::TargetKind::Button,
                jump_visual_order,
            );
            if preview.clicked() {
                self.prepare_current(app, false, false);
            }
            let run_once =
                ui.add_enabled(!self.draft.steps.is_empty(), egui::Button::new("Run Once"));
            register_paste_sequence_jump_target(
                jump_scope,
                ui,
                &run_once,
                "run-once",
                "Run Once",
                crate::gui::jump_mode::TargetKind::Button,
                jump_visual_order,
            );
            if run_once.clicked() {
                self.prepare_current(app, true, true);
            }
            let run = ui.add_enabled(!self.draft.steps.is_empty(), egui::Button::new("Run"));
            register_paste_sequence_jump_target(
                jump_scope,
                ui,
                &run,
                "run",
                "Run",
                crate::gui::jump_mode::TargetKind::Button,
                jump_visual_order,
            );
            if run.clicked() {
                self.prepare_current(app, true, false);
            }
            let save = ui.add_enabled(
                self.snapshot.is_some() && self.hotkey_update.is_none(),
                egui::Button::new("Save"),
            );
            register_paste_sequence_jump_target(
                jump_scope,
                ui,
                &save,
                "save",
                "Save",
                crate::gui::jump_mode::TargetKind::Button,
                jump_visual_order,
            );
            if save.clicked() {
                self.save_draft(ctx, app);
            }
        });
        ui.horizontal(|ui| {
            let start = ui.add_enabled(
                state == crate::paste_sequence::SequencePlaybackStatus::Ready,
                egui::Button::new("Start"),
            );
            register_paste_sequence_jump_target(
                jump_scope,
                ui,
                &start,
                "start",
                "Start",
                crate::gui::jump_mode::TargetKind::Button,
                jump_visual_order,
            );
            if start.clicked() {
                match app.start_sequence_playback() {
                    Ok(()) => {
                        self.status = Some("Sequence started.".into());
                        // Rendering temporarily takes the dialog out of LauncherApp.
                        if !app.should_keep_paste_sequence_dialog_visible_during_playback() {
                            self.suspend_for_playback();
                        }
                    }
                    Err(error) => self.inline_error = Some(error.message().into()),
                }
            }
            let next = ui.add_enabled(
                state == crate::paste_sequence::SequencePlaybackStatus::AwaitAdvance,
                egui::Button::new("Next"),
            );
            register_paste_sequence_jump_target(
                jump_scope,
                ui,
                &next,
                "next",
                "Next",
                crate::gui::jump_mode::TargetKind::Button,
                jump_visual_order,
            );
            if next.clicked() {
                match app.next_sequence_playback() {
                    Ok(()) => self.status = Some("Advanced to the next step.".into()),
                    Err(error) => self.inline_error = Some(error.message().into()),
                }
            }
            let cancel_run = ui.add_enabled(
                matches!(
                    state,
                    crate::paste_sequence::SequencePlaybackStatus::Preparing
                        | crate::paste_sequence::SequencePlaybackStatus::Ready
                        | crate::paste_sequence::SequencePlaybackStatus::Running
                        | crate::paste_sequence::SequencePlaybackStatus::AwaitAdvance
                        | crate::paste_sequence::SequencePlaybackStatus::AwaitTarget
                        | crate::paste_sequence::SequencePlaybackStatus::AwaitPaste
                ),
                egui::Button::new("Cancel Run"),
            );
            register_paste_sequence_jump_target(
                jump_scope,
                ui,
                &cancel_run,
                "cancel-run",
                "Cancel Run",
                crate::gui::jump_mode::TargetKind::Button,
                jump_visual_order,
            );
            if cancel_run.clicked() {
                app.cancel_active_paste_sequence();
                self.autostart_when_ready = false;
                self.status =
                    Some("Sequence cancelled; the latest clipboard value is retained.".into());
            }
            let cancel_editor = ui.button("Cancel Editor");
            register_paste_sequence_jump_target(
                jump_scope,
                ui,
                &cancel_editor,
                "cancel-editor",
                "Cancel Editor",
                crate::gui::jump_mode::TargetKind::Button,
                jump_visual_order,
            );
            if cancel_editor.clicked() {
                app.cancel_active_paste_sequence();
                self.end_session();
                app.close_paste_sequence_dialog();
            }
        });
        self.show_run_status(ui, app);
    }

    fn step_editor(&mut self, ui: &mut egui::Ui, max_height: f32) {
        let mut remove = None;
        let reveal = self.reveal_step.take();
        let steps = egui::ScrollArea::vertical()
            .id_salt("paste_sequence_step_editor")
            // The outer body owns overflow from the preceding controls. Reserve
            // this viewport rather than letting their remaining height collapse it.
            .min_scrolled_height(max_height)
            .max_height(max_height)
            .show(ui, |ui| {
                for (index, step) in self.draft.steps.iter_mut().enumerate() {
                    let selected = self.selected_step == Some(index);
                    let row =
                        ui.group(|ui| {
                            ui.set_max_width(ui.available_width());
                            ui.horizontal_wrapped(|ui| {
                                if ui
                                    .selectable_label(selected, format!("{}.", index + 1))
                                    .clicked()
                                {
                                    self.selected_step = Some(index);
                                }
                                match step {
                                    Step::SnippetRef { alias } => {
                                        ui.label("Snippet");
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(alias.as_str()).monospace(),
                                            )
                                            .wrap(),
                                        );
                                        if self.snippets.iter().any(|entry| {
                                            entry.alias == *alias && entry.hide_contents
                                        }) {
                                            ui.weak("hidden contents");
                                        }
                                    }
                                    Step::ClipboardSnapshotOrRef { .. } => {
                                        ui.label("Clipboard history item");
                                        ui.weak(
                                        "payload is not stored; the history reference can expire",
                                    );
                                    }
                                    Step::LiteralText { .. } => {
                                        ui.label("Temporary text");
                                        ui.weak("saved sequences store this text in plaintext");
                                    }
                                }
                                if ui.small_button("Remove").clicked() {
                                    remove = Some(index);
                                }
                            });
                            if let Step::LiteralText { text } = step {
                                ui.add(
                                    egui::TextEdit::multiline(text)
                                        .desired_rows(2)
                                        .desired_width(ui.available_width())
                                        .hint_text("Text pasted when this step runs"),
                                );
                            }
                        });
                    if reveal == Some(index) {
                        row.response.scroll_to_me_animation(
                            Some(egui::Align::Center),
                            egui::style::ScrollAnimation::none(),
                        );
                    }
                }
            });
        if reveal.is_some() {
            // The row request belongs to the inner list. Separately reveal that
            // viewport in the body when controls above it have scrolled out of view.
            ui.scroll_to_rect_animation(
                steps.inner_rect,
                Some(egui::Align::BOTTOM),
                egui::style::ScrollAnimation::none(),
            );
        }
        #[cfg(test)]
        {
            self.layout.steps = steps.inner_rect;
            self.layout.steps_content = steps.content_size;
            self.layout.steps_id = Some(steps.id);
            self.layout.steps_offset = steps.state.offset.y;
        }
        #[cfg(not(test))]
        let _ = steps;
        if let Some(index) = remove {
            self.draft.steps.remove(index);
            if self.selected_step == Some(index) {
                self.selected_step = None;
                self.reveal_step = None;
            } else if self.selected_step.is_some_and(|selected| selected > index) {
                self.selected_step = self.selected_step.map(|selected| selected - 1);
            }
        }
        if self.draft.steps.is_empty() {
            ui.weak("Add at least one step. Snippet prompts are resolved before any paste.");
        }
    }

    fn show_run_status(&self, ui: &mut egui::Ui, app: &super::LauncherApp) {
        let status = run_status_text(
            app.sequence_playback.status(),
            app.sequence_playback.current_step_number(),
        );
        let message = saved_run_status_text(self.saved_run.as_ref(), status);
        ui.add(egui::Label::new(&message).truncate())
            .on_hover_text(&message);
    }

    fn show_preview(&self, ui: &mut egui::Ui, preview: &PreparedSequencePreview) {
        ui.separator();
        ui.label(format!(
            "Preview · {} steps · {}",
            preview.item_count,
            playback_mode_label(preview.mode)
        ));
        if preview.mode != PlaybackMode::PasteToAdvance {
            ui.weak(format!(
                "Delay / minimum Next cooldown: {} ms",
                preview.delay_ms
            ));
        }
        ui.weak(format!("Paste target: {}", preview.target_label));
        ui.weak("Template values stay frozen from invocation; the paste target may change between steps.");
        egui::ScrollArea::vertical()
            .id_salt("paste_sequence_preview")
            .max_height(112.0)
            .show(ui, |ui| {
                for step in &preview.steps {
                    ui.horizontal(|ui| {
                        ui.label(format!("{}.", step.index + 1));
                        if step.hidden {
                            ui.weak("Hidden snippet contents");
                        } else if let Some(text) = &step.text {
                            ui.monospace(text.as_str());
                        } else {
                            ui.weak("Preview unavailable");
                        }
                    });
                }
            });
    }

    fn prepare_current(&mut self, app: &mut super::LauncherApp, autostart: bool, run_once: bool) {
        self.prepare_current_using(app, autostart, run_once, |app| {
            app.start_sequence_playback()
        });
    }

    fn prepare_current_using(
        &mut self,
        app: &mut super::LauncherApp,
        autostart: bool,
        run_once: bool,
        start: impl FnOnce(
            &mut super::LauncherApp,
        ) -> Result<(), crate::paste_sequence::SequencePlaybackError>,
    ) {
        self.inline_error = None;
        self.status = None;
        let Some(context) = self.context.clone() else {
            self.inline_error = Some(
                "Invocation context is unavailable. Close and reopen this editor before running."
                    .into(),
            );
            return;
        };
        self.load_sources();
        self.autostart_when_ready = autostart;
        let result = app.begin_paste_sequence_draft_preflight_with_context(
            self.current_draft(),
            &self.snippets,
            &self.history,
            context,
        );
        match result {
            Ok(()) => {
                self.saved_run = None;
                let prompt_open = app.snippet_prompt_dialog.is_open();
                self.status = Some(if run_once {
                    "Run Once is memory-only; nothing was written to the saved sequence file."
                        .into()
                } else if autostart {
                    "Sequence prepared; prompts must finish before Start.".into()
                } else {
                    "Preview is ready; no text has been pasted.".into()
                });
                if autostart && !prompt_open {
                    self.autostart_when_ready = false;
                    match start(app) {
                        Ok(()) => {
                            self.status = Some("Sequence started.".into());
                            // Rendering temporarily takes the dialog out of LauncherApp.
                            if !app.should_keep_paste_sequence_dialog_visible_during_playback() {
                                self.suspend_for_playback();
                            }
                        }
                        Err(error) => {
                            self.inline_error = Some(error.message().into());
                            self.autostart_when_ready = false;
                        }
                    }
                }
            }
            Err(error) => {
                self.autostart_when_ready = false;
                self.inline_error = Some(preflight_error_text(error).into());
            }
        }
    }

    fn current_draft(&self) -> SequenceDraft {
        let mut draft = self.draft.clone();
        draft.hotkey = if self.hotkey_text.trim().is_empty() {
            None
        } else {
            // Keep invalid text local so Save reports it below; previews and
            // Run intentionally do not install a route.
            HotkeyChord::new(self.hotkey_text.clone()).ok()
        };
        draft
    }

    #[cfg(test)]
    pub(crate) fn use_test_data_sources(&mut self, store_path: PathBuf) {
        self.store_path = store_path;
        self.sources_are_test_owned = true;
    }

    #[cfg(test)]
    pub(crate) fn test_draft_name(&self) -> &str {
        &self.draft.name
    }

    #[cfg(test)]
    pub(crate) fn test_selected_id(&self) -> Option<SequenceId> {
        self.selected_id
    }

    #[cfg(test)]
    pub(crate) fn test_draft_id(&self) -> Option<SequenceId> {
        self.draft.id
    }

    #[cfg(test)]
    pub(crate) fn test_remove_saved_snapshot_row(&mut self, id: SequenceId) -> bool {
        self.snapshot
            .as_mut()
            .is_some_and(|snapshot| snapshot.remove(id))
    }

    fn save_draft(&mut self, ctx: &egui::Context, app: &mut super::LauncherApp) {
        self.inline_error = None;
        let mut draft = self.current_draft();
        let requested_start = if !self.hotkey_text.trim().is_empty() && draft.hotkey.is_none() {
            StartShortcutUpdate::Invalid
        } else {
            StartShortcutUpdate::Set(draft.hotkey.clone())
        };
        let Some(previous) = self.snapshot.as_ref().cloned() else {
            self.inline_error = Some(
                "Saved sequences are unavailable because the store could not be loaded.".into(),
            );
            return;
        };
        // Content persistence must not depend on accepting a new start route.
        draft.hotkey = draft
            .id
            .and_then(|id| previous.sequence(id))
            .and_then(|sequence| sequence.hotkey.clone());
        let mut candidate = previous.clone();
        match candidate.upsert(draft.clone()) {
            Ok(id) => {
                self.status = Some("Saving sequence content…".into());
                self.begin_hotkey_update(
                    ctx,
                    app,
                    previous,
                    candidate,
                    SequenceEditorUpdate::Saved { id },
                    requested_start,
                );
            }
            Err(error) => self.inline_error = Some(store_error_text(&error)),
        }
    }

    fn begin_hotkey_update(
        &mut self,
        ctx: &egui::Context,
        app: &mut super::LauncherApp,
        previous: SequenceSnapshot,
        mut candidate: SequenceSnapshot,
        editor_update: SequenceEditorUpdate,
        requested_start: StartShortcutUpdate,
    ) {
        let settings_path = app.settings_path.clone();
        let store = SequenceStore::new(self.store_path.clone());
        let repaint = ctx.clone();
        let revision_notifier = app.paste_sequence_document_revision_notifier();
        let (sender, receiver) = std::sync::mpsc::channel();
        let spawned = app.settings_save_workers.spawn(move || {
            let ticket = std::cell::RefCell::new(None);
            let settings_for_apply = std::cell::RefCell::new(None);
            let plugin_enabled = std::cell::Cell::new(false);
            let result = commit_sequence_editor_update(
                &store,
                &previous,
                &mut candidate,
                &editor_update,
                requested_start,
                |bindings| {
                use crate::hotkey::reconfiguration::{
                    prepare_hotkey_reconfiguration_with_sequence_bindings,
                };
                use std::time::Duration;

                let settings = crate::settings::Settings::load(&settings_path)
                    .map_err(|error| format!("settings could not be loaded: {error}"))?;
                plugin_enabled.set(settings
                    .enabled_plugins
                    .as_ref()
                    .is_none_or(|plugins| plugins.contains("paste_sequence")));
                let mut prepared = prepare_hotkey_reconfiguration_with_sequence_bindings(
                    settings.clone(),
                    bindings.to_vec(),
                )
                .map_err(|error| format!("shortcut preparation failed: {error}"))?;
                prepared
                    .wait_prepared(Duration::from_secs(30))
                    .map_err(|error| format!("shortcut preparation failed: {error}"))?;
                *ticket.borrow_mut() = Some(prepared);
                *settings_for_apply.borrow_mut() = Some(settings);
                Ok(())
            },
            || {
                use crate::hotkey::reconfiguration::{
                    HotkeyApplySubmissionError, HotkeyTerminalDisposition,
                };
                use std::time::Duration;

                let mut ticket = ticket.borrow_mut().take().ok_or_else(|| {
                    RouteActivationFailure::Confirmed(
                        "prepared shortcut transaction was unavailable".into(),
                    )
                })?;
                let settings = settings_for_apply.borrow_mut().take().ok_or_else(|| {
                    RouteActivationFailure::Confirmed(
                        "prepared settings were unavailable".into(),
                    )
                })?;
                match ticket.apply(settings) {
                    Ok(()) => {}
                    Err(HotkeyApplySubmissionError::NotEnqueued(error)) => {
                        return Err(RouteActivationFailure::Confirmed(error.to_string()));
                    }
                    Err(HotkeyApplySubmissionError::AlreadyEnqueued(error)) => {
                        return Err(RouteActivationFailure::Uncertain(error.to_string()));
                    }
                }
                match ticket.wait_terminal_disposition(Duration::from_secs(30)) {
                    Ok(HotkeyTerminalDisposition::Applied(_)) => Ok(plugin_enabled.get()),
                    Ok(HotkeyTerminalDisposition::ConfirmedNotAppliedOrRestored(error)) => {
                        Err(RouteActivationFailure::Confirmed(error.to_string()))
                    }
                    Ok(HotkeyTerminalDisposition::RestorationUnavailable(error)) => {
                        Err(RouteActivationFailure::Uncertain(error.to_string()))
                    }
                    Err(error) => Err(RouteActivationFailure::Uncertain(format!(
                        "shortcut activation was not confirmed: {error}"
                    ))),
                }
            });
            if let Some(mut staged) = ticket.borrow_mut().take() {
                use std::time::Duration;
                if staged.cancel().is_ok() {
                    let _ = staged.wait_terminal_disposition(Duration::from_secs(5));
                }
            }
            let snapshot = load_sequence_snapshot_and_notify(&store, &revision_notifier);
            let _ = sender.send(HotkeyUpdateCompletion {
                result,
                snapshot,
                editor_update,
            });
            repaint.request_repaint();
            repaint.request_repaint_of(egui::ViewportId::ROOT);
        });
        match spawned {
            Ok(()) => self.hotkey_update = Some(receiver),
            Err(error) => {
                self.inline_error = Some(format!(
                    "The sequence update could not start; no changes were saved: {error}"
                ));
                self.status = None;
            }
        }
    }

    fn poll_hotkey_update(&mut self, ctx: &egui::Context) {
        let Some(receiver) = self.hotkey_update.as_ref() else {
            return;
        };
        match receiver.try_recv() {
            Ok(completion) => {
                self.hotkey_update = None;
                if let Some(snapshot) = completion.snapshot {
                    reconcile_editor_update(
                        &mut self.snapshot,
                        &mut self.draft,
                        &mut self.selected_id,
                        &mut self.selected_step,
                        &mut self.hotkey_text,
                        snapshot,
                        &completion.editor_update,
                        completion.result.is_ok(),
                    );
                }
                match completion.result {
                    Ok(SequenceCommitOutcome::ContentOnly) => {
                        self.status = Some(
                            if matches!(
                                completion.editor_update,
                                SequenceEditorUpdate::Removed { .. }
                            ) {
                                "Sequence removed; shortcut routes were unchanged."
                            } else {
                                "Sequence content saved; shortcut routes were unchanged."
                            }
                            .into(),
                        );
                        self.inline_error = None;
                    }
                    Ok(SequenceCommitOutcome::BindingApplied { plugin_enabled }) => {
                        self.status = Some(
                            binding_update_status(&completion.editor_update, plugin_enabled).into(),
                        );
                        self.inline_error = None;
                    }
                    Ok(SequenceCommitOutcome::ContentSavedBindingRejected {
                        message,
                        routes_uncertain,
                    }) => {
                        self.status = Some(
                            "Sequence content saved; start shortcut update was not accepted."
                                .into(),
                        );
                        self.inline_error = Some(if routes_uncertain {
                            format!(
                                "{message} Shortcut state is uncertain; reopen Settings to reconcile before using shortcuts."
                            )
                        } else {
                            message
                        });
                    }
                    Err(error) => {
                        self.status = None;
                        self.inline_error = Some(error);
                    }
                }
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.hotkey_update = None;
                self.status = None;
                self.inline_error = Some(
                    "Sequence update could not be confirmed; check the saved list before retrying."
                        .into(),
                );
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(100))
            }
        }
    }
}

fn register_paste_sequence_jump_target(
    scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
    ui: &egui::Ui,
    response: &egui::Response,
    local_identity: impl AsRef<str>,
    label: impl Into<String>,
    kind: crate::gui::jump_mode::TargetKind,
    visual_order: &mut u64,
) {
    let order = *visual_order;
    *visual_order = visual_order.saturating_add(1);
    let Some(scope) = scope else {
        return;
    };
    if let crate::gui::jump_mode::ui::RegistrationResult::Registered {
        directive: Some(directive),
        ..
    } = scope.register(ui, response, local_identity, label, kind, order)
    {
        directive.apply_to_response(response);
    }
}

fn commit_sequence_editor_update(
    store: &SequenceStore,
    previous: &SequenceSnapshot,
    candidate: &mut SequenceSnapshot,
    update: &SequenceEditorUpdate,
    requested_start: StartShortcutUpdate,
    prepare: impl FnOnce(&[SequenceHotkeyBinding]) -> Result<(), String>,
    apply: impl FnOnce() -> Result<bool, RouteActivationFailure>,
) -> Result<SequenceCommitOutcome, String> {
    if let SequenceEditorUpdate::Saved { id } = update {
        store.commit(candidate).map_err(|error| {
            format!(
                "Sequence content could not be saved: {}",
                store_error_text(&error)
            )
        })?;
        let requested = match requested_start {
            StartShortcutUpdate::Invalid => return Ok(SequenceCommitOutcome::ContentSavedBindingRejected {
                message: "Start shortcut is invalid. Saved content retains its previous start shortcut (or disabled route).".into(),
                routes_uncertain: false,
            }),
            StartShortcutUpdate::Set(requested) => requested,
            StartShortcutUpdate::Keep => return Ok(SequenceCommitOutcome::ContentOnly),
        };
        let saved = candidate
            .sequence(*id)
            .ok_or("Saved sequence could not be found after content commit")?;
        if saved.hotkey == requested {
            return Ok(SequenceCommitOutcome::ContentOnly);
        }
        let content_saved = candidate.clone();
        let mut binding_draft = SequenceDraft::from_sequence(saved);
        binding_draft.hotkey = requested;
        if let Err(error) = candidate.upsert(binding_draft) {
            return Ok(SequenceCommitOutcome::ContentSavedBindingRejected {
                message: format!(
                    "Start shortcut could not be saved: {}. Authored content was saved with its previous shortcut.",
                    store_error_text(&error)
                ),
                routes_uncertain: false,
            });
        }
        return Ok(
            match commit_sequence_candidate_with_route_update(
                store,
                &content_saved,
                candidate,
                prepare,
                apply,
            ) {
                Ok(plugin_enabled) => SequenceCommitOutcome::BindingApplied { plugin_enabled },
                Err(error) => SequenceCommitOutcome::ContentSavedBindingRejected {
                    message: format!("Content saved. {}", error.message),
                    routes_uncertain: error.uncertain,
                },
            },
        );
    }

    if previous.hotkey_bindings() == candidate.hotkey_bindings() {
        store.commit(candidate).map_err(|error| {
            format!(
                "Sequence removal could not be saved: {}",
                store_error_text(&error)
            )
        })?;
        return Ok(SequenceCommitOutcome::ContentOnly);
    }
    commit_sequence_candidate_with_route_update(store, previous, candidate, prepare, apply)
        .map(|plugin_enabled| SequenceCommitOutcome::BindingApplied { plugin_enabled })
        .map_err(|error| error.message)
}

fn commit_sequence_candidate_with_route_update(
    store: &SequenceStore,
    previous: &SequenceSnapshot,
    candidate: &mut SequenceSnapshot,
    prepare: impl FnOnce(&[SequenceHotkeyBinding]) -> Result<(), String>,
    apply: impl FnOnce() -> Result<bool, RouteActivationFailure>,
) -> Result<bool, RouteUpdateFailure> {
    let bindings = candidate.hotkey_bindings();
    prepare(&bindings).map_err(|error| RouteUpdateFailure {
        message: format!(
            "Shortcut preparation failed; the requested binding change was not saved: {error}"
        ),
        uncertain: false,
    })?;
    store
        .commit(candidate)
        .map_err(|error| RouteUpdateFailure {
            message: format!("Sequence could not be saved: {}", store_error_text(&error)),
            uncertain: false,
        })?;

    match apply() {
        Ok(plugin_enabled) => Ok(plugin_enabled),
        Err(RouteActivationFailure::Confirmed(error)) => {
            store
                .rollback_if_revision(candidate.revision(), previous)
                .map_err(|rollback| RouteUpdateFailure {
                    message: format!(
                        "Shortcut activation failed ({error}); rollback could not be completed: {}",
                        store_error_text(&rollback)
                    ),
                    uncertain: true,
                })?;
            Err(RouteUpdateFailure {
                message: format!(
                    "Shortcut activation failed; the requested binding change was rolled back: {error}"
                ),
                uncertain: false,
            })
        }
        Err(RouteActivationFailure::Uncertain(error)) => Err(RouteUpdateFailure {
            message: format!(
                "Shortcut activation could not be confirmed; the saved sequence was retained: {error}"
            ),
            uncertain: true,
        }),
    }
}

fn reconcile_editor_update(
    snapshot_slot: &mut Option<SequenceSnapshot>,
    draft: &mut SequenceDraft,
    selected_id: &mut Option<SequenceId>,
    selected_step: &mut Option<usize>,
    hotkey_text: &mut String,
    snapshot: SequenceSnapshot,
    update: &SequenceEditorUpdate,
    content_committed: bool,
) {
    // A failed write must retain its original revision while its sequence
    // still exists. Otherwise the old draft is silently rebased on retry.
    let should_apply_update = match update {
        SequenceEditorUpdate::Saved { .. } => content_committed,
        SequenceEditorUpdate::Removed { id } => snapshot.sequence(*id).is_none(),
    };
    if !should_apply_update {
        return;
    }
    if should_apply_update {
        match update {
            SequenceEditorUpdate::Saved { id } => {
                if let Some(saved) = snapshot.sequence(*id) {
                    *draft = SequenceDraft::from_sequence(saved);
                    *selected_id = Some(*id);
                    *selected_step = selected_step.filter(|index| *index < saved.steps.len());
                    *hotkey_text = saved
                        .hotkey
                        .as_ref()
                        .map(|chord| chord.as_str().to_owned())
                        .unwrap_or_default();
                } else {
                    *draft = SequenceDraft::new("New sequence", Vec::new());
                    *selected_id = None;
                    *selected_step = None;
                    hotkey_text.clear();
                }
            }
            SequenceEditorUpdate::Removed { .. } => {
                *draft = SequenceDraft::new("New sequence", Vec::new());
                *selected_id = None;
                *selected_step = None;
                hotkey_text.clear();
            }
        }
    }
    *snapshot_slot = Some(snapshot);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hotkey_directory_paste_worker_notifies_revision_after_ui_receiver_closes() {
        use std::sync::{
            Arc,
            atomic::{AtomicU64, Ordering},
            mpsc::channel,
        };

        let directory = tempfile::tempdir().unwrap();
        let store = SequenceStore::new(directory.path().join(PASTE_SEQUENCES_FILE));
        let mut candidate = store.load().unwrap();
        candidate
            .upsert(SequenceDraft::new(
                "Worker-owned commit",
                vec![Step::LiteralText {
                    text: "committed content".into(),
                }],
            ))
            .unwrap();
        store.commit(&mut candidate).unwrap();
        let committed_revision = candidate.revision();

        let revision = Arc::new(AtomicU64::new(0));
        let (main_wake, main_wake_rx) = channel();
        let notifier = super::super::PasteSequenceDocumentRevisionNotifier::new(
            Arc::clone(&revision),
            Some(main_wake),
        );
        let (completion_tx, completion_rx) = channel();
        drop(completion_rx);

        let worker = std::thread::spawn(move || {
            let snapshot = load_sequence_snapshot_and_notify(&store, &notifier);
            let _ = completion_tx.send(snapshot.is_some());
            snapshot
        });
        let loaded = worker.join().unwrap().unwrap();

        assert_eq!(loaded.revision(), committed_revision);
        assert_eq!(revision.load(Ordering::Acquire), committed_revision);
        assert!(main_wake_rx.try_recv().is_ok());
    }

    fn layout_app(ctx: &egui::Context, root: &std::path::Path) -> super::super::LauncherApp {
        use std::sync::{Arc, atomic::AtomicBool};
        let mut settings = crate::settings::Settings::default();
        settings.hotkey = None;
        settings.quit_hotkey = None;
        settings.help_hotkey = None;
        settings.dashboard.enabled = false;
        settings.enable_toasts = false;
        super::super::LauncherApp::new(
            ctx,
            Arc::new(Vec::new()),
            0,
            crate::plugin::PluginManager::new_inert_for_test(),
            root.join("actions.json").to_string_lossy().into_owned(),
            root.join("settings.json").to_string_lossy().into_owned(),
            settings,
            None,
            None,
            None,
            Some(std::collections::HashMap::new()),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        )
    }

    fn layout_frame(
        dialog: &mut PasteSequenceDialog,
        ctx: &egui::Context,
        app: &mut super::super::LauncherApp,
        size: egui::Vec2,
    ) {
        ctx.begin_pass(egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            ..Default::default()
        });
        dialog.ui(ctx, app);
        ctx.end_pass().drop_without_applying_deltas();
    }

    fn owned_editor_context() -> SequenceInvocationContext {
        SequenceInvocationContext::new(
            chrono::DateTime::parse_from_rfc3339("2024-01-02T03:04:05+00:00").unwrap(),
            None,
            None,
        )
    }

    #[test]
    fn saved_run_presentation_selects_fresh_content_and_preserves_open_or_suspended_drafts() {
        use crate::paste_sequence::{PlaybackMode, Sequence, SequenceId, Step};

        let directory = tempfile::tempdir().unwrap();
        let mut dialog = PasteSequenceDialog::default();
        dialog.use_test_data_sources(directory.path().join(PASTE_SEQUENCES_FILE));
        let first = Sequence {
            id: SequenceId::from_raw(11).unwrap(),
            name: "Grid sequence".into(),
            hotkey: None,
            mode: PlaybackMode::PasteToAdvance,
            delay_ms: 0,
            steps: vec![Step::LiteralText {
                text: "saved step".into(),
            }],
        };

        dialog.present_saved_run(&first, owned_editor_context());

        assert!(dialog.open);
        assert!(!dialog.suspended_for_playback);
        assert_eq!(dialog.selected_id, Some(first.id));
        assert_eq!(dialog.draft.id, Some(first.id));
        assert_eq!(dialog.draft.name, first.name);
        assert_eq!(dialog.draft.steps, first.steps);
        assert_eq!(dialog.saved_run.as_ref().map(|run| run.id), Some(first.id));

        let unsaved = SequenceDraft::new(
            "Unsaved draft",
            vec![
                Step::LiteralText {
                    text: "draft one".into(),
                },
                Step::LiteralText {
                    text: "draft two".into(),
                },
            ],
        );
        dialog.draft = unsaved.clone();
        dialog.selected_id = None;
        dialog.selected_step = Some(1);
        let second = Sequence {
            id: SequenceId::from_raw(12).unwrap(),
            name: "Other grid sequence".into(),
            hotkey: None,
            mode: PlaybackMode::Automatic,
            delay_ms: 0,
            steps: vec![Step::LiteralText {
                text: "other saved step".into(),
            }],
        };

        dialog.present_saved_run(&second, owned_editor_context());

        assert_eq!(dialog.draft, unsaved);
        assert_eq!(dialog.selected_id, None);
        assert_eq!(dialog.selected_step, Some(1));
        assert_eq!(dialog.saved_run.as_ref().map(|run| run.id), Some(second.id));
        assert_eq!(
            saved_run_status_text(dialog.saved_run.as_ref(), "Waiting for Ctrl+V".into()),
            "Saved sequence ‘Other grid sequence’ (ID 12) · Waiting for Ctrl+V"
        );

        dialog.suspend_for_playback();
        assert!(!dialog.open);
        assert!(dialog.suspended_for_playback);
        dialog.present_saved_run(&first, owned_editor_context());
        assert!(dialog.open);
        assert!(!dialog.suspended_for_playback);
        assert_eq!(dialog.draft, unsaved);
        assert_eq!(dialog.saved_run.as_ref().map(|run| run.id), Some(first.id));
    }

    #[test]
    fn paste_sequence_run_and_run_once_respect_visibility_preference_and_restart_all_modes() {
        use crate::paste_sequence::{
            SequencePlaybackStatus as Status, admission::physical_paste_bridge,
        };
        use std::sync::atomic::Ordering;
        let _guard = crate::gui::TEST_MUTEX.lock().unwrap();
        let bridge = physical_paste_bridge();
        let (wake, _rx) = std::sync::mpsc::channel();
        bridge.install_wake(wake);
        bridge.allow_test_routes();
        bridge.observer_live(true);
        for keep_visible in [false, true] {
            for run_once in [false, true] {
                for mode in [
                    PlaybackMode::PasteToAdvance,
                    PlaybackMode::Automatic,
                    PlaybackMode::ManualAdvance,
                ] {
                    let directory = tempfile::tempdir().unwrap();
                    let mut app =
                        super::super::command_host::tests::inert_test_app(directory.path());
                    app.enabled_plugins = None;
                    app.settings_editor
                        .paste_sequence
                        .keep_dialog_visible_during_playback = !keep_visible;
                    app.sequence_routes = Some(crate::paste_sequence::SequenceRoutePresentation {
                        generation: 1,
                        physical_paste: Default::default(),
                        next: crate::paste_sequence::SequenceShortcutPresentation::Disabled,
                        cancel: crate::paste_sequence::SequenceShortcutPresentation::Disabled,
                        starts: Vec::new(),
                        escape_cancel_enabled: true,
                        keep_dialog_visible_during_playback: keep_visible,
                    });
                    app.visible_flag.store(true, Ordering::SeqCst);
                    app.panel_states.paste_sequence_dialog = true;
                    app.panel_stack
                        .push(super::super::Panel::PasteSequenceDialog);
                    app.paste_sequence_dialog = PasteSequenceDialog {
                        open: true,
                        sources_are_test_owned: true,
                        store_path: directory.path().join(PASTE_SEQUENCES_FILE),
                        context: Some(owned_editor_context()),
                        draft: SequenceDraft::new(
                            "Unsaved text",
                            vec![
                                Step::LiteralText {
                                    text: "first".into(),
                                },
                                Step::LiteralText {
                                    text: "second".into(),
                                },
                            ],
                        ),
                        selected_step: Some(1),
                        ..Default::default()
                    };
                    app.paste_sequence_dialog.draft.mode = mode;
                    let target = crate::paste_sequence::SequenceTarget::from_external_window(
                        crate::active_window::ExternalForegroundWindow {
                            hwnd: 91,
                            process_id: 92,
                            thread_id: 93,
                            title: None,
                        },
                        std::process::id(),
                    )
                    .unwrap();
                    let mut platform =
                        super::super::command_host::tests::AppPastePlatform::new(target);
                    // Use the render path's taken-dialog ownership and existing platform fake.
                    let mut dialog = std::mem::take(&mut app.paste_sequence_dialog);
                    dialog.prepare_current_using(&mut app, true, run_once, |app| {
                        app.start_sequence_playback_using(&mut platform)
                    });
                    assert!(dialog.inline_error.is_none(), "{:?}", dialog.inline_error);
                    assert_eq!(dialog.open, keep_visible);
                    assert_eq!(dialog.suspended_for_playback, !keep_visible);
                    assert_eq!(app.visible_flag.load(Ordering::SeqCst), keep_visible);
                    assert_eq!(app.panel_states.paste_sequence_dialog, keep_visible);
                    assert!(
                        app.panel_stack
                            .contains(&super::super::Panel::PasteSequenceDialog)
                            == keep_visible
                    );
                    assert_eq!(
                        app.sequence_playback.status(),
                        match mode {
                            PlaybackMode::PasteToAdvance => Status::AwaitPaste,
                            PlaybackMode::Automatic => Status::Running,
                            PlaybackMode::ManualAdvance => Status::AwaitAdvance,
                        }
                    );
                    assert!(app.sequence_playback.started_run().is_some());
                    let current_step = match mode {
                        PlaybackMode::PasteToAdvance => 1,
                        PlaybackMode::Automatic | PlaybackMode::ManualAdvance => 2,
                    };
                    assert_eq!(
                        app.sequence_playback.current_step_number(),
                        Some((current_step, 2))
                    );
                    assert!(!dialog.store_path.exists());
                    app.paste_sequence_dialog = dialog;
                    if !keep_visible {
                        app.paste_sequence_dialog
                            .open_with_context(Ok(owned_editor_context()));
                    }
                    assert_eq!(app.paste_sequence_dialog.draft.name, "Unsaved text");
                    assert_eq!(app.paste_sequence_dialog.draft.steps.len(), 2);
                    if keep_visible {
                        assert_eq!(app.paste_sequence_dialog.selected_step, Some(1));
                        assert_eq!(
                            app.paste_sequence_dialog.status.as_deref(),
                            Some("Sequence started.")
                        );
                    }
                    assert!(!app.paste_sequence_dialog.take_autostart_when_ready());
                    if keep_visible && mode == PlaybackMode::Automatic {
                        let completion_time =
                            std::time::Instant::now() + std::time::Duration::from_millis(500);
                        let _ = app.sequence_playback.tick(&mut platform, completion_time);
                        assert_eq!(app.sequence_playback.status(), Status::Completed);
                        let _ = app.sequence_playback.tick(&mut platform, completion_time);
                        assert!(app.paste_sequence_dialog.open);
                        assert!(app.visible_flag.load(Ordering::SeqCst));
                        assert!(app.panel_states.paste_sequence_dialog);
                        assert_eq!(
                            app.paste_sequence_dialog.status.as_deref(),
                            Some("Sequence started.")
                        );
                    } else {
                        app.cancel_sequence_playback();
                        let cleanup_time =
                            std::time::Instant::now() + std::time::Duration::from_millis(500);
                        let _ = app.sequence_playback.tick(&mut platform, cleanup_time);
                        assert!(!app.sequence_playback.needs_tick());
                    }
                    if keep_visible {
                        let mut dialog = std::mem::take(&mut app.paste_sequence_dialog);
                        dialog.prepare_current_using(&mut app, true, run_once, |app| {
                            app.start_sequence_playback_using(&mut platform)
                        });
                        assert!(dialog.inline_error.is_none(), "{:?}", dialog.inline_error);
                        assert!(dialog.open);
                        assert_eq!(
                            app.sequence_playback.status(),
                            match mode {
                                PlaybackMode::PasteToAdvance => Status::AwaitPaste,
                                PlaybackMode::Automatic => Status::Running,
                                PlaybackMode::ManualAdvance => Status::AwaitAdvance,
                            }
                        );
                        assert!(app.visible_flag.load(Ordering::SeqCst));
                        assert!(app.panel_states.paste_sequence_dialog);
                        app.paste_sequence_dialog = dialog;
                        app.cancel_sequence_playback();
                    }
                }
            }
        }
        bridge.observer_live(false);
    }

    #[test]
    fn paste_sequence_editor_third_mode_preserves_delay_and_run_once_stays_prompted_memory_only() {
        use crate::paste_sequence::SequencePlaybackStatus;
        for size in [egui::vec2(900.0, 760.0), egui::vec2(600.0, 420.0)] {
            let directory = tempfile::tempdir().unwrap();
            let ctx = egui::Context::default();
            let mut app = layout_app(&ctx, directory.path());
            let store_path = directory.path().join(PASTE_SEQUENCES_FILE);
            let original = b"owned unchanged saved bytes";
            std::fs::write(&store_path, original).unwrap();
            let inventory = || {
                let mut entries: Vec<_> = std::fs::read_dir(directory.path())
                    .unwrap()
                    .map(|entry| entry.unwrap().file_name())
                    .collect();
                entries.sort();
                entries
            };
            let original_inventory = inventory();
            let mut dialog = PasteSequenceDialog {
                open: true,
                store_path,
                sources_are_test_owned: true,
                context: Some(owned_editor_context()),
                draft: SequenceDraft::new(
                    "Unsaved third mode",
                    vec![Step::SnippetRef {
                        alias: "owned_prompt".into(),
                    }],
                ),
                snippets: vec![SnippetEntry {
                    alias: "owned_prompt".into(),
                    text: "SECRET {{required}}".into(),
                    hide_contents: true,
                    prompt_for_fields: true,
                    fields: vec![crate::plugins::snippets::SnippetFieldDefinition::new(
                        "required",
                    )],
                    tags: vec![],
                }],
                ..Default::default()
            };
            dialog.draft.delay_ms = 1_234;
            for _ in 0..3 {
                layout_frame(&mut dialog, &ctx, &mut app, size);
            }
            for mode in [
                PlaybackMode::PasteToAdvance,
                PlaybackMode::Automatic,
                PlaybackMode::ManualAdvance,
                PlaybackMode::PasteToAdvance,
            ] {
                let label = playback_mode_label(mode);
                for _ in 0..12 {
                    let visible = editor_event_frame(&mut dialog, &ctx, &mut app, size, vec![]);
                    if visible.iter().any(|(text, _)| text == label) {
                        break;
                    }
                    let id = dialog.layout.body_id.unwrap();
                    let mut state = egui::scroll_area::State::load(&ctx, id).unwrap();
                    state.offset.y += 35.0;
                    state.store(&ctx, id);
                }
                editor_click(&mut dialog, &ctx, &mut app, size, label);
                assert_eq!(dialog.draft.mode, mode);
                assert_eq!(
                    dialog.draft.delay_ms, 1_234,
                    "switches preserve stored delay"
                );
                assert!(dialog.layout.footer.bottom() <= size.y + 1.0);
            }
            editor_click(&mut dialog, &ctx, &mut app, size, "Run Once");
            assert_eq!(
                app.sequence_playback.status(),
                SequencePlaybackStatus::Preparing
            );
            assert!(app.snippet_prompt_dialog.is_sequence_preflight());
            assert!(app.sequence_playback.started_run().is_none());
            assert!(dialog.status.as_deref().unwrap().contains("memory-only"));
            assert_eq!(dialog.current_draft().mode, PlaybackMode::PasteToAdvance);
            assert_eq!(std::fs::read(&dialog.store_path).unwrap(), original);
            assert_eq!(inventory(), original_inventory);
            app.cancel_active_paste_sequence();
        }
    }

    #[test]
    fn paste_sequence_editor_preview_masks_hidden_text_and_distinguishes_context_from_target() {
        let directory = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        let mut app = layout_app(&ctx, directory.path());
        let size = egui::vec2(900.0, 760.0);
        let mut dialog = PasteSequenceDialog {
            open: true,
            sources_are_test_owned: true,
            context: Some(owned_editor_context()),
            draft: SequenceDraft::new(
                "Safe preview",
                vec![Step::SnippetRef {
                    alias: "hidden".into(),
                }],
            ),
            snippets: vec![SnippetEntry {
                alias: "hidden".into(),
                text: "DO NOT DISCLOSE".into(),
                hide_contents: true,
                prompt_for_fields: false,
                fields: vec![],
                tags: vec![],
            }],
            ..Default::default()
        };
        dialog.draft.mode = PlaybackMode::PasteToAdvance;
        for _ in 0..3 {
            layout_frame(&mut dialog, &ctx, &mut app, size);
        }
        let visible = editor_event_frame(&mut dialog, &ctx, &mut app, size, vec![]);
        assert!(
            visible
                .iter()
                .any(|(text, _)| text.contains("No origin needed"))
        );
        editor_click(&mut dialog, &ctx, &mut app, size, "Preview");
        let preview = app.sequence_playback.prepared_preview().unwrap();
        assert!(preview.ready && preview.steps[0].hidden && preview.steps[0].text.is_none());
        let id = dialog.layout.body_id.unwrap();
        let mut state = egui::scroll_area::State::load(&ctx, id).unwrap();
        state.offset.y = (dialog.layout.body_content.y - dialog.layout.body.height()).max(0.0);
        state.store(&ctx, id);
        for _ in 0..3 {
            layout_frame(&mut dialog, &ctx, &mut app, size);
        }
        let visible = editor_event_frame(&mut dialog, &ctx, &mut app, size, vec![]);
        assert!(
            visible
                .iter()
                .any(|(text, _)| text.contains("Paste target: Current eligible"))
        );
        assert!(
            visible
                .iter()
                .any(|(text, _)| text.contains("Template values stay frozen"))
        );
        assert!(
            visible
                .iter()
                .any(|(text, _)| text == "Hidden snippet contents")
        );
        assert!(
            !visible
                .iter()
                .any(|(text, _)| text.contains("DO NOT DISCLOSE"))
        );
        assert_eq!(
            app.sequence_playback.status(),
            crate::paste_sequence::SequencePlaybackStatus::Ready
        );
        assert!(app.sequence_playback.started_run().is_none());
    }

    #[test]
    fn paste_sequence_editor_status_has_live_counts_and_clear_terminal_failure_feedback() {
        use crate::paste_sequence::{SequencePlaybackError, SequencePlaybackStatus as Status};
        for status in [
            Status::Idle,
            Status::Preparing,
            Status::Ready,
            Status::Completed,
            Status::Cancelled,
            Status::Failed(SequencePlaybackError::ClipboardUnavailable),
        ] {
            let text = run_status_text(status, Some((2, 3)));
            assert!(!text.contains("step 2"), "terminal/preview state: {text}");
        }
        assert!(
            run_status_text(Status::AwaitPaste, Some((2, 3)))
                .contains("Waiting for Ctrl+V in an eligible application · step 2 of 3")
        );
        assert!(
            run_status_text(
                Status::Failed(SequencePlaybackError::ClipboardUnavailable),
                None
            )
            .contains("The clipboard could not be opened")
        );
    }

    #[test]
    fn paste_settings_toolbar_opens_main_section_without_losing_an_open_draft() {
        let directory = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        let mut app = layout_app(&ctx, directory.path());
        app.show_settings = true;
        app.settings_editor.paste_sequence.next_hotkey = Some("Ctrl+F9".into());
        let mut dialog = PasteSequenceDialog::default();
        dialog.open = true;
        for _ in 0..3 {
            layout_frame(&mut dialog, &ctx, &mut app, egui::vec2(900.0, 760.0));
        }
        editor_click(
            &mut dialog,
            &ctx,
            &mut app,
            egui::vec2(900.0, 760.0),
            "Playback Settings",
        );
        assert!(app.show_settings);
        assert_eq!(
            app.settings_editor
                .pending_settings_navigation
                .as_ref()
                .map(|pending| pending.control_id.as_str()),
            Some("paste.next.enabled")
        );
        assert_eq!(
            app.settings_editor.paste_sequence.next_hotkey.as_deref(),
            Some("Ctrl+F9")
        );
        assert!(dialog.open);
    }

    #[test]
    fn ordered_steps_layout_bounds_actual_dialog_and_scroll_range() {
        for size in [egui::vec2(900.0, 760.0), egui::vec2(600.0, 420.0)] {
            for count in [1, 50, 100] {
                let ctx = egui::Context::default();
                let directory = tempfile::tempdir().unwrap();
                let mut app = layout_app(&ctx, directory.path());
                let mut dialog = PasteSequenceDialog {
                    open: true,
                    ..Default::default()
                };
                let mut saved = SequenceStore::new(directory.path().join(PASTE_SEQUENCES_FILE))
                    .load()
                    .unwrap();
                for index in 0..12 {
                    saved
                        .upsert(SequenceDraft::new(
                            format!("Saved {index} {}", "long name ".repeat(7)),
                            vec![Step::LiteralText {
                                text: "saved".into(),
                            }],
                        ))
                        .unwrap();
                }
                dialog.snapshot = Some(saved);
                dialog.draft.steps = (0..count)
                    .map(|index| {
                        if index % 2 == 0 {
                            Step::LiteralText {
                                text: format!("{}\n", "long temporary text ".repeat(30)).repeat(20),
                            }
                        } else {
                            Step::SnippetRef {
                                alias: "long_alias".repeat(80),
                            }
                        }
                    })
                    .collect();
                dialog.selected_step = Some(count - 1);
                let original = dialog.draft.steps.clone();
                // Include the first measurement pass and settled frames: parent growth must
                // never be required before the independent list starts clipping content.
                for _ in 0..4 {
                    layout_frame(&mut dialog, &ctx, &mut app, size);
                    let layout = dialog.layout;
                    assert!(
                        layout.window.height() <= size.y + 1.0,
                        "{count} {size:?}: {layout:?}"
                    );
                    assert!(layout.window.width() <= size.x + 1.0, "{layout:?}");
                    assert!(layout.steps.height() <= 261.0, "{layout:?}");
                    assert!(
                        layout.steps.width() <= layout.body.width() + 1.0,
                        "{layout:?}"
                    );
                    assert!(
                        layout.footer.bottom() <= layout.window.bottom(),
                        "{layout:?}"
                    );
                    assert!(layout.footer.bottom() <= size.y + 1.0, "{layout:?}");
                    assert!(layout.footer.top() >= layout.body.bottom(), "{layout:?}");
                }
                let layout = dialog.layout;
                assert!(
                    layout.steps_content.y > layout.steps.height(),
                    "multiline step must scroll: {layout:?}"
                );
                assert!(
                    layout.body_content.y > layout.body.height(),
                    "all controls remain reachable through body scroll: {layout:?}"
                );
                if size.y > 700.0 {
                    assert!(
                        layout.steps.height() >= 240.0,
                        "normal viewport: {layout:?}"
                    );
                } else {
                    assert!(
                        layout.steps.height() < 240.0,
                        "short viewport must shrink: {layout:?}"
                    );
                }
                let id = layout.steps_id.unwrap();
                let mut state = egui::scroll_area::State::load(&ctx, id).unwrap();
                state.offset.y = layout.steps_content.y - layout.steps.height() + 100.0;
                state.store(&ctx, id);
                layout_frame(&mut dialog, &ctx, &mut app, size);
                let scrolled = dialog.layout;
                let range = scrolled.steps_content.y - scrolled.steps.height();
                assert!(
                    (scrolled.steps_offset - range).abs() <= 1.0,
                    "last row must be reachable: {scrolled:?}"
                );
                assert_eq!(dialog.draft.steps, original);
                assert_eq!(dialog.selected_step, Some(count - 1));
            }
        }
    }

    // Text paint rectangles let this focused fixture drive the production controls
    // without a second implementation of their actions or a general widget harness.
    fn editor_event_frame(
        dialog: &mut PasteSequenceDialog,
        ctx: &egui::Context,
        app: &mut super::super::LauncherApp,
        size: egui::Vec2,
        events: Vec<egui::Event>,
    ) -> Vec<(String, egui::Rect)> {
        ctx.begin_pass(egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            events,
            ..Default::default()
        });
        dialog.ui(ctx, app);
        let output = ctx.end_pass();
        fn collect(
            shape: &egui::epaint::Shape,
            clip: egui::Rect,
            out: &mut Vec<(String, egui::Rect)>,
        ) {
            match shape {
                egui::epaint::Shape::Text(text) => {
                    let rect = text
                        .galley
                        .rect
                        .translate(text.pos.to_vec2())
                        .intersect(clip);
                    if rect.is_positive() {
                        out.push((text.galley.text().to_owned(), rect));
                    }
                }
                egui::epaint::Shape::Vec(shapes) => {
                    for shape in shapes {
                        collect(shape, clip, out);
                    }
                }
                _ => {}
            }
        }
        let mut text = Vec::new();
        for shape in &output.shapes {
            collect(&shape.shape, shape.clip_rect, &mut text);
        }
        output.drop_without_applying_deltas();
        text
    }

    fn editor_click(
        dialog: &mut PasteSequenceDialog,
        ctx: &egui::Context,
        app: &mut super::super::LauncherApp,
        size: egui::Vec2,
        label: &str,
    ) {
        let text = editor_event_frame(dialog, ctx, app, size, Vec::new());
        let rect = text
            .iter()
            .find(|(text, _)| text == label)
            .unwrap_or_else(|| panic!("missing visible {label:?}: {text:?}"))
            .1;
        let pos = rect.center();
        editor_event_frame(dialog, ctx, app, size, vec![egui::Event::PointerMoved(pos)]);
        editor_event_frame(
            dialog,
            ctx,
            app,
            size,
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        editor_event_frame(
            dialog,
            ctx,
            app,
            size,
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        for _ in 0..2 {
            editor_event_frame(dialog, ctx, app, size, Vec::new());
        }
    }

    fn editor_reveal_toolbar(
        dialog: &mut PasteSequenceDialog,
        ctx: &egui::Context,
        app: &mut super::super::LauncherApp,
        size: egui::Vec2,
    ) {
        let layout = dialog.layout;
        let mut state = egui::scroll_area::State::load(ctx, layout.body_id.unwrap()).unwrap();
        state.offset.y += layout.steps.top() - layout.body.center().y - 80.0;
        state.store(ctx, layout.body_id.unwrap());
        for _ in 0..2 {
            editor_event_frame(dialog, ctx, app, size, Vec::new());
        }
    }

    #[test]
    fn ordered_steps_editor_events_preserve_rows_scroll_and_lifecycle() {
        let ctx = egui::Context::default();
        let directory = tempfile::tempdir().unwrap();
        let mut app = layout_app(&ctx, directory.path());
        let size = egui::vec2(900.0, 760.0);
        let store_path = directory.path().join(PASTE_SEQUENCES_FILE);
        let store = SequenceStore::new(&store_path);
        let mut snapshot = store.load().unwrap();
        let saved = SequenceDraft::new(
            "Owned saved fixture",
            vec![Step::LiteralText {
                text: "persisted fixture".into(),
            }],
        );
        snapshot.upsert(saved.clone()).unwrap();
        store.commit(&mut snapshot).unwrap();
        let original_store = std::fs::read(&store_path).unwrap();
        let mut dialog = PasteSequenceDialog {
            open: true,
            store_path,
            snapshot: Some(snapshot),
            ..Default::default()
        };
        dialog.snippets = vec![SnippetEntry {
            alias: "fixture_snippet".into(),
            text: "dummy hidden payload".into(),
            hide_contents: true,
            prompt_for_fields: false,
            fields: Vec::new(),
            tags: Vec::new(),
        }];
        dialog.history = vec!["owned dummy history".into()];
        dialog.draft.steps = (0..50)
            .map(|index| Step::LiteralText {
                text: format!("fixture {index}"),
            })
            .collect();
        for _ in 0..4 {
            editor_event_frame(&mut dialog, &ctx, &mut app, size, Vec::new());
        }
        editor_click(&mut dialog, &ctx, &mut app, size, "1.");
        assert_eq!(dialog.selected_step, Some(0));
        editor_reveal_toolbar(&mut dialog, &ctx, &mut app, size);
        editor_click(&mut dialog, &ctx, &mut app, size, "Add Temporary Text");
        assert_eq!(dialog.selected_step, Some(50));
        assert!(dialog.reveal_step.is_none());
        let visible = editor_event_frame(&mut dialog, &ctx, &mut app, size, Vec::new());
        assert!(visible.iter().any(|(text, _)| text == "51."));
        editor_click(
            &mut dialog,
            &ctx,
            &mut app,
            size,
            "Text pasted when this step runs",
        );
        editor_event_frame(
            &mut dialog,
            &ctx,
            &mut app,
            size,
            vec![egui::Event::Text("edited owned text".into())],
        );
        assert_eq!(
            dialog.draft.steps[50],
            Step::LiteralText {
                text: "edited owned text".into()
            }
        );
        let focused = ctx.memory(|memory| memory.focused());
        assert!(focused.is_some());
        editor_event_frame(
            &mut dialog,
            &ctx,
            &mut app,
            size,
            vec![egui::Event::Key {
                key: egui::Key::Tab,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert_ne!(ctx.memory(|memory| memory.focused()), focused);
        for (button, selected) in [("Move Up", 49), ("Move Down", 50), ("Move Up", 49)] {
            editor_reveal_toolbar(&mut dialog, &ctx, &mut app, size);
            editor_click(&mut dialog, &ctx, &mut app, size, button);
            assert_eq!(dialog.selected_step, Some(selected));
            assert_eq!(
                dialog.draft.steps[selected],
                Step::LiteralText {
                    text: "edited owned text".into()
                }
            );
            let text = editor_event_frame(&mut dialog, &ctx, &mut app, size, Vec::new());
            assert!(text.iter().any(|(text, _)| text == "edited owned text"));
        }
        editor_reveal_toolbar(&mut dialog, &ctx, &mut app, size);
        editor_click(&mut dialog, &ctx, &mut app, size, "Remove");
        assert_eq!(dialog.draft.steps.len(), 50);
        assert_eq!(
            dialog.draft.steps[49],
            Step::LiteralText {
                text: "fixture 49".into()
            }
        );
        for (menu, item) in [
            ("Add Snippet", "fixture_snippet  \u{00b7}  hidden"),
            ("Add Clipboard Item", "History item 1"),
        ] {
            editor_reveal_toolbar(&mut dialog, &ctx, &mut app, size);
            editor_click(&mut dialog, &ctx, &mut app, size, menu);
            editor_click(&mut dialog, &ctx, &mut app, size, item);
        }
        assert_eq!(dialog.selected_step, Some(51));
        assert_eq!(
            dialog.draft.steps[50],
            Step::SnippetRef {
                alias: "fixture_snippet".into()
            }
        );
        assert_eq!(
            dialog.draft.steps[51],
            Step::clipboard_history_reference("owned dummy history")
        );
        editor_click(&mut dialog, &ctx, &mut app, size, "51.");
        assert_eq!(dialog.selected_step, Some(50));
        editor_reveal_toolbar(&mut dialog, &ctx, &mut app, size);
        editor_click(&mut dialog, &ctx, &mut app, size, "Remove");
        assert_eq!(
            dialog.draft.steps[50],
            Step::clipboard_history_reference("owned dummy history")
        );
        let expected_steps = dialog.draft.steps.clone();
        // Put both regions away from their upper boundaries before checking routing.
        let layout = dialog.layout;
        let mut state = egui::scroll_area::State::load(&ctx, layout.body_id.unwrap()).unwrap();
        state.offset.y = layout.body_content.y - layout.body.height();
        state.store(&ctx, layout.body_id.unwrap());
        for _ in 0..2 {
            editor_event_frame(&mut dialog, &ctx, &mut app, size, Vec::new());
        }
        // Scroll the real nested regions with pointer-positioned wheel events.
        for (inside_steps, outside_window) in [(true, false), (false, false), (false, true)] {
            let before = dialog.layout;
            let pos = if outside_window {
                egui::pos2(899.0, 759.0)
            } else if inside_steps {
                before.steps.intersect(before.body).center()
            } else {
                before.body.min + egui::vec2(5.0, 5.0)
            };
            editor_event_frame(
                &mut dialog,
                &ctx,
                &mut app,
                size,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: egui::vec2(0.0, 40.0),
                        phase: egui::TouchPhase::Move,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
            for _ in 0..30 {
                editor_event_frame(&mut dialog, &ctx, &mut app, size, Vec::new());
            }
            let after = dialog.layout;
            if inside_steps {
                assert!(
                    after.steps_offset < before.steps_offset,
                    "inner wheel: {before:?} -> {after:?}"
                );
                assert_eq!(after.body_offset, before.body_offset);
            } else {
                assert_eq!(after.steps_offset, before.steps_offset);
                if outside_window {
                    assert_eq!(after.body_offset, before.body_offset);
                } else {
                    assert!(
                        after.body_offset < before.body_offset,
                        "body wheel: {before:?} -> {after:?}"
                    );
                }
            }
        }
        for viewport in [egui::vec2(600.0, 420.0), size] {
            for _ in 0..4 {
                editor_event_frame(&mut dialog, &ctx, &mut app, viewport, Vec::new());
            }
            assert!(dialog.layout.footer.bottom() <= viewport.y + 1.0);
            assert_eq!(dialog.draft.steps, expected_steps);
        }
        assert_eq!(std::fs::read(&dialog.store_path).unwrap(), original_store);
        editor_click(&mut dialog, &ctx, &mut app, size, "Cancel Editor");
        assert!(!dialog.open);
        assert!(dialog.draft.steps.is_empty());
        assert_eq!(std::fs::read(&dialog.store_path).unwrap(), original_store);
        // Reopen with the owned catalog rather than reading real snippet/history files.
        dialog.open = true;
        dialog.snapshot = Some(store.load().unwrap());
        // This fixture reopens directly rather than using a fresh window. The
        // saved rows are above the prior short-viewport body scroll position.
        let id = dialog.layout.body_id.unwrap();
        let mut state = egui::scroll_area::State::load(&ctx, id).unwrap();
        state.offset.y = 0.0;
        state.store(&ctx, id);
        for _ in 0..3 {
            editor_event_frame(&mut dialog, &ctx, &mut app, size, Vec::new());
        }
        editor_click(
            &mut dialog,
            &ctx,
            &mut app,
            size,
            "Owned saved fixture  \u{00b7}  1 steps",
        );
        assert_eq!(dialog.draft.steps, saved.steps);
        assert_eq!(std::fs::read(&dialog.store_path).unwrap(), original_store);
    }

    fn hotkey_draft(name: &str) -> SequenceDraft {
        let mut draft = SequenceDraft::new(
            name,
            vec![Step::LiteralText {
                text: "authored text".into(),
            }],
        );
        draft.hotkey = Some(HotkeyChord::new("Ctrl+Shift+K").unwrap());
        draft
    }

    #[test]
    fn paste_sequence_content_only_save_ignores_conflicting_defaults_and_preserves_binding() {
        for binding in [None, Some(HotkeyChord::new("Ctrl+F9").unwrap())] {
            let directory = tempfile::tempdir().unwrap();
            let store = SequenceStore::new(directory.path().join(PASTE_SEQUENCES_FILE));
            let mut settings = crate::settings::Settings::default();
            settings.hotkey = Some("Shift+Alt+Win+End".into());
            let settings_path = directory.path().join("settings.json");
            settings.save(settings_path.to_str().unwrap()).unwrap();
            let settings_bytes = std::fs::read(&settings_path).unwrap();
            let mut previous = store.load().unwrap();
            let mut draft = hotkey_draft("Saved");
            draft.hotkey = binding.clone();
            let id = previous.upsert(draft).unwrap();
            store.commit(&mut previous).unwrap();
            let mut candidate = previous.clone();
            let mut edited = SequenceDraft::from_sequence(previous.sequence(id).unwrap());
            edited.name = "Content changed".into();
            edited.mode = PlaybackMode::ManualAdvance;
            edited.delay_ms = 123;
            edited.steps = vec![Step::LiteralText {
                text: "New Unicode 🦀\ncontent".into(),
            }];
            candidate.upsert(edited).unwrap();
            let result = commit_sequence_editor_update(
                &store,
                &previous,
                &mut candidate,
                &SequenceEditorUpdate::Saved { id },
                StartShortcutUpdate::Set(binding.clone()),
                |_| panic!("content-only save must not prepare global routes"),
                || panic!("content-only save must not apply global routes"),
            )
            .unwrap();
            assert!(matches!(result, SequenceCommitOutcome::ContentOnly));
            let saved = store.load().unwrap();
            assert_eq!(saved.sequence(id).unwrap().name, "Content changed");
            assert_eq!(saved.sequence(id).unwrap().hotkey, binding);
            assert_eq!(
                saved.sequence(id).unwrap().mode,
                PlaybackMode::ManualAdvance
            );
            assert_eq!(saved.sequence(id).unwrap().delay_ms, 123);
            assert_eq!(std::fs::read(&settings_path).unwrap(), settings_bytes);
        }
    }

    #[test]
    fn paste_sequence_rejected_start_keeps_authored_content_and_reconciles_actual_binding() {
        let directory = tempfile::tempdir().unwrap();
        let store = SequenceStore::new(directory.path().join(PASTE_SEQUENCES_FILE));
        let mut previous = store.load().unwrap();
        let id = previous.upsert(hotkey_draft("Original")).unwrap();
        store.commit(&mut previous).unwrap();
        let mut candidate = previous.clone();
        let mut edited = SequenceDraft::from_sequence(previous.sequence(id).unwrap());
        edited.name = "Authored changes".into();
        candidate.upsert(edited.clone()).unwrap();
        let result = commit_sequence_editor_update(
            &store,
            &previous,
            &mut candidate,
            &SequenceEditorUpdate::Saved { id },
            StartShortcutUpdate::Set(Some(HotkeyChord::new("Ctrl+F9").unwrap())),
            |_| Err("start conflicts with launcher".into()),
            || panic!("rejected start must not apply"),
        )
        .unwrap();
        assert!(matches!(
            result,
            SequenceCommitOutcome::ContentSavedBindingRejected {
                routes_uncertain: false,
                ..
            }
        ));
        let actual = store.load().unwrap();
        assert_eq!(actual.sequence(id).unwrap().name, "Authored changes");
        assert_eq!(
            actual.sequence(id).unwrap().hotkey,
            previous.sequence(id).unwrap().hotkey
        );
        edited.hotkey = Some(HotkeyChord::new("Ctrl+F9").unwrap());
        let mut slot = Some(previous);
        let mut selected = Some(id);
        let mut step = Some(99);
        let mut hotkey = "Ctrl+F9".into();
        reconcile_editor_update(
            &mut slot,
            &mut edited,
            &mut selected,
            &mut step,
            &mut hotkey,
            actual,
            &SequenceEditorUpdate::Saved { id },
            true,
        );
        assert_eq!(hotkey, "Ctrl+Shift+K");
        assert_eq!(edited.hotkey.as_ref().unwrap().as_str(), "Ctrl+Shift+K");
        assert_eq!(edited.name, "Authored changes");
        assert!(step.is_none());
    }

    #[test]
    fn paste_sequence_binding_rollback_retains_content_and_uncertainty_is_explicit() {
        for uncertain in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let store = SequenceStore::new(directory.path().join(PASTE_SEQUENCES_FILE));
            let previous = store.load().unwrap();
            let mut candidate = previous.clone();
            let mut draft = hotkey_draft("New authored sequence");
            draft.hotkey = None;
            let id = candidate.upsert(draft).unwrap();
            let result = commit_sequence_editor_update(
                &store,
                &previous,
                &mut candidate,
                &SequenceEditorUpdate::Saved { id },
                StartShortcutUpdate::Set(Some(HotkeyChord::new("Ctrl+F9").unwrap())),
                |_| Ok(()),
                || {
                    if uncertain {
                        Err(RouteActivationFailure::Uncertain(
                            "route status unavailable".into(),
                        ))
                    } else {
                        Err(RouteActivationFailure::Confirmed("route rejected".into()))
                    }
                },
            )
            .unwrap();
            assert!(
                matches!(result, SequenceCommitOutcome::ContentSavedBindingRejected { routes_uncertain, .. } if routes_uncertain == uncertain)
            );
            let mut saved = store.load().unwrap();
            let sequence = saved.sequence(id).unwrap();
            assert_eq!(sequence.name, "New authored sequence");
            assert_eq!(sequence.hotkey.is_some(), uncertain);
            let next_id = saved.upsert(hotkey_draft("Next")).unwrap();
            assert!(next_id.get() > id.get());
        }
    }

    #[test]
    fn paste_sequence_stale_content_and_binding_updates_never_overwrite_concurrent_writes() {
        let directory = tempfile::tempdir().unwrap();
        let store = SequenceStore::new(directory.path().join(PASTE_SEQUENCES_FILE));
        let mut previous = store.load().unwrap();
        let id = previous.upsert(hotkey_draft("Original")).unwrap();
        store.commit(&mut previous).unwrap();
        let mut candidate = previous.clone();
        let mut draft = SequenceDraft::from_sequence(previous.sequence(id).unwrap());
        draft.name = "Authored content".into();
        candidate.upsert(draft).unwrap();
        let result = commit_sequence_editor_update(
            &store,
            &previous,
            &mut candidate,
            &SequenceEditorUpdate::Saved { id },
            StartShortcutUpdate::Set(None),
            |_| {
                let mut concurrent = store.load().unwrap();
                let mut draft = SequenceDraft::from_sequence(concurrent.sequence(id).unwrap());
                draft.name = "Concurrent content".into();
                concurrent.upsert(draft).unwrap();
                store.commit(&mut concurrent).unwrap();
                Ok(())
            },
            || panic!("stale binding candidate must not apply"),
        )
        .unwrap();
        assert!(matches!(
            result,
            SequenceCommitOutcome::ContentSavedBindingRejected { .. }
        ));
        let bytes = std::fs::read(store.path()).unwrap();
        assert_eq!(
            store.load().unwrap().sequence(id).unwrap().name,
            "Concurrent content"
        );
        let error = commit_sequence_editor_update(
            &store,
            &previous,
            &mut candidate,
            &SequenceEditorUpdate::Saved { id },
            StartShortcutUpdate::Keep,
            |_| panic!("stale content must not prepare"),
            || panic!("stale content must not apply"),
        )
        .unwrap_err();
        assert!(error.contains("changed elsewhere"));
        assert_eq!(std::fs::read(store.path()).unwrap(), bytes);
        let mut slot = Some(previous.clone());
        let mut attempted = SequenceDraft::from_sequence(previous.sequence(id).unwrap());
        attempted.name = "Unsaved stale draft".into();
        let mut selected = Some(id);
        let mut step = Some(0);
        let mut hotkey = "Ctrl+Shift+K".into();
        reconcile_editor_update(
            &mut slot,
            &mut attempted,
            &mut selected,
            &mut step,
            &mut hotkey,
            store.load().unwrap(),
            &SequenceEditorUpdate::Saved { id },
            false,
        );
        assert_eq!(slot.as_ref().unwrap().revision(), previous.revision());
        assert_eq!(attempted.name, "Unsaved stale draft");
        reconcile_editor_update(
            &mut slot,
            &mut attempted,
            &mut selected,
            &mut step,
            &mut hotkey,
            store.load().unwrap(),
            &SequenceEditorUpdate::Removed { id },
            false,
        );
        assert_eq!(slot.as_ref().unwrap().revision(), previous.revision());
        assert_eq!(attempted.name, "Unsaved stale draft");
        let mut retry = slot.as_ref().unwrap().clone();
        retry.upsert(attempted).unwrap();
        assert!(
            commit_sequence_editor_update(
                &store,
                slot.as_ref().unwrap(),
                &mut retry,
                &SequenceEditorUpdate::Saved { id },
                StartShortcutUpdate::Keep,
                |_| panic!("retry must still be stale"),
                || panic!("retry must not apply")
            )
            .is_err()
        );
        assert_eq!(std::fs::read(store.path()).unwrap(), bytes);
    }

    #[test]
    fn paste_sequence_gui_invalid_start_saves_content_and_unbound_remove_needs_no_routes() {
        let directory = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        let mut app = layout_app(&ctx, directory.path());
        // Neither operation should load settings or contact a hotkey owner.
        std::fs::write(
            directory.path().join("settings.json"),
            "invalid settings fixture",
        )
        .unwrap();
        let store_path = directory.path().join(PASTE_SEQUENCES_FILE);
        let store = SequenceStore::new(&store_path);
        let mut dialog = PasteSequenceDialog {
            open: true,
            store_path,
            snapshot: Some(store.load().unwrap()),
            draft: hotkey_draft("Authored content"),
            hotkey_text: "Ctrl+invalid".into(),
            ..Default::default()
        };
        dialog.save_draft(&ctx, &mut app);
        let drain = |dialog: &mut PasteSequenceDialog| {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
            while dialog.hotkey_update.is_some() && std::time::Instant::now() < deadline {
                dialog.poll_hotkey_update(&ctx);
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            assert!(dialog.hotkey_update.is_none(), "worker did not complete");
        };
        drain(&mut dialog);
        assert!(dialog.status.as_deref().unwrap().contains("content saved"));
        assert!(dialog.inline_error.as_deref().unwrap().contains("invalid"));
        assert!(dialog.hotkey_text.is_empty());
        let id = dialog.selected_id.unwrap();
        assert_eq!(
            store.load().unwrap().sequence(id).unwrap().name,
            "Authored content"
        );
        dialog.remove_saved_sequence(&ctx, &mut app, id);
        drain(&mut dialog);
        assert!(store.load().unwrap().sequences().is_empty());
        assert!(dialog.selected_id.is_none());
        assert!(dialog.inline_error.is_none());
        assert!(dialog.status.as_deref().unwrap().contains("removed"));
        for plugin_enabled in [true, false] {
            let status =
                binding_update_status(&SequenceEditorUpdate::Removed { id }, plugin_enabled);
            assert!(status.contains("start shortcut was removed"));
            assert!(!status.contains("start shortcut routes are active"));
        }
    }

    #[test]
    fn paste_sequence_prepare_conflict_does_not_write_candidate() {
        let directory = tempfile::tempdir().unwrap();
        let store = SequenceStore::new(directory.path().join(PASTE_SEQUENCES_FILE));
        let previous = store.load().unwrap();
        let mut candidate = previous.clone();
        candidate.upsert(hotkey_draft("Candidate")).unwrap();

        let result = commit_sequence_candidate_with_route_update(
            &store,
            &previous,
            &mut candidate,
            |_| Err("hotkey conflicts with launcher".into()),
            || panic!("apply must not be reached after rejected preparation"),
        );

        assert!(result.unwrap_err().message.contains("was not saved"));
        let on_disk = store.load().unwrap();
        assert!(on_disk.sequences().is_empty());
        assert_eq!(on_disk.revision(), previous.revision());
    }

    #[test]
    fn paste_sequence_confirmed_apply_failure_rolls_back_and_never_reuses_id() {
        let directory = tempfile::tempdir().unwrap();
        let store = SequenceStore::new(directory.path().join(PASTE_SEQUENCES_FILE));
        let previous = store.load().unwrap();
        let mut candidate = previous.clone();
        let candidate_id = candidate.upsert(hotkey_draft("Candidate")).unwrap();

        let result = commit_sequence_candidate_with_route_update(
            &store,
            &previous,
            &mut candidate,
            |_| Ok(()),
            || Err(RouteActivationFailure::Confirmed("route rejected".into())),
        );

        assert!(result.unwrap_err().message.contains("rolled back"));
        let mut restored = store.load().unwrap();
        assert!(restored.sequences().is_empty());
        assert_eq!(restored.revision(), 2);
        let next_id = restored.upsert(hotkey_draft("Next candidate")).unwrap();
        assert!(next_id.get() > candidate_id.get());
    }

    #[test]
    fn paste_sequence_apply_rollback_never_overwrites_a_concurrent_edit() {
        let directory = tempfile::tempdir().unwrap();
        let store = SequenceStore::new(directory.path().join(PASTE_SEQUENCES_FILE));
        let previous = store.load().unwrap();
        let mut candidate = previous.clone();
        let id = candidate.upsert(hotkey_draft("Candidate")).unwrap();

        let result = commit_sequence_candidate_with_route_update(
            &store,
            &previous,
            &mut candidate,
            |_| Ok(()),
            || {
                let mut concurrent = store.load().unwrap();
                let mut edited = SequenceDraft::from_sequence(concurrent.sequence(id).unwrap());
                edited.name = "Concurrent edit".into();
                concurrent.upsert(edited).unwrap();
                store.commit(&mut concurrent).unwrap();
                Err(RouteActivationFailure::Confirmed("route rejected".into()))
            },
        );

        assert!(result.unwrap_err().message.contains("changed"));
        let current = store.load().unwrap();
        assert_eq!(current.sequence(id).unwrap().name, "Concurrent edit");
    }
}

fn store_error_text(error: &SequenceStoreError) -> String {
    match error {
        SequenceStoreError::StaleSnapshot { .. } => {
            "The saved sequence list changed elsewhere. Close and reopen the editor before saving."
                .into()
        }
        _ => error.to_string(),
    }
}

fn preflight_error_text(error: crate::paste_sequence::SequencePreflightError) -> &'static str {
    error.message()
}
