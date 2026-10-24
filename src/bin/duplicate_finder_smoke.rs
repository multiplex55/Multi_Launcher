//! Native fixture for the production Duplicate Finder dialog.

fn main() {
    if let Err(error) = multi_launcher::gui::duplicate_finder_smoke::run(std::env::args().skip(1)) {
        eprintln!("duplicate-finder-smoke: {error}");
        std::process::exit(1);
    }
}
