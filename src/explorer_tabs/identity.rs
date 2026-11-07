use std::collections::{BTreeMap, HashSet};
use std::num::{NonZeroU32, NonZeroU64, NonZeroUsize};

use super::operation::FolderPath;

pub const MAX_OBSERVED_WINDOWS: usize = 1024;
pub const MAX_SHELL_VIEWS: usize = 256;

/// The current identity of a top-level Explorer window and its root owner.
///
/// This value is an observation, not a capability to act on the window. Its
/// HWND, root, PID and thread IDs must be re-observed immediately before any
/// later use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WindowIdentity {
    hwnd: NonZeroUsize,
    root_hwnd: NonZeroUsize,
    process_id: NonZeroU32,
    thread_id: NonZeroU32,
    root_process_id: NonZeroU32,
    root_thread_id: NonZeroU32,
}

impl WindowIdentity {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        hwnd: usize,
        root_hwnd: usize,
        process_id: u32,
        thread_id: u32,
        root_process_id: u32,
        root_thread_id: u32,
    ) -> Option<Self> {
        Some(Self {
            hwnd: NonZeroUsize::new(hwnd)?,
            root_hwnd: NonZeroUsize::new(root_hwnd)?,
            process_id: NonZeroU32::new(process_id)?,
            thread_id: NonZeroU32::new(thread_id)?,
            root_process_id: NonZeroU32::new(root_process_id)?,
            root_thread_id: NonZeroU32::new(root_thread_id)?,
        })
    }

    pub fn hwnd(self) -> usize {
        self.hwnd.get()
    }

    pub fn root_hwnd(self) -> usize {
        self.root_hwnd.get()
    }

    pub fn process_id(self) -> u32 {
        self.process_id.get()
    }

    pub fn thread_id(self) -> u32 {
        self.thread_id.get()
    }

    pub fn root_process_id(self) -> u32 {
        self.root_process_id.get()
    }

    pub fn root_thread_id(self) -> u32 {
        self.root_thread_id.get()
    }
}

/// Worker-local monotonic identity for a canonical Shell browser binding.
///
/// The number is assigned by comparing live IUnknown references on the COM
/// worker. It is never a pointer value and is meaningful only for that worker
/// session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ShellBindingIdentity(NonZeroU64);

impl ShellBindingIdentity {
    pub(crate) fn from_worker_token(token: u64) -> Option<Self> {
        NonZeroU64::new(token).map(Self)
    }
}

/// Worker-local identity for an active IShellView binding.
///
/// This is not a UI Automation tab identity and cannot verify a folder-tab
/// operation. It is used only to compare Shell-view observations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ShellViewIdentity(NonZeroU64);

impl ShellViewIdentity {
    pub(crate) fn from_worker_token(token: u64) -> Option<Self> {
        NonZeroU64::new(token).map(Self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PidlPathMatch {
    Exact,
    Different,
}

/// A Shell PIDL comparison tied to the exact path supplied to the observer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellPathComparison {
    requested_path: FolderPath,
    result: PidlPathMatch,
}

impl ShellPathComparison {
    pub fn requested_path(&self) -> &FolderPath {
        &self.requested_path
    }

    pub fn result(&self) -> PidlPathMatch {
        self.result
    }

    pub(crate) fn new(requested_path: FolderPath, result: PidlPathMatch) -> Self {
        Self {
            requested_path,
            result,
        }
    }
}

/// One read-only active Shell view. Its registration index is only the
/// IShellWindows enumeration position; it is deliberately not called a tab
/// ordinal because Shell does not expose Explorer's rendered tab order here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellViewCandidate {
    window: WindowIdentity,
    shell_binding: ShellBindingIdentity,
    view_identity: ShellViewIdentity,
    registration_index: u16,
    current_folder: Option<FolderPath>,
    path_comparison: Option<ShellPathComparison>,
}

impl ShellViewCandidate {
    pub fn window(&self) -> WindowIdentity {
        self.window
    }

    pub fn shell_binding(&self) -> ShellBindingIdentity {
        self.shell_binding
    }

    pub fn view_identity(&self) -> ShellViewIdentity {
        self.view_identity
    }

    pub fn registration_index(&self) -> u16 {
        self.registration_index
    }

    pub fn current_folder(&self) -> Option<&FolderPath> {
        self.current_folder.as_ref()
    }

    pub fn path_comparison(&self) -> Option<&ShellPathComparison> {
        self.path_comparison.as_ref()
    }

    pub(crate) fn new(
        window: WindowIdentity,
        shell_binding: ShellBindingIdentity,
        view_identity: ShellViewIdentity,
        registration_index: u16,
        current_folder: Option<FolderPath>,
        path_comparison: Option<ShellPathComparison>,
    ) -> Self {
        Self {
            window,
            shell_binding,
            view_identity,
            registration_index,
            current_folder,
            path_comparison,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotFailure {
    WindowEnumerationFailed,
    WindowLimitExceeded,
    ShellUnavailable,
    ShellCountFailed,
    ShellCountInvalid,
    ShellLimitExceeded,
    ExplorerIdentityUnavailable,
    ShellItemUnavailable { index: u16 },
    ShellDispatchUnavailable { index: u16 },
    WindowHandleUnavailable { index: u16 },
    WindowIdentityUnavailable { index: u16 },
    ShellBrowserUnavailable { index: u16 },
    ActiveViewUnavailable { index: u16 },
    FolderUnavailable { index: u16 },
    ExpectedFolderUnavailable,
    InvalidWindowIdentity,
    DuplicateWindowHandle,
    UnknownWindowForView,
    AmbiguousShellBindings,
    DuplicateShellRegistration,
    DuplicateViewIdentity,
    DuplicateRegistrationIndex,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellViewSnapshot {
    windows: Vec<WindowIdentity>,
    views: Vec<ShellViewCandidate>,
    failure: Option<SnapshotFailure>,
}

impl ShellViewSnapshot {
    pub fn complete(
        windows: Vec<WindowIdentity>,
        views: Vec<ShellViewCandidate>,
    ) -> Result<Self, SnapshotFailure> {
        if windows.len() > MAX_OBSERVED_WINDOWS {
            return Err(SnapshotFailure::WindowLimitExceeded);
        }
        if views.len() > MAX_SHELL_VIEWS {
            return Err(SnapshotFailure::ShellLimitExceeded);
        }

        let mut windows_by_handle = BTreeMap::new();
        for window in &windows {
            if windows_by_handle.insert(window.hwnd(), *window).is_some() {
                return Err(SnapshotFailure::DuplicateWindowHandle);
            }
        }

        let mut shell_by_handle = BTreeMap::new();
        let mut shell_by_identity = HashSet::with_capacity(views.len());
        let mut view_identities = HashSet::with_capacity(views.len());
        let mut registration_indices = HashSet::with_capacity(views.len());
        for view in &views {
            if windows_by_handle.get(&view.window.hwnd()) != Some(&view.window) {
                return Err(SnapshotFailure::UnknownWindowForView);
            }
            match shell_by_handle.insert(view.window.hwnd(), view.shell_binding) {
                Some(previous) if previous != view.shell_binding => {
                    return Err(SnapshotFailure::AmbiguousShellBindings);
                }
                _ => {}
            }
            if !shell_by_identity.insert(view.shell_binding) {
                return Err(SnapshotFailure::DuplicateShellRegistration);
            }
            if !view_identities.insert(view.view_identity) {
                return Err(SnapshotFailure::DuplicateViewIdentity);
            }
            if !registration_indices.insert(view.registration_index) {
                return Err(SnapshotFailure::DuplicateRegistrationIndex);
            }
        }

        Ok(Self {
            windows,
            views,
            failure: None,
        })
    }

    pub fn incomplete(failure: SnapshotFailure) -> Self {
        Self {
            windows: Vec::new(),
            views: Vec::new(),
            failure: Some(failure),
        }
    }

    pub fn is_complete(&self) -> bool {
        self.failure.is_none()
    }

    pub fn failure(&self) -> Option<SnapshotFailure> {
        self.failure
    }

    pub fn windows(&self) -> &[WindowIdentity] {
        &self.windows
    }

    pub fn views(&self) -> &[ShellViewCandidate] {
        &self.views
    }
}

/// A Shell observation that looks like a newly appeared target. This remains
/// only a candidate: a matching folder and absent baseline HWND do not prove
/// that this operation caused the window to open. A later correlated event is
/// required before it can become an operation target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellTargetCandidate {
    window: WindowIdentity,
    shell_binding: ShellBindingIdentity,
    view_identity: ShellViewIdentity,
    requested_path: FolderPath,
}

impl ShellTargetCandidate {
    pub fn window(&self) -> WindowIdentity {
        self.window
    }

    pub fn shell_binding(&self) -> ShellBindingIdentity {
        self.shell_binding
    }

    pub fn view_identity(&self) -> ShellViewIdentity {
        self.view_identity
    }

    pub fn requested_path(&self) -> &FolderPath {
        &self.requested_path
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CandidateCorrelation {
    Unique(ShellTargetCandidate),
    None,
    Ambiguous,
    Incomplete(SnapshotFailure),
}

/// Correlates only an unfiltered, complete baseline with a complete later
/// observation. The result is intentionally not launch proof.
pub fn correlate_shell_candidate(
    before: &ShellViewSnapshot,
    after: &ShellViewSnapshot,
    requested_path: &FolderPath,
) -> CandidateCorrelation {
    if let Some(failure) = before.failure.or(after.failure) {
        return CandidateCorrelation::Incomplete(failure);
    }

    let before_hwnds: HashSet<_> = before.windows.iter().map(|window| window.hwnd()).collect();
    let mut candidates = Vec::new();
    for view in &after.views {
        if before_hwnds.contains(&view.window.hwnd()) {
            continue;
        }
        let exact_match = view.path_comparison.as_ref().is_some_and(|comparison| {
            comparison.requested_path == *requested_path
                && comparison.result == PidlPathMatch::Exact
        });
        if exact_match {
            candidates.push(ShellTargetCandidate {
                window: view.window,
                shell_binding: view.shell_binding,
                view_identity: view.view_identity,
                requested_path: requested_path.clone(),
            });
        }
    }
    match candidates.len() {
        0 => CandidateCorrelation::None,
        1 => CandidateCorrelation::Unique(candidates.remove(0)),
        _ => CandidateCorrelation::Ambiguous,
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;

    pub(crate) fn window(hwnd: usize) -> WindowIdentity {
        WindowIdentity::new(hwnd, hwnd, 100, 200, 100, 200).expect("nonzero fixture identity")
    }

    pub(crate) fn changed_process(
        window: WindowIdentity,
        process: u32,
        thread: u32,
    ) -> WindowIdentity {
        WindowIdentity::new(
            window.hwnd(),
            window.root_hwnd(),
            process,
            thread,
            window.root_process_id(),
            window.root_thread_id(),
        )
        .expect("nonzero fixture identity")
    }

    pub(crate) fn shell_binding(token: u64) -> ShellBindingIdentity {
        ShellBindingIdentity::from_worker_token(token).expect("nonzero fixture token")
    }

    pub(crate) fn view_identity(token: u64) -> ShellViewIdentity {
        ShellViewIdentity::from_worker_token(token).expect("nonzero fixture token")
    }

    pub(crate) fn view(
        window: WindowIdentity,
        shell_binding: ShellBindingIdentity,
        view_identity: ShellViewIdentity,
        registration_index: u16,
        path_comparison: Option<ShellPathComparison>,
    ) -> ShellViewCandidate {
        ShellViewCandidate::new(
            window,
            shell_binding,
            view_identity,
            registration_index,
            None,
            path_comparison,
        )
    }

    pub(crate) fn path_comparison(path: FolderPath, result: PidlPathMatch) -> ShellPathComparison {
        ShellPathComparison::new(path, result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::explorer_tabs::operation::FolderPath;

    #[test]
    fn explorer_tabs_identity_rejects_distinct_shell_bindings_on_same_hwnd() {
        let window = test_support::window(10);
        let result = ShellViewSnapshot::complete(
            vec![window],
            vec![
                test_support::view(
                    window,
                    test_support::shell_binding(1),
                    test_support::view_identity(1),
                    0,
                    None,
                ),
                test_support::view(
                    window,
                    test_support::shell_binding(2),
                    test_support::view_identity(2),
                    1,
                    None,
                ),
            ],
        );

        assert_eq!(result, Err(SnapshotFailure::AmbiguousShellBindings));
    }

    #[test]
    fn explorer_tabs_identity_rejects_unknown_windows_and_duplicate_bindings() {
        let window = test_support::window(10);
        let unknown = test_support::window(11);
        let unknown_window = ShellViewSnapshot::complete(
            vec![window],
            vec![test_support::view(
                unknown,
                test_support::shell_binding(1),
                test_support::view_identity(1),
                0,
                None,
            )],
        );
        assert_eq!(unknown_window, Err(SnapshotFailure::UnknownWindowForView));

        let duplicate_binding = ShellViewSnapshot::complete(
            vec![window],
            vec![
                test_support::view(
                    window,
                    test_support::shell_binding(1),
                    test_support::view_identity(1),
                    0,
                    None,
                ),
                test_support::view(
                    window,
                    test_support::shell_binding(1),
                    test_support::view_identity(2),
                    1,
                    None,
                ),
            ],
        );
        assert_eq!(
            duplicate_binding,
            Err(SnapshotFailure::DuplicateShellRegistration)
        );
    }

    #[test]
    fn explorer_tabs_identity_rejects_window_snapshot_over_limit() {
        let windows = (1..=MAX_OBSERVED_WINDOWS + 1)
            .map(test_support::window)
            .collect();
        assert_eq!(
            ShellViewSnapshot::complete(windows, Vec::new()),
            Err(SnapshotFailure::WindowLimitExceeded)
        );
    }

    #[test]
    fn explorer_tabs_identity_rejects_shell_view_snapshot_over_limit() {
        let windows: Vec<_> = (1..=MAX_SHELL_VIEWS + 1)
            .map(test_support::window)
            .collect();
        let views = windows
            .iter()
            .enumerate()
            .map(|(index, window)| {
                test_support::view(
                    *window,
                    test_support::shell_binding(index as u64 + 1),
                    test_support::view_identity(index as u64 + 1),
                    index as u16,
                    None,
                )
            })
            .collect();

        assert_eq!(
            ShellViewSnapshot::complete(windows, views),
            Err(SnapshotFailure::ShellLimitExceeded)
        );
    }

    #[test]
    fn explorer_tabs_identity_candidate_is_path_bound_bounded_and_not_launch_proof() {
        let folder = FolderPath::parse(r"C:\Projects\東京").unwrap();
        let old = test_support::window(10);
        let new = test_support::window(20);
        let before = ShellViewSnapshot::complete(
            vec![old],
            vec![test_support::view(
                old,
                test_support::shell_binding(1),
                test_support::view_identity(1),
                0,
                None,
            )],
        )
        .unwrap();
        let after = ShellViewSnapshot::complete(
            vec![old, new],
            vec![
                test_support::view(
                    old,
                    test_support::shell_binding(1),
                    test_support::view_identity(1),
                    0,
                    None,
                ),
                test_support::view(
                    new,
                    test_support::shell_binding(2),
                    test_support::view_identity(2),
                    1,
                    Some(test_support::path_comparison(
                        folder.clone(),
                        PidlPathMatch::Exact,
                    )),
                ),
            ],
        )
        .unwrap();
        let candidate = correlate_shell_candidate(&before, &after, &folder);
        let CandidateCorrelation::Unique(candidate) = candidate else {
            panic!("expected one shell candidate");
        };
        assert_eq!(candidate.window(), new);
        assert_eq!(candidate.requested_path(), &folder);

        let different_path = FolderPath::parse(r"C:\Other").unwrap();
        assert_eq!(
            correlate_shell_candidate(&before, &after, &different_path),
            CandidateCorrelation::None
        );

        let ambiguous = ShellViewSnapshot::complete(
            vec![old, new, test_support::window(30)],
            vec![
                after.views()[0].clone(),
                after.views()[1].clone(),
                test_support::view(
                    test_support::window(30),
                    test_support::shell_binding(3),
                    test_support::view_identity(3),
                    2,
                    Some(test_support::path_comparison(
                        folder.clone(),
                        PidlPathMatch::Exact,
                    )),
                ),
            ],
        )
        .unwrap();
        assert_eq!(
            correlate_shell_candidate(&before, &ambiguous, &folder),
            CandidateCorrelation::Ambiguous
        );
    }

    #[test]
    fn explorer_tabs_identity_same_hwnd_with_changed_process_is_not_the_same_target() {
        let original = test_support::window(10);
        let reused = test_support::changed_process(original, 101, 201);
        assert_ne!(original, reused);
        let before = ShellViewSnapshot::complete(vec![original], Vec::new()).unwrap();
        let after = ShellViewSnapshot::complete(vec![reused], Vec::new()).unwrap();
        assert!(matches!(
            correlate_shell_candidate(&before, &after, &FolderPath::parse(r"C:\Projects").unwrap()),
            CandidateCorrelation::None
        ));
    }
}
