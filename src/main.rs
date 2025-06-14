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
mod visibility;

use crate::actions::{load_actions, Action};
use crate::gui::LauncherApp;
use crate::hotkey::HotkeyTrigger;
use crate::visibility::handle_visibility_trigger;
use crate::plugin::PluginManager;
use crate::plugins_builtin::{CalculatorPlugin, WebSearchPlugin};
use crate::settings::Settings;

use eframe::egui;
use std::sync::{Arc, atomic::AtomicBool, Mutex};
use std::thread;

fn spawn_gui(
    actions: Vec<Action>,
    settings: &Settings,
) -> (thread::JoinHandle<()>, Arc<AtomicBool>, Arc<Mutex<Option<egui::Context>>>) {
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
    let ctx_handle = Arc::new(Mutex::new(None));
    let ctx_clone = ctx_handle.clone();

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
            Box::new(move |cc| {
                *ctx_clone.lock().unwrap() = Some(cc.egui_ctx.clone());
                tracing::debug!("egui context stored");
                Box::new(LauncherApp::new(
                    &cc.egui_ctx,
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

    (handle, visible_flag, ctx_handle)
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

    let hotkey = settings.hotkey();
    tracing::debug!(?hotkey, "configuring hotkeys");
    let trigger = Arc::new(HotkeyTrigger::new(hotkey));
    let quit_trigger = settings.quit_hotkey().map(|hk| Arc::new(HotkeyTrigger::new(hk)));

    let mut watched = vec![trigger.clone()];
    if let Some(qt) = &quit_trigger {
        watched.push(qt.clone());
    }

    let listener = HotkeyTrigger::start_listener(watched, "main");


    let (handle, visibility, ctx) = spawn_gui(actions.clone(), &settings);
    let mut queued_visibility: Option<bool> = None;
    let mut quit_requested = false;

    loop {
        if handle.is_finished() {
            listener.stop();
            if quit_requested {
                let _ = handle.join();
                break Ok(());
            } else {
                tracing::error!("gui thread terminated unexpectedly");
                let _ = handle.join();
                break Ok(());
            }
        }

        if let Some(qt) = &quit_trigger {
            if qt.take() {
                quit_requested = true;
                listener.stop();

                if let Ok(guard) = ctx.lock() {
                    if let Some(c) = &*guard {
                        c.send_viewport_cmd(egui::ViewportCommand::Close);
                        c.request_repaint();
                    }
                }

                let _ = handle.join();
                break Ok(());
            }
        }

        handle_visibility_trigger(trigger.as_ref(), &visibility, &ctx, &mut queued_visibility);

        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}
