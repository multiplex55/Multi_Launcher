use crate::actions::Action;
pub use crate::commands::ActivationSource;
use crate::dashboard::DashboardEvent;
use std::fmt::Display;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClipboardModifyGuiEvent {
    ImmediateOperationComplete,
    ImmediateOperationFailed,
    ConfigurationReloadSuccess,
    ConfigurationReloadFailure(String),
    StartupDiagnosticChanged(Option<String>),
}

#[derive(Clone)]
pub enum WatchEvent {
    Actions,
    Folders,
    Bookmarks,
    Clipboard,
    Snippets,
    Notes,
    Todos,
    Favorites,
    Gestures,
    Dashboard(DashboardEvent),
    Recycle(Result<(), String>),
    ExecuteAction(Action),
    ClipboardModify(ClipboardModifyGuiEvent),
    VirtualDesktop(VirtualDesktopGuiCompletion),
}

#[derive(Clone, Debug)]
pub struct VirtualDesktopGuiCompletion {
    pub invocation: crate::commands::CommandInvocation,
    pub completion_outcome: crate::commands::CommandOutcome,
    pub history_query: String,
    pub interaction_token: u64,
    pub expected_query: String,
    pub expected_visible: bool,
    pub result: Result<(), String>,
}

#[cfg(test)]
mod tests {
    use crate::commands::ActivationSource;

    #[test]
    fn activation_source_labels_are_stable() {
        assert_eq!(ActivationSource::Enter.label(), "enter");
        assert_eq!(ActivationSource::Click.label(), "click");
        assert_eq!(ActivationSource::Dashboard.label(), "dashboard");
        assert_eq!(ActivationSource::Gesture.label(), "gesture");
        assert_eq!(ActivationSource::Macro.label(), "macro");
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiErrorEvent {
    pub context: &'static str,
    pub message: String,
}

impl UiErrorEvent {
    pub fn new(context: &'static str, err: impl Display) -> Self {
        Self {
            context,
            message: err.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ResultContextMenuKind {
    Folder,
    Bookmark,
    Timer { id: u64 },
    Stopwatch { id: u64 },
    Snippet,
    Tempfile,
    Note { slug: String },
    Clipboard { idx: usize, label: String },
    Todo { idx: usize },
    Default,
}

#[derive(Clone)]
pub(crate) struct PendingConfirmCommand {
    pub(crate) invocation: crate::commands::CommandInvocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TestWatchEvent {
    Actions,
    Folders,
    Bookmarks,
    ClipboardModify(ClipboardModifyGuiEvent),
}

impl From<WatchEvent> for TestWatchEvent {
    fn from(value: WatchEvent) -> Self {
        match value {
            WatchEvent::Actions => TestWatchEvent::Actions,
            WatchEvent::Folders => TestWatchEvent::Folders,
            WatchEvent::Bookmarks => TestWatchEvent::Bookmarks,
            WatchEvent::Clipboard => TestWatchEvent::Actions,
            WatchEvent::Snippets => TestWatchEvent::Actions,
            WatchEvent::Notes => TestWatchEvent::Actions,
            WatchEvent::Todos => TestWatchEvent::Actions,
            WatchEvent::Favorites => TestWatchEvent::Actions,
            WatchEvent::Gestures => TestWatchEvent::Actions,
            WatchEvent::Dashboard(_) => TestWatchEvent::Actions,
            WatchEvent::Recycle(_) => unreachable!(),
            WatchEvent::ExecuteAction(_) => TestWatchEvent::Actions,
            WatchEvent::ClipboardModify(event) => TestWatchEvent::ClipboardModify(event),
            WatchEvent::VirtualDesktop(_) => TestWatchEvent::Actions,
        }
    }
}
