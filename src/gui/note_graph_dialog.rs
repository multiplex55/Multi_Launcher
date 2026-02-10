use crate::gui::LauncherApp;
use eframe::egui;
use serde::Deserialize;

#[derive(Default, Deserialize)]
struct NoteGraphDialogArgs {
    #[serde(default)]
    include_tags: Vec<String>,
    #[serde(default)]
    root: Option<String>,
}

#[derive(Default)]
pub struct NoteGraphDialog {
    pub open: bool,
    include_tags: Vec<String>,
    root: Option<String>,
}

impl NoteGraphDialog {
    pub fn open_with_args(&mut self, raw_args: Option<&str>) {
        let parsed = raw_args
            .and_then(|raw| serde_json::from_str::<NoteGraphDialogArgs>(raw).ok())
            .unwrap_or_default();
        self.include_tags = parsed.include_tags;
        self.root = parsed.root.filter(|root| !root.trim().is_empty());
        self.open = true;
    }

    pub fn ui(&mut self, ctx: &egui::Context, _app: &mut LauncherApp) {
        if !self.open {
            return;
        }

        egui::Window::new("Note Graph")
            .open(&mut self.open)
            .resizable(true)
            .default_size((460.0, 320.0))
            .show(ctx, |ui| {
                ui.label("Note graph view");
                if !self.include_tags.is_empty() {
                    ui.label(format!("Prefilter tags: {}", self.include_tags.join(", ")));
                }
                if let Some(root) = &self.root {
                    ui.label(format!("Centered on: {root}"));
                }
            });
    }
}
