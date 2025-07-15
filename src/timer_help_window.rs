use eframe::egui;

#[derive(Default)]
pub struct TimerHelpWindow {
    pub open: bool,
}

impl TimerHelpWindow {
    pub fn ui(&mut self, ctx: &egui::Context) {
        if !self.open {
            return;
        }
        let mut open = self.open;
        egui::Window::new("Timer Help")
            .open(&mut open)
            .resizable(true)
            .default_size((400.0, 220.0))
            .show(ctx, |ui| {
                ui.heading("Timer Plugin Usage");
                ui.separator();
                ui.label("Create a timer: use 'timer add <duration> [name]'. Examples:");
                ui.monospace("timer add 10s tea");
                ui.monospace("timer add 5m");
                ui.monospace("timer add 1:30");
                ui.label(
                    "Supported units are seconds (s), minutes (m) and hours (h). \
You can also use hh:mm:ss or mm:ss notation.",
                );
                ui.separator();
                ui.label("Set an alarm: use 'alarm <HH:MM> [name]'. Example:");
                ui.monospace("alarm 07:30 wake up");
                ui.separator();
                ui.label("Manage timers and alarms:");
                ui.monospace("timer list  # show active timers");
                ui.monospace("alarm list  # show active alarms");
                ui.monospace("timer pause <id>  # pause a timer");
                ui.monospace("timer resume <id>  # resume a timer");
                ui.monospace("timer cancel  # cancel timers/alarms");
                ui.monospace("timer rm  # remove timers");
            });
        self.open = open;
    }
}
