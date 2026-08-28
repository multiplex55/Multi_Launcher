//! Manual Win32 smoke harness for the production passive-overlay path.

#[cfg(windows)]
fn main() {
    if let Err(error) =
        multi_launcher::gui::mkmacro_dialog::visual_overlay::run_passive_overlay_smoke_test()
    {
        eprintln!("passive-overlay-smoke: failed: {error}");
        std::process::exit(1);
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("passive-overlay-smoke: unsupported platform; run this manual harness on Windows");
}
