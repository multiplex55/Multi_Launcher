use crate::actions::Action;
use crate::common::persistence::LoadState;
use crate::plugin::{Plugin, PluginSearchUpdates};
use crate::plugins::bookmarks::{BOOKMARKS_FILE, load_bookmarks_typed};
use crate::plugins::fav::{FAV_FILE, load_favs_typed};
use crate::plugins::folders::{FOLDERS_FILE, default_folders, load_folders_typed};
use crate::target_repair::{TargetInspection, inspect_target};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use url::Url;

const REFRESH_TTL: Duration = Duration::from_secs(5);
const SOURCE: &str = "missing";

#[derive(Clone, Default)]
pub(crate) struct MissingScanInputs {
    state: Arc<Mutex<CustomActionInput>>,
}

#[derive(Clone, Default)]
struct CustomActionInput {
    revision: u64,
    actions: Arc<Vec<Action>>,
}

impl MissingScanInputs {
    pub(crate) fn replace_custom_actions(&self, actions: Arc<Vec<Action>>) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.actions.as_slice() == actions.as_slice() {
            return;
        }
        state.revision = state.revision.wrapping_add(1).max(1);
        state.actions = actions;
    }

    fn snapshot(&self) -> CustomActionInput {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    fn is_current(&self, revision: u64) -> bool {
        self.state
            .lock()
            .map(|state| state.revision == revision)
            .unwrap_or(false)
    }
}

#[derive(Clone)]
struct ScanCache {
    input_revision: u64,
    created: Instant,
    actions: Vec<Action>,
}

#[derive(Default)]
struct RuntimeState {
    active_ticket: Option<u64>,
    cache: Option<ScanCache>,
    terminal: bool,
}

enum ScanCommand {
    Scan {
        ticket: u64,
        input: CustomActionInput,
    },
    Shutdown,
}

/// Missing-target discovery is explicitly requested and performed away from
/// the search/UI thread. The provider itself only reads a last-good result and
/// reports a normal refresh ticket while the worker is active.
pub struct MissingPlugin {
    updates: Arc<PluginSearchUpdates>,
    inputs: MissingScanInputs,
    state: Arc<Mutex<RuntimeState>>,
    sender: Option<SyncSender<ScanCommand>>,
    worker: Option<JoinHandle<()>>,
    reaper: Option<crate::thread_reaper::ReapPermit>,
    cancelled: Arc<AtomicBool>,
}

impl Default for MissingPlugin {
    fn default() -> Self {
        Self::new(
            Arc::new(PluginSearchUpdates::default()),
            MissingScanInputs::default(),
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        )
    }
}

impl MissingPlugin {
    pub(crate) fn new(
        updates: Arc<PluginSearchUpdates>,
        inputs: MissingScanInputs,
        root: PathBuf,
    ) -> Self {
        let state = Arc::new(Mutex::new(RuntimeState::default()));
        let cancelled = Arc::new(AtomicBool::new(false));
        let Ok(reaper) = crate::thread_reaper::reserve() else {
            return Self {
                updates,
                inputs,
                state: Arc::new(Mutex::new(RuntimeState {
                    terminal: true,
                    ..RuntimeState::default()
                })),
                sender: None,
                worker: None,
                reaper: None,
                cancelled,
            };
        };
        let (sender, receiver) = sync_channel(1);
        let worker_state = Arc::clone(&state);
        let worker_inputs = inputs.clone();
        let worker_updates = Arc::clone(&updates);
        let worker_cancelled = Arc::clone(&cancelled);
        let notifier = reaper.completion_notifier();
        let worker = thread::Builder::new()
            .name("missing-target-scan".into())
            .spawn(move || {
                let _completion_notifier = notifier;
                run_scans(
                    receiver,
                    worker_state,
                    worker_inputs,
                    worker_updates,
                    worker_cancelled,
                    root,
                );
            });
        match worker {
            Ok(worker) => Self {
                updates,
                inputs,
                state,
                sender: Some(sender),
                worker: Some(worker),
                reaper: Some(reaper),
                cancelled,
            },
            Err(_) => Self {
                updates,
                inputs,
                state: Arc::new(Mutex::new(RuntimeState {
                    terminal: true,
                    ..RuntimeState::default()
                })),
                sender: None,
                worker: None,
                reaper: None,
                cancelled,
            },
        }
    }

    fn explicit_scan_query(query: &str) -> bool {
        query.trim().eq_ignore_ascii_case("check missing")
    }

    fn partial_query(query: &str) -> bool {
        let query = query.trim();
        let prefix = "check missing";
        !query.is_empty()
            && query.len() < prefix.len()
            && prefix.starts_with(&query.to_ascii_lowercase())
    }

    fn schedule_scan(&self) -> Vec<Action> {
        let input = self.inputs.snapshot();
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.terminal {
            return vec![status_action(
                "Missing-target checks are unavailable right now.",
                "missing:status:unavailable",
            )];
        }
        if let Some(cache) = state.cache.as_ref()
            && cache.input_revision == input.revision
            && cache.created.elapsed() < REFRESH_TTL
            && state.active_ticket.is_none()
        {
            return cache.actions.clone();
        }

        let ticket = self.updates.schedule_or_join(SOURCE);
        crate::plugin::record_search_refresh_ticket(SOURCE, ticket.id);
        if ticket.start {
            state.active_ticket = Some(ticket.id);
            let command = ScanCommand::Scan {
                ticket: ticket.id,
                input,
            };
            let send_result = self.sender.as_ref().map(|sender| sender.try_send(command));
            if !matches!(send_result, Some(Ok(()))) {
                state.active_ticket = None;
                state.terminal = true;
                self.updates.cancel_ticket(SOURCE, ticket.id);
                return vec![status_action(
                    "Missing-target checks are unavailable right now.",
                    "missing:status:unavailable",
                )];
            }
        } else {
            state.active_ticket.get_or_insert(ticket.id);
        }
        vec![status_action(
            "Checking saved targets…",
            "missing:status:checking",
        )]
    }
}

impl Drop for MissingPlugin {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
        let active = self
            .state
            .lock()
            .ok()
            .and_then(|mut state| state.active_ticket.take());
        if let Some(ticket) = active {
            self.updates.cancel_ticket(SOURCE, ticket);
        }
        if let Some(sender) = self.sender.take() {
            let _ = sender.try_send(ScanCommand::Shutdown);
            drop(sender);
        }
        if let (Some(worker), Some(reaper)) = (self.worker.take(), self.reaper.take())
            && let Err(error) = reaper.reap(worker)
        {
            tracing::error!(%error, "missing-target scan join supervision failed");
        }
    }
}

impl Plugin for MissingPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        if Self::partial_query(query) {
            return vec![status_action(
                "Type “check missing” to inspect saved targets.",
                "missing:status:complete-query",
            )];
        }
        if Self::explicit_scan_query(query) {
            return self.schedule_scan();
        }
        Vec::new()
    }

    fn name(&self) -> &str {
        "missing"
    }

    fn description(&self) -> &str {
        "Check saved entries for missing targets"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![Action {
            label: "check missing".into(),
            desc: "Maintenance".into(),
            action: "query:check missing".into(),
            args: None,
        }]
    }
}

fn run_scans(
    receiver: Receiver<ScanCommand>,
    state: Arc<Mutex<RuntimeState>>,
    inputs: MissingScanInputs,
    updates: Arc<PluginSearchUpdates>,
    cancelled: Arc<AtomicBool>,
    root: PathBuf,
) {
    while let Ok(command) = receiver.recv() {
        let ScanCommand::Scan { ticket, input } = command else {
            break;
        };
        if cancelled.load(Ordering::Acquire) {
            updates.cancel_ticket(SOURCE, ticket);
            break;
        }
        let result = scan_saved_entries(&root, &input.actions, &cancelled);
        if cancelled.load(Ordering::Acquire) {
            updates.cancel_ticket(SOURCE, ticket);
            break;
        }
        if !inputs.is_current(input.revision) {
            if let Ok(mut state) = state.lock()
                && state.active_ticket == Some(ticket)
            {
                state.active_ticket = None;
            }
            updates.cancel_ticket(SOURCE, ticket);
            continue;
        }
        if let Ok(mut worker_state) = state.lock()
            && worker_state.active_ticket == Some(ticket)
        {
            worker_state.cache = Some(ScanCache {
                input_revision: input.revision,
                created: Instant::now(),
                actions: result,
            });
            worker_state.active_ticket = None;
        }
        updates.publish_ticket(SOURCE, ticket);
    }
    if let Ok(mut state) = state.lock() {
        state.terminal = true;
    }
}

fn scan_saved_entries(
    root: &Path,
    custom_actions: &[Action],
    cancelled: &AtomicBool,
) -> Vec<Action> {
    let mut results = Vec::new();
    let mut uncertain = false;
    let folder_path = root.join(FOLDERS_FILE);
    let folders = match load_folders_typed(&folder_path) {
        Ok(LoadState::Loaded(folders)) => folders,
        Ok(LoadState::Missing | LoadState::Empty) => default_folders(),
        Err(_) => {
            uncertain = true;
            Vec::new()
        }
    };
    for folder in folders {
        if cancelled.load(Ordering::Acquire) {
            return Vec::new();
        }
        match inspect_target(&folder.path) {
            TargetInspection::Missing => {
                let display_name = folder.alias.as_deref().unwrap_or(&folder.label);
                results.push(Action {
                    label: display_name.to_owned(),
                    desc: folder.path.clone(),
                    action: folder.path.clone(),
                    args: None,
                });
                results.push(Action {
                    label: format!("Remove missing folder {}", folder.path),
                    desc: "Maintenance".into(),
                    action: format!("folder:remove:{}", folder.path),
                    args: None,
                });
            }
            TargetInspection::UnknownUnavailable | TargetInspection::NotFilesystem => {
                uncertain = true;
            }
            TargetInspection::Available(_) => {}
        }
    }

    let favorites_path = root.join(FAV_FILE);
    match load_favs_typed(&favorites_path) {
        Ok(LoadState::Loaded(favorites)) => {
            for favorite in favorites {
                if cancelled.load(Ordering::Acquire) {
                    return Vec::new();
                }
                let path = favorite.args.as_deref().unwrap_or(&favorite.action);
                if !Path::new(path).is_absolute() {
                    continue;
                }
                match inspect_target(path) {
                    TargetInspection::Missing => results.push(Action {
                        label: format!("Remove missing fav {}", favorite.label),
                        desc: "Maintenance".into(),
                        action: format!("fav:remove:{}", favorite.label),
                        args: None,
                    }),
                    TargetInspection::UnknownUnavailable => uncertain = true,
                    TargetInspection::Available(_) | TargetInspection::NotFilesystem => {}
                }
            }
        }
        Ok(LoadState::Missing | LoadState::Empty) => {}
        Err(_) => uncertain = true,
    }

    let bookmarks_path = root.join(BOOKMARKS_FILE);
    match load_bookmarks_typed(&bookmarks_path) {
        Ok(LoadState::Loaded(bookmarks)) => {
            for bookmark in bookmarks {
                if Url::parse(&bookmark.url).is_err() {
                    results.push(Action {
                        label: format!("Remove invalid bookmark {}", bookmark.url),
                        desc: "Maintenance".into(),
                        action: format!("bookmark:remove:{}", bookmark.url),
                        args: None,
                    });
                }
            }
        }
        Ok(LoadState::Missing | LoadState::Empty) => {}
        Err(_) => uncertain = true,
    }

    for action in custom_actions {
        if cancelled.load(Ordering::Acquire) {
            return Vec::new();
        }
        if !Path::new(&action.action).is_absolute() {
            continue;
        }
        match inspect_target(&action.action) {
            TargetInspection::Missing => results.push(action.clone()),
            TargetInspection::UnknownUnavailable => uncertain = true,
            TargetInspection::Available(_) | TargetInspection::NotFilesystem => {}
        }
    }

    if uncertain {
        results.push(status_action(
            "Some saved targets could not be checked. Refresh after resolving access issues.",
            "missing:status:uncertain",
        ));
    } else if results.is_empty() {
        results.push(status_action(
            "No missing entries were found.",
            "missing:status:complete",
        ));
    }
    results
}

fn status_action(label: &str, action: &str) -> Action {
    Action {
        label: label.to_owned(),
        desc: "Maintenance".into(),
        action: action.to_owned(),
        args: None,
    }
}

/// Informational maintenance rows are never launch targets, even when a
/// caller activates one through a legacy action boundary.
pub(crate) fn is_status_action(action: &str) -> bool {
    matches!(
        action,
        "missing:status:checking"
            | "missing:status:uncertain"
            | "missing:status:unavailable"
            | "missing:status:complete-query"
            | "missing:status:complete"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn saved_action(path: &Path, label: &str) -> Action {
        Action {
            label: label.to_owned(),
            desc: "Saved target".into(),
            action: path.to_string_lossy().into_owned(),
            args: Some("--kept exactly".into()),
        }
    }

    #[test]
    fn explicit_maintenance_query_is_async_and_returns_exact_missing_custom_record() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("not-created.exe");
        std::fs::write(dir.path().join(BOOKMARKS_FILE), "[]").unwrap();
        std::fs::write(dir.path().join(FAV_FILE), "[]").unwrap();
        std::fs::write(dir.path().join(FOLDERS_FILE), "[]").unwrap();
        let updates = Arc::new(PluginSearchUpdates::default());
        let inputs = MissingScanInputs::default();
        let expected = saved_action(&missing, "Keep full record");
        inputs.replace_custom_actions(Arc::new(vec![expected.clone()]));
        let plugin = MissingPlugin::new(Arc::clone(&updates), inputs, dir.path().to_path_buf());

        let partial = plugin.search("check miss");
        assert_eq!(partial[0].action, "missing:status:complete-query");
        assert_eq!(updates.active_ticket(SOURCE), None);

        let first = plugin.search("check missing");
        assert_eq!(first[0].action, "missing:status:checking");
        let ticket = updates
            .active_ticket(SOURCE)
            .expect("explicit query schedules scan");
        assert!(!updates.ticket_resolved(SOURCE, ticket));

        let mut completed = None;
        for _ in 0..20_000 {
            let found = plugin.search("check missing");
            if found.iter().any(|action| action == &expected) {
                completed = Some(found);
                break;
            }
            thread::yield_now();
        }
        let completed = completed.expect("worker publishes the missing custom record");
        assert!(completed.iter().any(|action| action == &expected));
        assert!(updates.ticket_resolved(SOURCE, ticket));
    }

    #[test]
    fn maintenance_query_requires_the_complete_command_before_scanning() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(BOOKMARKS_FILE), "[]").unwrap();
        std::fs::write(dir.path().join(FAV_FILE), "[]").unwrap();
        std::fs::write(dir.path().join(FOLDERS_FILE), "[]").unwrap();
        let updates = Arc::new(PluginSearchUpdates::default());
        let plugin = MissingPlugin::new(
            Arc::clone(&updates),
            MissingScanInputs::default(),
            dir.path().to_path_buf(),
        );

        assert_eq!(
            plugin.search("check missing anything"),
            Vec::<Action>::new(),
            "extending the command must not repeat filesystem inspection"
        );
        assert_eq!(updates.active_ticket(SOURCE), None);
        assert_eq!(
            plugin.search(" CHECK MISSING ")[0].action,
            "missing:status:checking"
        );
    }
}
