use crate::draw::model::CanvasModel;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExitReason {
    UserRequest,
    Timeout,
    StartFailure,
    OverlayFailure,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveResult {
    Saved,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MainToOverlay {
    Start,
    RequestExit { reason: ExitReason },
    UpdateSettings,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverlayToMain {
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
