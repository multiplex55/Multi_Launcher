//! Checked File Lock snapshot and executable actions.
use super::*;
use multi_launcher::file_lock::{FileLockInspection, FileLockProcess};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action { Copy, Reveal }

fn selected_process<'a>(inspection: &'a FileLockInspection, key: ProcessKey, generation: u64) -> Option<&'a FileLockProcess> {
    if key.generation != generation { return None; }
    inspection.processes.iter().find(|process| process.process_id == key.pid && process.creation_filetime == key.created)
}
fn inspection_receipt_current(issued: &Arc<FileLockInspection>, current: Option<&Arc<FileLockInspection>>,
    generation_current: bool, selected_path: Option<&PathBuf>) -> bool {
    generation_current && selected_path == Some(&issued.path)
        && current.is_some_and(|current| Arc::ptr_eq(current, issued))
}

pub(super) fn toolbar(view: &FileLockView, inspection: Arc<FileLockInspection>, generation: u64, cx: &mut Context<FileLockView>) -> AnyElement {
    let copy_snapshot = inspection.clone();
    let reveal_snapshot = inspection;
    let selected = view.selected;
    let revealable = selected.and_then(|key| selected_process(&reveal_snapshot, key, generation))
        .is_some_and(|process| process.executable_path.is_some());
    let copy_mount = view.mount.clone(); let reveal_mount = view.mount.clone();
    let copy_focus = view.copy_focus.clone(); let reveal_focus = view.reveal_focus.clone();
    div().flex().flex_wrap().gap_2()
        .child(Button::new("file-lock-copy-snapshot").label("Copy snapshot").tab_stop(true).track_focus(&view.copy_focus)
            .disabled(view.action_pending || view.picker || view.rehosting)
            .on_click(cx.listener(move |view, _, window, cx| view.start_action(Action::Copy, copy_snapshot.clone(), generation, None, &copy_mount, copy_focus.clone(), window, cx))))
        .child(Button::new("file-lock-reveal-executable").label("Reveal executable").tab_stop(true).track_focus(&view.reveal_focus)
            .disabled(!revealable || view.action_pending || view.picker || view.rehosting)
            .on_click(cx.listener(move |view, _, window, cx| view.start_action(Action::Reveal, reveal_snapshot.clone(), generation, selected, &reveal_mount, reveal_focus.clone(), window, cx))))
        .into_any_element()
}

impl FileLockView {
    fn inspection_matches(&self, issued: &Arc<FileLockInspection>, generation: u64) -> bool {
        let view = self.session.view();
        let current = submission_current(self.submitted.as_ref(), self.selected_file.as_ref(), generation)
            && view.generation() == generation && view.inspection_is_current();
        inspection_receipt_current(issued, view.inspection(), current, self.selected_file.as_ref())
    }
    fn start_action(&mut self, action: Action, issued: Arc<FileLockInspection>, generation: u64, key: Option<ProcessKey>,
        mount: &utility_mount::MountReceipt, focus: FocusHandle, window: &mut Window, cx: &mut Context<Self>) {
        if self.picker || !mount.is_current() || !mount.matches(&self.mount) || !self.current(window, cx)
            || !self.inspection_matches(&issued, generation) { return; }
        let process = if action == Action::Reveal {
            let Some(key) = key.filter(|key| self.selected == Some(*key)) else { return; };
            let Some(process) = selected_process(&issued, key, generation).filter(|process| process.executable_path.is_some()) else { return; };
            Some(process.clone())
        } else { None };
        let Some(native) = self.native else { return; };
        let Some(root) = self.root.upgrade() else { return; };
        if !root.read(cx).external_owner_is_current(window.focused(cx).as_ref(), window, cx) { return; }
        focus.focus(window, cx);
        if !focus.is_focused(window) { return; }
        let Some(permit) = self.admission.admit() else { return; };
        let lease = Arc::new(AtomicBool::new(true)); let clipboard = action == Action::Copy;
        let admitted = root.update(cx, |root, cx| {
            if window.has_active_dialog(cx) || !window.is_window_active() || current_foreground_identity() != Some(native.identity)
                || !root.external_owner_is_current(Some(&focus), window, cx) { return false; }
            if clipboard && (root.clipboard_modify_startup.pending
                || root.clipboard_modify_view.as_ref().is_some_and(|view| view.read(cx).blocks_close())) { return false; }
            if !root.external_launch.begin_auxiliary_effect(lease.clone(), clipboard) { return false; }
            if clipboard { if let Some(modify) = &root.clipboard_modify_view {
                modify.update(cx, |view, cx| { view.retire_read_for_effect(); cx.notify(); });
            } }
            true
        });
        if !admitted { self.message = Some("Another clipboard or utility action is still active.".into()); cx.notify(); return; }
        let blur = lease.clone();
        self.action_blur = Some(cx.on_blur(&focus, window, move |_, _, _| blur.store(false, Ordering::Release)));
        let active = lease.clone();
        self.action_activation = Some(cx.observe_window_activation(window, move |_, window, _| {
            if !window.is_window_active() { active.store(false, Ordering::Release); }
        }));
        self.action_pending = true; self.action_lease = Some(lease.clone()); self.message = None;
        let worker_mount = mount.clone(); let receipt = mount.clone(); let host = self.host;
        let worker_lease = lease.clone(); let visibility = self.visibility.clone(); let admission = self.admission.clone();
        let revision = visibility.request_revision.load(Ordering::Acquire); let popup = self.popout;
        let completion_visibility = visibility.clone(); let completion_focus = focus.clone();
        let worker_snapshot = issued.clone();
        let task = cx.background_executor().spawn(async move {
            let current = || worker_lease.load(Ordering::Acquire) && worker_mount.is_current() && !admission.is_fenced()
                && current_foreground_identity() == Some(native.identity)
                && json_utility::native_host_current(&visibility, revision, Some(native), popup);
            match action {
                Action::Copy => super::super::external_launch::copy_if_current(
                    &multi_launcher::clipboard_modify::ArboardClipboardBackend,
                    &multi_launcher::file_lock::copy_snapshot(&worker_snapshot), &current),
                Action::Reveal => {
                    #[cfg(windows)]
                    { (|| -> anyhow::Result<()> {
                        let process = process.ok_or_else(|| anyhow::anyhow!("No process is selected"))?;
                        let path = process.executable_path.as_ref().ok_or_else(|| anyhow::anyhow!("Executable provenance unavailable"))?;
                        let verified = multi_launcher::file_lock::validate_reported_executable(process.process_id, process.creation_filetime, path)
                            .map_err(anyhow::Error::msg)?;
                        let mut liveness = None;
                        let result = multi_launcher::file_search::actions::reveal_checked_file_if_current(verified.path(), || {
                            match verified.ensure_process_is_alive() {
                                Ok(()) => current(),
                                Err(error) => { liveness = Some(error); false }
                            }
                        });
                        if let Some(error) = liveness { Err(anyhow::Error::msg(error)) } else { result }
                    })().map_err(|error| format!("Could not reveal executable: {error:#}")) }
                    #[cfg(not(windows))]
                    { let _ = (process, current); Err("Executable reveal requires Windows.".into()) }
                }
            }
        });
        let weak = cx.entity().downgrade(); let root = self.root.clone();
        cx.spawn(async move |_, cx| {
            let _permit = permit; let result = task.await;
            let _ = root.update(cx, |root, cx| {
                root.external_launch.finish_auxiliary_effect(&lease);
                if clipboard {
                    if root.launcher_search_error.as_deref() == Some(clipboard_modify_view::LAUNCHER_EFFECT_WAIT) { root.launcher_search_error = None; }
                    if let Some(view) = &root.clipboard_modify_view { view.update(cx, |view, cx| view.launcher_effect_settled(cx)); }
                }
                if root.restart_status.as_deref() == Some(WAIT) { root.restart_status = None; }
                cx.notify();
            });
            let host_current = host.update(cx, |_, window, app| {
                if window.window_handle().window_id() != host.window_id() || !window.is_window_active()
                    || window.has_active_dialog(app) || !completion_focus.is_focused(window)
                    || checked_native_window(window).ok().flatten() != Some(native)
                    || current_foreground_identity() != Some(native.identity)
                    || !json_utility::native_host_current(&completion_visibility, revision, Some(native), popup) { return false; }
                root.upgrade().is_some_and(|root| root.read(app).external_owner_is_current(Some(&completion_focus), window, app))
            }).unwrap_or(false);
            let _ = weak.update(cx, |view, cx| {
                if !view.action_lease.as_ref().is_some_and(|current| Arc::ptr_eq(current, &lease)) { return; }
                view.action_pending = false; view.action_lease = None;
                view.action_blur = None; view.action_activation = None;
                if view.message.as_deref() == Some(WAIT) { view.message = None; }
                let current = receipt.is_current() && receipt.matches(&view.mount) && view.host.window_id() == host.window_id()
                    && view.open && view.inspection_matches(&issued, generation)
                    && (action == Action::Copy || view.selected == key);
                if current && ((lease.load(Ordering::Acquire) && host_current) || (action == Action::Reveal && result.is_ok())) {
                    view.message = Some(match result { Ok(()) if clipboard => "Snapshot copied.".into(), Ok(()) => "Executable revealed in Explorer.".into(), Err(error) => error });
                }
                cx.notify();
            });
        }).detach();
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{inspection_receipt_current, selected_process, ProcessKey};
    use multi_launcher::clipboard_modify::clipboard::{ClipboardBackend, ClipboardError};
    use multi_launcher::file_lock::{FileLockInspection, FileLockProcess};
    use std::{path::PathBuf, sync::{Arc, Mutex, atomic::{AtomicBool, AtomicUsize, Ordering}}, time::UNIX_EPOCH};

    struct FakeClipboard { writes: Mutex<Vec<String>>, revoke_during_acquire: AtomicBool,
        lease: Arc<AtomicBool>, acquired_calls: AtomicUsize, ordinary_calls: AtomicUsize }
    impl ClipboardBackend for FakeClipboard {
        fn read_text(&self) -> Result<String, ClipboardError> { Ok(String::new()) }
        fn write_text(&self, text: &str) -> Result<(), ClipboardError> { self.writes.lock().unwrap().push(text.into()); Ok(()) }
        fn write_text_if(&self, text: &str, current: &dyn Fn() -> bool) -> Result<bool, ClipboardError> {
            self.ordinary_calls.fetch_add(1, Ordering::SeqCst);
            if self.revoke_during_acquire.load(Ordering::Acquire) { self.lease.store(false, Ordering::Release); }
            if !current() { return Ok(false); }
            self.write_text(text)?; Ok(true)
        }
        fn write_text_acquired_if(&self, text: &str, current: &dyn Fn() -> bool) -> Result<bool, ClipboardError> {
            self.acquired_calls.fetch_add(1, Ordering::SeqCst);
            if self.revoke_during_acquire.load(Ordering::Acquire) { self.lease.store(false, Ordering::Release); }
            if !current() { return Ok(false); }
            self.write_text(text)?; Ok(true)
        }
    }

    #[test]
    fn stale_snapshot_and_reused_pid_are_not_actionable() {
        let path = PathBuf::from("owned.txt");
        let process = FileLockProcess { process_id: 7, creation_filetime: 20,
            application_name: "app".into(), service_name: String::new(), executable_path: Some(PathBuf::from("app.exe")) };
        let result = Arc::new(FileLockInspection { path: path.clone(), identity: Default::default(),
            inspected_at: UNIX_EPOCH, processes: vec![process] });
        let copy = Arc::new((*result).clone());
        assert!(inspection_receipt_current(&result, Some(&result), true, Some(&path)));
        assert!(!inspection_receipt_current(&result, Some(&copy), true, Some(&path)));
        assert!(!inspection_receipt_current(&result, Some(&result), false, Some(&path)));
        let key = ProcessKey { generation: 4, pid: 7, created: 20 };
        assert!(selected_process(&result, key, 4).is_some());
        assert!(selected_process(&result, ProcessKey { created: 21, ..key }, 4).is_none());
        assert!(selected_process(&result, key, 5).is_none());
    }
    #[test]
    fn snapshot_copy_preserves_unicode_and_rejects_late_revocation() {
        let lease = Arc::new(AtomicBool::new(true));
        let backend = FakeClipboard { writes: Mutex::new(Vec::new()), revoke_during_acquire: AtomicBool::new(false),
            lease: lease.clone(), acquired_calls: AtomicUsize::new(0), ordinary_calls: AtomicUsize::new(0) };
        let payload = "Windows reported users: café";
        assert!(super::super::super::external_launch::copy_if_current(&backend, payload, &|| lease.load(Ordering::Acquire)).is_ok());
        assert_eq!(backend.writes.lock().unwrap().as_slice(), &[payload.to_owned()]);
        backend.revoke_during_acquire.store(true, Ordering::Release);
        assert!(super::super::super::external_launch::copy_if_current(&backend, payload, &|| lease.load(Ordering::Acquire)).is_err());
        assert_eq!(backend.writes.lock().unwrap().len(), 1);
        #[cfg(windows)]
        { assert_eq!(backend.acquired_calls.load(Ordering::SeqCst), 2); assert_eq!(backend.ordinary_calls.load(Ordering::SeqCst), 0); }
    }
}
