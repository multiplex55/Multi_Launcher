use crate::gui::LauncherApp;
use crate::gui::theme::widgets::compact_section_heading;
use eframe::egui;
use std::time::Instant;
use sysinfo::{MINIMUM_CPU_UPDATE_INTERVAL, ProcessRefreshKind, ProcessesToUpdate, System};

#[derive(Default)]
pub struct CpuListDialog {
    pub open: bool,
    count: usize,
    system: System,
    last_refresh: Option<Instant>,
}

impl CpuListDialog {
    pub fn open(&mut self, count: usize) {
        self.count = count;
        self.system = System::new_all();
        self.last_refresh = Some(Instant::now() - MINIMUM_CPU_UPDATE_INTERVAL);
        self.open = true;
    }

    fn refresh(&mut self) {
        if let Some(last) = self.last_refresh
            && last.elapsed() >= MINIMUM_CPU_UPDATE_INTERVAL
        {
            self.system.refresh_processes_specifics(
                ProcessesToUpdate::All,
                true,
                ProcessRefreshKind::everything(),
            );
            self.system.refresh_cpu_usage();
            self.last_refresh = Some(Instant::now());
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context, _app: &mut LauncherApp) {
        if !self.open {
            return;
        }
        self.refresh();
        let mut close = false;
        egui::Window::new("CPU Usage")
            .resizable(true)
            .max_width(ctx.content_rect().width().max(1.0))
            .max_height(ctx.content_rect().height().max(1.0))
            .open(&mut self.open)
            .show(ctx, |ui| {
                compact_section_heading(ui, "Processes", None);
                let process_list_height =
                    (ui.available_height() - cpu_close_footer_reserved_height(ui)).max(0.0);

                let mut processes: Vec<_> = self.system.processes().values().collect();
                processes.sort_by(|a, b| {
                    b.cpu_usage()
                        .partial_cmp(&a.cpu_usage())
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                let cpu_count = self.system.cpus().len() as f32;
                egui::ScrollArea::vertical()
                    .id_salt("cpu_process_list")
                    .auto_shrink([true, true])
                    .min_scrolled_height(0.0)
                    .max_height(process_list_height)
                    .show(ui, |ui| {
                        for process in processes.into_iter().take(self.count) {
                            let usage = if cpu_count > 0.0 {
                                process.cpu_usage() / cpu_count
                            } else {
                                0.0
                            };
                            ui.add(
                                egui::Label::new(format!(
                                    "{usage:.1}% - {}",
                                    process.name().to_string_lossy()
                                ))
                                .wrap(),
                            );
                        }
                    });

                ui.separator();
                ui.horizontal_wrapped(|ui| {
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

fn cpu_close_footer_reserved_height(ui: &mut egui::Ui) -> f32 {
    let spacing = ui.spacing().clone();
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
    let button_frame = style
        .button_style(
            &egui::widget_style::Classes::default(),
            egui::widget_style::WidgetState::Inactive,
        )
        .frame;
    let button_height = (ui.fonts_mut(|fonts| fonts.row_height(&font_id))
        + spacing.extra_text_line_spacing
        + button_frame.total_margin().sum().y)
        .max(spacing.interact_size.y);
    let separator_height = style
        .separator_style(
            &egui::widget_style::Classes::default(),
            egui::widget_style::WidgetState::Noninteractive,
        )
        .spacing;
    let height = button_height + 2.0 * spacing.item_spacing.y + separator_height;
    let pixels_per_point = ui.ctx().pixels_per_point().max(f32::EPSILON);
    (height * pixels_per_point).ceil() / pixels_per_point
}
