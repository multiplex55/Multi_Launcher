use crate::gui::LauncherApp;
use crate::gui::jump_mode::TargetGeneration;
use crate::gui::theme::widgets::{ButtonEmphasis, compact_section_heading, with_button_emphasis};
use crate::gui::theme::{TextRole, text_style_for_role};
use crate::plugins::clipboard::{CLIPBOARD_FILE, load_history, save_history};
use eframe::egui;
use std::collections::VecDeque;

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
    save: bool,
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

#[derive(Default)]
pub struct ClipboardDialog {
    pub open: bool,
    entries: Vec<String>,
    entry_tokens: Vec<u64>,
    selected_entry_token: Option<u64>,
    target_generation: u64,
    next_entry_token: u64,
    observed_clipboard_version: Option<u64>,
    snapshot_current: bool,
    edit_idx: Option<usize>,
    text: String,
    #[cfg(test)]
    test_visible_entry_rows: Vec<(u64, egui::Id, egui::Rect)>,
}

impl ClipboardDialog {
    pub fn open(&mut self) {
        self.replace_loaded_entries(load_history(CLIPBOARD_FILE).unwrap_or_default().into());
        self.open = true;
        self.edit_idx = None;
        self.text.clear();
    }

    pub fn ensure_open(&mut self) {
        if !self.open {
            self.open();
        }
    }

    pub fn open_edit(&mut self, idx: usize) {
        self.replace_loaded_entries(load_history(CLIPBOARD_FILE).unwrap_or_default().into());
        if idx < self.entries.len() {
            self.text = self.entries[idx].clone();
        } else {
            self.text.clear();
        }
        self.edit_idx = Some(idx);
        self.open = true;
    }

    pub(crate) fn jump_mode_target_generation(&self) -> TargetGeneration {
        TargetGeneration(self.target_generation)
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
    pub(crate) fn test_visible_entry_rows(&self) -> &[(u64, egui::Id, egui::Rect)] {
        &self.test_visible_entry_rows
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
        self.observed_clipboard_version = Some(crate::plugins::clipboard::clipboard_version());
    }

    pub(crate) fn observe_clipboard_version(&mut self) {
        if !self.open {
            return;
        }
        let version = crate::plugins::clipboard::clipboard_version();
        if self.observed_clipboard_version == Some(version) {
            if !self.snapshot_current && self.edit_idx.is_none() {
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
        } else {
            self.reload_after_edit();
        }
    }

    fn reload_after_edit(&mut self) {
        self.replace_loaded_entries(load_history(CLIPBOARD_FILE).unwrap_or_default().into());
    }

    fn save(&mut self, app: &mut LauncherApp) {
        let history: VecDeque<String> = self.entries.clone().into();
        if let Err(e) = save_history(CLIPBOARD_FILE, &history) {
            app.report_error_message(
                "ui operation",
                format!("Failed to save clipboard history: {e}"),
            );
        } else {
            app.search();
            app.focus_input();
        }
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

        if authorized_owner_pass && effects.save {
            self.save(app);
            self.snapshot_current = true;
            self.observe_clipboard_version();
        }
        if authorized_owner_pass && effects.close && !pinned {
            self.open = false;
            self.advance_target_generation();
        }
        if authorized_owner_pass && effects.invalid_text {
            app.report_error_message("ui operation", "Text required");
        }
    }

    fn contents_ui(
        &mut self,
        ui: &mut egui::Ui,
        scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
        authorized_owner_pass: bool,
    ) -> ClipboardDialogEffects {
        if let Some(idx) = self.edit_idx {
            self.edit_ui(ui, idx, authorized_owner_pass)
        } else {
            self.browse_ui_with_jump_mode(ui, scope, authorized_owner_pass)
        }
    }

    fn edit_ui(
        &mut self,
        ui: &mut egui::Ui,
        idx: usize,
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
        let can_save = self.snapshot_current;
        let edit_body_output = egui::ScrollArea::vertical()
            .id_salt("clipboard_history_edit_scroll")
            .auto_shrink([false, false])
            .min_scrolled_height(0.0)
            .max_height(body_height)
            .show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(text)
                        .id_salt("clipboard_history_edit_text")
                        .desired_rows(1)
                        .desired_width(f32::INFINITY),
                );
            });

        ui.separator();
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
                if save.clicked() && authorized_owner_pass {
                    if self.text.trim().is_empty() {
                        effects.invalid_text = true;
                    } else {
                        if idx < self.entries.len() {
                            self.entries[idx] = self.text.clone();
                        }
                        self.renew_entry_tokens();
                        self.edit_idx = None;
                        self.text.clear();
                        effects.save = true;
                    }
                }
                let cancel_button = ui.button("Cancel");
                if cancel_button.clicked() && authorized_owner_pass {
                    self.edit_idx = None;
                    self.advance_target_generation();
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
        let row_scope = self.snapshot_current.then_some(scope).flatten();
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
        let mut remove = None;
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
                    let preview = clipboard_history_entry_preview(&entry);
                    let selected = self.selected_entry_token == Some(entry_token);
                    let label = ui
                        .push_id(("clipboard-history-entry", entry_token), |ui| {
                            ui.selectable_label(selected, preview)
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
                    let select_target_id = row_scope.map(|scope| scope.target_id(&select_identity));
                    let directive = register_clipboard_target(
                        row_scope,
                        ui,
                        &label,
                        &select_identity,
                        "Clipboard entry",
                        crate::gui::jump_mode::TargetKind::Action,
                        &mut visual_order,
                    );
                    let idx_copy = idx;
                    label.clone().context_menu(|ui| {
                        if ui.button("Edit Entry").clicked()
                            && authorized_owner_pass
                            && self.snapshot_current
                        {
                            self.edit_idx = Some(idx_copy);
                            self.text = self.entries[idx_copy].clone();
                            self.advance_target_generation();
                            ui.close();
                        }
                        let remove_response = ui.button("Remove Entry");
                        if with_button_emphasis(ui, remove_response, ButtonEmphasis::Destructive)
                            .clicked()
                            && authorized_owner_pass
                            && self.snapshot_current
                        {
                            remove = Some(idx_copy);
                            ui.close();
                        }
                    });
                    if label.clicked() && authorized_owner_pass && self.snapshot_current {
                        self.selected_entry_token = Some(entry_token);
                        label.request_focus();
                    }
                    if directive == Some(crate::gui::jump_mode::ui::FocusDirective::Select)
                        && authorized_owner_pass
                        && self.snapshot_current
                        && self.entry_tokens.get(idx) == Some(&entry_token)
                    {
                        self.selected_entry_token = Some(entry_token);
                        label.request_focus();
                        if let (Some(scope), Some(target_id)) =
                            (row_scope, select_target_id.as_ref())
                        {
                            scope.record_select_focus_owner(target_id, label.id);
                        }
                    }
                    ui.push_id(("clipboard-history-row-controls", entry_token), |ui| {
                        ui.horizontal_wrapped(|ui| {
                            let edit_button = ui.button("Edit");
                            let edit_identity = format!("entry/{entry_token}/edit");
                            let edit_directive = register_clipboard_target(
                                row_scope,
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
                                && self.snapshot_current
                            {
                                self.edit_idx = Some(idx);
                                self.text = entry.clone();
                                self.advance_target_generation();
                            }
                            let remove_response = ui.button("Remove");
                            let remove_identity = format!("entry/{entry_token}/remove");
                            let remove_directive = register_clipboard_target(
                                row_scope,
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
                            if with_button_emphasis(
                                ui,
                                remove_response,
                                ButtonEmphasis::Destructive,
                            )
                            .clicked()
                                && remove_directive.is_none()
                                && authorized_owner_pass
                                && self.snapshot_current
                            {
                                remove = Some(idx);
                            }
                        });
                    });
                }
            });

        if let Some(idx) = remove.filter(|_| authorized_owner_pass && self.snapshot_current) {
            if idx < self.entries.len() {
                self.entries.remove(idx);
                self.renew_entry_tokens();
            }
            effects.save = true;
        }
        #[cfg(test)]
        {
            effects.browse_scroll_rect = Some(browse_output.inner_rect);
            effects.browse_scroll_clip_rect = Some(browse_scroll_clip);
            effects.browse_scroll_content_size = Some(browse_output.content_size);
            effects.browse_scroll_id = Some(browse_output.id);
        }
        let _ = browse_output;
        ui.separator();
        #[cfg(test)]
        let mut browse_footer_clip = egui::Rect::NOTHING;
        let footer_output =
            clipboard_history_footer_ui(ui, "clipboard_history_browse_footer", &["Close"], |ui| {
                #[cfg(test)]
                {
                    browse_footer_clip = ui.clip_rect();
                }
                let close_button = ui.button("Close");
                let close_directive = register_clipboard_target(
                    scope,
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
            observed_clipboard_version: Some(crate::plugins::clipboard::clipboard_version()),
            snapshot_current: true,
            edit_idx: None,
            text: long_entry.clone(),
            test_visible_entry_rows: Vec::new(),
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
                    assert!(!effects.close && !effects.save);
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
                    let effects = dialog.edit_ui(ui, 0, true);
                    assert!(!effects.save && !effects.close && !effects.invalid_text);
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
                    let effects = dialog.edit_ui(ui, 0, true);
                    assert!(!effects.save && !effects.close && !effects.invalid_text);
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
