//! Checked File Search origins shared by retained child utilities.
use super::*;
use multi_launcher::batch_rename::ExplicitDirectoryRoot;

#[derive(Clone)]
pub(super) enum HandoffTarget { FolderSize(ExplicitDirectoryRoot), FileLock(PathBuf) }

#[derive(Clone)]
pub(super) struct FileSearchOrigin {
    pub(super) receipt: super::file_search::SelectionReceipt,
    pub(super) mount: utility_mount::MountReceipt,
    pub(super) host: AnyWindowHandle,
    pub(super) native: NativeWindow,
    pub(super) focus: FocusHandle,
    pub(super) popout: bool,
    pub(super) root_revision: u64,
    pub(super) target: HandoffTarget,
    pub(super) fresh: bool,
}
impl FileSearchOrigin {
    pub(super) fn folder_root(&self) -> Option<&ExplicitDirectoryRoot> {
        if let HandoffTarget::FolderSize(root) = &self.target { Some(root) } else { None }
    }
}
impl RootWindow {
    pub(super) fn file_search_origin_current(&self, origin: &FileSearchOrigin, foreground: ForegroundIdentity, cx: &mut Context<Self>) -> bool {
        if self.restart_admission.is_fenced() || !origin.mount.is_current() || !native_window_is_valid(origin.native)
            || (!origin.popout && !json_utility::native_host_current(&self.root_visibility, origin.root_revision, Some(origin.native), false))
            || current_foreground_identity() != Some(foreground) || self.action_sheet.is_some() || self.context_favorite_popup.is_some() { return false; }
        let current = self.file_search.as_ref().is_some_and(|view| view.entity_id() == origin.receipt.owner
            && view.read(cx).is_mounted_in(origin.host.window_id(), origin.popout)
            && !view.read(cx).rehosting && view.read(cx).receipt_is_current(&origin.receipt, view.entity_id())
            && match &origin.target {
                HandoffTarget::FolderSize(_) => true,
                HandoffTarget::FileLock(path) => path == &origin.receipt.path
                    && view.read(cx).eligible_lock_receipt(&origin.receipt, view.entity_id()),
            });
        current && origin.host.update(cx, |_, window, app| {
            let contexts = window.context_stack();
            !window.has_active_dialog(app) && origin.focus.is_focused(window)
                && !context_stack_has_primary(&contexts, "PopupMenu")
                && !context_stack_has_primary(&contexts, "Dialog")
                && !context_stack_has_primary(&contexts, "AlertDialog")
        }).unwrap_or(false)
    }
    pub(super) fn file_search_origin_error(&self, origin: &FileSearchOrigin, message: &str, cx: &mut Context<Self>) {
        if !self.file_search_origin_current(origin, origin.native.identity, cx) { return; }
        if let Some(view) = self.file_search.clone() { view.update(cx, |view, cx| match &origin.target {
            HandoffTarget::FolderSize(_) => view.report_folder_size_error(message, cx),
            HandoffTarget::FileLock(_) => view.report_file_lock_error(message, cx),
        }); }
    }
    pub(super) fn abort_file_search_handoff(&mut self, origin: &FileSearchOrigin, owner: EntityId,
        expected: &utility_mount::MountReceipt, message: &str, cx: &mut Context<Self>) {
        match &origin.target {
            HandoffTarget::FolderSize(_) => if let Some(view) = self.folder_size_view.clone().filter(|v| v.entity_id() == owner) {
                view.update(cx, |view, cx| { if expected.is_current() && expected.matches(&view.mount) {
                    if origin.fresh { view.staging = false; view.shutdown(); } else { view.rehosting = false; } cx.notify();
                } });
            },
            HandoffTarget::FileLock(_) => if let Some(view) = self.file_lock_view.clone().filter(|v| v.entity_id() == owner) {
                view.update(cx, |view, cx| { if expected.is_current() && expected.matches(&view.mount) {
                    if origin.fresh { view.staging = false; view.shutdown(); } else { view.rehosting = false; } cx.notify();
                } });
            },
        }
        self.file_search_origin_error(origin, message, cx);
    }
    pub(super) fn commit_file_search_handoff(&mut self, origin: &FileSearchOrigin, owner: EntityId,
        host: WindowId, cx: &mut Context<Self>) -> bool {
        let applied = match &origin.target {
            HandoffTarget::FolderSize(root) => self.folder_size_view.clone().filter(|v| v.entity_id() == owner).is_some_and(|view|
                view.update(cx, |view, cx| { if !view.is_mounted_in(host, true) || !view.rehosting { return false; }
                    view.prefill(root.clone(), cx); view.staging = false; true })),
            HandoffTarget::FileLock(path) => self.file_lock_view.clone().filter(|v| v.entity_id() == owner).is_some_and(|view|
                view.update(cx, |view, cx| { if !view.is_mounted_in(host, true) || !view.rehosting { return false; }
                    view.prefill(path.clone(), cx); view.staging = false; true })),
        };
        if applied && matches!(&origin.target, HandoffTarget::FileLock(_)) {
            if let Some(source) = self.file_search.clone().filter(|view| view.entity_id() == origin.receipt.owner) {
                source.update(cx, |view, cx| view.clear_file_lock_error(&origin.receipt, cx));
            }
        }
        applied
    }
}
