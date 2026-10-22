//! Runs one bounded native presentation check for screenshot-pin opacity.

#[cfg(windows)]
fn main() {
    if let Err(error) = multi_launcher::gui::run_pin_opacity_smoke() {
        eprintln!("pin-opacity-smoke: {error}");
        std::process::exit(1);
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("pin-opacity-smoke: GAP: native presentation verification requires Windows");
    std::process::exit(2);
}
