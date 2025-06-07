mod actions;
mod actions_editor;
mod gui;
mod hotkey;
mod launcher;
mod plugin;
mod plugins_builtin;
mod indexer;
mod settings;
mod logging;

use crate::actions::load_actions;
use crate::gui::LauncherApp;
use crate::hotkey::HotkeyTrigger;
use crate::plugin::PluginManager;
use crate::plugins_builtin::{CalculatorPlugin, WebSearchPlugin};
use crate::settings::Settings;

use eframe::egui;

fn main() -> anyhow::Result<()> {
    logging::init();
    let settings = Settings::load("settings.json").unwrap_or_default();
    let mut actions = load_actions("actions.json").unwrap_or_default();

    if let Some(paths) = &settings.index_paths {
        actions.extend(indexer::index_paths(paths));
    }

    let trigger = HotkeyTrigger::new(settings.hotkey_key());
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
            let mut plugins = PluginManager::new();
            plugins.register(Box::new(WebSearchPlugin));
            plugins.register(Box::new(CalculatorPlugin));
            if let Some(dirs) = &settings.plugin_dirs {
                for dir in dirs {
                    if let Err(e) = plugins.load_dir(dir) {
                        tracing::error!("Failed to load plugins from {}: {}", dir, e);
                    }
                }
            }

            let actions_path = "actions.json".to_string();
            let _ = eframe::run_native(
                "Multi_LNCHR",
                native_options,
                Box::new(move |_cc| Box::new(LauncherApp::new(actions_for_window, plugins, actions_path))),
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}
