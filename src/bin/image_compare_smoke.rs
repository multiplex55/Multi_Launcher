//! Native fixture for real image-comparison dialog rendering and controls.

fn main() {
    if let Err(error) = multi_launcher::gui::image_compare_smoke::run(std::env::args().skip(1)) {
        eprintln!("image-compare-smoke: {error}");
        std::process::exit(1);
    }
}
