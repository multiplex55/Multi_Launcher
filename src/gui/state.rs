use crate::actions::Action;
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivationSource {
    Enter,
    Click,
    Dashboard,
    Gesture,
}

impl ActivationSource {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Enter => "enter",
            Self::Click => "click",
            Self::Dashboard => "dashboard",
            Self::Gesture => "gesture",
        }
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
pub(crate) struct PendingConfirmAction {
    pub(crate) action: Action,
    pub(crate) query_override: Option<String>,
    pub(crate) source: ActivationSource,
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
        }
    }
}
