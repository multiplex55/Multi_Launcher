//! Checked actions for the retained Folder Size report.
use super::*;
use multi_launcher::folder_size::FolderSizeEntryKind;

#[derive(Clone)]
pub(super) struct Menu {
    pub(super) serial: u64,
    pub(super) receipt: report::Receipt,
    pub(super) index: usize,
}

#[derive(Clone)]
pub(super) enum Target {
    Root { mount: utility_mount::MountReceipt, root: ExplicitDirectoryRoot },
    Row { receipt: report::Receipt, index: usize, kind: FolderSizeEntryKind },
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Action { Copy, RevealToolbar, RevealRow }

fn same_toolbar_target(issued: &Target, current: Option<&Target>) -> bool {
    match (issued, current) {
        (Target::Root { mount, root }, Some(Target::Root { mount: live, root: current })) =>
            mount.is_current() && mount.matches(live) && root == current,
        (Target::Row { receipt, index, kind }, Some(Target::Row { receipt: live, index: current, kind: current_kind })) =>
            receipt.mount.is_current() && receipt.mount.matches(&live.mount) && receipt.key == live.key
                && receipt.directory == live.directory && receipt.filter == live.filter
                && receipt.root == live.root && index == current && kind == current_kind,
        _ => false,
    }
}

pub(super) fn toolbar(view: &FolderSizeView, cx: &mut Context<FolderSizeView>) -> AnyElement {
    let target = view.toolbar_target();
    let copy = target.clone(); let reveal = target.clone();
    let copy_focus = view.copy_focus.clone(); let reveal_focus = view.reveal_focus.clone();
    div().flex().flex_wrap().gap_2()
        .child(Button::new("folder-size-copy-path").label("Copy path").tab_stop(true).track_focus(&view.copy_focus)
            .disabled(target.is_none() || view.effect_pending || view.picker || view.rehosting)
            .on_click(cx.listener(move |view, _, window, cx| {
                if let Some(target) = copy.clone() { view.start_report_action(target, Action::Copy, true, copy_focus.clone(), window, cx); }
            })))
        .child(Button::new("folder-size-reveal").label("Reveal in Explorer").tab_stop(true).track_focus(&view.reveal_focus)
            .disabled(target.as_ref().is_none_or(|target| matches!(target, Target::Row { kind: FolderSizeEntryKind::Omitted(_), .. }))
                || view.effect_pending || view.picker || view.rehosting)
            .on_click(cx.listener(move |view, _, window, cx| {
                if let Some(target) = reveal.clone() { view.start_report_action(target, Action::RevealToolbar, true, reveal_focus.clone(), window, cx); }
            }))).into_any_element()
}

impl FolderSizeView {
    pub(super) fn dismiss_report_menu(&mut self) {
        self.report_ui.menu = None;
        self.report_ui.menu_serial = self.report_ui.menu_serial.wrapping_add(1);
        self.menu_focus_guard = None;
    }
    pub(super) fn toolbar_target(&self) -> Option<Target> {
        let root = self.root_choice.clone()?;
        if let Some((generation, index)) = self.report_ui.report.selected() {
            let receipt = report::Receipt::capture(self)?;
            if receipt.key.generation() != generation { return None; }
            if !self.report_ui.report.visible_rows().is_some_and(|rows| rows.contains(&index)) {
                return (self.report_ui.report.directory() == Some((generation, index)))
                    .then_some(Target::Root { mount: self.mount.clone(), root });
            }
            let kind = self.report_ui.report.projection()?.row(index)?.kind();
            return Some(Target::Row { receipt, index, kind });
        }
        Some(Target::Root { mount: self.mount.clone(), root })
    }
    pub(super) fn open_row_menu(&mut self, receipt: report::Receipt, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if !receipt.row_current(self, index, window, cx) || self.picker { return; }
        if self.report_ui.report.projection().and_then(|p| p.row(index)).is_none() { return; }
        self.report_ui.report.select(receipt.key.generation(), index);
        self.report_ui.menu_serial = self.report_ui.menu_serial.wrapping_add(1);
        let serial = self.report_ui.menu_serial;
        let prior_focus = window.focused(cx);
        self.report_ui.menu = Some(Menu { serial, receipt, index });
        self.menu_focus_guard = Some(cx.on_focus_out(&self.menu_focus, window, move |view, _, _, cx| {
            if view.report_ui.menu.as_ref().is_some_and(|menu| menu.serial == serial) {
                view.dismiss_report_menu(); cx.notify();
            }
        }));
        let owner = cx.entity().downgrade();
        window.defer(cx, move |window, app| { let _ = owner.update(app, |view, cx| {
            if view.report_ui.menu.as_ref().is_some_and(|menu| menu.serial == serial) {
                if window.focused(cx) == prior_focus && view.gesture_current(window, cx) { view.menu_focus.focus(window, cx); }
                else { view.dismiss_report_menu(); cx.notify(); }
            }
        }); });
        cx.notify();
    }
    pub(super) fn choose_menu_action(&mut self, serial: u64, action: Action, window: &mut Window, cx: &mut Context<Self>) {
        let Some(menu) = self.report_ui.menu.clone().filter(|menu| menu.serial == serial && serial == self.report_ui.menu_serial) else { return; };
        if !self.menu_focus.contains_focused(window, cx)
            || !menu.receipt.row_current(self, menu.index, window, cx)
            || self.report_ui.report.selected() != Some((menu.receipt.key.generation(), menu.index)) {
            self.dismiss_report_menu(); cx.notify(); return;
        }
        let Some(kind) = self.report_ui.report.projection().and_then(|p| p.row(menu.index)).map(|r| r.kind()) else { return; };
        self.dismiss_report_menu();
        let focus = self.action_anchor.clone();
        self.start_report_action(Target::Row { receipt: menu.receipt, index: menu.index, kind }, action, false, focus, window, cx);
    }
    pub(super) fn start_report_action(&mut self, target: Target, action: Action, toolbar: bool, focus: FocusHandle,
        window: &mut Window, cx: &mut Context<Self>) {
        if self.picker || !self.gesture_current(window, cx) { return; }
        self.sync_report();
        if toolbar && !same_toolbar_target(&target, self.toolbar_target().as_ref()) { return; }
        let Some(native) = self.native else { return; };
        let (path, directory, publication) = match &target {
            Target::Root { mount, root } => {
                if !mount.is_current() || !mount.matches(&self.mount) || self.root_choice.as_ref() != Some(root) { return; }
                (root.as_path().to_path_buf(), true, None)
            }
            Target::Row { receipt, index, kind } => {
                if !receipt.row_current(self, *index, window, cx)
                    || self.report_ui.report.projection().and_then(|p| p.row(*index)).is_none_or(|row| row.kind() != *kind)
                    || (action != Action::Copy && matches!(kind, FolderSizeEntryKind::Omitted(_))) { return; }
                let Some(path) = self.report_ui.report.path_for_current_row(&self.session.view(), self.report_root.as_ref(),
                    self.root_choice.as_ref(), receipt.key, *index) else { return; };
                (path, *kind == FolderSizeEntryKind::Directory, Some((self.session.publication_guard(), receipt.key)))
            }
        };
        if path.to_string_lossy().contains('\0') { self.message = Some("Cannot use a path containing a NUL character.".into()); cx.notify(); return; }
        let Some(root) = self.root.upgrade() else { return; };
        if !root.read(cx).external_owner_is_current(window.focused(cx).as_ref(), window, cx) { return; }
        focus.focus(window, cx);
        if !focus.is_focused(window) { return; }
        let Some(permit) = self.admission.admit() else { return; };
        let lease = Arc::new(AtomicBool::new(true));
        let clipboard = action == Action::Copy;
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
        let blur_lease = lease.clone();
        self.effect_focus_guard = Some(cx.on_blur(&focus, window, move |_, _, _| blur_lease.store(false, Ordering::Release)));
        let active_lease = lease.clone();
        self.effect_activation_guard = Some(cx.observe_window_activation(window, move |_, window, _| {
            if !window.is_window_active() { active_lease.store(false, Ordering::Release); }
        }));
        self.effect_pending = true;
        self.effect_lease = Some(lease.clone());
        self.message = None;
        let mount = self.mount.clone(); let worker_mount = mount.clone();
        let worker_lease = lease.clone(); let admission = self.admission.clone();
        let visibility = self.visibility.clone(); let revision = visibility.request_revision.load(Ordering::Acquire);
        let popup = self.popout; let host = self.host; let worker_path = path.clone();
        let task = cx.background_executor().spawn(async move {
            let current = || worker_lease.load(Ordering::Acquire) && worker_mount.is_current() && !admission.is_fenced()
                && current_foreground_identity() == Some(native.identity)
                && json_utility::native_host_current(&visibility, revision, Some(native), popup)
                && publication.as_ref().is_none_or(|(guard, key)| guard.is_current(*key));
            if clipboard {
                return super::super::file_search_preview::copy_if_current(&multi_launcher::clipboard_modify::ArboardClipboardBackend,
                    &worker_path.display().to_string(), &current);
            }
            #[cfg(windows)]
            {
                use multi_launcher::file_search::actions::{checked_explorer_target, reveal_checked_item_if_current, CheckedSelectionKind};
                let result = if directory && action == Action::RevealToolbar {
                    (|| -> anyhow::Result<()> {
                        let target = checked_explorer_target(&worker_path, true)?;
                        multi_launcher::actions::exec::launch_association_if_current(target.as_os_str(), || {
                            checked_explorer_target(&worker_path, true).is_ok() && current()
                        })
                    })()
                } else {
                    reveal_checked_item_if_current(&worker_path, if directory { CheckedSelectionKind::Directory } else { CheckedSelectionKind::File }, current)
                };
                result.map_err(|error| format!("Could not open in Explorer: {error:#}"))
            }
            #[cfg(not(windows))]
            { let _ = (directory, action, current); Err("Explorer requires Windows.".into()) }
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
            let _ = weak.update(cx, |view, cx| {
                if !view.effect_lease.as_ref().is_some_and(|active| Arc::ptr_eq(active, &lease)) { return; }
                view.effect_pending = false; view.effect_lease = None;
                view.effect_focus_guard = None; view.effect_activation_guard = None;
                if mount.is_current() && mount.matches(&view.mount) && view.host.window_id() == host.window_id()
                    && lease.load(Ordering::Acquire) && view.open {
                    view.message = Some(match result { Ok(()) if clipboard => "Copied path.".into(), Ok(()) => "Opened in Explorer.".into(), Err(error) => error });
                }
                cx.notify();
            });
        }).detach();
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{same_toolbar_target, Target};
    use multi_launcher::batch_rename::ExplicitDirectoryRoot;
    use multi_launcher::clipboard_modify::clipboard::{ClipboardBackend, ClipboardError};
    use std::sync::{atomic::{AtomicBool, Ordering}, Mutex};

    #[derive(Default)]
    struct FakeClipboard { writes: Mutex<Vec<String>>, revoke_before_write: AtomicBool }
    impl ClipboardBackend for FakeClipboard {
        fn read_text(&self) -> Result<String, ClipboardError> { Ok(String::new()) }
        fn write_text(&self, text: &str) -> Result<(), ClipboardError> {
            self.writes.lock().unwrap().push(text.to_owned()); Ok(())
        }
        fn write_text_if(&self, text: &str, current: &dyn Fn() -> bool) -> Result<bool, ClipboardError> {
            if self.revoke_before_write.load(Ordering::Acquire) || !current() { return Ok(false); }
            self.write_text(text)?; Ok(true)
        }
        fn write_text_acquired_if(&self, text: &str, current: &dyn Fn() -> bool) -> Result<bool, ClipboardError> {
            self.write_text_if(text, current)
        }
    }
    #[test]
    fn toolbar_root_receipt_rejects_changed_root_and_retired_mount() {
        let owner = super::super::super::utility_mount::MountOwner::default();
        let mount = owner.attach();
        let first = ExplicitDirectoryRoot::new(std::env::temp_dir()).unwrap();
        let second = ExplicitDirectoryRoot::new(std::env::temp_dir().join("different")).unwrap();
        let issued = Target::Root { mount: mount.clone(), root: first.clone() };
        assert!(same_toolbar_target(&issued, Some(&Target::Root { mount: mount.clone(), root: first })));
        assert!(!same_toolbar_target(&issued, Some(&Target::Root { mount: mount.clone(), root: second })));
        owner.retire();
        assert!(!same_toolbar_target(&issued, Some(&issued)));
    }
    #[test]
    fn copied_path_respects_late_revocation_and_unicode() {
        let fake = FakeClipboard::default();
        let payload = "C:\\owned\\café.txt";
        assert!(super::super::super::file_search_preview::copy_if_current(&fake, payload, &|| true).is_ok());
        assert_eq!(fake.writes.lock().unwrap().as_slice(), &[payload.to_owned()]);
        fake.revoke_before_write.store(true, Ordering::Release);
        assert!(super::super::super::file_search_preview::copy_if_current(&fake, payload, &|| true).is_err());
        assert_eq!(fake.writes.lock().unwrap().len(), 1);
    }
}
