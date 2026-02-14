#![windows_subsystem = "windows"]

use multi_launcher::actions::{load_actions, Action};
use multi_launcher::gui::LauncherApp;
use multi_launcher::hotkey::HotkeyTrigger;
use multi_launcher::plugin::PluginManager;
use multi_launcher::settings::Settings;
use multi_launcher::visibility::{handle_visibility_trigger_with_draw_guard, VisibilityIntent};
use multi_launcher::{indexer, logging};

use eframe::{egui, icon_data};
use once_cell::sync::Lazy;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{channel, Sender},
    Arc, Mutex,
};
use std::thread;
use std::time::{Duration, Instant};

static RESTART_TX: Lazy<Mutex<Option<Sender<Settings>>>> = Lazy::new(|| Mutex::new(None));
static EVENT_TX: Lazy<Mutex<Option<Sender<()>>>> = Lazy::new(|| Mutex::new(None));
const DRAW_RUNTIME_TICK_INTERVAL: Duration = Duration::from_millis(100);

fn tick_draw_runtime(now: Instant) {
    if let Err(err) = multi_launcher::draw::runtime().tick(now) {
        tracing::warn!(?err, "draw runtime tick failed");
    }
}

pub fn request_hotkey_restart(settings: Settings) {
    match RESTART_TX.lock() {
        Ok(guard) => {
            if let Some(tx) = guard.as_ref() {
                let _ = tx.send(settings);
            }
        }
        Err(e) => {
            tracing::error!("failed to lock RESTART_TX: {e}");
        }
    }
    if let Ok(guard) = EVENT_TX.lock() {
        if let Some(tx) = guard.as_ref() {
            let _ = tx.send(());
        }
    }
}

/// Spawn the GUI on a separate thread.
///
/// `actions` is wrapped in an [`Arc`] so the main thread and GUI worker can
/// share a single action list without copying the underlying `Vec`. Cloning the
/// `Arc` only clones the pointer, leaving the `Vec<Action>` itself shared for
/// thread-safe reads. When passing the list to other threads or windows,
/// callers should [`Arc::clone`] the pointer instead of cloning the vector.
fn spawn_gui(
    actions: Arc<Vec<Action>>,
    custom_len: usize,
    settings: Settings,
    settings_path: String,
    enabled_capabilities: Option<std::collections::HashMap<String, Vec<String>>>,
    event_tx: Sender<()>,
) -> (
    thread::JoinHandle<()>,
    Arc<AtomicBool>,
    Arc<AtomicBool>,
    Arc<AtomicBool>,
    Arc<Mutex<Option<egui::Context>>>,
) {
    let custom_len_for_window = custom_len;
    let mut plugins = PluginManager::new();
    let empty_dirs = Vec::new();
    let dirs = settings.plugin_dirs.as_ref().unwrap_or(&empty_dirs);
    plugins.reload_from_dirs(
        dirs,
        settings.clipboard_limit,
        settings.net_unit,
        true,
        &settings.plugin_settings,
        Arc::clone(&actions),
    );
    // Ensure MG service starts even when there is no settings.json/plugin_settings entry yet.
    // Also ensures it is OFF if the plugin is disabled in enabled_plugins.
    multi_launcher::plugins::mouse_gestures::sync_enabled_plugins(
        settings.enabled_plugins.as_ref(),
    );

    let actions_path = "actions.json".to_string();
    let settings_path_for_window = settings_path.clone();
    let plugin_dirs = settings.plugin_dirs.clone();
    let index_paths = settings.index_paths.clone();
    let enabled_plugins = settings.enabled_plugins.clone();
    let visible_flag = Arc::new(AtomicBool::new(true));
    let restore_flag = Arc::new(AtomicBool::new(false));
    let help_flag = Arc::new(AtomicBool::new(false));
    let flag_clone = visible_flag.clone();
    let restore_clone = restore_flag.clone();
    let help_clone = help_flag.clone();
    let ctx_handle = Arc::new(Mutex::new(None));
    let ctx_clone = ctx_handle.clone();
    let actions_for_window = Arc::clone(&actions);

    let handle = thread::spawn(move || {
        let (w, h) = settings.window_size.unwrap_or((400, 220));
        let icon =
            icon_data::from_png_bytes(include_bytes!("../Resources/Green_MultiLauncher.png"))
                .expect("invalid icon");
        let mut viewport = egui::ViewportBuilder::default()
            .with_inner_size([w as f32, h as f32])
            .with_min_inner_size([320.0, 160.0])
            .with_visible(true)
            .with_icon(icon);
        if settings.always_on_top {
            viewport = viewport.with_always_on_top();
        }
        let native_options = eframe::NativeOptions {
            viewport,
            event_loop_builder: Some(Box::new(|_builder| {
                use winit::platform::windows::EventLoopBuilderExtWindows;
                _builder.with_any_thread(true);
            })),
            ..Default::default()
        };

        let _ = eframe::run_native(
            "Multi Lnchr",
            native_options,
            Box::new(move |cc| {
                if let Ok(mut guard) = ctx_clone.lock() {
                    *guard = Some(cc.egui_ctx.clone());
                } else {
                    tracing::error!("failed to lock ctx_clone");
                }
                tracing::debug!("egui context stored");
                Box::new(LauncherApp::new(
                    &cc.egui_ctx,
                    actions_for_window,
                    custom_len_for_window,
                    plugins,
                    actions_path,
                    settings_path_for_window,
                    settings.clone(),
                    plugin_dirs,
                    index_paths,
                    enabled_plugins,
                    enabled_capabilities,
                    flag_clone,
                    restore_clone,
                    help_clone,
                ))
            }),
        );
        let _ = event_tx.send(());
    });

    (handle, visible_flag, restore_flag, help_flag, ctx_handle)
}

fn main() -> anyhow::Result<()> {
    let mut settings = Settings::load("settings.json").unwrap_or_default();
    multi_launcher::settings::set_settings_path("settings.json");
    logging::init(settings.debug_logging, settings.log_file_path());
    tracing::debug!(?settings, "settings loaded");
    multi_launcher::plugins::mouse_gestures::sync_enabled_plugins(
        settings.enabled_plugins.as_ref(),
    );
    if let Some(value) = settings.plugin_settings.get("mouse_gestures") {
        if let Ok(cfg) = serde_json::from_value::<
            multi_launcher::plugins::mouse_gestures::MouseGestureSettings,
        >(value.clone())
        {
            multi_launcher::plugins::mouse_gestures::apply_runtime_settings(cfg);
        }
    }
    let mut actions_vec = load_actions("actions.json").unwrap_or_default();
    let custom_len = actions_vec.len();
    tracing::debug!("{} actions loaded", actions_vec.len());

    let (restart_tx, restart_rx) = channel::<Settings>();
    if let Ok(mut guard) = RESTART_TX.lock() {
        *guard = Some(restart_tx);
    } else {
        tracing::error!("failed to lock RESTART_TX while starting");
    }

    let (event_tx, event_rx) = channel::<()>();
    if let Ok(mut guard) = EVENT_TX.lock() {
        *guard = Some(event_tx.clone());
    }

    if let Some(paths) = &settings.index_paths {
        actions_vec.extend(indexer::index_paths(paths)?);
    }
    let actions = Arc::new(actions_vec);

    let hotkey = settings.hotkey();
    tracing::debug!(?hotkey, "configuring hotkeys");
    let mut trigger = Arc::new(HotkeyTrigger::new(hotkey));
    let mut quit_trigger = settings
        .quit_hotkey()
        .map(|hk| Arc::new(HotkeyTrigger::new(hk)));
    let mut help_trigger = settings
        .help_hotkey()
        .map(|hk| Arc::new(HotkeyTrigger::new(hk)));

    let mut watched = vec![trigger.clone()];
    if let Some(qt) = &quit_trigger {
        watched.push(qt.clone());
    }
    if let Some(ht) = &help_trigger {
        watched.push(ht.clone());
    }

    let mut listener = HotkeyTrigger::start_listener(watched, "main", event_tx.clone());

    // `visibility` holds whether the window is currently restored (true) or
    // minimized (false).
    let (handle, visibility, restore_flag, help_flag, ctx) = spawn_gui(
        Arc::clone(&actions),
        custom_len,
        settings.clone(),
        "settings.json".to_string(),
        settings.enabled_capabilities.clone(),
        event_tx.clone(),
    );
    let mut queued_visibility: Option<bool> = None;

    loop {
        let _ = event_rx.recv_timeout(DRAW_RUNTIME_TICK_INTERVAL);
        tick_draw_runtime(Instant::now());

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

        if let Some(ht) = &help_trigger {
            if ht.take() && visibility.load(Ordering::SeqCst) {
                help_flag.store(true, Ordering::SeqCst);
                if let Ok(guard) = ctx.lock() {
                    if let Some(c) = &*guard {
                        c.request_repaint();
                    }
                }
            }
        }

        if let Ok(new_settings) = restart_rx.try_recv() {
            listener.stop();
            settings = new_settings.clone();
            trigger = Arc::new(HotkeyTrigger::new(settings.hotkey()));
            quit_trigger = settings
                .quit_hotkey()
                .map(|hk| Arc::new(HotkeyTrigger::new(hk)));
            help_trigger = settings
                .help_hotkey()
                .map(|hk| Arc::new(HotkeyTrigger::new(hk)));
            let mut watched = vec![trigger.clone()];
            if let Some(qt) = &quit_trigger {
                watched.push(qt.clone());
            }
            if let Some(ht) = &help_trigger {
                watched.push(ht.clone());
            }
            listener = HotkeyTrigger::start_listener(watched, "main", event_tx.clone());
        }

        if handle_visibility_trigger_with_draw_guard(
            trigger.as_ref(),
            &visibility,
            &restore_flag,
            &ctx,
            &mut queued_visibility,
            VisibilityIntent::Toggle,
            || multi_launcher::draw::runtime().is_active(),
            || {
                if let Err(err) = multi_launcher::draw::runtime()
                    .request_exit(multi_launcher::draw::messages::ExitReason::LauncherHotkey)
                {
                    tracing::warn!(?err, "failed to request draw exit from launcher hotkey");
                }
            },
            {
                let (x, y) = settings.offscreen_pos.unwrap_or((2000, 2000));
                (x as f32, y as f32)
            },
            settings.follow_mouse,
            settings.static_location_enabled,
            settings.static_pos.map(|(x, y)| (x as f32, y as f32)),
            settings.static_size.map(|(w, h)| (w as f32, h as f32)),
            {
                let (w, h) = settings.window_size.unwrap_or((400, 220));
                (w as f32, h as f32)
            },
        ) {
            let _ = event_tx.send(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::tick_draw_runtime;
    use multi_launcher::draw::messages::ExitReason;
    use multi_launcher::draw::service::{runtime, set_runtime_restore_hook, EntryContext};
    use multi_launcher::draw::state::DrawLifecycle;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    #[test]
    fn main_loop_tick_harness_triggers_timeout_restore() {
        let rt = runtime();
        rt.reset_for_test();

        let restore_count = Arc::new(AtomicUsize::new(0));
        let restore_count_clone = Arc::clone(&restore_count);
        set_runtime_restore_hook(Some(Box::new(move || {
            restore_count_clone.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })));

        rt.force_lifecycle_for_test(DrawLifecycle::Exiting);
        rt.notify_overlay_exit(ExitReason::UserRequest)
            .expect("manual exit should restore");

        let timed_out_context = EntryContext {
            timeout_deadline: Some(Instant::now() - Duration::from_secs(1)),
            ..EntryContext::default()
        };
        rt.start_with_context(timed_out_context)
            .expect("start with timeout should succeed");

        tick_draw_runtime(Instant::now());

        assert_eq!(restore_count.load(Ordering::SeqCst), 2);
        assert_eq!(rt.lifecycle(), DrawLifecycle::Idle);
        set_runtime_restore_hook(None);
        rt.reset_for_test();
    }
}
