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
pub struct CommandOutcome {
    pub query: QueryPolicy,
    pub search: bool,
    pub visibility: VisibilityPolicy,
    pub restore: bool,
    pub focus: bool,
    pub move_cursor_end: bool,
    pub activate_first_result: Option<crate::commands::ActivationSource>,
    pub history: HistoryPolicy,
}

impl Default for CommandOutcome {
    fn default() -> Self {
        Self {
            query: QueryPolicy::Keep,
            search: false,
            visibility: VisibilityPolicy::Keep,
            restore: false,
            focus: false,
            move_cursor_end: false,
            activate_first_result: None,
            history: HistoryPolicy::Skip,
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
