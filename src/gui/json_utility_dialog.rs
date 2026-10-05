use crate::commands::JsonUtilityIntent;
use eframe::egui;

#[derive(Debug)]
pub struct JsonUtilityDialogState {
    pub open: bool,
    pub intent: JsonUtilityIntent,
}

impl Default for JsonUtilityDialogState {
    fn default() -> Self {
        Self {
            open: false,
            intent: JsonUtilityIntent::General,
        }
    }
}

impl JsonUtilityDialogState {
    pub fn open(&mut self, intent: JsonUtilityIntent) {
        self.intent = intent;
        self.open = true;
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        if !self.open {
            return;
        }

        egui::Window::new("JSON Utility")
            .id(egui::Id::new("json_utility_dialog"))
            .collapsible(false)
            .open(&mut self.open)
            .show(ctx, |ui| {
                ui.heading(self.intent.label());
                ui.label("The JSON editor and transform controls are coming in the next update.");
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opening_dialog_keeps_the_requested_intent() {
        let mut dialog = JsonUtilityDialogState::default();
        dialog.open(JsonUtilityIntent::Minify);

        assert!(dialog.open);
        assert_eq!(dialog.intent, JsonUtilityIntent::Minify);
    }
}
