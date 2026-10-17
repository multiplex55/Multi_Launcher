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
    Saved {
        id: SequenceId,
        draft: SequenceDraft,
    },
    Removed {
        id: SequenceId,
    },
}

struct HotkeyUpdateCompletion {
    result: Result<bool, String>,
    snapshot: Option<SequenceSnapshot>,
    editor_update: SequenceEditorUpdate,
}

enum RouteActivationFailure {
    Confirmed(String),
    Uncertain(String),
}

pub(crate) struct PasteSequenceDialog {
    pub(crate) open: bool,
    #[cfg(test)]
    layout: DialogLayout,
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
}

impl Default for PasteSequenceDialog {
    fn default() -> Self {
        Self {
            open: false,
            #[cfg(test)]
            layout: DialogLayout::default(),
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
        }
    }
}

impl PasteSequenceDialog {
    pub(crate) fn open_with_context(
        &mut self,
        context: Result<SequenceInvocationContext, crate::paste_sequence::SequencePreflightError>,
    ) {
        if !self.open {
            self.begin_session();
        }
        match context {
            Ok(context) => {
                self.context = Some(context);
                self.status = Some("Destination captured before the editor opened.".into());
            }
            Err(error) => {
                self.context = None;
                self.inline_error = Some(preflight_error_text(error).into());
            }
        }
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
        self.open = true;
    }

    fn load_sources(&mut self) {
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

    pub(crate) fn end_session(&mut self) {
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

    pub(crate) fn ui(&mut self, ctx: &egui::Context, app: &mut super::LauncherApp) {
        if !self.open {
            return;
        }
        self.poll_hotkey_update(ctx);
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
                            if ui.button("New").clicked() {
                                self.selected_id = None;
                                self.selected_step = None;
                                self.reveal_step = None;
                                self.remove_confirmation = None;
                                self.draft = SequenceDraft::new("New sequence", Vec::new());
                                self.hotkey_text.clear();
                                self.inline_error = None;
                            }
                            if ui
                                .add_enabled(
                                    self.selected_id.is_some() && self.hotkey_update.is_none(),
                                    egui::Button::new("Remove Saved"),
                                )
                                .clicked()
                            {
                                self.remove_confirmation = self.selected_id;
                            }
                        });
                        self.saved_sequence_list(ui);
                        self.saved_sequence_remove_confirmation(ui, ctx, app);
                        ui.separator();
                        self.editor(ui, app, step_height);
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
                let footer = ui.scope(|ui| self.editor_footer(ui, ctx, app));
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
    }

    fn saved_sequence_list(&mut self, ui: &mut egui::Ui) {
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
                    if ui
                        .selectable_label(
                            selected,
                            format!("{}  ·  {} steps", sequence.name, sequence.steps.len()),
                        )
                        .clicked()
                    {
                        self.draft = SequenceDraft::from_sequence(&sequence);
                        self.selected_id = Some(sequence.id);
                        self.selected_step = None;
                        self.reveal_step = None;
                        self.remove_confirmation = None;
                        self.hotkey_text = sequence
                            .hotkey
                            .as_ref()
                            .map(|chord| chord.as_str().to_owned())
                            .unwrap_or_default();
                        self.inline_error = None;
                    }
                }
            });
    }

    fn saved_sequence_remove_confirmation(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        app: &mut super::LauncherApp,
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
            if ui.button("Confirm Remove").clicked() {
                self.remove_saved_sequence(ctx, app, id);
            }
            if ui.button("Keep").clicked() {
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
        );
    }

    fn editor(&mut self, ui: &mut egui::Ui, app: &mut super::LauncherApp, step_height: f32) {
        if let Some(context) = &self.context {
            let label = context
                .target()
                .title()
                .filter(|title| !title.trim().is_empty())
                .unwrap_or("External window");
            ui.weak(format!("Captured destination: {label}"));
        } else {
            ui.weak(
                "No external destination captured; close and reopen this editor from the launcher.",
            );
        }
        let mut name_changed = false;
        ui.horizontal(|ui| {
            ui.label("Name");
            name_changed = ui.text_edit_singleline(&mut self.draft.name).changed();
        });
        if name_changed {
            self.inline_error = None;
        }
        ui.horizontal(|ui| {
            ui.label("Optional hotkey");
            ui.add(egui::TextEdit::singleline(&mut self.hotkey_text).desired_width(180.0));
        });
        ui.weak("Uses the application's existing chord syntax");
        ui.horizontal(|ui| {
            ui.label("Playback");
            ui.selectable_value(&mut self.draft.mode, PlaybackMode::Automatic, "Automatic");
            ui.selectable_value(
                &mut self.draft.mode,
                PlaybackMode::ManualAdvance,
                "Manual advance",
            );
        });
        ui.horizontal(|ui| {
            ui.label("Delay between steps");
            ui.add(egui::Slider::new(&mut self.draft.delay_ms, 0..=60_000).suffix(" ms"));
        });

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
            if ui.button("Add Temporary Text").clicked() {
                self.draft.steps.push(Step::LiteralText {
                    text: String::new(),
                });
                self.selected_step = self.draft.steps.len().checked_sub(1);
                self.reveal_step = self.selected_step;
            }
            if ui.button("Move Up").clicked()
                && let Some(index) = self.selected_step
                && index > 0
            {
                self.draft.steps.swap(index, index - 1);
                self.selected_step = Some(index - 1);
                self.reveal_step = self.selected_step;
            }
            if ui.button("Move Down").clicked()
                && let Some(index) = self.selected_step
                && index + 1 < self.draft.steps.len()
            {
                self.draft.steps.swap(index, index + 1);
                self.selected_step = Some(index + 1);
                self.reveal_step = self.selected_step;
            }
            if ui
                .add_enabled(self.selected_step.is_some(), egui::Button::new("Remove"))
                .clicked()
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
    ) {
        let state = app.sequence_playback.status();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!self.draft.steps.is_empty(), egui::Button::new("Preview"))
                .clicked()
            {
                self.prepare_current(app, false, false);
            }
            if ui
                .add_enabled(!self.draft.steps.is_empty(), egui::Button::new("Run Once"))
                .clicked()
            {
                self.prepare_current(app, true, true);
            }
            if ui
                .add_enabled(!self.draft.steps.is_empty(), egui::Button::new("Run"))
                .clicked()
            {
                self.prepare_current(app, true, false);
            }
            if ui
                .add_enabled(
                    self.snapshot.is_some() && self.hotkey_update.is_none(),
                    egui::Button::new("Save"),
                )
                .clicked()
            {
                self.save_draft(ctx, app);
            }
        });
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    state == crate::paste_sequence::SequencePlaybackStatus::Ready,
                    egui::Button::new("Start"),
                )
                .clicked()
            {
                match app.start_sequence_playback() {
                    Ok(()) => self.status = Some("Sequence started.".into()),
                    Err(error) => self.inline_error = Some(error.message().into()),
                }
            }
            if ui
                .add_enabled(
                    state == crate::paste_sequence::SequencePlaybackStatus::AwaitAdvance,
                    egui::Button::new("Next"),
                )
                .clicked()
            {
                match app.next_sequence_playback() {
                    Ok(()) => self.status = Some("Advanced to the next step.".into()),
                    Err(error) => self.inline_error = Some(error.message().into()),
                }
            }
            if ui
                .add_enabled(
                    matches!(
                        state,
                        crate::paste_sequence::SequencePlaybackStatus::Preparing
                            | crate::paste_sequence::SequencePlaybackStatus::Ready
                            | crate::paste_sequence::SequencePlaybackStatus::Running
                            | crate::paste_sequence::SequencePlaybackStatus::AwaitAdvance
                    ),
                    egui::Button::new("Cancel Run"),
                )
                .clicked()
            {
                app.cancel_active_paste_sequence();
                self.autostart_when_ready = false;
                self.status =
                    Some("Sequence cancelled; clipboard cleanup is pending if needed.".into());
            }
            if ui.button("Cancel Editor").clicked() {
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
                                        .hint_text("Text pasted only when the run starts"),
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
        use crate::paste_sequence::SequencePlaybackStatus as Status;
        let status = app.sequence_playback.status();
        let label = match status {
            Status::Idle => "Idle".to_owned(),
            Status::Preparing => "Preparing steps and prompts".into(),
            Status::Ready => "Ready; no text has been pasted".into(),
            Status::Running => "Running".into(),
            Status::AwaitAdvance => "Waiting for Next".into(),
            Status::Cancelled => "Cancelled".into(),
            Status::Completed => "Completed".into(),
            Status::Failed(error) => format!("Failed: {}", error.message()),
        };
        let message = if let Some((current, total)) = app.sequence_playback.current_step_number() {
            format!("{label} \u{00b7} step {current} of {total}")
        } else {
            format!("Run status: {label}")
        };
        ui.add(egui::Label::new(&message).truncate())
            .on_hover_text(&message);
    }

    fn show_preview(&self, ui: &mut egui::Ui, preview: &PreparedSequencePreview) {
        ui.separator();
        ui.label(format!(
            "Preview · {} steps · {:?} · {} ms delay",
            preview.item_count, preview.mode, preview.delay_ms
        ));
        ui.weak(format!("Target: {}", preview.target_label));
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
        self.inline_error = None;
        self.status = None;
        let Some(context) = self.context.clone() else {
            self.inline_error = Some("No external destination was captured before the editor opened. Close and reopen it from the launcher.".into());
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
                    match app.start_sequence_playback() {
                        Ok(()) => self.status = Some("Sequence started.".into()),
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

    fn save_draft(&mut self, ctx: &egui::Context, app: &mut super::LauncherApp) {
        self.inline_error = None;
        let mut draft = self.current_draft();
        if !self.hotkey_text.trim().is_empty() && draft.hotkey.is_none() {
            self.inline_error = Some("Hotkey chord is invalid.".into());
            return;
        }
        let Some(previous) = self.snapshot.as_ref().cloned() else {
            self.inline_error = Some(
                "Saved sequences are unavailable because the store could not be loaded.".into(),
            );
            return;
        };
        let mut candidate = previous.clone();
        match candidate.upsert(draft.clone()) {
            Ok(id) => {
                draft.id = Some(id);
                self.status = Some("Preparing sequence and shortcut update…".into());
                self.begin_hotkey_update(
                    ctx,
                    app,
                    previous,
                    candidate,
                    SequenceEditorUpdate::Saved { id, draft },
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
    ) {
        let settings_path = app.settings_path.clone();
        let store = SequenceStore::new(self.store_path.clone());
        let repaint = ctx.clone();
        let (sender, receiver) = std::sync::mpsc::channel();
        let spawned = app.settings_save_workers.spawn(move || {
            let ticket = std::cell::RefCell::new(None);
            let settings_for_apply = std::cell::RefCell::new(None);
            let plugin_enabled = std::cell::Cell::new(false);
            let result = commit_sequence_candidate_with_route_update(
                &store,
                &previous,
                &mut candidate,
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
            if result.is_err()
                && let Some(mut staged) = ticket.borrow_mut().take()
            {
                use std::time::Duration;
                if staged.cancel().is_ok() {
                    let _ = staged.wait_terminal_disposition(Duration::from_secs(5));
                }
            }
            let snapshot = store.load().ok();
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
                    Ok(true) => {
                        self.status = Some("Sequence saved; shortcut routes are active.".into());
                        self.inline_error = None;
                    }
                    Ok(false) => {
                        self.status = Some("Sequence saved; shortcuts are inactive while the Paste Sequences plugin is disabled.".into());
                        self.inline_error = None;
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

fn commit_sequence_candidate_with_route_update(
    store: &SequenceStore,
    previous: &SequenceSnapshot,
    candidate: &mut SequenceSnapshot,
    prepare: impl FnOnce(&[SequenceHotkeyBinding]) -> Result<(), String>,
    apply: impl FnOnce() -> Result<bool, RouteActivationFailure>,
) -> Result<bool, String> {
    let bindings = candidate.hotkey_bindings();
    prepare(&bindings).map_err(|error| {
        format!("Shortcut preparation failed; the sequence was not saved: {error}")
    })?;
    store
        .commit(candidate)
        .map_err(|error| format!("Sequence could not be saved: {}", store_error_text(&error)))?;

    match apply() {
        Ok(plugin_enabled) => Ok(plugin_enabled),
        Err(RouteActivationFailure::Confirmed(error)) => {
            store
                .rollback_if_revision(candidate.revision(), previous)
                .map_err(|rollback| {
                    format!(
                        "Shortcut activation failed ({error}); rollback could not be completed: {}",
                        store_error_text(&rollback)
                    )
                })?;
            Err(format!(
                "Shortcut activation failed; the sequence change was rolled back: {error}"
            ))
        }
        Err(RouteActivationFailure::Uncertain(error)) => Err(format!(
            "Shortcut activation could not be confirmed; the saved sequence was retained: {error}"
        )),
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
    transaction_applied: bool,
) {
    let should_apply_update = transaction_applied
        || match update {
            SequenceEditorUpdate::Saved { id, .. } => snapshot.sequence(*id).is_some(),
            SequenceEditorUpdate::Removed { id } => snapshot.sequence(*id).is_none(),
        };
    if should_apply_update {
        match update {
            SequenceEditorUpdate::Saved { id, draft: saved } => {
                *draft = saved.clone();
                *selected_id = Some(*id);
                *hotkey_text = saved
                    .hotkey
                    .as_ref()
                    .map(|chord| chord.as_str().to_owned())
                    .unwrap_or_default();
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
            "Text pasted only when the run starts",
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

        assert!(result.unwrap_err().contains("was not saved"));
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

        assert!(result.unwrap_err().contains("rolled back"));
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

        assert!(result.unwrap_err().contains("changed"));
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
