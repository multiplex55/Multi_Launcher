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

use crate::actions::{load_actions, Action};
use crate::gui::LauncherApp;
use crate::hotkey::HotkeyTrigger;
use crate::plugin::PluginManager;
use crate::plugins_builtin::{CalculatorPlugin, WebSearchPlugin};
use crate::settings::Settings;

use eframe::egui;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::thread;

fn spawn_gui(
    actions: Vec<Action>,
    settings: &Settings,
) -> (thread::JoinHandle<()>, Arc<AtomicBool>) {
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
    let plugin_dirs = settings.plugin_dirs.clone();
    let index_paths = settings.index_paths.clone();
    let close_flag = Arc::new(AtomicBool::new(false));
    let flag_clone = close_flag.clone();

    let handle = thread::spawn(move || {
        let native_options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([400.0, 220.0])
                .with_min_inner_size([320.0, 160.0])
                .with_always_on_top(),
            event_loop_builder: Some(Box::new(|builder| {
                #[cfg(target_os = "windows")]
                {
                    use winit::platform::windows::EventLoopBuilderExtWindows;
                    builder.with_any_thread(true);
                }
                #[cfg(target_os = "linux")]
                {
                    use winit::platform::wayland::EventLoopBuilderExtWayland;
                    use winit::platform::x11::EventLoopBuilderExtX11;
                    winit::platform::x11::EventLoopBuilderExtX11::with_any_thread(builder, true);
                    winit::platform::wayland::EventLoopBuilderExtWayland::with_any_thread(builder, true);
                }
            })),
            ..Default::default()
        };

        let _ = eframe::run_native(
            "Multi_LNCHR",
            native_options,
            Box::new(move |_cc| {
                Box::new(LauncherApp::new(
                    actions_for_window,
                    plugins,
                    actions_path,
                    plugin_dirs,
                    index_paths,
                    flag_clone,
                ))
            }),
        );
    });

    (handle, close_flag)
}

fn main() -> anyhow::Result<()> {
    logging::init();
    let settings = Settings::load("settings.json").unwrap_or_default();
    let mut actions = load_actions("actions.json").unwrap_or_default();

    if let Some(paths) = &settings.index_paths {
        actions.extend(indexer::index_paths(paths));
    }

    let trigger = HotkeyTrigger::new(settings.hotkey_key());
    trigger.start_listener();


    let mut running: Option<(thread::JoinHandle<()>, Arc<AtomicBool>)> = None;
    let mut desired_visible = false;

    loop {
        if let Some((handle, flag)) = &running {
            if !desired_visible {
                flag.store(true, Ordering::SeqCst);
            }

            if handle.is_finished() {
                if let Some((handle, _)) = running.take() {
                    let _ = handle.join();
                }
            }
        }

        if running.is_none() && desired_visible {
            let (h, f) = spawn_gui(actions.clone(), &settings);
            running = Some((h, f));
        }

        if trigger.take() {
            desired_visible = !desired_visible;
        }

        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}
