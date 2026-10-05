use crate::clipboard_modify::clipboard::{ArboardClipboardBackend, ClipboardBackend};
use crate::commands::JsonUtilityIntent;
use crate::json_transform::{JsonDocument, JsonTransformError};
use eframe::egui;

#[derive(Debug, Clone, Copy)]
enum Transform {
    Format,
    Minify,
}

/// Editable session state; clipboard writes are confined to explicit Copy.
#[derive(Debug, Default)]
struct JsonEditorState {
    text: String,
    parse_error: Option<JsonTransformError>,
    feedback: Option<Result<String, String>>,
}

impl JsonEditorState {
    fn initialize(&mut self, clipboard: &impl ClipboardBackend) {
        self.clear();
        if let Ok(text) = clipboard.read_text()
            && JsonDocument::parse(&text).is_ok()
        {
            self.text = text;
        }
    }

    fn reset_feedback(&mut self) {
        self.parse_error = None;
        self.feedback = None;
    }

    fn clear(&mut self) {
        self.text.clear();
        self.reset_feedback();
    }

    fn reload(&mut self, clipboard: &impl ClipboardBackend) {
        self.reset_feedback();
        match clipboard.read_text() {
            Ok(text) => self.text = text,
            Err(error) => self.feedback = Some(Err(format!("Could not read clipboard: {error}"))),
        }
    }

    fn transform(&mut self, transform: Transform) {
        self.reset_feedback();
        let result = JsonDocument::parse(&self.text).and_then(|document| match transform {
            Transform::Format => document.pretty(),
            Transform::Minify => document.minify(),
        });
        match result {
            Ok(text) => self.text = text,
            Err(error) => self.parse_error = Some(error),
        }
    }

    fn copy(&mut self, clipboard: &impl ClipboardBackend) {
        self.reset_feedback();
        if let Err(error) = JsonDocument::parse(&self.text) {
            self.parse_error = Some(error);
            return;
        }
        self.feedback = Some(
            clipboard
                .write_text(&self.text)
                .map(|()| "Copied JSON to clipboard".to_owned())
                .map_err(|error| format!("Could not copy JSON: {error}")),
        );
    }
}

#[derive(Debug)]
pub struct JsonUtilityDialogState {
    pub open: bool,
    pub intent: JsonUtilityIntent,
    editor: JsonEditorState,
    focus_editor: bool,
}

impl Default for JsonUtilityDialogState {
    fn default() -> Self {
        Self {
            open: false,
            intent: JsonUtilityIntent::General,
            editor: JsonEditorState::default(),
            focus_editor: false,
        }
    }
}

impl JsonUtilityDialogState {
    pub fn open(&mut self, intent: JsonUtilityIntent) {
        self.open_with_clipboard(intent, &ArboardClipboardBackend);
    }

    fn open_with_clipboard(
        &mut self,
        intent: JsonUtilityIntent,
        clipboard: &impl ClipboardBackend,
    ) {
        self.intent = intent;
        self.editor.initialize(clipboard);
        self.focus_editor = true;
        self.open = true;
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        if !self.open {
            return;
        }
        let mut open = self.open;
        egui::Window::new("JSON Utility")
            .id(egui::Id::new("json_utility_dialog"))
            .collapsible(false)
            .resizable(true)
            .default_size((560.0, 380.0))
            .min_size((340.0, 220.0))
            .open(&mut open)
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    let actions = if self.intent == JsonUtilityIntent::Minify {
                        [("Minify", Transform::Minify), ("Format", Transform::Format)]
                    } else {
                        [("Format", Transform::Format), ("Minify", Transform::Minify)]
                    };
                    for (label, transform) in actions {
                        if ui.button(label).clicked() {
                            self.editor.transform(transform);
                        }
                    }
                    if ui.button("Copy Result").clicked() {
                        self.editor.copy(&ArboardClipboardBackend);
                    }
                    if ui.button("Paste from Clipboard").clicked() {
                        self.editor.reload(&ArboardClipboardBackend);
                    }
                    if ui.button("Clear").clicked() {
                        self.editor.clear();
                        self.focus_editor = true;
                    }
                });
                if let Some(error) = &self.editor.parse_error {
                    ui.colored_label(ui.visuals().error_fg_color, &error.message);
                } else if let Some(feedback) = &self.editor.feedback {
                    match feedback {
                        Ok(message) => {
                            ui.label(message);
                        }
                        Err(message) => {
                            ui.colored_label(ui.visuals().error_fg_color, message);
                        }
                    }
                } else {
                    ui.weak("Strict JSON · Copy Result writes only when requested");
                }
                let editor_size = ui.available_size();
                egui::ScrollArea::both()
                    .id_source("json_utility_editor_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let response = ui.add_sized(
                            editor_size,
                            egui::TextEdit::multiline(&mut self.editor.text)
                                .id(egui::Id::new("json_utility_editor"))
                                .font(egui::TextStyle::Monospace)
                                .desired_width(f32::INFINITY)
                                .desired_rows(12)
                                .hint_text("Paste or type JSON here"),
                        );
                        if self.focus_editor {
                            response.request_focus();
                            self.focus_editor = false;
                        }
                        if response.changed() {
                            self.editor.reset_feedback();
                        }
                    });
            });
        self.open = open;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clipboard_modify::clipboard::ClipboardError;
    use std::sync::Mutex;

    struct FakeClipboard {
        text: Mutex<Result<String, ClipboardError>>,
        writes: Mutex<Vec<String>>,
        fail_write: bool,
    }

    impl FakeClipboard {
        fn new(text: &str) -> Self {
            Self {
                text: Mutex::new(Ok(text.to_owned())),
                writes: Mutex::new(Vec::new()),
                fail_write: false,
            }
        }
    }

    impl ClipboardBackend for FakeClipboard {
        fn read_text(&self) -> Result<String, ClipboardError> {
            self.text.lock().unwrap().clone()
        }
        fn write_text(&self, text: &str) -> Result<(), ClipboardError> {
            if self.fail_write {
                return Err(ClipboardError::Busy("occupied".into()));
            }
            self.writes.lock().unwrap().push(text.to_owned());
            Ok(())
        }
    }

    #[test]
    fn each_open_loads_only_valid_strict_clipboard_json_and_keeps_intent() {
        let clipboard = FakeClipboard::new(" {\"z\":1} ");
        let mut dialog = JsonUtilityDialogState::default();
        dialog.open_with_clipboard(JsonUtilityIntent::Minify, &clipboard);
        assert!(dialog.open);
        assert_eq!(dialog.intent, JsonUtilityIntent::Minify);
        assert_eq!(dialog.editor.text, " {\"z\":1} ");
        dialog.editor.transform(Transform::Format);
        *clipboard.text.lock().unwrap() = Ok("[1,]".into());
        dialog.open_with_clipboard(JsonUtilityIntent::Format, &clipboard);
        assert_eq!(dialog.intent, JsonUtilityIntent::Format);
        assert!(dialog.editor.text.is_empty());
        assert!(dialog.editor.parse_error.is_none());
        *clipboard.text.lock().unwrap() = Err(ClipboardError::NonText);
        dialog.open_with_clipboard(JsonUtilityIntent::General, &clipboard);
        assert!(dialog.editor.text.is_empty());
        assert!(clipboard.writes.lock().unwrap().is_empty());
    }

    #[test]
    fn format_and_minify_share_one_editable_session_without_copying() {
        let clipboard = FakeClipboard::new(" { \"z\": [1,true], \"a\": null } ");
        let mut editor = JsonEditorState::default();
        editor.initialize(&clipboard);
        editor.transform(Transform::Format);
        assert_eq!(
            editor.text,
            "{\n  \"z\": [\n    1,\n    true\n  ],\n  \"a\": null\n}"
        );
        editor.transform(Transform::Minify);
        assert_eq!(editor.text, r#"{"z":[1,true],"a":null}"#);
        assert!(editor.parse_error.is_none());
        assert!(clipboard.writes.lock().unwrap().is_empty());
    }

    #[test]
    fn parse_failure_preserves_exact_input_and_location_and_never_copies() {
        let clipboard = FakeClipboard::new("{\n  \"value\": }\n");
        let mut editor = JsonEditorState::default();
        editor.reload(&clipboard);
        let original = editor.text.clone();
        for transform in [Transform::Format, Transform::Minify] {
            editor.transform(transform);
            assert_eq!(editor.text, original);
            let error = editor.parse_error.as_ref().unwrap();
            assert_eq!(error.line, Some(2));
            assert!(error.column.is_some_and(|column| column > 0));
            assert!(error.message.contains("line 2 column"));
        }
        editor.copy(&clipboard);
        assert_eq!(editor.text, original);
        assert!(editor.parse_error.is_some());
        assert!(clipboard.writes.lock().unwrap().is_empty());
    }

    #[test]
    fn clear_and_explicit_reload_reset_errors_and_allow_invalid_input() {
        let clipboard = FakeClipboard::new("not JSON");
        let mut editor = JsonEditorState::default();
        editor.reload(&clipboard);
        assert_eq!(editor.text, "not JSON");
        editor.transform(Transform::Format);
        assert!(editor.parse_error.is_some());
        editor.clear();
        assert!(editor.text.is_empty());
        assert!(editor.parse_error.is_none());
        editor.reload(&clipboard);
        assert_eq!(editor.text, "not JSON");
        assert!(editor.parse_error.is_none());
        *clipboard.text.lock().unwrap() = Err(ClipboardError::NonText);
        editor.reload(&clipboard);
        assert_eq!(editor.text, "not JSON");
        assert!(matches!(editor.feedback, Some(Err(_))));
    }

    #[test]
    fn copy_validates_and_copies_current_text_only_on_explicit_action() {
        let mut clipboard = FakeClipboard::new("  [ 1, true ]  ");
        let mut editor = JsonEditorState::default();
        editor.initialize(&clipboard);
        editor.copy(&clipboard);
        assert_eq!(*clipboard.writes.lock().unwrap(), vec!["  [ 1, true ]  "]);
        assert!(matches!(editor.feedback, Some(Ok(_))));
        clipboard.fail_write = true;
        editor.copy(&clipboard);
        assert!(matches!(editor.feedback, Some(Err(_))));
        assert_eq!(clipboard.writes.lock().unwrap().len(), 1);
        assert_eq!(editor.text, "  [ 1, true ]  ");
    }
}
