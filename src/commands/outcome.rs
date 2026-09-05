#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QueryPolicy {
    Keep,
    Set(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VisibilityPolicy {
    Keep,
    Show,
    Hide,
    Toggle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryPolicy {
    Skip,
    Record,
    AlreadyApplied,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FavoriteLogPolicy {
    None,
    Ran { label: String, command: String },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToastPolicy {
    Launched(String),
    Copied(String),
    Info(String),
    Success(String),
    Error(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum ResultsPolicy {
    Keep,
    Replace(Vec<crate::actions::Action>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct CommandOutcome {
    pub query: QueryPolicy,
    pub search: bool,
    pub invalidate_results: bool,
    pub results: ResultsPolicy,
    pub visibility: VisibilityPolicy,
    pub restore: bool,
    pub focus: bool,
    pub move_cursor_end: bool,
    pub activate_first_result: Option<crate::commands::ActivationSource>,
    pub history: HistoryPolicy,
    pub toasts: Vec<ToastPolicy>,
    pub favorite_log: FavoriteLogPolicy,
}

impl Default for CommandOutcome {
    fn default() -> Self {
        Self {
            query: QueryPolicy::Keep,
            search: false,
            invalidate_results: false,
            results: ResultsPolicy::Keep,
            visibility: VisibilityPolicy::Keep,
            restore: false,
            focus: false,
            move_cursor_end: false,
            activate_first_result: None,
            history: HistoryPolicy::Skip,
            toasts: Vec::new(),
            favorite_log: FavoriteLogPolicy::None,
        }
    }
}

impl CommandOutcome {
    pub fn query(query: String) -> Self {
        Self {
            query: QueryPolicy::Set(query),
            search: true,
            visibility: VisibilityPolicy::Show,
            restore: true,
            focus: true,
            move_cursor_end: true,
            ..Self::default()
        }
    }
}
