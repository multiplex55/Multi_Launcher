//! Checked canonical Bookmark authoring for the launcher.
use super::*;
use multi_launcher::plugins::bookmarks::{self, CheckedBookmarkMutation, BookmarkEntry};

pub(super) const WAIT: &str = "Wait for the Bookmark operation to finish.";

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

pub(super) fn normalized_add_bypass(action: &Action, current: bool) -> bool {
    current && matches!(parse_action(action), Ok(Command::Storage(StorageCommand::BookmarkAdd(_))))
}

fn retained_alias(displayed: &str, initial_projection: &str, authored: &str) -> String {
    if displayed == initial_projection { authored.to_owned() } else { displayed.to_owned() }
}

fn refresh_bookmark_query(query: &str, bookmark_rows: bool, force: bool) -> bool {
    let trimmed = query.trim();
    force || bookmark_rows || trimmed.eq_ignore_ascii_case("bm")
        || trimmed.get(..3).is_some_and(|prefix| prefix.eq_ignore_ascii_case("bm "))
}

#[derive(Clone)]
struct Receipt { serial: u64, generation: u64, row: usize, query: String, action: Action, native: NativeWindow, revision: u64, source: ActivationSource, focus: Option<FocusHandle>, settings: external_launch::Settings, secondary: bool }
struct Prepared { entry: Option<BookmarkEntry>, completion: Option<external_launch::WorkerResult> }

#[derive(Clone)]
struct CompletionData { action: Action, query: String, source: ActivationSource, settings: external_launch::Settings }
impl From<&Receipt> for CompletionData {
    fn from(receipt: &Receipt) -> Self { Self { action: receipt.action.clone(), query: receipt.query.clone(), source: receipt.source, settings: receipt.settings.clone() } }
}
fn completion(receipt: &CompletionData, command: StorageCommand) -> external_launch::WorkerResult {
    let mut action = receipt.action.clone();
    if let StorageCommand::BookmarkRemove(path) = &command { action.action = format!("bookmark:remove:{path}"); }
    let outcome = multi_launcher::commands::CommandBus.complete_checked_bookmark(command, &action, receipt.source,
        receipt.settings.preserve_command, multi_launcher::commands::LauncherRunPreferences {
            clear_query_after_run: receipt.settings.clear_query_after_run,
            hide_after_run: receipt.settings.hide_after_run, refocus: true,
        }, Ok(()));
    external_launch::bookkeep_bookmark(outcome, action, receipt.query.clone(), receipt.source, receipt.settings.history_limit)
}

#[derive(Clone, Copy)]
enum Review { Add, Alias, Remove }

impl RootWindow {
    fn finish_bookmark_authoring(&mut self, serial: u64) -> bool {
        if !self.bookmark_authoring.finish(serial) { return false; }
        if self.restart_status.as_deref() == Some(WAIT) { self.restart_status = None; }
        true
    }
    fn finish_bookmark_review(&mut self, serial: u64) -> bool {
        if !self.bookmark_authoring.finish_review(serial) { return false; }
        if self.restart_status.as_deref() == Some(WAIT) { self.restart_status = None; }
        true
    }
    fn bookmark_receipt_current(&self, receipt: &Receipt, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.bookmark_presentation_current(receipt, window, cx)
            && self.search_catalog.providers.bookmark_row_is_current(&receipt.query, &receipt.action, self.enabled_plugins.as_ref(), self.enabled_capabilities.as_ref())
    }
    fn bookmark_presentation_current(&self, receipt: &Receipt, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.bookmark_authoring.pending && self.bookmark_authoring.serial == receipt.serial
            && !self.restart_admission.is_fenced()
            && self.root_window.is_some_and(|host| host.window_id() == window.window_handle().window_id())
            && self.native_window == Some(receipt.native) && current_foreground_identity() == Some(receipt.native.identity)
            && json_utility::native_host_current(&self.root_visibility, receipt.revision, Some(receipt.native), false)
            && self.query.read(cx).value().as_str() == receipt.query
            && self.launcher_results.resolve_result(receipt.generation, receipt.row, &receipt.query)
                .is_some_and(|result| result.action == receipt.action && matches!(result.origin, LauncherResultOrigin::Provider(ref name) if name == "bookmarks"))
    }

    pub(super) fn start_bookmark_command(&mut self, generation: u64, row: usize, query: &str, action: Action, source: ActivationSource, window: &mut Window, cx: &mut Context<Self>) {
        let kind = match parse_action(&action) {
            Ok(Command::Storage(StorageCommand::BookmarkDialog)) => Some((None, Some(Review::Add))),
            Ok(Command::Storage(StorageCommand::BookmarkAdd(path))) => Some((Some(path), None)),
            Ok(Command::Storage(StorageCommand::BookmarkRemove(_))) => Some((None, Some(Review::Remove))),
            _ => None,
        };
        if let Some((path, review)) = kind { self.begin_bookmark_operation(generation, row, query, action, path, review, source, false, window, cx); }
    }
    pub(super) fn start_bookmark_sheet(&mut self, generation: u64, row: usize, query: &str, action: Action, remove: bool, source: ActivationSource, window: &mut Window, cx: &mut Context<Self>) {
        self.begin_bookmark_operation(generation, row, query, action, None, Some(if remove { Review::Remove } else { Review::Alias }), source, true, window, cx);
    }
    fn begin_bookmark_operation(&mut self, generation: u64, row: usize, query: &str, action: Action,
        add: Option<String>, review: Option<Review>, source: ActivationSource, secondary: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.bookmark_authoring.busy() || self.folder_authoring.busy() || self.external_drop.busy() || self.restart_admission.is_fenced() || window.has_active_dialog(cx)
            || !window.is_window_active() || !self.external_owner_is_current(window.focused(cx).as_ref(), window, cx) { return; }
        let Some(native) = self.native_window.filter(|native| current_foreground_identity() == Some(native.identity)) else { return; };
        self.external_drop.revoke();
        let Some(permit) = self.restart_admission.admit() else { return; };
        let Some(serial) = self.bookmark_authoring.begin() else { return; };
        let receipt = Receipt { serial, generation, row, query: query.to_owned(), action, native,
            revision: self.root_visibility.request_revision.load(Ordering::Acquire), source, focus: window.focused(cx), settings: self.external_launch_preferences.clone(), secondary };
        if !self.bookmark_receipt_current(&receipt, window, cx) { self.finish_bookmark_authoring(serial); return; }
        if matches!(review, Some(Review::Add)) {
            self.show_bookmark_review(receipt, BookmarkEntry { url: String::new(), alias: None }, None, Review::Add, window, cx);
            drop(permit); return;
        }
        let lease = Arc::new(AtomicBool::new(true)); self.bookmark_authoring.lease = Some(lease.clone());
        let worker_action = receipt.action.clone(); let worker_completion = CompletionData::from(&receipt); let worker_revision = receipt.revision;
        let visibility = self.root_visibility.clone(); let admission = self.restart_admission.clone();
        let task = cx.background_executor().spawn(async move {
            let current = || lease.load(Ordering::Acquire) && !admission.is_fenced()
                && current_foreground_identity() == Some(native.identity)
                && json_utility::native_host_current(&visibility, worker_revision, Some(native), false);
            if !current() { anyhow::bail!("Bookmark operation was superseded."); }
            if let Some(path) = add {
                bookmarks::checked_add_bookmark(bookmarks::BOOKMARKS_FILE, &path, None, current)?;
                Ok(Prepared { entry: None, completion: Some(completion(&worker_completion, StorageCommand::BookmarkAdd(path))) })
            } else { bookmarks::checked_bookmark_search_entry(bookmarks::BOOKMARKS_FILE, &worker_action,
                matches!(parse_action(&worker_action), Ok(Command::Storage(StorageCommand::BookmarkRemove(_)))))
                .map(|entry| Prepared { entry: Some(entry), completion: None }) }
        });
        let root = cx.entity().downgrade(); let host = window.window_handle();
        cx.spawn(async move |_, cx| {
            let result = task.await;
            let updated = host.update(cx, |_, window, app| { let _ = root.update(app, |root, cx| {
                if root.bookmark_authoring.serial != serial || !root.bookmark_authoring.pending { return; }
                let lease_current = root.bookmark_authoring.lease.as_ref().is_some_and(|lease| lease.load(Ordering::Acquire));
                root.bookmark_authoring.lease = None;
                let current = lease_current && root.bookmark_receipt_current(&receipt, window, cx)
                    && root.external_owner_is_current(receipt.focus.as_ref(), window, cx) && !window.has_active_dialog(cx);
                match (result, review) {
                    (Ok(Prepared { entry: Some(entry), .. }), Some(review)) if current => root.show_bookmark_review(receipt, entry, None, review, window, cx),
                    (Ok(Prepared { completion: Some(completion), .. }), _) => {
                        root.finish_bookmark_authoring(serial); root.finish_bookmark_completion(&receipt, completion, current, window, cx);
                    },
                    (Ok(_), _) if !current => { root.finish_bookmark_authoring(serial); },
                    (Err(error), _) => { root.finish_bookmark_authoring(serial); if current { root.launcher_search_error = Some(format!("Bookmark operation failed: {error:#}")); } },
                    _ => { root.finish_bookmark_authoring(serial); }
                }
                cx.notify();
            }); });
            if updated.is_err() { let _ = root.update(cx, |root, cx| {
                if root.finish_bookmark_authoring(serial) { root.bookmark_updated(false, cx); cx.notify(); }
            }); }
            drop(permit);
        }).detach();
        cx.notify();
    }

    fn finish_bookmark_completion(&mut self, receipt: &Receipt, completion: external_launch::WorkerResult,
        current: bool, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(Ok(usage)) = completion.usage_result {
            let key = match parse_action(&receipt.action) {
                Ok(Command::Storage(StorageCommand::BookmarkAdd(_) | StorageCommand::BookmarkRemove(_))) => receipt.action.action.clone(),
                _ => format!("bookmark:remove:{}", receipt.action.action),
            };
            self.search_catalog.usage.insert(key, usage);
        }
        if current && let Ok(outcome) = completion.outcome {
            if let QueryPolicy::Set(query) = outcome.query {
                self.query_history.reset(); self.search_completion.clear_for_query_replacement();
                self.query.update(cx, |input, cx| input.replace_all(query, window, cx));
            }
        }
        self.bookmark_updated(current, cx);
    }

    fn bookmark_updated(&mut self, force: bool, cx: &mut Context<Self>) {
        if let Some(aliases) = self.search_catalog.providers.bookmark_aliases() { self.search_catalog.bookmark_aliases_lc = aliases; }
        let query = self.query.read(cx).value().to_string();
        let bookmark_rows = self.launcher_results.query == query && self.launcher_results.actions.iter().any(|row| matches!(&row.origin, LauncherResultOrigin::Provider(name) if name == "bookmarks"));
        if refresh_bookmark_query(&query, bookmark_rows, force) { self.refresh_launcher_results_with_viewport(query, false, cx); }
        if self.restart_status.as_deref() == Some(WAIT) { self.restart_status = None; }
    }

    fn show_bookmark_review(&mut self, receipt: Receipt, entry: BookmarkEntry, alias_draft: Option<String>, review: Review, window: &mut Window, cx: &mut Context<Self>) {
        if !self.bookmark_authoring.begin_review(receipt.serial) { return; }
        let authored_alias = alias_draft.unwrap_or_else(|| entry.alias.clone().unwrap_or_default());
        let input = cx.new(|cx| InputState::new(window, cx).default_value(authored_alias.clone()));
        let url_input = cx.new(|cx| InputState::new(window, cx).default_value(entry.url.clone()));
        let initial_alias = input.read(cx).value().to_string();
        let focus = cx.focus_handle(); let cancel_focus = cx.focus_handle(); let root = cx.entity().downgrade();
        let first = Cell::new(true); let cancel_root = root.clone(); let save_root = root.clone();
        let current_input = input.clone(); let current_url = url_input.clone();
        let focus_for_dialog = if matches!(review, Review::Add) { url_input.focus_handle(cx) } else { cancel_focus.clone() };
        window.open_dialog(cx, move |dialog, window, cx| {
            if first.replace(false) { focus_for_dialog.focus(window, cx); }
            let feedback = root.upgrade().and_then(|root| root.read(cx).bookmark_authoring.feedback.clone());
            let cancel = cancel_root.clone(); let save = save_root.clone(); let receipt = receipt.clone(); let entry = entry.clone();
            let input = current_input.clone(); let url_input = current_url.clone(); let focus = focus.clone(); let cancel_focus = cancel_focus.clone(); let initial_alias = initial_alias.clone(); let authored_alias = authored_alias.clone();
            let cancel_serial = receipt.serial; let escape_serial = receipt.serial; let escape_root = root.clone();
            let key_root = root.clone(); let key_focus = focus.clone(); let key_serial = receipt.serial;
            let save_receipt = receipt.clone();
            dialog.title(match review { Review::Add => "Add Bookmark", Review::Alias => "Bookmark Alias", Review::Remove => "Remove Bookmark" })
                .keyboard(false).close_button(false).overlay_closable(false)
                .child(div().track_focus(&focus).capture_key_down(move |event: &KeyDownEvent, window, cx| {
                    if event.keystroke.modifiers != Modifiers::default() || !key_focus.contains_focused(window, cx) { return; }
                    let escape = event.keystroke.key == "escape";
                    let held_accept = event.is_held && matches!(event.keystroke.key.as_str(), "enter" | "space");
                    if !escape && !held_accept { return; }
                    let _ = key_root.update(cx, |root, cx| {
                        if !root.bookmark_authoring.reviewing || root.bookmark_authoring.serial != key_serial { return; }
                        window.prevent_default(); cx.stop_propagation();
                        if escape && !event.is_held && root.finish_bookmark_review(key_serial) {
                            root.pane_escape_latched = true; window.close_dialog(cx); cx.notify();
                        }
                    });
                }).capture_action(move |_: &gpui_kit::base::input::Escape, window, cx| {
                    let _ = escape_root.update(cx, |root, cx| { if root.finish_bookmark_review(escape_serial) { root.pane_escape_latched = true; window.close_dialog(cx); cx.stop_propagation(); cx.notify(); } });
                }).flex().flex_col().gap_2()
                    .when(matches!(review, Review::Add), |panel| panel.child(Label::new("URL")).child(Input::new(&url_input).aria_label("Bookmark URL")))
                    .when(!matches!(review, Review::Add), |panel| panel.child(Label::new(entry.url.clone()).whitespace_normal()))
                    .children(feedback.map(Label::new))
                    .when(matches!(review, Review::Add | Review::Alias), |panel| panel.child(Label::new("Alias")).child(Input::new(&input).aria_label("Bookmark alias")))
                    .child(div().flex().gap_2()
                        .child(Button::new("bookmark-review-cancel").label("Cancel").track_focus(&cancel_focus).on_click(move |_, window, cx| {
                            let _ = cancel.update(cx, |root, cx| { if root.finish_bookmark_review(cancel_serial) { window.close_dialog(cx); cx.notify(); } });
                        }))
                        .child(Button::new("bookmark-review-save").label(match review { Review::Add | Review::Alias => "Save", Review::Remove => "Remove" })
                            .on_click(move |_, window, cx| {
                                let _ = save.update(cx, |root, cx| {
                                    if !root.bookmark_authoring.reviewing || root.bookmark_authoring.serial != save_receipt.serial { return; }
                                    if root.bookmark_receipt_current(&save_receipt, window, cx) && focus.contains_focused(window, cx)
                                        && !context_stack_has_primary(&window.context_stack(), "PopupMenu")
                                        && !context_stack_has_primary(&window.context_stack(), "AlertDialog") {
                                        let displayed = input.read(cx).value().to_string();
                                        let alias = retained_alias(&displayed, &initial_alias, &authored_alias);
                                        let mut selected = entry.clone();
                                        if matches!(review, Review::Add) {
                                            selected.url = url_input.read(cx).value().to_string();
                                            if selected.url.trim().is_empty() { root.bookmark_authoring.feedback = Some("URL required.".into()); cx.notify(); return; }
                                        }
                                        root.commit_bookmark_review(save_receipt.clone(), selected, review, alias, window, cx);
                                    } else {
                                        root.bookmark_authoring.feedback = Some("Bookmark selection changed. Cancel and reopen it from current Search results.".into()); cx.notify();
                                    }
                                });
                            }))))
        });
    }
    fn commit_bookmark_review(&mut self, receipt: Receipt, entry: BookmarkEntry, review: Review, alias: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(permit) = self.restart_admission.admit() else { return; };
        if !self.bookmark_authoring.claim_review(receipt.serial) { return; }
        window.close_dialog(cx);
        let lease = Arc::new(AtomicBool::new(true)); self.bookmark_authoring.lease = Some(lease.clone());
        let visibility = self.root_visibility.clone(); let admission = self.restart_admission.clone(); let native = receipt.native; let revision = receipt.revision;
        let worker_completion = CompletionData::from(&receipt); let secondary = receipt.secondary;
        let retry_entry = entry.clone(); let retry_alias = alias.clone();
        let task = cx.background_executor().spawn(async move {
            let current = || lease.load(Ordering::Acquire) && !admission.is_fenced()
                && current_foreground_identity() == Some(native.identity)
                && json_utility::native_host_current(&visibility, revision, Some(native), false);
            match review {
                Review::Add => { bookmarks::checked_add_bookmark(bookmarks::BOOKMARKS_FILE, &entry.url, Some(&alias), current)?; }
                Review::Alias => { bookmarks::checked_bookmark_update(bookmarks::BOOKMARKS_FILE, &entry, CheckedBookmarkMutation::SetAlias(alias), current)?; }
                Review::Remove => { bookmarks::checked_bookmark_update(bookmarks::BOOKMARKS_FILE, &entry, CheckedBookmarkMutation::Remove, current)?; }
            }
            Ok::<_, anyhow::Error>(if matches!(review, Review::Remove) && !secondary {
                Some(completion(&worker_completion, StorageCommand::BookmarkRemove(entry.url)))
            } else { None })
        });
        let root = cx.entity().downgrade(); let host = window.window_handle(); let serial = receipt.serial;
        cx.spawn(async move |_, cx| {
            let result = task.await;
            let updated = host.update(cx, |_, window, app| { let _ = root.update(app, |root, cx| {
                let current = root.bookmark_presentation_current(&receipt, window, cx)
                    && root.external_owner_is_current(receipt.focus.as_ref(), window, cx) && !window.has_active_dialog(cx);
                if root.bookmark_authoring.serial != serial || !root.bookmark_authoring.pending { return; }
                match result {
                    Ok(Some(completion)) => { root.finish_bookmark_authoring(serial); root.finish_bookmark_completion(&receipt, completion, current, window, cx); },
                    Ok(None) => { root.finish_bookmark_authoring(serial); root.bookmark_updated(current, cx); },
                    Err(error) if current => {
                        root.show_bookmark_review(receipt.clone(), retry_entry, Some(retry_alias), review, window, cx);
                        root.bookmark_authoring.feedback = Some(format!("Bookmark operation failed: {error:#}"));
                    },
                    Err(_) => { root.finish_bookmark_authoring(serial); }
                }
                cx.notify();
                let _ = window;
            }); });
            if updated.is_err() { let _ = root.update(cx, |root, cx| {
                if root.finish_bookmark_authoring(serial) { root.bookmark_updated(false, cx); cx.notify(); }
            }); }
            drop(permit);
        }).detach();
    }
}

#[cfg(test)]
mod tests {
    use super::{State, refresh_bookmark_query, normalized_add_bypass, retained_alias};
    use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
    #[test]
    fn retry_alias_draft_is_separate_from_canonical_record() {
        let expected = multi_launcher::plugins::bookmarks::BookmarkEntry { url: "http://example".into(), alias: Some("old".into()) };
        let draft = retained_alias("new", "new", "new");
        assert_eq!(draft, "new"); assert_eq!(expected.alias.as_deref(), Some("old"));
        assert_eq!(retained_alias("linebreak", "linebreak", "line\nbreak"), "line\nbreak");
        assert_eq!(retained_alias("edited", "linebreak", "line\nbreak"), "edited");
    }
    #[test]
    fn normalized_add_bypass_requires_exact_current_emission() {
        let action = multi_launcher::actions::Action { label: "Add bookmark https://example.com".into(), desc: "Bookmark".into(), action: "bookmark:add:https://example.com".into(), args: None };
        assert!(normalized_add_bypass(&action, true));
        assert!(!normalized_add_bypass(&action, false));
        let mut remove = action; remove.action = "bookmark:remove:https://example.com".into();
        assert!(!normalized_add_bypass(&remove, true));
    }
    #[test]
    fn bookmark_ack_does_not_reexecute_unrelated_calculator() {
        assert!(!refresh_bookmark_query("=2+2", false, false));
        assert!(refresh_bookmark_query("bm rm", false, false));
        assert!(refresh_bookmark_query("", false, true));
    }
    #[test]
    fn bookmark_attempts_are_exclusive_revocable_and_exactly_retired() {
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
