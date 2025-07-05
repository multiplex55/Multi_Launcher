mod actions;
mod actions_editor;
mod settings_editor;
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
use std::sync::{Arc, atomic::AtomicBool, Mutex, mpsc::{Sender, channel}};
use std::thread;
use once_cell::sync::Lazy;

static RESTART_TX: Lazy<Mutex<Option<Sender<Settings>>>> = Lazy::new(|| Mutex::new(None));

pub fn request_hotkey_restart(settings: Settings) {
    if let Some(tx) = RESTART_TX.lock().unwrap().as_ref() {
        let _ = tx.send(settings);
    }
}

fn spawn_gui(
    actions: Vec<Action>,
    settings: Settings,
    settings_path: String,
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
    let settings_path_for_window = settings_path.clone();
    let plugin_dirs = settings.plugin_dirs.clone();
    let index_paths = settings.index_paths.clone();
    let visible_flag = Arc::new(AtomicBool::new(true));
    let flag_clone = visible_flag.clone();
    let ctx_handle = Arc::new(Mutex::new(None));
    let ctx_clone = ctx_handle.clone();

    let handle = thread::spawn(move || {
        let native_options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([400.0, 220.0])
                .with_min_inner_size([320.0, 160.0])
                .with_always_on_top()
                .with_visible(true),
            event_loop_builder: Some(Box::new(|builder| {
                #[cfg(target_os = "windows")]
                {
                    use winit::platform::windows::EventLoopBuilderExtWindows;
                    builder.with_any_thread(true);
                }
                #[cfg(target_os = "linux")]
                {
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
                    settings_path_for_window,
                    settings.clone(),
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
    let mut settings = Settings::load("settings.json").unwrap_or_default();
    logging::init(settings.debug_logging);
    tracing::debug!(?settings, "settings loaded");
    let mut actions = load_actions("actions.json").unwrap_or_default();
    tracing::debug!("{} actions loaded", actions.len());

    let (restart_tx, restart_rx) = channel::<Settings>();
    *RESTART_TX.lock().unwrap() = Some(restart_tx);

    if let Some(paths) = &settings.index_paths {
        actions.extend(indexer::index_paths(paths));
    }

    let hotkey = settings.hotkey();
    tracing::debug!(?hotkey, "configuring hotkeys");
    let mut trigger = Arc::new(HotkeyTrigger::new(hotkey));
    let mut quit_trigger = settings.quit_hotkey().map(|hk| Arc::new(HotkeyTrigger::new(hk)));

    let mut watched = vec![trigger.clone()];
    if let Some(qt) = &quit_trigger {
        watched.push(qt.clone());
    }

    let mut listener = HotkeyTrigger::start_listener(watched, "main");


    let (handle, visibility, ctx) = spawn_gui(actions.clone(), settings.clone(), "settings.json".to_string());

    loop {
        if handle.is_finished() {
            listener.stop();
            let _ = handle.join();
            break Ok(());
        }

        if let Some(qt) = &quit_trigger {
            if qt.take() {
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


        if let Ok(new_settings) = restart_rx.try_recv() {
            listener.stop();
            settings = new_settings.clone();
            trigger = Arc::new(HotkeyTrigger::new(settings.hotkey()));
            quit_trigger = settings.quit_hotkey().map(|hk| Arc::new(HotkeyTrigger::new(hk)));
            let mut watched = vec![trigger.clone()];
            if let Some(qt) = &quit_trigger {
                watched.push(qt.clone());
            }
            listener = HotkeyTrigger::start_listener(watched, "main");
        }

        handle_visibility_trigger(trigger.as_ref(), &visibility);

        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}
