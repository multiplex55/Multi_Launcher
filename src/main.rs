mod actions;
mod gui;
mod hotkey;
mod launcher;

use crate::actions::load_actions;
use crate::gui::LauncherApp;
use crate::hotkey::HotkeyTrigger;

use eframe::egui;

fn main() -> anyhow::Result<()> {
    let actions = load_actions("actions.json")?;
    let trigger = HotkeyTrigger::new();
    trigger.start_listener();

    loop {
        if trigger.take() {
            let native_options = eframe::NativeOptions {
                viewport: egui::ViewportBuilder::default()
                    .with_inner_size([400.0, 220.0])
                    .with_min_inner_size([320.0, 160.0])
                    .with_always_on_top(),
                ..Default::default()
            };

            let actions_for_window = actions.clone();
            let _ = eframe::run_native(
                "Multi_LNCHR",
                native_options,
                Box::new(move |_cc| Box::new(LauncherApp::new(actions_for_window))),
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}
