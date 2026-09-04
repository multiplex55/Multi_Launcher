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
    pub(crate) fn push(&mut self, branch: ConditionBranch) {
        self.0.push(branch);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConditionImageOperation {
    ImportPng,
    CaptureRectangle,
    CropImage,
    PickRectangle,
    PreviewRectangle,
    HighlightMonitor,
    PickWindow,
    HighlightWindow,
}

/// Immutable identity of the image-search field that launched a crop.
///
/// The destination deliberately captures the source reference as well as the
/// editor identity. A Save As completion may arrive after the draft has been
/// edited, so the action editor must be able to prove that the field still
/// refers to the image that was opened in the crop editor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImageCropDestination {
    ImageActionReference {
        macro_id: u64,
        step_id: Option<u64>,
        draft_generation: u64,
        source: crate::mkmacro::MkImageRef,
    },
    ConditionImage {
        macro_id: u64,
        step_id: Option<u64>,
        draft_generation: u64,
        source: crate::mkmacro::MkImageRef,
        path: ConditionPath,
    },
}

impl ImageCropDestination {
    pub fn macro_id(&self) -> u64 {
        match self {
            Self::ImageActionReference { macro_id, .. } | Self::ConditionImage { macro_id, .. } => {
                *macro_id
            }
        }
    }

    pub fn step_id(&self) -> Option<u64> {
        match self {
            Self::ImageActionReference { step_id, .. } | Self::ConditionImage { step_id, .. } => {
                *step_id
            }
        }
    }

    pub fn draft_generation(&self) -> u64 {
        match self {
            Self::ImageActionReference {
                draft_generation, ..
            }
            | Self::ConditionImage {
                draft_generation, ..
            } => *draft_generation,
        }
    }

    pub fn source(&self) -> &crate::mkmacro::MkImageRef {
        match self {
            Self::ImageActionReference { source, .. } | Self::ConditionImage { source, .. } => {
                source
            }
        }
    }
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
