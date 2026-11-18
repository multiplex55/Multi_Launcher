//! Retained Downloads Triage view. All file operations leave as typed intents.
use crate::downloads_triage::{Category, Origin, Scan};
use crate::file_transfer::{DownloadsScanCompletion, EntryKind, EntrySnapshot, TransferLeaf};
use crate::gui::{
    jump_mode::{
        TargetKind,
        ui::{
            JumpModeScope, NavigationDialogKind, NavigationOwner, NavigationViewportBinding,
            register_and_apply_target,
        },
    },
    utility_host::{UtilityPresentation, presentation_button},
    utility_window_host::HostedUtilityDialog,
};
use eframe::egui;
use std::{
    collections::{HashMap, HashSet},
    ffi::OsStr,
    path::PathBuf,
    sync::Arc,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ViewTicket {
    pub session: u64,
    pub scan_generation: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Selection {
    pub ticket: ViewTicket,
    pub entries: Vec<EntrySnapshot>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TransferKind {
    Copy,
    Move,
}
#[derive(Debug)]
pub(super) enum Command {
    Closed,
    ChooseRoot {
        session: u64,
        hwnd: usize,
    },
    UseWindowsDownloads {
        session: u64,
    },
    Refresh {
        session: u64,
    },
    ChooseSuggestion {
        session: u64,
        hwnd: usize,
    },
    Mark {
        selection: Selection,
        reviewed: bool,
    },
    Open(Selection),
    Reveal(Selection),
    Rename {
        selection: Selection,
        name: String,
    },
    Transfer {
        selection: Selection,
        kind: TransferKind,
    },
    ExtractZip(Selection),
    Compare(Selection),
    Inspect(Selection),
    Recycle {
        selection: Selection,
        hwnd: usize,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ExtractionStatus {
    Full,
    Selection,
}
#[derive(Default)]
pub(super) struct Runtime {
    pub proposed_root: Option<PathBuf>,
    pub suggested_destination: Option<PathBuf>,
    pub scan: Option<Arc<Scan>>,
    pub scan_generation: u64,
    /// Current Windows FILETIME supplied by the controller for age groups.
    pub now_filetime: Option<i64>,
    pub reviewed: Arc<HashSet<[u8; 32]>>,
    pub verified_pairs: Arc<HashSet<([u8; 32], [u8; 32])>>,
    pub previously_extracted: Arc<HashMap<[u8; 32], ExtractionStatus>>,
    pub loading: bool,
    pub notice: Option<String>,
}
#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Filter {
    #[default]
    All,
    Unreviewed,
    Reviewed,
    Incomplete,
    Large,
    Installer,
    Archive,
    SameSize,
}
#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Group {
    #[default]
    Type,
    Modified,
    Size,
    Origin,
}
#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum RowSort {
    #[default]
    Name,
    Modified,
    Size,
}

#[derive(Clone, Default)]
struct View {
    generation: u64,
    selected: HashSet<[u8; 32]>,
    search: String,
    filter: Filter,
    group: Group,
    sort: RowSort,
    page: usize,
    skipped_page: usize,
    rename: String,
    recycle: Option<Selection>,
    recycle_page: usize,
}
#[derive(Default)]
pub(super) struct DownloadsTriageDialog {
    pub open: bool,
    session: u64,
    view: View,
    notice: Option<String>,
}
impl DownloadsTriageDialog {
    pub fn open(&mut self) {
        if !self.open {
            if let Some(next) = self.session.checked_add(1) {
                self.session = next;
                self.open = true;
                self.view = View::default();
                self.notice = None;
            }
        }
    }
    pub fn ensure_open(&mut self) {
        self.open();
    }
    pub fn close(&mut self) {
        self.open = false;
        self.view.recycle = None;
        self.view.selected.clear();
    }
    pub fn session(&self) -> u64 {
        self.session
    }
    pub fn set_notice(&mut self, session: u64, notice: String) -> bool {
        if !self.open || self.session != session {
            return false;
        }
        self.notice = Some(notice);
        true
    }
}
#[derive(Default)]
pub(super) struct Actions {
    session: u64,
    generation: u64,
    view: View,
    scan: Option<Arc<Scan>>,
    loading: bool,
    close: bool,
    host_request: Option<UtilityPresentation>,
    intent: Option<Intent>,
}
#[derive(Clone, Copy)]
enum Intent {
    ChooseRoot,
    UseWindowsDownloads,
    Refresh,
    ChooseSuggestion,
    Mark(bool),
    Open,
    Reveal,
    Rename,
    Transfer(TransferKind),
    ExtractZip,
    Compare,
    Inspect,
    BeginRecycle,
    ConfirmRecycle,
    CancelRecycle,
}
impl HostedUtilityDialog for DownloadsTriageDialog {
    const KIND: Option<NavigationDialogKind> = Some(NavigationDialogKind::DownloadsTriage);
    const KEY: &'static str = "downloads-triage";
    const TITLE: &'static str = "Downloads Triage";
    const SIZE: egui::Vec2 = egui::vec2(860.0, 680.0);
    const MINIMUM: egui::Vec2 = egui::vec2(440.0, 340.0);
    const SCROLL_NATIVE_WINDOW: bool = false;
    const REGISTER_TARGETS_ON_REPLAY: bool = true;
    type Runtime = Runtime;
    type Actions = Actions;
    type Checkpoint = ();
    type Command = Command;
    fn is_open(&self) -> bool {
        self.open
    }
    fn session(&self) -> u64 {
        self.session
    }
    fn parent_targets_allowed(&self, _: &Runtime) -> bool {
        true
    }
    fn parent_input_allowed(&self) -> bool {
        true
    }
    fn popup_layer(&self, _: &egui::Context, _: egui::LayerId) -> Option<egui::LayerId> {
        None
    }
    fn confirmation_binding(
        &self,
        _: egui::ViewportId,
        _: u64,
    ) -> Option<NavigationViewportBinding> {
        None
    }
    fn capture(&self) {}
    fn restore(&mut self, _: ()) {}
    fn accept_close(&mut self, _: bool) -> bool {
        if self.view.recycle.take().is_some() {
            false
        } else {
            true
        }
    }
    fn render_host(
        &mut self,
        ctx: &egui::Context,
        scopes: (Option<&JumpModeScope>, Option<&JumpModeScope>),
        _: NavigationOwner,
        parent: bool,
        _: NavigationOwner,
        _: bool,
        _: u64,
        runtime: &Runtime,
        child: Option<&mut egui::Ui>,
        presentation: UtilityPresentation,
    ) -> Actions {
        let mut view = self.view.clone();
        if view.generation != runtime.scan_generation {
            view.generation = runtime.scan_generation;
            view.selected.clear();
            view.page = 0;
            view.skipped_page = 0;
            view.recycle = None;
        }
        let mut actions = Actions {
            session: self.session,
            generation: runtime.scan_generation,
            view,
            scan: runtime.scan.clone(),
            loading: runtime.loading,
            ..Default::default()
        };
        let mut open = true;
        let mut show = |ui: &mut egui::Ui| {
            ui.add_enabled_ui(parent, |ui| {
                self.contents(ui, &mut actions, runtime, scopes.0, presentation)
            });
        };
        if let Some(ui) = child {
            show(ui);
        } else {
            egui::Window::new(Self::TITLE)
                .id(Self::layer().id)
                .open(&mut open)
                .default_size(Self::SIZE)
                .min_size(Self::MINIMUM)
                .show(ctx, show);
        }
        actions.close |= !open;
        actions
    }
    fn host_request(actions: &Actions) -> Option<UtilityPresentation> {
        actions.host_request
    }
    fn closed_command() -> Command {
        Command::Closed
    }
    fn command_closes(command: &Command) -> bool {
        matches!(command, Command::Closed)
    }
    fn complete_actions(&mut self, actions: Actions, hwnd: Option<usize>) -> Vec<Command> {
        if !self.open || self.session != actions.session {
            return Vec::new();
        }
        if actions.close {
            if self.view.recycle.take().is_some() {
                return Vec::new();
            }
            self.close();
            return vec![Command::Closed];
        }
        self.view = actions.view;
        let Some(intent) = actions.intent else {
            return Vec::new();
        };
        let ticket = ViewTicket {
            session: self.session,
            scan_generation: actions.generation,
        };
        let selection = || -> Option<Selection> {
            let scan = actions.scan.as_ref()?;
            let entries: Vec<_> = scan
                .rows
                .iter()
                .filter(|row| self.view.selected.contains(&row.review_proof))
                .map(|row| row.entry.clone())
                .collect();
            (!entries.is_empty()).then_some(Selection { ticket, entries })
        };
        let command = match intent {
            Intent::ChooseRoot | Intent::ChooseSuggestion => match hwnd {
                Some(hwnd) if matches!(intent, Intent::ChooseRoot) => Command::ChooseRoot {
                    session: self.session,
                    hwnd,
                },
                Some(hwnd) => Command::ChooseSuggestion {
                    session: self.session,
                    hwnd,
                },
                None => {
                    self.notice = Some("The file picker is unavailable for this window.".into());
                    return Vec::new();
                }
            },
            Intent::UseWindowsDownloads => Command::UseWindowsDownloads {
                session: self.session,
            },
            Intent::Refresh => Command::Refresh {
                session: self.session,
            },
            Intent::CancelRecycle => {
                self.view.recycle = None;
                return Vec::new();
            }
            Intent::BeginRecycle if !actions.loading => {
                self.view.recycle = selection();
                self.view.recycle_page = 0;
                return Vec::new();
            }
            Intent::ConfirmRecycle if !actions.loading => {
                let Some(current) = selection() else {
                    self.view.recycle = None;
                    return Vec::new();
                };
                if self.view.recycle.as_ref() != Some(&current) {
                    self.view.recycle = None;
                    return Vec::new();
                }
                self.view.recycle = None;
                let Some(hwnd) = hwnd else {
                    self.notice =
                        Some("This window cannot own a Recycle Bin request right now.".into());
                    return Vec::new();
                };
                Command::Recycle {
                    selection: current,
                    hwnd,
                }
            }
            Intent::Mark(reviewed) if !actions.loading => match selection() {
                Some(selection) => Command::Mark {
                    selection,
                    reviewed,
                },
                None => return Vec::new(),
            },
            Intent::Open if !actions.loading => match selection() {
                Some(s) if s.entries.len() == 1 => Command::Open(s),
                _ => return Vec::new(),
            },
            Intent::Reveal if !actions.loading => match selection() {
                Some(s) if s.entries.len() == 1 => Command::Reveal(s),
                _ => return Vec::new(),
            },
            Intent::Inspect if !actions.loading => match selection() {
                Some(s) if s.entries.len() == 1 => Command::Inspect(s),
                _ => return Vec::new(),
            },
            Intent::Rename if !actions.loading => match selection() {
                Some(s)
                    if s.entries.len() == 1
                        && TransferLeaf::new(OsStr::new(&self.view.rename)).is_ok() =>
                {
                    Command::Rename {
                        selection: s,
                        name: self.view.rename.clone(),
                    }
                }
                _ => return Vec::new(),
            },
            Intent::Transfer(kind) if !actions.loading => match selection() {
                Some(selection) => Command::Transfer { selection, kind },
                None => return Vec::new(),
            },
            Intent::ExtractZip if !actions.loading => match selection() {
                Some(s) if s.entries.len() == 1 && is_zip(&s.entries[0]) => Command::ExtractZip(s),
                _ => return Vec::new(),
            },
            Intent::Compare if !actions.loading => match selection() {
                Some(s) if comparable(&s.entries) => Command::Compare(s),
                _ => return Vec::new(),
            },
            _ => return Vec::new(),
        };
        self.view.recycle = None;
        vec![command]
    }
    #[cfg(test)]
    fn test_button(_: &Actions) -> Option<(egui::Rect, egui::Rect, bool)> {
        None
    }
}
fn is_zip(entry: &EntrySnapshot) -> bool {
    entry.metadata().kind == EntryKind::File
        && entry
            .path()
            .extension()
            .is_some_and(|s| s.eq_ignore_ascii_case("zip"))
}
fn comparable(entries: &[EntrySnapshot]) -> bool {
    entries.len() == 2
        && entries.iter().all(|e| e.metadata().kind == EntryKind::File)
        && entries[0].metadata().byte_len.is_some()
        && entries[0].metadata().byte_len == entries[1].metadata().byte_len
}
fn date(value: i64) -> String {
    let seconds = value
        .checked_div(10_000_000)
        .and_then(|n| n.checked_sub(11_644_473_600));
    seconds
        .and_then(|s| chrono::DateTime::from_timestamp(s, 0))
        .map(|v| v.format("%Y-%m-%d %H:%M UTC").to_string())
        .unwrap_or_else(|| "Unknown".into())
}
fn group_label(row: &crate::downloads_triage::Row, group: Group, now: Option<i64>) -> String {
    match group {
        Group::Type => format!("{:?}", row.category),
        Group::Modified => {
            let modified = row.entry.metadata().modified;
            match now
                .and_then(|n| n.checked_sub(modified))
                .filter(|age| modified > 0 && *age >= 0)
            {
                Some(age) if age < 86_400 * 10_000_000_i64 => "Last 24 hours".into(),
                Some(age) if age < 7 * 86_400 * 10_000_000_i64 => "Last 7 days".into(),
                Some(age) if age < 30 * 86_400 * 10_000_000_i64 => "Last 30 days".into(),
                Some(_) => "Older".into(),
                None => "Age unknown".into(),
            }
        }
        Group::Size => match row.entry.metadata().byte_len {
            None => "Folders".into(),
            Some(n) if n >= 100 * 1024 * 1024 => "100 MiB or more".into(),
            Some(_) => "Under 100 MiB".into(),
        },
        Group::Origin => match &row.origin {
            Origin::Unknown => "Origin unknown".into(),
            Origin::WebHost(host) => host.clone(),
        },
    }
}
impl DownloadsTriageDialog {
    fn contents(
        &self,
        ui: &mut egui::Ui,
        actions: &mut Actions,
        runtime: &Runtime,
        scope: Option<&JumpModeScope>,
        presentation: UtilityPresentation,
    ) {
        let mut order = 0;
        macro_rules! target {
            ($ui:expr, $r:expr, $id:expr, $label:expr, $kind:expr) => {
                register_and_apply_target(scope, $ui, &$r, $id, $label, $kind, &mut order);
            };
        }
        ui.horizontal_wrapped(|ui| {
            let (button, next) = presentation_button(ui, presentation, ui.is_enabled());
            target!(
                ui,
                button,
                "presentation",
                "Change window presentation",
                TargetKind::Button
            );
            if button.clicked() {
                actions.host_request = Some(next);
            }
            let close = ui.button("Close");
            target!(ui, close, "close", "Close", TargetKind::Button);
            actions.close |= close.clicked();
        });
        egui::ScrollArea::vertical()
            .id_salt("downloads-triage-content")
            .show(ui, |ui| {
                if let Some(path) = &runtime.proposed_root {
                    ui.label(format!("Downloads location: {}", path.display()));
                } else {
                    ui.label("Downloads location is unavailable.");
                }
                if let Some(path) = &runtime.suggested_destination {
                    ui.label(format!("Suggested destination: {}", path.display()));
                }
                for message in [runtime.notice.as_deref(), self.notice.as_deref()]
                    .into_iter()
                    .flatten()
                {
                    ui.label(message);
                }
                if runtime.loading {
                    ui.label("Scanning…");
                }
                ui.horizontal_wrapped(|ui| {
                    for (label, key, intent) in [
                        ("Choose root…", "root", Intent::ChooseRoot),
                        (
                            "Use Windows Downloads",
                            "windows",
                            Intent::UseWindowsDownloads,
                        ),
                        ("Refresh", "refresh", Intent::Refresh),
                        (
                            "Set destination suggestion…",
                            "suggestion",
                            Intent::ChooseSuggestion,
                        ),
                    ] {
                        let button = ui.add_enabled(!runtime.loading, egui::Button::new(label));
                        target!(ui, button, key, label, TargetKind::Button);
                        if button.clicked() {
                            actions.intent = Some(intent);
                        }
                    }
                });
                let Some(scan) = &runtime.scan else {
                    ui.label("Choose a location and refresh to inspect files.");
                    return;
                };
                match &scan.completion {
                    DownloadsScanCompletion::Complete => {}
                    DownloadsScanCompletion::Limited => {
                        ui.label("Scan reached its limit; the list is partial.");
                    }
                    DownloadsScanCompletion::Cancelled => {
                        ui.label("Scan cancelled; the list is partial.");
                    }
                    DownloadsScanCompletion::Io(message) => {
                        ui.label(format!("Scan stopped: {message}"));
                    }
                }
                let search = ui.add(
                    egui::TextEdit::singleline(&mut actions.view.search)
                        .hint_text("Search names")
                        .char_limit(256),
                );
                target!(
                    ui,
                    search,
                    "search",
                    "Search downloads",
                    TargetKind::TextEntry
                );
                if search.changed() {
                    actions.view.page = 0;
                }
                ui.horizontal_wrapped(|ui| {
                    for (value, label, key) in [
                        (Filter::All, "All", "all"),
                        (Filter::Unreviewed, "Unreviewed", "unreviewed"),
                        (Filter::Reviewed, "Reviewed", "reviewed"),
                        (Filter::Incomplete, "Possibly incomplete", "incomplete"),
                        (Filter::Large, "Large", "large"),
                        (Filter::Installer, "Old installers", "installers"),
                        (Filter::Archive, "Archives", "archives"),
                        (Filter::SameSize, "Same-size suspects", "same-size"),
                    ] {
                        let r = ui.selectable_value(&mut actions.view.filter, value, label);
                        target!(
                            ui,
                            r,
                            &format!("filter/{key}"),
                            label,
                            TargetKind::Selectable
                        );
                        if r.changed() {
                            actions.view.page = 0;
                        }
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    for (value, label, key) in [
                        (Group::Type, "Type", "type"),
                        (Group::Modified, "Modified age", "modified"),
                        (Group::Size, "Size", "size"),
                        (Group::Origin, "Origin", "origin"),
                    ] {
                        let r = ui.selectable_value(&mut actions.view.group, value, label);
                        target!(
                            ui,
                            r,
                            &format!("group/{key}"),
                            label,
                            TargetKind::Selectable
                        );
                        if r.changed() {
                            actions.view.page = 0;
                        }
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label("Sort by");
                    for (sort, label, id) in [
                        (RowSort::Name, "Name", "name"),
                        (RowSort::Modified, "Most recently modified", "modified"),
                        (RowSort::Size, "Largest", "size"),
                    ] {
                        let response = ui.selectable_value(&mut actions.view.sort, sort, label);
                        target!(
                            ui,
                            response,
                            &format!("sort/{id}"),
                            label,
                            TargetKind::Selectable
                        );
                        if response.changed() {
                            actions.view.page = 0;
                        }
                    }
                });
                let query = actions.view.search.to_lowercase();
                let mut visible: Vec<_> = scan
                    .rows
                    .iter()
                    .filter(|row| {
                        let reviewed = runtime.reviewed.contains(&row.review_proof);
                        let matches_filter = match actions.view.filter {
                            Filter::All => true,
                            Filter::Unreviewed => !reviewed,
                            Filter::Reviewed => reviewed,
                            Filter::Incomplete => row.incomplete,
                            Filter::Large => row.large,
                            Filter::Installer => row.older_installer,
                            Filter::Archive => row.category == Category::Archive,
                            Filter::SameSize => row.suspected_same_size,
                        };
                        matches_filter
                            && row
                                .entry
                                .leaf()
                                .as_os_str()
                                .to_string_lossy()
                                .to_lowercase()
                                .contains(&query)
                    })
                    .map(|row| {
                        (
                            group_label(row, actions.view.group, runtime.now_filetime),
                            row,
                        )
                    })
                    .collect();
                visible.sort_by(|(ga, a), (gb, b)| {
                    let order = match actions.view.sort {
                        RowSort::Name => std::cmp::Ordering::Equal,
                        RowSort::Modified => b
                            .entry
                            .metadata()
                            .modified
                            .cmp(&a.entry.metadata().modified),
                        RowSort::Size => b
                            .entry
                            .metadata()
                            .byte_len
                            .cmp(&a.entry.metadata().byte_len),
                    };
                    ga.cmp(gb)
                        .then(order)
                        .then_with(|| a.entry.leaf().as_os_str().cmp(b.entry.leaf().as_os_str()))
                });
                let pages = visible.len().div_ceil(50).max(1);
                actions.view.page = actions.view.page.min(pages - 1);
                ui.horizontal(|ui| {
                    let prev =
                        ui.add_enabled(actions.view.page > 0, egui::Button::new("Previous page"));
                    target!(
                        ui,
                        prev,
                        "page/previous",
                        "Previous page",
                        TargetKind::Button
                    );
                    if prev.clicked() {
                        actions.view.page -= 1;
                    }
                    ui.label(format!("Page {} / {}", actions.view.page + 1, pages));
                    let next = ui.add_enabled(
                        actions.view.page + 1 < pages,
                        egui::Button::new("Next page"),
                    );
                    target!(ui, next, "page/next", "Next page", TargetKind::Button);
                    if next.clicked() {
                        actions.view.page += 1;
                    }
                });
                let mut prior = None;
                for (group, row) in visible.into_iter().skip(actions.view.page * 50).take(50) {
                    if prior.as_ref() != Some(&group) {
                        ui.strong(&group);
                        prior = Some(group);
                    }
                    let mut selected = actions.view.selected.contains(&row.review_proof);
                    let name = row.entry.leaf().as_os_str().to_string_lossy();
                    let response = ui
                        .push_id(row.review_proof, |ui| {
                            ui.checkbox(&mut selected, name.as_ref())
                        })
                        .inner;
                    let id = format!("row/{:x}", egui::Id::new(row.review_proof).value());
                    target!(
                        ui,
                        response,
                        &id,
                        format!("Select {name}"),
                        TargetKind::Checkbox
                    );
                    if response.changed() {
                        if selected {
                            actions.view.selected.insert(row.review_proof);
                        } else {
                            actions.view.selected.remove(&row.review_proof);
                        }
                        actions.view.recycle = None;
                    }
                    let meta = row.entry.metadata();
                    ui.label(format!(
                        "Created: {} · Modified: {} · Size: {} · {}",
                        date(meta.created),
                        date(meta.modified),
                        meta.byte_len
                            .map_or_else(|| "—".into(), |n| format!("{n} bytes")),
                        if runtime.reviewed.contains(&row.review_proof) {
                            "Reviewed"
                        } else {
                            "Not reviewed"
                        }
                    ));
                    let mut labels = Vec::new();
                    if row.incomplete {
                        labels.push("Possibly incomplete");
                    }
                    if row.large {
                        labels.push("Large (100 MiB+)");
                    }
                    if row.older_installer {
                        labels.push("Installer older than 30 days");
                    }
                    if row.suspected_same_size {
                        labels.push("Same-size suspect (not verified)");
                    }
                    if row.category == Category::Archive {
                        labels.push("Archive");
                    }
                    if runtime
                        .verified_pairs
                        .iter()
                        .any(|(a, b)| *a == row.review_proof || *b == row.review_proof)
                    {
                        labels.push("Verified duplicate content");
                    }
                    match runtime.previously_extracted.get(&row.review_proof) {
                        Some(ExtractionStatus::Full) => labels.push("Previously fully extracted"),
                        Some(ExtractionStatus::Selection) => {
                            labels.push("Previously extracted selection")
                        }
                        None => {}
                    }
                    if !labels.is_empty() {
                        ui.weak(labels.join(" · "));
                    }
                }
                let selected: Vec<_> = scan
                    .rows
                    .iter()
                    .filter(|row| actions.view.selected.contains(&row.review_proof))
                    .map(|row| row.entry.clone())
                    .collect();
                ui.separator();
                ui.label(format!("{} selected", selected.len()));
                let active = !runtime.loading && !selected.is_empty();
                let single = active && selected.len() == 1;
                ui.horizontal_wrapped(|ui| {
                    for (label, id, intent, enabled) in [
                        ("Mark reviewed", "mark", Intent::Mark(true), active),
                        ("Mark unreviewed", "unmark", Intent::Mark(false), active),
                        ("Open", "open", Intent::Open, single),
                        ("Reveal", "reveal", Intent::Reveal, single),
                        ("Inspect", "inspect", Intent::Inspect, single),
                        (
                            "Copy…",
                            "copy",
                            Intent::Transfer(TransferKind::Copy),
                            active,
                        ),
                        (
                            "Move…",
                            "move",
                            Intent::Transfer(TransferKind::Move),
                            active,
                        ),
                        (
                            "Extract ZIP…",
                            "extract",
                            Intent::ExtractZip,
                            single && selected.first().is_some_and(is_zip),
                        ),
                        (
                            "Compare…",
                            "compare",
                            Intent::Compare,
                            active && comparable(&selected),
                        ),
                        (
                            "Recycle…",
                            "recycle",
                            Intent::BeginRecycle,
                            active && selected.len() <= crate::file_transfer::MAX_RECYCLE_SELECTION,
                        ),
                    ] {
                        let b = ui.add_enabled(enabled, egui::Button::new(label));
                        target!(ui, b, id, label, TargetKind::Button);
                        if b.clicked() {
                            actions.intent = Some(intent);
                        }
                    }
                });
                ui.horizontal(|ui| {
                    let field = ui.add(
                        egui::TextEdit::singleline(&mut actions.view.rename)
                            .hint_text("New filename")
                            .char_limit(255),
                    );
                    target!(
                        ui,
                        field,
                        "rename/name",
                        "New filename",
                        TargetKind::TextEntry
                    );
                    let b = ui.add_enabled(
                        single && TransferLeaf::new(OsStr::new(&actions.view.rename)).is_ok(),
                        egui::Button::new("Rename…"),
                    );
                    target!(ui, b, "rename", "Rename selected file", TargetKind::Button);
                    if b.clicked() {
                        actions.intent = Some(Intent::Rename);
                    }
                });
                if let Some(armed) = &actions.view.recycle {
                    ui.group(|ui| {
                        ui.strong("Confirm Recycle Bin");
                        ui.label("These exact entries will be sent to the Recycle Bin:");
                        let pages = armed.entries.len().div_ceil(50).max(1);
                        actions.view.recycle_page = actions.view.recycle_page.min(pages - 1);
                        ui.horizontal(|ui| {
                            let prev = ui.add_enabled(
                                actions.view.recycle_page > 0,
                                egui::Button::new("Previous confirmation page"),
                            );
                            target!(
                                ui,
                                prev,
                                "recycle/page/previous",
                                "Previous confirmation page",
                                TargetKind::Button
                            );
                            if prev.clicked() {
                                actions.view.recycle_page -= 1;
                            }
                            ui.label(format!(
                                "{} entries · page {} / {}",
                                armed.entries.len(),
                                actions.view.recycle_page + 1,
                                pages
                            ));
                            let next = ui.add_enabled(
                                actions.view.recycle_page + 1 < pages,
                                egui::Button::new("Next confirmation page"),
                            );
                            target!(
                                ui,
                                next,
                                "recycle/page/next",
                                "Next confirmation page",
                                TargetKind::Button
                            );
                            if next.clicked() {
                                actions.view.recycle_page += 1;
                            }
                        });
                        for entry in armed
                            .entries
                            .iter()
                            .skip(actions.view.recycle_page * 50)
                            .take(50)
                        {
                            ui.label(entry.path().display().to_string());
                        }
                        let confirm = ui.button("Send selected to Recycle Bin");
                        target!(
                            ui,
                            confirm,
                            "recycle/confirm",
                            "Confirm Recycle Bin",
                            TargetKind::Button
                        );
                        if confirm.clicked() {
                            actions.intent = Some(Intent::ConfirmRecycle);
                        }
                        let cancel = ui.button("Cancel");
                        target!(
                            ui,
                            cancel,
                            "recycle/cancel",
                            "Cancel Recycle Bin",
                            TargetKind::Button
                        );
                        if cancel.clicked() {
                            actions.intent = Some(Intent::CancelRecycle);
                        }
                    });
                }
                if !scan.skipped.is_empty() {
                    ui.separator();
                    ui.label(format!("Skipped: {}", scan.skipped.len()));
                    let pages = scan.skipped.len().div_ceil(50).max(1);
                    actions.view.skipped_page = actions.view.skipped_page.min(pages - 1);
                    ui.horizontal(|ui| {
                        let prev = ui.add_enabled(
                            actions.view.skipped_page > 0,
                            egui::Button::new("Previous skipped"),
                        );
                        target!(
                            ui,
                            prev,
                            "skipped/previous",
                            "Previous skipped",
                            TargetKind::Button
                        );
                        if prev.clicked() {
                            actions.view.skipped_page -= 1;
                        }
                        ui.label(format!("{} / {}", actions.view.skipped_page + 1, pages));
                        let next = ui.add_enabled(
                            actions.view.skipped_page + 1 < pages,
                            egui::Button::new("Next skipped"),
                        );
                        target!(ui, next, "skipped/next", "Next skipped", TargetKind::Button);
                        if next.clicked() {
                            actions.view.skipped_page += 1;
                        }
                    });
                    for item in scan
                        .skipped
                        .iter()
                        .skip(actions.view.skipped_page * 50)
                        .take(50)
                    {
                        ui.label(format!("{}: {}", item.name.to_string_lossy(), item.reason));
                    }
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn downloads_triage_dialog_session_retires_selection() {
        let mut dialog = DownloadsTriageDialog::default();
        dialog.open();
        let first = dialog.session();
        dialog.view.selected.insert([1; 32]);
        dialog.close();
        dialog.open();
        assert_ne!(first, dialog.session());
        assert!(dialog.view.selected.is_empty());
        assert!(!dialog.set_notice(first, "stale".into()));
    }
}
