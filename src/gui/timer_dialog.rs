use crate::gui::LauncherApp;
use crate::plugins::timer::{parse_duration, parse_hhmm, start_alarm_named, start_timer_named};
use eframe::egui;
use egui_toast::{Toast, ToastKind, ToastOptions};
#[derive(Default)]
pub struct TimerDialog {
    pub open: bool,
    mode: Mode,
    duration: String,
    time: String,
    label: String,
    sound_idx: usize,
    focus_duration_next_frame: bool,
}

#[derive(Default, Clone, Copy, PartialEq)]
enum Mode {
    #[default]
    Timer,
    Alarm,
}

impl TimerDialog {
    pub fn open_timer(&mut self) {
        self.mode = Mode::Timer;
        self.open = true;
        self.duration.clear();
        self.label.clear();
        self.sound_idx = 0;
        self.focus_duration_next_frame = true;
    }

    pub fn open_alarm(&mut self) {
        self.focus_duration_next_frame = false;
        self.mode = Mode::Alarm;
        self.open = true;
        self.time.clear();
        self.label.clear();
        self.sound_idx = 0;
    }

    fn title(&self) -> &'static str {
        match self.mode {
            Mode::Timer => "Create Timer",
            Mode::Alarm => "Set Alarm",
        }
    }

    fn duration_ui(&mut self, ui: &mut egui::Ui) {
        let response = ui.text_edit_singleline(&mut self.duration);
        if self.focus_duration_next_frame {
            response.request_focus();
            self.focus_duration_next_frame = false;
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        if !self.open {
            return;
        }
        let mut open_val = self.open;
        let mut close = false;
        egui::Window::new(self.title())
            .open(&mut open_val)
            .resizable(false)
            .show(ctx, |ui| {
                match self.mode {
                    Mode::Timer => {
                        ui.horizontal(|ui| {
                            ui.label("Duration (Ns/Nm/Nh or hh:mm:ss)");
                            self.duration_ui(ui);
                        });
                    }
                    Mode::Alarm => {
                        ui.horizontal(|ui| {
                            ui.label("Time (HH:MM, Nd HH:MM or YYYY-MM-DD HH:MM)");
                            ui.text_edit_singleline(&mut self.time);
                        });
                    }
                }
                ui.horizontal(|ui| {
                    ui.label("Name");
                    ui.text_edit_singleline(&mut self.label);
                });
                ui.horizontal(|ui| {
                    ui.label("Sound");
                    egui::ComboBox::from_id_source("sound_select")
                        .selected_text(crate::sound::SOUND_NAMES[self.sound_idx])
                        .show_ui(ui, |ui| {
                            for (idx, name) in crate::sound::SOUND_NAMES.iter().enumerate() {
                                ui.selectable_value(&mut self.sound_idx, idx, *name);
                            }
                        });
                    if ui.button("Play").clicked() {
                        let name = crate::sound::SOUND_NAMES[self.sound_idx];
                        if name == "None" {
                            if app.enable_toasts {
                                app.add_toast(Toast {
                                    text: "'None' is not a valid sound".into(),
                                    kind: ToastKind::Error,
                                    options: ToastOptions::default()
                                        .duration_in_seconds(app.toast_duration as f64),
                                });
                            }
                        } else {
                            crate::sound::play_sound(name);
                        }
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button("Start").clicked() {
                        match self.mode {
                            Mode::Timer => {
                                if let Some(d) = parse_duration(&self.duration) {
                                    start_timer_named(
                                        d,
                                        if self.label.is_empty() {
                                            None
                                        } else {
                                            Some(self.label.clone())
                                        },
                                        crate::sound::SOUND_NAMES[self.sound_idx].to_string(),
                                    );
                                    close = true;
                                    app.focus_input();
                                } else {
                                    app.report_error_message("ui operation", "Invalid duration");
                                }
                            }
                            Mode::Alarm => {
                                if let Some((h, m, date)) = parse_hhmm(&self.time) {
                                    start_alarm_named(
                                        h,
                                        m,
                                        date,
                                        if self.label.is_empty() {
                                            None
                                        } else {
                                            Some(self.label.clone())
                                        },
                                        crate::sound::SOUND_NAMES[self.sound_idx].to_string(),
                                    );
                                    close = true;
                                } else {
                                    app.report_error_message("ui operation", "Invalid time");
                                }
                            }
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
            });
        if close {
            open_val = false;
        }
        self.open = open_val;
    }
}

#[cfg(test)]
mod tests {
    use super::{Mode, TimerDialog};
    use eframe::egui;

    fn render_duration_frame(dialog: &mut TimerDialog, ctx: &egui::Context) {
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| dialog.duration_ui(ui));
        });
    }

    #[test]
    fn open_timer_resets_timer_fields_and_arms_duration_focus() {
        let mut dialog = TimerDialog {
            duration: "5m".into(),
            time: "12:30".into(),
            label: "Tea".into(),
            sound_idx: 2,
            ..Default::default()
        };

        dialog.open_timer();

        assert_eq!(dialog.mode, Mode::Timer);
        assert!(dialog.open);
        assert!(dialog.duration.is_empty());
        assert!(dialog.label.is_empty());
        assert_eq!(dialog.sound_idx, 0);
        assert!(dialog.focus_duration_next_frame);
        assert_eq!(dialog.time, "12:30");
        assert_eq!(dialog.title(), "Create Timer");
    }

    #[test]
    fn duration_focus_request_is_consumed_once() {
        let ctx = egui::Context::default();
        let mut dialog = TimerDialog::default();
        dialog.open_timer();

        render_duration_frame(&mut dialog, &ctx);
        assert!(!dialog.focus_duration_next_frame);

        render_duration_frame(&mut dialog, &ctx);
        assert!(!dialog.focus_duration_next_frame);
    }

    #[test]
    fn open_alarm_clears_pending_timer_focus_and_keeps_alarm_title() {
        let mut dialog = TimerDialog::default();
        dialog.open_timer();
        assert!(dialog.focus_duration_next_frame);

        dialog.open_alarm();

        assert!(!dialog.focus_duration_next_frame);
        assert_eq!(dialog.mode, Mode::Alarm);
        assert!(dialog.open);
        assert_eq!(dialog.title(), "Set Alarm");

        dialog.open_timer();
        assert_eq!(dialog.title(), "Create Timer");
    }
}

#[derive(Default)]
pub struct TimerCompletionDialog {
    pub open: bool,
    msg: String,
}

impl TimerCompletionDialog {
    pub fn open_message(&mut self, msg: String) {
        self.msg = msg;
        self.open = true;
    }

    pub fn ui(&mut self, ctx: &egui::Context) {
        if !self.open {
            return;
        }
        let mut open_val = self.open;
        let mut close = false;
        egui::Window::new("Timer Finished")
            .collapsible(false)
            .resizable(false)
            .open(&mut open_val)
            .show(ctx, |ui| {
                ui.label(&self.msg);
                if ui.button("OK").clicked() {
                    close = true;
                }
            });
        if close {
            open_val = false;
        }
        self.open = open_val;
    }
}
