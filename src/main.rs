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
    let visible_flag = Arc::new(AtomicBool::new(false));
    let flag_clone = visible_flag.clone();

    let handle = thread::spawn(move || {
        let native_options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([400.0, 220.0])
                .with_min_inner_size([320.0, 160.0])
                .with_always_on_top()
                .with_visible(false),
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

    (handle, visible_flag)
}

fn main() -> anyhow::Result<()> {
    logging::init();
    let settings = Settings::load("settings.json").unwrap_or_default();
    tracing::debug!(?settings, "settings loaded");
    let mut actions = load_actions("actions.json").unwrap_or_default();
    tracing::debug!("{} actions loaded", actions.len());

    if let Some(paths) = &settings.index_paths {
        actions.extend(indexer::index_paths(paths));
    }

    let trigger_key = settings.hotkey_key();
    tracing::debug!(?trigger_key, "configuring hotkey");
    let trigger = HotkeyTrigger::new(trigger_key);
    trigger.start_listener();


    let (handle, visibility) = spawn_gui(actions.clone(), &settings);
    visibility.store(false, Ordering::SeqCst);

    loop {
        if handle.is_finished() {
            tracing::error!("gui thread terminated unexpectedly");
            let _ = handle.join();
            break Ok(());
        }

        if trigger.take() {
            let next = !visibility.load(Ordering::SeqCst);
            visibility.store(next, Ordering::SeqCst);
            tracing::debug!("toggle visible -> {}", next);
        }

        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}
