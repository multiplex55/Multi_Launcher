#![windows_subsystem = "windows"]
#![allow(clippy::type_complexity)]

use multi_launcher::actions::{Action, load_startup_actions};
use multi_launcher::common::persistence::PersistenceError;
use multi_launcher::gui::LauncherApp;
use multi_launcher::hotkey::{HotkeyTrigger, parse_hotkey};
use multi_launcher::platform::{
    app_data::AppDataRoot,
    single_instance::{SingleInstanceAcquire, SingleInstanceGuard},
};
use multi_launcher::plugin::PluginManager;
use multi_launcher::screen_draw::{ScreenDrawRecoveryBridge, ScreenDrawSettings};
use multi_launcher::settings::Settings;
use multi_launcher::startup::{SettingsStartupDiagnostic, load_startup_preload};
use multi_launcher::visibility::handle_visibility_trigger;
use multi_launcher::{indexer, logging};

use eframe::{egui, icon_data};
use once_cell::sync::Lazy;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc::{Sender, channel},
};
use std::thread;

fn build_viewport_with_icon(settings: &Settings, icon_bytes: &[u8]) -> egui::ViewportBuilder {
    let (w, h) = settings.window_size.unwrap_or((400, 220));
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([w as f32, h as f32])
        .with_min_inner_size([320.0, 160.0])
        .with_visible(true);

    match icon_data::from_png_bytes(icon_bytes) {
        Ok(icon) => {
            viewport = viewport.with_icon(icon);
        }
        Err(err) => {
            tracing::warn!(
                ?err,
                "failed to decode launcher icon; continuing without custom icon"
            );
        }
    }

    if settings.always_on_top {
        viewport = viewport.with_always_on_top();
    }

    viewport
}

static RESTART_TX: Lazy<Mutex<Option<Sender<Settings>>>> = Lazy::new(|| Mutex::new(None));
static EVENT_TX: Lazy<Mutex<Option<Sender<()>>>> = Lazy::new(|| Mutex::new(None));

fn reserved_launcher_hotkeys(settings: &Settings) -> Vec<(String, String)> {
    let mut reserved = Vec::new();
    let launcher = settings
        .hotkey
        .as_deref()
        .filter(|hotkey| parse_hotkey(hotkey).is_some())
        .unwrap_or("F2");
    reserved.push(("launcher toggle".into(), launcher.into()));
    if let Some(hotkey) = settings
        .quit_hotkey
        .as_deref()
        .filter(|hotkey| parse_hotkey(hotkey).is_some())
    {
        reserved.push(("quit launcher".into(), hotkey.into()));
    }
    if let Some(hotkey) = settings
        .help_hotkey
        .as_deref()
        .filter(|hotkey| parse_hotkey(hotkey).is_some())
    {
        reserved.push(("help launcher".into(), hotkey.into()));
    }
    if let Ok(Some(hotkey)) = screen_draw_launch_hotkey_text(settings) {
        reserved.push(("launch Screen Draw".into(), hotkey.into()));
    }
    if let Ok(Some(hotkey)) = screen_draw_emergency_hotkey_text(settings) {
        reserved.push(("Screen Draw emergency".into(), hotkey));
    }
    reserved
}

fn configured_screen_draw_settings(settings: &Settings) -> Result<ScreenDrawSettings, String> {
    settings
        .plugin_settings
        .get("screen_draw")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|error| format!("invalid Screen Draw settings: {error}"))
        .map(|settings| settings.unwrap_or_default())
}

fn screen_draw_launch_hotkey_text(settings: &Settings) -> Result<Option<String>, String> {
    if !settings.plugin_settings.contains_key("screen_draw") {
        return Ok(None);
    }
    let screen_draw = configured_screen_draw_settings(settings)?;
    match screen_draw.launch_hotkey.as_ref() {
        Some(chord) if chord.is_valid() => {
            let parsed = parse_hotkey(chord.as_str()).expect("validated Screen Draw hotkey");
            let conflicts = [
                ("launcher toggle", Some(settings.hotkey())),
                ("quit launcher", settings.quit_hotkey()),
                ("help launcher", settings.help_hotkey()),
            ];
            if let Some((name, _)) = conflicts.into_iter().find(|(_, existing)| {
                existing.is_some_and(|existing| same_hotkey(parsed, existing))
            }) {
                return Err(format!(
                    "Screen Draw launch hotkey '{}' conflicts with {name}; global launch hotkey disabled",
                    chord.as_str()
                ));
            }
            if screen_draw_emergency_hotkey_text(settings)
                .ok()
                .flatten()
                .and_then(|emergency| parse_hotkey(&emergency))
                .is_some_and(|emergency| same_hotkey(parsed, emergency))
            {
                return Err(format!(
                    "Screen Draw launch hotkey '{}' conflicts with Screen Draw emergency; global launch hotkey disabled",
                    chord.as_str()
                ));
            }
            Ok(Some(chord.as_str().to_owned()))
        }
        Some(chord) => Err(format!(
            "invalid Screen Draw launch hotkey '{}'; global launch hotkey disabled",
            chord.as_str()
        )),
        None => Ok(None),
    }
}

fn screen_draw_emergency_hotkey_text(settings: &Settings) -> Result<Option<String>, String> {
    let screen_draw = configured_screen_draw_settings(settings)?;
    let chord = &screen_draw.emergency_hotkey;
    if !chord.is_valid() {
        return Err(format!(
            "invalid Screen Draw emergency hotkey '{}'; emergency hotkey disabled",
            chord.as_str()
        ));
    }
    let parsed = parse_hotkey(chord.as_str()).expect("validated Screen Draw emergency hotkey");
    let conflicts = [
        ("launcher toggle", Some(settings.hotkey())),
        ("quit launcher", settings.quit_hotkey()),
        ("help launcher", settings.help_hotkey()),
    ];
    if let Some((name, _)) = conflicts
        .into_iter()
        .find(|(_, existing)| existing.is_some_and(|existing| same_hotkey(parsed, existing)))
    {
        return Err(format!(
            "Screen Draw emergency hotkey '{}' conflicts with {name}; emergency hotkey disabled",
            chord.as_str()
        ));
    }
    Ok(Some(chord.as_str().to_owned()))
}

fn same_hotkey(
    left: multi_launcher::hotkey::Hotkey,
    right: multi_launcher::hotkey::Hotkey,
) -> bool {
    left.key == right.key
        && left.ctrl == right.ctrl
        && left.shift == right.shift
        && left.alt == right.alt
        && left.win == right.win
}

fn screen_draw_launch_trigger(settings: &Settings) -> Option<Arc<HotkeyTrigger>> {
    match screen_draw_launch_hotkey_text(settings) {
        Ok(Some(chord)) => parse_hotkey(&chord).map(|hotkey| Arc::new(HotkeyTrigger::new(hotkey))),
        Ok(None) => None,
        Err(error) => {
            tracing::warn!(%error, "Screen Draw launch hotkey is unavailable");
            None
        }
    }
}

fn screen_draw_emergency_trigger(settings: &Settings) -> Option<Arc<HotkeyTrigger>> {
    match screen_draw_emergency_hotkey_text(settings) {
        Ok(Some(chord)) => parse_hotkey(&chord).map(|hotkey| Arc::new(HotkeyTrigger::new(hotkey))),
        Ok(None) => None,
        Err(error) => {
            tracing::warn!(%error, "Screen Draw emergency hotkey is unavailable");
            None
        }
    }
}

fn take_screen_draw_recovery_trigger(trigger: &HotkeyTrigger, screen_draw_active: bool) -> bool {
    screen_draw_active && trigger.take()
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
    if let Ok(guard) = EVENT_TX.lock()
        && let Some(tx) = guard.as_ref()
    {
        let _ = tx.send(());
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
    startup_settings_diagnostic: Option<SettingsStartupDiagnostic>,
    startup_actions_diagnostic: Option<PersistenceError>,
    startup_recovery: multi_launcher::persistence::RecoveryStartupResult,
    enabled_capabilities: Option<std::collections::HashMap<String, Vec<String>>>,
    screen_draw_recovery_bridge: Arc<ScreenDrawRecoveryBridge>,
    event_tx: Sender<()>,
) -> (
    thread::JoinHandle<()>,
    Arc<AtomicBool>,
    Arc<AtomicBool>,
    Arc<AtomicBool>,
    Arc<Mutex<Option<egui::Context>>>,
) {
    let custom_len_for_window = custom_len;
    let reserved_launcher_hotkeys = reserved_launcher_hotkeys(&settings);
    let reserved_launcher_hotkey_refs = reserved_launcher_hotkeys
        .iter()
        .map(|(name, hotkey)| (name.as_str(), hotkey.as_str()))
        .collect::<Vec<_>>();
    let manager_timer = multi_launcher::performance::Timer::start();
    let mut plugins = PluginManager::new_with_reserved_hotkeys(&reserved_launcher_hotkey_refs);
    manager_timer.finish("startup.plugin_manager");
    let empty_dirs = Vec::new();
    let dirs = settings.plugin_dirs.as_ref().unwrap_or(&empty_dirs);
    let mut plugin_settings = settings.plugin_settings.clone();
    plugin_settings.insert(
        "note".into(),
        multi_launcher::plugins::note::note_plugin_settings_with_backlinks(
            plugin_settings.get("note"),
            settings.note.backlinks_enabled,
            settings.note.aliases_enabled,
            settings.note.templates_enabled,
        ),
    );
    let registration_timer = multi_launcher::performance::Timer::start();
    plugins.reload_from_dirs(
        dirs,
        settings.clipboard_limit,
        settings.net_unit,
        true,
        &plugin_settings,
        Arc::clone(&actions),
    );
    registration_timer.finish("startup.plugin_registration");
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
        let viewport = build_viewport_with_icon(
            &settings,
            include_bytes!("../Resources/Green_MultiLauncher.png"),
        );
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
                let launcher_timer = multi_launcher::performance::Timer::start();
                let mut app = LauncherApp::new(
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
                );
                app.install_screen_draw_recovery_bridge(screen_draw_recovery_bridge);
                app.startup_settings_diagnostic = startup_settings_diagnostic;
                app.actions_persistence_diagnostic = startup_actions_diagnostic;
                app.set_startup_persistence_context(startup_recovery);
                launcher_timer.finish("startup.launcher_app");
                Box::new(app)
            }),
        );
        let _ = event_tx.send(());
    });

    (handle, visible_flag, restore_flag, help_flag, ctx_handle)
}

fn main() -> anyhow::Result<()> {
    multi_launcher::performance::init_process_timer();
    // This ownership boundary must stay ahead of every persistent read,
    // migration, watcher, worker, plugin, and GUI initialization.
    let app_data_root = AppDataRoot::from_settings_path("settings.json")?;
    let _single_instance_guard = match SingleInstanceGuard::acquire(&app_data_root)? {
        SingleInstanceAcquire::Acquired(guard) => guard,
        SingleInstanceAcquire::AlreadyRunning => return Ok(()),
    };
    let settings_timer = multi_launcher::performance::Timer::start();
    let startup_preload = load_startup_preload(&app_data_root, "settings.json");
    let startup_recovery = startup_preload.recovery;
    let startup_settings = startup_preload.settings;
    let mut settings = startup_settings.settings;
    let startup_settings_diagnostic = startup_settings.diagnostic;
    multi_launcher::settings::set_settings_path("settings.json");
    let _logging_guard = logging::init(settings.debug_logging, settings.log_file_path());
    settings_timer.finish("startup.settings_load");
    if let Some(diagnostic) = startup_recovery.diagnostic.as_ref() {
        tracing::error!(error = %diagnostic, "startup recovery remains pending for retry");
    } else if let Some(action) = startup_recovery.applied.as_ref() {
        tracing::info!(?action, "applied staged startup recovery");
    }
    tracing::debug!(?settings, "settings loaded");
    if let Some(diagnostic) = startup_settings_diagnostic.as_ref() {
        tracing::error!(
            error = %diagnostic.error(),
            "settings startup used temporary defaults without replacing the persisted file"
        );
    }
    multi_launcher::plugins::mouse_gestures::sync_enabled_plugins(
        settings.enabled_plugins.as_ref(),
    );
    if let Some(value) = settings.plugin_settings.get("mouse_gestures")
        && let Ok(cfg) = serde_json::from_value::<
            multi_launcher::plugins::mouse_gestures::MouseGestureSettings,
        >(value.clone())
    {
        multi_launcher::plugins::mouse_gestures::apply_runtime_settings(cfg);
    }
    let actions_timer = multi_launcher::performance::Timer::start();
    let startup_actions = load_startup_actions("actions.json");
    let mut actions_vec = startup_actions.actions;
    let startup_actions_diagnostic = startup_actions.diagnostic;
    let custom_len = actions_vec.len();
    tracing::debug!("{} actions loaded", actions_vec.len());
    if let Some(diagnostic) = startup_actions_diagnostic.as_ref() {
        tracing::error!(
            error = %diagnostic,
            "actions startup used a temporary empty list without replacing the persisted file"
        );
    }
    actions_timer.finish("startup.action_load");

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

    let index_timer = multi_launcher::performance::Timer::start();
    if let Some(paths) = &settings.index_paths {
        let options = indexer::IndexOptions::with_max_items(settings.max_indexed_items);
        for batch in indexer::index_paths_batched(paths, options) {
            actions_vec.extend(batch?);
        }
    }
    index_timer.finish("startup.action_indexing");
    let actions = Arc::new(actions_vec);
    let screen_draw_recovery_bridge = Arc::new(ScreenDrawRecoveryBridge::default());

    let hotkey = settings.hotkey();
    tracing::debug!(?hotkey, "configuring hotkeys");
    let mut trigger = Arc::new(HotkeyTrigger::new(hotkey));
    let mut quit_trigger = settings
        .quit_hotkey()
        .map(|hk| Arc::new(HotkeyTrigger::new(hk)));
    let mut help_trigger = settings
        .help_hotkey()
        .map(|hk| Arc::new(HotkeyTrigger::new(hk)));
    let mut screen_draw_trigger = screen_draw_launch_trigger(&settings);
    let mut emergency_trigger = screen_draw_emergency_trigger(&settings);

    let mut watched = vec![trigger.clone()];
    if let Some(qt) = &quit_trigger {
        watched.push(qt.clone());
    }
    if let Some(ht) = &help_trigger {
        watched.push(ht.clone());
    }
    if let Some(sd) = &screen_draw_trigger {
        watched.push(sd.clone());
    }
    if let Some(emergency) = &emergency_trigger {
        watched.push(emergency.clone());
    }

    let mut listener = HotkeyTrigger::start_listener(watched, "main", event_tx.clone());

    // `visibility` holds whether the window is currently restored (true) or
    // minimized (false).
    let (handle, visibility, restore_flag, help_flag, ctx) = spawn_gui(
        Arc::clone(&actions),
        custom_len,
        settings.clone(),
        "settings.json".to_string(),
        startup_settings_diagnostic,
        startup_actions_diagnostic,
        startup_recovery,
        settings.enabled_capabilities.clone(),
        Arc::clone(&screen_draw_recovery_bridge),
        event_tx.clone(),
    );
    let mut queued_visibility: Option<bool> = None;

    loop {
        if let Err(err) = event_rx.recv() {
            tracing::error!(?err, "event channel closed; shutting down launcher loop");
            listener.stop();
            let _ = handle.join();
            break Ok(());
        }

        if handle.is_finished() {
            listener.stop();
            let _ = handle.join();
            break Ok(());
        }

        if let Some(emergency) = &emergency_trigger
            && emergency.take()
            && screen_draw_recovery_bridge.is_active()
        {
            if let Err(error) = screen_draw_recovery_bridge.emergency_pause() {
                tracing::error!(%error, "failed to deliver Screen Draw emergency pause");
            }
            multi_launcher::gui::send_event(multi_launcher::gui::WatchEvent::ScreenDrawEmergency);
            if let Ok(guard) = ctx.lock()
                && let Some(c) = &*guard
            {
                c.request_repaint();
            }
        }

        if take_screen_draw_recovery_trigger(&trigger, screen_draw_recovery_bridge.is_active()) {
            multi_launcher::gui::send_event(multi_launcher::gui::WatchEvent::ScreenDrawRecover);
            if let Ok(guard) = ctx.lock()
                && let Some(c) = &*guard
            {
                c.request_repaint();
            }
        }

        if let Some(qt) = &quit_trigger
            && qt.take()
        {
            listener.stop();

            if let Ok(guard) = ctx.lock()
                && let Some(c) = &*guard
            {
                c.send_viewport_cmd(egui::ViewportCommand::Close);
                c.request_repaint();
            }

            let _ = handle.join();
            break Ok(());
        }

        if let Some(ht) = &help_trigger
            && ht.take()
            && visibility.load(Ordering::SeqCst)
        {
            help_flag.store(true, Ordering::SeqCst);
            if let Ok(guard) = ctx.lock()
                && let Some(c) = &*guard
            {
                c.request_repaint();
            }
        }

        if let Some(sd) = &screen_draw_trigger
            && sd.take()
        {
            multi_launcher::gui::send_event(multi_launcher::gui::WatchEvent::ScreenDrawStart);
            if let Ok(guard) = ctx.lock()
                && let Some(c) = &*guard
            {
                c.request_repaint();
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
            screen_draw_trigger = screen_draw_launch_trigger(&settings);
            emergency_trigger = screen_draw_emergency_trigger(&settings);
            let mut watched = vec![trigger.clone()];
            if let Some(qt) = &quit_trigger {
                watched.push(qt.clone());
            }
            if let Some(ht) = &help_trigger {
                watched.push(ht.clone());
            }
            if let Some(sd) = &screen_draw_trigger {
                watched.push(sd.clone());
            }
            if let Some(emergency) = &emergency_trigger {
                watched.push(emergency.clone());
            }
            listener = HotkeyTrigger::start_listener(watched, "main", event_tx.clone());
        }

        let visibility_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            handle_visibility_trigger(
                trigger.as_ref(),
                &visibility,
                &restore_flag,
                &ctx,
                &mut queued_visibility,
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
            )
        }));

        match visibility_result {
            Ok(true) => {
                let _ = event_tx.send(());
            }
            Ok(false) => {}
            Err(_panic_payload) => {
                tracing::error!("visibility handler panicked; continuing event loop");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_viewport_with_invalid_icon_bytes_does_not_panic() {
        let settings = Settings::default();
        let result = std::panic::catch_unwind(|| build_viewport_with_icon(&settings, b"not-a-png"));
        assert!(result.is_ok());
    }

    #[test]
    fn screen_draw_launch_hotkey_is_reserved_and_parsed() {
        let mut settings = Settings::default();
        settings.plugin_settings.insert(
            "screen_draw".into(),
            serde_json::json!({ "launch_hotkey": "Ctrl+Shift+D" }),
        );
        assert_eq!(
            screen_draw_launch_hotkey_text(&settings),
            Ok(Some("Ctrl+Shift+D".into()))
        );
        assert!(
            reserved_launcher_hotkeys(&settings)
                .contains(&("launch Screen Draw".into(), "Ctrl+Shift+D".into()))
        );
        assert!(screen_draw_launch_trigger(&settings).is_some());
    }

    #[test]
    fn invalid_screen_draw_launch_hotkey_is_safely_disabled() {
        let mut settings = Settings::default();
        settings.plugin_settings.insert(
            "screen_draw".into(),
            serde_json::json!({ "launch_hotkey": "Ctrl+DefinitelyNotAKey" }),
        );
        assert!(screen_draw_launch_hotkey_text(&settings).is_err());
        assert!(screen_draw_launch_trigger(&settings).is_none());
        assert!(
            !reserved_launcher_hotkeys(&settings)
                .iter()
                .any(|(name, _)| name == "launch Screen Draw")
        );
    }

    #[test]
    fn conflicting_screen_draw_launch_hotkey_is_safely_disabled() {
        let mut settings = Settings {
            quit_hotkey: Some("Ctrl+Q".into()),
            help_hotkey: Some("Ctrl+H".into()),
            ..Settings::default()
        };
        for chord in ["F2", "Ctrl+Q", "Ctrl+H"] {
            settings.plugin_settings.insert(
                "screen_draw".into(),
                serde_json::json!({ "launch_hotkey": chord }),
            );
            let error = screen_draw_launch_hotkey_text(&settings).unwrap_err();
            assert!(error.contains("conflicts"), "{error}");
            assert!(screen_draw_launch_trigger(&settings).is_none());
        }
    }

    #[test]
    fn emergency_hotkey_is_process_reserved_and_wins_screen_draw_conflict() {
        let mut settings = Settings::default();
        settings.plugin_settings.insert(
            "screen_draw".into(),
            serde_json::json!({
                "launch_hotkey": "Ctrl+Shift+F11",
                "emergency_hotkey": "Ctrl+Shift+F11"
            }),
        );
        assert!(screen_draw_launch_hotkey_text(&settings).is_err());
        assert_eq!(
            screen_draw_emergency_hotkey_text(&settings),
            Ok(Some("Ctrl+Shift+F11".into()))
        );
        let reserved = reserved_launcher_hotkeys(&settings);
        assert!(
            !reserved
                .iter()
                .any(|(name, _)| name == "launch Screen Draw")
        );
        assert!(reserved.contains(&("Screen Draw emergency".into(), "Ctrl+Shift+F11".into())));
    }

    #[test]
    fn emergency_hotkey_conflicting_with_launcher_is_disabled() {
        let mut settings = Settings::default();
        settings.plugin_settings.insert(
            "screen_draw".into(),
            serde_json::json!({ "emergency_hotkey": "F2" }),
        );
        assert!(screen_draw_emergency_hotkey_text(&settings).is_err());
        assert!(screen_draw_emergency_trigger(&settings).is_none());
        assert!(
            !reserved_launcher_hotkeys(&settings)
                .iter()
                .any(|(name, _)| name == "Screen Draw emergency")
        );
    }

    #[test]
    fn active_screen_draw_consumes_launcher_trigger_exactly_once() {
        let trigger = HotkeyTrigger::new(parse_hotkey("F2").unwrap());
        *trigger.open.lock().unwrap() = true;

        assert!(take_screen_draw_recovery_trigger(&trigger, true));
        assert!(!trigger.take());

        *trigger.open.lock().unwrap() = true;
        assert!(!take_screen_draw_recovery_trigger(&trigger, false));
        assert!(trigger.take());
    }
}
