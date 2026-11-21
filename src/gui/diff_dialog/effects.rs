//! File effects produced by a Diff view and applied by its workspace owner.
use crate::diff::model::{DiffSide, TextViewModel};

pub(super) enum ViewEffect {
    SaveText(TextSaveRequest),
    CopyHunk(HunkCopyRequest),
    Export(super::export::ExportRequest),
    SetTextRules(crate::diff::text_compare::TextComparisonRules),
    OpenTextRules,
    ResolveTextConflict { side: DiffSide, reload: bool },
}

#[derive(Clone, Copy)]
pub(super) struct TextSaveRequest {
    side: DiffSide,
    source_id: u64,
    revision: u64,
}

impl TextSaveRequest {
    pub(super) fn capture(model: &TextViewModel, side: DiffSide) -> Self {
        let document = match side {
            DiffSide::Left => &model.left,
            DiffSide::Right => &model.right,
        };
        Self {
            side,
            source_id: document.source_id(),
            revision: document.revision,
        }
    }

    pub(super) fn apply(self, model: &mut TextViewModel, picker: PickerOwner) -> bool {
        if !self.matches(model) {
            return false;
        }
        super::text_view::save_side_in_host(model, self.side, picker)
    }

    fn matches(self, model: &TextViewModel) -> bool {
        let current = Self::capture(model, self.side);
        current.source_id == self.source_id && current.revision == self.revision
    }

    pub(super) fn targets_side(self, side: DiffSide) -> bool {
        self.side == side
    }
}

#[derive(Clone, Copy)]
pub(super) struct HunkCopyRequest {
    from: DiffSide,
    row: Option<usize>,
    left_source_id: u64,
    right_source_id: u64,
    left_revision: u64,
    right_revision: u64,
    rules_revision: u64,
    annotation_revision: u64,
}

impl HunkCopyRequest {
    pub(super) fn capture(model: &TextViewModel, from: DiffSide) -> Self {
        Self {
            from,
            row: model.current_row,
            left_source_id: model.left.source_id(),
            right_source_id: model.right.source_id(),
            left_revision: model.left.revision,
            right_revision: model.right.revision,
            rules_revision: model.rules.revision,
            annotation_revision: model.annotation_revision,
        }
    }

    pub(super) fn apply(self, model: &mut TextViewModel) {
        if !self.matches(model) {
            return;
        }
        super::text_view::apply_hunk_copy(model, self.from, self.row);
    }

    fn matches(self, model: &TextViewModel) -> bool {
        self.left_source_id == model.left.source_id()
            && self.right_source_id == model.right.source_id()
            && self.left_revision == model.left.revision
            && self.right_revision == model.right.revision
            && self.rules_revision == model.rules.revision
            && self.annotation_revision == model.annotation_revision
    }
}

#[derive(Clone, Copy)]
pub(super) enum PickerOwner {
    Legacy,
    Hosted(Option<usize>),
}

impl PickerOwner {
    pub(super) fn dialog(self) -> Result<rfd::FileDialog, &'static str> {
        let dialog = rfd::FileDialog::new();
        #[cfg(windows)]
        if let Self::Hosted(hwnd) = self {
            let hwnd = hwnd
                .and_then(|hwnd| std::num::NonZeroIsize::new(hwnd as isize))
                .ok_or("Diff cannot verify its file picker window. Focus Diff and try again.")?;
            return Ok(dialog.set_parent(&crate::gui::add_action_dialog::NativeDialogParent(hwnd)));
        }
        #[cfg(not(windows))]
        let _ = self;
        Ok(dialog)
    }
}
