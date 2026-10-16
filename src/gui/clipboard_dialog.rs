use crate::gui::LauncherApp;
use crate::gui::theme::widgets::{ButtonEmphasis, compact_section_heading, with_button_emphasis};
use crate::gui::theme::{TextRole, text_style_for_role};
use crate::plugins::clipboard::{CLIPBOARD_FILE, load_history, save_history};
use eframe::egui;
use std::collections::VecDeque;

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
    edit_idx: Option<usize>,
    text: String,
}

impl ClipboardDialog {
    pub fn open(&mut self) {
        self.entries = load_history(CLIPBOARD_FILE).unwrap_or_default().into();
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
        self.entries = load_history(CLIPBOARD_FILE).unwrap_or_default().into();
        if idx < self.entries.len() {
            self.text = self.entries[idx].clone();
        } else {
            self.text.clear();
        }
        self.edit_idx = Some(idx);
        self.open = true;
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
        if !self.open {
            return;
        }
        let pinned = app.pinned_panels.contains(&super::Panel::ClipboardDialog);
        let mut window_open = self.open;
        let mut effects = ClipboardDialogEffects::default();
        egui::Window::new("Clipboard History")
            .open(&mut window_open)
            .resizable(true)
            .default_size((360.0, 240.0))
            .min_width(100.0)
            .min_height(100.0)
            .show(ctx, |ui| effects = self.contents_ui(ui));
        self.open = window_open || pinned;

        if effects.save {
            self.save(app);
        }
        if effects.close && !pinned {
            self.open = false;
        }
        if effects.invalid_text {
            app.report_error_message("ui operation", "Text required");
        }
    }

    fn contents_ui(&mut self, ui: &mut egui::Ui) -> ClipboardDialogEffects {
        if let Some(idx) = self.edit_idx {
            self.edit_ui(ui, idx)
        } else {
            self.browse_ui(ui)
        }
    }

    fn edit_ui(&mut self, ui: &mut egui::Ui, idx: usize) -> ClipboardDialogEffects {
        compact_section_heading(ui, "Text", None);
        let footer_height = clipboard_history_footer_reserved_height(ui, &["Save", "Cancel"]);
        let body_height = (ui.available_height() - footer_height).max(0.0);
        let edit_body_output = egui::ScrollArea::vertical()
            .id_salt("clipboard_history_edit_scroll")
            .auto_shrink([false, false])
            .min_scrolled_height(0.0)
            .max_height(body_height)
            .show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut self.text)
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
                let save_button = ui.button("Save");
                let save = with_button_emphasis(ui, save_button, ButtonEmphasis::Primary);
                if save.clicked() {
                    if self.text.trim().is_empty() {
                        effects.invalid_text = true;
                    } else {
                        if idx < self.entries.len() {
                            self.entries[idx] = self.text.clone();
                        }
                        self.edit_idx = None;
                        self.text.clear();
                        effects.save = true;
                    }
                }
                let cancel_button = ui.button("Cancel");
                if cancel_button.clicked() {
                    self.edit_idx = None;
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
        let mut effects = ClipboardDialogEffects::default();
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
                    let preview = clipboard_history_entry_preview(&entry);
                    let label = ui.add(egui::Label::new(preview).truncate());
                    let idx_copy = idx;
                    label.clone().context_menu(|ui| {
                        if ui.button("Edit Entry").clicked() {
                            self.edit_idx = Some(idx_copy);
                            self.text = self.entries[idx_copy].clone();
                            ui.close();
                        }
                        let remove_response = ui.button("Remove Entry");
                        if with_button_emphasis(ui, remove_response, ButtonEmphasis::Destructive)
                            .clicked()
                        {
                            remove = Some(idx_copy);
                            ui.close();
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        let edit_button = ui.button("Edit");
                        #[cfg(test)]
                        if idx == 0 {
                            effects.browse_edit_rect = Some(edit_button.rect);
                        }
                        if edit_button.clicked() {
                            self.edit_idx = Some(idx);
                            self.text = entry.clone();
                        }
                        let remove_response = ui.button("Remove");
                        #[cfg(test)]
                        if idx == 0 {
                            effects.browse_remove_rect = Some(remove_response.rect);
                        }
                        if with_button_emphasis(ui, remove_response, ButtonEmphasis::Destructive)
                            .clicked()
                        {
                            remove = Some(idx);
                        }
                    });
                }
            });

        if let Some(idx) = remove {
            self.entries.remove(idx);
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
                if close_button.clicked() {
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
        let directory = tempfile::tempdir().unwrap();
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(directory.path()).unwrap();

        let saved_history = VecDeque::from([
            "first saved item".to_owned(),
            "second saved item".to_owned(),
            "third saved item".to_owned(),
        ]);
        let expected_history = saved_history.iter().cloned().collect::<Vec<_>>();
        save_history(CLIPBOARD_FILE, &saved_history).unwrap();

        let ctx = egui::Context::default();
        let actions_path = directory.path().join("actions.json");
        let settings_path = directory.path().join("settings.json");
        let mut app = LauncherApp::new(
            &ctx,
            Arc::new(Vec::new()),
            0,
            PluginManager::new(),
            actions_path.to_string_lossy().into_owned(),
            settings_path.to_string_lossy().into_owned(),
            Settings::default(),
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

        std::env::set_current_dir(original_dir).unwrap();
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
            edit_idx: None,
            text: long_entry.clone(),
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
                    let effects = dialog.edit_ui(ui, 0);
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
                    let effects = dialog.edit_ui(ui, 0);
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
