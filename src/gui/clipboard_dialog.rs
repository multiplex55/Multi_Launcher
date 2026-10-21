#[cfg(not(test))]
use crate::clipboard_modify::clipboard::ArboardClipboardBackend;
use crate::clipboard_modify::clipboard::ClipboardBackend;
use crate::gui::LauncherApp;
use crate::gui::jump_mode::TargetGeneration;
use crate::gui::theme::widgets::{ButtonEmphasis, compact_section_heading, with_button_emphasis};
use crate::gui::theme::{TextRole, text_style_for_role};
#[cfg(test)]
use crate::plugins::clipboard::save_history;
use crate::plugins::clipboard::{
    CLIPBOARD_FILE, CheckedHistoryUpdate, load_history, remove_entry_if_unchanged,
    save_history_if_unchanged,
};
use eframe::egui;
use std::collections::VecDeque;
#[cfg(test)]
use std::sync::Arc;

fn register_clipboard_target(
    scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
    ui: &egui::Ui,
    response: &egui::Response,
    identity: impl AsRef<str>,
    label: &'static str,
    kind: crate::gui::jump_mode::TargetKind,
    visual_order: &mut u64,
) -> Option<crate::gui::jump_mode::ui::FocusDirective> {
    crate::gui::jump_mode::ui::register_and_apply_target(
        scope,
        ui,
        response,
        identity,
        label,
        kind,
        visual_order,
    )
}

const CLIPBOARD_HISTORY_PREVIEW_CHAR_LIMIT: usize = 160;

#[derive(Default)]
struct ClipboardDialogEffects {
    save_request: Option<ClipboardSaveRequest>,
    copy_request: Option<ClipboardCopyRequest>,
    removal_request: Option<(usize, u64)>,
    confirm_removal: bool,
    cancel_removal: bool,
    close: bool,
    invalid_text: bool,
    #[cfg(test)]
    browse_scroll_rect: Option<egui::Rect>,
    #[cfg(test)]
    browse_footer_rect: Option<egui::Rect>,
    #[cfg(test)]
    edit_footer_rect: Option<egui::Rect>,
    #[cfg(test)]
    edit_footer_max_scroll_x: Option<f32>,
    #[cfg(test)]
    browse_edit_rect: Option<egui::Rect>,
    #[cfg(test)]
    browse_remove_rect: Option<egui::Rect>,
    #[cfg(test)]
    browse_scroll_clip_rect: Option<egui::Rect>,
    #[cfg(test)]
    browse_scroll_content_size: Option<egui::Vec2>,
    #[cfg(test)]
    browse_footer_clip_rect: Option<egui::Rect>,
    #[cfg(test)]
    edit_footer_clip_rect: Option<egui::Rect>,
    #[cfg(test)]
    browse_scroll_id: Option<egui::Id>,
    #[cfg(test)]
    edit_footer_scroll_id: Option<egui::Id>,
}

struct ClipboardSaveRequest {
    index: usize,
    entry_token: u64,
    edit_session: u64,
    expected_entries: VecDeque<String>,
    replacement: VecDeque<String>,
}

struct ClipboardCopyRequest {
    index: usize,
    entry_token: u64,
    expected_entries: VecDeque<String>,
    entry: String,
}

#[derive(Clone)]
struct PendingClipboardRemoval {
    index: usize,
    entry_token: u64,
    confirmation_instance: u64,
    expected_entry: String,
    expected_entries: VecDeque<String>,
}

#[derive(Default)]
pub struct ClipboardDialog {
    pub open: bool,
    entries: Vec<String>,
    entry_tokens: Vec<u64>,
    selected_entry_token: Option<u64>,
    target_generation: u64,
    next_entry_token: u64,
    edit_session: u64,
    edit_entry_token: Option<u64>,
    pending_removal: Option<PendingClipboardRemoval>,
    removal_instance: u64,
    inline_error: Option<String>,
    observed_clipboard_version: Option<u64>,
    snapshot_current: bool,
    edit_idx: Option<usize>,
    text: String,
    #[cfg(test)]
    test_visible_entry_rows: Vec<(u64, egui::Id, egui::Rect)>,
    #[cfg(test)]
    test_edit_text_widget_id: Option<egui::Id>,
    #[cfg(test)]
    test_clipboard_backend: Option<Arc<dyn ClipboardBackend>>,
}

impl ClipboardDialog {
    pub fn open(&mut self) {
        self.replace_loaded_entries(load_history(CLIPBOARD_FILE).unwrap_or_default().into());
        self.open = true;
        self.edit_idx = None;
        self.edit_entry_token = None;
        self.pending_removal = None;
        self.advance_edit_session();
        self.text.clear();
    }

    pub fn ensure_open(&mut self) {
        if !self.open {
            self.open();
        }
    }

    pub fn open_edit(&mut self, idx: usize) {
        self.replace_loaded_entries(load_history(CLIPBOARD_FILE).unwrap_or_default().into());
        self.edit_entry_token = self.entry_tokens.get(idx).copied();
        if idx < self.entries.len() {
            self.text = self.entries[idx].clone();
        } else {
            self.text.clear();
        }
        self.edit_idx = Some(idx);
        self.pending_removal = None;
        self.advance_edit_session();
        self.open = true;
    }

    pub(crate) fn jump_mode_target_generation(&self) -> TargetGeneration {
        TargetGeneration(self.target_generation)
    }

    pub(crate) fn jump_mode_pending_removal_identity(&self) -> Option<(u64, u64)> {
        self.pending_removal
            .as_ref()
            .map(|pending| (pending.confirmation_instance, pending.entry_token))
    }

    #[cfg(test)]
    pub(crate) fn test_selected_entry_index(&self) -> Option<usize> {
        let token = self.selected_entry_token?;
        self.entry_tokens
            .iter()
            .position(|current| *current == token)
    }

    #[cfg(test)]
    pub(crate) fn test_is_editing(&self) -> bool {
        self.edit_idx.is_some()
    }

    #[cfg(test)]
    pub(crate) fn test_text(&self) -> &str {
        &self.text
    }

    #[cfg(test)]
    pub(crate) fn test_edit_text_widget_id(&self) -> Option<egui::Id> {
        self.edit_idx.and(self.test_edit_text_widget_id)
    }

    #[cfg(test)]
    pub(crate) fn test_visible_entry_rows(&self) -> &[(u64, egui::Id, egui::Rect)] {
        &self.test_visible_entry_rows
    }

    #[cfg(test)]
    pub(crate) fn set_test_clipboard_backend(&mut self, backend: Arc<dyn ClipboardBackend>) {
        self.test_clipboard_backend = Some(backend);
    }

    #[cfg(test)]
    pub(crate) fn test_inline_error(&self) -> Option<&str> {
        self.inline_error.as_deref()
    }

    fn advance_target_generation(&mut self) {
        self.target_generation = self.target_generation.wrapping_add(1).max(1);
        self.selected_entry_token = None;
    }

    fn renew_entry_tokens(&mut self) {
        self.entry_tokens = (0..self.entries.len())
            .map(|_| {
                self.next_entry_token = self.next_entry_token.wrapping_add(1).max(1);
                self.next_entry_token
            })
            .collect();
        self.advance_target_generation();
    }

    fn replace_loaded_entries(&mut self, entries: Vec<String>) {
        self.entries = entries;
        self.renew_entry_tokens();
        self.snapshot_current = true;
        self.pending_removal = None;
        self.inline_error = None;
        self.observed_clipboard_version = Some(crate::plugins::clipboard::clipboard_version());
    }

    pub(crate) fn observe_clipboard_version(&mut self) {
        if !self.open {
            return;
        }
        let version = crate::plugins::clipboard::clipboard_version();
        if self.observed_clipboard_version == Some(version) {
            if !self.snapshot_current && self.edit_idx.is_none() && self.pending_removal.is_none() {
                self.reload_after_edit();
            }
            return;
        }

        self.observed_clipboard_version = Some(version);
        if self.edit_idx.is_some() {
            // Do not replace an active edit draft with an external clipboard
            // update. Its old browse identities are no longer authoritative.
            self.snapshot_current = false;
            self.renew_entry_tokens();
            self.inline_error = Some(
                "Clipboard history changed. Your draft is preserved, but cannot be saved.".into(),
            );
        } else if self.pending_removal.is_some() {
            self.snapshot_current = false;
            self.advance_target_generation();
            self.inline_error = Some(
                "Clipboard history changed. Cancel this confirmation to reload current rows."
                    .into(),
            );
        } else {
            self.reload_after_edit();
        }
    }

    fn reload_after_edit(&mut self) {
        self.replace_loaded_entries(load_history(CLIPBOARD_FILE).unwrap_or_default().into());
    }

    fn advance_edit_session(&mut self) {
        self.edit_session = self.edit_session.wrapping_add(1).max(1);
    }

    fn begin_edit(&mut self, index: usize, entry_token: u64) {
        if !self.snapshot_current
            || self.pending_removal.is_some()
            || self.entry_tokens.get(index) != Some(&entry_token)
            || self.entries.get(index).is_none()
        {
            return;
        }
        self.edit_idx = Some(index);
        self.edit_entry_token = Some(entry_token);
        self.text = self.entries[index].clone();
        self.advance_edit_session();
        self.advance_target_generation();
        self.inline_error = None;
    }

    fn request_removal(&mut self, index: usize, entry_token: u64) {
        let Some(expected_entry) = self.entries.get(index).cloned() else {
            return;
        };
        if !self.snapshot_current
            || self.pending_removal.is_some()
            || self.entry_tokens.get(index) != Some(&entry_token)
        {
            return;
        }
        self.removal_instance = self.removal_instance.wrapping_add(1).max(1);
        self.pending_removal = Some(PendingClipboardRemoval {
            index,
            entry_token,
            confirmation_instance: self.removal_instance,
            expected_entry,
            expected_entries: self.entries.clone().into(),
        });
        self.advance_target_generation();
        self.inline_error = None;
    }

    fn cancel_removal(&mut self) {
        if self.pending_removal.take().is_some() {
            self.advance_target_generation();
        }
        self.inline_error = None;
    }

    fn stale_snapshot_message() -> &'static str {
        "Clipboard history changed. No changes were saved; cancel and reload before continuing."
    }

    fn commit_edit(&mut self, request: ClipboardSaveRequest, app: &mut LauncherApp) {
        let expected = self.entries.iter().cloned().collect::<VecDeque<_>>();
        let valid_edit = self.snapshot_current
            && self.edit_idx == Some(request.index)
            && self.edit_entry_token == Some(request.entry_token)
            && self.edit_session == request.edit_session
            && self.entry_tokens.get(request.index) == Some(&request.entry_token)
            && expected == request.expected_entries;
        if !valid_edit {
            self.snapshot_current = false;
            self.advance_target_generation();
            self.inline_error = Some(Self::stale_snapshot_message().into());
            return;
        }

        match save_history_if_unchanged(
            CLIPBOARD_FILE,
            &request.expected_entries,
            &request.replacement,
        ) {
            Ok(CheckedHistoryUpdate::Updated) => {
                self.entries = request.replacement.into();
                self.edit_idx = None;
                self.edit_entry_token = None;
                self.pending_removal = None;
                self.text.clear();
                self.snapshot_current = true;
                self.inline_error = None;
                self.renew_entry_tokens();
                app.search();
                app.focus_input();
            }
            Ok(CheckedHistoryUpdate::Conflict) => {
                self.snapshot_current = false;
                self.observed_clipboard_version =
                    Some(crate::plugins::clipboard::clipboard_version());
                self.advance_target_generation();
                self.inline_error = Some(Self::stale_snapshot_message().into());
            }
            Err(_) => {
                self.inline_error =
                    Some("Could not save clipboard history. Your draft is preserved.".into());
            }
        }
    }

    fn confirm_removal(&mut self, app: &mut LauncherApp) {
        let Some(pending) = self.pending_removal.clone() else {
            return;
        };
        let expected = self.entries.iter().cloned().collect::<VecDeque<_>>();
        if !self.snapshot_current
            || self.entry_tokens.get(pending.index) != Some(&pending.entry_token)
            || self.entries.get(pending.index) != Some(&pending.expected_entry)
            || expected != pending.expected_entries
        {
            self.snapshot_current = false;
            self.observed_clipboard_version = Some(crate::plugins::clipboard::clipboard_version());
            self.advance_target_generation();
            self.inline_error = Some(Self::stale_snapshot_message().into());
            return;
        }

        match remove_entry_if_unchanged(CLIPBOARD_FILE, &pending.expected_entries, pending.index) {
            Ok(CheckedHistoryUpdate::Updated) => {
                if pending.index < self.entries.len() {
                    self.entries.remove(pending.index);
                }
                self.pending_removal = None;
                self.snapshot_current = true;
                self.inline_error = None;
                self.renew_entry_tokens();
                app.search();
                app.focus_input();
            }
            Ok(CheckedHistoryUpdate::Conflict) => {
                self.snapshot_current = false;
                self.observed_clipboard_version =
                    Some(crate::plugins::clipboard::clipboard_version());
                self.advance_target_generation();
                self.inline_error = Some(Self::stale_snapshot_message().into());
            }
            Err(_) => {
                self.inline_error = Some(
                    "Could not remove the clipboard entry. The confirmation is still open.".into(),
                );
            }
        }
    }

    fn copy_history_entry(&mut self, request: ClipboardCopyRequest) {
        let expected = self.entries.iter().cloned().collect::<VecDeque<_>>();
        let valid_entry = self.snapshot_current
            && self.edit_idx.is_none()
            && self.pending_removal.is_none()
            && expected == request.expected_entries
            && self.entry_tokens.get(request.index) == Some(&request.entry_token)
            && self.entries.get(request.index) == Some(&request.entry);
        if !valid_entry {
            self.inline_error = Some("Clipboard history changed. No item was copied.".into());
            return;
        }

        match load_history(CLIPBOARD_FILE) {
            Ok(current) if current == request.expected_entries => {}
            Ok(current) => {
                self.replace_loaded_entries(current.into());
                self.inline_error = Some(
                    "Clipboard history changed. No item was copied; current rows were reloaded."
                        .into(),
                );
                return;
            }
            Err(_) => {
                self.snapshot_current = false;
                self.advance_target_generation();
                self.inline_error =
                    Some("Clipboard history could not be verified. No item was copied.".into());
                return;
            }
        }

        let result = {
            #[cfg(test)]
            let result = self
                .test_clipboard_backend
                .as_deref()
                .ok_or_else(|| {
                    crate::clipboard_modify::clipboard::ClipboardError::Permanent(
                        "test clipboard backend is unavailable".into(),
                    )
                })
                .and_then(|backend| backend.write_text(&request.entry));
            #[cfg(not(test))]
            let result = ArboardClipboardBackend.write_text(&request.entry);
            result
        };
        self.inline_error = result.err().map(|_| "Clipboard copy failed.".into());
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        self.observe_clipboard_version();
        self.ui_with_jump_mode(ctx, app, None, true, true);
    }

    pub(crate) fn ui_with_jump_mode(
        &mut self,
        ctx: &egui::Context,
        app: &mut LauncherApp,
        scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
        native_ui_owner: bool,
        first_ui_pass: bool,
    ) {
        if !self.open {
            return;
        }
        let pinned = app.pinned_panels.contains(&super::Panel::ClipboardDialog);
        let mut window_open = self.open;
        let mut effects = ClipboardDialogEffects::default();
        let authorized_owner_pass = native_ui_owner && first_ui_pass;
        let scope = native_ui_owner.then_some(scope).flatten();
        let generation_before_ui = self.target_generation;
        egui::Window::new("Clipboard History")
            .open(&mut window_open)
            .resizable(true)
            .default_size((360.0, 240.0))
            .min_width(100.0)
            .min_height(100.0)
            .show(ctx, |ui| {
                let authorized_owner_pass =
                    authorized_owner_pass && ui.is_visible() && !ui.is_sizing_pass();
                effects = ui
                    .add_enabled_ui(native_ui_owner, |ui| {
                        self.contents_ui(ui, scope, authorized_owner_pass)
                    })
                    .inner;
            });
        if authorized_owner_pass {
            self.open = window_open || pinned;
        }

        if authorized_owner_pass {
            if let Some((index, entry_token)) = effects.removal_request {
                self.request_removal(index, entry_token);
            }
            if effects.confirm_removal {
                self.confirm_removal(app);
            }
            if effects.cancel_removal {
                self.cancel_removal();
            }
            if let Some(request) = effects.save_request {
                self.commit_edit(request, app);
            }
            if let Some(request) = effects.copy_request {
                self.copy_history_entry(request);
            }
        }
        if authorized_owner_pass && effects.close && !pinned {
            self.open = false;
            self.advance_target_generation();
        }
        if authorized_owner_pass && effects.invalid_text {
            app.report_error_message("ui operation", "Text required");
        }
        if self.target_generation != generation_before_ui
            && let Some(scope) = scope
        {
            scope.discard_registered_targets();
        }
    }

    fn contents_ui(
        &mut self,
        ui: &mut egui::Ui,
        scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
        authorized_owner_pass: bool,
    ) -> ClipboardDialogEffects {
        if let Some(idx) = self.edit_idx {
            self.edit_ui(ui, idx, scope, authorized_owner_pass)
        } else {
            self.browse_ui_with_jump_mode(ui, scope, authorized_owner_pass)
        }
    }

    fn edit_ui(
        &mut self,
        ui: &mut egui::Ui,
        idx: usize,
        scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
        authorized_owner_pass: bool,
    ) -> ClipboardDialogEffects {
        compact_section_heading(ui, "Text", None);
        let footer_height = clipboard_history_footer_reserved_height(ui, &["Save", "Cancel"]);
        let body_height = (ui.available_height() - footer_height).max(0.0);
        let mut text_shadow;
        let text = if authorized_owner_pass {
            &mut self.text
        } else {
            text_shadow = self.text.clone();
            &mut text_shadow
        };
        let edit_entry_token = self.edit_entry_token;
        let edit_session = self.edit_session;
        #[cfg(test)]
        let mut edit_text_widget_id = None;
        let can_save = self.snapshot_current
            && edit_entry_token.is_some_and(|token| self.entry_tokens.get(idx) == Some(&token));
        let mut visual_order = 0;
        let edit_body_output = egui::ScrollArea::vertical()
            .id_salt("clipboard_history_edit_scroll")
            .auto_shrink([false, false])
            .min_scrolled_height(0.0)
            .max_height(body_height)
            .show(ui, |ui| {
                let text_response = ui.add(
                    egui::TextEdit::multiline(text)
                        .id_salt(("clipboard_history_edit_text", edit_session))
                        .desired_rows(1)
                        .desired_width(f32::INFINITY),
                );
                #[cfg(test)]
                {
                    edit_text_widget_id = Some(text_response.id);
                }
                register_clipboard_target(
                    scope,
                    ui,
                    &text_response,
                    "editor/text",
                    "Clipboard text",
                    crate::gui::jump_mode::TargetKind::TextEntry,
                    &mut visual_order,
                );
            });

        #[cfg(test)]
        {
            self.test_edit_text_widget_id = edit_text_widget_id;
        }

        ui.separator();
        if let Some(error) = &self.inline_error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        let mut effects = ClipboardDialogEffects::default();
        let _ = edit_body_output;
        #[cfg(test)]
        let mut edit_footer_clip = egui::Rect::NOTHING;
        let footer_output = clipboard_history_footer_ui(
            ui,
            "clipboard_history_edit_footer",
            &["Save", "Cancel"],
            |ui| {
                #[cfg(test)]
                {
                    edit_footer_clip = ui.clip_rect();
                }
                let save_button = ui.add_enabled(can_save, egui::Button::new("Save"));
                let save = with_button_emphasis(ui, save_button, ButtonEmphasis::Primary);
                let save_directive = register_clipboard_target(
                    scope,
                    ui,
                    &save,
                    "editor/save",
                    "Save clipboard entry",
                    crate::gui::jump_mode::TargetKind::Button,
                    &mut visual_order,
                );
                if save.clicked() && save_directive.is_none() && authorized_owner_pass {
                    if self.text.trim().is_empty() {
                        effects.invalid_text = true;
                    } else if let Some(entry_token) = edit_entry_token {
                        let expected_entries =
                            self.entries.iter().cloned().collect::<VecDeque<_>>();
                        if idx < expected_entries.len() {
                            let mut replacement = expected_entries.clone();
                            replacement[idx] = self.text.clone();
                            effects.save_request = Some(ClipboardSaveRequest {
                                index: idx,
                                entry_token,
                                edit_session: self.edit_session,
                                expected_entries,
                                replacement,
                            });
                        }
                    }
                }
                let cancel_button = ui.button("Cancel");
                let cancel_directive = register_clipboard_target(
                    scope,
                    ui,
                    &cancel_button,
                    "editor/cancel",
                    "Cancel clipboard edit",
                    crate::gui::jump_mode::TargetKind::Button,
                    &mut visual_order,
                );
                if cancel_button.clicked() && cancel_directive.is_none() && authorized_owner_pass {
                    self.edit_idx = None;
                    self.edit_entry_token = None;
                    self.advance_edit_session();
                    self.advance_target_generation();
                    self.inline_error = None;
                }
            },
        );
        #[cfg(test)]
        {
            effects.edit_footer_rect = Some(footer_output.inner_rect);
            effects.edit_footer_max_scroll_x =
                Some((footer_output.content_size.x - footer_output.inner_rect.width()).max(0.0));
            effects.edit_footer_clip_rect = Some(edit_footer_clip);
            effects.edit_footer_scroll_id = Some(footer_output.id);
        }
        let _ = footer_output;
        effects
    }

    #[cfg(test)]
    fn browse_ui(&mut self, ui: &mut egui::Ui) -> ClipboardDialogEffects {
        self.browse_ui_with_jump_mode(ui, None, true)
    }

    fn browse_ui_with_jump_mode(
        &mut self,
        ui: &mut egui::Ui,
        scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
        authorized_owner_pass: bool,
    ) -> ClipboardDialogEffects {
        let mut effects = ClipboardDialogEffects::default();
        let mut visual_order = 0;
        let parent_scope = (self.pending_removal.is_none() && self.snapshot_current)
            .then_some(scope)
            .flatten();
        let removal_scope = self.pending_removal.as_ref().and(scope);
        let pending_removal = self.pending_removal.clone();
        #[cfg(test)]
        self.test_visible_entry_rows.clear();
        let heading = ui.add(
            egui::Label::new(
                egui::RichText::new("Clipboard entries")
                    .text_style(text_style_for_role(TextRole::SectionHeading))
                    .strong(),
            )
            .truncate(),
        );
        heading.on_hover_text("Clipboard entries");
        let footer_height = clipboard_history_footer_reserved_height(ui, &["Close"]);
        let body_height = (ui.available_height() - footer_height).max(0.0);
        let mut removal_request = None;
        #[cfg(test)]
        let mut browse_scroll_clip = egui::Rect::NOTHING;
        let browse_output = egui::ScrollArea::both()
            .id_salt("clipboard_history_browse_scroll")
            .auto_shrink([false, false])
            .min_scrolled_height(0.0)
            .max_height(body_height)
            .show(ui, |ui| {
                #[cfg(test)]
                {
                    browse_scroll_clip = ui.clip_rect();
                }
                for idx in 0..self.entries.len() {
                    let entry = self.entries[idx].clone();
                    let Some(entry_token) = self.entry_tokens.get(idx).copied() else {
                        continue;
                    };
                    let confirmation_row = pending_removal.as_ref().is_some_and(|pending| {
                        pending.index == idx && pending.entry_token == entry_token
                    });
                    let parent_actions_enabled = pending_removal.is_none() && self.snapshot_current;
                    let preview = clipboard_history_entry_preview(&entry);
                    let selected = self.selected_entry_token == Some(entry_token);
                    let label = ui
                        .add_enabled_ui(parent_actions_enabled, |ui| {
                            ui.push_id(("clipboard-history-entry", entry_token), |ui| {
                                ui.selectable_label(selected, preview)
                            })
                            .inner
                        })
                        .inner;
                    #[cfg(test)]
                    if label.rect.intersect(ui.clip_rect()).is_positive() {
                        self.test_visible_entry_rows.push((
                            entry_token,
                            label.id,
                            label.rect.intersect(ui.clip_rect()),
                        ));
                    }
                    let select_identity = format!("entry/{entry_token}/select");
                    let select_target_id =
                        parent_scope.map(|scope| scope.target_id(&select_identity));
                    let directive = register_clipboard_target(
                        parent_scope,
                        ui,
                        &label,
                        &select_identity,
                        "Clipboard entry",
                        crate::gui::jump_mode::TargetKind::Action,
                        &mut visual_order,
                    );
                    let idx_copy = idx;
                    if parent_actions_enabled {
                        label.clone().context_menu(|ui| {
                            if ui.button("Edit Entry").clicked() && authorized_owner_pass {
                                self.begin_edit(idx_copy, entry_token);
                                ui.close();
                            }
                            let remove_response = ui.button("Remove Entry");
                            if with_button_emphasis(
                                ui,
                                remove_response,
                                ButtonEmphasis::Destructive,
                            )
                            .clicked()
                                && authorized_owner_pass
                            {
                                removal_request = Some((idx_copy, entry_token));
                                ui.close();
                            }
                        });
                    }
                    if label.clicked() && authorized_owner_pass && parent_actions_enabled {
                        self.selected_entry_token = Some(entry_token);
                        label.request_focus();
                    }
                    if directive == Some(crate::gui::jump_mode::ui::FocusDirective::Select)
                        && authorized_owner_pass
                        && parent_actions_enabled
                        && self.entry_tokens.get(idx) == Some(&entry_token)
                    {
                        self.selected_entry_token = Some(entry_token);
                        label.request_focus();
                        if let (Some(scope), Some(target_id)) =
                            (parent_scope, select_target_id.as_ref())
                        {
                            scope.record_select_focus_owner(target_id, label.id);
                        }
                    }
                    ui.push_id(("clipboard-history-row-controls", entry_token), |ui| {
                        ui.horizontal_wrapped(|ui| {
                            if let Some(pending) =
                                pending_removal.as_ref().filter(|_| confirmation_row)
                            {
                                let confirm_enabled = self.snapshot_current
                                    && self.entries.iter().cloned().collect::<VecDeque<_>>()
                                        == pending.expected_entries
                                    && self.entry_tokens.get(idx) == Some(&pending.entry_token);
                                let confirm = ui
                                    .push_id(
                                        (
                                            "clipboard-history-remove-confirm",
                                            pending.confirmation_instance,
                                            entry_token,
                                        ),
                                        |ui| {
                                            ui.add_enabled(
                                                confirm_enabled,
                                                egui::Button::new("Confirm"),
                                            )
                                        },
                                    )
                                    .inner;
                                let confirm_directive = register_clipboard_target(
                                    removal_scope,
                                    ui,
                                    &confirm,
                                    "removal/confirm",
                                    "Confirm clipboard entry removal",
                                    crate::gui::jump_mode::TargetKind::Button,
                                    &mut visual_order,
                                );
                                if confirm.clicked()
                                    && confirm_directive.is_none()
                                    && authorized_owner_pass
                                {
                                    effects.confirm_removal = true;
                                }
                                let cancel = ui
                                    .push_id(
                                        (
                                            "clipboard-history-remove-cancel",
                                            pending.confirmation_instance,
                                            entry_token,
                                        ),
                                        |ui| ui.button("Cancel"),
                                    )
                                    .inner;
                                let cancel_directive = register_clipboard_target(
                                    removal_scope,
                                    ui,
                                    &cancel,
                                    "removal/cancel",
                                    "Cancel clipboard entry removal",
                                    crate::gui::jump_mode::TargetKind::Button,
                                    &mut visual_order,
                                );
                                if cancel.clicked()
                                    && cancel_directive.is_none()
                                    && authorized_owner_pass
                                {
                                    effects.cancel_removal = true;
                                }
                            } else {
                                ui.add_enabled_ui(parent_actions_enabled, |ui| {
                                    let copy = ui.button("Copy");
                                    let copy_identity = format!("entry/{entry_token}/copy");
                                    let copy_directive = register_clipboard_target(
                                        parent_scope,
                                        ui,
                                        &copy,
                                        &copy_identity,
                                        "Copy clipboard entry",
                                        crate::gui::jump_mode::TargetKind::Button,
                                        &mut visual_order,
                                    );
                                    if copy.clicked()
                                        && copy_directive.is_none()
                                        && authorized_owner_pass
                                    {
                                        effects.copy_request = Some(ClipboardCopyRequest {
                                            index: idx,
                                            entry_token,
                                            expected_entries: self
                                                .entries
                                                .iter()
                                                .cloned()
                                                .collect(),
                                            entry: entry.clone(),
                                        });
                                    }
                                    let edit_button = ui.button("Edit");
                                    let edit_identity = format!("entry/{entry_token}/edit");
                                    let edit_directive = register_clipboard_target(
                                        parent_scope,
                                        ui,
                                        &edit_button,
                                        &edit_identity,
                                        "Edit clipboard entry",
                                        crate::gui::jump_mode::TargetKind::Button,
                                        &mut visual_order,
                                    );
                                    #[cfg(test)]
                                    if idx == 0 {
                                        effects.browse_edit_rect = Some(edit_button.rect);
                                    }
                                    if edit_button.clicked()
                                        && edit_directive.is_none()
                                        && authorized_owner_pass
                                    {
                                        self.begin_edit(idx, entry_token);
                                    }
                                    let remove_response = ui.button("Remove");
                                    let remove_identity = format!("entry/{entry_token}/remove");
                                    let remove_directive = register_clipboard_target(
                                        parent_scope,
                                        ui,
                                        &remove_response,
                                        &remove_identity,
                                        "Remove clipboard entry",
                                        crate::gui::jump_mode::TargetKind::Button,
                                        &mut visual_order,
                                    );
                                    #[cfg(test)]
                                    if idx == 0 {
                                        effects.browse_remove_rect = Some(remove_response.rect);
                                    }
                                    if remove_response.clicked()
                                        && remove_directive.is_none()
                                        && authorized_owner_pass
                                    {
                                        removal_request = Some((idx, entry_token));
                                    }
                                });
                            }
                        });
                    });
                }
            });

        if authorized_owner_pass && let Some((index, entry_token)) = removal_request {
            effects.removal_request = Some((index, entry_token));
        }
        #[cfg(test)]
        {
            effects.browse_scroll_rect = Some(browse_output.inner_rect);
            effects.browse_scroll_clip_rect = Some(browse_scroll_clip);
            effects.browse_scroll_content_size = Some(browse_output.content_size);
            effects.browse_scroll_id = Some(browse_output.id);
        }
        let _ = browse_output;
        if let Some(error) = &self.inline_error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        ui.separator();
        #[cfg(test)]
        let mut browse_footer_clip = egui::Rect::NOTHING;
        let footer_output =
            clipboard_history_footer_ui(ui, "clipboard_history_browse_footer", &["Close"], |ui| {
                #[cfg(test)]
                {
                    browse_footer_clip = ui.clip_rect();
                }
                let close_enabled = self.pending_removal.is_none();
                let close_button = ui.add_enabled(close_enabled, egui::Button::new("Close"));
                let close_directive = register_clipboard_target(
                    close_enabled.then_some(scope).flatten(),
                    ui,
                    &close_button,
                    "close",
                    "Close Clipboard History",
                    crate::gui::jump_mode::TargetKind::Button,
                    &mut visual_order,
                );
                if close_button.clicked() && close_directive.is_none() && authorized_owner_pass {
                    effects.close = true;
                }
            });
        #[cfg(test)]
        {
            effects.browse_footer_rect = Some(footer_output.inner_rect);
            effects.browse_footer_clip_rect = Some(browse_footer_clip);
        }
        let _ = footer_output;
        effects
    }
}

fn clipboard_history_entry_preview(text: &str) -> String {
    let mut preview = String::with_capacity(CLIPBOARD_HISTORY_PREVIEW_CHAR_LIMIT + 3);
    let mut chars = text.chars();
    for character in chars.by_ref().take(CLIPBOARD_HISTORY_PREVIEW_CHAR_LIMIT) {
        preview.push(if matches!(character, '\n' | '\r') {
            ' '
        } else {
            character
        });
    }
    if chars.next().is_some() {
        preview.push('…');
    }
    preview
}

fn clipboard_history_footer_ui<R>(
    ui: &mut egui::Ui,
    id_salt: &'static str,
    labels: &[&str],
    add_buttons: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::scroll_area::ScrollAreaOutput<egui::InnerResponse<R>> {
    let (button_height, content_width) = clipboard_history_footer_metrics(ui, labels);
    let needs_scroll = content_width > ui.available_width();
    let scroll_height = button_height
        + if needs_scroll {
            ui.spacing().scroll.bar_width
        } else {
            0.0
        };
    let visibility = if needs_scroll {
        egui::scroll_area::ScrollBarVisibility::AlwaysVisible
    } else {
        egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded
    };
    egui::ScrollArea::horizontal()
        .id_salt(id_salt)
        .auto_shrink([false, true])
        .scroll_bar_visibility(visibility)
        .max_height(scroll_height)
        .show(ui, |ui| {
            ui.set_max_height(button_height);
            ui.horizontal(add_buttons)
        })
}

fn clipboard_history_footer_reserved_height(ui: &mut egui::Ui, labels: &[&str]) -> f32 {
    let (button_height, content_width) = clipboard_history_footer_metrics(ui, labels);
    let spacing = ui.spacing().clone();
    let style = ui.style().clone();
    let separator_height = style
        .separator_style(
            &egui::widget_style::Classes::default(),
            egui::widget_style::WidgetState::Noninteractive,
        )
        .spacing;
    let scrollbar_height = if content_width > ui.available_width() {
        spacing.scroll.bar_width
    } else {
        0.0
    };
    button_height + scrollbar_height + 2.0 * spacing.item_spacing.y + separator_height
}

fn clipboard_history_footer_metrics(ui: &mut egui::Ui, labels: &[&str]) -> (f32, f32) {
    let item_spacing = ui.spacing().item_spacing;
    let interact_size = ui.spacing().interact_size;
    let extra_text_line_spacing = ui.spacing().extra_text_line_spacing;
    let style = ui.style().clone();
    let font_id = style
        .override_font_id
        .clone()
        .or_else(|| {
            style
                .override_text_style
                .as_ref()
                .map(|text_style| text_style.resolve(&style))
        })
        .unwrap_or_else(|| egui::TextStyle::Body.resolve(&style));
    let text_height = ui.fonts_mut(|fonts| fonts.row_height(&font_id)) + extra_text_line_spacing;
    let button_frame = style
        .button_style(
            &egui::widget_style::Classes::default(),
            egui::widget_style::WidgetState::Inactive,
        )
        .frame;
    let button_height = (text_height + button_frame.total_margin().sum().y).max(interact_size.y);
    let text_color = ui.visuals().text_color();
    let mut content_width = 0.0;
    for label in labels {
        let text_width = ui.fonts_mut(|fonts| {
            fonts
                .layout_no_wrap((*label).to_owned(), font_id.clone(), text_color)
                .size()
                .x
        });
        let width = (text_width + button_frame.total_margin().sum().x).max(interact_size.x);
        if content_width > 0.0 {
            content_width += item_spacing.x;
        }
        content_width += width;
    }
    (button_height, content_width)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::theme::initialize_runtime;
    use crate::settings::ThemeSettings;
    use crate::{plugin::PluginManager, settings::Settings};
    use std::sync::{Arc, atomic::AtomicBool};

    fn button_bounds(
        output: &egui::FullOutput,
        label: &str,
        within: egui::Rect,
    ) -> Option<egui::Rect> {
        output
            .platform_output
            .accesskit_update
            .as_ref()?
            .nodes
            .iter()
            .filter(|(_, node)| {
                node.role() == egui::accesskit::Role::Button && node.label() == Some(label)
            })
            .find_map(|(_, node)| {
                let bounds = node.bounds()?;
                let bounds = egui::Rect::from_min_max(
                    egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                    egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
                );
                let visible_bounds = bounds.intersect(within);
                visible_bounds.is_positive().then_some(visible_bounds)
            })
    }

    #[test]
    fn ensure_open_loads_saved_history_once_and_preserves_edit_draft() {
        let _lock = crate::gui::TEST_MUTEX.lock().unwrap();
        let workspace = crate::performance::workloads::IsolatedWorkspace::new();

        let saved_history = VecDeque::from([
            "first saved item".to_owned(),
            "second saved item".to_owned(),
            "third saved item".to_owned(),
        ]);
        let expected_history = saved_history.iter().cloned().collect::<Vec<_>>();
        save_history(CLIPBOARD_FILE, &saved_history).unwrap();

        let ctx = egui::Context::default();
        let actions_path = workspace.root().join("actions.json");
        let settings_path = workspace.root().join("settings.json");
        let mut settings = Settings::default();
        settings.paste_sequence.next_hotkey = None;
        settings.paste_sequence.cancel_hotkey = None;
        let mut app = LauncherApp::new(
            &ctx,
            Arc::new(Vec::new()),
            0,
            PluginManager::new(),
            actions_path.to_string_lossy().into_owned(),
            settings_path.to_string_lossy().into_owned(),
            settings,
            None,
            None,
            None,
            None,
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        );

        app.ensure_open(super::super::Panel::ClipboardDialog);
        assert!(app.clipboard_dialog.open);
        assert_eq!(app.clipboard_dialog.entries, expected_history);

        app.clipboard_dialog.edit_idx = Some(1);
        app.clipboard_dialog.text = "unsaved edit draft".to_owned();
        save_history(
            CLIPBOARD_FILE,
            &VecDeque::from(["external update".to_owned()]),
        )
        .unwrap();

        app.ensure_open(super::super::Panel::ClipboardDialog);
        assert!(app.clipboard_dialog.open);
        assert_eq!(
            app.clipboard_dialog.entries.len(),
            3,
            "open history is not reloaded mid-session"
        );
        assert_eq!(app.clipboard_dialog.entries[1], "second saved item");
        assert_eq!(app.clipboard_dialog.edit_idx, Some(1));
        assert_eq!(app.clipboard_dialog.text, "unsaved edit draft");
    }

    #[test]
    fn jump_mode_clipboard_browse_version_change_invalidates_rows_and_preserves_edit_draft() {
        let _lock = crate::gui::TEST_MUTEX.lock().unwrap();
        let _workspace = crate::performance::workloads::IsolatedWorkspace::new();
        let initial = VecDeque::from([
            "duplicate secret λ".to_owned(),
            "duplicate secret λ".to_owned(),
        ]);
        save_history(CLIPBOARD_FILE, &initial).unwrap();

        let mut dialog = ClipboardDialog::default();
        dialog.open();
        let original_generation = dialog.target_generation;
        let original_tokens = dialog.entry_tokens.clone();
        let externally_inserted = VecDeque::from([
            "new clipboard item".to_owned(),
            "duplicate secret λ".to_owned(),
            "duplicate secret λ".to_owned(),
        ]);
        save_history(CLIPBOARD_FILE, &externally_inserted).unwrap();
        dialog.observe_clipboard_version();

        assert!(dialog.snapshot_current);
        assert_eq!(
            dialog.entries,
            externally_inserted.iter().cloned().collect::<Vec<_>>()
        );
        assert_ne!(dialog.target_generation, original_generation);
        assert_ne!(dialog.entry_tokens, original_tokens);

        let browse_generation = dialog.target_generation;
        let browse_tokens = dialog.entry_tokens.clone();
        dialog.edit_idx = Some(2);
        dialog.text = "unsaved editor draft".to_owned();

        save_history(
            CLIPBOARD_FILE,
            &VecDeque::from(["external clipboard update".to_owned()]),
        )
        .unwrap();
        dialog.observe_clipboard_version();

        assert!(!dialog.snapshot_current);
        assert_ne!(dialog.target_generation, browse_generation);
        assert_ne!(dialog.entry_tokens, browse_tokens);
        assert_eq!(
            dialog.entries,
            externally_inserted.iter().cloned().collect::<Vec<_>>()
        );
        assert_eq!(dialog.text, "unsaved editor draft");
        assert_eq!(dialog.edit_idx, Some(2));

        dialog.edit_idx = None;
        dialog.observe_clipboard_version();
        assert!(dialog.snapshot_current);
        assert_eq!(dialog.entries, ["external clipboard update"]);
        assert_ne!(dialog.entry_tokens, browse_tokens);
    }

    #[test]
    fn jump_mode_clipboard_checked_persistence_preserves_draft_on_conflict_and_io_failure() {
        let _lock = crate::gui::TEST_MUTEX.lock().unwrap();
        let workspace = crate::performance::workloads::IsolatedWorkspace::new();
        let original = VecDeque::from(["private first".to_owned(), "private second".to_owned()]);
        save_history(CLIPBOARD_FILE, &original).unwrap();

        let ctx = egui::Context::default();
        let mut settings = Settings::default();
        settings.paste_sequence.next_hotkey = None;
        settings.paste_sequence.cancel_hotkey = None;
        let mut app = LauncherApp::new(
            &ctx,
            Arc::new(Vec::new()),
            0,
            PluginManager::new_inert_for_test(),
            workspace
                .root()
                .join("actions.json")
                .to_string_lossy()
                .into_owned(),
            workspace
                .root()
                .join("settings.json")
                .to_string_lossy()
                .into_owned(),
            settings,
            None,
            None,
            Some(std::collections::HashSet::new()),
            Some(std::collections::HashMap::new()),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        );
        app.clipboard_dialog.open();
        let token = app.clipboard_dialog.entry_tokens[0];
        app.clipboard_dialog.begin_edit(0, token);
        app.clipboard_dialog.text = "unsaved Unicode draft λ\nsecond line".into();
        let request = ClipboardSaveRequest {
            index: 0,
            entry_token: token,
            edit_session: app.clipboard_dialog.edit_session,
            expected_entries: original.clone(),
            replacement: VecDeque::from([
                "unsaved Unicode draft λ\nsecond line".to_owned(),
                "private second".to_owned(),
            ]),
        };

        let external = VecDeque::from([
            "external insertion".to_owned(),
            "private first".to_owned(),
            "private second".to_owned(),
        ]);
        save_history(CLIPBOARD_FILE, &external).unwrap();
        let external_bytes = std::fs::read(CLIPBOARD_FILE).unwrap();
        let mut dialog = std::mem::take(&mut app.clipboard_dialog);
        dialog.commit_edit(request, &mut app);
        app.clipboard_dialog = dialog;

        assert_eq!(std::fs::read(CLIPBOARD_FILE).unwrap(), external_bytes);
        assert!(app.clipboard_dialog.test_is_editing());
        assert_eq!(
            app.clipboard_dialog.text,
            "unsaved Unicode draft λ\nsecond line"
        );
        assert!(!app.clipboard_dialog.snapshot_current);
        assert!(app.clipboard_dialog.test_inline_error().is_some());

        // Make the path unreadable as a history file. The checked operation
        // must preserve the current draft on this I/O failure as well.
        std::fs::remove_file(CLIPBOARD_FILE).unwrap();
        std::fs::create_dir(CLIPBOARD_FILE).unwrap();
        app.clipboard_dialog.snapshot_current = true;
        app.clipboard_dialog.entries = original.iter().cloned().collect();
        app.clipboard_dialog.entry_tokens[0] = token;
        app.clipboard_dialog.edit_idx = Some(0);
        app.clipboard_dialog.edit_entry_token = Some(token);
        app.clipboard_dialog.text = "draft survives failed write".into();
        let failed_request = ClipboardSaveRequest {
            index: 0,
            entry_token: token,
            edit_session: app.clipboard_dialog.edit_session,
            expected_entries: original.clone(),
            replacement: VecDeque::from([
                "draft survives failed write".to_owned(),
                "private second".to_owned(),
            ]),
        };
        let mut dialog = std::mem::take(&mut app.clipboard_dialog);
        dialog.commit_edit(failed_request, &mut app);
        app.clipboard_dialog = dialog;

        assert!(app.clipboard_dialog.test_is_editing());
        assert_eq!(
            app.clipboard_dialog.test_text(),
            "draft survives failed write"
        );
        assert!(app.clipboard_dialog.test_inline_error().is_some());
    }

    #[test]
    fn jump_mode_clipboard_checked_persistence_keeps_pending_removal_after_external_change() {
        let _lock = crate::gui::TEST_MUTEX.lock().unwrap();
        let _workspace = crate::performance::workloads::IsolatedWorkspace::new();
        save_history(CLIPBOARD_FILE, &VecDeque::from(["same row".to_owned()])).unwrap();
        let mut dialog = ClipboardDialog::default();
        dialog.open();
        let token = dialog.entry_tokens[0];
        dialog.request_removal(0, token);

        save_history(
            CLIPBOARD_FILE,
            &VecDeque::from(["external row".to_owned(), "same row".to_owned()]),
        )
        .unwrap();
        dialog.observe_clipboard_version();
        let pending_identity = dialog.jump_mode_pending_removal_identity();
        assert!(!dialog.snapshot_current);
        assert_eq!(pending_identity, Some((1, token)));

        dialog.observe_clipboard_version();
        assert!(!dialog.snapshot_current);
        assert_eq!(
            dialog.jump_mode_pending_removal_identity(),
            pending_identity
        );
        assert_eq!(dialog.entries, ["same row"]);
        assert!(dialog.test_inline_error().is_some());
    }

    #[test]
    fn long_history_entry_keeps_browse_and_edit_footers_reachable_without_mutating_text() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut theme = ThemeSettings::default();
        theme.preferences.typography_scale = 2.0;
        initialize_runtime(&ctx, &theme);
        let screen_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(320.0, 400.0));
        let window_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(100.0, 320.0));
        let input = || egui::RawInput {
            screen_rect: Some(screen_rect),
            ..Default::default()
        };
        let mut effective_font_size = None;
        let long_entry = format!("{}\n{}", "clipboard text ".repeat(80), "tail".repeat(80));
        let mut dialog = ClipboardDialog {
            open: true,
            entries: vec![long_entry.clone()],
            entry_tokens: vec![1],
            selected_entry_token: None,
            target_generation: 1,
            next_entry_token: 1,
            edit_session: 1,
            edit_entry_token: Some(1),
            pending_removal: None,
            removal_instance: 0,
            inline_error: None,
            observed_clipboard_version: Some(crate::plugins::clipboard::clipboard_version()),
            snapshot_current: true,
            edit_idx: None,
            text: long_entry.clone(),
            test_visible_entry_rows: Vec::new(),
            test_edit_text_widget_id: None,
            test_clipboard_backend: None,
        };

        let mut first_frame_sizing_pass = false;
        let mut first_frame_visible = true;
        let first_frame_output = ctx.run_ui(input(), |ctx| {
            egui::Window::new("Clipboard History")
                .fixed_rect(window_rect)
                .resizable(false)
                .min_width(100.0)
                .min_height(100.0)
                .show(ctx, |ui| {
                    first_frame_sizing_pass = ui.is_sizing_pass();
                    first_frame_visible = ui.is_visible();
                    ui.style_mut().override_font_id = Some(egui::FontId::proportional(31.0));
                    let _ = dialog.browse_ui(ui);
                });
        });
        first_frame_output.drop_without_applying_deltas();

        let mut browse_content_rect = egui::Rect::NOTHING;
        let mut browse_window_rect = egui::Rect::NOTHING;
        let mut browse_sizing_pass = true;
        let mut browse_visible = false;
        let mut browse_list_rect = egui::Rect::NOTHING;
        let mut browse_list_clip_rect = egui::Rect::NOTHING;
        let mut browse_list_content_size = egui::Vec2::ZERO;
        let mut browse_edit_rect = egui::Rect::NOTHING;
        let mut browse_remove_rect = egui::Rect::NOTHING;
        let mut browse_footer_rect = egui::Rect::NOTHING;
        let mut browse_footer_clip_rect = egui::Rect::NOTHING;
        let mut browse_scroll_id = None;
        let browse_output = ctx.run_ui(input(), |ctx| {
            let window_output = egui::Window::new("Clipboard History")
                .fixed_rect(window_rect)
                .resizable(false)
                .min_width(100.0)
                .min_height(100.0)
                .show(ctx, |ui| {
                    browse_content_rect = ui.max_rect();
                    browse_sizing_pass = ui.is_sizing_pass();
                    browse_visible = ui.is_visible();
                    ui.style_mut().override_font_id = Some(egui::FontId::proportional(31.0));
                    effective_font_size =
                        ui.style().override_font_id.as_ref().map(|font| font.size);
                    let effects = dialog.browse_ui(ui);
                    assert!(!effects.close && effects.save_request.is_none());
                    browse_list_rect = effects
                        .browse_scroll_rect
                        .expect("browse records its native list viewport");
                    browse_list_clip_rect = effects
                        .browse_scroll_clip_rect
                        .expect("browse records its native list clip");
                    browse_list_content_size = effects
                        .browse_scroll_content_size
                        .expect("browse records its full content size");
                    browse_edit_rect = effects
                        .browse_edit_rect
                        .expect("browse records its first Edit rectangle");
                    browse_remove_rect = effects
                        .browse_remove_rect
                        .expect("browse records its first Remove rectangle");
                    browse_footer_rect = effects
                        .browse_footer_rect
                        .expect("browse records its native footer viewport");
                    browse_footer_clip_rect = effects
                        .browse_footer_clip_rect
                        .expect("browse records its native footer clip");
                    browse_scroll_id = effects.browse_scroll_id;
                });
            if let Some(window_output) = window_output {
                browse_window_rect = window_output.response.rect;
            }
        });
        let _browse_list_visible_clip = browse_content_rect
            .intersect(browse_list_rect)
            .intersect(browse_list_clip_rect);
        let browse_footer_visible_clip = browse_content_rect
            .intersect(browse_footer_rect)
            .intersect(browse_footer_clip_rect);
        let close_bounds = button_bounds(&browse_output, "Close", browse_footer_visible_clip);
        let stored_entry_unchanged = dialog.entries[0] == long_entry;
        browse_output.drop_without_applying_deltas();
        assert!(
            first_frame_sizing_pass && !first_frame_visible,
            "first Window frame should be the invisible sizing pass"
        );
        assert!(
            !browse_sizing_pass && browse_visible,
            "browse geometry must come from a visible non-sizing frame"
        );
        assert!(
            screen_rect.contains_rect(browse_window_rect),
            "the constrained browse window remains within its viewport"
        );
        assert!(stored_entry_unchanged);
        assert!(
            browse_list_content_size.y > browse_list_rect.height(),
            "the constrained list retains native vertical scroll range"
        );
        assert!(
            close_bounds.is_some(),
            "Close remains visible in the actual footer viewport"
        );

        let browse_scroll_id = browse_scroll_id.expect("browse exposes its native scroll id");
        let initial_scroll_state = egui::scroll_area::State::load(&ctx, browse_scroll_id)
            .expect("browse stores its vertical scroll state");
        let initial_scroll_offset = initial_scroll_state.offset.y;
        for (label, action_rect) in [("Edit", browse_edit_rect), ("Remove", browse_remove_rect)] {
            let mut scroll_state = egui::scroll_area::State::load(&ctx, browse_scroll_id)
                .expect("browse stores its vertical scroll state");
            scroll_state.offset.y = (initial_scroll_offset + action_rect.center().y
                - browse_list_rect.center().y)
                .max(0.0);
            scroll_state.store(&ctx, browse_scroll_id);

            let mut action_content_rect = egui::Rect::NOTHING;
            let mut action_list_rect = egui::Rect::NOTHING;
            let mut action_list_clip_rect = egui::Rect::NOTHING;
            let mut action_rect = egui::Rect::NOTHING;
            let scrolled_output = ctx.run_ui(input(), |ctx| {
                egui::Window::new("Clipboard History")
                    .fixed_rect(window_rect)
                    .resizable(false)
                    .min_width(100.0)
                    .min_height(100.0)
                    .show(ctx, |ui| {
                        action_content_rect = ui.max_rect();
                        ui.style_mut().override_font_id = Some(egui::FontId::proportional(31.0));
                        let effects = dialog.browse_ui(ui);
                        action_list_rect = effects
                            .browse_scroll_rect
                            .expect("browse records its native list viewport");
                        action_list_clip_rect = effects
                            .browse_scroll_clip_rect
                            .expect("browse records its native list clip");
                        action_rect = match label {
                            "Edit" => effects.browse_edit_rect,
                            "Remove" => effects.browse_remove_rect,
                            _ => None,
                        }
                        .expect("browse records the requested action rectangle");
                    });
            });
            let visible_clip = action_content_rect
                .intersect(action_list_rect)
                .intersect(action_list_clip_rect);
            let action_is_visible = action_rect.intersect(visible_clip).is_positive();
            let action_accessible = button_bounds(&scrolled_output, label, visible_clip).is_some();
            scrolled_output.drop_without_applying_deltas();
            assert!(
                action_is_visible && action_accessible,
                "{label} is reachable through native list scrolling"
            );
        }

        assert_eq!(
            clipboard_history_entry_preview(&long_entry).chars().count(),
            CLIPBOARD_HISTORY_PREVIEW_CHAR_LIMIT + 1,
            "the browse row is bounded while the stored text stays complete"
        );

        dialog.edit_idx = Some(0);
        dialog.edit_entry_token = Some(1);
        let mut edit_content_rect = egui::Rect::NOTHING;
        let mut edit_footer_id = None;
        let mut edit_footer_rect = egui::Rect::NOTHING;
        let mut edit_footer_clip_rect = egui::Rect::NOTHING;
        let mut edit_footer_max_scroll_x = 0.0;
        let edit_output = ctx.run_ui(input(), |ctx| {
            egui::Window::new("Clipboard History")
                .fixed_rect(window_rect)
                .resizable(false)
                .min_width(100.0)
                .min_height(100.0)
                .show(ctx, |ui| {
                    edit_content_rect = ui.max_rect();
                    ui.style_mut().override_font_id = Some(egui::FontId::proportional(31.0));
                    let effects = dialog.edit_ui(ui, 0, None, true);
                    assert!(
                        effects.save_request.is_none() && !effects.close && !effects.invalid_text
                    );
                    edit_footer_rect = effects
                        .edit_footer_rect
                        .expect("edit records its native footer viewport");
                    edit_footer_clip_rect = effects
                        .edit_footer_clip_rect
                        .expect("edit records its native footer clip");
                    edit_footer_max_scroll_x = effects
                        .edit_footer_max_scroll_x
                        .expect("edit records its native horizontal scroll extent");
                    edit_footer_id = effects.edit_footer_scroll_id;
                });
        });
        let save_bounds = button_bounds(
            &edit_output,
            "Save",
            edit_content_rect
                .intersect(edit_footer_rect)
                .intersect(edit_footer_clip_rect),
        );
        let editor_text_unchanged = dialog.text == long_entry;
        let entry_still_unchanged = dialog.entries[0] == long_entry;
        edit_output.drop_without_applying_deltas();
        assert!(editor_text_unchanged && entry_still_unchanged);

        let save_is_reachable = save_bounds.is_some();
        let footer_id = edit_footer_id.expect("the edit window records its footer id");
        let mut footer_state = egui::scroll_area::State::load(&ctx, footer_id)
            .expect("the edit footer stores its horizontal scroll state");
        footer_state.offset.x = edit_footer_max_scroll_x;
        footer_state.store(&ctx, footer_id);

        let mut scrolled_edit_content_rect = egui::Rect::NOTHING;
        let mut scrolled_edit_footer_rect = egui::Rect::NOTHING;
        let mut scrolled_edit_footer_clip_rect = egui::Rect::NOTHING;
        let scrolled_edit_output = ctx.run_ui(input(), |ctx| {
            egui::Window::new("Clipboard History")
                .fixed_rect(window_rect)
                .resizable(false)
                .min_width(100.0)
                .min_height(100.0)
                .show(ctx, |ui| {
                    scrolled_edit_content_rect = ui.max_rect();
                    ui.style_mut().override_font_id = Some(egui::FontId::proportional(31.0));
                    let effects = dialog.edit_ui(ui, 0, None, true);
                    assert!(
                        effects.save_request.is_none() && !effects.close && !effects.invalid_text
                    );
                    scrolled_edit_footer_rect = effects
                        .edit_footer_rect
                        .expect("edit records its native footer viewport");
                    scrolled_edit_footer_clip_rect = effects
                        .edit_footer_clip_rect
                        .expect("edit records its native footer clip");
                });
        });
        let cancel_bounds = button_bounds(
            &scrolled_edit_output,
            "Cancel",
            scrolled_edit_content_rect
                .intersect(scrolled_edit_footer_rect)
                .intersect(scrolled_edit_footer_clip_rect),
        );
        let edit_state_unchanged = dialog.text == long_entry && dialog.entries[0] == long_entry;
        scrolled_edit_output.drop_without_applying_deltas();
        assert!(save_is_reachable);
        assert!(cancel_bounds.is_some());
        assert!(
            edit_state_unchanged,
            "browsing and scrolling do not mutate the draft"
        );
        assert_eq!(
            effective_font_size,
            Some(31.0),
            "the constrained-window fixture uses an effective 31 point font"
        );
    }
}
