//! Immutable destinations shared by condition editing and background image authoring.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ConditionBranch {
    All(usize),
    Any(usize),
    Not,
    Index(usize),
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ConditionPath(Vec<ConditionBranch>);
impl ConditionPath {
    pub fn root() -> Self {
        Self::default()
    }
    pub fn from_indexes(indexes: impl Into<Vec<usize>>) -> Self {
        Self(
            indexes
                .into()
                .into_iter()
                .map(ConditionBranch::Index)
                .collect(),
        )
    }
    pub fn branches(&self) -> &[ConditionBranch] {
        &self.0
    }
    pub(crate) fn prepend(&mut self, branch: ConditionBranch) {
        self.0.insert(0, branch);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConditionImageOperation {
    ImportPng,
    CaptureRectangle,
    PickRectangle,
    PreviewRectangle,
    HighlightMonitor,
    PickWindow,
    HighlightWindow,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConditionOperationDestination {
    pub macro_id: u64,
    pub step_id: Option<u64>,
    pub draft_generation: u64,
    pub path: ConditionPath,
    pub operation: ConditionImageOperation,
}
