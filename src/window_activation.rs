//! Shared Windows foreground activation with explicit virtual-desktop policy.

use crate::virtual_desktop::{VirtualDesktopError, VirtualDesktopId, VirtualDesktopService};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    sync::{
        Arc, Condvar, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowDesktopPolicy {
    /// Switch to the desktop which already owns the window. Never relocate it.
    FollowWindow,
    /// Refuse to activate a window that is not already on the current desktop.
    CurrentDesktopOnly,
    /// Relocate the target onto the current desktop before activation.
    MoveToCurrentDesktop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowActivationRequest {
    pub hwnd: usize,
    pub desktop_policy: WindowDesktopPolicy,
}

/// Fences launcher ROOT activation against a newer visibility request and a
/// stale/reused native window identity.
#[derive(Clone)]
pub struct WindowActivationFence {
    pub revision: crate::visibility::VisibilityRevision,
    pub request_revision: u64,
    pub visible: Arc<AtomicBool>,
    pub expected_process: u32,
    pub root_window: crate::visibility::RootWindowBridge,
    pub root_window_generation: u64,
}

impl WindowActivationRequest {
    pub const fn follow_window(hwnd: usize) -> Self {
        Self {
            hwnd,
            desktop_policy: WindowDesktopPolicy::FollowWindow,
        }
    }

    pub const fn move_to_current_desktop(hwnd: usize) -> Self {
        Self {
            hwnd,
            desktop_policy: WindowDesktopPolicy::MoveToCurrentDesktop,
        }
    }

    pub const fn current_desktop_only(hwnd: usize) -> Self {
        Self {
            hwnd,
            desktop_policy: WindowDesktopPolicy::CurrentDesktopOnly,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowActivationErrorKind {
    InvalidWindow,
    Desktop,
    ForegroundDenied,
    Superseded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowActivationError {
    pub kind: WindowActivationErrorKind,
    pub message: String,
    pub context: BTreeMap<String, String>,
}

impl WindowActivationError {
    fn new(kind: WindowActivationErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            context: BTreeMap::new(),
        }
    }

    pub fn context(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.context.insert(key.into(), value.into());
        self
    }
}

impl fmt::Display for WindowActivationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.message.fmt(f)
    }
}

impl std::error::Error for WindowActivationError {}

pub fn activate_window(request: WindowActivationRequest) -> Result<(), WindowActivationError> {
    activate_window_with_fence(request, None)
}

pub fn activate_window_if_current(
    request: WindowActivationRequest,
    fence: WindowActivationFence,
) -> Result<(), WindowActivationError> {
    activate_window_with_fence(request, Some(fence))
}

fn activate_window_with_fence(
    request: WindowActivationRequest,
    fence: Option<WindowActivationFence>,
) -> Result<(), WindowActivationError> {
    #[cfg(windows)]
    {
        activate_with_fence(&mut WindowsActivationBackend, request, fence.as_ref())
    }
    #[cfg(not(windows))]
    {
        let _ = (request, fence);
        Err(WindowActivationError::new(
            WindowActivationErrorKind::ForegroundDenied,
            "Window activation is available only on Windows",
        ))
    }
}

trait ActivationBackend {
    fn is_window(&mut self, hwnd: usize) -> bool;
    fn window_desktop(&mut self, hwnd: usize) -> Result<VirtualDesktopId, VirtualDesktopError>;
    fn current_desktop(&mut self) -> Result<VirtualDesktopId, VirtualDesktopError>;
    fn is_window_on_current(&mut self, hwnd: usize) -> Result<bool, VirtualDesktopError>;
    fn switch_desktop(&mut self, desktop: &VirtualDesktopId) -> Result<(), VirtualDesktopError>;
    fn move_window(
        &mut self,
        hwnd: usize,
        desktop: &VirtualDesktopId,
    ) -> Result<(), VirtualDesktopError>;
    fn restore(&mut self, hwnd: usize) -> bool;
    fn is_minimized(&mut self, hwnd: usize) -> bool;
    fn is_window_visible(&mut self, hwnd: usize) -> bool;
    fn is_physically_parked(&mut self, hwnd: usize) -> bool;
    fn hide_without_activation(&mut self, hwnd: usize) -> bool;
    fn show_without_activation(&mut self, hwnd: usize) -> bool;
    fn foreground_window(&mut self) -> Option<usize>;
    fn window_thread(&mut self, hwnd: usize) -> u32;
    fn current_thread(&mut self) -> u32;
    fn attach_input(&mut self, from: u32, to: u32, attach: bool) -> Result<(), String>;
    fn bring_to_top(&mut self, hwnd: usize) -> bool;
    fn set_foreground(&mut self, hwnd: usize) -> bool;
    fn target_process(&mut self, hwnd: usize) -> u32;
    fn pause(&mut self, duration: Duration);
}

const MAX_ROOT_FOREGROUND_TRANSITIONS: usize = 256;
const ROOT_FOREGROUND_EVENT_TIMEOUT: Duration = Duration::from_millis(500);
const MAX_ROOT_FOCUS_COMPENSATION_ATTEMPTS: usize = 3;
const HIDDEN_ROOT_FALLBACK_TIMEOUT: Duration = Duration::from_millis(500);
const HIDDEN_ROOT_FALLBACK_POLL: Duration = Duration::from_millis(10);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ForegroundWindowIdentity {
    hwnd: usize,
    process_id: u32,
    thread_id: u32,
    lifetime_token: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ForegroundJournalEventKind {
    Foreground,
    WindowCreated,
    WindowDestroyed,
}

#[derive(Clone, Copy)]
struct ForegroundTransition {
    sequence: u64,
    kind: ForegroundJournalEventKind,
    hwnd: usize,
    identity: Option<ForegroundWindowIdentity>,
}

#[derive(Default)]
struct ForegroundJournalState {
    next_sequence: u64,
    dropped_through: u64,
    transitions: std::collections::VecDeque<ForegroundTransition>,
    next_lifetime_token: u64,
    current_lifetimes: BTreeMap<usize, u64>,
    invalidated_lifetimes: BTreeSet<usize>,
    tracked_handles: BTreeSet<usize>,
}

struct ForegroundJournalInner {
    state: Mutex<ForegroundJournalState>,
    changed: Condvar,
    alive: AtomicBool,
    cancel: AtomicBool,
    #[cfg(windows)]
    hook_thread_id: std::sync::atomic::AtomicU32,
    #[cfg(test)]
    before_cursor_test: Mutex<Option<Box<dyn FnOnce() + Send>>>,
}

#[derive(Clone)]
struct ForegroundTransitionJournal {
    inner: Arc<ForegroundJournalInner>,
}

#[derive(Clone)]
struct ForegroundObservation {
    journal: ForegroundTransitionJournal,
    cursor: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum JournalFocusChoice {
    Newer(ForegroundWindowIdentity),
    NoNewerExternal,
    Invalidated,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PostCompensationChoice {
    Stable,
    Newer(ForegroundWindowIdentity),
    Invalidated,
}

impl ForegroundTransitionJournal {
    fn new() -> Self {
        Self {
            inner: Arc::new(ForegroundJournalInner {
                state: Mutex::new(ForegroundJournalState {
                    transitions: std::collections::VecDeque::with_capacity(
                        MAX_ROOT_FOREGROUND_TRANSITIONS,
                    ),
                    ..ForegroundJournalState::default()
                }),
                changed: Condvar::new(),
                alive: AtomicBool::new(true),
                cancel: AtomicBool::new(false),
                #[cfg(windows)]
                hook_thread_id: std::sync::atomic::AtomicU32::new(0),
                #[cfg(test)]
                before_cursor_test: Mutex::new(None),
            }),
        }
    }

    fn cursor(&self) -> Option<u64> {
        #[cfg(test)]
        if let Some(before_cursor) = self
            .inner
            .before_cursor_test
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
        {
            before_cursor();
        }
        if !self.inner.alive.load(Ordering::Acquire) {
            return None;
        }
        let state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Some(state.next_sequence)
    }

    fn track_window(&self, hwnd: usize) {
        if hwnd == 0 {
            return;
        }
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.tracked_handles.insert(hwnd);
        if !state.invalidated_lifetimes.contains(&hwnd)
            && !state.current_lifetimes.contains_key(&hwnd)
        {
            state.next_lifetime_token = state.next_lifetime_token.wrapping_add(1).max(1);
            let token = state.next_lifetime_token;
            state.current_lifetimes.insert(hwnd, token);
        }
    }

    fn record_event(
        &self,
        kind: ForegroundJournalEventKind,
        hwnd: usize,
        native_identity: Option<(u32, u32)>,
    ) -> Option<ForegroundWindowIdentity> {
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if matches!(
            kind,
            ForegroundJournalEventKind::WindowCreated | ForegroundJournalEventKind::WindowDestroyed
        ) && !state.tracked_handles.contains(&hwnd)
        {
            return None;
        }
        if kind == ForegroundJournalEventKind::Foreground {
            state.tracked_handles.insert(hwnd);
        }
        state.next_sequence = state.next_sequence.wrapping_add(1).max(1);
        if state.transitions.len() == MAX_ROOT_FOREGROUND_TRANSITIONS
            && let Some(dropped) = state.transitions.pop_front()
        {
            state.dropped_through = state.dropped_through.max(dropped.sequence);
        }
        let sequence = state.next_sequence;
        let identity = match kind {
            ForegroundJournalEventKind::WindowCreated => {
                state.next_lifetime_token = state.next_lifetime_token.wrapping_add(1).max(1);
                let token = state.next_lifetime_token;
                state.current_lifetimes.insert(hwnd, token);
                state.invalidated_lifetimes.remove(&hwnd);
                None
            }
            ForegroundJournalEventKind::WindowDestroyed => {
                state.current_lifetimes.remove(&hwnd);
                state.invalidated_lifetimes.insert(hwnd);
                None
            }
            ForegroundJournalEventKind::Foreground => {
                if state.invalidated_lifetimes.contains(&hwnd) {
                    None
                } else {
                    let lifetime_token = if let Some(token) = state.current_lifetimes.get(&hwnd) {
                        *token
                    } else {
                        state.next_lifetime_token =
                            state.next_lifetime_token.wrapping_add(1).max(1);
                        let token = state.next_lifetime_token;
                        state.current_lifetimes.insert(hwnd, token);
                        token
                    };
                    native_identity.and_then(|(process_id, thread_id)| {
                        (process_id != 0 && thread_id != 0).then_some(ForegroundWindowIdentity {
                            hwnd,
                            process_id,
                            thread_id,
                            lifetime_token,
                        })
                    })
                }
            }
        };
        state.transitions.push_back(ForegroundTransition {
            sequence,
            kind,
            hwnd,
            identity,
        });
        self.inner.changed.notify_all();
        identity
    }

    fn identity_for_current_window(
        &self,
        hwnd: usize,
        process_id: u32,
        thread_id: u32,
        cursor: u64,
    ) -> Option<ForegroundWindowIdentity> {
        if process_id == 0 || thread_id == 0 {
            return None;
        }
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.tracked_handles.insert(hwnd);
        if state.invalidated_lifetimes.contains(&hwnd) {
            return None;
        }
        let latest_lifecycle = state.transitions.iter().rev().find(|transition| {
            transition.hwnd == hwnd
                && transition.sequence > cursor
                && matches!(
                    transition.kind,
                    ForegroundJournalEventKind::WindowCreated
                        | ForegroundJournalEventKind::WindowDestroyed
                )
        });
        if latest_lifecycle
            .is_some_and(|event| event.kind == ForegroundJournalEventKind::WindowDestroyed)
        {
            return None;
        }
        let lifetime_token = match state.current_lifetimes.get(&hwnd).copied() {
            Some(token) => token,
            None => {
                state.next_lifetime_token = state.next_lifetime_token.wrapping_add(1).max(1);
                let token = state.next_lifetime_token;
                state.current_lifetimes.insert(hwnd, token);
                token
            }
        };
        Some(ForegroundWindowIdentity {
            hwnd,
            process_id,
            thread_id,
            lifetime_token,
        })
    }

    fn latest_non_root_before_observed_root(
        &self,
        cursor: u64,
        root_hwnd: usize,
        timeout: Duration,
    ) -> Option<JournalFocusChoice> {
        let deadline = Instant::now() + timeout;
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        loop {
            if !self.inner.alive.load(Ordering::Acquire)
                || state.dropped_through > cursor
                || state
                    .transitions
                    .front()
                    .is_some_and(|transition| transition.sequence > cursor.saturating_add(1))
            {
                return None;
            }

            let transitions: Vec<_> = state
                .transitions
                .iter()
                .filter(|transition| transition.sequence > cursor)
                .copied()
                .collect();
            if let Some(root_index) = transitions.iter().rposition(|transition| {
                transition.kind == ForegroundJournalEventKind::Foreground
                    && transition.hwnd == root_hwnd
            }) {
                let Some(root_identity) = transitions[root_index].identity else {
                    return Some(JournalFocusChoice::Invalidated);
                };
                if state.current_lifetimes.get(&root_identity.hwnd)
                    != Some(&root_identity.lifetime_token)
                {
                    return Some(JournalFocusChoice::Invalidated);
                }
                let latest_foreground = transitions
                    .iter()
                    .rposition(|transition| {
                        transition.kind == ForegroundJournalEventKind::Foreground
                    })
                    .and_then(|index| transitions.get(index));
                let candidate =
                    if latest_foreground.is_some_and(|transition| transition.hwnd != root_hwnd) {
                        latest_foreground
                    } else {
                        transitions[..root_index].iter().rev().find(|transition| {
                            transition.kind == ForegroundJournalEventKind::Foreground
                                && transition.hwnd != root_hwnd
                        })
                    };
                let Some(candidate) = candidate else {
                    return Some(JournalFocusChoice::NoNewerExternal);
                };
                let Some(identity) = candidate.identity else {
                    return Some(JournalFocusChoice::Invalidated);
                };
                let destroyed_after_candidate = transitions.iter().any(|transition| {
                    transition.kind == ForegroundJournalEventKind::WindowDestroyed
                        && transition.hwnd == candidate.hwnd
                        && transition.sequence > candidate.sequence
                });
                if destroyed_after_candidate
                    || state.current_lifetimes.get(&identity.hwnd) != Some(&identity.lifetime_token)
                {
                    return Some(JournalFocusChoice::Invalidated);
                }
                return Some(JournalFocusChoice::Newer(identity));
            }

            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return None;
            }
            let (next_state, wait_result) = self
                .inner
                .changed
                .wait_timeout(state, remaining)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state = next_state;
            if wait_result.timed_out() {
                return None;
            }
        }
    }

    fn still_has_lifetime(&self, identity: ForegroundWindowIdentity) -> bool {
        let state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.current_lifetimes.get(&identity.hwnd) == Some(&identity.lifetime_token)
    }

    fn newest_unowned_external_after(
        &self,
        cursor: u64,
        root_hwnds: &[usize],
        owned_foreground: &BTreeSet<usize>,
    ) -> Option<JournalFocusChoice> {
        let state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !self.inner.alive.load(Ordering::Acquire)
            || state.dropped_through > cursor
            || state
                .transitions
                .front()
                .is_some_and(|transition| transition.sequence > cursor.saturating_add(1))
        {
            return None;
        }
        let candidate = state.transitions.iter().rev().find(|transition| {
            transition.sequence > cursor
                && transition.kind == ForegroundJournalEventKind::Foreground
                && !root_hwnds.contains(&transition.hwnd)
                && !owned_foreground.contains(&transition.hwnd)
        });
        let Some(candidate) = candidate else {
            return Some(JournalFocusChoice::NoNewerExternal);
        };
        let Some(identity) = candidate.identity else {
            return Some(JournalFocusChoice::Invalidated);
        };
        if state.current_lifetimes.get(&identity.hwnd) != Some(&identity.lifetime_token)
            || state.transitions.iter().any(|transition| {
                transition.sequence > candidate.sequence
                    && transition.hwnd == identity.hwnd
                    && matches!(
                        transition.kind,
                        ForegroundJournalEventKind::WindowCreated
                            | ForegroundJournalEventKind::WindowDestroyed
                    )
            })
        {
            return Some(JournalFocusChoice::Invalidated);
        }
        Some(JournalFocusChoice::Newer(identity))
    }

    fn post_compensation_external(
        &self,
        cursor: u64,
        root_hwnds: &[usize],
        compensation: ForegroundWindowIdentity,
        timeout: Duration,
    ) -> Option<PostCompensationChoice> {
        let deadline = Instant::now() + timeout;
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        loop {
            if !self.inner.alive.load(Ordering::Acquire)
                || state.dropped_through > cursor
                || state
                    .transitions
                    .front()
                    .is_some_and(|transition| transition.sequence > cursor.saturating_add(1))
            {
                return None;
            }

            let transitions: Vec<_> = state
                .transitions
                .iter()
                .filter(|transition| transition.sequence > cursor)
                .copied()
                .collect();
            let target_index = transitions.iter().rposition(|transition| {
                transition.kind == ForegroundJournalEventKind::Foreground
                    && transition.hwnd == compensation.hwnd
            });
            if let Some(target_index) = target_index {
                let Some(target_identity) = transitions[target_index].identity else {
                    return Some(PostCompensationChoice::Invalidated);
                };
                if target_identity.lifetime_token != compensation.lifetime_token
                    || state.current_lifetimes.get(&compensation.hwnd)
                        != Some(&compensation.lifetime_token)
                {
                    return Some(PostCompensationChoice::Invalidated);
                }
                let latest_foreground = transitions
                    .iter()
                    .rposition(|transition| {
                        transition.kind == ForegroundJournalEventKind::Foreground
                    })
                    .and_then(|index| transitions.get(index));
                if let Some(latest) = latest_foreground
                    && latest.hwnd != compensation.hwnd
                {
                    if root_hwnds.contains(&latest.hwnd) {
                        return Some(PostCompensationChoice::Invalidated);
                    }
                    let Some(identity) = latest.identity else {
                        return Some(PostCompensationChoice::Invalidated);
                    };
                    if state.current_lifetimes.get(&identity.hwnd) != Some(&identity.lifetime_token)
                    {
                        return Some(PostCompensationChoice::Invalidated);
                    }
                    return Some(PostCompensationChoice::Newer(identity));
                }

                let candidate = transitions[..target_index].iter().rev().find(|transition| {
                    transition.kind == ForegroundJournalEventKind::Foreground
                        && !root_hwnds.contains(&transition.hwnd)
                        && transition.hwnd != compensation.hwnd
                });
                let Some(candidate) = candidate else {
                    return Some(PostCompensationChoice::Stable);
                };
                let Some(identity) = candidate.identity else {
                    return Some(PostCompensationChoice::Invalidated);
                };
                if transitions.iter().any(|transition| {
                    matches!(
                        transition.kind,
                        ForegroundJournalEventKind::WindowDestroyed
                            | ForegroundJournalEventKind::WindowCreated
                    ) && transition.hwnd == candidate.hwnd
                        && transition.sequence > candidate.sequence
                }) || state.current_lifetimes.get(&identity.hwnd)
                    != Some(&identity.lifetime_token)
                {
                    return Some(PostCompensationChoice::Invalidated);
                }
                return Some(PostCompensationChoice::Newer(identity));
            }

            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return None;
            }
            let (next_state, wait_result) = self
                .inner
                .changed
                .wait_timeout(state, remaining)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state = next_state;
            if wait_result.timed_out() {
                return None;
            }
        }
    }

    #[cfg(test)]
    fn record_test_foreground(&self, hwnd: usize, process_id: u32, thread_id: u32) {
        self.record_event(
            ForegroundJournalEventKind::Foreground,
            hwnd,
            Some((process_id, thread_id)),
        );
    }

    #[cfg(test)]
    fn record_test_created(&self, hwnd: usize) {
        self.record_event(ForegroundJournalEventKind::WindowCreated, hwnd, None);
    }

    #[cfg(test)]
    fn record_test_destroyed(&self, hwnd: usize) {
        self.record_event(ForegroundJournalEventKind::WindowDestroyed, hwnd, None);
    }

    #[cfg(test)]
    fn inject_before_next_cursor(&self, callback: impl FnOnce() + Send + 'static) {
        *self
            .inner
            .before_cursor_test
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(Box::new(callback));
    }
}

fn capture_native_foreground_identity(hwnd: usize) -> Option<(u32, u32)> {
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{GetWindowThreadProcessId, IsWindow};
        let hwnd = HWND(hwnd as *mut _);
        if hwnd.0.is_null() || !unsafe { IsWindow(hwnd) }.as_bool() {
            return None;
        }
        let mut process_id = 0;
        let thread_id = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
        (process_id != 0 && thread_id != 0).then_some((process_id, thread_id))
    }
    #[cfg(not(windows))]
    {
        let _ = hwnd;
        None
    }
}

#[cfg(windows)]
thread_local! {
    static ROOT_FOREGROUND_JOURNAL_CALLBACK: std::cell::RefCell<Option<ForegroundTransitionJournal>> = const { std::cell::RefCell::new(None) };
}

#[cfg(windows)]
unsafe extern "system" fn root_foreground_event_callback(
    _: windows::Win32::UI::Accessibility::HWINEVENTHOOK,
    event: u32,
    hwnd: windows::Win32::Foundation::HWND,
    object_id: i32,
    child_id: i32,
    event_thread_id: u32,
    _: u32,
) {
    if hwnd.0.is_null() {
        return;
    }
    ROOT_FOREGROUND_JOURNAL_CALLBACK.with(|slot| {
        if let Ok(journal) = slot.try_borrow()
            && let Some(journal) = journal.as_ref()
        {
            use windows::Win32::UI::WindowsAndMessaging::{
                EVENT_OBJECT_CREATE, EVENT_OBJECT_DESTROY, EVENT_SYSTEM_FOREGROUND, OBJID_WINDOW,
            };
            let hwnd = hwnd.0 as usize;
            if event == EVENT_SYSTEM_FOREGROUND {
                let identity = capture_native_foreground_identity(hwnd)
                    .filter(|(_, thread_id)| event_thread_id == 0 || *thread_id == event_thread_id);
                journal.record_event(ForegroundJournalEventKind::Foreground, hwnd, identity);
            } else if object_id == OBJID_WINDOW.0 && child_id == 0 {
                match event {
                    EVENT_OBJECT_CREATE => {
                        journal.record_event(ForegroundJournalEventKind::WindowCreated, hwnd, None);
                    }
                    EVENT_OBJECT_DESTROY => {
                        journal.record_event(
                            ForegroundJournalEventKind::WindowDestroyed,
                            hwnd,
                            None,
                        );
                    }
                    _ => {}
                }
            }
        }
    });
}

#[cfg(windows)]
fn start_root_foreground_journal() -> Option<ForegroundTransitionJournal> {
    use std::sync::mpsc::sync_channel;
    use windows::Win32::{
        System::Threading::GetCurrentThreadId,
        UI::{
            Accessibility::{SetWinEventHook, UnhookWinEvent},
            WindowsAndMessaging::{
                DispatchMessageW, EVENT_OBJECT_CREATE, EVENT_OBJECT_DESTROY,
                EVENT_SYSTEM_FOREGROUND, GetMessageW, MSG, PM_NOREMOVE, PeekMessageW,
                TranslateMessage, WINEVENT_OUTOFCONTEXT,
            },
        },
    };

    let journal = ForegroundTransitionJournal::new();
    journal.inner.alive.store(false, Ordering::Release);
    let worker = journal.clone();
    let (ready_tx, ready_rx) = sync_channel(0);
    let join = std::thread::Builder::new()
        .name("root-foreground-journal".into())
        .spawn(move || {
            use std::sync::atomic::Ordering;
            let thread_id = unsafe { GetCurrentThreadId() };
            worker
                .inner
                .hook_thread_id
                .store(thread_id, Ordering::Release);
            ROOT_FOREGROUND_JOURNAL_CALLBACK.with(|slot| {
                *slot.borrow_mut() = Some(worker.clone());
            });
            let mut queued = MSG::default();
            let _ = unsafe { PeekMessageW(&mut queued, None, 0, 0, PM_NOREMOVE) };
            let foreground_hook = unsafe {
                SetWinEventHook(
                    EVENT_SYSTEM_FOREGROUND,
                    EVENT_SYSTEM_FOREGROUND,
                    None,
                    Some(root_foreground_event_callback),
                    0,
                    0,
                    WINEVENT_OUTOFCONTEXT,
                )
            };
            let create_hook = unsafe {
                SetWinEventHook(
                    EVENT_OBJECT_CREATE,
                    EVENT_OBJECT_CREATE,
                    None,
                    Some(root_foreground_event_callback),
                    0,
                    0,
                    WINEVENT_OUTOFCONTEXT,
                )
            };
            let destroy_hook = unsafe {
                SetWinEventHook(
                    EVENT_OBJECT_DESTROY,
                    EVENT_OBJECT_DESTROY,
                    None,
                    Some(root_foreground_event_callback),
                    0,
                    0,
                    WINEVENT_OUTOFCONTEXT,
                )
            };
            let usable = !foreground_hook.0.is_null()
                && !create_hook.0.is_null()
                && !destroy_hook.0.is_null();
            worker.inner.alive.store(usable, Ordering::Release);
            let _ = ready_tx.send(usable);
            if usable && !worker.inner.cancel.load(Ordering::Acquire) {
                let mut message = MSG::default();
                while unsafe { GetMessageW(&mut message, None, 0, 0) }.0 > 0 {
                    let _ = unsafe { TranslateMessage(&message) };
                    unsafe { DispatchMessageW(&message) };
                    if worker.inner.cancel.load(Ordering::Acquire) {
                        break;
                    }
                }
            }
            for hook in [foreground_hook, create_hook, destroy_hook] {
                if !hook.0.is_null() {
                    let _ = unsafe { UnhookWinEvent(hook) };
                }
            }
            worker.inner.alive.store(false, Ordering::Release);
            worker.inner.changed.notify_all();
            ROOT_FOREGROUND_JOURNAL_CALLBACK.with(|slot| *slot.borrow_mut() = None);
        })
        .ok()?;
    match ready_rx.recv_timeout(Duration::from_secs(1)) {
        Ok(true) => Some(journal),
        Ok(false) => {
            let _ = join.join();
            None
        }
        Err(_) => {
            journal.inner.cancel.store(true, Ordering::Release);
            let thread_id = journal.inner.hook_thread_id.load(Ordering::Acquire);
            if thread_id != 0 {
                use windows::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_QUIT};
                let _ = unsafe { PostThreadMessageW(thread_id, WM_QUIT, None, None) };
            }
            drop(join);
            None
        }
    }
}

fn current_root_foreground_observation() -> Option<ForegroundObservation> {
    #[cfg(windows)]
    {
        static JOURNAL: OnceLock<Option<ForegroundTransitionJournal>> = OnceLock::new();
        let journal = JOURNAL
            .get_or_init(start_root_foreground_journal)
            .as_ref()?;
        Some(ForegroundObservation {
            journal: journal.clone(),
            cursor: journal.cursor()?,
        })
    }
    #[cfg(not(windows))]
    {
        None
    }
}

const FOREGROUND_VERIFY_DELAYS: [Duration; 4] = [
    Duration::from_millis(0),
    Duration::from_millis(10),
    Duration::from_millis(20),
    Duration::from_millis(40),
];
const RESTORE_VERIFY_DELAYS: [Duration; 5] = [
    Duration::from_millis(0),
    Duration::from_millis(10),
    Duration::from_millis(20),
    Duration::from_millis(40),
    Duration::from_millis(80),
];
const DESKTOP_TRANSITION_DELAYS: [Duration; 8] = [
    Duration::from_millis(0),
    Duration::from_millis(25),
    Duration::from_millis(50),
    Duration::from_millis(100),
    Duration::from_millis(150),
    Duration::from_millis(200),
    Duration::from_millis(250),
    Duration::from_millis(300),
];

fn activate_with(
    backend: &mut impl ActivationBackend,
    request: WindowActivationRequest,
) -> Result<(), WindowActivationError> {
    activate_with_fence(backend, request, None)
}

fn cancelled_error(hwnd: usize) -> WindowActivationError {
    WindowActivationError::new(
        WindowActivationErrorKind::Superseded,
        "Launcher activation was superseded by a newer visibility request",
    )
    .context("hwnd", hwnd.to_string())
}

fn authorize<T>(
    backend: &mut impl ActivationBackend,
    fence: Option<&WindowActivationFence>,
    hwnd: usize,
    effects: &mut ActivationEffects,
    side_effect: impl FnOnce(&mut dyn ActivationBackend) -> T,
) -> Result<T, WindowActivationError> {
    let Some(fence) = fence else {
        return Ok(side_effect(backend));
    };
    ensure_current(backend, Some(fence), hwnd)?;
    // Native activation may synchronously wait on another GUI thread. Keep it
    // outside VisibilityRevision's ordering gate so that thread can publish a
    // newer visibility request while USER32/COM is working.
    effects.native_side_effect_started = true;
    let result = side_effect(backend);
    ensure_current(backend, Some(fence), hwnd)?;
    Ok(result)
}

fn ensure_current(
    backend: &mut impl ActivationBackend,
    fence: Option<&WindowActivationFence>,
    hwnd: usize,
) -> Result<(), WindowActivationError> {
    let Some(fence) = fence else {
        return Ok(());
    };
    if !request_is_current(fence, hwnd) {
        return Err(cancelled_error(hwnd));
    }

    // IsWindow and GetWindowThreadProcessId do not synchronously message the
    // target. Keep even these checks outside the gate, then take one final
    // revision snapshot immediately before the caller proceeds.
    if !backend.is_window(hwnd) || backend.target_process(hwnd) != fence.expected_process {
        return Err(cancelled_error(hwnd));
    }
    request_is_current(fence, hwnd)
        .then_some(())
        .ok_or_else(|| cancelled_error(hwnd))
}

fn request_is_current(fence: &WindowActivationFence, hwnd: usize) -> bool {
    fence
        .revision
        .with_current(
            fence.request_revision,
            || {
                fence.visible.load(Ordering::Acquire)
                    && fence
                        .root_window
                        .is_current(hwnd, fence.root_window_generation)
            },
            || (),
        )
        .is_some()
}

fn wait_until_fenced<B: ActivationBackend>(
    backend: &mut B,
    delays: &[Duration],
    fence: Option<&WindowActivationFence>,
    hwnd: usize,
    mut condition: impl FnMut(&mut B) -> bool,
) -> Result<bool, WindowActivationError> {
    for &delay in delays {
        ensure_current(backend, fence, hwnd)?;
        if !delay.is_zero() {
            backend.pause(delay);
        }
        ensure_current(backend, fence, hwnd)?;
        let satisfied = condition(backend);
        ensure_current(backend, fence, hwnd)?;
        if satisfied {
            return Ok(true);
        }
    }
    Ok(false)
}

fn wait_for_foreground_fenced<B: ActivationBackend>(
    backend: &mut B,
    hwnd: usize,
    fence: Option<&WindowActivationFence>,
) -> Result<bool, WindowActivationError> {
    wait_until_fenced(backend, &FOREGROUND_VERIFY_DELAYS, fence, hwnd, |backend| {
        backend.foreground_window() == Some(hwnd)
    })
}

fn activate_with_fence(
    backend: &mut impl ActivationBackend,
    request: WindowActivationRequest,
    fence: Option<&WindowActivationFence>,
) -> Result<(), WindowActivationError> {
    #[cfg(not(test))]
    let observation = fence.and_then(|_| current_root_foreground_observation());
    #[cfg(test)]
    let observation = None;
    activate_with_observation(backend, request, fence, observation.as_ref())
}

fn activate_with_observation(
    backend: &mut impl ActivationBackend,
    request: WindowActivationRequest,
    fence: Option<&WindowActivationFence>,
    observation: Option<&ForegroundObservation>,
) -> Result<(), WindowActivationError> {
    if let Some(observation) = observation {
        observation.journal.track_window(request.hwnd);
    }
    let previous_foreground =
        fence.and_then(|_| capture_previous_foreground(backend, request.hwnd, observation));
    let mut effects = ActivationEffects::default();
    let result = activate_with_fence_inner(backend, request, fence, &mut effects);
    let result = match (result, fence) {
        (Ok(()), Some(fence)) => ensure_current(backend, Some(fence), request.hwnd),
        (result, _) => result,
    };
    if effects.native_side_effect_started
        && result
            .as_ref()
            .is_err_and(|error| error.kind == WindowActivationErrorKind::Superseded)
        && let Some(fence) = fence
    {
        if let RootFocusReconcileResult::Unresolved(reason) = reconcile_superseded_root_activation(
            backend,
            fence,
            request.hwnd,
            previous_foreground,
            observation,
        ) {
            tracing::warn!(
                hwnd = request.hwnd,
                reason,
                "ROOT focus reconciliation is unresolved"
            );
        }
    }
    result
}

#[derive(Default)]
struct ActivationEffects {
    native_side_effect_started: bool,
}

type PreviousForeground = ForegroundWindowIdentity;

#[derive(Clone, Copy, PartialEq, Eq)]
struct RootPresentationSnapshot {
    revision: u64,
    visible: bool,
    focus_intent: crate::visibility::RootFocusIntent,
    hwnd: usize,
    generation: u64,
}

fn capture_previous_foreground(
    backend: &mut impl ActivationBackend,
    root_hwnd: usize,
    observation: Option<&ForegroundObservation>,
) -> Option<PreviousForeground> {
    let observation = observation?;
    let hwnd = backend.foreground_window()?;
    if hwnd == root_hwnd || hwnd == 0 || !backend.is_window(hwnd) {
        return None;
    }
    let process_id = backend.target_process(hwnd);
    let thread_id = backend.window_thread(hwnd);
    observation
        .journal
        .identity_for_current_window(hwnd, process_id, thread_id, observation.cursor)
}

fn root_presentation_snapshot(fence: &WindowActivationFence) -> RootPresentationSnapshot {
    let (revision, (visible, focus_intent, (hwnd, generation))) = fence.revision.inspect(|| {
        (
            fence.visible.load(Ordering::Acquire),
            fence.revision.focus_intent(),
            fence.root_window.identity(),
        )
    });
    RootPresentationSnapshot {
        revision,
        visible,
        focus_intent,
        hwnd,
        generation,
    }
}

fn reconcile_superseded_root_activation(
    backend: &mut impl ActivationBackend,
    fence: &WindowActivationFence,
    root_hwnd: usize,
    previous_foreground: Option<PreviousForeground>,
    observation: Option<&ForegroundObservation>,
) -> RootFocusReconcileResult {
    fence.root_window.request_presentation_reconcile();
    let result = reconcile_superseded_root_activation_inner(
        backend,
        fence,
        root_hwnd,
        previous_foreground,
        observation,
    );
    if matches!(result, RootFocusReconcileResult::Unresolved(_))
        && let RootFocusReconcileResult::Unresolved(reason) = result
    {
        let latest = root_presentation_snapshot(fence);
        if !latest.visible && backend.foreground_window() == Some(latest.hwnd) {
            match release_focus_for_hidden_root(backend, fence, latest) {
                Ok(()) => return RootFocusReconcileResult::FocusReleasedByHideFallback,
                Err(fallback_reason) => {
                    tracing::warn!(
                        hwnd = latest.hwnd,
                        reason,
                        fallback_reason,
                        "ROOT remains foreground after activation reconciliation"
                    );
                }
            }
        }
        return RootFocusReconcileResult::Unresolved(reason);
    }
    result
}

fn reconcile_superseded_root_activation_inner(
    backend: &mut impl ActivationBackend,
    fence: &WindowActivationFence,
    root_hwnd: usize,
    previous_foreground: Option<PreviousForeground>,
    observation: Option<&ForegroundObservation>,
) -> RootFocusReconcileResult {
    // USER32 cannot condition SetForegroundWindow on a revision. Keep all
    // native effects outside the visibility gate, then use the independently
    // pumped journal to converge if a newer external owner was overtaken.
    let deadline = Instant::now() + ROOT_FOREGROUND_EVENT_TIMEOUT;
    let latest = root_presentation_snapshot(fence);
    if latest.visible && latest.focus_intent == crate::visibility::RootFocusIntent::ActivateRoot {
        return RootFocusReconcileResult::NewestRootActivation;
    }
    let Some(observation) = observation else {
        return RootFocusReconcileResult::Unresolved("foreground journal unavailable");
    };
    if backend.foreground_window() != Some(root_hwnd) {
        return RootFocusReconcileResult::NewerForeground;
    }
    if latest.hwnd != root_hwnd
        || !backend.is_window(root_hwnd)
        || backend.target_process(root_hwnd) != fence.expected_process
    {
        return RootFocusReconcileResult::Unresolved("stale ROOT foreground identity");
    }

    let initial_choice = observation.journal.latest_non_root_before_observed_root(
        observation.cursor,
        root_hwnd,
        deadline.saturating_duration_since(Instant::now()),
    );
    let mut target = match initial_choice {
        Some(JournalFocusChoice::Newer(identity)) => identity,
        Some(JournalFocusChoice::NoNewerExternal) => {
            let Some(previous) = previous_foreground else {
                return RootFocusReconcileResult::Unresolved("no eligible prior foreground");
            };
            previous
        }
        Some(JournalFocusChoice::Invalidated) => {
            return RootFocusReconcileResult::Unresolved("newer foreground lifetime invalidated");
        }
        None => {
            return RootFocusReconcileResult::Unresolved("foreground event coverage incomplete");
        }
    };

    let mut owned_foreground = BTreeSet::new();
    let mut attempts = 0;
    while attempts < MAX_ROOT_FOCUS_COMPENSATION_ATTEMPTS && Instant::now() < deadline {
        // Anchor the journal before the final validations. Any external
        // foreground edge after this point must be included in the post-call
        // reconciliation, even if it arrives between validation and USER32.
        let Some(compensation_cursor) = observation.journal.cursor() else {
            return RootFocusReconcileResult::Unresolved("foreground journal stopped");
        };
        let mut newest = root_presentation_snapshot(fence);
        if newest.visible && newest.focus_intent == crate::visibility::RootFocusIntent::ActivateRoot
        {
            return RootFocusReconcileResult::NewestRootActivation;
        }
        if newest.hwnd != root_hwnd {
            return RootFocusReconcileResult::Unresolved(
                "ROOT handle changed during reconciliation",
            );
        }

        let foreground = backend.foreground_window();
        if foreground != Some(root_hwnd)
            && !foreground.is_some_and(|hwnd| owned_foreground.contains(&hwnd))
        {
            return RootFocusReconcileResult::NewerForeground;
        }
        if foreground == Some(root_hwnd)
            && (!backend.is_window(root_hwnd)
                || backend.target_process(root_hwnd) != fence.expected_process)
        {
            return RootFocusReconcileResult::Unresolved(
                "ROOT HWND was replaced during activation",
            );
        }

        let root_hwnds = [root_hwnd, newest.hwnd];
        match observation.journal.newest_unowned_external_after(
            observation.cursor,
            &root_hwnds,
            &owned_foreground,
        ) {
            Some(JournalFocusChoice::Newer(identity)) => target = identity,
            Some(JournalFocusChoice::NoNewerExternal) => {}
            Some(JournalFocusChoice::Invalidated) => {
                return RootFocusReconcileResult::Unresolved(
                    "newer foreground lifetime invalidated",
                );
            }
            None => {
                return RootFocusReconcileResult::Unresolved(
                    "foreground event coverage incomplete",
                );
            }
        }
        if root_hwnds.contains(&target.hwnd)
            || !observation.journal.still_has_lifetime(target)
            || !backend.is_window(target.hwnd)
            || backend.target_process(target.hwnd) != target.process_id
            || backend.window_thread(target.hwnd) != target.thread_id
        {
            return RootFocusReconcileResult::Unresolved(
                "compensation target identity invalidated",
            );
        }
        if root_presentation_snapshot(fence) != newest {
            continue;
        }
        attempts += 1;
        let accepted = backend.set_foreground(target.hwnd);
        let after = root_presentation_snapshot(fence);
        if after.visible && after.focus_intent == crate::visibility::RootFocusIntent::ActivateRoot {
            return RootFocusReconcileResult::NewestRootActivation;
        }
        if after.hwnd != newest.hwnd {
            return RootFocusReconcileResult::Unresolved("ROOT handle changed during compensation");
        }
        newest = after;

        let actual_foreground = backend.foreground_window();
        if actual_foreground != Some(target.hwnd) {
            if actual_foreground != Some(root_hwnd)
                && !actual_foreground.is_some_and(|hwnd| owned_foreground.contains(&hwnd))
            {
                return RootFocusReconcileResult::NewerForeground;
            }
            if !accepted && actual_foreground == Some(root_hwnd) {
                continue;
            }
            return RootFocusReconcileResult::Unresolved(
                "compensation foreground was not observed",
            );
        }

        owned_foreground.insert(target.hwnd);
        let remaining = deadline.saturating_duration_since(Instant::now());
        let post = observation.journal.post_compensation_external(
            compensation_cursor,
            &root_hwnds,
            target,
            remaining,
        );
        match post {
            Some(PostCompensationChoice::Stable) => {
                if backend.foreground_window() == Some(target.hwnd)
                    && root_presentation_snapshot(fence) == newest
                {
                    return RootFocusReconcileResult::Converged;
                }
                return RootFocusReconcileResult::NewerForeground;
            }
            Some(PostCompensationChoice::Newer(identity)) => {
                if backend.foreground_window() == Some(identity.hwnd) {
                    return RootFocusReconcileResult::NewerForeground;
                }
                target = identity;
            }
            Some(PostCompensationChoice::Invalidated) => {
                return RootFocusReconcileResult::Unresolved(
                    "foreground event identity invalidated",
                );
            }
            None => {
                return RootFocusReconcileResult::Unresolved(
                    "post-compensation event coverage incomplete",
                );
            }
        }
    }
    RootFocusReconcileResult::Unresolved("bounded compensation attempts exhausted")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RootFocusReconcileResult {
    Converged,
    FocusReleasedByHideFallback,
    NewerForeground,
    NewestRootActivation,
    Unresolved(&'static str),
}

fn release_focus_for_hidden_root(
    backend: &mut impl ActivationBackend,
    fence: &WindowActivationFence,
    initial: RootPresentationSnapshot,
) -> Result<(), &'static str> {
    let deadline = Instant::now() + HIDDEN_ROOT_FALLBACK_TIMEOUT;
    let hwnd = initial.hwnd;
    let generation = initial.generation;

    let current_hidden_root = |backend: &mut dyn ActivationBackend| {
        let snapshot = root_presentation_snapshot(fence);
        if snapshot.hwnd != hwnd || snapshot.generation != generation {
            return Err("ROOT HWND generation changed during hide fallback");
        }
        if snapshot.visible {
            return Err("newer visible request superseded hide fallback");
        }
        if !fence.root_window.is_current(hwnd, generation)
            || !backend.is_window(hwnd)
            || backend.target_process(hwnd) != fence.expected_process
        {
            return Err("ROOT identity could not be fenced during hide fallback");
        }
        Ok(snapshot)
    };

    loop {
        current_hidden_root(backend)?;
        if backend.is_physically_parked(hwnd) {
            break;
        }
        if Instant::now() >= deadline {
            return Err("ROOT did not reach physically parked bounds");
        }
        backend.pause(HIDDEN_ROOT_FALLBACK_POLL);
    }

    current_hidden_root(backend)?;
    if !backend.hide_without_activation(hwnd) {
        return Err("SW_HIDE could not be issued to current ROOT");
    }

    loop {
        match current_hidden_root(backend) {
            Ok(_) => {}
            Err(reason) => {
                let _ = restore_root_drawability_if_current(
                    backend,
                    fence,
                    hwnd,
                    generation,
                    initial.revision,
                );
                fence.root_window.request_presentation_reconcile();
                return Err(reason);
            }
        }
        if !backend.is_window_visible(hwnd) && backend.foreground_window() != Some(hwnd) {
            break;
        }
        if Instant::now() >= deadline {
            if !backend.is_window_visible(hwnd) {
                let _ = restore_root_drawability_if_current(
                    backend,
                    fence,
                    hwnd,
                    generation,
                    initial.revision,
                );
            }
            fence.root_window.request_presentation_reconcile();
            return Err("ROOT hide did not release foreground before deadline");
        }
        backend.pause(HIDDEN_ROOT_FALLBACK_POLL);
    }

    if let Err(reason) = current_hidden_root(backend) {
        fence.root_window.request_presentation_reconcile();
        let _ =
            restore_root_drawability_if_current(backend, fence, hwnd, generation, initial.revision);
        return Err(reason);
    }
    if !backend.show_without_activation(hwnd) {
        fence.root_window.request_presentation_reconcile();
        return Err("SW_SHOWNOACTIVATE could not be issued to current ROOT");
    }

    loop {
        match current_hidden_root(backend) {
            Ok(_) => {}
            Err(reason) => {
                fence.root_window.request_presentation_reconcile();
                return Err(reason);
            }
        }
        if backend.is_window_visible(hwnd)
            && backend.is_physically_parked(hwnd)
            && backend.foreground_window() != Some(hwnd)
        {
            return Ok(());
        }
        if Instant::now() >= deadline {
            fence.root_window.request_presentation_reconcile();
            return Err("ROOT did not remain visible, parked, and non-foreground");
        }
        backend.pause(HIDDEN_ROOT_FALLBACK_POLL);
    }
}

fn restore_root_drawability_if_current(
    backend: &mut impl ActivationBackend,
    fence: &WindowActivationFence,
    hwnd: usize,
    generation: u64,
    minimum_revision: u64,
) -> bool {
    let current = root_presentation_snapshot(fence);
    if current.hwnd != hwnd
        || current.generation != generation
        || current.revision < minimum_revision
        || !fence.root_window.is_current(hwnd, generation)
        || !backend.is_window(hwnd)
        || backend.target_process(hwnd) != fence.expected_process
    {
        return false;
    }

    // Recovery restores a drawable surface only. Both current focus intents
    // use a no-activate show here; the revision owner handles any activation.
    match current.focus_intent {
        crate::visibility::RootFocusIntent::ActivateRoot
        | crate::visibility::RootFocusIntent::PreserveForeground => {
            backend.show_without_activation(hwnd)
        }
    }
}

fn activate_with_fence_inner(
    backend: &mut impl ActivationBackend,
    request: WindowActivationRequest,
    fence: Option<&WindowActivationFence>,
    effects: &mut ActivationEffects,
) -> Result<(), WindowActivationError> {
    if request.hwnd == 0 || !backend.is_window(request.hwnd) {
        return Err(WindowActivationError::new(
            WindowActivationErrorKind::InvalidWindow,
            "Target window no longer exists",
        )
        .context("hwnd", request.hwnd.to_string()));
    }

    prepare_desktop(backend, request, fence, effects)?;
    if backend.is_minimized(request.hwnd) {
        let restore_result = authorize(backend, fence, request.hwnd, effects, |backend| {
            backend.restore(request.hwnd)
        })?;
        if !wait_until_fenced(
            backend,
            &RESTORE_VERIFY_DELAYS,
            fence,
            request.hwnd,
            |backend| !backend.is_minimized(request.hwnd),
        )? {
            return Err(WindowActivationError::new(
                WindowActivationErrorKind::ForegroundDenied,
                "Target window did not restore before activation",
            )
            .context("hwnd", request.hwnd.to_string())
            .context("restore_request", restore_result.to_string()));
        }
    }

    let direct_result = authorize(backend, fence, request.hwnd, effects, |backend| {
        backend.set_foreground(request.hwnd)
    })?;
    if wait_for_foreground_fenced(backend, request.hwnd, fence)? {
        return Ok(());
    }

    let bring_result = authorize(backend, fence, request.hwnd, effects, |backend| {
        backend.bring_to_top(request.hwnd)
    })?;
    let top_set_result = authorize(backend, fence, request.hwnd, effects, |backend| {
        backend.set_foreground(request.hwnd)
    })?;
    if wait_for_foreground_fenced(backend, request.hwnd, fence)? {
        return Ok(());
    }

    let current_thread = backend.current_thread();
    let foreground_thread = backend
        .foreground_window()
        .map(|hwnd| backend.window_thread(hwnd))
        .unwrap_or(0);
    let target_thread = backend.window_thread(request.hwnd);
    let target_pid = backend.target_process(request.hwnd);
    let mut guard = InputAttachmentGuard::new(backend);
    let mut attach_failures = Vec::new();
    for thread in [foreground_thread, target_thread] {
        if thread != 0
            && current_thread != 0
            && thread != current_thread
            && !guard.attached.contains(&(thread, current_thread))
        {
            if let Err(error) = guard.attach(thread, current_thread, fence, request.hwnd) {
                attach_failures.push(error);
            }
        }
    }

    let attached_bring_result =
        authorize(guard.backend(), fence, request.hwnd, effects, |backend| {
            backend.bring_to_top(request.hwnd)
        })?;
    let fallback_result = authorize(guard.backend(), fence, request.hwnd, effects, |backend| {
        backend.set_foreground(request.hwnd)
    })?;
    let activated = wait_for_foreground_fenced(guard.backend(), request.hwnd, fence)?;
    let detach_failures = guard.finish();
    if !detach_failures.is_empty() {
        return Err(WindowActivationError::new(
            WindowActivationErrorKind::ForegroundDenied,
            "Window activation completed, but input queues could not be detached cleanly",
        )
        .context("hwnd", request.hwnd.to_string())
        .context("detach_failures", detach_failures.join("; "))
        .context("activation_observed", activated.to_string()));
    }
    if activated {
        return Ok(());
    }

    Err(WindowActivationError::new(
        WindowActivationErrorKind::ForegroundDenied,
        "Windows denied foreground activation after the direct and attached-input attempts",
    )
    .context("hwnd", request.hwnd.to_string())
    .context("target_pid", target_pid.to_string())
    .context("desktop_policy", format!("{:?}", request.desktop_policy))
    .context("direct_set_foreground", direct_result.to_string())
    .context("bring_to_top", bring_result.to_string())
    .context("top_set_foreground", top_set_result.to_string())
    .context("attached_bring_to_top", attached_bring_result.to_string())
    .context("fallback_set_foreground", fallback_result.to_string())
    .context("attach_failures", attach_failures.join("; "))
    .context(
        "possible_cause",
        "foreground-lock policy, elevation/UIPI mismatch, or an unresponsive target",
    ))
}

fn wait_until<B: ActivationBackend>(
    backend: &mut B,
    delays: &[Duration],
    mut condition: impl FnMut(&mut B) -> bool,
) -> bool {
    for &delay in delays {
        if !delay.is_zero() {
            backend.pause(delay);
        }
        if condition(backend) {
            return true;
        }
    }
    false
}

fn wait_for_foreground<B: ActivationBackend>(backend: &mut B, hwnd: usize) -> bool {
    wait_until(backend, &FOREGROUND_VERIFY_DELAYS, |backend| {
        backend.foreground_window() == Some(hwnd)
    })
}

struct InputAttachmentGuard<'a, B: ActivationBackend> {
    backend: &'a mut B,
    attached: Vec<(u32, u32)>,
}
impl<'a, B: ActivationBackend> InputAttachmentGuard<'a, B> {
    fn new(backend: &'a mut B) -> Self {
        Self {
            backend,
            attached: Vec::with_capacity(2),
        }
    }
    fn backend(&mut self) -> &mut B {
        self.backend
    }
    fn attach(
        &mut self,
        from: u32,
        to: u32,
        fence: Option<&WindowActivationFence>,
        hwnd: usize,
    ) -> Result<(), String> {
        ensure_current(self.backend, fence, hwnd).map_err(|error| error.to_string())?;
        self.backend.attach_input(from, to, true)?;
        // Record ownership before the post-effect fence check so Drop still
        // detaches this queue if a newer visibility request arrived meanwhile.
        self.attached.push((from, to));
        ensure_current(self.backend, fence, hwnd).map_err(|error| error.to_string())?;
        Ok(())
    }
    fn finish(mut self) -> Vec<String> {
        let mut failures = Vec::new();
        while let Some((from, to)) = self.attached.pop() {
            if let Err(error) = self.backend.attach_input(from, to, false) {
                failures.push(error);
            }
        }
        failures
    }
}
impl<B: ActivationBackend> Drop for InputAttachmentGuard<'_, B> {
    fn drop(&mut self) {
        while let Some((from, to)) = self.attached.pop() {
            let _ = self.backend.attach_input(from, to, false);
        }
    }
}

fn prepare_desktop(
    backend: &mut impl ActivationBackend,
    request: WindowActivationRequest,
    fence: Option<&WindowActivationFence>,
    effects: &mut ActivationEffects,
) -> Result<(), WindowActivationError> {
    match request.desktop_policy {
        WindowDesktopPolicy::FollowWindow => {
            let on_current = backend
                .is_window_on_current(request.hwnd)
                .map_err(|error| desktop_error(error, request))?;
            if !on_current {
                let target = backend
                    .window_desktop(request.hwnd)
                    .map_err(|error| desktop_error(error, request))?;
                authorize(backend, fence, request.hwnd, effects, |backend| {
                    backend.switch_desktop(&target)
                })?
                .map_err(|error| desktop_error(error, request))?;
                if !wait_until_fenced(
                    backend,
                    &DESKTOP_TRANSITION_DELAYS,
                    fence,
                    request.hwnd,
                    |backend| backend.is_window_on_current(request.hwnd).unwrap_or(false),
                )? {
                    return Err(WindowActivationError::new(
                        WindowActivationErrorKind::Desktop,
                        "Target desktop did not become current before activation",
                    )
                    .context("desktop_policy", format!("{:?}", request.desktop_policy)));
                }
            }
        }
        WindowDesktopPolicy::CurrentDesktopOnly => {
            if !backend
                .is_window_on_current(request.hwnd)
                .map_err(|error| desktop_error(error, request))?
            {
                return Err(WindowActivationError::new(
                    WindowActivationErrorKind::Desktop,
                    "Target window is on another virtual desktop",
                )
                .context("desktop_policy", format!("{:?}", request.desktop_policy)));
            }
        }
        WindowDesktopPolicy::MoveToCurrentDesktop => {
            let on_current = backend
                .is_window_on_current(request.hwnd)
                .map_err(|error| desktop_error(error, request))?;
            if !on_current {
                let current = backend
                    .current_desktop()
                    .map_err(|error| desktop_error(error, request))?;
                authorize(backend, fence, request.hwnd, effects, |backend| {
                    backend.move_window(request.hwnd, &current)
                })?
                .map_err(|error| desktop_error(error, request))?;
                if !wait_until_fenced(
                    backend,
                    &DESKTOP_TRANSITION_DELAYS,
                    fence,
                    request.hwnd,
                    |backend| backend.is_window_on_current(request.hwnd).unwrap_or(false),
                )? {
                    return Err(WindowActivationError::new(
                        WindowActivationErrorKind::Desktop,
                        "Target window did not reach the current desktop before activation",
                    )
                    .context("desktop_policy", format!("{:?}", request.desktop_policy)));
                }
            }
        }
    }
    Ok(())
}

fn desktop_error(
    error: VirtualDesktopError,
    request: WindowActivationRequest,
) -> WindowActivationError {
    let mut activation_error =
        WindowActivationError::new(WindowActivationErrorKind::Desktop, error.message)
            .context("desktop_operation", error.operation)
            .context("desktop_policy", format!("{:?}", request.desktop_policy));
    for (key, value) in error.context {
        activation_error = activation_error.context(key, value);
    }
    activation_error
}

#[cfg(windows)]
struct WindowsActivationBackend;

#[cfg(windows)]
fn native_window_intersects_physical_display(hwnd: usize) -> Option<bool> {
    use windows::Win32::{
        Foundation::{BOOL, HWND, LPARAM, RECT},
        Graphics::Gdi::{EnumDisplayMonitors, HDC, HMONITOR},
        UI::WindowsAndMessaging::GetWindowRect,
    };

    #[derive(Clone, Copy)]
    struct Scan {
        window: RECT,
        saw_display: bool,
        intersects: bool,
    }

    unsafe extern "system" fn visit_display(
        _monitor: HMONITOR,
        _dc: HDC,
        display_rect: *mut RECT,
        context: LPARAM,
    ) -> BOOL {
        if display_rect.is_null() {
            return BOOL(1);
        }
        let scan = unsafe { &mut *(context.0 as *mut Scan) };
        let display = unsafe { *display_rect };
        scan.saw_display = true;
        scan.intersects |= display.right > scan.window.left
            && display.left < scan.window.right
            && display.bottom > scan.window.top
            && display.top < scan.window.bottom;
        BOOL(1)
    }

    let hwnd = HWND(hwnd as *mut _);
    let mut window = RECT::default();
    if unsafe { GetWindowRect(hwnd, &mut window) }.is_err() {
        return None;
    }
    let mut scan = Scan {
        window,
        saw_display: false,
        intersects: false,
    };
    unsafe {
        let _ = EnumDisplayMonitors(
            HDC::default(),
            None,
            Some(visit_display),
            LPARAM(&mut scan as *mut _ as isize),
        );
    }
    scan.saw_display.then_some(scan.intersects)
}

#[cfg(windows)]
impl ActivationBackend for WindowsActivationBackend {
    fn is_window(&mut self, hwnd: usize) -> bool {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::IsWindow;
        unsafe { IsWindow(HWND(hwnd as *mut _)) }.as_bool()
    }

    fn window_desktop(&mut self, hwnd: usize) -> Result<VirtualDesktopId, VirtualDesktopError> {
        use windows::Win32::Foundation::HWND;
        VirtualDesktopService.desktop_for_window(HWND(hwnd as *mut _))
    }

    fn current_desktop(&mut self) -> Result<VirtualDesktopId, VirtualDesktopError> {
        VirtualDesktopService.current().map(|desktop| desktop.id)
    }

    fn is_window_on_current(&mut self, hwnd: usize) -> Result<bool, VirtualDesktopError> {
        use windows::Win32::Foundation::HWND;
        VirtualDesktopService.is_window_on_current_desktop(HWND(hwnd as *mut _))
    }

    fn switch_desktop(&mut self, desktop: &VirtualDesktopId) -> Result<(), VirtualDesktopError> {
        VirtualDesktopService.switch_to_id(desktop)
    }

    fn move_window(
        &mut self,
        hwnd: usize,
        desktop: &VirtualDesktopId,
    ) -> Result<(), VirtualDesktopError> {
        use windows::Win32::Foundation::HWND;
        VirtualDesktopService.move_window_to_desktop(HWND(hwnd as *mut _), desktop)
    }

    fn restore(&mut self, hwnd: usize) -> bool {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{SW_RESTORE, ShowWindowAsync};
        unsafe { ShowWindowAsync(HWND(hwnd as *mut _), SW_RESTORE) }.as_bool()
    }

    fn is_minimized(&mut self, hwnd: usize) -> bool {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::IsIconic;
        unsafe { IsIconic(HWND(hwnd as *mut _)) }.as_bool()
    }

    fn is_window_visible(&mut self, hwnd: usize) -> bool {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::IsWindowVisible;
        unsafe { IsWindowVisible(HWND(hwnd as *mut _)) }.as_bool()
    }

    fn is_physically_parked(&mut self, hwnd: usize) -> bool {
        native_window_intersects_physical_display(hwnd) == Some(false)
    }

    fn hide_without_activation(&mut self, hwnd: usize) -> bool {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{SW_HIDE, ShowWindowAsync};
        let hwnd = HWND(hwnd as *mut _);
        let _ = unsafe { ShowWindowAsync(hwnd, SW_HIDE) };
        self.is_window(hwnd.0 as usize)
    }

    fn show_without_activation(&mut self, hwnd: usize) -> bool {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{SW_SHOWNOACTIVATE, ShowWindowAsync};
        let hwnd = HWND(hwnd as *mut _);
        let _ = unsafe { ShowWindowAsync(hwnd, SW_SHOWNOACTIVATE) };
        self.is_window(hwnd.0 as usize)
    }

    fn foreground_window(&mut self) -> Option<usize> {
        use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
        let hwnd = unsafe { GetForegroundWindow() };
        (!hwnd.0.is_null()).then_some(hwnd.0 as usize)
    }

    fn window_thread(&mut self, hwnd: usize) -> u32 {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;
        unsafe { GetWindowThreadProcessId(HWND(hwnd as *mut _), None) }
    }

    fn current_thread(&mut self) -> u32 {
        use windows::Win32::System::Threading::GetCurrentThreadId;
        unsafe { GetCurrentThreadId() }
    }

    fn attach_input(&mut self, from: u32, to: u32, attach: bool) -> Result<(), String> {
        use windows::Win32::System::Threading::AttachThreadInput;
        if unsafe { AttachThreadInput(from, to, attach) }.as_bool() {
            Ok(())
        } else {
            Err(windows::core::Error::from_win32().to_string())
        }
    }

    fn bring_to_top(&mut self, hwnd: usize) -> bool {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::BringWindowToTop;
        unsafe { BringWindowToTop(HWND(hwnd as *mut _)) }.is_ok()
    }

    fn set_foreground(&mut self, hwnd: usize) -> bool {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow;
        unsafe { SetForegroundWindow(HWND(hwnd as *mut _)) }.as_bool()
    }

    fn target_process(&mut self, hwnd: usize) -> u32 {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;
        let mut pid = 0;
        unsafe { GetWindowThreadProcessId(HWND(hwnd as *mut _), Some(&mut pid)) };
        pid
    }

    fn pause(&mut self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeBackend {
        events: Vec<String>,
        valid: bool,
        target: VirtualDesktopId,
        current: VirtualDesktopId,
        on_current: bool,
        foreground: Option<usize>,
        root_visible: bool,
        root_parked: bool,
        foreground_after_root_hide: Option<usize>,
        fail_root_hide: bool,
        ignore_root_hide: bool,
        keep_foreground_after_root_hide: bool,
        fail_root_show: bool,
        revision_on_root_hide: Option<(crate::visibility::VisibilityRevision, Arc<AtomicBool>)>,
        root_window_reuse_on_hide: Option<crate::visibility::RootWindowBridge>,
        activate_on_attempt: usize,
        attempts: usize,
        attach_fail_at: Option<(u32, u32)>,
        detach_fail_at: Option<(u32, u32)>,
        transition_delay_after_action: usize,
        transition_checks_remaining: usize,
        minimized: bool,
        cancel_on_pause: Option<(crate::visibility::VisibilityRevision, Arc<AtomicBool>)>,
        blocked_set_foreground:
            Option<(std::sync::mpsc::Sender<()>, std::sync::mpsc::Receiver<()>)>,
        blocked_set_foreground_target: Option<usize>,
        foreground_journal: Option<ForegroundTransitionJournal>,
        external_focus_before_set: Option<usize>,
        external_focus_after_block: Option<usize>,
        external_focus_during_compensation: Option<(usize, usize)>,
        recreate_lifetime_during_compensation: Option<usize>,
        failed_foreground_attempts: BTreeMap<usize, usize>,
        recreate_lifetime_after_external: Option<usize>,
        identity_overrides: BTreeMap<usize, (u32, u32)>,
        reuse_identity_after_external: Option<(usize, u32, u32)>,
    }

    impl FakeBackend {
        fn new() -> Self {
            Self {
                events: Vec::new(),
                valid: true,
                target: id(2),
                current: id(1),
                on_current: false,
                foreground: Some(7),
                root_visible: true,
                root_parked: true,
                foreground_after_root_hide: Some(7),
                fail_root_hide: false,
                ignore_root_hide: false,
                keep_foreground_after_root_hide: false,
                fail_root_show: false,
                revision_on_root_hide: None,
                root_window_reuse_on_hide: None,
                activate_on_attempt: 1,
                attempts: 0,
                attach_fail_at: None,
                detach_fail_at: None,
                transition_delay_after_action: 0,
                transition_checks_remaining: 0,
                minimized: false,
                cancel_on_pause: None,
                blocked_set_foreground: None,
                blocked_set_foreground_target: None,
                foreground_journal: None,
                external_focus_before_set: None,
                external_focus_after_block: None,
                external_focus_during_compensation: None,
                recreate_lifetime_during_compensation: None,
                failed_foreground_attempts: BTreeMap::new(),
                recreate_lifetime_after_external: None,
                identity_overrides: BTreeMap::new(),
                reuse_identity_after_external: None,
            }
        }

        fn record_foreground(&self, hwnd: usize) {
            if let Some(journal) = &self.foreground_journal {
                let (process_id, thread_id) = self
                    .identity_overrides
                    .get(&hwnd)
                    .copied()
                    .unwrap_or((99, if hwnd == 42 { 3 } else { 2 }));
                journal.record_test_foreground(hwnd, process_id, thread_id);
            }
        }
    }

    fn id(number: u32) -> VirtualDesktopId {
        VirtualDesktopId::parse(&format!("{number:08x}-0000-0000-0000-000000000000")).unwrap()
    }

    fn hidden_root_fence(
        revision: &crate::visibility::VisibilityRevision,
        visible: &Arc<AtomicBool>,
    ) -> (WindowActivationFence, crate::visibility::RootWindowBridge) {
        let request_revision = revision
            .request(|| visible.store(false, Ordering::Release))
            .0;
        let root_window = crate::visibility::RootWindowBridge::default();
        root_window.set_identity_for_test(42);
        let (_, root_window_generation) = root_window.identity();
        (
            WindowActivationFence {
                revision: revision.clone(),
                request_revision,
                visible: Arc::clone(visible),
                expected_process: 99,
                root_window: root_window.clone(),
                root_window_generation,
            },
            root_window,
        )
    }

    impl ActivationBackend for FakeBackend {
        fn is_window(&mut self, _: usize) -> bool {
            self.events.push("validate".into());
            self.valid
        }
        fn window_desktop(&mut self, _: usize) -> Result<VirtualDesktopId, VirtualDesktopError> {
            self.events.push("target_desktop".into());
            Ok(self.target.clone())
        }
        fn current_desktop(&mut self) -> Result<VirtualDesktopId, VirtualDesktopError> {
            self.events.push("current_desktop".into());
            Ok(self.current.clone())
        }
        fn is_window_on_current(&mut self, _: usize) -> Result<bool, VirtualDesktopError> {
            self.events.push("is_current".into());
            if self.transition_checks_remaining > 0 {
                self.transition_checks_remaining -= 1;
                return Ok(false);
            }
            Ok(self.on_current)
        }
        fn switch_desktop(
            &mut self,
            desktop: &VirtualDesktopId,
        ) -> Result<(), VirtualDesktopError> {
            self.events.push(format!("switch:{desktop}"));
            self.current = desktop.clone();
            self.on_current = true;
            self.transition_checks_remaining = self.transition_delay_after_action;
            Ok(())
        }
        fn move_window(
            &mut self,
            _: usize,
            desktop: &VirtualDesktopId,
        ) -> Result<(), VirtualDesktopError> {
            self.events.push(format!("move:{desktop}"));
            self.target = desktop.clone();
            self.on_current = true;
            self.transition_checks_remaining = self.transition_delay_after_action;
            Ok(())
        }
        fn restore(&mut self, _: usize) -> bool {
            self.events.push("restore".into());
            self.minimized = false;
            true
        }
        fn is_minimized(&mut self, _: usize) -> bool {
            self.events.push("is_minimized".into());
            self.minimized
        }
        fn is_window_visible(&mut self, hwnd: usize) -> bool {
            self.events.push(format!("visible:{hwnd}"));
            self.root_visible
        }
        fn is_physically_parked(&mut self, hwnd: usize) -> bool {
            self.events.push(format!("parked:{hwnd}"));
            self.root_parked
        }
        fn hide_without_activation(&mut self, hwnd: usize) -> bool {
            self.events.push(format!("hide_noactivate:{hwnd}"));
            if self.fail_root_hide {
                return false;
            }
            if self.ignore_root_hide {
                return true;
            }
            self.root_visible = false;
            if self.foreground == Some(hwnd) && !self.keep_foreground_after_root_hide {
                self.foreground = self.foreground_after_root_hide;
                if let Some(foreground) = self.foreground {
                    self.record_foreground(foreground);
                }
            }
            if let Some(root_window) = self.root_window_reuse_on_hide.take() {
                root_window.set_identity_for_test(43);
                root_window.set_identity_for_test(hwnd);
            }
            if let Some((revision, visible)) = self.revision_on_root_hide.take() {
                revision.request_with_focus_intent(
                    crate::visibility::RootFocusIntent::PreserveForeground,
                    || visible.store(true, Ordering::Release),
                );
            }
            true
        }
        fn show_without_activation(&mut self, hwnd: usize) -> bool {
            self.events.push(format!("show_noactivate:{hwnd}"));
            if self.fail_root_show {
                return false;
            }
            self.root_visible = true;
            true
        }
        fn foreground_window(&mut self) -> Option<usize> {
            self.events.push("foreground".into());
            self.foreground
        }
        fn window_thread(&mut self, hwnd: usize) -> u32 {
            self.events.push(format!("thread:{hwnd}"));
            self.identity_overrides
                .get(&hwnd)
                .map(|(_, thread_id)| *thread_id)
                .unwrap_or(if hwnd == 42 { 3 } else { 2 })
        }
        fn current_thread(&mut self) -> u32 {
            self.events.push("thread:current".into());
            1
        }
        fn attach_input(&mut self, from: u32, to: u32, attach: bool) -> Result<(), String> {
            self.events.push(format!("attach:{from}:{to}:{attach}"));
            let failure = if attach {
                self.attach_fail_at
            } else {
                self.detach_fail_at
            };
            if failure == Some((from, to)) {
                Err(format!("attach failure {from}->{to}"))
            } else {
                Ok(())
            }
        }
        fn bring_to_top(&mut self, hwnd: usize) -> bool {
            self.events.push(format!("top:{hwnd}"));
            false
        }
        fn set_foreground(&mut self, hwnd: usize) -> bool {
            self.attempts += 1;
            self.events.push(format!("set:{hwnd}"));
            if let Some((entered, resume)) = self.blocked_set_foreground.take() {
                if self
                    .blocked_set_foreground_target
                    .is_none_or(|target| target == hwnd)
                {
                    let _ = entered.send(());
                    let _ = resume.recv_timeout(Duration::from_secs(5));
                } else {
                    self.blocked_set_foreground = Some((entered, resume));
                }
            }
            if let Some((compensation_target, external)) = self.external_focus_during_compensation
                && hwnd == compensation_target
            {
                self.external_focus_during_compensation = None;
                self.foreground = Some(external);
                self.record_foreground(external);
            }
            if self.recreate_lifetime_during_compensation == Some(hwnd)
                && let Some(journal) = &self.foreground_journal
            {
                self.recreate_lifetime_during_compensation = None;
                journal.record_test_destroyed(hwnd);
                journal.record_test_created(hwnd);
            }
            if let Some(remaining) = self.failed_foreground_attempts.get_mut(&hwnd)
                && *remaining > 0
            {
                *remaining -= 1;
                return false;
            }
            if self.attempts >= self.activate_on_attempt {
                if let Some(external) = self.external_focus_before_set.take() {
                    self.foreground = Some(external);
                    self.record_foreground(external);
                    if self.recreate_lifetime_after_external == Some(external)
                        && let Some(journal) = &self.foreground_journal
                    {
                        journal.record_test_destroyed(external);
                        journal.record_test_created(external);
                        self.recreate_lifetime_after_external = None;
                    }
                    if let Some((hwnd, process_id, thread_id)) =
                        self.reuse_identity_after_external.take()
                    {
                        self.identity_overrides
                            .insert(hwnd, (process_id, thread_id));
                    }
                }
                self.foreground = Some(hwnd);
                self.record_foreground(hwnd);
                if let Some(external) = self.external_focus_after_block.take() {
                    self.foreground = Some(external);
                    self.record_foreground(external);
                }
                true
            } else {
                false
            }
        }
        fn target_process(&mut self, hwnd: usize) -> u32 {
            self.identity_overrides
                .get(&hwnd)
                .map(|(process_id, _)| *process_id)
                .unwrap_or(99)
        }
        fn pause(&mut self, duration: Duration) {
            self.events.push(format!("pause:{}", duration.as_millis()));
            if let Some((revision, visible)) = self.cancel_on_pause.take() {
                revision.request(|| visible.store(false, Ordering::SeqCst));
            }
        }
    }

    #[test]
    fn follow_window_switches_then_activates_without_moving() {
        let mut backend = FakeBackend::new();
        activate_with(&mut backend, WindowActivationRequest::follow_window(42)).unwrap();
        assert_eq!(
            &backend.events[..6],
            [
                "validate",
                "is_current",
                "target_desktop",
                &format!("switch:{}", id(2)),
                "is_current",
                "is_minimized"
            ]
        );
        assert!(
            !backend
                .events
                .iter()
                .any(|event| event.starts_with("move:"))
        );
    }

    #[test]
    fn newer_root_visibility_request_cancels_activation_after_desktop_wait() {
        let mut backend = FakeBackend::new();
        backend.transition_delay_after_action = 1;
        let revision = crate::visibility::VisibilityRevision::default();
        let visible = Arc::new(AtomicBool::new(true));
        let request_revision = revision.request(|| ()).0;
        let root_window = crate::visibility::RootWindowBridge::default();
        root_window.set_identity_for_test(42);
        let (_, root_window_generation) = root_window.identity();
        backend.cancel_on_pause = Some((revision.clone(), Arc::clone(&visible)));
        let fence = WindowActivationFence {
            revision,
            request_revision,
            visible: Arc::clone(&visible),
            expected_process: 99,
            root_window,
            root_window_generation,
        };

        let error = activate_with_fence(
            &mut backend,
            WindowActivationRequest::follow_window(42),
            Some(&fence),
        )
        .unwrap_err();

        assert_eq!(error.kind, WindowActivationErrorKind::Superseded);
        assert!(!visible.load(Ordering::SeqCst));
        assert!(
            backend
                .events
                .iter()
                .any(|event| event.starts_with("switch:"))
        );
        assert!(!backend.events.iter().any(|event| event.starts_with("set:")));
        assert!(!backend.events.iter().any(|event| event.starts_with("top:")));
    }

    #[test]
    fn blocked_native_activation_does_not_block_root_inspection_or_new_requests() {
        use std::sync::mpsc;

        let mut backend = FakeBackend::new();
        backend.on_current = true;
        let journal = ForegroundTransitionJournal::new();
        backend.foreground_journal = Some(journal.clone());
        let observation = ForegroundObservation {
            cursor: journal.cursor().unwrap(),
            journal: journal.clone(),
        };
        let (effect_started_tx, effect_started_rx) = mpsc::channel();
        let (resume_effect_tx, resume_effect_rx) = mpsc::channel();
        backend.blocked_set_foreground = Some((effect_started_tx, resume_effect_rx));

        let revision = crate::visibility::VisibilityRevision::default();
        let visible = Arc::new(AtomicBool::new(true));
        let request_revision = revision.request(|| ()).0;
        let root_window = crate::visibility::RootWindowBridge::default();
        root_window.set_identity_for_test(42);
        let (_, root_window_generation) = root_window.identity();
        let fence = WindowActivationFence {
            revision: revision.clone(),
            request_revision,
            visible: Arc::clone(&visible),
            expected_process: 99,
            root_window: root_window.clone(),
            root_window_generation,
        };

        let activation = std::thread::spawn(move || {
            let result = activate_with_observation(
                &mut backend,
                WindowActivationRequest::follow_window(42),
                Some(&fence),
                Some(&observation),
            );
            (result, backend.foreground, backend.events)
        });
        effect_started_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("fake foreground activation should reach its blocking native effect");

        let revision_for_update = revision.clone();
        let visible_for_update = Arc::clone(&visible);
        let (update_finished_tx, update_finished_rx) = mpsc::channel();
        let update = std::thread::spawn(move || {
            let inspected =
                revision_for_update.inspect(|| visible_for_update.load(Ordering::Acquire));
            let (new_revision, was_visible) =
                revision_for_update.request(|| visible_for_update.swap(false, Ordering::AcqRel));
            let _ = update_finished_tx.send((inspected, new_revision, was_visible));
        });

        let update_result = update_finished_rx.recv_timeout(Duration::from_secs(1));
        let _ = resume_effect_tx.send(());
        update.join().expect("visibility inspection/request thread");
        let (result, final_foreground, events) = activation.join().expect("activation worker");
        let (inspected, newer_revision, was_visible) = update_result
            .expect("inspection and newer visibility request should proceed during activation");

        assert!(inspected.1);
        assert!(was_visible);
        assert!(newer_revision > request_revision);
        assert!(!visible.load(Ordering::Acquire));
        assert_eq!(
            result.unwrap_err().kind,
            WindowActivationErrorKind::Superseded
        );
        assert_eq!(final_foreground, Some(7));
        assert!(
            events.iter().any(|event| event == "set:7"),
            "stale ROOT focus must return to the foreground captured before activation"
        );
        assert!(root_window.take_presentation_reconcile_request());
    }

    #[test]
    fn stale_root_activation_reconciles_the_newest_a_b_c_focus_intent() {
        use std::sync::mpsc;

        for (latest_intent, expected_foreground) in [
            (
                crate::visibility::RootFocusIntent::PreserveForeground,
                Some(7),
            ),
            (crate::visibility::RootFocusIntent::ActivateRoot, Some(42)),
        ] {
            let mut backend = FakeBackend::new();
            backend.on_current = true;
            let journal = ForegroundTransitionJournal::new();
            backend.foreground_journal = Some(journal.clone());
            let observation = ForegroundObservation {
                cursor: journal.cursor().unwrap(),
                journal,
            };
            if latest_intent == crate::visibility::RootFocusIntent::ActivateRoot {
                // A returns without foreground; the coalesced current C
                // activation must establish the newest native focus state.
                backend.activate_on_attempt = 2;
            }
            let (effect_started_tx, effect_started_rx) = mpsc::channel();
            let (resume_effect_tx, resume_effect_rx) = mpsc::channel();
            backend.blocked_set_foreground = Some((effect_started_tx, resume_effect_rx));

            let revision = crate::visibility::VisibilityRevision::default();
            let visible = Arc::new(AtomicBool::new(true));
            let request_revision = revision.request(|| ()).0;
            let root_window = crate::visibility::RootWindowBridge::default();
            root_window.set_identity_for_test(42);
            let (_, root_window_generation) = root_window.identity();
            let fence = WindowActivationFence {
                revision: revision.clone(),
                request_revision,
                visible: Arc::clone(&visible),
                expected_process: 99,
                root_window: root_window.clone(),
                root_window_generation,
            };

            let activation = std::thread::spawn(move || {
                let result = activate_with_observation(
                    &mut backend,
                    WindowActivationRequest::follow_window(42),
                    Some(&fence),
                    Some(&observation),
                );
                (result, backend)
            });
            effect_started_rx
                .recv_timeout(Duration::from_secs(1))
                .expect("activation must block in its first native foreground effect");

            revision.request(|| visible.store(false, Ordering::Release));
            revision.request_with_focus_intent(latest_intent, || {
                visible.store(true, Ordering::Release)
            });
            let latest_revision = revision.current();
            let _ = resume_effect_tx.send(());

            let (result, mut backend) = activation.join().expect("activation worker");
            assert_eq!(
                result.unwrap_err().kind,
                WindowActivationErrorKind::Superseded
            );
            assert!(
                root_window.take_presentation_reconcile_request(),
                "newest request {latest_revision} must be reconciled by the ROOT GUI owner"
            );
            if latest_intent == crate::visibility::RootFocusIntent::ActivateRoot {
                let current_fence = WindowActivationFence {
                    revision: revision.clone(),
                    request_revision: latest_revision,
                    visible: Arc::clone(&visible),
                    expected_process: 99,
                    root_window: root_window.clone(),
                    root_window_generation,
                };
                // Model the coalescing restore queue's C operation after A
                // returns, preserving C's revision and native trace identity.
                activate_with_fence(
                    &mut backend,
                    WindowActivationRequest::follow_window(42),
                    Some(&current_fence),
                )
                .unwrap();
            }
            assert_eq!(backend.foreground, expected_foreground);
            match latest_intent {
                crate::visibility::RootFocusIntent::PreserveForeground => assert!(
                    backend.events.iter().any(|event| event == "set:7"),
                    "A→B→C PreserveForeground should return focus to its pre-A owner"
                ),
                crate::visibility::RootFocusIntent::ActivateRoot => assert!(
                    backend
                        .events
                        .iter()
                        .filter(|event| *event == "set:42")
                        .count()
                        == 2,
                    "A must be superseded, then the queued C activation must focus ROOT"
                ),
            }
        }
    }

    #[test]
    fn stale_root_activation_does_not_steal_focus_from_a_new_external_window() {
        use std::sync::mpsc;

        let mut backend = FakeBackend::new();
        backend.on_current = true;
        let journal = ForegroundTransitionJournal::new();
        backend.foreground_journal = Some(journal.clone());
        let observation = ForegroundObservation {
            cursor: journal.cursor().unwrap(),
            journal,
        };
        backend.external_focus_after_block = Some(8);
        let (effect_started_tx, effect_started_rx) = mpsc::channel();
        let (resume_effect_tx, resume_effect_rx) = mpsc::channel();
        backend.blocked_set_foreground = Some((effect_started_tx, resume_effect_rx));

        let revision = crate::visibility::VisibilityRevision::default();
        let visible = Arc::new(AtomicBool::new(true));
        let request_revision = revision.request(|| ()).0;
        let root_window = crate::visibility::RootWindowBridge::default();
        root_window.set_identity_for_test(42);
        let (_, root_window_generation) = root_window.identity();
        let fence = WindowActivationFence {
            revision: revision.clone(),
            request_revision,
            visible: Arc::clone(&visible),
            expected_process: 99,
            root_window: root_window.clone(),
            root_window_generation,
        };

        let activation = std::thread::spawn(move || {
            let result = activate_with_observation(
                &mut backend,
                WindowActivationRequest::follow_window(42),
                Some(&fence),
                Some(&observation),
            );
            (result, backend.foreground, backend.events)
        });
        effect_started_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("activation must block in its first native foreground effect");

        revision.request(|| visible.store(false, Ordering::Release));
        let _ = resume_effect_tx.send(());
        let (result, final_foreground, events) = activation.join().expect("activation worker");

        assert_eq!(
            result.unwrap_err().kind,
            WindowActivationErrorKind::Superseded
        );
        assert_eq!(final_foreground, Some(8));
        assert!(
            !events.iter().any(|event| event == "set:7"),
            "compensation must leave a newer external foreground owner alone"
        );
        assert!(root_window.take_presentation_reconcile_request());
    }

    #[test]
    fn stale_root_activation_restores_latest_external_foreground_seen_during_effect() {
        use std::sync::mpsc;

        let mut backend = FakeBackend::new();
        backend.on_current = true;
        backend.external_focus_before_set = Some(8);
        let journal = ForegroundTransitionJournal::new();
        backend.foreground_journal = Some(journal.clone());
        let observation = ForegroundObservation {
            cursor: journal.cursor().unwrap(),
            journal,
        };
        let (effect_started_tx, effect_started_rx) = mpsc::channel();
        let (resume_effect_tx, resume_effect_rx) = mpsc::channel();
        backend.blocked_set_foreground = Some((effect_started_tx, resume_effect_rx));

        let revision = crate::visibility::VisibilityRevision::default();
        let visible = Arc::new(AtomicBool::new(true));
        let request_revision = revision.request(|| ()).0;
        let root_window = crate::visibility::RootWindowBridge::default();
        root_window.set_identity_for_test(42);
        let (_, root_window_generation) = root_window.identity();
        let fence = WindowActivationFence {
            revision: revision.clone(),
            request_revision,
            visible: Arc::clone(&visible),
            expected_process: 99,
            root_window: root_window.clone(),
            root_window_generation,
        };

        let activation = std::thread::spawn(move || {
            let result = activate_with_observation(
                &mut backend,
                WindowActivationRequest::follow_window(42),
                Some(&fence),
                Some(&observation),
            );
            (result, backend)
        });
        effect_started_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("activation must block before its delayed foreground effect");

        revision.request(|| visible.store(false, Ordering::Release));
        revision.request_with_focus_intent(
            crate::visibility::RootFocusIntent::PreserveForeground,
            || visible.store(true, Ordering::Release),
        );
        let _ = resume_effect_tx.send(());

        let (result, backend) = activation.join().expect("activation worker");
        assert_eq!(
            result.unwrap_err().kind,
            WindowActivationErrorKind::Superseded
        );
        assert_eq!(backend.foreground, Some(8));
        assert!(backend.events.iter().any(|event| event == "set:8"));
        assert!(
            !backend.events.iter().any(|event| event == "set:7"),
            "the focus target observed after A began must supersede pre-A focus"
        );
        assert!(root_window.take_presentation_reconcile_request());
    }

    #[test]
    fn stale_root_activation_does_not_fall_back_after_observed_external_hwnd_reuse() {
        use std::sync::mpsc;

        let mut backend = FakeBackend::new();
        backend.on_current = true;
        backend.external_focus_before_set = Some(8);
        backend.reuse_identity_after_external = Some((8, 100, 20));
        let journal = ForegroundTransitionJournal::new();
        backend.foreground_journal = Some(journal.clone());
        let observation = ForegroundObservation {
            cursor: journal.cursor().unwrap(),
            journal,
        };
        let (effect_started_tx, effect_started_rx) = mpsc::channel();
        let (resume_effect_tx, resume_effect_rx) = mpsc::channel();
        backend.blocked_set_foreground = Some((effect_started_tx, resume_effect_rx));

        let revision = crate::visibility::VisibilityRevision::default();
        let visible = Arc::new(AtomicBool::new(true));
        let request_revision = revision.request(|| ()).0;
        let root_window = crate::visibility::RootWindowBridge::default();
        root_window.set_identity_for_test(42);
        let (_, root_window_generation) = root_window.identity();
        let fence = WindowActivationFence {
            revision: revision.clone(),
            request_revision,
            visible: Arc::clone(&visible),
            expected_process: 99,
            root_window: root_window.clone(),
            root_window_generation,
        };

        let activation = std::thread::spawn(move || {
            let result = activate_with_observation(
                &mut backend,
                WindowActivationRequest::follow_window(42),
                Some(&fence),
                Some(&observation),
            );
            (result, backend)
        });
        effect_started_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("activation must block before its delayed foreground effect");
        revision.request(|| visible.store(false, Ordering::Release));
        revision.request_with_focus_intent(
            crate::visibility::RootFocusIntent::PreserveForeground,
            || visible.store(true, Ordering::Release),
        );
        let _ = resume_effect_tx.send(());

        let (result, backend) = activation.join().expect("activation worker");
        assert_eq!(
            result.unwrap_err().kind,
            WindowActivationErrorKind::Superseded
        );
        assert_eq!(backend.foreground, Some(42));
        assert!(
            !backend.events.iter().any(|event| event == "set:8"),
            "the journaled HWND was reused and must not be activated"
        );
        assert!(
            !backend.events.iter().any(|event| event == "set:7"),
            "do not fall back to a stale pre-A target after a newer HWND was reused"
        );
    }

    #[test]
    fn compensation_converges_to_external_focus_that_arrives_during_set_foreground() {
        let mut backend = FakeBackend::new();
        backend.foreground = Some(42);
        backend.external_focus_during_compensation = Some((8, 9));
        let journal = ForegroundTransitionJournal::new();
        let cursor = journal.cursor().unwrap();
        journal.record_test_foreground(8, 99, 2);
        journal.record_test_foreground(42, 99, 3);
        backend.foreground_journal = Some(journal.clone());
        let observation = ForegroundObservation { journal, cursor };
        let previous = observation
            .journal
            .identity_for_current_window(7, 99, 2, cursor)
            .unwrap();
        let revision = crate::visibility::VisibilityRevision::default();
        let visible = Arc::new(AtomicBool::new(false));
        let (fence, _) = hidden_root_fence(&revision, &visible);

        let outcome = reconcile_superseded_root_activation(
            &mut backend,
            &fence,
            42,
            Some(previous),
            Some(&observation),
        );

        assert_eq!(outcome, RootFocusReconcileResult::Converged);
        assert_eq!(backend.foreground, Some(9));
        assert!(backend.events.iter().any(|event| event == "set:8"));
        assert!(backend.events.iter().any(|event| event == "set:9"));
    }

    #[test]
    fn external_focus_arriving_immediately_before_compensation_cursor_is_preserved() {
        let mut backend = FakeBackend::new();
        backend.foreground = Some(42);
        let journal = ForegroundTransitionJournal::new();
        let cursor = journal.cursor().unwrap();
        journal.record_test_foreground(8, 99, 2);
        journal.record_test_foreground(42, 99, 3);
        backend.foreground_journal = Some(journal.clone());
        let observation = ForegroundObservation { journal, cursor };
        let previous = observation
            .journal
            .identity_for_current_window(7, 99, 2, cursor)
            .unwrap();
        let revision = crate::visibility::VisibilityRevision::default();
        let visible = Arc::new(AtomicBool::new(false));
        let (fence, _) = hidden_root_fence(&revision, &visible);
        let journal_for_edge = observation.journal.clone();
        observation
            .journal
            .inject_before_next_cursor(move || journal_for_edge.record_test_foreground(9, 99, 2));

        let outcome = reconcile_superseded_root_activation(
            &mut backend,
            &fence,
            42,
            Some(previous),
            Some(&observation),
        );

        assert_eq!(outcome, RootFocusReconcileResult::Converged);
        assert_eq!(backend.foreground, Some(9));
        assert!(backend.events.iter().any(|event| event == "set:9"));
        assert!(!backend.events.iter().any(|event| event == "set:8"));
    }

    #[test]
    fn unrelated_lifecycle_churn_does_not_evict_tracked_focus_evidence() {
        let mut backend = FakeBackend::new();
        backend.foreground = Some(42);
        let journal = ForegroundTransitionJournal::new();
        let cursor = journal.cursor().unwrap();
        journal.record_test_foreground(8, 99, 2);
        journal.record_test_foreground(42, 99, 3);
        for hwnd in 10_000..10_400 {
            journal.record_test_created(hwnd);
            journal.record_test_destroyed(hwnd);
        }
        backend.foreground_journal = Some(journal.clone());
        let observation = ForegroundObservation { journal, cursor };
        let previous = observation
            .journal
            .identity_for_current_window(7, 99, 2, cursor)
            .unwrap();
        let revision = crate::visibility::VisibilityRevision::default();
        let visible = Arc::new(AtomicBool::new(false));
        let (fence, _) = hidden_root_fence(&revision, &visible);

        let outcome = reconcile_superseded_root_activation(
            &mut backend,
            &fence,
            42,
            Some(previous),
            Some(&observation),
        );

        assert_eq!(outcome, RootFocusReconcileResult::Converged);
        assert_eq!(backend.foreground, Some(8));
        assert!(backend.events.iter().any(|event| event == "set:8"));
    }

    #[test]
    fn preserve_foreground_revision_converges_after_external_focus_during_blocked_compensation() {
        use std::sync::mpsc;

        let mut backend = FakeBackend::new();
        backend.foreground = Some(42);
        backend.blocked_set_foreground_target = Some(8);
        let journal = ForegroundTransitionJournal::new();
        let cursor = journal.cursor().unwrap();
        journal.record_test_foreground(8, 99, 2);
        journal.record_test_foreground(42, 99, 3);
        backend.foreground_journal = Some(journal.clone());
        let observation = ForegroundObservation { journal, cursor };
        let previous = observation
            .journal
            .identity_for_current_window(7, 99, 2, cursor)
            .unwrap();
        let (effect_started_tx, effect_started_rx) = mpsc::channel();
        let (resume_effect_tx, resume_effect_rx) = mpsc::channel();
        backend.blocked_set_foreground = Some((effect_started_tx, resume_effect_rx));

        let revision = crate::visibility::VisibilityRevision::default();
        let visible = Arc::new(AtomicBool::new(false));
        let (fence, _) = hidden_root_fence(&revision, &visible);
        let observation_for_worker = observation.clone();
        let activation = std::thread::spawn(move || {
            let result = reconcile_superseded_root_activation(
                &mut backend,
                &fence,
                42,
                Some(previous),
                Some(&observation_for_worker),
            );
            (result, backend)
        });
        effect_started_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("compensation should block at its first SetForeground call");

        revision.request_with_focus_intent(
            crate::visibility::RootFocusIntent::PreserveForeground,
            || visible.store(true, Ordering::Release),
        );
        observation.journal.record_test_foreground(9, 99, 2);
        let _ = resume_effect_tx.send(());

        let (outcome, backend) = activation.join().expect("compensation worker");
        assert_eq!(outcome, RootFocusReconcileResult::Converged);
        assert_eq!(backend.foreground, Some(9));
        assert!(backend.events.iter().any(|event| event == "set:8"));
        assert!(backend.events.iter().any(|event| event == "set:9"));
    }

    #[test]
    fn same_pid_and_thread_hwnd_reuse_invalidates_journaled_external_target() {
        let mut backend = FakeBackend::new();
        backend.foreground = Some(42);
        let journal = ForegroundTransitionJournal::new();
        let cursor = journal.cursor().unwrap();
        journal.record_test_foreground(8, 99, 2);
        journal.record_test_destroyed(8);
        journal.record_test_created(8);
        journal.record_test_foreground(42, 99, 3);
        backend.foreground_journal = Some(journal.clone());
        let observation = ForegroundObservation { journal, cursor };
        let previous = observation
            .journal
            .identity_for_current_window(7, 99, 2, cursor)
            .unwrap();
        let revision = crate::visibility::VisibilityRevision::default();
        let visible = Arc::new(AtomicBool::new(false));
        let (fence, _) = hidden_root_fence(&revision, &visible);

        let outcome = reconcile_superseded_root_activation(
            &mut backend,
            &fence,
            42,
            Some(previous),
            Some(&observation),
        );

        assert_eq!(
            outcome,
            RootFocusReconcileResult::FocusReleasedByHideFallback
        );
        assert_eq!(backend.foreground, Some(7));
        assert!(backend.root_visible);
        assert!(backend.root_parked);
        assert!(!backend.events.iter().any(|event| event == "set:8"));
        assert!(!backend.events.iter().any(|event| event == "set:7"));
    }

    #[test]
    fn target_destroyed_during_compensation_is_not_reported_as_converged() {
        let mut backend = FakeBackend::new();
        backend.foreground = Some(42);
        backend.recreate_lifetime_during_compensation = Some(8);
        let journal = ForegroundTransitionJournal::new();
        let cursor = journal.cursor().unwrap();
        journal.record_test_foreground(8, 99, 2);
        journal.record_test_foreground(42, 99, 3);
        backend.foreground_journal = Some(journal.clone());
        let observation = ForegroundObservation { journal, cursor };
        let previous = observation
            .journal
            .identity_for_current_window(7, 99, 2, cursor)
            .unwrap();
        let revision = crate::visibility::VisibilityRevision::default();
        let visible = Arc::new(AtomicBool::new(false));
        let (fence, _) = hidden_root_fence(&revision, &visible);

        let outcome = reconcile_superseded_root_activation(
            &mut backend,
            &fence,
            42,
            Some(previous),
            Some(&observation),
        );

        assert_eq!(
            outcome,
            RootFocusReconcileResult::Unresolved("foreground event identity invalidated")
        );
        assert_eq!(backend.foreground, Some(8));
        assert!(backend.events.iter().any(|event| event == "set:8"));
        assert!(!backend.events.iter().any(|event| event == "set:7"));
    }

    #[test]
    fn denied_compensation_uses_bounded_hidden_root_focus_release() {
        let mut backend = FakeBackend::new();
        backend.foreground = Some(42);
        backend
            .failed_foreground_attempts
            .insert(7, MAX_ROOT_FOCUS_COMPENSATION_ATTEMPTS);
        let journal = ForegroundTransitionJournal::new();
        let cursor = journal.cursor().unwrap();
        journal.record_test_foreground(42, 99, 3);
        backend.foreground_journal = Some(journal.clone());
        let observation = ForegroundObservation { journal, cursor };
        let previous = observation
            .journal
            .identity_for_current_window(7, 99, 2, cursor)
            .unwrap();
        let revision = crate::visibility::VisibilityRevision::default();
        let visible = Arc::new(AtomicBool::new(false));
        let (fence, _) = hidden_root_fence(&revision, &visible);

        let outcome = reconcile_superseded_root_activation(
            &mut backend,
            &fence,
            42,
            Some(previous),
            Some(&observation),
        );

        assert_eq!(
            outcome,
            RootFocusReconcileResult::FocusReleasedByHideFallback
        );
        assert_eq!(backend.foreground, Some(7));
        assert!(backend.root_visible);
        assert!(backend.root_parked);
        assert_eq!(
            backend
                .events
                .iter()
                .filter(|event| *event == "set:7")
                .count(),
            MAX_ROOT_FOCUS_COMPENSATION_ATTEMPTS
        );
        assert!(
            backend
                .events
                .iter()
                .any(|event| event == "hide_noactivate:42")
        );
        assert!(
            backend
                .events
                .iter()
                .any(|event| event == "show_noactivate:42")
        );
    }

    #[test]
    fn hide_fallback_stops_and_reshows_root_when_preserve_request_supersedes_it() {
        let mut backend = FakeBackend::new();
        backend.foreground = Some(42);
        backend
            .failed_foreground_attempts
            .insert(7, MAX_ROOT_FOCUS_COMPENSATION_ATTEMPTS);
        let journal = ForegroundTransitionJournal::new();
        let cursor = journal.cursor().unwrap();
        journal.record_test_foreground(42, 99, 3);
        backend.foreground_journal = Some(journal.clone());
        let observation = ForegroundObservation { journal, cursor };
        let previous = observation
            .journal
            .identity_for_current_window(7, 99, 2, cursor)
            .unwrap();
        let revision = crate::visibility::VisibilityRevision::default();
        let visible = Arc::new(AtomicBool::new(false));
        let (fence, root_window) = hidden_root_fence(&revision, &visible);
        backend.revision_on_root_hide = Some((revision.clone(), Arc::clone(&visible)));

        let outcome = reconcile_superseded_root_activation(
            &mut backend,
            &fence,
            42,
            Some(previous),
            Some(&observation),
        );

        assert_eq!(
            outcome,
            RootFocusReconcileResult::Unresolved("bounded compensation attempts exhausted")
        );
        assert!(visible.load(Ordering::Acquire));
        assert_eq!(backend.foreground, Some(7));
        assert!(backend.root_visible);
        assert!(
            backend
                .events
                .iter()
                .any(|event| event == "hide_noactivate:42")
        );
        assert!(
            backend
                .events
                .iter()
                .any(|event| event == "show_noactivate:42")
        );
        assert!(root_window.take_presentation_reconcile_request());
    }

    #[test]
    fn hide_fallback_timeout_remains_unresolved() {
        let mut backend = FakeBackend::new();
        backend.foreground = Some(42);
        backend.keep_foreground_after_root_hide = true;
        backend
            .failed_foreground_attempts
            .insert(7, MAX_ROOT_FOCUS_COMPENSATION_ATTEMPTS);
        let journal = ForegroundTransitionJournal::new();
        let cursor = journal.cursor().unwrap();
        journal.record_test_foreground(42, 99, 3);
        backend.foreground_journal = Some(journal.clone());
        let observation = ForegroundObservation { journal, cursor };
        let previous = observation
            .journal
            .identity_for_current_window(7, 99, 2, cursor)
            .unwrap();
        let revision = crate::visibility::VisibilityRevision::default();
        let visible = Arc::new(AtomicBool::new(false));
        let (fence, _) = hidden_root_fence(&revision, &visible);
        let started = Instant::now();

        let outcome = reconcile_superseded_root_activation(
            &mut backend,
            &fence,
            42,
            Some(previous),
            Some(&observation),
        );

        assert_eq!(
            outcome,
            RootFocusReconcileResult::Unresolved("bounded compensation attempts exhausted")
        );
        assert!(started.elapsed() >= HIDDEN_ROOT_FALLBACK_TIMEOUT);
        assert!(started.elapsed() < HIDDEN_ROOT_FALLBACK_TIMEOUT + Duration::from_secs(1));
        assert_eq!(backend.foreground, Some(42));
        assert!(backend.root_visible);
        assert!(
            backend
                .events
                .iter()
                .any(|event| event == "show_noactivate:42"),
            "timeout recovery should keep the current ROOT drawable"
        );
    }

    #[test]
    fn hide_fallback_does_not_reshow_reused_root_hwnd() {
        let mut backend = FakeBackend::new();
        backend.foreground = Some(42);
        backend
            .failed_foreground_attempts
            .insert(7, MAX_ROOT_FOCUS_COMPENSATION_ATTEMPTS);
        let journal = ForegroundTransitionJournal::new();
        let cursor = journal.cursor().unwrap();
        journal.record_test_foreground(42, 99, 3);
        backend.foreground_journal = Some(journal.clone());
        let observation = ForegroundObservation { journal, cursor };
        let previous = observation
            .journal
            .identity_for_current_window(7, 99, 2, cursor)
            .unwrap();
        let revision = crate::visibility::VisibilityRevision::default();
        let visible = Arc::new(AtomicBool::new(false));
        let (fence, root_window) = hidden_root_fence(&revision, &visible);
        let (_, original_generation) = root_window.identity();
        backend.root_window_reuse_on_hide = Some(root_window.clone());

        let outcome = reconcile_superseded_root_activation(
            &mut backend,
            &fence,
            42,
            Some(previous),
            Some(&observation),
        );

        assert_eq!(
            outcome,
            RootFocusReconcileResult::Unresolved("bounded compensation attempts exhausted")
        );
        assert_eq!(root_window.identity(), (42, original_generation + 2));
        assert!(!backend.root_visible);
        assert_eq!(backend.foreground, Some(7));
        assert!(
            !backend
                .events
                .iter()
                .any(|event| event == "show_noactivate:42"),
            "numeric HWND reuse must not receive stale hide recovery"
        );
        assert!(root_window.take_presentation_reconcile_request());
    }

    #[test]
    fn missing_or_overflowed_hook_events_fail_closed_with_a_deadline() {
        let journal = ForegroundTransitionJournal::new();
        let cursor = journal.cursor().unwrap();
        let started = Instant::now();
        assert_eq!(
            journal.latest_non_root_before_observed_root(cursor, 42, Duration::from_millis(10)),
            None
        );
        assert!(started.elapsed() < Duration::from_millis(250));

        let overflowed = ForegroundTransitionJournal::new();
        let old_cursor = overflowed.cursor().unwrap();
        for hwnd in 100..(100 + MAX_ROOT_FOREGROUND_TRANSITIONS as usize + 1) {
            overflowed.record_test_foreground(hwnd, 99, 2);
        }
        assert_eq!(
            overflowed.latest_non_root_before_observed_root(
                old_cursor,
                42,
                Duration::from_millis(10)
            ),
            None,
            "event loss must invalidate the compensation evidence"
        );
    }

    #[test]
    fn stale_root_activation_does_not_compensate_over_a_later_external_transition() {
        use std::sync::mpsc;

        let mut backend = FakeBackend::new();
        backend.on_current = true;
        backend.external_focus_after_block = Some(9);
        let journal = ForegroundTransitionJournal::new();
        backend.foreground_journal = Some(journal.clone());
        let observation = ForegroundObservation {
            cursor: journal.cursor().unwrap(),
            journal,
        };
        let (effect_started_tx, effect_started_rx) = mpsc::channel();
        let (resume_effect_tx, resume_effect_rx) = mpsc::channel();
        backend.blocked_set_foreground = Some((effect_started_tx, resume_effect_rx));

        let revision = crate::visibility::VisibilityRevision::default();
        let visible = Arc::new(AtomicBool::new(true));
        let request_revision = revision.request(|| ()).0;
        let root_window = crate::visibility::RootWindowBridge::default();
        root_window.set_identity_for_test(42);
        let (_, root_window_generation) = root_window.identity();
        let fence = WindowActivationFence {
            revision: revision.clone(),
            request_revision,
            visible: Arc::clone(&visible),
            expected_process: 99,
            root_window: root_window.clone(),
            root_window_generation,
        };

        let activation = std::thread::spawn(move || {
            let result = activate_with_observation(
                &mut backend,
                WindowActivationRequest::follow_window(42),
                Some(&fence),
                Some(&observation),
            );
            (result, backend)
        });
        effect_started_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("activation must block before its delayed foreground effect");
        revision.request(|| visible.store(false, Ordering::Release));
        let _ = resume_effect_tx.send(());

        let (result, backend) = activation.join().expect("activation worker");
        assert_eq!(
            result.unwrap_err().kind,
            WindowActivationErrorKind::Superseded
        );
        assert_eq!(backend.foreground, Some(9));
        assert!(
            !backend.events.iter().any(|event| event == "set:7"),
            "a later external transition must remain the foreground owner"
        );
    }

    #[test]
    fn stale_root_activation_compensates_after_numeric_root_handle_is_reused() {
        use std::sync::mpsc;

        let mut backend = FakeBackend::new();
        backend.on_current = true;
        let journal = ForegroundTransitionJournal::new();
        backend.foreground_journal = Some(journal.clone());
        let observation = ForegroundObservation {
            cursor: journal.cursor().unwrap(),
            journal: journal.clone(),
        };
        let (effect_started_tx, effect_started_rx) = mpsc::channel();
        let (resume_effect_tx, resume_effect_rx) = mpsc::channel();
        backend.blocked_set_foreground = Some((effect_started_tx, resume_effect_rx));

        let revision = crate::visibility::VisibilityRevision::default();
        let visible = Arc::new(AtomicBool::new(true));
        let request_revision = revision.request(|| ()).0;
        let root_window = crate::visibility::RootWindowBridge::default();
        root_window.set_identity_for_test(42);
        let (_, root_window_generation) = root_window.identity();
        let fence = WindowActivationFence {
            revision: revision.clone(),
            request_revision,
            visible: Arc::clone(&visible),
            expected_process: 99,
            root_window: root_window.clone(),
            root_window_generation,
        };

        let activation = std::thread::spawn(move || {
            let result = activate_with_observation(
                &mut backend,
                WindowActivationRequest::follow_window(42),
                Some(&fence),
                Some(&observation),
            );
            (result, backend)
        });
        effect_started_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("activation must block before its delayed foreground effect");
        root_window.set_identity_for_test(43);
        root_window.set_identity_for_test(42);
        journal.record_test_destroyed(42);
        journal.record_test_created(42);
        revision.request(|| visible.store(false, Ordering::Release));
        let _ = resume_effect_tx.send(());

        let (result, backend) = activation.join().expect("activation worker");
        assert_eq!(
            result.unwrap_err().kind,
            WindowActivationErrorKind::Superseded
        );
        assert_eq!(backend.foreground, Some(7));
        assert_eq!(
            backend
                .events
                .iter()
                .filter(|event| *event == "set:42")
                .count(),
            1,
            "compensation must not activate the HWND whose lifetime changed"
        );
        assert!(backend.events.iter().any(|event| event == "set:7"));
    }

    #[test]
    fn foreground_journal_waits_for_delayed_root_transition() {
        use std::sync::mpsc;

        let journal = ForegroundTransitionJournal::new();
        let cursor = journal.cursor().unwrap();
        let external = journal
            .record_event(ForegroundJournalEventKind::Foreground, 8, Some((20, 2)))
            .unwrap();

        let waiting_journal = journal.clone();
        let (started_tx, started_rx) = mpsc::channel();
        let (result_tx, result_rx) = mpsc::channel();
        let waiter = std::thread::spawn(move || {
            let _ = started_tx.send(());
            let result = waiting_journal.latest_non_root_before_observed_root(
                cursor,
                42,
                Duration::from_secs(1),
            );
            let _ = result_tx.send(result);
        });
        started_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("journal waiter should start");
        journal.record_test_foreground(42, 10, 3);

        assert_eq!(
            result_rx
                .recv_timeout(Duration::from_secs(1))
                .expect("delayed ROOT event should release the waiter"),
            Some(JournalFocusChoice::Newer(external))
        );
        waiter.join().expect("journal observer thread");
    }

    #[test]
    fn root_activation_rejects_same_process_hwnd_after_root_lifetime_changes() {
        let mut backend = FakeBackend::new();
        backend.on_current = true;
        let revision = crate::visibility::VisibilityRevision::default();
        let visible = Arc::new(AtomicBool::new(true));
        let request_revision = revision.request(|| ()).0;
        let root_window = crate::visibility::RootWindowBridge::default();
        root_window.set_identity_for_test(42);
        let (_, root_window_generation) = root_window.identity();
        let fence = WindowActivationFence {
            revision,
            request_revision,
            visible,
            expected_process: 99,
            root_window: root_window.clone(),
            root_window_generation,
        };

        // Simulate a replacement root window in the same process reusing its
        // predecessor's numeric HWND before the old restore worker resumes.
        root_window.set_identity_for_test(43);
        root_window.set_identity_for_test(42);

        let error = activate_with_fence(
            &mut backend,
            WindowActivationRequest::follow_window(42),
            Some(&fence),
        )
        .unwrap_err();

        assert_eq!(error.kind, WindowActivationErrorKind::Superseded);
        assert!(!backend.events.iter().any(|event| event.starts_with("set:")));
        assert!(!backend.events.iter().any(|event| event.starts_with("top:")));
    }

    #[test]
    fn non_minimized_window_is_never_restored_and_preserves_maximized_state() {
        let mut backend = FakeBackend::new();
        backend.on_current = true;
        activate_with(&mut backend, WindowActivationRequest::follow_window(42)).unwrap();
        assert!(!backend.events.iter().any(|event| event == "restore"));
    }

    #[test]
    fn minimized_window_is_restored_before_foreground_activation() {
        let mut backend = FakeBackend::new();
        backend.on_current = true;
        backend.minimized = true;
        activate_with(&mut backend, WindowActivationRequest::follow_window(42)).unwrap();
        let minimized = backend
            .events
            .iter()
            .position(|event| event == "is_minimized")
            .unwrap();
        let restore = backend
            .events
            .iter()
            .position(|event| event == "restore")
            .unwrap();
        let foreground = backend
            .events
            .iter()
            .position(|event| event == "set:42")
            .unwrap();
        assert!(minimized < restore && restore < foreground);
    }

    #[test]
    fn move_to_current_is_explicit_and_does_not_switch() {
        let mut backend = FakeBackend::new();
        activate_with(
            &mut backend,
            WindowActivationRequest::move_to_current_desktop(42),
        )
        .unwrap();
        assert!(
            backend
                .events
                .iter()
                .any(|event| event == &format!("move:{}", id(1)))
        );
        assert!(
            backend
                .events
                .iter()
                .any(|event| event == "current_desktop")
        );
        assert!(
            !backend
                .events
                .iter()
                .any(|event| event.starts_with("switch:"))
        );
    }

    #[test]
    fn follow_window_allows_an_animated_desktop_transition_to_settle() {
        let mut backend = FakeBackend::new();
        backend.transition_delay_after_action = 5;
        activate_with(&mut backend, WindowActivationRequest::follow_window(42)).unwrap();
        let pauses: Vec<_> = backend
            .events
            .iter()
            .filter(|event| event.starts_with("pause:"))
            .cloned()
            .collect();
        assert_eq!(
            pauses,
            [
                "pause:25",
                "pause:50",
                "pause:100",
                "pause:150",
                "pause:200"
            ]
        );
    }

    #[test]
    fn desktop_transition_wait_is_bounded_when_the_shell_never_settles() {
        let mut backend = FakeBackend::new();
        backend.transition_delay_after_action = usize::MAX;
        let error =
            activate_with(&mut backend, WindowActivationRequest::follow_window(42)).unwrap_err();
        assert_eq!(error.kind, WindowActivationErrorKind::Desktop);
        let pauses: Vec<_> = backend
            .events
            .iter()
            .filter(|event| event.starts_with("pause:"))
            .cloned()
            .collect();
        assert_eq!(
            pauses,
            [
                "pause:25",
                "pause:50",
                "pause:100",
                "pause:150",
                "pause:200",
                "pause:250",
                "pause:300"
            ]
        );
    }

    #[test]
    fn invalid_window_stops_before_desktop_or_foreground_calls() {
        let mut backend = FakeBackend::new();
        backend.valid = false;
        let error =
            activate_with(&mut backend, WindowActivationRequest::follow_window(42)).unwrap_err();
        assert_eq!(error.kind, WindowActivationErrorKind::InvalidWindow);
        assert_eq!(backend.events, ["validate"]);
    }

    #[test]
    fn fallback_attaches_and_always_detaches_in_reverse_order() {
        let mut backend = FakeBackend::new();
        backend.target = backend.current.clone();
        backend.activate_on_attempt = usize::MAX;
        let error =
            activate_with(&mut backend, WindowActivationRequest::follow_window(42)).unwrap_err();
        assert_eq!(error.kind, WindowActivationErrorKind::ForegroundDenied);
        let attachments: Vec<_> = backend
            .events
            .iter()
            .filter(|event| event.starts_with("attach:"))
            .cloned()
            .collect();
        assert_eq!(
            attachments,
            [
                "attach:2:1:true",
                "attach:3:1:true",
                "attach:3:1:false",
                "attach:2:1:false"
            ]
        );
    }

    #[test]
    fn current_desktop_only_rejects_cross_desktop_target_before_restore() {
        let mut backend = FakeBackend::new();
        let error = activate_with(
            &mut backend,
            WindowActivationRequest {
                hwnd: 42,
                desktop_policy: WindowDesktopPolicy::CurrentDesktopOnly,
            },
        )
        .unwrap_err();
        assert_eq!(error.kind, WindowActivationErrorKind::Desktop);
        assert_eq!(backend.events, ["validate", "is_current"]);
    }

    #[test]
    fn successful_fallback_still_detaches_every_attached_queue() {
        let mut backend = FakeBackend::new();
        backend.target = backend.current.clone();
        backend.on_current = true;
        backend.activate_on_attempt = 3;
        activate_with(&mut backend, WindowActivationRequest::follow_window(42)).unwrap();
        let attachments: Vec<_> = backend
            .events
            .iter()
            .filter(|event| event.starts_with("attach:"))
            .cloned()
            .collect();
        assert_eq!(
            attachments,
            [
                "attach:2:1:true",
                "attach:3:1:true",
                "attach:3:1:false",
                "attach:2:1:false"
            ]
        );
    }

    #[test]
    fn partial_attach_failure_detaches_only_successful_attachment() {
        let mut backend = FakeBackend::new();
        backend.target = backend.current.clone();
        backend.on_current = true;
        backend.activate_on_attempt = usize::MAX;
        backend.attach_fail_at = Some((3, 1));
        let error =
            activate_with(&mut backend, WindowActivationRequest::follow_window(42)).unwrap_err();
        assert_eq!(error.kind, WindowActivationErrorKind::ForegroundDenied);
        let attachments: Vec<_> = backend
            .events
            .iter()
            .filter(|e| e.starts_with("attach:"))
            .cloned()
            .collect();
        assert_eq!(
            attachments,
            ["attach:2:1:true", "attach:3:1:true", "attach:2:1:false"]
        );
    }

    #[test]
    fn detach_failure_is_reported_even_after_activation() {
        let mut backend = FakeBackend::new();
        backend.target = backend.current.clone();
        backend.on_current = true;
        backend.activate_on_attempt = 3;
        backend.detach_fail_at = Some((3, 1));
        let error =
            activate_with(&mut backend, WindowActivationRequest::follow_window(42)).unwrap_err();
        assert_eq!(error.kind, WindowActivationErrorKind::ForegroundDenied);
        assert_eq!(
            error.context.get("activation_observed").map(String::as_str),
            Some("true")
        );
        assert!(error.context["detach_failures"].contains("3->1"));
    }
}
