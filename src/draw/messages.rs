use crate::draw::model::{CanvasModel, Color, Tool};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitDialogMode {
    Hidden,
    PromptVisible,
    Saving,
    ErrorVisible,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExitReason {
    UserRequest,
    Timeout,
    StartFailure,
    OverlayFailure,
    LauncherHotkey,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveResult {
    Saved,
    Skipped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayCommand {
    SelectTool(Tool),
    SetStrokeWidth(u32),
    SetColor(Color),
    SetFillEnabled(bool),
    SetFillColor(Color),
    Undo,
    Redo,
    Save,
    ToggleToolbarVisibility,
    ToggleToolbarCollapsed,
    SetToolbarPosition { x: i32, y: i32 },
    Exit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MainToOverlay {
    Start,
    SetExitDialogMode { mode: ExitDialogMode },
    RequestExit { reason: ExitReason },
    UpdateSettings,
    DispatchCommand { command: OverlayCommand },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverlayToMain {
    LifecycleEvent(OverlayLifecycleEvent),
    Exited {
        reason: ExitReason,
        save_result: SaveResult,
    },
    SaveProgress {
        canvas: CanvasModel,
    },
    SaveError {
        error: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverlayLifecycleEvent {
    Started,
    ExitRequested { reason: ExitReason },
    SettingsApplied,
}
