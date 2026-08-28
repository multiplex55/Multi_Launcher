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
    /// Converts the typed path to the legacy positional representation used by
    /// non-image condition editors. A `Not` node has one child at index zero.
    pub fn indexes(&self) -> Vec<usize> {
        self.0
            .iter()
            .map(|branch| match *branch {
                ConditionBranch::All(index)
                | ConditionBranch::Any(index)
                | ConditionBranch::Index(index) => index,
                ConditionBranch::Not => 0,
            })
            .collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_condition_path_converts_to_window_picker_indexes() {
        let mut path = ConditionPath::root();
        path.prepend(ConditionBranch::Any(4));
        path.prepend(ConditionBranch::Not);
        path.prepend(ConditionBranch::All(2));

        assert_eq!(path.indexes(), vec![2, 0, 4]);
        assert_eq!(
            path.branches(),
            &[
                ConditionBranch::All(2),
                ConditionBranch::Not,
                ConditionBranch::Any(4),
            ]
        );
    }
}
