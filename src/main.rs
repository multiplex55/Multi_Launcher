mod gui;
mod utils;
mod window_manager;

use gui::AppSettings;
use log::info;
use std::env;
use std::fs::File;
use std::io::Write; // Fix for write_all error
use std::sync::{Arc, Mutex};

fn main() {
    // Ensure logging is initialized
    ensure_logging_initialized();

    // Backtrace for Debug
    env::set_var("RUST_BACKTRACE", "1");

    info!("Starting Multi Manager application...");

    let settings_file = "settings.json";

    let app = gui::App::new(settings_file);

    // // Initialize the application states
    // let app = gui::App {
    //     app_title_name: "Multi Manager".to_string(),
    //     hotkey_promise: Arc::new(Mutex::new(None)), // Initialize the promise
    //     settings: Arc::new(Mutex::new(AppSettings::default())),
    // };

    // Launch GUI and set the taskbar icon after creating the window
    gui::run_gui(app);
}

fn ensure_logging_initialized() {
    // Attempt to initialize logging configuration
    if let Err(err) = log4rs::init_file("log4rs.yaml", Default::default()) {
        eprintln!("Failed to initialize log4rs: {}", err);

        // Create a default log4rs.yaml file
        let default_config = r#"
appenders:
  file:
    kind: file
    path: "multi_launcher.log"
    append: false # Overwrite the logfile on each program run
    encoder:
      pattern: "{d} - {l} - {m}{n}"

root:
  level: info
  appenders:
    - file
"#;

        if let Err(e) = File::create("log4rs.yaml")
            .and_then(|mut file| file.write_all(default_config.as_bytes()))
        {
            eprintln!("Failed to create default log4rs.yaml: {}", e);
            std::process::exit(1); // Exit if we cannot create the default configuration
        }

        // Retry initializing log4rs with the newly created configuration file
        if let Err(e) = log4rs::init_file("log4rs.yaml", Default::default()) {
            eprintln!(
                "Failed to reinitialize log4rs with default configuration: {}",
                e
            );
            std::process::exit(1); // Exit if retry fails
        }
    }
}
