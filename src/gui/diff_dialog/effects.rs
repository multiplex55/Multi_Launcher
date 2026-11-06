//! File effects produced by a Diff view and applied by its workspace owner.
use crate::diff::model::{DiffSide, TextViewModel};

pub(super) enum ViewEffect {
    SaveText(TextSaveRequest),
    Export(super::export::ExportRequest),
    SetTextRules(crate::diff::text_compare::TextComparisonRules),
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
