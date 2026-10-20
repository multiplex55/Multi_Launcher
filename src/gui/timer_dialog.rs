use crate::gui::LauncherApp;
use crate::gui::theme::widgets::{ButtonEmphasis, secondary_help, with_button_emphasis};
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

#[cfg(feature = "qa-framebuffer-capture")]
pub(crate) struct TimerDialogQaObservation {
    pub(crate) open: bool,
    pub(crate) mode: &'static str,
    pub(crate) duration_text_chars: usize,
    pub(crate) label_text_chars: usize,
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
enum Mode {
    #[default]
    Timer,
    Alarm,
}

impl TimerDialog {
    #[cfg(feature = "qa-framebuffer-capture")]
    pub(crate) fn qa_observation(&self) -> TimerDialogQaObservation {
        let (mode, value) = match self.mode {
            Mode::Timer => ("timer", self.duration.as_str()),
            Mode::Alarm => ("alarm", self.time.as_str()),
        };
        TimerDialogQaObservation {
            open: self.open,
            mode,
            duration_text_chars: value.chars().count(),
            label_text_chars: self.label.chars().count(),
        }
    }

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

    pub(crate) fn jump_mode_window_title(&self) -> Option<&'static str> {
        self.open.then(|| self.title())
    }

    #[cfg(test)]
    pub(crate) fn test_duration(&self) -> &str {
        &self.duration
    }

    fn duration_ui(&mut self, ui: &mut egui::Ui) -> egui::Response {
        let response = ui.add(
            egui::TextEdit::singleline(&mut self.duration)
                .desired_width(ui.available_width().min(220.0).max(72.0)),
        );
        if self.focus_duration_next_frame {
            response.request_focus();
            self.focus_duration_next_frame = false;
        }
        response
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        self.ui_with_jump_mode(ctx, app, None);
    }

    pub(crate) fn ui_with_jump_mode(
        &mut self,
        ctx: &egui::Context,
        app: &mut LauncherApp,
        jump_scope: Option<crate::gui::jump_mode::ui::JumpModeScope>,
    ) {
        if !self.open {
            return;
        }
        let mut open_val = self.open;
        let mut close = false;
        let mut jump_visual_order = 0;
        egui::Window::new(self.title())
            .open(&mut open_val)
            .resizable(false)
            .max_width(ctx.content_rect().width().max(1.0))
            .show(ctx, |ui| {
                match self.mode {
                    Mode::Timer => {
                        ui.horizontal_wrapped(|ui| {
                            let label = ui.label("Duration");
                            let response = self.duration_ui(ui).labelled_by(label.id);
                            register_timer_jump_target(
                                jump_scope.as_ref(),
                                ui,
                                &response,
                                "duration",
                                "Duration",
                                crate::gui::jump_mode::TargetKind::TextEntry,
                                &mut jump_visual_order,
                            );
                        });
                        secondary_help(ui, "Use Ns, Nm, Nh, mm:ss, or hh:mm:ss.");
                    }
                    Mode::Alarm => {
                        ui.horizontal_wrapped(|ui| {
                            let label = ui.label("Time");
                            let response = ui
                                .add(
                                    egui::TextEdit::singleline(&mut self.time)
                                        .desired_width(ui.available_width().min(220.0).max(72.0)),
                                )
                                .labelled_by(label.id);
                            register_timer_jump_target(
                                jump_scope.as_ref(),
                                ui,
                                &response,
                                "time",
                                "Time",
                                crate::gui::jump_mode::TargetKind::TextEntry,
                                &mut jump_visual_order,
                            );
                        });
                        secondary_help(ui, "Use HH:MM, Nd HH:MM, or YYYY-MM-DD HH:MM.");
                    }
                }
                ui.horizontal_wrapped(|ui| {
                    let label = ui.label("Name");
                    let response = ui
                        .add(
                            egui::TextEdit::singleline(&mut self.label)
                                .desired_width(ui.available_width().min(220.0).max(72.0)),
                        )
                        .labelled_by(label.id);
                    register_timer_jump_target(
                        jump_scope.as_ref(),
                        ui,
                        &response,
                        "name",
                        "Name",
                        crate::gui::jump_mode::TargetKind::TextEntry,
                        &mut jump_visual_order,
                    );
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label("Sound");
                    let sound_combo = egui::ComboBox::from_id_salt("sound_select")
                        .width(ui.available_width().min(160.0).max(72.0))
                        .selected_text(crate::sound::SOUND_NAMES[self.sound_idx])
                        .show_ui(ui, |ui| {
                            let popup_scope = jump_scope
                                .as_ref()
                                .map(|scope| scope.combo_popup_scope("sound"));
                            for (idx, name) in crate::sound::SOUND_NAMES.iter().enumerate() {
                                let option = ui.selectable_value(&mut self.sound_idx, idx, *name);
                                if let Some(scope) = popup_scope.as_ref() {
                                    register_timer_jump_target(
                                        Some(scope),
                                        ui,
                                        &option,
                                        format!("sound/option/{idx}"),
                                        format!("Sound: {name}"),
                                        crate::gui::jump_mode::TargetKind::Selectable,
                                        &mut jump_visual_order,
                                    );
                                }
                            }
                        });
                    register_timer_jump_target(
                        jump_scope.as_ref(),
                        ui,
                        &sound_combo.response,
                        "sound",
                        "Sound",
                        crate::gui::jump_mode::TargetKind::ComboBox,
                        &mut jump_visual_order,
                    );
                    let play_button = ui.button("Play");
                    register_timer_jump_target(
                        jump_scope.as_ref(),
                        ui,
                        &play_button,
                        "play",
                        "Play",
                        crate::gui::jump_mode::TargetKind::Button,
                        &mut jump_visual_order,
                    );
                    if play_button.clicked() {
                        let name = crate::sound::SOUND_NAMES[self.sound_idx];
                        if name == "None" {
                            if app.enable_toasts {
                                app.add_toast(Toast {
                                    text: "'None' is not a valid sound".into(),
                                    kind: ToastKind::Error,
                                    options: ToastOptions::default()
                                        .duration_in_seconds(app.toast_duration as f64),

                                    style: Default::default(),
                                });
                            }
                        } else {
                            crate::sound::play_sound(name);
                        }
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    let start_button = ui.button("Start");
                    let start_button =
                        with_button_emphasis(ui, start_button, ButtonEmphasis::Primary);
                    register_timer_jump_target(
                        jump_scope.as_ref(),
                        ui,
                        &start_button,
                        "start",
                        "Start",
                        crate::gui::jump_mode::TargetKind::Button,
                        &mut jump_visual_order,
                    );
                    if start_button.clicked() {
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
                    let cancel_button = ui.button("Cancel");
                    register_timer_jump_target(
                        jump_scope.as_ref(),
                        ui,
                        &cancel_button,
                        "cancel",
                        "Cancel",
                        crate::gui::jump_mode::TargetKind::Button,
                        &mut jump_visual_order,
                    );
                    if cancel_button.clicked() {
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

fn register_timer_jump_target(
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

#[cfg(test)]
mod tests {
    use super::{Mode, TimerDialog};
    use eframe::egui;

    fn render_duration_frame(dialog: &mut TimerDialog, ctx: &egui::Context) {
        ctx.run_ui(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| dialog.duration_ui(ui));
        })
        .drop_without_applying_deltas();
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
