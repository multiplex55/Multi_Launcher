//! Explicit, non-persistent reconnect and identity-checked placement.
use crate::multi_manager::{
    bindings::{self, WindowBindingSnapshot, WorkspaceBindingSnapshot},
    identity,
    model::{MmRect, MmWorkspace},
    reconnect::{self, ReconnectDeps, ReconnectSummary},
    store::{self, CheckedWorkspaceSnapshot, WorkspaceSource},
    win::{self, EnumeratedWindow},
};
use crate::window_catalog::{WindowNativeIdentity, WindowSwitchIdentity};
use anyhow::{Context, Result};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    sync::{Arc, atomic::AtomicBool},
    path::{Path, PathBuf},
};

#[derive(Clone)]
pub struct ReconnectedWorkspace {
    pub workspace: MmWorkspace,
    pub identities: HashMap<usize, WindowSwitchIdentity>,
    pub summary: ReconnectSummary,
}

/// One checked, in-memory restore result for an exact canonical workspace row.
pub struct RestoredWorkspace {
    index: usize,
    overlay: ReconnectedWorkspace,
}
impl RestoredWorkspace {
    pub fn index(&self) -> usize { self.index }
    #[cfg(test)]
    fn workspace(&self) -> &MmWorkspace { &self.overlay.workspace }
    pub fn bound_count(&self) -> usize { self.overlay.identities.len() }
    pub fn has_identity(&self, identity: WindowNativeIdentity) -> bool {
        self.overlay.identities.values().any(|item| item.native == identity)
    }
}

/// Source-bound plan. The legacy format has no historical PID/TID, so matching
/// current metadata cannot prove same-process, same-metadata HWND lineage.
pub struct RestorePlan {
    workspaces: CheckedWorkspaceSnapshot,
    bindings_path: PathBuf,
    binding_bytes: Vec<u8>,
    rows: Vec<RestoredWorkspace>,
    pub bound: usize,
    pub skipped: usize,
}
impl RestorePlan {
    pub fn rows(&self) -> &[RestoredWorkspace] { &self.rows }
    fn is_current(&self) -> bool {
        let Ok(_workspace_permit) = crate::common::repair_watch::require_path_read(self.workspaces.path()) else { return false; };
        let Ok(_bindings_permit) = crate::common::repair_watch::require_path_read(&self.bindings_path) else { return false; };
        self.sources_match()
    }
    fn sources_match(&self) -> bool {
        store::checked_workspace_source_is_current(&self.workspaces)
            && std::fs::read(&self.bindings_path).is_ok_and(|bytes| bytes == self.binding_bytes)
    }
}

/// Load the editor's paired sources while Repair is excluded from both paths.
pub fn load_editor_sources_checked(workspace_path: &Path, bindings_path: &Path, current: impl Fn() -> bool) -> (Result<CheckedWorkspaceSnapshot>, Result<Option<Vec<WorkspaceBindingSnapshot>>>) {
    let permits = crate::common::repair_watch::require_path_read(workspace_path).and_then(|workspace| crate::common::repair_watch::require_path_read(bindings_path).map(|bindings| (workspace, bindings)));
    let _permits = match permits {
        Ok(permits) => permits,
        Err(error) => {
            let message = format!("{error:#}");
            return (Err(error), Err(anyhow::anyhow!(message)));
        }
    };
    let workspace = store::load_checked_workspaces(workspace_path, &current);
    let bindings = bindings::load_bindings_if_exists(bindings_path).and_then(|value| { anyhow::ensure!(current(), "MultiManager source read superseded"); Ok(value) });
    (workspace, bindings)
}

/// Caller holds both Repair read permits for the lifetime of the returned basis.
pub(crate) fn load_editor_source_basis(workspace_path: &Path, bindings_path: &Path, current: impl Fn() -> bool) -> Result<(CheckedWorkspaceSnapshot, Option<Vec<u8>>)> {
    let workspace = store::load_checked_workspaces(workspace_path, &current)?;
    let bytes = match std::fs::read(bindings_path) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error).with_context(|| format!("read {}", bindings_path.display())),
    };
    bindings::decode_binding_source(bindings_path, bytes.as_deref())?;
    anyhow::ensure!(current(), "MultiManager source read superseded");
    Ok((workspace, bytes))
}

/// Observes exactly one saved HWND, never enumerating the desktop.
pub trait RestoreObserver {
    fn observe(&self, hwnd: usize) -> Option<(EnumeratedWindow, WindowSwitchIdentity)>;
}
pub struct NativeRestore;
impl RestoreObserver for NativeRestore {
    fn observe(&self, hwnd: usize) -> Option<(EnumeratedWindow, WindowSwitchIdentity)> { fresh_candidate(hwnd) }
}

fn unique_workspace(workspaces: &[MmWorkspace], snapshot: &WorkspaceBindingSnapshot) -> Option<usize> {
    let id = snapshot.workspace_id.as_deref().filter(|id| !id.trim().is_empty());
    let mut id_matches = workspaces.iter().enumerate().filter(|(_, row)| id.is_some_and(|id| row.id == id));
    let id_match = id_matches.next().map(|(index, _)| index);
    if id_matches.next().is_some() { return None; }
    let index = if let Some(index) = id_match {
        // Stock ID selection remains authoritative when it is unique.
        if bindings::workspace_index_for_snapshot(workspaces, snapshot) != Some(index) { return None; }
        index
    } else {
        if snapshot.workspace_name.trim().is_empty() { return None; }
        let mut names = workspaces.iter().enumerate().filter(|(_, row)| row.name == snapshot.workspace_name);
        let (index, row) = names.next()?;
        if names.next().is_some() || (id.is_some() && !row.id.trim().is_empty()) { return None; }
        index
    };
    let row = &workspaces[index];
    if row.disabled || !row.valid { return None; }
    if !row.id.trim().is_empty() && workspaces.iter().filter(|candidate| candidate.id == row.id).count() != 1 { return None; }
    Some(index)
}

fn unique_member(workspace: &MmWorkspace, binding: &WindowBindingSnapshot) -> Option<usize> {
    let title = (!binding.captured_title.trim().is_empty()).then_some(binding.captured_title.as_str());
    let alias = binding.alias.as_deref().filter(|alias| !alias.trim().is_empty());
    if title.is_none() && alias.is_none() { return None; }
    let mut matches = workspace.windows.iter().enumerate().filter(|(_, member)| {
        title.is_none_or(|title| member.captured_title == title)
            && alias.is_none_or(|alias| member.alias == alias)
    });
    let (index, _) = matches.next()?;
    if matches.next().is_some() { return None; }
    // Unique saved identity safely supersedes a stale or default-zero ordinal.
    Some(index)
}

fn metadata_matches(window: &crate::multi_manager::model::MmWindow, live: &EnumeratedWindow) -> bool {
    identity::has_stable_metadata(window) && identity::stable_identity_matches_enumerated(window, live)
        && (window.executable.trim().is_empty() || identity::same_value(&window.executable, &live.executable))
}

fn valid_observation(hwnd: usize, window: &EnumeratedWindow, identity: &WindowSwitchIdentity) -> bool {
    let native = identity.native;
    native.hwnd == hwnd && window.hwnd == hwnd && native.pid != 0 && native.tid != 0 && native.pid != std::process::id()
        && identity.descriptor.hwnd == hwnd && identity.descriptor.pid == native.pid
        && identity.descriptor.executable.as_deref().unwrap_or("") == window.executable
        && identity.descriptor.class_name.as_deref().unwrap_or("") == window.class_name
        && identity.descriptor.process_path.as_deref().unwrap_or("") == window.process_path
}

/// Build a restore plan off the UI thread without scanning, moving or writing.
pub fn restore_checked(workspace_source: &CheckedWorkspaceSnapshot, bindings_path: &Path, current: impl Fn() -> bool, observer: &impl RestoreObserver) -> Result<RestorePlan> {
    let _workspace_permit = crate::common::repair_watch::require_path_read(workspace_source.path())?;
    let _bindings_permit = crate::common::repair_watch::require_path_read(bindings_path)?;
    anyhow::ensure!(current(), "Restore superseded");
    anyhow::ensure!(store::checked_workspace_source_is_current(workspace_source) && current(), "MultiManager workspaces changed; reload before restoring");
    // Parse the captured original bytes, not normalized/generated legacy IDs.
    let original: Vec<MmWorkspace> = match workspace_source.source() {
        WorkspaceSource::Loaded(bytes) => serde_json::from_slice(bytes).context("parse original MultiManager workspaces")?,
        WorkspaceSource::Missing | WorkspaceSource::Empty(_) => Vec::new(),
    };
    let workspaces = workspace_source.workspaces();
    anyhow::ensure!(original.len() == workspaces.len(), "MultiManager workspace receipt is inconsistent");
    let binding_bytes = std::fs::read(bindings_path).with_context(|| format!("read MultiManager bindings {}", bindings_path.display()))?;
    let snapshots: Vec<WorkspaceBindingSnapshot> = serde_json::from_slice(&binding_bytes).with_context(|| format!("parse MultiManager bindings {}", bindings_path.display()))?;
    let mut hwnd_counts = HashMap::<usize, usize>::new();
    for binding in snapshots.iter().flat_map(|snapshot| &snapshot.windows) {
        if binding.hwnd != 0 { *hwnd_counts.entry(binding.hwnd).or_default() += 1; }
    }
    let mut candidates = Vec::<(usize, usize, &WindowBindingSnapshot)>::new();
    let mut skipped = 0;
    for snapshot in &snapshots {
        let Some(row) = unique_workspace(&original, snapshot) else { skipped += snapshot.windows.len(); continue; };
        for binding in &snapshot.windows {
            let Some(member) = unique_member(&original[row], binding) else { skipped += 1; continue; };
            candidates.push((row, member, binding));
        }
    }
    let mut member_counts = HashMap::<(usize, usize), usize>::new();
    for (row, member, _) in &candidates {
        *member_counts.entry((*row, *member)).or_default() += 1;
    }
    let mut rows = HashMap::<usize, ReconnectedWorkspace>::new();
    let mut bound = 0;
    for (row, member, binding) in candidates {
        anyhow::ensure!(current(), "Restore superseded");
        let workspace = &workspaces[row];
        let target = &workspace.windows[member];
        let saved = bindings::window_from_snapshot(binding);
        if target.disabled || binding.hwnd == 0 || member_counts[&(row, member)] != 1
            || hwnd_counts.get(&binding.hwnd).copied() != Some(1)
            || !identity::has_stable_metadata(&saved) || !identity::has_stable_metadata(target) { skipped += 1; continue; }
        let Some((observed, identity)) = observer.observe(binding.hwnd) else { skipped += 1; continue; };
        if !valid_observation(binding.hwnd, &observed, &identity) || !metadata_matches(&saved, &observed) || !metadata_matches(target, &observed) { skipped += 1; continue; }
        let overlay = rows.entry(row).or_insert_with(|| {
            let mut workspace = workspace.clone();
            for window in &mut workspace.windows { window.mark_missing(); window.live_title.clear(); }
            ReconnectedWorkspace { workspace, identities: HashMap::new(), summary: ReconnectSummary::default() }
        });
        overlay.workspace.windows[member].mark_bound(binding.hwnd);
        overlay.workspace.windows[member].live_title = observed.title;
        overlay.identities.insert(member, identity);
        bound += 1;
    }
    let mut rows: Vec<_> = rows.into_iter().map(|(index, overlay)| RestoredWorkspace { index, overlay }).collect();
    rows.sort_by_key(|row| row.index);
    let plan = RestorePlan { workspaces: workspace_source.clone(), bindings_path: bindings_path.to_path_buf(), binding_bytes, rows, bound, skipped };
    anyhow::ensure!(current() && plan.sources_match() && current(), "Restore sources changed or request superseded");
    Ok(plan)
}

/// Apply only an exact Restore-derived row, checking both sources throughout placement.
pub fn place_restored_checked(plan: Arc<RestorePlan>, row: usize, issuer: WindowNativeIdentity, destination: Destination,
    current: Arc<dyn Fn() -> bool + Send + Sync>, backend: &mut impl PlacementBackend) -> PlacementReport {
    let Some(restored) = plan.rows.iter().find(|candidate| candidate.index == row) else {
        return PlacementReport { error: Some("Restored workspace is unavailable".into()), ..Default::default() };
    };
    let checked_plan = plan.clone();
    let checked: Arc<dyn Fn() -> bool + Send + Sync> = Arc::new(move || current() && checked_plan.is_current() && current());
    if !checked() { return PlacementReport { error: Some("Restore sources changed or request superseded".into()), ..Default::default() }; }
    place_checked(&restored.overlay, issuer, destination, checked, backend)
}

/// Enumeration is injected so a real failure cannot be confused with an empty desktop.
pub trait ReconnectBackend {
    fn enumerate(&self) -> Result<Vec<(EnumeratedWindow, WindowSwitchIdentity)>>;
    fn enumerate_if_current(&self, current: &dyn Fn() -> bool) -> Result<Vec<(EnumeratedWindow, WindowSwitchIdentity)>> {
        anyhow::ensure!(current(), "Reconnect superseded");
        self.enumerate()
    }
}

/// A catalog reconnect owns its original workspace source and row ordinals.
pub struct ReconnectCatalogPlan {
    source: CheckedWorkspaceSnapshot,
    rows: Vec<ReconnectedCatalogRow>,
    pub skipped: usize,
    pub summary: ReconnectSummary,
    pub outcome: ReconnectCatalogOutcome,
}

pub struct ReconnectedCatalogRow {
    pub index: usize,
    pub workspace: MmWorkspace,
    pub identities: HashMap<usize, WindowSwitchIdentity>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReconnectCatalogOutcome { Complete, Partial, NoMatch }

/// Load only the committed workspace store and reconnect it without writing.
pub fn reconnect_catalog_from_path_checked(
    path: &Path,
    current: impl Fn() -> bool,
    backend: &impl ReconnectBackend,
) -> Result<ReconnectCatalogPlan> {
    let _permit = crate::common::repair_watch::require_path_read(path)?;
    let source = store::load_checked_workspaces(path, &current)?;
    reconnect_catalog_checked_inner(&source, current, backend)
}

impl ReconnectCatalogPlan {
    pub fn rows(&self) -> &[ReconnectedCatalogRow] { &self.rows }
    /// Exact raw source identity also supports legacy rows with synthesized IDs.
    pub fn matches_raw_source(&self, snapshot: &CheckedWorkspaceSnapshot) -> bool {
        self.source.path() == snapshot.path() && self.source.source() == snapshot.source()
    }
    /// Compare the retained committed receipt without reading the filesystem.
    pub fn matches_snapshot(&self, snapshot: &CheckedWorkspaceSnapshot) -> bool {
        self.source.path() == snapshot.path() && self.source.source() == snapshot.source() && self.source.workspaces() == snapshot.workspaces()
    }
    pub fn source_is_current(&self) -> bool {
        let Ok(_permit) = crate::common::repair_watch::require_path_read(self.source.path()) else { return false; };
        store::checked_workspace_source_is_current(&self.source)
    }
}

/// Match all eligible committed rows against one fresh window enumeration.
/// Saved HWNDs are never admitted as live identity, and no state is persisted.
pub fn reconnect_catalog_checked(
    source: &CheckedWorkspaceSnapshot,
    current: impl Fn() -> bool,
    backend: &impl ReconnectBackend,
) -> Result<ReconnectCatalogPlan> {
    let _permit = crate::common::repair_watch::require_path_read(source.path())?;
    reconnect_catalog_checked_inner(source, current, backend)
}

fn enumeration_source_current(
    source_current: impl FnOnce() -> bool,
    current: impl FnOnce() -> bool,
) -> bool {
    source_current() && current()
}

fn reconnect_catalog_checked_inner(
    source: &CheckedWorkspaceSnapshot,
    current: impl Fn() -> bool,
    backend: &impl ReconnectBackend,
) -> Result<ReconnectCatalogPlan> {
    let source_current = || current() && store::checked_workspace_source_is_current(source);
    anyhow::ensure!(source_current(), "Reconnect source changed or request superseded");
    let original = source.workspaces();
    let mut id_counts = HashMap::new();
    for row in original { *id_counts.entry(row.id.as_str()).or_insert(0usize) += 1; }
    let mut indices = Vec::new();
    let mut shadow = Vec::new();
    for (index, row) in original.iter().enumerate() {
        if row.disabled || !row.valid || row.id.trim().is_empty()
            || id_counts.get(row.id.as_str()) != Some(&1) { continue; }
        let mut row = row.clone();
        for window in &mut row.windows { window.hwnd = 0; window.binding_verified = false; }
        indices.push(index);
        shadow.push(row);
    }
    let skipped = original.len() - indices.len();
    let snapshot = reconnect::collect_reconnect_snapshot(&shadow);
    let observations = if snapshot.windows.iter().any(|window| !window.fingerprint.disabled) {
        backend
            .enumerate_if_current(&|| enumeration_source_current(&source_current, &current))
            .context("Could not enumerate windows")?
    } else { Vec::new() };
    anyhow::ensure!(source_current(), "Reconnect source changed or request superseded");
    let mut captured = HashMap::new();
    let mut windows = Vec::new();
    for (window, identity) in observations {
        if identity.native.hwnd != window.hwnd { continue; }
        anyhow::ensure!(!captured.contains_key(&window.hwnd), "Window enumeration contained duplicate handles");
        captured.insert(window.hwnd, identity);
        windows.push(window);
    }
    let (_, patches) = reconnect::build_reconnect_patches(&snapshot, ReconnectDeps {
        is_window: &|_| false, query_identity: &|_| None,
        enumerate_top_level_windows: &|| windows.clone(),
    });
    anyhow::ensure!(source_current(), "Reconnect source changed or request superseded");
    let mut rows = Vec::with_capacity(shadow.len());
    let summary = reconnect::apply_reconnect_patches(&mut shadow, &patches);
    let mut assigned = HashSet::new();
    for (index, row) in indices.into_iter().zip(shadow.iter_mut()) {
        let mut identities = HashMap::new();
        for (window_index, window) in row.windows.iter().enumerate() {
            if window.disabled || !window.binding_verified || window.hwnd == 0 { continue; }
            let Some(identity) = captured.get(&window.hwnd) else { continue; };
            anyhow::ensure!(assigned.insert(identity.native.hwnd), "Reconnect produced duplicate window handles");
            identities.insert(window_index, identity.clone());
        }
        rows.push(ReconnectedCatalogRow { index, workspace: row.clone(), identities });
    }
    anyhow::ensure!(source_current(), "Reconnect source changed or request superseded");
    let unresolved = summary.missing + summary.ambiguous + summary.metadata_mismatch + summary.stale_results_discarded;
    let outcome = if summary.reconnected + summary.already_valid == 0 { ReconnectCatalogOutcome::NoMatch }
        else if unresolved != 0 || skipped != 0 { ReconnectCatalogOutcome::Partial }
        else { ReconnectCatalogOutcome::Complete };
    Ok(ReconnectCatalogPlan { source: source.clone(), rows, skipped, summary, outcome })
}

pub struct NativeReconnect;

#[cfg(any(test, windows))]
fn observation_is_current(
    before: &crate::window_catalog::WindowDescriptor,
    after: &crate::window_catalog::WindowDescriptor,
    first: &WindowSwitchIdentity,
    second: &WindowSwitchIdentity,
    tool: bool,
    launcher_pid: u32,
) -> bool {
    !tool
        && first.native.pid != launcher_pid
        && !before.title.trim().is_empty()
        && before == after
        && first == second
}
impl ReconnectBackend for NativeReconnect {
    fn enumerate(&self) -> Result<Vec<(EnumeratedWindow, WindowSwitchIdentity)>> {
        self.enumerate_if_current(&|| true)
    }
    fn enumerate_if_current(&self, current: &dyn Fn() -> bool) -> Result<Vec<(EnumeratedWindow, WindowSwitchIdentity)>> {
        static ENUMERATION: std::sync::Mutex<()> = std::sync::Mutex::new(());
        with_enumeration_lane(&ENUMERATION, current, || {
        let mut checked = Vec::new();
        for candidate in win::enumerate_top_level_windows()? {
            if let Some(observation) = fresh_candidate(candidate.hwnd) {
                checked.push(observation);
            }
        }
        Ok(checked)
        })
    }
}

fn with_enumeration_lane<T>(lane: &std::sync::Mutex<()>, current: &dyn Fn() -> bool, enumerate: impl FnOnce() -> Result<T>) -> Result<T> {
    let _guard = lane.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    anyhow::ensure!(current(), "Reconnect superseded");
    enumerate()
}

#[cfg(windows)]
fn native_extended_style(hwnd: windows::Win32::Foundation::HWND) -> Result<u32> {
    use windows::Win32::{
        Foundation::{GetLastError, SetLastError, WIN32_ERROR},
        UI::WindowsAndMessaging::{GWL_EXSTYLE, GetWindowLongPtrW},
    };
    unsafe {
        SetLastError(WIN32_ERROR(0));
    }
    let style = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } as u32;
    let error = unsafe { GetLastError() };
    anyhow::ensure!(
        style != 0 || error == WIN32_ERROR(0),
        "Could not read window style: {error:?}"
    );
    Ok(style)
}

#[cfg(windows)]
fn fresh_candidate(hwnd: usize) -> Option<(EnumeratedWindow, WindowSwitchIdentity)> {
    use windows::Win32::{Foundation::HWND, UI::WindowsAndMessaging::WS_EX_TOOLWINDOW};
    let (before, identity) = crate::window_catalog::current_switch_window(hwnd)?;
    let rect = win::window_rect(hwnd)?;
    let style = native_extended_style(HWND(hwnd as *mut _)).ok()?;
    let (after, final_identity) = crate::window_catalog::current_switch_window(hwnd)?;
    if !observation_is_current(
        &before,
        &after,
        &identity,
        &final_identity,
        style & WS_EX_TOOLWINDOW.0 != 0,
        std::process::id(),
    ) {
        return None;
    }
    Some((
        EnumeratedWindow {
            hwnd,
            title: after.title,
            executable: after.executable.unwrap_or_default(),
            class_name: after.class_name.unwrap_or_default(),
            process_path: after.process_path.unwrap_or_default(),
            rect,
        },
        identity,
    ))
}

#[cfg(not(windows))]
fn fresh_candidate(_: usize) -> Option<(EnumeratedWindow, WindowSwitchIdentity)> {
    None
}

pub fn reconnect_checked(
    workspaces: &[MmWorkspace],
    workspace_index: usize,
    current: impl Fn() -> bool,
    backend: &impl ReconnectBackend,
) -> Result<ReconnectedWorkspace> {
    anyhow::ensure!(current(), "Reconnect superseded");
    let selected = workspaces
        .get(workspace_index)
        .context("Workspace no longer exists")?;
    anyhow::ensure!(
        !selected.id.is_empty()
            && workspaces
                .iter()
                .filter(|row| row.id == selected.id)
                .count()
                == 1,
        "Workspace ID is ambiguous; fix duplicate IDs before reconnecting"
    );
    anyhow::ensure!(
        !selected.disabled && selected.valid,
        "Workspace is disabled or invalid"
    );
    let mut shadow = vec![selected.clone()];
    // Saved HWNDs and historical binding flags never become placement authority.
    for window in &mut shadow[0].windows {
        window.hwnd = 0;
        window.binding_verified = false;
    }
    let snapshot = reconnect::collect_reconnect_snapshot(&shadow);
    let captured = RefCell::new(HashMap::<usize, WindowSwitchIdentity>::new());
    let error = RefCell::new(None::<String>);
    let duplicate = RefCell::new(false);
    let enumerate = || match backend.enumerate_if_current(&current) {
        Ok(rows) => rows
            .into_iter()
            .filter_map(|(window, identity)| {
                if identity.native.hwnd != window.hwnd {
                    return None;
                }
                if captured.borrow().contains_key(&window.hwnd) {
                    *duplicate.borrow_mut() = true;
                    return None;
                }
                captured.borrow_mut().insert(window.hwnd, identity);
                Some(window)
            })
            .collect(),
        Err(failure) => {
            *error.borrow_mut() = Some(format!("{failure:#}"));
            Vec::new()
        }
    };
    let (_worker, patches) = reconnect::build_reconnect_patches(
        &snapshot,
        ReconnectDeps {
            is_window: &|_| false,
            query_identity: &|_| None,
            enumerate_top_level_windows: &enumerate,
        },
    );
    if let Some(error) = error.into_inner() {
        anyhow::bail!("Could not enumerate windows: {error}");
    }
    anyhow::ensure!(
        !duplicate.into_inner(),
        "Window enumeration contained duplicate handles"
    );
    anyhow::ensure!(current(), "Reconnect superseded");
    let summary = reconnect::apply_reconnect_patches(&mut shadow, &patches);
    let captured = captured.into_inner();
    let mut identities = HashMap::new();
    let mut assigned = HashSet::new();
    for (index, window) in shadow[0].windows.iter().enumerate() {
        if window.disabled || !window.binding_verified || window.hwnd == 0 {
            continue;
        }
        let Some(identity) = captured.get(&window.hwnd) else {
            continue;
        };
        anyhow::ensure!(
            assigned.insert(identity.native.hwnd),
            "Reconnect produced duplicate window handles"
        );
        identities.insert(index, identity.clone());
    }
    Ok(ReconnectedWorkspace {
        workspace: shadow.remove(0),
        identities,
        summary,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Destination {
    Home,
    Target,
}

#[derive(Debug, Default)]
pub struct PlacementReport {
    pub moved: usize,
    pub skipped: usize,
    pub error: Option<String>,
}

#[derive(Debug, Default)]
pub struct PlacementOutcome {
    pub cleanup_warning: Option<String>,
}

pub trait PlacementBackend {
    fn workspace_on_current_desktop(
        &mut self,
        required: Option<&crate::virtual_desktop::VirtualDesktopId>,
        current: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Result<bool>;
    fn on_current_desktop(
        &mut self,
        target: &WindowSwitchIdentity,
        current: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Result<bool>;
    fn activate(
        &mut self,
        issuer: WindowNativeIdentity,
        target: &WindowSwitchIdentity,
        previous: Option<&WindowSwitchIdentity>,
        required: Option<&crate::virtual_desktop::VirtualDesktopId>,
        current: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Result<()>;
    fn place(
        &mut self,
        issuer: WindowNativeIdentity,
        target: &WindowSwitchIdentity,
        rect: MmRect,
        required: Option<&crate::virtual_desktop::VirtualDesktopId>,
        current: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Result<PlacementOutcome>;
}

/// Applies captured targets in recorded order. The only accepted foreground
/// predecessor after a successful placement is the exact prior target.
pub fn place_checked(
    overlay: &ReconnectedWorkspace,
    issuer: WindowNativeIdentity,
    destination: Destination,
    current: Arc<dyn Fn() -> bool + Send + Sync>,
    backend: &mut impl PlacementBackend,
) -> PlacementReport {
    place_targets(&overlay.workspace, &overlay.identities, issuer, destination, current, backend)
}

pub fn place_catalog_checked(plan: Arc<ReconnectCatalogPlan>, index: usize, issuer: WindowNativeIdentity, destination: Destination, current: Arc<dyn Fn() -> bool + Send + Sync>, backend: &mut impl PlacementBackend) -> PlacementReport {
    let Some(row) = plan.rows.iter().find(|row| row.index == index) else { return PlacementReport { error: Some("Reconnected workspace is unavailable".into()), ..Default::default() }; };
    let source = plan.clone();
    let checked: Arc<dyn Fn() -> bool + Send + Sync> = Arc::new(move || current() && source.source_is_current() && current());
    place_targets(&row.workspace, &row.identities, issuer, destination, checked, backend)
}

fn place_targets(workspace: &MmWorkspace, identities: &HashMap<usize, WindowSwitchIdentity>, issuer: WindowNativeIdentity, destination: Destination, current: Arc<dyn Fn() -> bool + Send + Sync>, backend: &mut impl PlacementBackend) -> PlacementReport {
    let mut report = PlacementReport::default();
    let mut previous = None::<WindowSwitchIdentity>;
    let required = if destination == Destination::Target {
        workspace
            .virtual_desktop
            .as_ref()
            .map(|binding| &binding.id)
    } else {
        None
    };
    match backend.workspace_on_current_desktop(required, current.clone()) {
        Ok(true) => {}
        Ok(false) => {
            report.skipped = identities.len();
            return report;
        }
        Err(error) => {
            report.error = Some(format!("{error:#}"));
            return report;
        }
    }
    let mut seen = HashSet::new();
    if identities
        .values()
        .any(|target| !seen.insert(target.native.hwnd))
    {
        report.error = Some("Duplicate window handle in reconnect result".into());
        return report;
    }
    seen.clear();
    for (index, window) in workspace.windows.iter().enumerate() {
        if window.disabled {
            continue;
        }
        let Some(target) = identities.get(&index) else {
            report.skipped += 1;
            continue;
        };
        if !seen.insert(target.native.hwnd) {
            report.error = Some("Duplicate window handle in reconnect result".into());
            break;
        }
        let rect = match destination {
            Destination::Home => window.home_rect.or(workspace.home_rect),
            Destination::Target => window.target_rect.or(workspace.target_rect),
        };
        let Some(rect) = rect.filter(|rect| rect.w > 0 && rect.h > 0) else {
            report.skipped += 1;
            continue;
        };
        if !(current)() {
            report.error = Some("Placement superseded".into());
            break;
        }
        match backend.on_current_desktop(target, current.clone()) {
            Ok(true) => {}
            Ok(false) => {
                report.skipped += 1;
                continue;
            }
            Err(error) => {
                report.error = Some(format!("{error:#}"));
                break;
            }
        }
        if let Err(error) =
            backend.activate(issuer, target, previous.as_ref(), required, current.clone())
        {
            report.error = Some(format!("{error:#}"));
            break;
        }
        match backend.place(issuer, target, rect, required, current.clone()) {
            Ok(outcome) => {
                report.moved += 1;
                if let Some(warning) = outcome.cleanup_warning {
                    report.error = Some(warning);
                    break;
                }
            }
            Err(error) => {
                report.error = Some(format!("{error:#}"));
                break;
            }
        }
        previous = Some(target.clone());
    }
    report
}

pub struct NativePlacement {
    pub effect_started: Arc<AtomicBool>,
}
impl PlacementBackend for NativePlacement {
    fn workspace_on_current_desktop(
        &mut self,
        required: Option<&crate::virtual_desktop::VirtualDesktopId>,
        current: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Result<bool> {
        native_workspace_on_current_desktop(required, current)
    }
    fn on_current_desktop(
        &mut self,
        target: &WindowSwitchIdentity,
        current: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Result<bool> {
        native_target_on_current_desktop(target, current)
    }
    fn activate(
        &mut self,
        issuer: WindowNativeIdentity,
        target: &WindowSwitchIdentity,
        previous: Option<&WindowSwitchIdentity>,
        required: Option<&crate::virtual_desktop::VirtualDesktopId>,
        current: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Result<()> {
        crate::window_activation::activate_checked_window_switch_after(
            crate::window_activation::CheckedWindowSwitch {
                initiating: issuer,
                selected: target.clone(),
                is_current: current,
            },
            previous.cloned(),
            required.cloned(),
            self.effect_started.clone(),
        )
        .map_err(anyhow::Error::new)
    }
    fn place(
        &mut self,
        issuer: WindowNativeIdentity,
        target: &WindowSwitchIdentity,
        rect: MmRect,
        required: Option<&crate::virtual_desktop::VirtualDesktopId>,
        current: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Result<PlacementOutcome> {
        native_checked_geometry(issuer, target, rect, required, current)
    }
}

#[cfg(windows)]
fn native_workspace_on_current_desktop(
    required: Option<&crate::virtual_desktop::VirtualDesktopId>,
    current: Arc<dyn Fn() -> bool + Send + Sync>,
) -> Result<bool> {
    if let Some(required) = required {
        anyhow::ensure!(current(), "Placement superseded");
        let desktop = crate::virtual_desktop::VirtualDesktopService
            .current_for_launcher_restore()
            .map_err(anyhow::Error::new)?;
        anyhow::ensure!(current(), "Placement superseded");
        Ok(required == &desktop)
    } else {
        Ok(true)
    }
}

#[cfg(not(windows))]
fn native_workspace_on_current_desktop(
    _: Option<&crate::virtual_desktop::VirtualDesktopId>,
    _: Arc<dyn Fn() -> bool + Send + Sync>,
) -> Result<bool> {
    anyhow::bail!("MultiManager window movement is available only on Windows")
}

#[cfg(any(test, windows))]
fn identity_only_cleanup(
    identity_current: impl FnOnce() -> bool,
    demote: impl FnOnce() -> Result<()>,
) -> Result<bool> {
    if identity_current() {
        demote()?;
        return Ok(true);
    }
    Ok(false)
}

#[cfg(any(test, windows))]
fn cleanup_after_move(
    was_topmost: bool,
    identity_current: impl FnOnce() -> bool,
    demote: impl FnOnce() -> Result<()>,
) -> Option<String> {
    if was_topmost {
        return None;
    }
    match identity_only_cleanup(identity_current, demote) {
        Ok(true) => None,
        Ok(false) => {
            Some("Window moved, but its identity changed before temporary topmost cleanup.".into())
        }
        Err(error) => Some(format!(
            "Window moved, but temporary topmost cleanup failed: {error:#}"
        )),
    }
}

#[cfg(windows)]
fn native_target_on_current_desktop(
    target: &WindowSwitchIdentity,
    current: Arc<dyn Fn() -> bool + Send + Sync>,
) -> Result<bool> {
    use windows::Win32::Foundation::HWND;
    anyhow::ensure!(
        current()
            && crate::window_catalog::current_switch_identity(target.native.hwnd).as_ref()
                == Some(target),
        "Window identity changed or placement superseded"
    );
    let result = crate::virtual_desktop::VirtualDesktopService
        .is_window_on_current_desktop(HWND(target.native.hwnd as *mut _))
        .map_err(anyhow::Error::new)?;
    anyhow::ensure!(current(), "Placement superseded");
    Ok(result)
}

#[cfg(not(windows))]
fn native_target_on_current_desktop(
    _: &WindowSwitchIdentity,
    _: Arc<dyn Fn() -> bool + Send + Sync>,
) -> Result<bool> {
    anyhow::bail!("MultiManager window movement is available only on Windows")
}

#[cfg(windows)]
fn native_checked_geometry(
    issuer: WindowNativeIdentity,
    target: &WindowSwitchIdentity,
    rect: MmRect,
    required: Option<&crate::virtual_desktop::VirtualDesktopId>,
    current: Arc<dyn Fn() -> bool + Send + Sync>,
) -> Result<PlacementOutcome> {
    use windows::Win32::{
        Foundation::HWND,
        UI::WindowsAndMessaging::{
            GetForegroundWindow, HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE,
            SWP_NOSIZE, SWP_SHOWWINDOW, SetWindowPos, WS_EX_TOPMOST,
        },
    };
    let hwnd = HWND(target.native.hwnd as *mut _);
    anyhow::ensure!(
        rect.w > 0 && rect.h > 0 && current(),
        "Placement superseded"
    );
    anyhow::ensure!(
        crate::window_catalog::current_switch_identity(target.native.hwnd).as_ref() == Some(target),
        "Window identity changed"
    );
    anyhow::ensure!(
        crate::virtual_desktop::VirtualDesktopService
            .is_window_on_current_desktop(hwnd)
            .map_err(anyhow::Error::new)?,
        "Window is on another virtual desktop"
    );
    anyhow::ensure!(current(), "Placement superseded");
    let style = native_extended_style(hwnd)?;
    let was_topmost = style & WS_EX_TOPMOST.0 != 0;
    // The authority callback can read disk; sample all native proofs after it.
    if let Some(required) = required {
        let actual = crate::virtual_desktop::VirtualDesktopService
            .current_for_launcher_restore()
            .map_err(anyhow::Error::new)?;
        anyhow::ensure!(&actual == required, "Workspace target desktop changed");
    }
    anyhow::ensure!(
        crate::window_catalog::native_window_identity(issuer.hwnd) == Some(issuer)
            && crate::virtual_desktop::VirtualDesktopService
                .is_window_on_current_desktop(HWND(issuer.hwnd as *mut _))
                .map_err(anyhow::Error::new)?
            && crate::window_catalog::current_switch_identity(target.native.hwnd).as_ref()
                == Some(target)
            && crate::virtual_desktop::VirtualDesktopService
                .is_window_on_current_desktop(hwnd)
                .map_err(anyhow::Error::new)?
            && crate::window_catalog::native_window_identity(
                unsafe { GetForegroundWindow() }.0 as usize
            ) == Some(target.native),
        "Window identity, desktop or foreground changed"
    );
    unsafe {
        SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            rect.x,
            rect.y,
            rect.w,
            rect.h,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        )
    }
    .context("place window")?;
    // Cleanup is identity-only: revocation must never strand a valid window topmost.
    let cleanup_warning = cleanup_after_move(
        was_topmost,
        || {
            crate::window_catalog::native_window_identity(target.native.hwnd) == Some(target.native)
                && crate::window_catalog::describe_window(target.native.hwnd)
                    .is_some_and(|descriptor| target.descriptor.matches(&descriptor))
                && crate::window_catalog::native_window_identity(target.native.hwnd)
                    == Some(target.native)
        },
        || {
            unsafe {
                SetWindowPos(
                    hwnd,
                    HWND_NOTOPMOST,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                )
            }
            .context("remove temporary topmost")
        },
    );
    Ok(PlacementOutcome { cleanup_warning })
}

#[cfg(not(windows))]
fn native_checked_geometry(
    _: WindowNativeIdentity,
    _: &WindowSwitchIdentity,
    _: MmRect,
    _: Option<&crate::virtual_desktop::VirtualDesktopId>,
    _: Arc<dyn Fn() -> bool + Send + Sync>,
) -> Result<PlacementOutcome> {
    anyhow::bail!("MultiManager window movement is available only on Windows")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::multi_manager::model::MmWindow;
    use crate::window_catalog::WindowTargetIdentity;

    fn identity(hwnd: usize, pid: u32) -> WindowSwitchIdentity {
        WindowSwitchIdentity {
            native: WindowNativeIdentity {
                hwnd,
                pid,
                tid: hwnd as u32,
            },
            descriptor: WindowTargetIdentity {
                hwnd,
                pid,
                executable: Some("editor.exe".into()),
                process_path: Some("C:\\editor.exe".into()),
                class_name: Some("Editor".into()),
            },
        }
    }
    fn candidate(hwnd: usize) -> EnumeratedWindow {
        EnumeratedWindow {
            hwnd,
            title: "Editor".into(),
            executable: "editor.exe".into(),
            process_path: "C:\\editor.exe".into(),
            class_name: "Editor".into(),
            rect: MmRect {
                x: 0,
                y: 0,
                w: 100,
                h: 100,
            },
        }
    }
    #[test]
    fn candidate_observation_rejects_reused_hidden_class_and_self() {
        let id = identity(10, 22);
        let before = crate::window_catalog::WindowDescriptor {
            title: "Editor".into(),
            hwnd: 10,
            pid: 22,
            executable: Some("editor.exe".into()),
            process_path: Some("C:\\editor.exe".into()),
            class_name: Some("Editor".into()),
        };
        assert!(observation_is_current(&before, &before, &id, &id, false, 1));
        assert!(!observation_is_current(&before, &before, &id, &id, true, 1));
        assert!(!observation_is_current(
            &before, &before, &id, &id, false, 22
        ));
        let mut changed = before.clone();
        changed.title = "Other".into();
        assert!(!observation_is_current(
            &before, &changed, &id, &id, false, 1
        ));
        let mut reused = id.clone();
        reused.native.tid += 1;
        assert!(!observation_is_current(
            &before, &before, &id, &reused, false, 1
        ));
    }
    struct FakeReconnect {
        fail: bool,
        rows: Vec<(EnumeratedWindow, WindowSwitchIdentity)>,
    }
    impl ReconnectBackend for FakeReconnect {
        fn enumerate(&self) -> Result<Vec<(EnumeratedWindow, WindowSwitchIdentity)>> {
            if self.fail {
                anyhow::bail!("enumeration failed");
            }
            Ok(self.rows.clone())
        }
    }
    fn workspace() -> MmWorkspace {
        MmWorkspace {
            id: "one".into(),
            name: "Workspace".into(),
            windows: vec![MmWindow {
                captured_title: "Editor".into(),
                executable: "editor.exe".into(),
                process_path: "C:\\editor.exe".into(),
                class_name: "Editor".into(),
                hwnd: 999,
                binding_verified: true,
                ..Default::default()
            }],
            ..Default::default()
        }
    }
    struct FakeRestore {
        rows: HashMap<usize, (EnumeratedWindow, WindowSwitchIdentity)>,
        replace: Option<PathBuf>,
    }
    impl RestoreObserver for FakeRestore {
        fn observe(&self, hwnd: usize) -> Option<(EnumeratedWindow, WindowSwitchIdentity)> {
            if let Some(path) = &self.replace { std::fs::write(path, b"[]").unwrap(); }
            self.rows.get(&hwnd).cloned()
        }
    }
    fn saved_binding(workspace: &str, hwnd: usize) -> WorkspaceBindingSnapshot {
        WorkspaceBindingSnapshot {
            workspace_id: Some(workspace.into()), workspace_name: workspace.into(),
            windows: vec![WindowBindingSnapshot { window_id: 0, window_index: 0, captured_title: "Editor".into(), alias: None, hwnd,
                executable: Some("editor.exe".into()), class_name: Some("Editor".into()), process_path: Some("C:\\editor.exe".into()) }],
        }
    }
    fn restore_for_test(path: &Path, bindings: &Path, current: impl Fn() -> bool, observer: &impl RestoreObserver) -> Result<RestorePlan> {
        let snapshot = store::load_checked_workspaces(path, &current)?;
        restore_checked(&snapshot, bindings, current, observer)
    }
    #[test]
    fn checked_restore_binds_only_fresh_matching_rows_and_never_writes() {
        let dir = tempfile::tempdir().unwrap();
        let workspace_path = dir.path().join("workspaces.json"); let bindings_path = dir.path().join("bindings.json");
        let first = workspace(); let mut second = workspace(); second.id = "two".into(); second.name = "Second".into();
        let raw_workspace = serde_json::to_vec(&[first, second]).unwrap();
        let raw_bindings = serde_json::to_vec(&[saved_binding("one", 10), saved_binding("two", 11)]).unwrap();
        std::fs::write(&workspace_path, &raw_workspace).unwrap(); std::fs::write(&bindings_path, &raw_bindings).unwrap();
        let mut mismatch = candidate(11); mismatch.process_path = "C:\\other.exe".into();
        let other_pid = std::process::id().wrapping_add(1).max(1);
        let mut mismatched_identity = identity(11, other_pid); mismatched_identity.descriptor.process_path = Some(mismatch.process_path.clone());
        let observer = FakeRestore { rows: HashMap::from([(10, (candidate(10), identity(10, other_pid))), (11, (mismatch, mismatched_identity))]), replace: None };
        let plan = restore_for_test(&workspace_path, &bindings_path, || true, &observer).unwrap();
        assert_eq!((plan.bound, plan.skipped), (1, 1));
        assert_eq!(plan.rows().len(), 1); assert_eq!(plan.rows()[0].index(), 0);
        assert_eq!(plan.rows()[0].workspace().windows[0].hwnd, 10);
        assert_eq!(std::fs::read(&workspace_path).unwrap(), raw_workspace);
        assert_eq!(std::fs::read(&bindings_path).unwrap(), raw_bindings);
        assert!(plan.is_current());
        std::fs::write(&workspace_path, b"[]").unwrap();
        assert!(!plan.is_current());
    }
    #[test]
    fn checked_restore_rejects_duplicate_targets_title_only_and_late_source_change() {
        let dir = tempfile::tempdir().unwrap();
        let workspace_path = dir.path().join("workspaces.json"); let bindings_path = dir.path().join("bindings.json");
        std::fs::write(&workspace_path, serde_json::to_vec(&[workspace()]).unwrap()).unwrap();
        let mut first = saved_binding("one", 10);
        first.windows.push(first.windows[0].clone());
        std::fs::write(&bindings_path, serde_json::to_vec(&[first]).unwrap()).unwrap();
        let observer = FakeRestore { rows: HashMap::from([(10, (candidate(10), identity(10, std::process::id().wrapping_add(1).max(1))))]), replace: None };
        let plan = restore_for_test(&workspace_path, &bindings_path, || true, &observer).unwrap();
        assert_eq!((plan.bound, plan.skipped), (0, 2));
        let mut title_only = saved_binding("one", 10);
        title_only.windows[0].executable = None; title_only.windows[0].class_name = None; title_only.windows[0].process_path = None;
        std::fs::write(&bindings_path, serde_json::to_vec(&[title_only]).unwrap()).unwrap();
        assert_eq!(restore_for_test(&workspace_path, &bindings_path, || true, &observer).unwrap().bound, 0);
        std::fs::write(&bindings_path, serde_json::to_vec(&[saved_binding("one", 10)]).unwrap()).unwrap();
        let mutating = FakeRestore { rows: observer.rows.clone(), replace: Some(bindings_path.clone()) };
        assert!(restore_for_test(&workspace_path, &bindings_path, || true, &mutating).is_err());
        std::fs::write(&bindings_path, b"{bad").unwrap();
        assert!(restore_for_test(&workspace_path, &bindings_path, || true, &observer).is_err());
        std::fs::remove_file(&bindings_path).unwrap();
        assert!(restore_for_test(&workspace_path, &bindings_path, || true, &observer).is_err());
    }
    #[test]
    fn checked_restore_uses_unique_saved_member_after_reorder_and_recovers_invalid_health() {
        let dir = tempfile::tempdir().unwrap();
        let workspace_path = dir.path().join("workspaces.json"); let bindings_path = dir.path().join("bindings.json");
        let mut row = workspace();
        let mut a = row.windows[0].clone(); a.captured_title = "Document A".into(); a.valid = false;
        a.home_rect = Some(MmRect { x: 111, y: 0, w: 200, h: 100 });
        let mut b = row.windows[0].clone(); b.captured_title = "Document B".into();
        b.home_rect = Some(MmRect { x: 222, y: 0, w: 200, h: 100 });
        row.windows = vec![b, a];
        std::fs::write(&workspace_path, serde_json::to_vec(&[row.clone()]).unwrap()).unwrap();
        let mut binding = saved_binding("one", 10); binding.windows[0].captured_title = "Document A".into();
        let mut legacy_binding = serde_json::to_value([binding.clone()]).unwrap();
        let object = legacy_binding[0]["windows"][0].as_object_mut().unwrap();
        object.remove("window_id"); object.remove("window_index");
        std::fs::write(&bindings_path, serde_json::to_vec(&legacy_binding).unwrap()).unwrap();
        let observer = FakeRestore { rows: HashMap::from([(10, (candidate(10), identity(10, std::process::id().wrapping_add(1).max(1))))]), replace: None };
        let plan = restore_for_test(&workspace_path, &bindings_path, || true, &observer).unwrap();
        assert_eq!((plan.bound, plan.skipped), (1, 0));
        let restored = plan.rows()[0].workspace();
        assert_eq!(restored.windows[0].hwnd, 0);
        assert_eq!(restored.windows[1].hwnd, 10);
        assert!(restored.windows[1].valid);
        assert_eq!(restored.windows[1].home_rect.unwrap().x, 111);
        row.windows[0].captured_title = "Document A".into();
        std::fs::write(&workspace_path, serde_json::to_vec(&[row]).unwrap()).unwrap();
        assert_eq!(restore_for_test(&workspace_path, &bindings_path, || true, &observer).unwrap().bound, 0);
        binding.windows[0].captured_title.clear();
        std::fs::write(&bindings_path, serde_json::to_vec(&[binding]).unwrap()).unwrap();
        assert_eq!(restore_for_test(&workspace_path, &bindings_path, || true, &observer).unwrap().bound, 0);
    }
    #[test]
    fn checked_restore_keeps_retained_legacy_identity_and_refuses_ambiguous_names() {
        let dir = tempfile::tempdir().unwrap();
        let workspace_path = dir.path().join("workspaces.json"); let bindings_path = dir.path().join("bindings.json");
        let mut row = workspace(); row.id.clear(); row.name = "Legacy".into();
        let raw = serde_json::to_vec(&[row.clone()]).unwrap(); std::fs::write(&workspace_path, &raw).unwrap();
        let mut binding = saved_binding("one", 10); binding.workspace_id = None; binding.workspace_name = "Legacy".into();
        std::fs::write(&bindings_path, serde_json::to_vec(&[binding.clone()]).unwrap()).unwrap();
        let snapshot = store::load_checked_workspaces(&workspace_path, || true).unwrap();
        let retained_id = snapshot.workspaces()[0].id.clone();
        let observer = FakeRestore { rows: HashMap::from([(10, (candidate(10), identity(10, std::process::id().wrapping_add(1).max(1))))]), replace: None };
        let plan = restore_checked(&snapshot, &bindings_path, || true, &observer).unwrap();
        assert_eq!(plan.bound, 1); assert_eq!(plan.rows()[0].workspace().id, retained_id);
        assert_eq!(std::fs::read(&workspace_path).unwrap(), raw);
        std::fs::write(&workspace_path, serde_json::to_vec(&[row.clone(), row]).unwrap()).unwrap();
        assert!(!plan.is_current());
        assert_eq!(restore_for_test(&workspace_path, &bindings_path, || true, &observer).unwrap().bound, 0);
        let mut durable = workspace(); durable.name = "Legacy".into();
        std::fs::write(&workspace_path, serde_json::to_vec(&[durable]).unwrap()).unwrap();
        binding.workspace_id = Some("different-id".into());
        std::fs::write(&bindings_path, serde_json::to_vec(&[binding]).unwrap()).unwrap();
        assert_eq!(restore_for_test(&workspace_path, &bindings_path, || true, &observer).unwrap().bound, 0);
    }
    #[test]
    fn checked_workspace_matching_treats_whitespace_id_as_absent_and_unique_ids_as_authority() {
        let mut a = workspace(); a.id.clear(); a.name = "A".into();
        let mut b = workspace(); b.id.clear(); b.name = "B".into();
        let mut binding = saved_binding("one", 10);
        binding.workspace_id = Some("   ".into()); binding.workspace_name = "B".into();
        assert_eq!(unique_workspace(&[a.clone(), b.clone()], &binding), Some(1));
        a.id = "one".into(); b.id = "two".into(); a.name = "New Workspace".into(); b.name = "New Workspace".into();
        binding.workspace_id = Some("two".into()); binding.workspace_name = "New Workspace".into();
        assert_eq!(unique_workspace(&[a.clone(), b.clone()], &binding), Some(1));
        binding.workspace_id = None;
        assert_eq!(unique_workspace(&[a, b], &binding), None);
    }
    #[test]
    fn reconnect_preserves_fallible_enumeration_and_never_trusts_saved_hwnd() {
        let row = workspace();
        assert!(
            reconnect_checked(
                &[row.clone()],
                0,
                || true,
                &FakeReconnect {
                    fail: true,
                    rows: vec![]
                }
            )
            .is_err()
        );
        let empty = reconnect_checked(
            &[row.clone()],
            0,
            || true,
            &FakeReconnect {
                fail: false,
                rows: vec![],
            },
        )
        .unwrap();
        assert!(empty.identities.is_empty());
        let found = reconnect_checked(
            &[row.clone()],
            0,
            || true,
            &FakeReconnect {
                fail: false,
                rows: vec![(candidate(10), identity(10, 22))],
            },
        )
        .unwrap();
        assert_eq!(found.identities.get(&0), Some(&identity(10, 22)));
        assert_ne!(found.workspace.windows[0].hwnd, 999);
        assert!(
            reconnect_checked(
                &[row.clone(), row],
                0,
                || true,
                &FakeReconnect {
                    fail: false,
                    rows: vec![]
                }
            )
            .is_err()
        );
    }

    #[test]
    fn catalog_reconnect_enumerates_once_and_never_assigns_one_window_twice() {
        struct Backend { calls: std::cell::Cell<usize> }
        impl ReconnectBackend for Backend {
            fn enumerate(&self) -> Result<Vec<(EnumeratedWindow, WindowSwitchIdentity)>> {
                self.calls.set(self.calls.get() + 1);
                Ok(vec![(candidate(10), identity(10, 22))])
            }
        }
        let dir = tempfile::tempdir().unwrap(); let path = dir.path().join("workspaces.json");
        let first = workspace(); let mut second = workspace(); second.id = "two".into();
        let mut disabled = workspace(); disabled.id = "disabled".into(); disabled.disabled = true;
        std::fs::write(&path, serde_json::to_vec(&[first, second, disabled]).unwrap()).unwrap();
        let source = store::load_checked_workspaces(&path, || true).unwrap();
        let backend = Backend { calls: std::cell::Cell::new(0) };
        let plan = reconnect_catalog_checked(&source, || true, &backend).unwrap();
        assert_eq!(backend.calls.get(), 1);
        assert_eq!(plan.rows().iter().map(|row| row.index).collect::<Vec<_>>(), vec![0, 1]);
        assert_eq!(plan.skipped, 1);
        assert_eq!(plan.summary.reconnected, 1);
        assert_eq!(plan.summary.missing, 1);
        assert_eq!(plan.outcome, ReconnectCatalogOutcome::Partial);
        assert_eq!(plan.rows().iter().map(|row| row.identities.len()).sum::<usize>(), 1);
        assert!(plan.matches_snapshot(&source));
        assert!(plan.source_is_current());
    }

    #[test]
    fn catalog_reconnect_refuses_failed_enumeration_and_late_source_change() {
        struct Backend { path: PathBuf, mutate: bool, fail: bool }
        impl ReconnectBackend for Backend {
            fn enumerate(&self) -> Result<Vec<(EnumeratedWindow, WindowSwitchIdentity)>> {
                if self.mutate { std::fs::write(&self.path, b"[]")?; }
                if self.fail { anyhow::bail!("failed"); }
                Ok(Vec::new())
            }
        }
        let dir = tempfile::tempdir().unwrap(); let path = dir.path().join("workspaces.json");
        let original = serde_json::to_vec(&[workspace()]).unwrap();
        std::fs::write(&path, &original).unwrap();
        let source = store::load_checked_workspaces(&path, || true).unwrap();
        assert!(reconnect_catalog_checked(&source, || true, &Backend { path: path.clone(), mutate: false, fail: true }).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);
        let plan = reconnect_catalog_checked(&source, || true, &Backend { path: path.clone(), mutate: false, fail: false }).unwrap();
        assert_eq!(plan.outcome, ReconnectCatalogOutcome::NoMatch);
        assert!(reconnect_catalog_checked(&source, || true, &Backend { path: path.clone(), mutate: true, fail: false }).is_err());
        assert!(!plan.source_is_current());
    }

    #[test]
    fn enumeration_rejects_retirement_during_source_check() {
        let current = std::cell::Cell::new(true);
        assert!(!enumeration_source_current(
            || {
                current.set(false);
                true
            },
            || current.get(),
        ));
    }

    #[test]
    fn queued_enumeration_rechecks_authority_after_lane_acquisition() {
        use std::sync::{Arc, atomic::{AtomicBool, AtomicUsize, Ordering}};
        let lane = Arc::new(std::sync::Mutex::new(()));
        let hold = lane.lock().unwrap();
        let current = Arc::new(AtomicBool::new(true));
        let calls = Arc::new(AtomicUsize::new(0));
        let (ready, waiting) = std::sync::mpsc::channel();
        let worker_lane = lane.clone();
        let worker_current = current.clone();
        let worker_calls = calls.clone();
        let worker = std::thread::spawn(move || {
            ready.send(()).unwrap();
            with_enumeration_lane(&worker_lane, &|| worker_current.load(Ordering::Acquire), || {
                worker_calls.fetch_add(1, Ordering::AcqRel);
                Ok(())
            })
        });
        waiting.recv().unwrap();
        current.store(false, Ordering::Release);
        drop(hold);
        assert!(worker.join().unwrap().is_err());
        assert_eq!(calls.load(Ordering::Acquire), 0);
    }

    #[test]
    fn startup_catalog_loads_only_workspace_and_refuses_retired_authority() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("workspaces.json");
        let bytes = serde_json::to_vec(&[workspace()]).unwrap();
        std::fs::write(&path, &bytes).unwrap();
        let backend = FakeReconnect { fail: false, rows: vec![] };
        let plan = reconnect_catalog_from_path_checked(&path, || true, &backend).unwrap();
        assert_eq!(plan.rows().len(), 1);
        assert!(plan.rows()[0].identities.is_empty());
        assert!(reconnect_catalog_from_path_checked(&path, || false, &backend).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert!(!dir.path().join("bindings.json").exists());
    }

    #[test]
    fn catalog_legacy_row_has_source_ordinal_and_never_trusts_saved_handle() {
        let dir = tempfile::tempdir().unwrap(); let path = dir.path().join("legacy.json");
        let bytes = br#"[{"name":"Legacy","windows":[{"title":"Editor","hwnd":999,"binding_verified":true}]}]"#;
        std::fs::write(&path, bytes).unwrap();
        let source = store::load_checked_workspaces(&path, || true).unwrap();
        let plan = reconnect_catalog_checked(&source, || true, &FakeReconnect { fail: false, rows: vec![] }).unwrap();
        let reopened = store::load_checked_workspaces(&path, || true).unwrap();
        assert!(plan.matches_raw_source(&reopened));
        assert_eq!(plan.rows()[0].index, 0);
        assert!(plan.rows()[0].identities.is_empty());
        assert_eq!(plan.rows()[0].workspace.windows[0].hwnd, 0);
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }

    #[test]
    fn catalog_final_authority_cannot_replace_source_after_last_check() {
        let dir = tempfile::tempdir().unwrap(); let path = dir.path().join("workspaces.json");
        std::fs::write(&path, serde_json::to_vec(&[workspace()]).unwrap()).unwrap();
        let source = store::load_checked_workspaces(&path, || true).unwrap();
        let calls = std::cell::Cell::new(0);
        let result = reconnect_catalog_checked(&source, || { calls.set(calls.get() + 1); if calls.get() == 4 { std::fs::write(&path, b"[]").unwrap(); } true }, &FakeReconnect { fail: false, rows: vec![] });
        assert!(result.is_err());
    }

    struct FakePlacement {
        foreground: WindowNativeIdentity,
        moved: Vec<(usize, MmRect)>,
        unrelated_after_first: bool,
        cleanup_warning: bool,
        foreign_target: bool,
        change_target_at_activation: bool,
    }
    impl PlacementBackend for FakePlacement {
        fn workspace_on_current_desktop(
            &mut self,
            required: Option<&crate::virtual_desktop::VirtualDesktopId>,
            current: Arc<dyn Fn() -> bool + Send + Sync>,
        ) -> Result<bool> {
            anyhow::ensure!(current(), "revoked");
            Ok(required.is_none() || !self.foreign_target)
        }
        fn on_current_desktop(
            &mut self,
            _: &WindowSwitchIdentity,
            current: Arc<dyn Fn() -> bool + Send + Sync>,
        ) -> Result<bool> {
            anyhow::ensure!(current(), "revoked");
            Ok(true)
        }
        fn activate(
            &mut self,
            issuer: WindowNativeIdentity,
            target: &WindowSwitchIdentity,
            previous: Option<&WindowSwitchIdentity>,
            required: Option<&crate::virtual_desktop::VirtualDesktopId>,
            current: Arc<dyn Fn() -> bool + Send + Sync>,
        ) -> Result<()> {
            anyhow::ensure!(
                current()
                    && !(required.is_some() && self.change_target_at_activation)
                    && self.foreground == previous.map_or(issuer, |previous| previous.native),
                "unrelated foreground"
            );
            self.foreground = target.native;
            Ok(())
        }
        fn place(
            &mut self,
            _: WindowNativeIdentity,
            target: &WindowSwitchIdentity,
            rect: MmRect,
            _: Option<&crate::virtual_desktop::VirtualDesktopId>,
            current: Arc<dyn Fn() -> bool + Send + Sync>,
        ) -> Result<PlacementOutcome> {
            anyhow::ensure!(
                current() && self.foreground == target.native,
                "stale placement"
            );
            self.moved.push((target.native.hwnd, rect));
            if self.unrelated_after_first {
                self.foreground = WindowNativeIdentity {
                    hwnd: 90,
                    pid: 90,
                    tid: 90,
                };
            }
            Ok(PlacementOutcome {
                cleanup_warning: self.cleanup_warning.then(|| "cleanup failed".into()),
            })
        }
    }
    #[test]
    fn restored_placement_refuses_changed_source_before_any_move() {
        let dir = tempfile::tempdir().unwrap();
        let workspace_path = dir.path().join("workspaces.json"); let bindings_path = dir.path().join("bindings.json");
        std::fs::write(&workspace_path, serde_json::to_vec(&[workspace()]).unwrap()).unwrap();
        std::fs::write(&bindings_path, serde_json::to_vec(&[saved_binding("one", 10)]).unwrap()).unwrap();
        let observer = FakeRestore { rows: HashMap::from([(10, (candidate(10), identity(10, std::process::id().wrapping_add(1).max(1))))]), replace: None };
        let plan = Arc::new(restore_for_test(&workspace_path, &bindings_path, || true, &observer).unwrap());
        std::fs::write(&bindings_path, b"[]").unwrap();
        let issuer = WindowNativeIdentity { hwnd: 1, pid: 1, tid: 1 };
        let mut backend = FakePlacement { foreground: issuer, moved: Vec::new(), unrelated_after_first: false, cleanup_warning: false, foreign_target: false, change_target_at_activation: false };
        let report = place_restored_checked(plan, 0, issuer, Destination::Home, Arc::new(|| true), &mut backend);
        assert!(report.error.is_some());
        assert!(backend.moved.is_empty());
        assert_eq!(backend.foreground, issuer);
    }
    #[test]
    fn checked_placement_preserves_order_rect_fallback_and_foreground_chain() {
        let mut row = workspace();
        row.windows.push(row.windows[0].clone());
        let fallback = MmRect {
            x: -30,
            y: 5,
            w: 400,
            h: 300,
        };
        let specific = MmRect {
            x: 20,
            y: -9,
            w: 100,
            h: 80,
        };
        row.home_rect = Some(fallback);
        row.windows[1].home_rect = Some(specific);
        let overlay = ReconnectedWorkspace {
            workspace: row,
            identities: HashMap::from([(0, identity(10, 22)), (1, identity(11, 22))]),
            summary: ReconnectSummary::default(),
        };
        let issuer = WindowNativeIdentity {
            hwnd: 1,
            pid: 1,
            tid: 1,
        };
        let mut backend = FakePlacement {
            foreground: issuer,
            moved: vec![],
            unrelated_after_first: false,
            cleanup_warning: false,
            foreign_target: false,
            change_target_at_activation: false,
        };
        let report = place_checked(
            &overlay,
            issuer,
            Destination::Home,
            Arc::new(|| true),
            &mut backend,
        );
        assert_eq!(report.moved, 2);
        assert!(report.error.is_none());
        assert_eq!(backend.moved, vec![(10, fallback), (11, specific)]);
        let mut backend = FakePlacement {
            foreground: issuer,
            moved: vec![],
            unrelated_after_first: true,
            cleanup_warning: false,
            foreign_target: false,
            change_target_at_activation: false,
        };
        let report = place_checked(
            &overlay,
            issuer,
            Destination::Home,
            Arc::new(|| true),
            &mut backend,
        );
        assert_eq!(report.moved, 1);
        assert!(report.error.is_some());
        let mut backend = FakePlacement {
            foreground: issuer,
            moved: vec![],
            unrelated_after_first: false,
            cleanup_warning: true,
            foreign_target: false,
            change_target_at_activation: false,
        };
        let report = place_checked(
            &overlay,
            issuer,
            Destination::Home,
            Arc::new(|| true),
            &mut backend,
        );
        assert_eq!(report.moved, 1);
        assert_eq!(report.error.as_deref(), Some("cleanup failed"));
    }
    #[test]
    fn topmost_cleanup_depends_only_on_exact_identity() {
        let calls = std::cell::Cell::new(0);
        assert!(
            cleanup_after_move(
                true,
                || true,
                || {
                    calls.set(calls.get() + 1);
                    Ok(())
                }
            )
            .is_none()
        );
        identity_only_cleanup(
            || true,
            || {
                calls.set(calls.get() + 1);
                Ok(())
            },
        )
        .unwrap();
        identity_only_cleanup(
            || false,
            || {
                calls.set(calls.get() + 1);
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn home_ignores_foreign_target_binding_but_target_rechecks_it() {
        let mut row = workspace();
        let rect = MmRect {
            x: 0,
            y: 0,
            w: 100,
            h: 100,
        };
        row.home_rect = Some(rect);
        row.target_rect = Some(rect);
        row.virtual_desktop = Some(crate::virtual_desktop::VirtualDesktopBinding {
            id: crate::virtual_desktop::VirtualDesktopId::parse(
                "550e8400-e29b-41d4-a716-446655440000",
            )
            .unwrap(),
            cached_name: None,
        });
        let overlay = ReconnectedWorkspace {
            workspace: row,
            identities: HashMap::from([(0, identity(10, 22))]),
            summary: ReconnectSummary::default(),
        };
        let issuer = WindowNativeIdentity {
            hwnd: 1,
            pid: 1,
            tid: 1,
        };
        let mut foreign = FakePlacement {
            foreground: issuer,
            moved: vec![],
            unrelated_after_first: false,
            cleanup_warning: false,
            foreign_target: true,
            change_target_at_activation: false,
        };
        assert_eq!(
            place_checked(
                &overlay,
                issuer,
                Destination::Home,
                Arc::new(|| true),
                &mut foreign
            )
            .moved,
            1
        );
        foreign.foreground = issuer;
        foreign.moved.clear();
        let target = place_checked(
            &overlay,
            issuer,
            Destination::Target,
            Arc::new(|| true),
            &mut foreign,
        );
        assert_eq!(target.moved, 0);
        assert_eq!(target.skipped, 1);
        let mut changed = FakePlacement {
            foreground: issuer,
            moved: vec![],
            unrelated_after_first: false,
            cleanup_warning: false,
            foreign_target: false,
            change_target_at_activation: true,
        };
        let target = place_checked(
            &overlay,
            issuer,
            Destination::Target,
            Arc::new(|| true),
            &mut changed,
        );
        assert_eq!(target.moved, 0);
        assert!(target.error.is_some());
    }
}
