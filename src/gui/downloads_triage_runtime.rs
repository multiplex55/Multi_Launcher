//! Executes retained Downloads Triage requests against the app profile.
use super::add_action_dialog::NativeDialogParent;
use super::downloads_triage_dialog::{
    Command, DownloadsTriageDialog, ExtractionStatus, Runtime, Selection, TransferKind,
};
use crate::archive::{ArchiveCoverage, ArchiveProvenance};
use crate::downloads_triage::{DownloadsStore, Scan, proposed_root, scan};
use crate::file_operations::ScanCancellation;
use crate::file_transfer::{
    DownloadsScanCompletion, EntrySnapshot, TransferLeaf, compare_downloads_pair,
    downloads_review_key, recycle_downloads, rename_downloads_entry, with_downloads_entry_path,
};
use crate::platform::app_data::AppDataRoot;
use anyhow::{Context, Result, ensure};
use eframe::egui;
use std::collections::{HashMap, HashSet};
use std::num::NonZeroIsize;
use std::path::PathBuf;
use std::sync::{Arc, mpsc};
use std::time::{SystemTime, UNIX_EPOCH};

type Key = [u8; 32];

pub(super) enum Handoff {
    Transfer {
        entries: Vec<EntrySnapshot>,
        kind: TransferKind,
        suggestion: Option<PathBuf>,
    },
    ExtractZip {
        entry: EntrySnapshot,
        suggestion: Option<PathBuf>,
    },
    Inspect(EntrySnapshot),
}

struct ScanUpdate {
    proposed_root: Option<PathBuf>,
    scan: Option<Arc<Scan>>,
    suggestion: Option<PathBuf>,
    observed_time: Option<i64>,
    reviewed: Arc<HashSet<Key>>,
    previously_extracted: Arc<HashMap<Key, ExtractionStatus>>,
    notice: Option<String>,
}

enum Update {
    Scan(ScanUpdate),
    Reviewed(Arc<HashSet<Key>>),
    Suggestion(PathBuf),
    Notice(String),
    Changed(String),
    Compared((Key, Key), bool),
}

enum FolderChange {
    Keep,
    Set(Option<PathBuf>),
}

struct Work {
    session: u64,
    cancellation: ScanCancellation,
    result: mpsc::Receiver<Result<Update>>,
}

#[derive(Default)]
pub(super) struct DownloadsTriageRuntime {
    view: Runtime,
    rows: HashMap<Key, usize>,
    profile: Option<AppDataRoot>,
    work: Option<Work>,
    deferred_open: Option<(egui::Context, u64)>,
}

impl DownloadsTriageRuntime {
    pub(super) fn snapshot(&self) -> &Runtime {
        &self.view
    }

    pub(super) fn needs_poll(&self) -> bool {
        self.work.is_some() || self.deferred_open.is_some()
    }

    pub(super) fn close_view(&mut self) {
        self.deferred_open = None;
        if let Some(work) = &self.work {
            work.cancellation.cancel();
        }
        self.view = Runtime::default();
        self.rows.clear();
    }

    pub(super) fn open_view(&mut self, ctx: &egui::Context, session: u64, profile: AppDataRoot) {
        self.profile = Some(profile);
        if self.work.is_some() {
            self.deferred_open = Some((ctx.clone(), session));
            self.view.loading = true;
        } else {
            self.refresh(ctx, session, FolderChange::Keep);
        }
    }

    fn refresh(&mut self, ctx: &egui::Context, session: u64, change: FolderChange) {
        self.run(ctx, session, move |profile, cancel| {
            let mut store = DownloadsStore::open(profile)?;
            let mut preferences = store.preferences();
            if let FolderChange::Set(path) = change {
                ensure!(!cancel.is_cancelled(), "Downloads scan cancelled.");
                preferences.root = path;
                store.set_preferences(&preferences)?;
            }
            let mut result = ScanUpdate {
                proposed_root: preferences.root.clone(),
                scan: None,
                suggestion: preferences.destination,
                observed_time: None,
                reviewed: Arc::default(),
                previously_extracted: Arc::default(),
                notice: None,
            };
            let root = match proposed_root(preferences.root.as_deref()) {
                Ok(root) => root,
                Err(error) => {
                    result.notice = Some(format!("Could not resolve Windows Downloads ({error})."));
                    return Ok(Update::Scan(result));
                }
            };
            result.proposed_root = Some(root.clone());
            let now = SystemTime::now();
            match scan(&root, cancel, now) {
                Ok(current)
                    if !matches!(current.completion, DownloadsScanCompletion::Cancelled) =>
                {
                    result.reviewed = reviewed(&store, &current);
                    match ArchiveProvenance::read(profile) {
                        Ok(provenance) => {
                            result.previously_extracted = Arc::new(
                                current
                                    .rows
                                    .iter()
                                    .filter_map(|row| {
                                        let status = match provenance.coverage_for(&row.entry)? {
                                            ArchiveCoverage::Full => ExtractionStatus::Full,
                                            ArchiveCoverage::Selection => {
                                                ExtractionStatus::Selection
                                            }
                                        };
                                        Some((row.review_proof, status))
                                    })
                                    .collect(),
                            );
                        }
                        Err(error) => {
                            result.notice = Some(format!(
                                "Could not read prior ZIP extraction records ({error})."
                            ));
                        }
                    }
                    result.observed_time = now
                        .duration_since(UNIX_EPOCH)
                        .ok()
                        .and_then(|age| age.as_secs().checked_add(11_644_473_600))
                        .and_then(|seconds| seconds.checked_mul(10_000_000))
                        .and_then(|ticks| i64::try_from(ticks).ok());
                    result.scan = Some(Arc::new(current));
                }
                Ok(_) => result.notice = Some("Downloads scan cancelled.".into()),
                Err(error) => result.notice = Some(error.to_string()),
            }
            Ok(Update::Scan(result))
        });
    }

    fn run(
        &mut self,
        ctx: &egui::Context,
        session: u64,
        execute: impl FnOnce(&AppDataRoot, &ScanCancellation) -> Result<Update> + Send + 'static,
    ) {
        if self.work.is_some() {
            return;
        }
        let Some(profile) = self.profile.clone() else {
            self.view.loading = false;
            self.view.notice = Some("Downloads profile is unavailable.".into());
            return;
        };
        let ctx = ctx.clone();
        let cancellation = ScanCancellation::new();
        let worker_cancel = cancellation.clone();
        let (sender, result) = mpsc::channel();
        let spawn = std::thread::Builder::new()
            .name("downloads-triage".into())
            .spawn(move || {
                let value = execute(&profile, &worker_cancel);
                let _ = sender.send(value);
                ctx.request_repaint_of(egui::ViewportId::ROOT);
            });
        match spawn {
            Ok(_) => {
                self.view.loading = true;
                self.view.notice = None;
                self.work = Some(Work {
                    session,
                    cancellation,
                    result,
                });
            }
            Err(error) => {
                self.view.loading = false;
                self.view.notice = Some(format!("Could not start Downloads work ({error})."));
            }
        }
    }

    pub(super) fn poll(&mut self, dialog: &mut DownloadsTriageDialog) {
        let Some(work) = &self.work else { return };
        let result = match work.result.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => Err(anyhow::anyhow!(
                "Downloads work stopped before returning a result."
            )),
        };
        let session = work.session;
        self.work = None;
        if dialog.open && session == dialog.session() {
            self.view.loading = false;
            match result {
                Ok(Update::Scan(scan)) => self.apply_scan(scan),
                Ok(Update::Reviewed(reviewed)) => self.view.reviewed = reviewed,
                Ok(Update::Suggestion(path)) => self.view.suggested_destination = Some(path),
                Ok(Update::Notice(message)) => self.view.notice = Some(message),
                Ok(Update::Changed(message)) => {
                    self.view.scan = None;
                    self.rows.clear();
                    self.view.reviewed = Arc::default();
                    self.view.verified_pairs = Arc::default();
                    self.view.previously_extracted = Arc::default();
                    self.view.notice = Some(message);
                }
                Ok(Update::Compared(pair, identical)) => {
                    let verified = Arc::make_mut(&mut self.view.verified_pairs);
                    let message = if identical {
                        verified.insert(pair);
                        "The two selected files have the same verified main data."
                    } else {
                        verified.remove(&pair);
                        "The selected files have different main data."
                    };
                    self.view.notice = Some(message.into());
                }
                Err(error) => self.view.notice = Some(format!("{error:#}")),
            }
        }
        if let Some((ctx, pending)) = self.deferred_open.take()
            && dialog.open
            && pending == dialog.session()
        {
            self.refresh(&ctx, pending, FolderChange::Keep);
        }
    }

    fn apply_scan(&mut self, scan: ScanUpdate) {
        let Some(next) = self.view.scan_generation.checked_add(1) else {
            self.view.scan = None;
            self.rows.clear();
            self.view.notice = Some("Downloads view has exhausted its scan identity.".into());
            return;
        };
        self.view.scan_generation = next;
        self.view.proposed_root = scan.proposed_root;
        self.view.scan = scan.scan;
        self.view.suggested_destination = scan.suggestion;
        self.view.now_filetime = scan.observed_time;
        self.view.reviewed = scan.reviewed;
        self.view.verified_pairs = Arc::default();
        self.view.previously_extracted = scan.previously_extracted;
        self.view.notice = scan.notice;
        self.rows.clear();
        if let Some(scan) = &self.view.scan {
            self.rows.extend(
                scan.rows
                    .iter()
                    .enumerate()
                    .map(|(i, row)| (row.review_proof, i)),
            );
        }
    }

    fn accepts(&self, dialog: &DownloadsTriageDialog, selection: &Selection) -> bool {
        let Some(scan) = &self.view.scan else {
            return false;
        };
        if !dialog.open
            || self.view.loading
            || dialog.session() != selection.ticket.session
            || self.view.scan_generation != selection.ticket.scan_generation
            || selection.entries.is_empty()
            || selection.entries.len() > self.rows.len()
        {
            return false;
        }
        let mut seen = HashSet::with_capacity(selection.entries.len());
        selection.entries.iter().all(|entry| {
            let key = downloads_review_key(entry);
            seen.insert(key)
                && self
                    .rows
                    .get(&key)
                    .is_some_and(|index| scan.rows[*index].entry == *entry)
        })
    }

    fn choose_directory(
        &self,
        dialog: &DownloadsTriageDialog,
        session: u64,
        hwnd: usize,
    ) -> Option<PathBuf> {
        if !dialog.open || dialog.session() != session || self.view.loading {
            return None;
        }
        let hwnd = NonZeroIsize::new(hwnd as isize)?;
        rfd::FileDialog::new()
            .set_parent(&NativeDialogParent(hwnd))
            .pick_folder()
    }

    pub(super) fn dispatch(
        &mut self,
        ctx: &egui::Context,
        dialog: &DownloadsTriageDialog,
        command: Command,
    ) -> Option<Handoff> {
        let session = dialog.session();
        match command {
            Command::Closed => self.close_view(),
            Command::Refresh { session: required } if required == session && dialog.open => {
                self.refresh(ctx, session, FolderChange::Keep);
            }
            Command::UseWindowsDownloads { session: required }
                if required == session && dialog.open =>
            {
                self.refresh(ctx, session, FolderChange::Set(None));
            }
            Command::ChooseRoot {
                session: required,
                hwnd,
            } => {
                if let Some(path) = self.choose_directory(dialog, required, hwnd) {
                    self.refresh(ctx, session, FolderChange::Set(Some(path)));
                }
            }
            Command::ChooseSuggestion {
                session: required,
                hwnd,
            } => {
                if let Some(path) = self.choose_directory(dialog, required, hwnd) {
                    self.run(ctx, session, move |profile, cancel| {
                        let mut store = DownloadsStore::open(profile)?;
                        let mut preferences = store.preferences();
                        ensure!(
                            !cancel.is_cancelled(),
                            "Downloads destination selection cancelled."
                        );
                        preferences.destination = Some(path.clone());
                        store.set_preferences(&preferences)?;
                        Ok(Update::Suggestion(path))
                    });
                }
            }
            Command::Mark {
                selection,
                reviewed: mark,
            } if self.accepts(dialog, &selection) => {
                let current = self.view.scan.clone()?;
                self.run(ctx, session, move |profile, cancel| {
                    let mut store = DownloadsStore::open(profile)?;
                    store.set_reviewed(
                        &selection.entries.iter().collect::<Vec<_>>(),
                        mark,
                        cancel,
                    )?;
                    Ok(Update::Reviewed(reviewed(&store, &current)))
                });
            }
            Command::Transfer { selection, kind } if self.accepts(dialog, &selection) => {
                return Some(Handoff::Transfer {
                    entries: selection.entries,
                    kind,
                    suggestion: self.view.suggested_destination.clone(),
                });
            }
            Command::ExtractZip(selection)
                if self.accepts(dialog, &selection) && selection.entries.len() == 1 =>
            {
                return Some(Handoff::ExtractZip {
                    entry: selection.entries.into_iter().next()?,
                    suggestion: self.view.suggested_destination.clone(),
                });
            }
            Command::Inspect(selection)
                if self.accepts(dialog, &selection) && selection.entries.len() == 1 =>
            {
                return selection.entries.into_iter().next().map(Handoff::Inspect);
            }
            Command::Open(selection)
                if self.accepts(dialog, &selection) && selection.entries.len() == 1 =>
            {
                let entry = selection.entries.into_iter().next()?;
                self.run(ctx, session, move |_, cancel| {
                    with_downloads_entry_path(
                        &entry,
                        cancel,
                        crate::file_search::actions::open_path,
                    )?;
                    Ok(Update::Notice(
                        "Asked Windows to open the selected item.".into(),
                    ))
                });
            }
            Command::Reveal(selection)
                if self.accepts(dialog, &selection) && selection.entries.len() == 1 =>
            {
                let entry = selection.entries.into_iter().next()?;
                self.run(ctx, session, move |_, cancel| {
                    with_downloads_entry_path(
                        &entry,
                        cancel,
                        crate::file_search::actions::reveal_path,
                    )?;
                    Ok(Update::Notice(
                        "Asked Explorer to reveal the selected item.".into(),
                    ))
                });
            }
            Command::Rename { selection, name }
                if self.accepts(dialog, &selection) && selection.entries.len() == 1 =>
            {
                let entry = selection.entries.into_iter().next()?;
                self.run(ctx, session, move |_, cancel| {
                    let leaf = TransferLeaf::new(std::ffi::OsStr::new(&name))?;
                    let renamed = rename_downloads_entry(&entry, &leaf, cancel)?;
                    Ok(Update::Changed(format!(
                        "Renamed to {}. Refresh to review the current folder.",
                        renamed.display()
                    )))
                });
            }
            Command::Compare(selection)
                if self.accepts(dialog, &selection) && selection.entries.len() == 2 =>
            {
                self.run(ctx, session, move |_, cancel| {
                    let first = &selection.entries[0];
                    let second = &selection.entries[1];
                    let mut keys = [downloads_review_key(first), downloads_review_key(second)];
                    keys.sort_unstable();
                    let identical = compare_downloads_pair(first, second, cancel)?;
                    Ok(Update::Compared((keys[0], keys[1]), identical))
                });
            }
            Command::Recycle { selection, hwnd } if self.accepts(dialog, &selection) => {
                self.run(ctx, session, move |_, cancel| {
                    let report = recycle_downloads(&selection.entries, hwnd, cancel)?;
                    let mut note = format!(
                        "Windows confirmed recycling {} of {} selected items. Attempted {}.",
                        report.recycled,
                        selection.entries.len(),
                        report.attempted,
                    );
                    if report.stopped {
                        note.push_str(" Windows stopped processing the selection.");
                    }
                    for (path, issue) in report.issues.iter().take(3) {
                        note.push_str(&format!("\n{} · {issue}", path.display()));
                    }
                    note.push_str("\nRefresh to review the current folder.");
                    Ok(Update::Changed(note))
                });
            }
            _ => {}
        }
        None
    }
}

impl super::LauncherApp {
    pub(super) fn dispatch_downloads_triage(
        &mut self,
        ctx: &egui::Context,
        command: Command,
        origin: &super::utility_host::AcceptedParentOpen<u64>,
    ) {
        use super::file_operations_center_dialog::ActionKind;
        use super::utility_host::{ChildOpenOrigin, UtilityPresentation};
        use crate::commands::DialogCommandHost;

        let Some(handoff) =
            self.downloads_triage_runtime
                .dispatch(ctx, &self.downloads_triage_dialog, command)
        else {
            return;
        };
        let origin = ChildOpenOrigin::Parent(origin.clone());
        match handoff {
            Handoff::Transfer {
                entries,
                kind,
                suggestion,
            } => {
                let kind = match kind {
                    TransferKind::Copy => ActionKind::Copy,
                    TransferKind::Move => ActionKind::Move,
                };
                self.open_file_operations_from_origin(None, &origin);
                self.file_operations_center_dialog
                    .open_with_reviewed_sources(entries, kind, suggestion);
            }
            Handoff::ExtractZip { entry, suggestion } => match self.open_zip_extractor_dialog() {
                Ok(()) => self.zip_extractor_dialog.load_exact_archive(
                    entry,
                    suggestion,
                    &mut self.archive_session,
                ),
                Err(message) => {
                    self.downloads_triage_dialog
                        .set_notice(self.downloads_triage_dialog.session(), message);
                }
            },
            Handoff::Inspect(entry) => {
                use crate::file_transfer::{EntryKind, recheck_source_metadata};
                if let Err(error) = recheck_source_metadata(&entry, &ScanCancellation::new()) {
                    self.downloads_triage_dialog
                        .set_notice(self.downloads_triage_dialog.session(), error.to_string());
                    return;
                }
                let path = entry.path();
                match entry.metadata().kind {
                    EntryKind::Directory => {
                        if let ChildOpenOrigin::Parent(receipt) = origin {
                            self.folder_size_dialog.open_from_parent(ctx, path, receipt);
                            self.focus_panel(super::Panel::FolderSizeDialog);
                        }
                    }
                    EntryKind::File => {
                        let was_open = self.hash_generator_dialog.open;
                        self.hash_generator_dialog.open_with_file(path);
                        let desired = self.hash_generator_host.open_from_origin(
                            ctx,
                            &self.hash_generator_dialog,
                            was_open,
                            &origin,
                        );
                        self.panel_stack
                            .retain(|panel| *panel != super::Panel::HashGeneratorDialog);
                        self.panel_stack.push(super::Panel::HashGeneratorDialog);
                        if desired == Some(UtilityPresentation::Docked) {
                            self.request_launcher_visibility(true);
                        }
                    }
                }
            }
        }
    }
}

fn reviewed(store: &DownloadsStore, scan: &Scan) -> Arc<HashSet<Key>> {
    Arc::new(
        scan.rows
            .iter()
            .filter_map(|row| {
                store
                    .is_reviewed(&row.review_proof)
                    .then_some(row.review_proof)
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::downloads_triage::DownloadsPreferences;
    use crate::gui::downloads_triage_dialog::ViewTicket;
    use std::time::{Duration, Instant};

    fn settle(runtime: &mut DownloadsTriageRuntime, dialog: &mut DownloadsTriageDialog) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while runtime.needs_poll() {
            runtime.poll(dialog);
            assert!(
                Instant::now() < deadline,
                "The scoped Downloads request did not settle."
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn downloads_triage_runtime_marks_exact_rows_and_rejects_a_stale_transfer_ticket() {
        let files = tempfile::tempdir().unwrap();
        let profile_dir = tempfile::tempdir().unwrap();
        let output = tempfile::tempdir().unwrap();
        std::fs::write(files.path().join("selected.txt"), b"selected").unwrap();
        std::fs::write(files.path().join("other.txt"), b"other").unwrap();
        let profile = AppDataRoot::from_path(profile_dir.path());
        let mut store = DownloadsStore::open(&profile).unwrap();
        store
            .set_preferences(&DownloadsPreferences {
                root: Some(files.path().into()),
                destination: Some(output.path().into()),
            })
            .unwrap();

        let ctx = egui::Context::default();
        let mut dialog = DownloadsTriageDialog::default();
        dialog.open();
        let mut runtime = DownloadsTriageRuntime::default();
        runtime.open_view(&ctx, dialog.session(), profile);
        settle(&mut runtime, &mut dialog);

        let entry = runtime
            .snapshot()
            .scan
            .as_ref()
            .unwrap()
            .rows
            .iter()
            .find(|row| row.entry.leaf().as_os_str() == "selected.txt")
            .unwrap()
            .entry
            .clone();
        let selection = Selection {
            ticket: ViewTicket {
                session: dialog.session(),
                scan_generation: runtime.snapshot().scan_generation,
            },
            entries: vec![entry.clone()],
        };
        runtime.dispatch(
            &ctx,
            &dialog,
            Command::Mark {
                selection: selection.clone(),
                reviewed: true,
            },
        );
        settle(&mut runtime, &mut dialog);
        assert!(
            runtime
                .snapshot()
                .reviewed
                .contains(&downloads_review_key(&entry))
        );
        match runtime.dispatch(
            &ctx,
            &dialog,
            Command::Transfer {
                selection: selection.clone(),
                kind: TransferKind::Copy,
            },
        ) {
            Some(Handoff::Transfer {
                entries,
                kind: TransferKind::Copy,
                suggestion,
            }) => {
                assert_eq!(entries, vec![entry]);
                assert_eq!(suggestion.as_deref(), Some(output.path()));
            }
            _ => panic!("The current selection should be admitted exactly."),
        }

        runtime.dispatch(
            &ctx,
            &dialog,
            Command::Refresh {
                session: dialog.session(),
            },
        );
        settle(&mut runtime, &mut dialog);
        assert!(
            runtime
                .dispatch(
                    &ctx,
                    &dialog,
                    Command::Transfer {
                        selection,
                        kind: TransferKind::Move
                    }
                )
                .is_none()
        );
        dialog.close();
        runtime.close_view();
        assert!(!runtime.needs_poll());
        assert!(runtime.snapshot().scan.is_none());
    }
}
