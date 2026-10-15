use crate::toast_log::TOAST_LOG_FILE;
use eframe::egui;

#[derive(Default)]
pub struct ToastLogDialog {
    pub open: bool,
    lines: Vec<String>,
}

impl ToastLogDialog {
    pub fn open(&mut self) {
        self.lines = Self::read_last_lines(TOAST_LOG_FILE, 20);
        self.open = true;
    }

    fn read_last_lines(path: &str, count: usize) -> Vec<String> {
        let content = std::fs::read_to_string(path).unwrap_or_default();
        let mut lines: Vec<String> = content.lines().map(|s| s.to_owned()).collect();
        if lines.len() > count {
            lines.drain(0..lines.len() - count);
        }
        lines
    }

    pub fn ui(&mut self, ctx: &egui::Context, _app: &mut crate::gui::LauncherApp) {
        if !self.open {
            return;
        }
        let mut close = false;
        egui::Window::new("Toast Log")
            .resizable(true)
            .default_size((360.0, 200.0))
            .open(&mut self.open)
            .show(ctx, |ui| {
                let footer_height = toast_log_footer_reserved_height(ui);
                egui::ScrollArea::vertical()
                    .id_salt("toast_log_content")
                    .min_scrolled_height(0.0)
                    .max_height((ui.available_height() - footer_height).max(0.0))
                    .show(ui, |ui| {
                        for line in &self.lines {
                            ui.label(line);
                        }
                    });
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Refresh").clicked() {
                        self.lines = Self::read_last_lines(TOAST_LOG_FILE, 20);
                    }
                    if ui.button("Close").clicked() {
                        close = true;
                    }
                });
            });
        if close {
            self.open = false;
        }
    }
}

fn toast_log_footer_reserved_height(ui: &mut egui::Ui) -> f32 {
    let spacing = ui.spacing().clone();
    let style = ui.style().clone();
    let native_button_style = style.button_style(
        &egui::widget_style::Classes::default(),
        egui::widget_style::WidgetState::Inactive,
    );
    let button_font = style
        .override_font_id
        .clone()
        .or_else(|| {
            style
                .override_text_style
                .as_ref()
                .map(|text_style| text_style.resolve(&style))
        })
        .unwrap_or_else(|| native_button_style.text_style.font_id.clone());
    let frame_margin = if ui.visuals().button_frame {
        native_button_style.frame.total_margin().sum()
    } else {
        egui::Vec2::ZERO
    };
    let available_width = ui.available_width().max(1.0);
    let text_width = (available_width - frame_margin.x).max(1.0);
    let wrap_mode = ui.wrap_mode();
    let mut row_width = 0.0;
    let mut row_height = 0.0f32;
    let mut actions_height = 0.0;

    for label in ["Refresh", "Close"] {
        let text_size = egui::WidgetText::from(label)
            .into_galley(ui, Some(wrap_mode), text_width, button_font.clone())
            .size();
        let button_width = (text_size.x + frame_margin.x)
            .max(spacing.interact_size.x)
            .min(available_width);
        let button_height = (text_size.y + spacing.extra_text_line_spacing + frame_margin.y)
            .max(spacing.interact_size.y);
        if row_width > 0.0 && row_width + spacing.item_spacing.x + button_width > available_width {
            actions_height += row_height + spacing.item_spacing.y;
            row_width = button_width;
            row_height = button_height;
        } else {
            if row_width > 0.0 {
                row_width += spacing.item_spacing.x;
            }
            row_width += button_width;
            row_height = row_height.max(button_height);
        }
    }
    actions_height += row_height.max(spacing.interact_size.y);
    let separator_spacing = style
        .separator_style(
            &egui::widget_style::Classes::default(),
            egui::widget_style::WidgetState::Noninteractive,
        )
        .spacing;
    let footer_height = actions_height + 2.0 * spacing.item_spacing.y + separator_spacing;
    let pixels_per_point = ui.ctx().pixels_per_point().max(f32::EPSILON);
    (footer_height * pixels_per_point).ceil() / pixels_per_point
}
