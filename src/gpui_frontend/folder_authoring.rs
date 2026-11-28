//! Checked canonical Folder authoring for the launcher.
use super::*;
use multi_launcher::plugins::folders::{self, CheckedFolderMutation, FolderEntry};

pub(super) const WAIT: &str = "Wait for the Folder operation to finish.";

#[derive(Default)]
pub(super) struct State { serial: u64, pending: bool, reviewing: bool, feedback: Option<String>, lease: Option<Arc<AtomicBool>> }
impl State {
    pub(super) fn busy(&self) -> bool { self.pending }
    pub(super) fn revoke(&mut self) { if let Some(lease) = &self.lease { lease.store(false, Ordering::Release); } }
    fn begin(&mut self) -> Option<u64> {
        if self.pending { return None; }
        self.serial = self.serial.wrapping_add(1).max(1); self.pending = true; self.reviewing = false; self.feedback = None; Some(self.serial)
    }
    fn finish(&mut self, serial: u64) -> bool {
        if !self.pending || self.serial != serial { return false; }
        self.revoke(); self.pending = false; self.reviewing = false; self.feedback = None; self.lease = None; true
    }
    fn begin_review(&mut self, serial: u64) -> bool {
        if !self.pending || self.serial != serial || self.reviewing { return false; }
        self.reviewing = true; true
    }
    fn finish_review(&mut self, serial: u64) -> bool {
        if !self.pending || self.serial != serial || !self.reviewing { return false; }
        self.finish(serial)
    }
    fn claim_review(&mut self, serial: u64) -> bool {
        if !self.pending || self.serial != serial || !self.reviewing { return false; }
        self.reviewing = false; self.feedback = None; true
    }
}

fn refresh_folder_query(query: &str, folder_rows: bool, force: bool) -> bool {
    let trimmed = query.trim();
    force || folder_rows || trimmed.eq_ignore_ascii_case("f")
        || trimmed.get(..2).is_some_and(|prefix| prefix.eq_ignore_ascii_case("f "))
}

#[derive(Clone)]
struct Receipt { serial: u64, native: NativeWindow, revision: u64, focus: Option<FocusHandle>, search: Option<SearchReceipt> }
#[derive(Clone)]
struct SearchReceipt { generation: u64, row: usize, query: String, action: Action, source: ActivationSource, settings: external_launch::Settings, secondary: bool, omni: Option<Arc<omni_search::Projection>> }
struct Prepared { entry: Option<FolderEntry>, completion: Option<external_launch::WorkerResult> }

#[derive(Clone)]
struct CompletionData { action: Action, query: String, source: ActivationSource, settings: external_launch::Settings }
impl From<&SearchReceipt> for CompletionData {
    fn from(receipt: &SearchReceipt) -> Self { Self { action: receipt.action.clone(), query: receipt.query.clone(), source: receipt.source, settings: receipt.settings.clone() } }
}
fn completion(receipt: &CompletionData, command: StorageCommand) -> external_launch::WorkerResult {
    let mut action = receipt.action.clone();
    if let StorageCommand::FolderRemove(path) = &command { action.action = format!("folder:remove:{path}"); }
    let outcome = multi_launcher::commands::CommandBus.complete_checked_folder(command, &action, receipt.source,
        receipt.settings.preserve_command, multi_launcher::commands::LauncherRunPreferences {
            clear_query_after_run: receipt.settings.clear_query_after_run,
            hide_after_run: receipt.settings.hide_after_run, refocus: true,
        }, Ok(()));
    external_launch::bookkeep_folder(outcome, action, receipt.query.clone(), receipt.source, receipt.settings.history_limit)
}

#[derive(Clone, Copy)]
enum Review { Alias, Remove }

impl RootWindow {
    fn finish_folder_authoring(&mut self, serial: u64) -> bool {
        if !self.folder_authoring.finish(serial) { return false; }
        if self.restart_status.as_deref() == Some(WAIT) { self.restart_status = None; }
        true
    }
    fn finish_folder_review(&mut self, serial: u64) -> bool {
        if !self.folder_authoring.finish_review(serial) { return false; }
        if self.restart_status.as_deref() == Some(WAIT) { self.restart_status = None; }
        true
    }
    fn folder_receipt_current(&self, receipt: &Receipt, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.folder_presentation_current(receipt, window, cx)
            && receipt.search.as_ref().is_none_or(|search| match &search.omni { Some(projection) => self.search_catalog.providers.omni.child_is_current(&search.query, &search.action, multi_launcher::plugins::omni_search::OmniSource::Folder, projection, self.enabled_plugins.as_ref(), self.enabled_capabilities.as_ref()), None => self.search_catalog.providers.folder_row_is_current(&search.query, &search.action, self.enabled_plugins.as_ref(), self.enabled_capabilities.as_ref()) })
    }
    fn folder_presentation_current(&self, receipt: &Receipt, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.folder_authoring.pending && self.folder_authoring.serial == receipt.serial
            && !self.restart_admission.is_fenced()
            && self.root_window.is_some_and(|host| host.window_id() == window.window_handle().window_id())
            && self.native_window == Some(receipt.native) && current_foreground_identity() == Some(receipt.native.identity)
            && json_utility::native_host_current(&self.root_visibility, receipt.revision, Some(receipt.native), false)
            && receipt.search.as_ref().is_none_or(|search| self.query.read(cx).value().as_str() == search.query
                && self.launcher_results.resolve_result(search.generation, search.row, &search.query)
                    .is_some_and(|result| result.action == search.action && match &search.omni { Some(_) => matches!(result.origin, LauncherResultOrigin::Omni(multi_launcher::plugins::omni_search::OmniSource::Folder)), None => matches!(result.origin, LauncherResultOrigin::Provider(ref name) if name == "folders") }))
    }

    pub(super) fn start_folder_command(&mut self, generation: u64, row: usize, query: &str, action: Action, source: ActivationSource, window: &mut Window, cx: &mut Context<Self>) {
        let kind = match parse_action(&action) {
            Ok(Command::Storage(StorageCommand::FolderAdd(path))) => Some((Some(path), None)),
            Ok(Command::Storage(StorageCommand::FolderRemove(_))) => Some((None, Some(Review::Remove))),
            _ => None,
        };
        if let Some((path, review)) = kind { self.begin_folder_operation(generation, row, query, action, path, review, source, false, None, window, cx); }
    }
    pub(super) fn start_folder_sheet(&mut self, generation: u64, row: usize, query: &str, action: Action, remove: bool, source: ActivationSource, omni: Option<Arc<omni_search::Projection>>, window: &mut Window, cx: &mut Context<Self>) {
        self.begin_folder_operation(generation, row, query, action, None, Some(if remove { Review::Remove } else { Review::Alias }), source, true, omni, window, cx);
    }
    fn begin_folder_operation(&mut self, generation: u64, row: usize, query: &str, action: Action,
        add: Option<String>, review: Option<Review>, source: ActivationSource, secondary: bool, omni: Option<Arc<omni_search::Projection>>, window: &mut Window, cx: &mut Context<Self>) {
        if self.folder_authoring.busy() || self.bookmark_authoring.busy() || self.external_drop.busy() || self.restart_admission.is_fenced() || window.has_active_dialog(cx)
            || !window.is_window_active() || !self.external_owner_is_current(window.focused(cx).as_ref(), window, cx) { return; }
        let Some(native) = self.native_window.filter(|native| current_foreground_identity() == Some(native.identity)) else { return; };
        self.external_drop.revoke();
        let Some(permit) = self.restart_admission.admit() else { return; };
        let Some(serial) = self.folder_authoring.begin() else { return; };
        if omni.is_some() && (!secondary || !matches!(review, Some(Review::Alias))) { self.finish_folder_authoring(serial); return; }
        let receipt = Receipt { serial, native, revision: self.root_visibility.request_revision.load(Ordering::Acquire), focus: window.focused(cx),
            search: Some(SearchReceipt { generation, row, query: query.to_owned(), action, source, settings: self.external_launch_preferences.clone(), secondary, omni }) };
        if !self.folder_receipt_current(&receipt, window, cx) { self.finish_folder_authoring(serial); return; }
        let lease = Arc::new(AtomicBool::new(true)); self.folder_authoring.lease = Some(lease.clone());
        let Some(search) = receipt.search.as_ref() else { self.finish_folder_authoring(serial); return; };
        let worker_action = search.action.clone(); let worker_completion = CompletionData::from(search); let worker_revision = receipt.revision;
        let visibility = self.root_visibility.clone(); let admission = self.restart_admission.clone();
        let task = cx.background_executor().spawn(async move {
            let current = || lease.load(Ordering::Acquire) && !admission.is_fenced()
                && current_foreground_identity() == Some(native.identity)
                && json_utility::native_host_current(&visibility, worker_revision, Some(native), false);
            if !current() { anyhow::bail!("Folder operation was superseded."); }
            if let Some(path) = add {
                folders::checked_add_folder(folders::FOLDERS_FILE, &path, current)?;
                Ok(Prepared { entry: None, completion: Some(completion(&worker_completion, StorageCommand::FolderAdd(path))) })
            } else { folders::checked_folder_search_entry(folders::FOLDERS_FILE, &worker_action,
                matches!(parse_action(&worker_action), Ok(Command::Storage(StorageCommand::FolderRemove(_)))))
                .map(|entry| Prepared { entry: Some(entry), completion: None }) }
        });
        let root = cx.entity().downgrade(); let host = window.window_handle();
        cx.spawn(async move |_, cx| {
            let result = task.await;
            let updated = host.update(cx, |_, window, app| { let _ = root.update(app, |root, cx| {
                if root.folder_authoring.serial != serial || !root.folder_authoring.pending { return; }
                let lease_current = root.folder_authoring.lease.as_ref().is_some_and(|lease| lease.load(Ordering::Acquire));
                root.folder_authoring.lease = None;
                let current = lease_current && root.folder_receipt_current(&receipt, window, cx)
                    && root.external_owner_is_current(receipt.focus.as_ref(), window, cx) && !window.has_active_dialog(cx);
                match (result, review) {
                    (Ok(Prepared { entry: Some(entry), .. }), Some(review)) if current => root.show_folder_review(receipt, entry, review, window, cx),
                    (Ok(Prepared { completion: Some(completion), .. }), _) => {
                        root.finish_folder_authoring(serial); root.finish_folder_completion(&receipt, completion, current, window, cx);
                    },
                    (Ok(_), _) if !current => { root.finish_folder_authoring(serial); },
                    (Err(error), _) => { root.finish_folder_authoring(serial); if current { root.launcher_search_error = Some(format!("Folder operation failed: {error:#}")); } },
                    _ => { root.finish_folder_authoring(serial); }
                }
                cx.notify();
            }); });
            if updated.is_err() { let _ = root.update(cx, |root, cx| {
                if root.finish_folder_authoring(serial) { root.folder_updated(false, cx); cx.notify(); }
            }); }
            drop(permit);
        }).detach();
        cx.notify();
    }

    fn finish_folder_completion(&mut self, receipt: &Receipt, completion: external_launch::WorkerResult,
        current: bool, window: &mut Window, cx: &mut Context<Self>) {
        if let (Some(search), Some(Ok(usage))) = (&receipt.search, completion.usage_result) {
            let key = match parse_action(&search.action) {
                Ok(Command::Storage(StorageCommand::FolderAdd(_) | StorageCommand::FolderRemove(_))) => search.action.action.clone(),
                _ => format!("folder:remove:{}", search.action.action),
            };
            self.search_catalog.usage.insert(key, usage);
        }
        if current && let Ok(outcome) = completion.outcome {
            if let QueryPolicy::Set(query) = outcome.query {
                self.query_history.reset(); self.search_completion.clear_for_query_replacement();
                self.query.update(cx, |input, cx| input.replace_all(query, window, cx));
            }
        }
        self.folder_updated(current, cx);
    }

    pub(super) fn folder_updated(&mut self, force: bool, cx: &mut Context<Self>) {
        if let Some(aliases) = self.search_catalog.providers.folder_aliases() { self.search_catalog.folder_aliases_lc = aliases; }
        let query = self.query.read(cx).value().to_string();
        let folder_rows = self.launcher_results.query == query && self.launcher_results.actions.iter().any(|row| matches!(&row.origin, LauncherResultOrigin::Provider(name) if name == "folders"));
        if refresh_folder_query(&query, folder_rows, force) || super::omni_search::is_omni_query(&query) { self.refresh_launcher_results_with_viewport(query, false, cx); }
        if self.restart_status.as_deref() == Some(WAIT) { self.restart_status = None; }
    }

    pub(super) fn show_dropped_folder_alias(&mut self, entry: FolderEntry, native: NativeWindow, revision: u64, window: &mut Window, cx: &mut Context<Self>) {
        if self.folder_authoring.busy() || self.bookmark_authoring.busy() || self.restart_admission.is_fenced()
            || window.has_active_dialog(cx) || !window.is_window_active()
            || current_foreground_identity() != Some(native.identity) { return; }
        let Some(serial) = self.folder_authoring.begin() else { return; };
        let receipt = Receipt { serial, native, revision, focus: window.focused(cx), search: None };
        if !self.folder_receipt_current(&receipt, window, cx) { self.finish_folder_authoring(serial); return; }
        self.show_folder_review(receipt, entry, Review::Alias, window, cx);
    }

    fn show_folder_review(&mut self, receipt: Receipt, entry: FolderEntry, review: Review, window: &mut Window, cx: &mut Context<Self>) {
        if !self.folder_authoring.begin_review(receipt.serial) { return; }
        let input = cx.new(|cx| InputState::new(window, cx).default_value(entry.alias.clone().unwrap_or_default()));
        let initial_alias = input.read(cx).value().to_string();
        let focus = cx.focus_handle(); let cancel_focus = cx.focus_handle(); let root = cx.entity().downgrade();
        let first = Cell::new(true); let cancel_root = root.clone(); let save_root = root.clone();
        let current_input = input.clone(); let focus_for_dialog = cancel_focus.clone();
        window.open_dialog(cx, move |dialog, window, cx| {
            if first.replace(false) { focus_for_dialog.focus(window, cx); }
            let feedback = root.upgrade().and_then(|root| root.read(cx).folder_authoring.feedback.clone());
            let cancel = cancel_root.clone(); let save = save_root.clone(); let receipt = receipt.clone(); let entry = entry.clone();
            let input = current_input.clone(); let focus = focus.clone(); let cancel_focus = cancel_focus.clone(); let initial_alias = initial_alias.clone();
            let cancel_serial = receipt.serial; let escape_serial = receipt.serial; let escape_root = root.clone();
            let key_root = root.clone(); let key_focus = focus.clone(); let key_serial = receipt.serial;
            let save_receipt = receipt.clone();
            dialog.title(match review { Review::Alias => "Folder Alias", Review::Remove => "Remove Folder" })
                .keyboard(false).close_button(false).overlay_closable(false)
                .child(div().track_focus(&focus).capture_key_down(move |event: &KeyDownEvent, window, cx| {
                    if event.keystroke.modifiers != Modifiers::default() || !key_focus.contains_focused(window, cx) { return; }
                    let escape = event.keystroke.key == "escape";
                    let held_accept = event.is_held && matches!(event.keystroke.key.as_str(), "enter" | "space");
                    if !escape && !held_accept { return; }
                    let _ = key_root.update(cx, |root, cx| {
                        if !root.folder_authoring.reviewing || root.folder_authoring.serial != key_serial { return; }
                        window.prevent_default(); cx.stop_propagation();
                        if escape && !event.is_held && root.finish_folder_review(key_serial) {
                            root.pane_escape_latched = true; window.close_dialog(cx); cx.notify();
                        }
                    });
                }).capture_action(move |_: &gpui_kit::base::input::Escape, window, cx| {
                    let _ = escape_root.update(cx, |root, cx| { if root.finish_folder_review(escape_serial) { root.pane_escape_latched = true; window.close_dialog(cx); cx.stop_propagation(); cx.notify(); } });
                }).flex().flex_col().gap_2()
                    .child(Label::new(entry.path.clone()).whitespace_normal())
                    .children(feedback.map(Label::new))
                    .when(matches!(review, Review::Alias), |panel| panel.child(Input::new(&input).aria_label("Folder alias")))
                    .child(div().flex().gap_2()
                        .child(Button::new("folder-review-cancel").label("Cancel").track_focus(&cancel_focus).on_click(move |_, window, cx| {
                            let _ = cancel.update(cx, |root, cx| { if root.finish_folder_review(cancel_serial) { window.close_dialog(cx); cx.notify(); } });
                        }))
                        .child(Button::new("folder-review-save").label(match review { Review::Alias => "Save", Review::Remove => "Remove" })
                            .on_click(move |_, window, cx| {
                                let _ = save.update(cx, |root, cx| {
                                    if !root.folder_authoring.reviewing || root.folder_authoring.serial != save_receipt.serial { return; }
                                    if root.folder_receipt_current(&save_receipt, window, cx) && focus.contains_focused(window, cx)
                                        && !context_stack_has_primary(&window.context_stack(), "PopupMenu")
                                        && !context_stack_has_primary(&window.context_stack(), "AlertDialog") {
                                        let displayed = input.read(cx).value().to_string();
                                        let alias = if displayed == initial_alias { entry.alias.clone().unwrap_or_default() } else { displayed };
                                        root.commit_folder_review(save_receipt.clone(), entry.clone(), review, alias, window, cx);
                                    } else {
                                        root.folder_authoring.feedback = Some("Folder selection changed. Cancel and reopen it from current Search results.".into()); cx.notify();
                                    }
                                });
                            }))))
        });
    }
    fn commit_folder_review(
        &mut self,
        receipt: Receipt,
        entry: FolderEntry,
        review: Review,
        alias: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(permit) = self.restart_admission.admit() else {
            return;
        };
        if !self.folder_authoring.claim_review(receipt.serial) {
            return;
        }
        window.close_dialog(cx);
        let lease = Arc::new(AtomicBool::new(true));
        self.folder_authoring.lease = Some(lease.clone());
        if receipt
            .search
            .as_ref()
            .is_some_and(|search| search.omni.is_some())
        {
            let Some(focus) = receipt.focus.clone() else {
                self.finish_folder_authoring(receipt.serial);
                return;
            };
            let revoked = lease.clone();
            let guard = cx.on_blur(&focus, window, move |_, _, _| {
                revoked.store(false, Ordering::Release)
            });
            let root = cx.entity().downgrade();
            window.defer(cx, move |window, cx| {
                let _ = root.update(cx, |root, cx| {
                    if !lease.load(Ordering::Acquire)
                        || !focus.is_focused(window)
                        || window.has_active_dialog(cx)
                        || !root.folder_receipt_current(&receipt, window, cx)
                        || !root.external_owner_is_current(Some(&focus), window, cx)
                    {
                        root.finish_folder_authoring(receipt.serial);
                        cx.notify();
                        return;
                    }
                    root.launch_folder_review(
                        receipt,
                        entry,
                        review,
                        alias,
                        permit,
                        lease,
                        Some(guard),
                        window,
                        cx,
                    );
                });
            });
        } else {
            self.launch_folder_review(
                receipt, entry, review, alias, permit, lease, None, window, cx,
            );
        }
    }
    fn launch_folder_review(
        &mut self,
        receipt: Receipt,
        entry: FolderEntry,
        review: Review,
        alias: String,
        permit: EffectPermit,
        lease: Arc<AtomicBool>,
        guard: Option<Subscription>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let visibility = self.root_visibility.clone();
        let admission = self.restart_admission.clone();
        let native = receipt.native;
        let revision = receipt.revision;
        let worker_completion = receipt.search.as_ref().map(CompletionData::from);
        let secondary = receipt
            .search
            .as_ref()
            .is_none_or(|search| search.secondary);
        let omni = receipt
            .search
            .as_ref()
            .and_then(|search| search.omni.clone());
        let feedback_lease = lease.clone();
        let task = cx.background_executor().spawn(async move {
            let mutation = match review {
                Review::Alias => CheckedFolderMutation::SetAlias(alias),
                Review::Remove => CheckedFolderMutation::Remove,
            };
            folders::checked_folder_update(folders::FOLDERS_FILE, &entry, mutation, || {
                lease.load(Ordering::Acquire)
                    && omni
                        .as_ref()
                        .is_none_or(|projection| projection.settings_current())
                    && !admission.is_fenced()
                    && current_foreground_identity() == Some(native.identity)
                    && json_utility::native_host_current(&visibility, revision, Some(native), false)
            })?;
            Ok::<_, anyhow::Error>(if matches!(review, Review::Remove) && !secondary {
                worker_completion.map(|completion_data| {
                    completion(&completion_data, StorageCommand::FolderRemove(entry.path))
                })
            } else {
                None
            })
        });
        let root = cx.entity().downgrade();
        let host = window.window_handle();
        let serial = receipt.serial;
        cx.spawn(async move |_, cx| {
            let _guard = guard;
            let result = task.await;
            let updated = host.update(cx, |_, window, app| {
                let _ = root.update(app, |root, cx| {
                    let current = root.folder_presentation_current(&receipt, window, cx)
                        && receipt
                            .search
                            .as_ref()
                            .and_then(|search| search.omni.as_ref())
                            .is_none_or(|projection| {
                                feedback_lease.load(Ordering::Acquire)
                                    && projection.settings_current()
                            })
                        && (if receipt.search.is_none() {
                            root.drop_focus_current(receipt.focus.as_ref(), window, cx)
                        } else {
                            root.external_owner_is_current(receipt.focus.as_ref(), window, cx)
                        })
                        && !window.has_active_dialog(cx);
                    if root.finish_folder_authoring(serial) {
                        match result {
                            Ok(Some(completion)) => root.finish_folder_completion(
                                &receipt, completion, current, window, cx,
                            ),
                            Ok(None) => root.folder_updated(current, cx),
                            Err(error) if current => {
                                root.launcher_search_error =
                                    Some(format!("Folder operation failed: {error:#}"))
                            }
                            Err(_) => {}
                        }
                        cx.notify();
                    }
                    let _ = window;
                });
            });
            if updated.is_err() {
                let _ = root.update(cx, |root, cx| {
                    if root.finish_folder_authoring(serial) {
                        root.folder_updated(false, cx);
                        cx.notify();
                    }
                });
            }
            drop(permit);
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::{State, refresh_folder_query};
    use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
    #[test]
    fn folder_ack_does_not_reexecute_unrelated_calculator() {
        assert!(!refresh_folder_query("=2+2", false, false));
        assert!(refresh_folder_query("f rm", false, false));
        assert!(refresh_folder_query("", false, true));
    }
    #[test]
    fn folder_attempts_are_exclusive_revocable_and_exactly_retired() {
        let mut state = State::default();
        let first = state.begin().unwrap(); assert!(state.begin().is_none());
        let lease = Arc::new(AtomicBool::new(true)); state.lease = Some(lease.clone());
        state.revoke(); assert!(!lease.load(Ordering::Acquire));
        assert!(!state.finish(first + 1)); assert!(state.finish(first));
        let next = state.begin().unwrap(); assert_ne!(first, next); assert!(!state.finish(first));
        assert!(state.begin_review(next)); assert!(state.claim_review(next));
        assert!(!state.finish_review(next)); assert!(state.busy()); assert!(state.finish(next));
    }
}
