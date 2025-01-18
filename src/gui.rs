use crate::window_manager::check_hotkeys;
use eframe::egui;
use eframe::egui::ViewportBuilder;
use eframe::NativeOptions;
use eframe::{self, App as EframeApp};
use poll_promise::Promise;
use serde::{Deserialize, Serialize};
use std::fs;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct MenuItem {
    name: String,
    r#type: String,
    hotkey: String,
    location: String,
}

#[derive(Serialize, Deserialize, Default)]
pub struct AppSettings {
    menu_items: Vec<MenuItem>,
}

#[derive(Clone)]
pub struct App {
    pub app_title_name: String,
    pub settings: Arc<Mutex<AppSettings>>,
    pub hotkey_promise: Arc<Mutex<Option<Promise<()>>>>,
}

pub fn run_gui(app: App) {
    app.validate_initial_hotkeys();

    let app_for_promise = app.clone();
    let hotkey_promise = Promise::spawn_thread("Hotkey Checker", move || loop {
        check_hotkeys(&app_for_promise);
        thread::sleep(Duration::from_millis(100));
    });
    *app.hotkey_promise.lock().unwrap() = Some(hotkey_promise);

    // let icon_data = include_bytes!("../resources/app_icon.ico");
    // let image = image::load_from_memory(icon_data)
    //     .expect("Failed to load embedded icon")
    //     .to_rgba8();
    // let (width, height) = image.dimensions();
    // let icon_rgba = image.into_raw();

    let options = NativeOptions {
        viewport: ViewportBuilder::default(),
        // .with_icon(egui::IconData {
        //     rgba: icon_rgba,
        //     width,
        //     height,
        // }),
        ..Default::default()
    };

    eframe::run_native(
        &app.app_title_name.clone(),
        options,
        Box::new(|_cc| Ok(Box::new(app))),
    )
    .expect("Failed to run GUI");
}

impl EframeApp for App {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // frame.set_visible(true);

        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.menu_button("File", |_| {});
                ui.menu_button("Favorite", |_| {});
                ui.menu_button("Tools", |_| {});
                ui.menu_button("Options", |_| {});
            });
        });

        egui::TopBottomPanel::bottom("bottom_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Save and Close").clicked() {
                    // self.save_settings("settings.json");
                    // self.show_gui = false; // Close the GUI
                }
                if ui.button("Save").clicked() {
                    // self.save_settings("settings.json");
                }
                if ui.button("Close").clicked() {
                    // self.show_gui = false; // Close the GUI
                }
            });
        });

        egui::SidePanel::left("left_panel").show(ctx, |ui| {
            ui.vertical(|ui| {
                if ui.button("Add").clicked() {
                    // Add action
                }
                if ui.button("Edit").clicked() {
                    // Edit action
                }
                if ui.button("Remove").clicked() {
                    // Remove action
                }
            });
        });

        egui::SidePanel::right("right_panel").show(ctx, |ui| {
            ui.vertical(|ui| {
                if ui.button("Copy").clicked() {
                    // Copy action
                }
                if ui.button("Move").clicked() {
                    // Move action
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            let settings = self.settings.lock().unwrap();
            egui::Grid::new("menu_grid").striped(true).show(ui, |ui| {
                ui.heading("Name");
                ui.heading("Type");
                ui.heading("Hotkey");
                ui.heading("Location");
                ui.end_row();

                for item in &settings.menu_items {
                    ui.label(&item.name);
                    ui.label(&item.r#type);
                    ui.label(&item.hotkey);
                    ui.label(&item.location);
                    ui.end_row();
                }
            });
        });
    }
}

impl App {
    fn validate_initial_hotkeys(&self) {
        //     let mut initial_validation_done = self.initial_validation_done.lock().unwrap();
        //     if !*initial_validation_done {
        //         let mut workspaces = self.workspaces.lock().unwrap();
        //         for (i, workspace) in workspaces.iter_mut().enumerate() {
        //             if let Some(ref mut hotkey) = workspace.hotkey {
        //                 if !hotkey.register(self, i as i32) {
        //                     warn!(
        //                         "Failed to register hotkey '{}' for workspace '{}'",
        //                         hotkey, workspace.name
        //                     );
        //                 }
        //             }
        //         }
        //         *initial_validation_done = true;
        //     }
    }
    // TEMP FIX: Update `App::new` to properly load the `settings_file` with detailed error handling.
    pub fn new(settings_file: &str) -> Self {
        // Attempt to read the file
        let settings = match fs::read_to_string(settings_file) {
            Ok(data) => match serde_json::from_str(&data) {
                Ok(parsed) => {
                    log::info!("Settings successfully loaded from {}", settings_file);
                    parsed
                }
                Err(err) => {
                    log::warn!(
                        "Failed to deserialize settings from {}: {}. Using default settings.",
                        settings_file,
                        err
                    );
                    AppSettings::default()
                }
            },
            Err(err) => {
                log::warn!(
                    "Failed to read settings file {}: {}. Using default settings.",
                    settings_file,
                    err
                );
                AppSettings::default()
            }
        };

        // Initialize and return the App
        Self {
            app_title_name: "Multi Manager".to_string(), // Default title
            hotkey_promise: Arc::new(Mutex::new(None)),  // Initialize hotkey promise
            settings: Arc::new(Mutex::new(settings)),    // Use the loaded settings
        }
    }

    fn save_settings(&self, settings_file: &str) {
        if let Ok(settings_guard) = self.settings.lock() {
            if let Ok(settings) = serde_json::to_string_pretty(&*settings_guard) {
                let _ = fs::write(settings_file, settings);
            }
        }
    }
}
