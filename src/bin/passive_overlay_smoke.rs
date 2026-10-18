//! Manual Win32 smoke harness for the production passive-overlay path.

#[cfg(windows)]
fn main() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::ERROR)
        .try_init();
    let probe = std::env::args().any(|arg| arg == "--rectangle-shield-probe");
    let rectangle = std::env::args().any(|arg| arg == "--rectangle-input-isolation");
    let result = if probe {
        multi_launcher::gui::mkmacro_dialog::visual_overlay::run_rectangle_shield_probe()
    } else if rectangle {
        multi_launcher::gui::mkmacro_dialog::visual_overlay::run_rectangle_input_isolation_smoke_test()
    } else {
        multi_launcher::gui::mkmacro_dialog::visual_overlay::run_passive_overlay_smoke_test()
    };
    if let Err(error) = result {
        eprintln!("passive-overlay-smoke: failed: {error}");
        std::process::exit(1);
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("passive-overlay-smoke: unsupported platform; run this manual harness on Windows");
}
