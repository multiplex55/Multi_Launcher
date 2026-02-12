use eframe::egui;

use super::ActivationSource;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmationResult {
    None,
    Confirmed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DestructiveAction {
    ClearClipboard,
    ClearHistory,
    ClearTodos,
    DeleteTodo,
    DeleteNote,
    DeleteGesture,
    ClearTempfiles,
    ClearBrowserTabCache,
    EmptyRecycleBin,
    ResetWidgetSettings,
}

impl DestructiveAction {
    pub fn from_action(action: &crate::actions::Action) -> Option<Self> {
        match action.action.as_str() {
            "clipboard:clear" => Some(Self::ClearClipboard),
            "history:clear" => Some(Self::ClearHistory),
            "todo:clear" => Some(Self::ClearTodos),
            "tempfile:clear" => Some(Self::ClearTempfiles),
            "tab:clear" => Some(Self::ClearBrowserTabCache),
            "recycle:clean" => Some(Self::EmptyRecycleBin),
            _ if action.action.starts_with("todo:remove:") => Some(Self::DeleteTodo),
            _ if action.action.starts_with("note:remove:") => Some(Self::DeleteNote),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::ClearClipboard => "Clear clipboard history",
            Self::ClearHistory => "Clear search history",
            Self::ClearTodos => "Clear completed todos",
            Self::DeleteTodo => "Delete todo",
            Self::DeleteNote => "Delete note",
            Self::DeleteGesture => "Delete gesture",
            Self::ClearTempfiles => "Clear temp files",
            Self::ClearBrowserTabCache => "Clear browser tab cache",
            Self::EmptyRecycleBin => "Empty recycle bin",
            Self::ResetWidgetSettings => "Reset widget settings",
        }
    }

    pub fn warning(self) -> &'static str {
        "This action cannot be undone."
    }
}

#[cfg(test)]
mod tests {
    use super::DestructiveAction;
    use crate::actions::Action;

    #[test]
    fn from_action_maps_note_remove() {
        let action = Action {
            label: "Delete note".into(),
            desc: "Notes".into(),
            action: "note:remove:project-idea".into(),
            args: None,
        };

        assert_eq!(
            DestructiveAction::from_action(&action),
            Some(DestructiveAction::DeleteNote)
        );
    }
}

#[derive(Debug, Clone)]
pub struct ConfirmationModal {
    open: bool,
    title: String,
    description: String,
    warning: String,
    confirm_label: String,
    cancel_label: String,
    source_label: Option<String>,
}

impl Default for ConfirmationModal {
    fn default() -> Self {
        Self {
            open: false,
            title: "Confirm destructive action".into(),
            description: String::new(),
            warning: "This action cannot be undone.".into(),
            confirm_label: "Confirm".into(),
            cancel_label: "Cancel".into(),
            source_label: None,
        }
    }
}

impl ConfirmationModal {
    pub fn open_for(&mut self, kind: DestructiveAction) {
        self.open_for_source(kind, None);
    }

    pub fn open_for_source(&mut self, kind: DestructiveAction, source: Option<ActivationSource>) {
        self.title = "Confirm destructive action".into();
        self.description = kind.label().into();
        self.warning = kind.warning().into();
        self.confirm_label = "Confirm".into();
        self.cancel_label = "Cancel".into();
        self.source_label = source.map(|source| format!("Triggered by {}", source.label()));
        self.open = true;
    }

    pub fn ui(&mut self, ctx: &egui::Context) -> ConfirmationResult {
        if !self.open {
            return ConfirmationResult::None;
        }
        let mut result = ConfirmationResult::None;
        let mut open = true;
        egui::Window::new(self.title.clone())
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .open(&mut open)
            .show(ctx, |ui| {
                if !self.description.is_empty() {
                    ui.label(&self.description);
                }
                if let Some(label) = &self.source_label {
                    ui.label(label);
                }
                ui.colored_label(egui::Color32::YELLOW, &self.warning);
                ui.horizontal(|ui| {
                    if ui.button(&self.confirm_label).clicked() {
                        result = ConfirmationResult::Confirmed;
                    }
                    if ui.button(&self.cancel_label).clicked() {
                        result = ConfirmationResult::Cancelled;
                    }
                });
            });
        if result != ConfirmationResult::None {
            self.open = false;
        }
        if !open {
            self.open = false;
            if result == ConfirmationResult::None {
                result = ConfirmationResult::Cancelled;
            }
        }
        result
    }
}
