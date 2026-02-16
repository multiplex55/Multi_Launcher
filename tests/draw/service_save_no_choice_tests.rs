use anyhow::Result;
use multi_launcher::draw::messages::{ExitReason, MainToOverlay, OverlayToMain};
use multi_launcher::draw::service::{
    runtime, set_runtime_spawn_hook, DrawRuntime, EntryContext, OverlayStartupHandshake,
};
use multi_launcher::draw::settings::DrawSettings;
use multi_launcher::draw::state::DrawLifecycle;
use std::sync::{Mutex, OnceLock};

fn test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn reset_runtime(rt: &DrawRuntime) {
    set_runtime_spawn_hook(None);
    rt.reset_for_test();
}

#[test]
fn cancel_choice_returns_to_draw_mode_without_save_side_effects() -> Result<()> {
    let _guard = test_lock().lock().expect("runtime test lock");
    let rt = runtime();
    reset_runtime(rt);

    let temp_dir = tempfile::tempdir()?;
    let mut settings = DrawSettings::default();
    settings.fixed_save_folder_display = temp_dir.path().to_string_lossy().to_string();
    rt.apply_settings(settings);

    let (overlay_tx, overlay_rx) = std::sync::mpsc::channel::<OverlayToMain>();
    let shared_rx = std::sync::Arc::new(Mutex::new(Some(overlay_rx)));
    set_runtime_spawn_hook(Some(Box::new({
        let shared_rx = std::sync::Arc::clone(&shared_rx);
        move |_| {
            let (main_tx, main_rx) = std::sync::mpsc::channel::<MainToOverlay>();
            let handle = std::thread::spawn(move || while main_rx.recv().is_ok() {});
            Ok(OverlayStartupHandshake {
                overlay_thread_handle: handle,
                main_to_overlay_tx: main_tx,
                overlay_to_main_rx: shared_rx.lock().expect("rx lock").take().expect("rx once"),
            })
        }
    })));

    rt.start_with_context(EntryContext::default())?;
    rt.request_exit(ExitReason::UserRequest)?;
    overlay_tx.send(OverlayToMain::ExitDialogCanceled)?;
    rt.tick(std::time::Instant::now())?;

    assert_eq!(rt.lifecycle(), DrawLifecycle::Active);
    assert!(rt.exit_prompt_state().is_none());
    assert!(!temp_dir.path().read_dir()?.any(|e| {
        e.ok()
            .and_then(|x| x.path().extension().map(|ext| ext == "png"))
            .unwrap_or(false)
    }));

    reset_runtime(rt);
    Ok(())
}
