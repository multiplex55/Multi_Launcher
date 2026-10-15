use crate::gui::LauncherApp;
use crate::gui::theme::widgets::{ButtonEmphasis, secondary_help, with_button_emphasis};
use crate::plugins::tempfile::create_named_file;
use eframe::egui;
use egui_toast::{Toast, ToastKind, ToastOptions};

#[derive(Default)]
pub struct TempfileDialog {
    pub open: bool,
    alias: String,
    text: String,
    #[cfg(test)]
    test_layout: Option<TempfileDialogLayout>,
}

#[cfg(test)]
#[derive(Clone, Copy)]
struct TempfileDialogLayout {
    window_rect: egui::Rect,
    alias_rect: egui::Rect,
    editor_clip_rect: egui::Rect,
    editor_viewport_clip_rect: egui::Rect,
    editor_content_size: egui::Vec2,
    editor_scroll_id: egui::Id,
    editor_scroll_offset: egui::Vec2,
    footer_clip_rect: egui::Rect,
    save_rect: egui::Rect,
    cancel_rect: egui::Rect,
}

impl TempfileDialog {
    pub fn open(&mut self) {
        self.open = true;
        self.alias.clear();
        self.text.clear();
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        if !self.open {
            return;
        }
        let mut close = false;
        #[cfg(test)]
        let window_id = egui::Id::new("Create Temp File");
        #[cfg(test)]
        let mut test_layout = None;
        let window_output = egui::Window::new("Create Temp File")
            .open(&mut self.open)
            .resizable(true)
            .default_size((360.0, 240.0))
            .min_width(200.0)
            .min_height(150.0)
            .show(ctx, |ui| {
                #[cfg(test)]
                let mut alias_rect = egui::Rect::NOTHING;
                let alias = ui.horizontal_wrapped(|ui| {
                    let label = ui.label("Alias");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.alias)
                            .desired_width(ui.available_width().min(320.0).max(72.0)),
                    )
                    .labelled_by(label.id)
                });
                #[cfg(test)]
                {
                    alias_rect = alias.inner.rect;
                }

                secondary_help(
                    ui,
                    "Save creates a text file and opens it in its associated application.",
                );
                ui.label("Text");

                let footer_height = tempfile_footer_reserved_height(ui, &["Save", "Cancel"]);
                let editor_max_height = (ui.available_height() - footer_height).max(0.0);
                #[cfg(test)]
                let mut editor_parent_clip_rect = egui::Rect::NOTHING;
                let editor_output = egui::ScrollArea::vertical()
                    .id_salt("tempfile_text_scroll")
                    .auto_shrink([false, false])
                    .min_scrolled_height(0.0)
                    .max_height(editor_max_height)
                    .show(ui, |ui| {
                        #[cfg(test)]
                        {
                            editor_parent_clip_rect = ui.clip_rect();
                        }
                        egui::TextEdit::multiline(&mut self.text)
                            .id_salt("tempfile_text_input")
                            .desired_width(f32::INFINITY)
                            .show(ui)
                    });

                ui.separator();
                #[cfg(test)]
                let mut footer_clip_rect = egui::Rect::NOTHING;
                #[cfg(test)]
                let mut save_rect = egui::Rect::NOTHING;
                #[cfg(test)]
                let mut cancel_rect = egui::Rect::NOTHING;
                ui.horizontal_wrapped(|ui| {
                    #[cfg(test)]
                    {
                        footer_clip_rect = ui.clip_rect();
                    }
                    let save = ui.button("Save");
                    let save = with_button_emphasis(ui, save, ButtonEmphasis::Primary);
                    #[cfg(test)]
                    {
                        save_rect = save.rect;
                    }
                    if save.clicked() {
                        if self.alias.trim().is_empty() {
                            app.report_error_message("ui operation", "Alias required");
                        } else {
                            match create_named_file(&self.alias, &self.text) {
                                Ok(path) => {
                                    if let Err(e) = open::that(&path) {
                                        app.report_error_message(
                                            "ui operation",
                                            format!("Failed to open: {e}"),
                                        );
                                    } else {
                                        if app.enable_toasts {
                                            app.add_toast(Toast {
                                                text: format!(
                                                    "Created {}",
                                                    path.file_name()
                                                        .map(|n| n.to_string_lossy())
                                                        .unwrap_or_else(|| path.to_string_lossy())
                                                )
                                                .into(),
                                                kind: ToastKind::Success,
                                                options: ToastOptions::default()
                                                    .duration_in_seconds(app.toast_duration as f64),

                                                style: Default::default(),
                                            });
                                        }
                                        close = true;
                                        app.search();
                                        app.focus_input();
                                    }
                                }
                                Err(e) => app.report_error_message(
                                    "ui operation",
                                    format!("Failed to save: {e}"),
                                ),
                            }
                        }
                    }
                    let cancel = ui.button("Cancel");
                    #[cfg(test)]
                    {
                        cancel_rect = cancel.rect;
                    }
                    if cancel.clicked() {
                        close = true;
                    }
                });

                #[cfg(test)]
                {
                    test_layout = Some(TempfileDialogLayout {
                        window_rect: egui::Rect::NOTHING,
                        alias_rect,
                        editor_clip_rect: editor_output
                            .inner
                            .text_clip_rect
                            .intersect(editor_parent_clip_rect),
                        editor_viewport_clip_rect: editor_parent_clip_rect,
                        editor_content_size: editor_output.content_size,
                        editor_scroll_id: editor_output.id,
                        editor_scroll_offset: editor_output.state.offset,
                        footer_clip_rect,
                        save_rect,
                        cancel_rect,
                    });
                }
                #[cfg(not(test))]
                let _ = (alias, editor_output);
            });

        #[cfg(test)]
        {
            self.test_layout = match (window_output.as_ref(), test_layout) {
                (Some(window), Some(layout)) => Some(TempfileDialogLayout {
                    window_rect: ctx
                        .memory(|memory| memory.area_rect(window_id))
                        .unwrap_or(window.response.rect),
                    ..layout
                }),
                _ => None,
            };
        }
        #[cfg(not(test))]
        let _ = window_output;
        if close {
            self.open = false;
        }
    }
}

fn tempfile_footer_reserved_height(ui: &mut egui::Ui, labels: &[&str]) -> f32 {
    let spacing = ui.spacing().clone();
    let style = ui.style().clone();
    let native_button_style = style.button_style(
        &egui::widget_style::Classes::default(),
        egui::widget_style::WidgetState::Inactive,
    );
    let font_id = style
        .override_font_id
        .clone()
        .or_else(|| {
            style
                .override_text_style
                .clone()
                .map(|text_style| text_style.resolve(&style))
        })
        .unwrap_or_else(|| native_button_style.text_style.font_id.clone());
    let button_frame = native_button_style.frame;
    let button_height = (ui.fonts_mut(|fonts| fonts.row_height(&font_id))
        + spacing.extra_text_line_spacing
        + button_frame.total_margin().sum().y)
        .max(spacing.interact_size.y);
    let available_width = ui.available_width().max(1.0);
    let text_color = ui.visuals().text_color();
    let mut row_width = 0.0;
    let mut row_count = 1_usize;
    for label in labels {
        let text_width = ui.fonts_mut(|fonts| {
            fonts
                .layout_no_wrap((*label).to_owned(), font_id.clone(), text_color)
                .size()
                .x
        });
        let button_width =
            (text_width + button_frame.total_margin().sum().x).max(spacing.interact_size.x);
        let gap = if row_width == 0.0 {
            0.0
        } else {
            spacing.item_spacing.x
        };
        if row_width > 0.0 && row_width + gap + button_width > available_width {
            row_count += 1;
            row_width = button_width;
        } else {
            if row_width > 0.0 {
                row_width += spacing.item_spacing.x;
            }
            row_width += button_width;
        }
    }
    let separator_height = style
        .separator_style(
            &egui::widget_style::Classes::default(),
            egui::widget_style::WidgetState::Noninteractive,
        )
        .spacing;
    let height = row_count as f32 * button_height
        + row_count.saturating_sub(1) as f32 * spacing.item_spacing.y
        + 2.0 * spacing.item_spacing.y
        + separator_height;
    let pixels_per_point = ui.pixels_per_point().max(f32::EPSILON);
    (height * pixels_per_point).ceil() / pixels_per_point
}

#[cfg(test)]
mod tests {
    use super::{TempfileDialog, TempfileDialogLayout};
    use eframe::egui;

    fn test_app(ctx: &egui::Context, root: &std::path::Path) -> crate::gui::LauncherApp {
        use crate::plugin::PluginManager;
        use crate::settings::Settings;
        use std::sync::{Arc, atomic::AtomicBool};

        crate::gui::LauncherApp::new(
            ctx,
            Arc::new(Vec::new()),
            0,
            PluginManager::new_inert_for_test(),
            root.join("actions.json").to_string_lossy().into_owned(),
            root.join("settings.json").to_string_lossy().into_owned(),
            Settings::default(),
            None,
            None,
            None,
            None,
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        )
    }

    fn render_frame(
        ctx: &egui::Context,
        dialog: &mut TempfileDialog,
        app: &mut crate::gui::LauncherApp,
        screen_rect: egui::Rect,
    ) -> egui::FullOutput {
        ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(screen_rect),
                focused: true,
                ..Default::default()
            },
            |ctx| dialog.ui(ctx, app),
        )
    }

    #[test]
    fn long_text_scrolls_above_reachable_footer_without_render_mutation() {
        let directory = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        let mut app = test_app(&ctx, directory.path());
        let text = (0..96)
            .map(|line| format!("第 {line} 行 — café 🌱\n{}", "Unicode текст ".repeat(8)))
            .collect::<Vec<_>>()
            .join("\n");
        let mut dialog = TempfileDialog {
            open: true,
            alias: "draft alias".into(),
            text: text.clone(),
            ..Default::default()
        };
        let screen_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(360.0, 270.0));

        for _ in 0..3 {
            render_frame(&ctx, &mut dialog, &mut app, screen_rect).drop_without_applying_deltas();
        }

        let layout = dialog
            .test_layout
            .expect("the default Tempfile window rendered its layout");
        let initial_text_unchanged = dialog.text == text;
        let initial_alias_unchanged = dialog.alias == "draft alias";
        let scroll_range =
            (layout.editor_content_size.y - layout.editor_viewport_clip_rect.height()).max(0.0);
        assert!(
            screen_rect.contains_rect(layout.window_rect),
            "the default Tempfile window remains inside the constrained viewport"
        );
        assert!(layout.editor_clip_rect.is_positive());
        assert!(layout.editor_viewport_clip_rect.is_positive());
        assert!(
            layout
                .window_rect
                .contains_rect(layout.editor_viewport_clip_rect),
            "the actual editor viewport remains inside the window"
        );
        assert!(
            layout.editor_content_size.y > layout.editor_viewport_clip_rect.height(),
            "the multiline editor has real vertical scroll range"
        );
        assert!(initial_text_unchanged && initial_alias_unchanged && dialog.open);
        assert!(layout.window_rect.contains_rect(layout.alias_rect));
        assert!(screen_rect.contains_rect(layout.alias_rect));
        for action in [layout.save_rect, layout.cancel_rect] {
            assert!(layout.window_rect.contains_rect(action));
            assert!(screen_rect.contains_rect(action));
            assert!(layout.footer_clip_rect.contains_rect(action));
        }
        assert!(
            layout.editor_viewport_clip_rect.bottom() <= layout.save_rect.top(),
            "the actual editor clip ends above the reserved action footer"
        );
        assert!(!layout.save_rect.intersects(layout.cancel_rect));

        let mut scroll_state = egui::scroll_area::State::load(&ctx, layout.editor_scroll_id)
            .expect("the multiline editor stores its scroll state");
        scroll_state.offset.y = scroll_range.min(72.0);
        scroll_state.store(&ctx, layout.editor_scroll_id);
        let output = render_frame(&ctx, &mut dialog, &mut app, screen_rect);
        let scrolled_layout = dialog.test_layout;
        let scrolled_text_unchanged = dialog.text == text;
        let scrolled_alias_unchanged = dialog.alias == "draft alias";
        output.drop_without_applying_deltas();

        let scrolled_layout =
            scrolled_layout.expect("the Tempfile layout is rendered after scrolling");
        assert!(scrolled_layout.editor_scroll_offset.y > 0.0);
        assert_eq!(scrolled_layout.editor_scroll_id, layout.editor_scroll_id);
        assert!(scrolled_text_unchanged && scrolled_alias_unchanged && dialog.open);
        assert!(screen_rect.contains_rect(scrolled_layout.save_rect));
        assert!(screen_rect.contains_rect(scrolled_layout.cancel_rect));
    }
}
