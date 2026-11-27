//! Explorer drop admission and ordered canonical folder/app staging.
use super::*;
use multi_launcher::plugins::folders::{self, FolderEntry};

pub(super) const WAIT: &str = "Wait for the dropped paths to finish processing.";

struct QueuedAlias { entry: FolderEntry, native: NativeWindow, revision: u64, focus: Option<FocusHandle>, draft: Option<EntityId>, query: String }
#[derive(Default)]
pub(super) struct State { serial: u64, pending: bool, pointer_down: bool, lease: Option<Arc<AtomicBool>>, alias: Option<QueuedAlias> }
impl State {
    pub(super) fn busy(&self) -> bool { self.pending }
    pub(super) fn pointer_down(&mut self) { self.pointer_down = true; self.revoke_inflight(); }
    pub(super) fn pointer_up(&mut self) { self.pointer_down = false; }
    pub(super) fn revoke_inflight(&mut self) {
        if let Some(lease) = &self.lease { lease.store(false, Ordering::Release); }
    }
    pub(super) fn revoke(&mut self) { self.revoke_inflight(); self.alias = None; }
    fn begin(&mut self) -> Option<(u64, Arc<AtomicBool>)> {
        if self.pending { return None; }
        self.revoke(); self.serial = self.serial.wrapping_add(1).max(1); self.pending = true;
        let lease = Arc::new(AtomicBool::new(true)); self.lease = Some(lease.clone()); Some((self.serial, lease))
    }
    fn finish(&mut self, serial: u64) -> bool {
        if !self.pending || self.serial != serial { return false; }
        self.pending = false; self.lease = None; true
    }
}

fn native_pointer_pressed() -> bool {
    #[cfg(windows)]
    {
        use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON, VK_RBUTTON, VK_MBUTTON, VK_XBUTTON1, VK_XBUTTON2};
        [VK_LBUTTON, VK_RBUTTON, VK_MBUTTON, VK_XBUTTON1, VK_XBUTTON2].into_iter()
            .any(|key| unsafe { GetAsyncKeyState(i32::from(key.0)) } < 0)
    }
    #[cfg(not(windows))]
    { false }
}

fn can_deliver(pointer_down: bool, native_pressed: bool) -> bool { !pointer_down && !native_pressed }

fn native_visible(visibility: &RootVisibility, native: NativeWindow, revision: u64) -> bool {
    #[cfg(windows)]
    { visibility.is_current_visible_revision(revision, native) && checked_hash_popup_visible(native) }
    #[cfg(not(windows))]
    { let _ = native; visibility.is_current(revision, true) }
}

fn empty_focus_owner_current<T>(origin: Option<&T>, current: Option<&T>, blocked: bool) -> bool {
    !blocked && origin.is_none() && current.is_none()
}

pub(super) fn same_query_projection(current: &str, next: &str) -> bool { current == next }

fn delivery_current(lease: bool, owner: bool, draft_available: bool) -> bool {
    lease && owner && draft_available
}

#[derive(Default)]
struct Processed { last_file: Option<String>, last_folder: Option<FolderEntry>, errors: Vec<String>, committed: bool }
fn process_paths(paths: Vec<PathBuf>, store: &str, current: impl Fn() -> bool) -> Processed {
    let mut result = Processed::default();
    for path in paths {
        if !current() { break; }
        let Some(text) = path.to_str().filter(|value| !value.contains(['\r', '\n'])) else {
            result.errors.push("A dropped path cannot be represented in the editor.".into()); continue;
        };
        if path.is_dir() {
            match folders::checked_add_folder(store, text, &current) {
                Ok(entries) => {
                    result.committed = true;
                    let mut matches = entries.into_iter().filter(|entry| entry.path == text);
                    if let Some(entry) = matches.next() && matches.next().is_none() { result.last_folder = Some(entry); }
                    else { result.errors.push(format!("Folder is ambiguous: {text}")); }
                }
                Err(error) => result.errors.push(format!("Could not add folder {text}: {error:#}")),
            }
        } else if path.is_file() { result.last_file = Some(text.to_owned()); }
        else { result.errors.push(format!("Dropped path is unavailable: {text}")); }
    }
    result
}

impl RootWindow {
    pub(super) fn drop_focus_current(&self, origin: Option<&FocusHandle>, window: &Window, cx: &App) -> bool {
        let context = window.context_stack();
        let blocked = self.action_sheet.is_some() || self.context_favorite_popup.is_some()
            || self.file_menu_open || self.settings_menu_open || self.help_menu_open
            || context_stack_has_primary(&context, "PopupMenu")
            || context_stack_has_primary(&context, "AlertDialog")
            || context_stack_has_primary(&context, "Dialog");
        empty_focus_owner_current(origin, window.focused(cx).as_ref(), blocked)
            || self.external_owner_is_current(origin, window, cx)
    }
    fn drop_owner_current(&self, native: NativeWindow, revision: u64, origin: Option<&FocusHandle>, window: &mut Window, cx: &mut Context<Self>) -> bool {
        !self.restart_admission.is_fenced() && self.root_window.is_some_and(|host| host.window_id() == window.window_handle().window_id())
            && self.native_window == Some(native) && native_visible(&self.root_visibility, native, revision)
            && !window.has_active_dialog(cx) && self.action_sheet.is_none() && self.context_favorite_popup.is_none()
            && self.drop_focus_current(origin, window, cx)
            && !self.folder_authoring.busy() && !self.bookmark_authoring.busy()
            && !self.hash_file_picker_pending.load(Ordering::Acquire) && !self.external_launch.is_pending()
            && !self.external_launch.auxiliary_effect_pending() && !self.structured_pins.busy()
    }

    pub(super) fn receive_external_drop(&mut self, paths: Vec<PathBuf>, window: &mut Window, cx: &mut Context<Self>) {
        if paths.is_empty() || self.external_drop.busy() || !self.app_editor.external_drop_safe() { return; }
        let Some(native) = self.native_window else { return; };
        let revision = self.root_visibility.request_revision.load(Ordering::Acquire);
        let origin = window.focused(cx); let issued_draft = self.app_editor.drop_draft_id();
        if !self.drop_owner_current(native, revision, origin.as_ref(), window, cx) { return; }
        let Some(permit) = self.restart_admission.admit() else { return; };
        let Some((serial, lease)) = self.external_drop.begin() else { return; };
        let visibility = self.root_visibility.clone(); let admission = self.restart_admission.clone();
        let task = cx.background_executor().spawn(async move {
            process_paths(paths, folders::FOLDERS_FILE, || lease.load(Ordering::Acquire) && !admission.is_fenced() && native_visible(&visibility, native, revision))
        });
        let root = cx.entity().downgrade(); let host = window.window_handle();
        cx.spawn(async move |_, cx| {
            let result = task.await;
            let updated = host.update(cx, |_, window, app| { let _ = root.update(app, |root, cx| {
                if !root.external_drop.pending || root.external_drop.serial != serial { return; }
                let lease_current = root.external_drop.lease.as_ref().is_some_and(|lease| lease.load(Ordering::Acquire));
                let current = delivery_current(lease_current, root.drop_owner_current(native, revision, origin.as_ref(), window, cx),
                    root.app_editor.external_drop_safe() && root.app_editor.drop_draft_id() == issued_draft);
                root.external_drop.finish(serial);
                if root.restart_status.as_deref() == Some(WAIT) { root.restart_status = None; }
                if result.committed { root.folder_updated(false, cx); }
                if current {
                    let focused = window.is_window_active() && current_foreground_identity() == Some(native.identity);
                    let mut errors = result.errors;
                    if let Some(path) = result.last_file {
                        if root.app_editor.accepts_external_drop() { root.open_dropped_app_draft(&path, focused, window, cx); }
                        else { errors.push("Save or cancel the current App Editor draft before dropping a file.".into()); }
                    }
                    if let Some(entry) = result.last_folder {
                        root.external_drop.alias = Some(QueuedAlias { entry, native, revision, focus: window.focused(cx), draft: root.app_editor.drop_draft_id(), query: root.query.read(cx).value().to_string() });
                        root.deliver_queued_drop_alias(window, cx);
                    }
                    if !errors.is_empty() { root.launcher_search_error = Some(errors.join("\n")); }
                }
                cx.notify();
            }); });
            if updated.is_err() { let _ = root.update(cx, |root, cx| {
                if root.external_drop.finish(serial) { if root.restart_status.as_deref() == Some(WAIT) { root.restart_status = None; } root.folder_updated(false, cx); cx.notify(); }
            }); }
            drop(permit);
        }).detach();
        cx.notify();
    }

    pub(super) fn defer_queued_drop_alias(&self, window: &mut Window, cx: &mut Context<Self>) {
        if self.external_drop.alias.is_none() { return; }
        let root = cx.entity().downgrade();
        window.defer(cx, move |window, app| { let _ = root.update(app, |root, cx| root.deliver_queued_drop_alias(window, cx)); });
    }

    pub(super) fn deliver_queued_drop_alias(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(queued) = self.external_drop.alias.as_ref() else { return; };
        if !can_deliver(self.external_drop.pointer_down, native_pointer_pressed()) { return; }
        if !window.is_window_active() || current_foreground_identity() != Some(queued.native.identity) { return; }
        let current = self.drop_owner_current(queued.native, queued.revision, queued.focus.as_ref(), window, cx)
            && self.app_editor.drop_draft_id() == queued.draft
            && self.query.read(cx).value().as_str() == queued.query;
        if !current { self.external_drop.alias = None; return; }
        let queued = self.external_drop.alias.take().unwrap();
        self.show_dropped_folder_alias(queued.entry, queued.native, queued.revision, window, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::{State, delivery_current, process_paths, empty_focus_owner_current, same_query_projection, can_deliver};
    use std::sync::atomic::Ordering;
    #[test]
    fn ordered_mixed_drop_keeps_each_directory_and_only_last_file_draft() {
        let temp = tempfile::tempdir().unwrap();
        let a = temp.path().join("first"); let b = temp.path().join("second");
        std::fs::create_dir(&a).unwrap(); std::fs::create_dir(&b).unwrap();
        let exe = temp.path().join("app.exe"); let link = temp.path().join("shortcut.lnk");
        std::fs::write(&exe, b"owned exe").unwrap(); std::fs::write(&link, b"owned link").unwrap();
        let store = temp.path().join("folders.json");
        let result = process_paths(vec![a.clone(), exe, b.clone(), link.clone(), temp.path().join("missing")], store.to_str().unwrap(), || true);
        assert!(result.committed); assert_eq!(result.last_folder.unwrap().path, b.to_str().unwrap());
        assert_eq!(result.last_file.as_deref(), link.to_str()); assert_eq!(result.errors.len(), 1);
        let saved = multi_launcher::plugins::folders::load_folders(store.to_str().unwrap()).unwrap();
        let paths = saved.iter().map(|entry| entry.path.as_str()).collect::<Vec<_>>();
        assert!(paths.iter().position(|path| *path == a.to_str().unwrap()).unwrap() < paths.iter().position(|path| *path == b.to_str().unwrap()).unwrap());
        assert!(!temp.path().join("actions.json").exists()); assert_eq!(std::fs::read(link).unwrap(), b"owned link");
    }
    #[test]
    fn revocation_after_first_commit_preserves_prior_folder_only() {
        let temp = tempfile::tempdir().unwrap(); let first = temp.path().join("first"); let second = temp.path().join("second");
        std::fs::create_dir(&first).unwrap(); std::fs::create_dir(&second).unwrap();
        let store = temp.path().join("folders.json");
        let result = process_paths(vec![first.clone(), second.clone()], store.to_str().unwrap(), || !store.exists());
        assert!(result.committed); assert_eq!(result.last_folder.unwrap().path, first.to_str().unwrap());
        let entries = multi_launcher::plugins::folders::load_folders(store.to_str().unwrap()).unwrap();
        assert!(entries.iter().any(|entry| entry.path == first.to_str().unwrap()));
        assert!(!entries.iter().any(|entry| entry.path == second.to_str().unwrap()));
    }
    #[test]
    fn delivery_rejects_revoked_owner_or_replaced_draft() {
        assert!(delivery_current(true, true, true));
        assert!(!delivery_current(false, true, true));
        assert!(!delivery_current(true, false, true));
        assert!(!delivery_current(true, true, false));
        assert!(empty_focus_owner_current::<u8>(None, None, false));
        assert!(!empty_focus_owner_current(None, Some(&1u8), false));
        assert!(!empty_focus_owner_current::<u8>(None, None, true));
        assert!(same_query_projection("search", "search"));
        assert!(!same_query_projection("search", "other"));
        assert!(can_deliver(false, false)); assert!(!can_deliver(true, false)); assert!(!can_deliver(false, true));
    }
    #[test]
    fn drop_reservation_rejects_repeated_and_stale_settlement() {
        let mut state = State::default(); let (first, lease) = state.begin().unwrap();
        assert!(state.begin().is_none()); state.revoke(); assert!(!lease.load(Ordering::Acquire));
        assert!(!state.finish(first + 1)); assert!(state.finish(first));
        let (next, _) = state.begin().unwrap(); assert!(!state.finish(first)); assert!(state.finish(next));
    }
}
