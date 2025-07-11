use crate::gui::LauncherApp;
use crate::plugins::timer::{parse_duration, parse_hhmm, start_alarm_named, start_timer_named};
use eframe::egui;
#[derive(Default)]
pub struct TimerDialog {
    pub open: bool,
    mode: Mode,
    duration: String,
    time: String,
    label: String,
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
    }

    pub fn open_alarm(&mut self) {
        self.mode = Mode::Alarm;
        self.open = true;
        self.time.clear();
        self.label.clear();
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        if !self.open { return; }
        let mut open_val = self.open;
        let mut close = false;
        egui::Window::new(match self.mode { Mode::Timer => "Create Timer", Mode::Alarm => "Set Alarm" })
            .open(&mut open_val)
            .resizable(false)
            .show(ctx, |ui| {
                match self.mode {
                    Mode::Timer => {
                        ui.horizontal(|ui| {
                            ui.label("Duration (Ns/Nm/Nh)");
                            ui.text_edit_singleline(&mut self.duration);
                        });
                    }
                    Mode::Alarm => {
                        ui.horizontal(|ui| {
                            ui.label("Time (HH:MM)");
                            ui.text_edit_singleline(&mut self.time);
                        });
                    }
                }
                ui.horizontal(|ui| {
                    ui.label("Name");
                    ui.text_edit_singleline(&mut self.label);
                });
                ui.horizontal(|ui| {
                    if ui.button("Start").clicked() {
                        match self.mode {
                            Mode::Timer => {
                                if let Some(d) = parse_duration(&self.duration) {
                                    start_timer_named(d, if self.label.is_empty() { None } else { Some(self.label.clone()) });
                                    close = true;
                                    app.focus_input();
                                } else {
                                    app.error = Some("Invalid duration".into());
                                }
                            }
                            Mode::Alarm => {
                                if let Some((h,m)) = parse_hhmm(&self.time) {
                                    start_alarm_named(h, m, if self.label.is_empty(){None}else{Some(self.label.clone())});
                                    close = true;
                                    app.error = Some("Invalid time".into());
                                }
                            }
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
            });
        if close { open_val = false; }
        self.open = open_val;
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
        if !self.open { return; }
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
        if close { open_val = false; }
        self.open = open_val;
    }
}
