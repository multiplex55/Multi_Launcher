//! Native Windows boundary helpers for the opt-in radial acceptance process.

#![cfg(windows)]

mod suite;
use super::{AcceptanceHotkey, foreign_edge_indices_interfering_owned_spans, owned_gesture_spans};
pub(super) use suite::{
    CopiedAuthoringOptions, record_environment_failure, run_copied_profile_suite, run_hotkey_suite,
    run_suite,
};

use std::fmt::Write as _;
use std::fs::File;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use windows::Win32::Foundation::{
    BOOL, CloseHandle, GetLastError, HANDLE, HANDLE_FLAG_INHERIT, HANDLE_FLAGS, HWND, LPARAM,
    POINT, RECT, SetHandleInformation, WAIT_OBJECT_0, WAIT_TIMEOUT, WPARAM,
};
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::StationsAndDesktops::{
    CloseDesktop, DESKTOP_ACCESS_FLAGS, DESKTOP_CREATEWINDOW, DESKTOP_READOBJECTS,
    DESKTOP_WRITEOBJECTS, GetThreadDesktop, GetUserObjectInformationW, HDESK, OpenInputDesktop,
    SetThreadDesktop, UOI_NAME,
};
use windows::Win32::System::SystemServices::SS_NOTIFY;
use windows::Win32::System::Threading::{
    AttachThreadInput, CREATE_UNICODE_ENVIRONMENT, CreateProcessW, GetCurrentThreadId,
    GetExitCodeProcess, GetExitCodeThread, GetProcessIdOfThread, OpenThread, PROCESS_INFORMATION,
    STARTF_USESTDHANDLES, STARTUPINFOW, THREAD_QUERY_LIMITED_INFORMATION, TerminateProcess,
    WaitForSingleObject,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationSelectionItemPattern,
    IUIAutomationTogglePattern, IUIAutomationTreeWalker, IUIAutomationValuePattern, ToggleState,
    TreeScope_Descendants, UIA_EditControlTypeId, UIA_NamePropertyId, UIA_SelectionItemPatternId,
    UIA_TogglePatternId, UIA_ValuePatternId,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, HOT_KEY_MODIFIERS, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT,
    KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, MOUSEEVENTF_ABSOLUTE,
    MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MOVE, MOUSEEVENTF_MOVE_NOCOALESCE,
    MOUSEEVENTF_VIRTUALDESK, MOUSEINPUT, RegisterHotKey, SendInput, UnregisterHotKey, VIRTUAL_KEY,
    VK_CONTROL, VK_END, VK_F4, VK_F11, VK_F24, VK_LBUTTON, VK_LCONTROL, VK_LMENU, VK_LSHIFT,
    VK_LWIN, VK_MENU, VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SHIFT, VK_TAB,
};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, CallNextHookEx, CreateWindowExW, DestroyWindow, DispatchMessageW,
    EnumWindows, GetClassNameW, GetClientRect, GetClipCursor, GetForegroundWindow, GetMessageW,
    GetSystemMetrics, GetWindowRect, GetWindowTextLengthW, GetWindowTextW,
    GetWindowThreadProcessId, HWND_NOTOPMOST, HWND_TOPMOST, IsChild, IsIconic, IsWindowVisible,
    KBDLLHOOKSTRUCT, PM_NOREMOVE, PeekMessageW, PostMessageW, PostThreadMessageW,
    SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SetCursorPos, SetForegroundWindow, SetWindowPos,
    SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, WH_KEYBOARD_LL, WINDOW_STYLE, WM_APP,
    WM_CLOSE, WM_KEYDOWN, WM_KEYUP, WM_QUIT, WM_SYSKEYDOWN, WM_SYSKEYUP, WS_CAPTION,
    WS_EX_TOOLWINDOW, WS_SYSMENU, WS_VISIBLE, WindowFromPoint,
};
use windows::core::w;
use windows::core::{Interface, PCWSTR, PWSTR, VARIANT};

thread_local! {
    static RUNNER_HOOK_EVENTS: std::cell::RefCell<Option<std::sync::mpsc::Sender<RunnerHookEdge>>> = const { std::cell::RefCell::new(None) };
}

#[derive(Clone, Copy)]
struct RunnerHookEdge {
    vk: u32,
    down: bool,
    injected: bool,
    extra_info: usize,
    at: Instant,
}

const TRACE_ENV: &str = "MULTI_LAUNCHER_RADIAL_ACCEPTANCE_TRACE";
const PREPARE_HOLD_ENV: &str = "MULTI_LAUNCHER_RADIAL_ACCEPTANCE_PREPARE_HOLD_FILE";
const PREPARE_HOLD_FILE_NAME: &str = "radial-acceptance-prepare.hold";
const ROOT_TITLE: &str = "Multi Lnchr";
const DESIGNER_TITLE: &str = "Radial Designer";
pub(super) const RADIAL_HOST_WINDOW_CLASS: &str = "MultiLauncherRadialHost";
const WINDOW_POLL: Duration = Duration::from_millis(25);
const FOREGROUND_TRANSITION_TIMEOUT: Duration = Duration::from_millis(500);
const STARTUP_TIMEOUT: Duration = Duration::from_secs(20);
const CASE_TIMEOUT: Duration = Duration::from_secs(4);
const MAX_ENUMERATED_WINDOWS: usize = 256;
const MAX_POINTER_CORRECTIONS: usize = 4;
const MAX_POINTER_CORRECTION_PIXELS: u32 = 8;
const ACCEPTANCE_RUNNER_INPUT_COOKIE: usize = 0x5241_4449_414C_0001; // "RADIAL\x01"
const ACCEPTANCE_HOTKEY_ID: i32 = 0x4D4C;
// Keep this acceptance-only pump probe message aligned with
// native_service::WM_HOOK_PUMP_PROBE in hotkey/launcher_invocation.rs.
const HOOK_PUMP_PROBE_MESSAGE: u32 = WM_APP + 0x54;
const FOCUS_ANCHOR_COMMAND_MESSAGE: u32 = WM_APP + 0x55;

pub(super) struct InputDesktopAttachment {
    previous: HDESK,
    attached: HDESK,
    changed: bool,
}

#[derive(Clone, Debug)]
pub(super) struct NativeInputEdgeEvidence {
    pub inserted: usize,
    pub at_unix_ms: u128,
    pub foreground_hwnd: u64,
    pub foreground_pid: u32,
    pub input_desktop: String,
    pub cleanup_status: String,
    keyboard_input: Option<KeyboardInputEvidence>,
}

#[derive(Clone, Debug)]
struct KeyboardInputEvidence {
    vk: u16,
    scan: u16,
    flags: u32,
    extra_info: usize,
    async_state_before: i16,
    async_state_after: i16,
}

impl NativeInputEdgeEvidence {
    pub fn describe(&self) -> String {
        format!(
            "inserted={} at_unix_ms={} foreground=HWND:{} PID:{} desktop={} cleanup={}{}",
            self.inserted,
            self.at_unix_ms,
            self.foreground_hwnd,
            self.foreground_pid,
            self.input_desktop,
            self.cleanup_status,
            self.keyboard_input.as_ref().map_or_else(String::new, |key| format!(
                " keyboard(vk=0x{:04x},scan=0x{:04x},flags=0x{:04x},extra_info=0x{:x},async_state_before=0x{:04x},async_state_after=0x{:04x})",
                key.vk,
                key.scan,
                key.flags,
                key.extra_info,
                key.async_state_before as u16,
                key.async_state_after as u16
            ))
        )
    }
}

#[derive(Clone, Debug)]
pub(super) struct F11TapEvidence {
    pub down: NativeInputEdgeEvidence,
    pub up: NativeInputEdgeEvidence,
}

impl F11TapEvidence {
    pub fn describe(&self) -> String {
        format!(
            "down=[{}], up=[{}]",
            self.down.describe(),
            self.up.describe()
        )
    }
}

#[derive(Clone, Debug)]
pub(super) struct AcceptanceHotkeyTapEvidence {
    pub down: NativeInputEdgeEvidence,
    pub up: NativeInputEdgeEvidence,
    pub observed_vks: Vec<u32>,
}

#[derive(Clone, Debug)]
pub(super) struct AcceptanceHotkeyBurstEvidence {
    pub down_inserted: usize,
    pub up_inserted: usize,
    pub input_desktop: String,
    pub cleanup: String,
}

impl AcceptanceHotkeyTapEvidence {
    pub fn describe(&self) -> String {
        format!(
            "owned key down=[{}], up=[{}], observed_vks={:?}",
            self.down.describe(),
            self.up.describe(),
            self.observed_vks
        )
    }
}

pub(super) struct RunnerHookObservation {
    pub desktop: String,
    pub down_seen: bool,
    pub up_seen: bool,
    pub down_injected: bool,
    pub up_injected: bool,
}

#[derive(Clone, Debug)]
pub(super) struct RunnerChordKeyObservation {
    pub vk: u32,
    pub down: usize,
    pub up: usize,
    pub injected_down: usize,
    pub injected_up: usize,
}

#[derive(Clone, Debug)]
pub(super) struct RunnerChordObservation {
    pub desktop: String,
    pub keys: Vec<RunnerChordKeyObservation>,
    pub ordered_edges: Vec<RunnerChordEdge>,
    pub foreign_edges: Vec<RunnerChordEdge>,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct RunnerChordEdge {
    pub vk: u32,
    pub down: bool,
    pub injected: bool,
    pub extra_info: usize,
    pub at: Instant,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RunnerChordTiming {
    pub primary_hold_ms: Vec<u128>,
    pub released_gap_ms: Vec<u128>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct RunnerKeyQuietEvidence {
    pub quiet_ms: u128,
    pub matching_edges: usize,
}

impl RunnerChordObservation {
    pub fn exact_injected_pairs(&self, expected_pairs: usize) -> bool {
        !self.keys.is_empty()
            && self.keys.iter().all(|key| {
                key.down == expected_pairs
                    && key.up == expected_pairs
                    && key.injected_down == expected_pairs
                    && key.injected_up == expected_pairs
            })
    }

    pub fn exact_injected_sequence(&self, expected: &[(u32, bool)]) -> bool {
        self.ordered_edges.len() == expected.len()
            && self.ordered_edges.iter().zip(expected).all(
                |(edge, (expected_vk, expected_down))| {
                    edge.vk == *expected_vk
                        && edge.down == *expected_down
                        && edge.injected
                        && edge.extra_info == ACCEPTANCE_RUNNER_INPUT_COOKIE
                },
            )
    }

    pub fn foreign_edges_interfering_with_owned_gestures(&self) -> Vec<RunnerChordEdge> {
        let spans = owned_gesture_spans(
            self.ordered_edges
                .iter()
                .map(|edge| (edge.at, edge.vk, edge.down)),
        );
        let foreign_timeline = self
            .foreign_edges
            .iter()
            .map(|edge| (edge.at, edge.vk, edge.down))
            .collect::<Vec<_>>();
        let mut interfering =
            foreign_edge_indices_interfering_owned_spans(&spans, &foreign_timeline)
                .into_iter()
                .map(|index| self.foreign_edges[index])
                .collect::<Vec<_>>();
        interfering.sort_by_key(|edge| edge.at);
        interfering
    }

    pub fn timing(
        &self,
        hotkey: AcceptanceHotkey,
        taps: usize,
    ) -> Result<RunnerChordTiming, String> {
        let edges_per_tap = match hotkey {
            AcceptanceHotkey::F11 => 2,
            AcceptanceHotkey::ShiftAltWinEnd => 8,
        };
        if self.ordered_edges.len() != taps.saturating_mul(edges_per_tap) {
            return Err("observed hotkey edge timing was incomplete".into());
        }
        if self
            .ordered_edges
            .windows(2)
            .any(|pair| pair[1].at < pair[0].at)
        {
            return Err("hook observer timestamps moved backwards".into());
        }
        let primary_down_offset = match hotkey {
            AcceptanceHotkey::F11 => 0,
            AcceptanceHotkey::ShiftAltWinEnd => 3,
        };
        let primary_up_offset = primary_down_offset + 1;
        let mut primary_hold_ms = Vec::with_capacity(taps);
        let mut released_gap_ms = Vec::with_capacity(taps.saturating_sub(1));
        for tap in 0..taps {
            let base = tap * edges_per_tap;
            let down = self.ordered_edges[base + primary_down_offset];
            let up = self.ordered_edges[base + primary_up_offset];
            if !down.down || up.down || down.vk != up.vk {
                return Err("primary key hold timing did not have a down/up pair".into());
            }
            let hold = up
                .at
                .checked_duration_since(down.at)
                .ok_or_else(|| "primary key release preceded its press".to_string())?;
            primary_hold_ms.push(hold.as_millis());
            if tap + 1 < taps {
                let next_down = self.ordered_edges[(tap + 1) * edges_per_tap + primary_down_offset];
                if !next_down.down || next_down.vk != down.vk {
                    return Err("next primary press was missing from cadence trace".into());
                }
                let gap = next_down
                    .at
                    .checked_duration_since(up.at)
                    .ok_or_else(|| "next primary press preceded the prior release".to_string())?;
                released_gap_ms.push(gap.as_millis());
            }
        }
        Ok(RunnerChordTiming {
            primary_hold_ms,
            released_gap_ms,
        })
    }

    pub fn describe(&self) -> String {
        let keys = self
            .keys
            .iter()
            .map(|key| {
                format!(
                    "vk=0x{:02x} down/up={}/{} injected={}/{}",
                    key.vk, key.down, key.up, key.injected_down, key.injected_up
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        let foreign_edges = self
            .foreign_edges
            .iter()
            .take(8)
            .map(|edge| {
                format!(
                    "0x{:02x}:{}:injected={}:extra=0x{:x}",
                    edge.vk,
                    if edge.down { "down" } else { "up" },
                    edge.injected,
                    edge.extra_info
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "runner chord observer desktop={} [{keys}] foreign_matching_edges={} foreign_samples=[{}]",
            self.desktop,
            self.foreign_edges.len(),
            foreign_edges
        )
    }
}

impl RunnerHookObservation {
    pub fn describe(&self) -> String {
        format!(
            "runner observer desktop={} saw down/up={}/{} injected down/up={}/{}",
            self.desktop, self.down_seen, self.up_seen, self.down_injected, self.up_injected
        )
    }

    pub fn merge(&self, other: &Self) -> Self {
        Self {
            desktop: self.desktop.clone(),
            down_seen: self.down_seen || other.down_seen,
            up_seen: self.up_seen || other.up_seen,
            down_injected: self.down_injected || other.down_injected,
            up_injected: self.up_injected || other.up_injected,
        }
    }
}

pub(super) struct RunnerHookObserver {
    events: std::sync::mpsc::Receiver<RunnerHookEdge>,
    probe_acks: std::sync::mpsc::Receiver<u64>,
    unhook_result: std::sync::mpsc::Receiver<Result<(), String>>,
    thread_id: u32,
    hook_id: usize,
    join: Option<std::thread::JoinHandle<()>>,
    desktop: String,
}

impl RunnerHookObserver {
    pub fn start() -> Result<Self, String> {
        let (event_tx, events) = std::sync::mpsc::channel();
        let (probe_ack_tx, probe_acks) = std::sync::mpsc::channel();
        let (unhook_result_tx, unhook_result) = std::sync::mpsc::sync_channel(1);
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
        let join = std::thread::Builder::new()
            .name("radial-acceptance-hook-observer".into())
            .spawn(move || {
                let thread_id = unsafe { GetCurrentThreadId() };
                let mut queue_probe = windows::Win32::UI::WindowsAndMessaging::MSG::default();
                let _ = unsafe { PeekMessageW(&mut queue_probe, None, 0, 0, PM_NOREMOVE) };
                RUNNER_HOOK_EVENTS.with(|slot| *slot.borrow_mut() = Some(event_tx));
                let desktop = match unsafe { GetThreadDesktop(thread_id) } {
                    Ok(desktop) => {
                        desktop_name(desktop).unwrap_or_else(|error| format!("unknown ({error})"))
                    }
                    Err(error) => format!("unknown ({error})"),
                };
                let module = match unsafe { GetModuleHandleW(None) } {
                    Ok(module) => module,
                    Err(error) => {
                        let _ = ready_tx
                            .send(Err(format!("read acceptance hook module handle: {error}")));
                        RUNNER_HOOK_EVENTS.with(|slot| *slot.borrow_mut() = None);
                        return;
                    }
                };
                let hook = match unsafe {
                    SetWindowsHookExW(WH_KEYBOARD_LL, Some(runner_hook_proc), module, 0)
                } {
                    Ok(hook) => hook,
                    Err(error) => {
                        let _ = ready_tx
                            .send(Err(format!("install acceptance observer hook: {error}")));
                        RUNNER_HOOK_EVENTS.with(|slot| *slot.borrow_mut() = None);
                        return;
                    }
                };
                if ready_tx
                    .send(Ok((thread_id, desktop, hook.0 as usize)))
                    .is_err()
                {
                    let _ = unsafe { UnhookWindowsHookEx(hook) };
                    RUNNER_HOOK_EVENTS.with(|slot| *slot.borrow_mut() = None);
                    return;
                }
                let mut message = windows::Win32::UI::WindowsAndMessaging::MSG::default();
                while unsafe { GetMessageW(&mut message, None, 0, 0) }.0 > 0 {
                    if message.message == HOOK_PUMP_PROBE_MESSAGE {
                        let _ = probe_ack_tx.send(message.wParam.0 as u64);
                        continue;
                    }
                    let _ = unsafe { TranslateMessage(&message) };
                    unsafe { DispatchMessageW(&message) };
                }
                let unhook_result = unsafe { UnhookWindowsHookEx(hook) }
                    .map_err(|error| format!("unhook acceptance observer HHOOK: {error}"));
                let _ = unhook_result_tx.send(unhook_result);
                RUNNER_HOOK_EVENTS.with(|slot| *slot.borrow_mut() = None);
            })
            .map_err(|error| format!("start acceptance hook observer: {error}"))?;
        let (thread_id, desktop, hook_id) = match ready_rx.recv_timeout(Duration::from_secs(2)) {
            Ok(Ok(ready)) => ready,
            Ok(Err(error)) => {
                let _ = join.join();
                return Err(error);
            }
            Err(error) => {
                return Err(format!("acceptance hook observer did not start: {error}"));
            }
        };
        Ok(Self {
            events,
            probe_acks,
            unhook_result,
            thread_id,
            hook_id,
            join: Some(join),
            desktop,
        })
    }

    pub fn wait_for_vk(&mut self, vk: u32, timeout: Duration) -> RunnerHookObservation {
        let deadline = Instant::now() + timeout;
        let mut down_seen = false;
        let mut up_seen = false;
        let mut down_injected = false;
        let mut up_injected = false;
        while !down_seen || !up_seen {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            match self
                .events
                .recv_timeout(deadline.saturating_duration_since(now))
            {
                Ok(edge) if edge.vk == vk => {
                    if edge.down {
                        down_seen = true;
                        down_injected |= edge.injected;
                    } else {
                        up_seen = true;
                        up_injected |= edge.injected;
                    }
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
        RunnerHookObservation {
            desktop: self.desktop.clone(),
            down_seen,
            up_seen,
            down_injected: down_seen && down_injected,
            up_injected: up_seen && up_injected,
        }
    }

    pub fn wait_for_key_quiet(
        &mut self,
        vks: &[u32],
        quiet_period: Duration,
        timeout: Duration,
    ) -> Result<RunnerKeyQuietEvidence, String> {
        if vks.is_empty() || quiet_period.is_zero() || timeout < quiet_period {
            return Err("key quiet preflight requires keys and a bounded positive interval".into());
        }
        let started = Instant::now();
        let deadline = started + timeout;
        let mut last_matching = started;
        let mut matching_edges = 0usize;
        loop {
            let now = Instant::now();
            let quiet_elapsed = now.saturating_duration_since(last_matching);
            if quiet_elapsed >= quiet_period {
                return Ok(RunnerKeyQuietEvidence {
                    quiet_ms: quiet_elapsed.as_millis(),
                    matching_edges,
                });
            }
            if now >= deadline {
                return Err(format!(
                    "matching hotkey edges did not remain quiet for {}ms within {}ms (observed {matching_edges} matching edges)",
                    quiet_period.as_millis(),
                    timeout.as_millis()
                ));
            }
            let wait_for = (quiet_period - quiet_elapsed).min(deadline - now);
            match self.events.recv_timeout(wait_for) {
                Ok(edge) if vks.contains(&edge.vk) => {
                    matching_edges = matching_edges.saturating_add(1);
                    last_matching = Instant::now();
                }
                Ok(_) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    return Err("runner hook observer disconnected during quiet preflight".into());
                }
            }
        }
    }

    pub fn wait_for_chord(&mut self, vks: &[u32], timeout: Duration) -> RunnerHookObservation {
        let deadline = Instant::now() + timeout;
        let mut observed = std::collections::BTreeMap::<u32, [bool; 4]>::new();
        for vk in vks {
            observed.insert(*vk, [false; 4]);
        }
        while observed.values().any(|edges| !(edges[0] && edges[1])) {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            match self
                .events
                .recv_timeout(deadline.saturating_duration_since(now))
            {
                Ok(edge) => {
                    if let Some(edges) = observed.get_mut(&edge.vk) {
                        let (seen_index, injected_index) = if edge.down { (0, 2) } else { (1, 3) };
                        edges[seen_index] = true;
                        edges[injected_index] |= edge.injected;
                    }
                }
                Err(_) => break,
            }
        }
        let complete = !observed.is_empty() && observed.values().all(|edges| edges[0] && edges[1]);
        RunnerHookObservation {
            desktop: self.desktop.clone(),
            down_seen: complete,
            up_seen: complete,
            down_injected: complete && observed.values().all(|edges| edges[2]),
            up_injected: complete && observed.values().all(|edges| edges[3]),
        }
    }

    pub fn wait_for_chord_burst(
        &mut self,
        vks: &[u32],
        expected_pairs: usize,
        timeout: Duration,
    ) -> RunnerChordObservation {
        let deadline = Instant::now() + timeout;
        let mut counts = std::collections::BTreeMap::<u32, [usize; 4]>::new();
        for vk in vks {
            counts.entry(*vk).or_insert([0; 4]);
        }
        let mut ordered_edges = Vec::with_capacity(expected_pairs.saturating_mul(vks.len()) * 2);
        let mut foreign_edges = Vec::new();
        let complete = |counts: &std::collections::BTreeMap<u32, [usize; 4]>| {
            !counts.is_empty()
                && counts
                    .values()
                    .all(|edges| edges[0] >= expected_pairs && edges[1] >= expected_pairs)
        };
        while !complete(&counts) {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            match self
                .events
                .recv_timeout(deadline.saturating_duration_since(now))
            {
                Ok(edge) => {
                    record_runner_chord_edge(
                        edge,
                        &mut counts,
                        &mut ordered_edges,
                        &mut foreign_edges,
                    );
                }
                Err(_) => break,
            }
        }

        // Drain a short quiet interval after the requested edge budget so an
        // extra edge in the same uninterrupted burst cannot hide behind the
        // first complete set of down/up observations.
        let mut quiet_deadline = Instant::now() + Duration::from_millis(40);
        while Instant::now() < quiet_deadline && Instant::now() < deadline {
            match self.events.recv_timeout(
                quiet_deadline
                    .min(deadline)
                    .saturating_duration_since(Instant::now()),
            ) {
                Ok(edge) => {
                    record_runner_chord_edge(
                        edge,
                        &mut counts,
                        &mut ordered_edges,
                        &mut foreign_edges,
                    );
                    quiet_deadline = Instant::now() + Duration::from_millis(40);
                }
                Err(_) => break,
            }
        }

        RunnerChordObservation {
            desktop: self.desktop.clone(),
            keys: counts
                .into_iter()
                .map(|(vk, edges)| RunnerChordKeyObservation {
                    vk,
                    down: edges[0],
                    up: edges[1],
                    injected_down: edges[2],
                    injected_up: edges[3],
                })
                .collect(),
            ordered_edges,
            foreign_edges,
        }
    }

    pub fn drain_pending(&mut self) -> usize {
        let mut drained = 0;
        while self.events.try_recv().is_ok() {
            drained += 1;
        }
        drained
    }

    pub fn wait_for_vk_edge(
        &mut self,
        vk: u32,
        down: bool,
        timeout: Duration,
    ) -> RunnerHookObservation {
        let deadline = Instant::now() + timeout;
        let mut observed = false;
        let mut injected = false;
        while !observed {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            match self
                .events
                .recv_timeout(deadline.saturating_duration_since(now))
            {
                Ok(edge) if edge.vk == vk && edge.down == down => {
                    observed = true;
                    injected = edge.injected;
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
        RunnerHookObservation {
            desktop: self.desktop.clone(),
            down_seen: observed && down,
            up_seen: observed && !down,
            down_injected: observed && down && injected,
            up_injected: observed && !down && injected,
        }
    }

    pub fn thread_id(&self) -> u32 {
        self.thread_id
    }

    pub fn hook_id(&self) -> usize {
        self.hook_id
    }

    pub fn pump_roundtrip(&self, probe_id: u64, timeout: Duration) -> Result<(), String> {
        unsafe {
            PostThreadMessageW(
                self.thread_id,
                HOOK_PUMP_PROBE_MESSAGE,
                WPARAM(probe_id as usize),
                LPARAM(0),
            )
        }
        .map_err(|error| format!("post runner hook-pump probe: {error}"))?;
        let deadline = Instant::now() + timeout;
        loop {
            let now = Instant::now();
            if now >= deadline {
                return Err(format!(
                    "runner hook thread {} did not acknowledge pump probe {probe_id}",
                    self.thread_id
                ));
            }
            match self
                .probe_acks
                .recv_timeout(deadline.saturating_duration_since(now))
            {
                Ok(acknowledged) if acknowledged == probe_id => return Ok(()),
                Ok(_) => {}
                Err(error) => {
                    return Err(format!(
                        "runner hook thread {} pump probe {probe_id} acknowledgement failed: {error}",
                        self.thread_id
                    ));
                }
            }
        }
    }

    pub fn stop_and_report(&mut self) -> Result<(), String> {
        let mut errors = Vec::new();
        if self.thread_id != 0 && self.join.is_some() {
            if let Err(error) = unsafe {
                PostThreadMessageW(
                    self.thread_id,
                    WM_QUIT,
                    windows::Win32::Foundation::WPARAM(0),
                    windows::Win32::Foundation::LPARAM(0),
                )
            } {
                errors.push(format!(
                    "post WM_QUIT to acceptance observer thread {}: {error}",
                    self.thread_id
                ));
            }
        }
        if let Some(join) = self.join.take() {
            if join.join().is_err() {
                errors.push(format!(
                    "acceptance observer thread {} panicked while stopping",
                    self.thread_id
                ));
            }
        }
        match self.unhook_result.recv_timeout(Duration::from_secs(1)) {
            Ok(Ok(())) => {}
            Ok(Err(error)) => errors.push(error),
            Err(error) => errors.push(format!(
                "acceptance observer thread {} did not report UnhookWindowsHookEx: {error}",
                self.thread_id
            )),
        }
        self.thread_id = 0;
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }
}

fn record_runner_chord_edge(
    edge: RunnerHookEdge,
    counts: &mut std::collections::BTreeMap<u32, [usize; 4]>,
    owned_edges: &mut Vec<RunnerChordEdge>,
    foreign_edges: &mut Vec<RunnerChordEdge>,
) {
    let Some(count) = counts.get_mut(&edge.vk) else {
        return;
    };
    let observed = RunnerChordEdge {
        vk: edge.vk,
        down: edge.down,
        injected: edge.injected,
        extra_info: edge.extra_info,
        at: edge.at,
    };
    if edge.extra_info != ACCEPTANCE_RUNNER_INPUT_COOKIE {
        foreign_edges.push(observed);
        return;
    }
    let (seen_index, injected_index) = if edge.down { (0, 2) } else { (1, 3) };
    count[seen_index] = count[seen_index].saturating_add(1);
    if edge.injected {
        count[injected_index] = count[injected_index].saturating_add(1);
    }
    owned_edges.push(observed);
}

pub(super) fn thread_liveness(thread_id: u32) -> String {
    if thread_id == 0 {
        return "thread id unavailable".into();
    }
    let handle = match unsafe { OpenThread(THREAD_QUERY_LIMITED_INFORMATION, false, thread_id) } {
        Ok(handle) => handle,
        Err(error) => return format!("thread={thread_id} liveness unavailable ({error})"),
    };
    let mut exit_code = 0;
    let result = unsafe { GetExitCodeThread(handle, &mut exit_code) };
    let _ = unsafe { CloseHandle(handle) };
    match result {
        Ok(()) if exit_code == 259 => format!("thread={thread_id} alive=true"),
        Ok(()) => format!("thread={thread_id} alive=false exit_code={exit_code}"),
        Err(error) => format!("thread={thread_id} liveness unavailable ({error})"),
    }
}

pub(super) fn post_validated_hook_pump_probe(
    thread_id: u32,
    expected_process_id: u32,
    probe_id: u64,
) -> Result<(), String> {
    if thread_id == 0 {
        return Err("refused hook-pump probe to thread id 0".into());
    }
    let handle = unsafe { OpenThread(THREAD_QUERY_LIMITED_INFORMATION, false, thread_id) }
        .map_err(|error| format!("open hook thread {thread_id} for validation: {error}"))?;
    let process_id = unsafe { GetProcessIdOfThread(handle) };
    let _ = unsafe { CloseHandle(handle) };
    if process_id != expected_process_id {
        return Err(format!(
            "refused hook-pump probe: thread {thread_id} belongs to PID {process_id}, expected PID {expected_process_id}"
        ));
    }
    unsafe {
        PostThreadMessageW(
            thread_id,
            HOOK_PUMP_PROBE_MESSAGE,
            WPARAM(probe_id as usize),
            LPARAM(0),
        )
    }
    .map_err(|error| format!("post production hook-pump probe: {error}"))
}

impl Drop for RunnerHookObserver {
    fn drop(&mut self) {
        let _ = self.stop_and_report();
    }
}

unsafe extern "system" fn runner_hook_proc(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    if code >= 0 {
        let transition = match wparam.0 as u32 {
            WM_KEYDOWN | WM_SYSKEYDOWN => Some(true),
            WM_KEYUP | WM_SYSKEYUP => Some(false),
            _ => None,
        };
        if let Some(down) = transition {
            let data = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
            // Observe every keyboard edge. The suite correlates only the
            // requested virtual keys and our unique injection cookie; filtering
            // here would make supported direct-trigger chords invisible.
            forward_runner_hook_edge(RunnerHookEdge {
                vk: data.vkCode,
                down,
                injected: data
                    .flags
                    .contains(windows::Win32::UI::WindowsAndMessaging::LLKHF_INJECTED),
                extra_info: data.dwExtraInfo,
                at: Instant::now(),
            });
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

fn forward_runner_hook_edge(edge: RunnerHookEdge) {
    RUNNER_HOOK_EVENTS.with(|slot| {
        if let Ok(sender) = slot.try_borrow()
            && let Some(sender) = sender.as_ref()
        {
            let _ = sender.send(edge);
        }
    });
}

#[derive(Clone, Debug)]
pub(super) struct PointerClickEvidence {
    pub nudge_movement: NativeInputEdgeEvidence,
    pub movement: NativeInputEdgeEvidence,
    pub pointer_correction_events: usize,
    pub pointer_position_preexisting_ack: bool,
    pub pointer_move_acknowledged: bool,
    pub down: NativeInputEdgeEvidence,
    pub down_acknowledged: bool,
    pub left_button_state_after_down: i16,
    pub left_button_state_before_up: i16,
    pub up: NativeInputEdgeEvidence,
    pub up_acknowledged: bool,
    pub left_button_state_after_up: i16,
    pub target_hwnd: HWND,
    pub nudge_under_cursor_hwnd: HWND,
    pub under_cursor_hwnd: HWND,
    pub foreground_hwnd: HWND,
    pub screen_point: (i32, i32),
}

impl PointerClickEvidence {
    pub fn describe(&self) -> String {
        format!(
            "screen_point=({},{}), target_hwnd={}, nudge_under_cursor_hwnd={}, under_cursor_hwnd={}, foreground_hwnd={}, preexisting_egui_pointer_ack={}, fresh_egui_pointer_move_ack={}, nudge_move=[{}], move=[{}], pointer_correction_events={}, down=[{}], root_or_designer_down_ack={}, left_button_async_after_down=0x{:04x}, left_button_async_before_up=0x{:04x}, up=[{}], root_or_designer_up_ack={}, left_button_async_after_up=0x{:04x}",
            self.screen_point.0,
            self.screen_point.1,
            hwnd_id(self.target_hwnd),
            hwnd_id(self.nudge_under_cursor_hwnd),
            hwnd_id(self.under_cursor_hwnd),
            hwnd_id(self.foreground_hwnd),
            self.pointer_position_preexisting_ack,
            self.pointer_move_acknowledged,
            self.nudge_movement.describe(),
            self.movement.describe(),
            self.pointer_correction_events,
            self.down.describe(),
            self.down_acknowledged,
            self.left_button_state_after_down as u16,
            self.left_button_state_before_up as u16,
            self.up.describe(),
            self.up_acknowledged,
            self.left_button_state_after_up as u16
        )
    }
}

impl InputDesktopAttachment {
    pub fn release_for_thread_exit(mut self) -> Option<isize> {
        if self.changed {
            self.changed = false;
            Some(self.attached.0 as isize)
        } else {
            None
        }
    }
}

pub(super) fn close_input_desktop_after_driver_exit(handle: isize) -> Result<(), String> {
    unsafe { CloseDesktop(HDESK(handle as *mut std::ffi::c_void)) }
        .map_err(|error| format!("close input desktop after driver thread exit: {error}"))
}

impl Drop for InputDesktopAttachment {
    fn drop(&mut self) {
        if self.changed && unsafe { SetThreadDesktop(self.previous) }.is_ok() {
            let _ = unsafe { CloseDesktop(self.attached) };
            self.changed = false;
        }
    }
}

pub(super) fn attach_to_input_desktop() -> Result<(String, InputDesktopAttachment), String> {
    let thread_id = unsafe { GetCurrentThreadId() };
    let current = unsafe { GetThreadDesktop(thread_id) }
        .map_err(|error| format!("read runner thread desktop: {error}"))?;
    let current_name = desktop_name(current)?;
    let input = unsafe {
        OpenInputDesktop(
            Default::default(),
            false,
            DESKTOP_ACCESS_FLAGS(
                DESKTOP_READOBJECTS.0 | DESKTOP_WRITEOBJECTS.0 | DESKTOP_CREATEWINDOW.0,
            ),
        )
    }
    .map_err(|error| format!("open current Windows input desktop: {error}"))?;
    let input_name = match desktop_name(input) {
        Ok(name) => name,
        Err(error) => {
            let _ = unsafe { CloseDesktop(input) };
            return Err(error);
        }
    };
    if !input_name.eq_ignore_ascii_case("Default") {
        let _ = unsafe { CloseDesktop(input) };
        return Err(format!(
            "active input desktop is '{input_name}', expected the interactive Default desktop"
        ));
    }

    let changed = !current_name.eq_ignore_ascii_case(&input_name);
    if changed {
        if let Err(error) = unsafe { SetThreadDesktop(input) } {
            let _ = unsafe { CloseDesktop(input) };
            return Err(format!(
                "attach runner thread to input desktop '{input_name}': {error}"
            ));
        }
    }
    if !changed {
        unsafe { CloseDesktop(input) }
            .map_err(|error| format!("close duplicate input-desktop handle: {error}"))?;
    }

    let attachment = InputDesktopAttachment {
        previous: current,
        attached: input,
        changed,
    };
    let cursor = cursor_position()?;
    let foreground = unsafe { GetForegroundWindow() };
    let foreground_pid = window_process_id(foreground);
    Ok((
        format!(
            "runner thread desktop '{current_name}' -> '{input_name}'; cursor=({},{}); foreground_hwnd={} foreground_pid={foreground_pid}",
            cursor.x,
            cursor.y,
            hwnd_id(foreground)
        ),
        attachment,
    ))
}

fn desktop_name(desktop: HDESK) -> Result<String, String> {
    let mut name = [0u16; 256];
    let mut needed = 0u32;
    unsafe {
        GetUserObjectInformationW(
            HANDLE(desktop.0),
            UOI_NAME,
            Some(name.as_mut_ptr().cast()),
            std::mem::size_of_val(&name) as u32,
            Some(&mut needed),
        )
    }
    .map_err(|error| format!("read Windows desktop identity: {error}"))?;
    let length = name
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(name.len());
    Ok(String::from_utf16_lossy(&name[..length]))
}

pub(super) fn preflight_acceptance_hotkey(hotkey: AcceptanceHotkey) -> Result<(), String> {
    let (label, modifiers, key) = match hotkey {
        AcceptanceHotkey::F11 => ("F11", HOT_KEY_MODIFIERS(0), VK_F11),
        AcceptanceHotkey::ShiftAltWinEnd => (
            "Shift+Alt+Win+End",
            // RegisterHotKey MOD_SHIFT | MOD_ALT | MOD_WIN.
            HOT_KEY_MODIFIERS(0x0004 | 0x0001 | 0x0008),
            VK_END,
        ),
    };
    input_modifiers_clear()?;
    if unsafe { GetAsyncKeyState(i32::from(key.0)) } < 0 {
        return Err(format!(
            "refusing hotkey preflight while {label} key is held"
        ));
    }
    unsafe { RegisterHotKey(None, ACCEPTANCE_HOTKEY_ID, modifiers, key.0 as u32) }.map_err(
        |error| format!("acceptance hotkey {label} is already registered or unavailable: {error}"),
    )?;
    let unregister = unsafe { UnregisterHotKey(None, ACCEPTANCE_HOTKEY_ID) }.map_err(|error| {
        format!("release {label} acceptance hotkey preflight registration: {error}")
    });
    input_modifiers_clear()?;
    if unsafe { GetAsyncKeyState(i32::from(key.0)) } < 0 {
        return Err(format!("{label} key became held during hotkey preflight"));
    }
    unregister
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WindowRole {
    Root,
    Designer,
    OtherChild,
}

#[derive(Clone, Debug)]
pub(super) struct WindowSnapshot {
    pub hwnd: HWND,
    pub process_id: u32,
    pub role: WindowRole,
    pub class_name: String,
    pub visible: bool,
    pub minimized: bool,
    pub bounds: [i32; 4],
}

impl WindowSnapshot {
    pub fn is_nonzero(&self) -> bool {
        self.bounds[2] > self.bounds[0] && self.bounds[3] > self.bounds[1]
    }

    pub fn intersects_virtual_screen(&self) -> bool {
        let (left, top, width, height) = virtual_screen_bounds();
        self.bounds[0] < left.saturating_add(width)
            && self.bounds[2] > left
            && self.bounds[1] < top.saturating_add(height)
            && self.bounds[3] > top
    }
}

pub(super) struct NativeChild {
    process: ChildProcessHandle,
    process_id: u32,
    started: SystemTime,
    root: WindowSnapshot,
    log_path: PathBuf,
    desktop_name: String,
}

pub(super) struct NativeLaunchFailure {
    pub process_id: Option<u32>,
    pub started: Option<SystemTime>,
    pub windows: Vec<WindowSnapshot>,
    pub message: String,
}

impl std::fmt::Display for NativeLaunchFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct NativeExitStatus {
    code: u32,
}

impl NativeExitStatus {
    pub fn success(self) -> bool {
        self.code == 0
    }
}

impl std::fmt::Display for NativeExitStatus {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "exit code {}", self.code)
    }
}

struct ChildProcessHandle {
    handle: HANDLE,
    process_id: u32,
}

impl ChildProcessHandle {
    fn try_wait(&self) -> Result<Option<NativeExitStatus>, String> {
        match unsafe { WaitForSingleObject(self.handle, 0) } {
            WAIT_OBJECT_0 => {
                let mut code = 0;
                unsafe { GetExitCodeProcess(self.handle, &mut code) }
                    .map_err(|error| format!("read candidate exit status: {error}"))?;
                Ok(Some(NativeExitStatus { code }))
            }
            WAIT_TIMEOUT => Ok(None),
            status => Err(format!(
                "inspect candidate PID {} wait status: 0x{:08x}",
                self.process_id, status.0
            )),
        }
    }

    fn terminate_and_wait(&self) -> Result<(), String> {
        if self.try_wait()?.is_some() {
            return Ok(());
        }
        unsafe { TerminateProcess(self.handle, 1) }.map_err(|error| {
            format!(
                "terminate isolated candidate PID {}: {error}",
                self.process_id
            )
        })?;
        match unsafe { WaitForSingleObject(self.handle, 5_000) } {
            WAIT_OBJECT_0 => Ok(()),
            WAIT_TIMEOUT => Err(format!(
                "isolated candidate PID {} did not exit after termination request",
                self.process_id
            )),
            status => Err(format!(
                "wait for terminated candidate PID {} returned 0x{:08x}",
                self.process_id, status.0
            )),
        }
    }
}

impl Drop for ChildProcessHandle {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.handle) };
    }
}

pub(super) struct FocusAnchor {
    hwnd: HWND,
    process_id: u32,
    display_bounds: Vec<[i32; 4]>,
    ui_thread_id: u32,
    command_tx: std::sync::mpsc::SyncSender<FocusAnchorCommand>,
    ui_thread: Option<std::thread::JoinHandle<()>>,
}

enum FocusAnchorCommandKind {
    Raise,
    MoveTo { left: i32, top: i32 },
    RestoreNonTopmost,
    Focus,
    Destroy,
}

struct FocusAnchorCommand {
    kind: FocusAnchorCommandKind,
    reply: std::sync::mpsc::SyncSender<Result<(), String>>,
}

impl FocusAnchor {
    pub fn create() -> Result<Self, String> {
        let display_bounds = suite::native_display_bounds()?;
        let process_id = std::process::id();
        let (hwnd, ui_thread_id, command_tx, ui_thread) = spawn_focus_anchor_window(process_id)?;
        if window_process_id(hwnd) != process_id {
            let _ = unsafe { PostThreadMessageW(ui_thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) };
            let _ = ui_thread.join();
            return Err("focus anchor HWND is not owned by the acceptance runner".into());
        }
        let anchor = Self {
            hwnd,
            process_id,
            display_bounds,
            ui_thread_id,
            command_tx,
            ui_thread: Some(ui_thread),
        };
        anchor.raise_for_checked_activation()?;
        let placement = anchor.find_uncovered_client_center();
        let restore_z_order = anchor.restore_non_topmost();
        match (placement, restore_z_order) {
            (Ok((_, evidence)), Ok(())) => {
                tracing::debug!(%evidence, "validated runner focus anchor placement");
                Ok(anchor)
            }
            (Err(error), Ok(())) => Err(error),
            (Ok(_), Err(error)) => Err(format!(
                "focus anchor placement validated but runner anchor z-order could not be restored: {error}"
            )),
            (Err(placement_error), Err(z_order_error)) => Err(format!(
                "{placement_error}; restoring runner anchor z-order also failed: {z_order_error}"
            )),
        }
    }

    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }

    pub fn process_id(&self) -> u32 {
        self.process_id
    }

    pub fn snapshot(&self) -> Option<WindowSnapshot> {
        if window_process_id(self.hwnd) != self.process_id {
            return None;
        }
        let mut rect = RECT::default();
        unsafe { GetWindowRect(self.hwnd, &mut rect) }.ok()?;
        Some(WindowSnapshot {
            hwnd: self.hwnd,
            process_id: self.process_id,
            role: WindowRole::OtherChild,
            class_name: window_class_name(self.hwnd),
            visible: unsafe { IsWindowVisible(self.hwnd) }.as_bool(),
            minimized: unsafe { IsIconic(self.hwnd) }.as_bool(),
            bounds: [rect.left, rect.top, rect.right, rect.bottom],
        })
    }

    pub fn focus(&self) -> Result<(), String> {
        match self
            .send_ui_command(FocusAnchorCommandKind::Focus)
            .and_then(|()| focus_is_validated(self.hwnd, self.process_id))
        {
            Ok(()) => Ok(()),
            Err(focus_error) => self.click_to_activate().map_err(|activation_error| {
                format!(
                    "validated focus transition failed: {focus_error}; checked anchor activation click failed: {activation_error}"
                )
            }),
        }
    }

    fn click_to_activate(&self) -> Result<(), String> {
        if window_process_id(self.hwnd) != self.process_id
            || self.process_id != std::process::id()
            || !unsafe { IsWindowVisible(self.hwnd) }.as_bool()
            || unsafe { IsIconic(self.hwnd) }.as_bool()
        {
            return Err("focus anchor is no longer a visible, runner-owned window".into());
        }
        if let Err(error) = self.raise_for_checked_activation() {
            let restore = self.restore_non_topmost();
            return Err(format!("{error}; z-order recovery={restore:?}"));
        }

        let mut candidate_evidence = String::new();
        let activate = (|| {
            let (point, evidence) = self.find_uncovered_client_center()?;
            candidate_evidence = evidence;
            unsafe { SetCursorPos(point.x, point.y) }
                .map_err(|error| format!("move pointer onto runner anchor: {error}"))?;
            let mut button_guard = MouseButtonGuard::new(self.hwnd, self.process_id);
            let (inserted, down_error) = send_input_checked_prefix(
                &[mouse_input(true)],
                "runner focus-anchor activation mouse down",
            );
            button_guard.armed = inserted != 0;
            if let Some(error) = down_error {
                let cleanup = if button_guard.armed {
                    button_guard.release().map(|_| "released".to_string())
                } else {
                    Ok("no down event was inserted".to_string())
                };
                return Err(match cleanup {
                    Ok(cleanup) => format!("{error}; mouse-up cleanup={cleanup}"),
                    Err(cleanup_error) => {
                        format!("{error}; mouse-up cleanup failed: {cleanup_error}")
                    }
                });
            }
            if inserted != 1 {
                return Err(format!(
                    "runner focus-anchor activation mouse down inserted {inserted}/1 events"
                ));
            }
            button_guard.release().map_err(|error| {
                format!("runner focus-anchor activation mouse up failed: {error}")
            })?;
            wait_for_foreground(self.hwnd, self.process_id)
                .map_err(|error| format!("{error}; candidates={candidate_evidence}"))
        })();
        let restore_z_order = self.restore_non_topmost();
        activate.map_err(|error| {
            if candidate_evidence.is_empty() {
                error
            } else {
                format!("{error}; anchor candidates={candidate_evidence}")
            }
        })?;
        restore_z_order?;
        focus_is_validated(self.hwnd, self.process_id)
            .map_err(|error| format!("{error}; anchor candidates={candidate_evidence}"))
    }

    fn raise_for_checked_activation(&self) -> Result<(), String> {
        self.send_ui_command(FocusAnchorCommandKind::Raise)
    }

    fn restore_non_topmost(&self) -> Result<(), String> {
        self.send_ui_command(FocusAnchorCommandKind::RestoreNonTopmost)
    }

    fn send_ui_command(&self, kind: FocusAnchorCommandKind) -> Result<(), String> {
        let (reply_tx, reply_rx) = std::sync::mpsc::sync_channel(1);
        self.command_tx
            .try_send(FocusAnchorCommand {
                kind,
                reply: reply_tx,
            })
            .map_err(|error| match error {
                std::sync::mpsc::TrySendError::Full(_) => {
                    "focus anchor UI command queue is full".to_string()
                }
                std::sync::mpsc::TrySendError::Disconnected(_) => {
                    "focus anchor UI command thread has exited".to_string()
                }
            })?;
        unsafe {
            PostThreadMessageW(
                self.ui_thread_id,
                FOCUS_ANCHOR_COMMAND_MESSAGE,
                WPARAM(0),
                LPARAM(0),
            )
        }
        .map_err(|error| format!("wake focus anchor UI thread: {error}"))?;
        reply_rx
            .recv_timeout(Duration::from_secs(2))
            .map_err(|error| format!("focus anchor UI command timed out: {error}"))?
    }

    fn find_uncovered_client_center(&self) -> Result<(POINT, String), String> {
        let current = self.snapshot().ok_or_else(|| {
            "focus anchor lost runner ownership while checking physical-display placement"
                .to_string()
        })?;
        let width = current.bounds[2].saturating_sub(current.bounds[0]);
        let height = current.bounds[3].saturating_sub(current.bounds[1]);
        let positions = focus_anchor_candidate_positions(&self.display_bounds, width, height);
        if positions.is_empty() {
            return Err(format!(
                "no physical display can contain runner focus anchor rect size={width}x{height}; displays={:?}",
                self.display_bounds
            ));
        }

        let mut candidate_evidence = Vec::with_capacity(positions.len());
        for (left, top) in positions {
            let requested_rect = [
                left,
                top,
                left.saturating_add(width),
                top.saturating_add(height),
            ];
            if let Err(error) = self.send_ui_command(FocusAnchorCommandKind::MoveTo { left, top }) {
                candidate_evidence.push(format!(
                    "candidate_rect={requested_rect:?} center=unavailable hit=unavailable desktop={:?} placement_error={error}",
                    input_desktop_evidence()
                ));
                continue;
            }
            let point = match self.client_center_screen() {
                Ok(point) => point,
                Err(error) => {
                    candidate_evidence.push(format!(
                        "candidate_rect={requested_rect:?} center=unavailable hit=unavailable desktop={:?} center_error={error}",
                        input_desktop_evidence()
                    ));
                    continue;
                }
            };
            let actual_rect = self
                .snapshot()
                .map(|snapshot| snapshot.bounds)
                .unwrap_or(requested_rect);
            let hit = unsafe { WindowFromPoint(point) };
            let hit_pid = window_process_id(hit);
            let hit_bounds = window_bounds(hit);
            let desktop =
                input_desktop_evidence().unwrap_or_else(|error| format!("unavailable({error})"));
            candidate_evidence.push(format!(
                "candidate_rect={actual_rect:?} center=({}, {}) hit_hwnd={} hit_pid={} hit_class={:?} hit_rect={hit_bounds:?} desktop={desktop}",
                point.x,
                point.y,
                hwnd_id(hit),
                hit_pid,
                window_class_name(hit),
            ));
            if !hit.is_invalid()
                && hit_pid == self.process_id
                && (hit == self.hwnd || unsafe { IsChild(self.hwnd, hit) }.as_bool())
            {
                return Ok((point, candidate_evidence.join(" | ")));
            }
        }
        Err(format!(
            "all bounded physical-display runner focus anchor positions were covered by non-owned windows; anchor_hwnd={} anchor_pid={} candidate_count={} candidates={}",
            hwnd_id(self.hwnd),
            self.process_id,
            candidate_evidence.len(),
            candidate_evidence.join(" | ")
        ))
    }

    fn client_center_screen(&self) -> Result<POINT, String> {
        let mut client = RECT::default();
        unsafe { GetClientRect(self.hwnd, &mut client) }
            .map_err(|error| format!("read runner anchor client bounds: {error}"))?;
        if client.right <= client.left || client.bottom <= client.top {
            return Err("runner anchor has empty client bounds".into());
        }
        let mut point = POINT {
            x: client.left + (client.right - client.left) / 2,
            y: client.top + (client.bottom - client.top) / 2,
        };
        if !unsafe { ClientToScreen(self.hwnd, &mut point) }.as_bool() {
            return Err("convert runner anchor client center to screen coordinates".into());
        }
        Ok(point)
    }
}

fn focus_anchor_candidate_positions(
    displays: &[[i32; 4]],
    anchor_width: i32,
    anchor_height: i32,
) -> Vec<(i32, i32)> {
    const HORIZONTAL_INSET: i32 = 16;
    const TOP_INSET: i32 = 24;
    const BOTTOM_INSET: i32 = 48;
    let mut positions = Vec::new();
    for display in displays {
        let display_width = display[2].saturating_sub(display[0]);
        let display_height = display[3].saturating_sub(display[1]);
        if display_width < anchor_width.saturating_add(HORIZONTAL_INSET * 2)
            || display_height < anchor_height.saturating_add(TOP_INSET + BOTTOM_INSET)
        {
            continue;
        }
        let left = display[0].saturating_add(HORIZONTAL_INSET);
        let right = display[2]
            .saturating_sub(anchor_width)
            .saturating_sub(HORIZONTAL_INSET);
        let middle_x = left.saturating_add(right.saturating_sub(left) / 2);
        let top = display[1].saturating_add(TOP_INSET);
        let bottom = display[3]
            .saturating_sub(anchor_height)
            .saturating_sub(BOTTOM_INSET);
        let middle_y = top.saturating_add(bottom.saturating_sub(top) / 2);
        for y in [top, middle_y, bottom] {
            for x in [left, middle_x, right] {
                if !positions.contains(&(x, y)) {
                    positions.push((x, y));
                }
            }
        }
    }
    positions
}

fn focus_anchor_window_style() -> WINDOW_STYLE {
    WINDOW_STYLE(WS_CAPTION.0 | WS_SYSMENU.0 | WS_VISIBLE.0 | SS_NOTIFY.0)
}

fn spawn_focus_anchor_window(
    process_id: u32,
) -> Result<
    (
        HWND,
        u32,
        std::sync::mpsc::SyncSender<FocusAnchorCommand>,
        std::thread::JoinHandle<()>,
    ),
    String,
> {
    let (thread_id_tx, thread_id_rx) = std::sync::mpsc::sync_channel(1);
    let (window_tx, window_rx) = std::sync::mpsc::sync_channel(1);
    let (command_tx, command_rx) = std::sync::mpsc::sync_channel::<FocusAnchorCommand>(1);
    let ui_thread = std::thread::Builder::new()
        .name("radial-acceptance-focus-anchor".into())
        .spawn(move || {
            let thread_id = unsafe { GetCurrentThreadId() };
            let mut queue_probe = windows::Win32::UI::WindowsAndMessaging::MSG::default();
            let _ = unsafe { PeekMessageW(&mut queue_probe, None, 0, 0, PM_NOREMOVE) };
            if thread_id_tx.send(thread_id).is_err() {
                return;
            }

            let hwnd = unsafe {
                CreateWindowExW(
                    WS_EX_TOOLWINDOW,
                    w!("STATIC"),
                    w!("Radial acceptance focus anchor"),
                    focus_anchor_window_style(),
                    48,
                    48,
                    320,
                    96,
                    HWND::default(),
                    None,
                    None,
                    None,
                )
            };
            let hwnd = match hwnd {
                Ok(hwnd) => hwnd,
                Err(error) => {
                    let _ =
                        window_tx.send(Err(format!("create runner-owned focus anchor: {error}")));
                    return;
                }
            };
            if window_tx.send(Ok(hwnd.0 as usize)).is_err() {
                let _ = unsafe { DestroyWindow(hwnd) };
                return;
            }

            let mut message = windows::Win32::UI::WindowsAndMessaging::MSG::default();
            while unsafe { GetMessageW(&mut message, None, 0, 0) }.0 > 0 {
                if message.message == FOCUS_ANCHOR_COMMAND_MESSAGE {
                    if let Ok(command) = command_rx.try_recv() {
                        let (result, should_exit) =
                            execute_focus_anchor_command(hwnd, process_id, command.kind);
                        let _ = command.reply.send(result);
                        if should_exit {
                            break;
                        }
                    }
                    continue;
                }
                let _ = unsafe { TranslateMessage(&message) };
                unsafe { DispatchMessageW(&message) };
            }
            if window_process_id(hwnd) == process_id {
                let _ = unsafe { DestroyWindow(hwnd) };
            }
        })
        .map_err(|error| format!("start focus anchor UI thread: {error}"))?;

    let thread_id = match thread_id_rx.recv_timeout(Duration::from_secs(2)) {
        Ok(thread_id) => thread_id,
        Err(error) => {
            let _ = ui_thread.join();
            return Err(format!("focus anchor UI thread did not start: {error}"));
        }
    };
    let hwnd = match window_rx.recv_timeout(Duration::from_secs(2)) {
        Ok(Ok(hwnd)) => HWND(hwnd as *mut std::ffi::c_void),
        Ok(Err(error)) => {
            let _ = ui_thread.join();
            return Err(error);
        }
        Err(error) => {
            let _ = unsafe { PostThreadMessageW(thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) };
            let _ = ui_thread.join();
            return Err(format!("focus anchor window creation timed out: {error}"));
        }
    };

    Ok((hwnd, thread_id, command_tx, ui_thread))
}

fn execute_focus_anchor_command(
    hwnd: HWND,
    process_id: u32,
    kind: FocusAnchorCommandKind,
) -> (Result<(), String>, bool) {
    let should_exit = matches!(&kind, FocusAnchorCommandKind::Destroy);
    let result = match kind {
        FocusAnchorCommandKind::Raise => {
            let _ = unsafe { BringWindowToTop(hwnd) };
            unsafe {
                SetWindowPos(
                    hwnd,
                    HWND_TOPMOST,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                )
            }
            .map_err(|error| {
                format!("temporarily raise runner anchor for checked activation: {error}")
            })
        }
        FocusAnchorCommandKind::MoveTo { left, top } => unsafe {
            SetWindowPos(
                hwnd,
                HWND_TOPMOST,
                left,
                top,
                0,
                0,
                SWP_NOSIZE | SWP_NOACTIVATE,
            )
        }
        .map_err(|error| format!("move runner anchor to validated candidate: {error}")),
        FocusAnchorCommandKind::RestoreNonTopmost => unsafe {
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
        .map_err(|error| format!("restore runner anchor non-topmost z-order: {error}")),
        FocusAnchorCommandKind::Focus => focus_window_transition(hwnd, process_id),
        FocusAnchorCommandKind::Destroy => {
            if window_process_id(hwnd) != process_id {
                Err("refused to destroy a focus anchor HWND without runner ownership".into())
            } else {
                Ok(())
            }
        }
    };
    (result, should_exit)
}

impl Drop for FocusAnchor {
    fn drop(&mut self) {
        let destroy = self.send_ui_command(FocusAnchorCommandKind::Destroy);
        if let Err(error) = destroy {
            tracing::warn!(%error, "could not send focus anchor destroy command");
            let _ = unsafe { PostThreadMessageW(self.ui_thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) };
        }
        if let Some(join) = self.ui_thread.take() {
            if !join.is_finished() {
                let _ =
                    unsafe { PostThreadMessageW(self.ui_thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) };
            }
            if join.join().is_err() {
                tracing::warn!("focus anchor UI thread panicked during cleanup");
            }
        }
    }
}

impl NativeChild {
    pub fn launch(
        executable: &Path,
        profile: &Path,
        log_path: &Path,
        stdout_path: &Path,
        stderr_path: &Path,
    ) -> Result<Self, NativeLaunchFailure> {
        let stdout = File::create(stdout_path)
            .map_err(|error| launch_failure(format!("create child stdout log: {error}")))?;
        let stderr = File::create(stderr_path)
            .map_err(|error| launch_failure(format!("create child stderr log: {error}")))?;
        let started = SystemTime::now();
        let process_information = create_acceptance_process(executable, profile, &stdout, &stderr)
            .map_err(|error| launch_failure(format!("launch isolated candidate: {error}")))?;
        let process_id = process_information.dwProcessId;
        let process = ChildProcessHandle {
            handle: process_information.hProcess,
            process_id,
        };
        let _ = unsafe { CloseHandle(process_information.hThread) };

        let root = match wait_for_root(&process, process_id, STARTUP_TIMEOUT) {
            Ok(root) => root,
            Err(error) => {
                let windows = enumerate_process_windows(process_id);
                let cleanup = process.terminate_and_wait();
                return Err(launch_failure_for_child(
                    process_id,
                    started,
                    windows,
                    format!(
                        "ROOT discovery on explicit lpDesktop='WinSta0\\Default': {error}; cleanup={cleanup:?}"
                    ),
                ));
            }
        };
        if !root.is_nonzero() {
            let windows = enumerate_process_windows(process_id);
            let cleanup = process.terminate_and_wait();
            return Err(launch_failure_for_child(
                process_id,
                started,
                windows,
                format!(
                    "ROOT HWND {} has zero-sized bounds {:?}; cleanup={cleanup:?}",
                    hwnd_id(root.hwnd),
                    root.bounds
                ),
            ));
        }
        Ok(Self {
            process,
            process_id,
            started,
            root,
            log_path: log_path.to_path_buf(),
            desktop_name: "WinSta0\\Default (ROOT HWND enumerated on attached input desktop)"
                .into(),
        })
    }

    pub fn process_id(&self) -> u32 {
        self.process_id
    }

    pub fn started(&self) -> SystemTime {
        self.started
    }

    pub fn root(&self) -> &WindowSnapshot {
        &self.root
    }

    pub fn log_path(&self) -> &Path {
        &self.log_path
    }

    pub fn desktop_name(&self) -> &str {
        &self.desktop_name
    }

    pub fn refresh_root(&self) -> Result<WindowSnapshot, String> {
        find_window(self.process_id, WindowRole::Root)
            .ok_or_else(|| "child ROOT HWND is no longer present".to_string())
    }

    pub fn designer(&self) -> Option<WindowSnapshot> {
        find_window(self.process_id, WindowRole::Designer)
    }

    pub fn windows(&self) -> Vec<WindowSnapshot> {
        enumerate_process_windows(self.process_id)
    }

    pub fn foreground_is_child(&self) -> bool {
        let foreground = unsafe { GetForegroundWindow() };
        window_process_id(foreground) == self.process_id
    }

    pub fn focus_window(&self, window: &WindowSnapshot) -> Result<(), String> {
        self.validate_window(window.hwnd)?;
        focus_owned_window(window.hwnd, self.process_id)
    }

    pub fn client_bounds(&self, window: &WindowSnapshot) -> Result<[i32; 4], String> {
        self.validate_window(window.hwnd)?;
        let mut bounds = RECT::default();
        unsafe { GetClientRect(window.hwnd, &mut bounds) }
            .map_err(|error| format!("read child-owned client bounds: {error}"))?;
        Ok([bounds.left, bounds.top, bounds.right, bounds.bottom])
    }

    pub fn client_screen_bounds(&self, window: &WindowSnapshot) -> Result<[i32; 4], String> {
        self.validate_window(window.hwnd)?;
        let mut bounds = RECT::default();
        unsafe { GetClientRect(window.hwnd, &mut bounds) }
            .map_err(|error| format!("read child-owned client bounds: {error}"))?;
        let mut top_left = POINT {
            x: bounds.left,
            y: bounds.top,
        };
        let mut bottom_right = POINT {
            x: bounds.right,
            y: bounds.bottom,
        };
        if !unsafe { ClientToScreen(window.hwnd, &mut top_left) }.as_bool()
            || !unsafe { ClientToScreen(window.hwnd, &mut bottom_right) }.as_bool()
        {
            return Err("convert child-owned client bounds to screen coordinates".into());
        }
        Ok([top_left.x, top_left.y, bottom_right.x, bottom_right.y])
    }

    pub fn resize_window(
        &self,
        window: &WindowSnapshot,
        width: i32,
        height: i32,
    ) -> Result<(), String> {
        self.validate_window(window.hwnd)?;
        if width < 520 || height < 380 {
            return Err("refused Designer size below its declared minimum viewport".into());
        }
        unsafe {
            SetWindowPos(
                window.hwnd,
                None,
                window.bounds[0],
                window.bounds[1],
                width,
                height,
                SWP_NOZORDER | SWP_NOACTIVATE,
            )
        }
        .map_err(|error| format!("resize child-owned Designer window: {error}"))
    }

    pub fn validate_window(&self, hwnd: HWND) -> Result<(), String> {
        if hwnd.is_invalid() || window_process_id(hwnd) != self.process_id {
            return Err("refused input to a window not owned by the acceptance child".to_string());
        }
        Ok(())
    }

    pub fn send_f11(
        &self,
        target_hwnd: HWND,
        target_process_id: u32,
        down_time: Duration,
    ) -> Result<F11TapEvidence, String> {
        if target_process_id != self.process_id && target_process_id != std::process::id() {
            return Err("F11 target must be owned by the acceptance runner or child".into());
        }
        let down = [runner_owned_key_input(VK_F11, Default::default())];
        let down = send_validated_input(target_hwnd, target_process_id, &down, "F11 down")?;
        let mut release_guard = OwnedKeyboardReleaseGuard::new();
        release_guard.owned.push(OwnedKeyboardKey {
            vk: VK_F11,
            extended: false,
        });
        std::thread::sleep(down_time);
        let up = release_guard.release()?;
        Ok(F11TapEvidence { down, up })
    }

    pub fn send_acceptance_hotkey(
        &self,
        target_hwnd: HWND,
        target_process_id: u32,
        hotkey: AcceptanceHotkey,
        dwell: Duration,
    ) -> Result<AcceptanceHotkeyTapEvidence, String> {
        match hotkey {
            AcceptanceHotkey::F11 => {
                let evidence = self.send_f11(target_hwnd, target_process_id, dwell)?;
                Ok(AcceptanceHotkeyTapEvidence {
                    down: evidence.down,
                    up: evidence.up,
                    observed_vks: vec![VK_F11.0 as u32],
                })
            }
            AcceptanceHotkey::ShiftAltWinEnd => {
                if target_process_id != self.process_id && target_process_id != std::process::id() {
                    return Err(
                        "Shift+Alt+Win+End target must be owned by the acceptance runner or child"
                            .into(),
                    );
                }
                send_shift_alt_win_end(target_hwnd, target_process_id, dwell)
            }
        }
    }

    pub fn verify_acceptance_hotkey_released(
        &self,
        hotkey: AcceptanceHotkey,
    ) -> Result<String, String> {
        verify_no_acceptance_hotkey_keys_held(&acceptance_hotkey_keys(hotkey))
    }

    pub fn send_acceptance_direct_trigger(
        &self,
        target_hwnd: HWND,
        target_process_id: u32,
        trigger_key: u16,
        dwell: Duration,
    ) -> Result<AcceptanceHotkeyTapEvidence, String> {
        if target_process_id != self.process_id && target_process_id != std::process::id() {
            return Err(
                "direct-trigger target must be owned by the acceptance runner or child".into(),
            );
        }
        send_acceptance_direct_trigger(
            target_hwnd,
            target_process_id,
            VIRTUAL_KEY(trigger_key),
            dwell,
        )
    }

    pub fn send_acceptance_hotkey_burst(
        &self,
        target_hwnd: HWND,
        target_process_id: u32,
        hotkey: AcceptanceHotkey,
        taps: usize,
        down_time: Duration,
        released_time: Duration,
    ) -> Result<AcceptanceHotkeyBurstEvidence, String> {
        if target_process_id != self.process_id && target_process_id != std::process::id() {
            return Err(
                "hotkey burst target must be owned by the acceptance runner or child".into(),
            );
        }
        if taps == 0 || down_time.is_zero() || released_time.is_zero() {
            return Err("hotkey burst requires taps and positive down/released intervals".into());
        }
        let keys = acceptance_hotkey_keys(hotkey);
        let input_desktop = input_desktop_evidence()?;
        focus_is_validated(target_hwnd, target_process_id)?;
        input_modifiers_clear()?;
        if let Some(held) = keys
            .iter()
            .find(|key| unsafe { GetAsyncKeyState(i32::from(key.vk.0)) < 0 })
        {
            return Err(format!(
                "refusing uninterrupted hotkey burst because key {:?} is already held",
                held.vk
            ));
        }

        let mut owned = Vec::<OwnedKeyboardKey>::with_capacity(keys.len());
        let mut down_inserted = 0usize;
        let mut up_inserted = 0usize;
        let mut next_press = Instant::now();
        for _ in 0..taps {
            sleep_until(next_press);
            let down_events = keys
                .iter()
                .copied()
                .map(OwnedKeyboardKey::down)
                .collect::<Vec<_>>();
            let (inserted, error) =
                send_input_checked_prefix(&down_events, "hotkey burst key-down");
            down_inserted = down_inserted.saturating_add(inserted);
            owned.extend(keys.iter().copied().take(inserted));
            if let Some(error) = error {
                let owned_before_cleanup = describe_owned_keyboard_keys(&owned);
                let cleanup = cleanup_owned_keyboard_keys(&mut owned);
                let physically_down = keys_still_down(&keys);
                let external_interference =
                    keys_down_without_owned_keydowns(&keys, &owned, &physically_down);
                return Err(format!(
                    "hotkey burst key-down failed after inserting {inserted}/{} events: {error}; owned_before_cleanup={owned_before_cleanup}; owned_after_cleanup={}; physical_state_after_cleanup={}; external_interference_without_owned_down={}; cleanup={cleanup}; input_desktop={input_desktop}",
                    keys.len(),
                    describe_owned_keyboard_keys(&owned),
                    describe_owned_keyboard_keys(&physically_down),
                    describe_owned_keyboard_keys(&external_interference)
                ));
            }

            // Start the measured dwell after SendInput has inserted the down
            // edges; synchronous insertion latency must not shorten the hold.
            let press_at = Instant::now();
            sleep_until(press_at + down_time);
            let release_order = keys.iter().rev().copied().collect::<Vec<_>>();
            let up_events = release_order
                .iter()
                .copied()
                .map(OwnedKeyboardKey::up)
                .collect::<Vec<_>>();
            let (inserted, error) = send_input_checked_prefix(&up_events, "hotkey burst key-up");
            up_inserted = up_inserted.saturating_add(inserted);
            let retired = retire_inserted_keyboard_ups(&mut owned, &release_order, inserted);
            if let Some(error) = error {
                let held_before_cleanup = describe_owned_keyboard_keys(&owned);
                let cleanup = cleanup_owned_keyboard_keys(&mut owned);
                let held_after_cleanup = describe_owned_keyboard_keys(&owned);
                let physical_state = describe_keys_still_down(&keys);
                return Err(format!(
                    "hotkey burst key-up failed after inserting {inserted}/{} events (retired {retired} owned downs): {error}; owned_before_cleanup={held_before_cleanup}; owned_after_cleanup={held_after_cleanup}; physical_state_after_cleanup={physical_state}; cleanup={cleanup}; input_desktop={input_desktop}",
                    release_order.len()
                ));
            }
            next_press = Instant::now() + released_time;
        }

        let cleanup = if owned.is_empty() {
            "no_owned_keydowns_remain".to_string()
        } else {
            cleanup_owned_keyboard_keys(&mut owned)
        };
        let still_held = keys_still_down(&keys);
        if !still_held.is_empty() || !owned.is_empty() {
            let external_interference =
                keys_down_without_owned_keydowns(&keys, &owned, &still_held);
            return Err(format!(
                "hotkey burst ended with owned keydowns remaining={}; physical keys still down={}; external interference with no matching owned keydown={}; no extra synthetic key-up was sent; cleanup={cleanup}; input_desktop={input_desktop}",
                describe_owned_keyboard_keys(&owned),
                describe_owned_keyboard_keys(&still_held),
                describe_owned_keyboard_keys(&external_interference)
            ));
        }
        let cleanup = format!("{cleanup};async_state_clear");
        Ok(AcceptanceHotkeyBurstEvidence {
            down_inserted,
            up_inserted,
            input_desktop,
            cleanup,
        })
    }

    pub fn press_hook_sentinel(
        &self,
        target_hwnd: HWND,
        target_process_id: u32,
    ) -> Result<NativeInputEdgeEvidence, String> {
        if target_process_id != self.process_id {
            return Err("F24 hook sentinel target must be owned by the acceptance child".into());
        }
        let events = [key_input(VK_F24, false), key_input(VK_F24, true)];
        send_validated_input(target_hwnd, target_process_id, &events, "F24 hook sentinel")
    }

    pub fn press_f11(
        &self,
        target_hwnd: HWND,
        target_process_id: u32,
    ) -> Result<NativeInputEdgeEvidence, String> {
        validate_f11_target(self.process_id, target_hwnd, target_process_id)?;
        let down = [key_input(VK_F11, false)];
        send_validated_input(target_hwnd, target_process_id, &down, "F11 hold down")
    }

    pub fn release_f11(
        &self,
        expected_foreground_hwnd: HWND,
    ) -> Result<NativeInputEdgeEvidence, String> {
        let foreground = unsafe { GetForegroundWindow() };
        let process_id = window_process_id(foreground);
        if foreground != expected_foreground_hwnd
            || (process_id != self.process_id && process_id != std::process::id())
        {
            return Err(
                "refused F11 release without a validated runner- or child-owned foreground HWND"
                    .into(),
            );
        }
        let up = [key_input(VK_F11, true)];
        send_validated_input(foreground, process_id, &up, "F11 release")
    }

    pub fn try_wait(&self) -> Result<Option<NativeExitStatus>, String> {
        self.process.try_wait()
    }

    pub fn kill(&mut self) -> Result<(), String> {
        self.process.terminate_and_wait()
    }

    pub fn wait(&mut self) -> Result<NativeExitStatus, String> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = self.process.try_wait()? {
                return Ok(status);
            }
            if Instant::now() >= deadline {
                return Err(format!(
                    "timed out waiting for candidate PID {}",
                    self.process_id
                ));
            }
            std::thread::sleep(WINDOW_POLL);
        }
    }
}

impl Drop for NativeChild {
    fn drop(&mut self) {
        if self.process.try_wait().ok().flatten().is_none() {
            let _ = self.process.terminate_and_wait();
        }
    }
}

fn launch_failure(message: String) -> NativeLaunchFailure {
    NativeLaunchFailure {
        process_id: None,
        started: None,
        windows: Vec::new(),
        message,
    }
}

fn launch_failure_for_child(
    process_id: u32,
    started: SystemTime,
    windows: Vec<WindowSnapshot>,
    message: String,
) -> NativeLaunchFailure {
    NativeLaunchFailure {
        process_id: Some(process_id),
        started: Some(started),
        windows,
        message: format!("{message}; isolated child PID {process_id}"),
    }
}

fn create_acceptance_process(
    executable: &Path,
    profile: &Path,
    stdout: &File,
    stderr: &File,
) -> Result<PROCESS_INFORMATION, String> {
    let executable = executable
        .canonicalize()
        .map_err(|error| format!("resolve candidate executable path: {error}"))?;
    let current_directory = profile
        .canonicalize()
        .map_err(|error| format!("resolve isolated profile path: {error}"))?;
    let application = wide_null(executable.as_os_str());
    let current_directory = wide_null(current_directory.as_os_str());
    let mut command_line = quoted_command_line_argument(executable.as_os_str());
    let mut environment = acceptance_environment_block(profile);
    let mut desktop = "WinSta0\\Default"
        .encode_utf16()
        .chain([0])
        .collect::<Vec<_>>();

    let stdout_handle = HANDLE(stdout.as_raw_handle());
    let stderr_handle = HANDLE(stderr.as_raw_handle());
    unsafe {
        SetHandleInformation(stdout_handle, HANDLE_FLAG_INHERIT.0, HANDLE_FLAG_INHERIT)
            .map_err(|error| format!("make candidate stdout log inheritable: {error}"))?;
        SetHandleInformation(stderr_handle, HANDLE_FLAG_INHERIT.0, HANDLE_FLAG_INHERIT)
            .map_err(|error| format!("make candidate stderr log inheritable: {error}"))?;
    }

    let mut startup = STARTUPINFOW {
        cb: std::mem::size_of::<STARTUPINFOW>() as u32,
        lpDesktop: PWSTR(desktop.as_mut_ptr()),
        dwFlags: STARTF_USESTDHANDLES,
        hStdInput: stdout_handle,
        hStdOutput: stdout_handle,
        hStdError: stderr_handle,
        ..Default::default()
    };
    let mut process = PROCESS_INFORMATION::default();
    let created = unsafe {
        CreateProcessW(
            PCWSTR(application.as_ptr()),
            PWSTR(command_line.as_mut_ptr()),
            None,
            None,
            true,
            CREATE_UNICODE_ENVIRONMENT,
            Some(environment.as_mut_ptr().cast()),
            PCWSTR(current_directory.as_ptr()),
            &mut startup,
            &mut process,
        )
    };
    let _ = unsafe { SetHandleInformation(stdout_handle, HANDLE_FLAG_INHERIT.0, HANDLE_FLAGS(0)) };
    let _ = unsafe { SetHandleInformation(stderr_handle, HANDLE_FLAG_INHERIT.0, HANDLE_FLAGS(0)) };
    created.map_err(|error| format!("CreateProcessW on WinSta0\\Default: {error}"))?;
    Ok(process)
}

fn wide_null(value: &std::ffi::OsStr) -> Vec<u16> {
    value.encode_wide().chain([0]).collect()
}

fn quoted_command_line_argument(value: &std::ffi::OsStr) -> Vec<u16> {
    let mut command_line = vec![b'"' as u16];
    let mut backslashes = 0usize;
    for unit in value.encode_wide() {
        if unit == b'\\' as u16 {
            backslashes += 1;
        } else if unit == b'"' as u16 {
            command_line.extend(std::iter::repeat(b'\\' as u16).take(backslashes * 2 + 1));
            command_line.push(unit);
            backslashes = 0;
        } else {
            command_line.extend(std::iter::repeat(b'\\' as u16).take(backslashes));
            command_line.push(unit);
            backslashes = 0;
        }
    }
    command_line.extend(std::iter::repeat(b'\\' as u16).take(backslashes * 2));
    command_line.push(b'"' as u16);
    command_line.push(0);
    command_line
}

fn acceptance_environment_block(profile: &Path) -> Vec<u16> {
    const COPY_CONTROLLED_ENV: [&str; 4] = [
        "ML_NOTES_DIR",
        "ML_NOTE_TEMPLATES_DIR",
        "ML_TMP_DIR",
        "ML_SKIP_CLIPBOARD_SYNC",
    ];
    let mut entries = std::env::vars_os()
        .filter_map(|(name, value)| {
            let wide_name = name.encode_wide().collect::<Vec<_>>();
            if wide_key_eq_ascii(&wide_name, TRACE_ENV)
                || wide_key_eq_ascii(&wide_name, PREPARE_HOLD_ENV)
                || COPY_CONTROLLED_ENV
                    .iter()
                    .any(|key| wide_key_eq_ascii(&wide_name, key))
            {
                None
            } else {
                Some((wide_name, value.encode_wide().collect::<Vec<_>>()))
            }
        })
        .collect::<Vec<_>>();
    entries.push((TRACE_ENV.encode_utf16().collect(), vec![b'1' as u16]));
    entries.push((
        PREPARE_HOLD_ENV.encode_utf16().collect(),
        profile
            .join(PREPARE_HOLD_FILE_NAME)
            .as_os_str()
            .encode_wide()
            .collect(),
    ));
    for (name, relative) in [
        ("ML_NOTES_DIR", "notes"),
        ("ML_NOTE_TEMPLATES_DIR", "note_templates"),
        ("ML_TMP_DIR", "multi_launcher_tmp"),
    ] {
        entries.push((
            name.encode_utf16().collect(),
            profile.join(relative).as_os_str().encode_wide().collect(),
        ));
    }
    entries.push((
        "ML_SKIP_CLIPBOARD_SYNC".encode_utf16().collect(),
        vec![b'1' as u16],
    ));
    entries.sort_by_cached_key(|(name, _)| {
        name.iter()
            .map(|unit| ascii_upper(*unit))
            .collect::<Vec<_>>()
    });

    let mut block = Vec::new();
    for (name, value) in entries {
        block.extend(name);
        block.push(b'=' as u16);
        block.extend(value);
        block.push(0);
    }
    block.push(0);
    if block.len() == 1 {
        block.push(0);
    }
    block
}

fn wide_key_eq_ascii(value: &[u16], expected: &str) -> bool {
    value
        .iter()
        .copied()
        .map(ascii_upper)
        .eq(expected.encode_utf16().map(ascii_upper))
}

fn ascii_upper(unit: u16) -> u16 {
    if (b'a' as u16..=b'z' as u16).contains(&unit) {
        unit - 32
    } else {
        unit
    }
}

fn validate_f11_target(
    child_process_id: u32,
    target_hwnd: HWND,
    target_process_id: u32,
) -> Result<(), String> {
    if target_process_id != child_process_id && target_process_id != std::process::id() {
        return Err("F11 target must be owned by the acceptance runner or child".into());
    }
    focus_is_validated(target_hwnd, target_process_id)?;
    input_modifiers_clear()
}

fn send_input_checked(events: &[INPUT], operation: &str) -> Result<usize, String> {
    let (inserted, error) = send_input_checked_prefix(events, operation);
    match error {
        Some(error) => Err(error),
        None => Ok(inserted),
    }
}

fn send_input_checked_prefix(events: &[INPUT], operation: &str) -> (usize, Option<String>) {
    let inserted = unsafe { SendInput(events, std::mem::size_of::<INPUT>() as i32) } as usize;
    if inserted != events.len() {
        // SendInput may report UIPI blocks without updating GetLastError, so retain both the
        // checked insertion count and the immediate last-error value for diagnosis.
        let last_error = unsafe { GetLastError() }.0;
        return (
            inserted,
            Some(format!(
                "SendInput {operation} inserted {inserted}/{} events (immediate GetLastError=0x{last_error:08x}; UIPI may block without setting it)",
                events.len()
            )),
        );
    }
    (inserted, None)
}

fn unix_time_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn send_validated_input(
    target_hwnd: HWND,
    target_process_id: u32,
    events: &[INPUT],
    operation: &str,
) -> Result<NativeInputEdgeEvidence, String> {
    send_validated_input_allowing_owned_keys(target_hwnd, target_process_id, events, operation, &[])
        .map_err(|(_, error)| error)
}

fn send_validated_input_allowing_owned_keys(
    target_hwnd: HWND,
    target_process_id: u32,
    events: &[INPUT],
    operation: &str,
    owned_keys: &[VIRTUAL_KEY],
) -> Result<NativeInputEdgeEvidence, (usize, String)> {
    let input_desktop = input_desktop_evidence().map_err(|error| (0, error))?;
    focus_is_validated(target_hwnd, target_process_id).map_err(|error| (0, error))?;
    input_modifiers_clear_except(owned_keys).map_err(|error| (0, error))?;
    let (foreground, foreground_pid) = capture_foreground();
    if foreground != target_hwnd || foreground_pid != target_process_id {
        return Err((
            0,
            format!(
                "refused {operation}: target foreground changed before SendInput; expected HWND={} PID={target_process_id}, actual HWND={} PID={foreground_pid}; {input_desktop}",
                hwnd_id(target_hwnd),
                hwnd_id(foreground)
            ),
        ));
    }
    let at_unix_ms = unix_time_ms();
    let mut keyboard_input = events.first().and_then(|event| {
        (event.r#type == INPUT_KEYBOARD).then(|| {
            let keyboard = unsafe { event.Anonymous.ki };
            KeyboardInputEvidence {
                vk: keyboard.wVk.0,
                scan: keyboard.wScan,
                flags: keyboard.dwFlags.0,
                extra_info: keyboard.dwExtraInfo,
                async_state_before: unsafe { GetAsyncKeyState(i32::from(keyboard.wVk.0)) },
                async_state_after: 0,
            }
        })
    });
    let (inserted, error) = send_input_checked_prefix(events, operation);
    if let Some(keyboard) = keyboard_input.as_mut() {
        keyboard.async_state_after = unsafe { GetAsyncKeyState(i32::from(keyboard.vk)) };
    }
    let evidence = NativeInputEdgeEvidence {
        inserted,
        at_unix_ms,
        foreground_hwnd: hwnd_id(foreground),
        foreground_pid,
        input_desktop,
        cleanup_status: "not_required_for_down_edge".into(),
        keyboard_input,
    };
    if let Some(error) = error {
        Err((inserted, error))
    } else {
        Ok(evidence)
    }
}

fn input_desktop_evidence() -> Result<String, String> {
    let thread_desktop = unsafe { GetThreadDesktop(GetCurrentThreadId()) }
        .map_err(|error| format!("read input thread desktop before SendInput: {error}"))?;
    let thread_name = desktop_name(thread_desktop)?;
    let input_desktop = unsafe { OpenInputDesktop(Default::default(), false, DESKTOP_READOBJECTS) }
        .map_err(|error| format!("read active input desktop before SendInput: {error}"))?;
    let active_name = desktop_name(input_desktop);
    let close_result = unsafe { CloseDesktop(input_desktop) };
    let active_name = active_name?;
    close_result.map_err(|error| format!("close input desktop evidence handle: {error}"))?;
    if !thread_name.eq_ignore_ascii_case("Default") || !active_name.eq_ignore_ascii_case("Default")
    {
        return Err(format!(
            "refused native input outside WinSta0\\Default; thread desktop='{thread_name}', active input desktop='{active_name}'"
        ));
    }
    Ok(format!("thread={thread_name};active={active_name}"))
}

fn send_owned_keyboard_release(
    events: &[INPUT],
    operation: &str,
) -> Result<NativeInputEdgeEvidence, (usize, String)> {
    let input_desktop = input_desktop_evidence().map_err(|error| (0, error))?;
    let (foreground, foreground_pid) = capture_foreground();
    let at_unix_ms = unix_time_ms();
    let mut keyboard_input = events.first().and_then(|event| {
        (event.r#type == INPUT_KEYBOARD).then(|| {
            let keyboard = unsafe { event.Anonymous.ki };
            KeyboardInputEvidence {
                vk: keyboard.wVk.0,
                scan: keyboard.wScan,
                flags: keyboard.dwFlags.0,
                extra_info: keyboard.dwExtraInfo,
                async_state_before: unsafe { GetAsyncKeyState(i32::from(keyboard.wVk.0)) },
                async_state_after: 0,
            }
        })
    });
    let (inserted, error) = send_input_checked_prefix(events, operation);
    if let Some(keyboard) = keyboard_input.as_mut() {
        keyboard.async_state_after = unsafe { GetAsyncKeyState(i32::from(keyboard.vk)) };
    }
    if let Some(error) = error {
        return Err((inserted, error));
    }
    Ok(NativeInputEdgeEvidence {
        inserted,
        at_unix_ms,
        foreground_hwnd: hwnd_id(foreground),
        foreground_pid,
        input_desktop,
        cleanup_status: "release_inserted;async_state_to_be_verified_by_owner".into(),
        keyboard_input,
    })
}

fn key_input(key: VIRTUAL_KEY, key_up: bool) -> INPUT {
    key_input_with_flags(
        key,
        if key_up {
            KEYEVENTF_KEYUP
        } else {
            Default::default()
        },
    )
}

fn key_input_with_flags(
    key: VIRTUAL_KEY,
    extra_flags: windows::Win32::UI::Input::KeyboardAndMouse::KEYBD_EVENT_FLAGS,
) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: key,
                wScan: 0,
                dwFlags: extra_flags,
                time: 0,
                // Zero is ordinary SendInput provenance; never use the app's self-injection tag.
                dwExtraInfo: 0,
            },
        },
    }
}

fn runner_owned_key_input(
    key: VIRTUAL_KEY,
    extra_flags: windows::Win32::UI::Input::KeyboardAndMouse::KEYBD_EVENT_FLAGS,
) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: key,
                wScan: 0,
                dwFlags: extra_flags,
                time: 0,
                dwExtraInfo: ACCEPTANCE_RUNNER_INPUT_COOKIE,
            },
        },
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct OwnedKeyboardKey {
    vk: VIRTUAL_KEY,
    extended: bool,
}

impl OwnedKeyboardKey {
    fn down(self) -> INPUT {
        runner_owned_key_input(
            self.vk,
            if self.extended {
                KEYEVENTF_EXTENDEDKEY
            } else {
                Default::default()
            },
        )
    }

    fn up(self) -> INPUT {
        runner_owned_key_input(
            self.vk,
            KEYEVENTF_KEYUP
                | if self.extended {
                    KEYEVENTF_EXTENDEDKEY
                } else {
                    Default::default()
                },
        )
    }
}

fn acceptance_hotkey_keys(hotkey: AcceptanceHotkey) -> Vec<OwnedKeyboardKey> {
    match hotkey {
        AcceptanceHotkey::F11 => vec![OwnedKeyboardKey {
            vk: VK_F11,
            extended: false,
        }],
        AcceptanceHotkey::ShiftAltWinEnd => vec![
            OwnedKeyboardKey {
                vk: VK_LSHIFT,
                extended: false,
            },
            OwnedKeyboardKey {
                vk: VK_LMENU,
                extended: false,
            },
            OwnedKeyboardKey {
                vk: VK_LWIN,
                extended: true,
            },
            OwnedKeyboardKey {
                vk: VK_END,
                extended: true,
            },
        ],
    }
}

fn retire_inserted_keyboard_ups(
    owned: &mut Vec<OwnedKeyboardKey>,
    release_order: &[OwnedKeyboardKey],
    inserted: usize,
) -> usize {
    let mut retired = 0;
    for released in release_order.iter().take(inserted) {
        if let Some(index) = owned.iter().position(|owned_key| owned_key == released) {
            owned.remove(index);
            retired += 1;
        }
    }
    retired
}

fn describe_owned_keyboard_keys(keys: &[OwnedKeyboardKey]) -> String {
    keys.iter()
        .map(|key| format!("{:?}", key.vk))
        .collect::<Vec<_>>()
        .join(",")
}

fn keys_still_down(keys: &[OwnedKeyboardKey]) -> Vec<OwnedKeyboardKey> {
    keys.iter()
        .filter(|key| unsafe { GetAsyncKeyState(i32::from(key.vk.0)) < 0 })
        .copied()
        .collect()
}

fn keys_down_without_owned_keydowns(
    candidates: &[OwnedKeyboardKey],
    owned_keydowns: &[OwnedKeyboardKey],
    physically_down: &[OwnedKeyboardKey],
) -> Vec<OwnedKeyboardKey> {
    candidates
        .iter()
        .filter(|candidate| {
            physically_down.contains(candidate) && !owned_keydowns.contains(candidate)
        })
        .copied()
        .collect()
}

fn describe_keys_still_down(keys: &[OwnedKeyboardKey]) -> String {
    format!("{:?}", describe_owned_keyboard_keys(&keys_still_down(keys)))
}

fn cleanup_owned_keyboard_keys(owned: &mut Vec<OwnedKeyboardKey>) -> String {
    if owned.is_empty() {
        return "no_owned_keys".into();
    }
    let initial = owned.len();
    let mut total_inserted = 0usize;
    let mut attempts = 0usize;
    let mut last_error = None;
    while !owned.is_empty() && attempts < initial.saturating_add(1) {
        attempts += 1;
        if let Err(error) = input_desktop_evidence() {
            last_error = Some(error);
            break;
        }
        let release_order = owned.iter().rev().copied().collect::<Vec<_>>();
        let events = release_order
            .iter()
            .copied()
            .map(OwnedKeyboardKey::up)
            .collect::<Vec<_>>();
        let (inserted, error) = send_input_checked_prefix(&events, "owned hotkey cleanup");
        total_inserted = total_inserted.saturating_add(inserted);
        if error.is_some() {
            last_error = error;
        }
        retire_inserted_keyboard_ups(owned, &release_order, inserted);
        if inserted == 0 {
            break;
        }
    }
    format!(
        "release_attempts={attempts};release_inserted={total_inserted}/{initial};owned_remaining={};physical_state_after_cleanup={};last_error={}",
        describe_owned_keyboard_keys(owned),
        describe_keys_still_down(owned),
        last_error.as_deref().unwrap_or("none")
    )
}

fn verify_no_acceptance_hotkey_keys_held(keys: &[OwnedKeyboardKey]) -> Result<String, String> {
    let physically_down = keys_still_down(keys);
    if physically_down.is_empty() {
        return Ok("async_state_clear_after_owned_release".into());
    }
    let external_interference = keys_down_without_owned_keydowns(keys, &[], &physically_down);
    Err(format!(
        "keys remain physically down after all runner-owned key-up events were inserted: {}; no owned keydown remains, so classify as external physical/key interference and do not send another synthetic key-up",
        describe_owned_keyboard_keys(&external_interference)
    ))
}

fn sleep_until(deadline: Instant) {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if !remaining.is_zero() {
        std::thread::sleep(remaining);
    }
}

fn send_shift_alt_win_end(
    target_hwnd: HWND,
    target_process_id: u32,
    dwell: Duration,
) -> Result<AcceptanceHotkeyTapEvidence, String> {
    const CHORD: [OwnedKeyboardKey; 4] = [
        OwnedKeyboardKey {
            vk: VK_LSHIFT,
            extended: false,
        },
        OwnedKeyboardKey {
            vk: VK_LMENU,
            extended: false,
        },
        OwnedKeyboardKey {
            vk: VK_LWIN,
            extended: true,
        },
        OwnedKeyboardKey {
            vk: VK_END,
            extended: true,
        },
    ];

    focus_is_validated(target_hwnd, target_process_id)?;
    input_modifiers_clear()?;
    if unsafe { GetAsyncKeyState(i32::from(VK_END.0)) } < 0 {
        return Err("refusing Shift+Alt+Win+End while End is already held".into());
    }

    let mut release_guard = OwnedKeyboardReleaseGuard::new();
    let down_events = CHORD.map(OwnedKeyboardKey::down);
    let down = match send_validated_input_allowing_owned_keys(
        target_hwnd,
        target_process_id,
        &down_events,
        "Shift+Alt+Win+End chord down",
        &[],
    ) {
        Ok(evidence) => {
            release_guard.owned.extend(CHORD);
            evidence
        }
        Err((inserted, error)) => {
            release_guard.owned.extend(CHORD.into_iter().take(inserted));
            let cleanup = if release_guard.owned.is_empty() {
                "no_owned_keys".to_string()
            } else {
                release_guard
                    .release()
                    .map(|evidence| format!("released={}", evidence.inserted))
                    .unwrap_or_else(|cleanup_error| cleanup_error)
            };
            return Err(format!("{error}; cleanup={cleanup}"));
        }
    };
    std::thread::sleep(dwell);
    let up = release_guard.release()?;
    Ok(AcceptanceHotkeyTapEvidence {
        down,
        up,
        observed_vks: CHORD.map(|key| key.vk.0 as u32).to_vec(),
    })
}

fn send_acceptance_direct_trigger(
    target_hwnd: HWND,
    target_process_id: u32,
    trigger_key: VIRTUAL_KEY,
    dwell: Duration,
) -> Result<AcceptanceHotkeyTapEvidence, String> {
    let chord = [
        OwnedKeyboardKey {
            vk: VK_LCONTROL,
            extended: false,
        },
        OwnedKeyboardKey {
            vk: VK_LMENU,
            extended: false,
        },
        OwnedKeyboardKey {
            vk: trigger_key,
            extended: false,
        },
    ];

    focus_is_validated(target_hwnd, target_process_id)?;
    input_modifiers_clear()?;
    if chord
        .iter()
        .any(|key| unsafe { GetAsyncKeyState(i32::from(key.vk.0)) } < 0)
    {
        return Err("refusing direct trigger while one of Ctrl+Alt+T is already held".into());
    }

    let mut release_guard = OwnedKeyboardReleaseGuard::new();
    let down_events = chord.map(OwnedKeyboardKey::down);
    let down = match send_validated_input_allowing_owned_keys(
        target_hwnd,
        target_process_id,
        &down_events,
        "acceptance direct-trigger chord down",
        &[],
    ) {
        Ok(evidence) => {
            release_guard.owned.extend(chord);
            evidence
        }
        Err((inserted, error)) => {
            release_guard.owned.extend(chord.into_iter().take(inserted));
            let cleanup = if release_guard.owned.is_empty() {
                "no_owned_keys".to_string()
            } else {
                release_guard
                    .release()
                    .map(|evidence| format!("released={}", evidence.inserted))
                    .unwrap_or_else(|cleanup_error| cleanup_error)
            };
            return Err(format!("{error}; cleanup={cleanup}"));
        }
    };
    std::thread::sleep(dwell);
    let up = release_guard.release()?;
    input_modifiers_clear()?;
    if unsafe { GetAsyncKeyState(i32::from(trigger_key.0)) } < 0 {
        return Err("direct-trigger key remained down after owned release".into());
    }
    Ok(AcceptanceHotkeyTapEvidence {
        down,
        up,
        observed_vks: chord.map(|key| key.vk.0 as u32).to_vec(),
    })
}

struct OwnedKeyboardReleaseGuard {
    owned: Vec<OwnedKeyboardKey>,
}

impl OwnedKeyboardReleaseGuard {
    fn new() -> Self {
        Self { owned: Vec::new() }
    }

    fn release(&mut self) -> Result<NativeInputEdgeEvidence, String> {
        if self.owned.is_empty() {
            return Err("owned keyboard release was requested with no inserted key-downs".into());
        }
        let release_order = self.owned.iter().rev().copied().collect::<Vec<_>>();
        let events = release_order
            .iter()
            .copied()
            .map(OwnedKeyboardKey::up)
            .collect::<Vec<_>>();
        match send_owned_keyboard_release(&events, "owned chord key-up") {
            Ok(mut evidence) => {
                let retired = retire_inserted_keyboard_ups(
                    &mut self.owned,
                    &release_order,
                    release_order.len(),
                );
                if !self.owned.is_empty() {
                    return Err(format!(
                        "inserted all {} release events but retired only {retired} owned keydowns; remaining={}",
                        release_order.len(),
                        describe_owned_keyboard_keys(&self.owned)
                    ));
                }
                let cleanup_status = verify_no_acceptance_hotkey_keys_held(&release_order)?;
                evidence.cleanup_status = cleanup_status;
                Ok(evidence)
            }
            Err((inserted, error)) => {
                let retired =
                    retire_inserted_keyboard_ups(&mut self.owned, &release_order, inserted);
                let held_before_cleanup = self
                    .owned
                    .iter()
                    .map(|key| format!("{:?}", key.vk))
                    .collect::<Vec<_>>();
                let cleanup = cleanup_owned_keyboard_keys(&mut self.owned);
                let held_after_cleanup = self
                    .owned
                    .iter()
                    .map(|key| format!("{:?}", key.vk))
                    .collect::<Vec<_>>();
                let physically_down = keys_still_down(&release_order);
                let external_interference =
                    keys_down_without_owned_keydowns(&release_order, &self.owned, &physically_down);
                Err(format!(
                    "{error}; release_inserted={inserted}/{}; retired={retired}; owned_before_cleanup={held_before_cleanup:?}; owned_after_cleanup={held_after_cleanup:?}; physical_state_after_cleanup={}; external_interference_without_owned_down={}; cleanup={cleanup}",
                    release_order.len(),
                    describe_owned_keyboard_keys(&physically_down),
                    describe_owned_keyboard_keys(&external_interference)
                ))
            }
        }
    }
}

impl Drop for OwnedKeyboardReleaseGuard {
    fn drop(&mut self) {
        if !self.owned.is_empty() {
            let _ = self.release();
        }
    }
}

fn unicode_input(code_unit: u16, key_up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: Default::default(),
                wScan: code_unit,
                dwFlags: KEYEVENTF_UNICODE
                    | if key_up {
                        KEYEVENTF_KEYUP
                    } else {
                        Default::default()
                    },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn held_modifiers_except(allowed: &[VIRTUAL_KEY]) -> String {
    let keys = [
        ("Shift", VK_SHIFT),
        ("LeftShift", VK_LSHIFT),
        ("RightShift", VK_RSHIFT),
        ("Control", VK_CONTROL),
        ("LeftControl", VK_LCONTROL),
        ("RightControl", VK_RCONTROL),
        ("Alt", VK_MENU),
        ("LeftAlt", VK_LMENU),
        ("RightAlt", VK_RMENU),
        ("LeftWin", VK_LWIN),
        ("RightWin", VK_RWIN),
    ];
    let is_allowed = |key: VIRTUAL_KEY| {
        allowed.iter().any(|owned| {
            owned.0 == key.0
                || (key == VK_SHIFT && matches!(*owned, VK_LSHIFT | VK_RSHIFT))
                || (key == VK_CONTROL && matches!(*owned, VK_LCONTROL | VK_RCONTROL))
                || (key == VK_MENU && matches!(*owned, VK_LMENU | VK_RMENU))
        })
    };
    keys.iter()
        .filter(|(_, key)| (unsafe { GetAsyncKeyState(i32::from(key.0)) } < 0) && !is_allowed(*key))
        .map(|(name, _)| *name)
        .collect::<Vec<_>>()
        .join(", ")
}

fn find_window(process_id: u32, role: WindowRole) -> Option<WindowSnapshot> {
    let mut matching = enumerate_process_windows(process_id)
        .into_iter()
        .filter(|window| window.role == role);
    matching.next()
}

fn wait_for_window(
    process_id: u32,
    role: WindowRole,
    timeout: Duration,
) -> Result<WindowSnapshot, String> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(window) = find_window(process_id, role) {
            return Ok(window);
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "no child-owned {} HWND appeared within {} seconds",
                match role {
                    WindowRole::Root => ROOT_TITLE,
                    WindowRole::Designer => DESIGNER_TITLE,
                    WindowRole::OtherChild => "other window",
                },
                timeout.as_secs()
            ));
        }
        std::thread::sleep(WINDOW_POLL);
    }
}

fn wait_for_root(
    child: &ChildProcessHandle,
    process_id: u32,
    timeout: Duration,
) -> Result<WindowSnapshot, String> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(window) = find_window(process_id, WindowRole::Root) {
            return Ok(window);
        }
        if let Some(status) = child.try_wait()? {
            return Err(format!("candidate exited before ROOT appeared ({status})"));
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "no child-owned {} HWND appeared within {} seconds",
                ROOT_TITLE,
                timeout.as_secs()
            ));
        }
        std::thread::sleep(WINDOW_POLL);
    }
}

fn enumerate_process_windows(process_id: u32) -> Vec<WindowSnapshot> {
    let mut state = EnumState {
        process_id,
        windows: Vec::new(),
    };
    let parameter = LPARAM((&mut state as *mut EnumState) as isize);
    let _ = unsafe { EnumWindows(Some(enum_window), parameter) };
    state.windows
}

struct EnumState {
    process_id: u32,
    windows: Vec<WindowSnapshot>,
}

unsafe extern "system" fn enum_window(hwnd: HWND, parameter: LPARAM) -> BOOL {
    let Some(state) = (unsafe { (parameter.0 as *mut EnumState).as_mut() }) else {
        return BOOL(0);
    };
    if state.windows.len() >= MAX_ENUMERATED_WINDOWS {
        return BOOL(0);
    }
    if window_process_id(hwnd) != state.process_id {
        return BOOL(1);
    }
    let role = window_role(hwnd);
    let mut rect = RECT::default();
    let bounds = if unsafe { GetWindowRect(hwnd, &mut rect) }.is_ok() {
        [rect.left, rect.top, rect.right, rect.bottom]
    } else {
        [0, 0, 0, 0]
    };
    state.windows.push(WindowSnapshot {
        hwnd,
        process_id: state.process_id,
        role,
        class_name: window_class_name(hwnd),
        visible: unsafe { IsWindowVisible(hwnd) }.as_bool(),
        minimized: unsafe { IsIconic(hwnd) }.as_bool(),
        bounds,
    });
    BOOL(1)
}

fn window_role(hwnd: HWND) -> WindowRole {
    let title = window_title(hwnd);
    match title.as_str() {
        ROOT_TITLE => WindowRole::Root,
        DESIGNER_TITLE => WindowRole::Designer,
        _ => WindowRole::OtherChild,
    }
}

fn window_title(hwnd: HWND) -> String {
    let length = unsafe { GetWindowTextLengthW(hwnd) }.max(0) as usize;
    if length == 0 || length > 512 {
        return String::new();
    }
    let mut buffer = vec![0_u16; length.saturating_add(1)];
    let copied = unsafe { GetWindowTextW(hwnd, &mut buffer) }.max(0) as usize;
    String::from_utf16_lossy(&buffer[..copied.min(buffer.len())])
}

fn window_class_name(hwnd: HWND) -> String {
    if hwnd.is_invalid() {
        return String::new();
    }
    let mut buffer = [0_u16; 256];
    let copied = unsafe { GetClassNameW(hwnd, &mut buffer) }.max(0) as usize;
    String::from_utf16_lossy(&buffer[..copied.min(buffer.len())])
}

fn window_process_id(hwnd: HWND) -> u32 {
    if hwnd.is_invalid() {
        return 0;
    }
    let mut process_id = 0;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
    process_id
}

fn window_bounds(hwnd: HWND) -> Option<[i32; 4]> {
    if hwnd.is_invalid() {
        return None;
    }
    let mut rect = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut rect) }
        .ok()
        .map(|()| [rect.left, rect.top, rect.right, rect.bottom])
}

fn virtual_screen_bounds() -> (i32, i32, i32, i32) {
    unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    }
}

pub(super) fn hwnd_id(hwnd: HWND) -> u64 {
    hwnd.0 as usize as u64
}

pub(super) fn window_process_id_for(hwnd: HWND) -> u32 {
    window_process_id(hwnd)
}

pub(super) fn wait_for_designer(child: &NativeChild) -> Result<WindowSnapshot, String> {
    wait_for_window(child.process_id, WindowRole::Designer, CASE_TIMEOUT)
}

pub(super) fn wait_until<F>(timeout: Duration, mut condition: F) -> bool
where
    F: FnMut() -> bool,
{
    let deadline = Instant::now() + timeout;
    loop {
        if condition() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(WINDOW_POLL);
    }
}

pub(super) fn find_child_window(child: &NativeChild, role: WindowRole) -> Option<WindowSnapshot> {
    find_window(child.process_id, role)
}

pub(super) fn restore_foreground(hwnd: HWND, expected_pid: u32) -> Result<(), String> {
    if hwnd.is_invalid() || window_process_id(hwnd) != expected_pid {
        return Err("saved foreground HWND no longer belongs to its original process".into());
    }
    focus_window_transition(hwnd, expected_pid)
}

pub(super) fn focus_owned_window(hwnd: HWND, expected_pid: u32) -> Result<(), String> {
    if hwnd.is_invalid() || window_process_id(hwnd) != expected_pid {
        return Err("focus target does not belong to the expected acceptance process".into());
    }
    focus_window_transition(hwnd, expected_pid)
}

fn focus_window_transition(hwnd: HWND, expected_pid: u32) -> Result<(), String> {
    if hwnd.is_invalid() || window_process_id(hwnd) != expected_pid {
        return Err(
            "foreground transition target no longer belongs to its validated process".into(),
        );
    }
    let _ = unsafe { SetForegroundWindow(hwnd) };
    if focus_is_validated(hwnd, expected_pid).is_ok() {
        return Ok(());
    }

    let mut foreground = unsafe { GetForegroundWindow() };
    if foreground.is_invalid() {
        if wait_for_foreground(hwnd, expected_pid).is_ok() {
            return Ok(());
        }
        foreground = unsafe { GetForegroundWindow() };
        if foreground.is_invalid() {
            return retry_set_foreground(hwnd, expected_pid);
        }
    }
    let current_thread = unsafe { GetCurrentThreadId() };
    let foreground_thread = unsafe { GetWindowThreadProcessId(foreground, None) };
    if foreground_thread == 0 {
        return Err(
            "could not identify the current foreground thread for a validated focus transition"
                .into(),
        );
    }
    let attachment = match InputThreadAttachment::attach(current_thread, foreground_thread) {
        Ok(attachment) => attachment,
        Err(attach_error) => {
            return retry_set_foreground(hwnd, expected_pid).map_err(|retry_error| {
                format!(
                    "foreground attachment failed from runner thread {current_thread} to HWND={} PID={} thread {foreground_thread}: {attach_error}; direct foreground retry failed: {retry_error}",
                    hwnd_id(foreground),
                    window_process_id(foreground)
                )
            });
        }
    };
    let _ = unsafe { SetForegroundWindow(hwnd) };
    let focus_result = wait_for_foreground(hwnd, expected_pid);
    let detach_result = attachment.detach();
    detach_result?;
    match focus_result {
        Ok(()) => Ok(()),
        Err(focus_error) => retry_set_foreground(hwnd, expected_pid).map_err(|retry_error| {
            format!("attached foreground transition failed: {focus_error}; direct retry failed: {retry_error}")
        }),
    }
}

fn retry_set_foreground(hwnd: HWND, expected_pid: u32) -> Result<(), String> {
    let deadline = Instant::now() + FOREGROUND_TRANSITION_TIMEOUT;
    loop {
        let _ = unsafe { SetForegroundWindow(hwnd) };
        if focus_is_validated(hwnd, expected_pid).is_ok() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return focus_is_validated(hwnd, expected_pid);
        }
        std::thread::sleep(WINDOW_POLL);
    }
}

fn wait_for_foreground(hwnd: HWND, expected_pid: u32) -> Result<(), String> {
    let deadline = Instant::now() + FOREGROUND_TRANSITION_TIMEOUT;
    loop {
        if focus_is_validated(hwnd, expected_pid).is_ok() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return focus_is_validated(hwnd, expected_pid);
        }
        std::thread::sleep(WINDOW_POLL);
    }
}

pub(super) fn request_window_close(
    child: &NativeChild,
    target: &WindowSnapshot,
) -> Result<(), String> {
    child.validate_window(target.hwnd)?;
    unsafe { PostMessageW(target.hwnd, WM_CLOSE, WPARAM(0), LPARAM(0)) }
        .map_err(|error| format!("post bounded WM_CLOSE to child-owned HWND: {error}"))
}

pub(super) fn focus_is_validated(hwnd: HWND, expected_pid: u32) -> Result<(), String> {
    let foreground = unsafe { GetForegroundWindow() };
    if foreground != hwnd || window_process_id(foreground) != expected_pid {
        return Err(format!(
            "foreground mismatch: expected owned HWND={} PID={}, actual HWND={} PID={}",
            hwnd_id(hwnd),
            expected_pid,
            hwnd_id(foreground),
            window_process_id(foreground)
        ));
    }
    Ok(())
}

struct InputThreadAttachment {
    source_thread: u32,
    target_thread: u32,
    attached: bool,
}

impl InputThreadAttachment {
    fn attach(source_thread: u32, target_thread: u32) -> Result<Self, String> {
        if source_thread == target_thread {
            return Ok(Self {
                source_thread,
                target_thread,
                attached: false,
            });
        }
        if !unsafe { AttachThreadInput(source_thread, target_thread, BOOL(1)) }.as_bool() {
            return Err(format!(
                "could not attach runner input queue to current foreground thread: {}",
                std::io::Error::last_os_error()
            ));
        }
        Ok(Self {
            source_thread,
            target_thread,
            attached: true,
        })
    }

    fn detach(mut self) -> Result<(), String> {
        if self.attached
            && !unsafe { AttachThreadInput(self.source_thread, self.target_thread, BOOL(0)) }
                .as_bool()
        {
            return Err(format!(
                "could not detach runner input queue from foreground thread: {}",
                std::io::Error::last_os_error()
            ));
        }
        self.attached = false;
        Ok(())
    }
}

impl Drop for InputThreadAttachment {
    fn drop(&mut self) {
        if self.attached {
            let _ = unsafe { AttachThreadInput(self.source_thread, self.target_thread, BOOL(0)) };
            self.attached = false;
        }
    }
}

pub(super) fn capture_foreground() -> (HWND, u32) {
    let hwnd = unsafe { GetForegroundWindow() };
    (hwnd, window_process_id(hwnd))
}

pub(super) fn cursor_position() -> Result<POINT, String> {
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
    let mut point = POINT::default();
    unsafe { GetCursorPos(&mut point) }
        .map_err(|error| format!("read cursor position: {error}"))?;
    Ok(point)
}

pub(super) fn set_cursor_position(point: POINT) -> Result<(), String> {
    use windows::Win32::UI::WindowsAndMessaging::SetCursorPos;
    let clip_bounds = cursor_clip_bounds();
    let input_target = cursor_restore_input_target(point, virtual_screen_bounds())?;
    let direct_error = unsafe { SetCursorPos(input_target.x, input_target.y) }
        .err()
        .map(|error| error.to_string());
    if cursor_position().is_ok_and(|current| cursor_points_match(current, point)) {
        return Ok(());
    }

    let input_desktop = input_desktop_evidence()
        .map_err(|error| format!("restore cursor position after {direct_error:?}: {error}"))?;
    let movement = mouse_move_input(input_target)?;
    let absolute_inserted = send_input_checked(&[movement], "absolute cursor restoration")?;
    let mut current = cursor_position()?;
    if cursor_points_match(current, point) {
        return Ok(());
    }

    let mut relative_inserted = 0;
    for _ in 0..4 {
        let correction = relative_mouse_move_input(
            input_target.x.saturating_sub(current.x),
            input_target.y.saturating_sub(current.y),
        );
        relative_inserted += send_input_checked(&[correction], "relative cursor restoration")?;
        current = cursor_position()?;
        if cursor_points_match(current, point) {
            return Ok(());
        }
    }

    Err(format!(
        "cursor restoration did not reach ({},{}): input target=({},{}), SetCursorPos={direct_error:?}, absolute SendInput inserted {absolute_inserted} event, relative SendInput inserted {relative_inserted} events on {input_desktop}, observed=({}, {}), clip_bounds={clip_bounds:?}, virtual_screen_bounds={:?}",
        point.x,
        point.y,
        input_target.x,
        input_target.y,
        current.x,
        current.y,
        virtual_screen_bounds()
    ))
}

fn cursor_clip_bounds() -> Result<[i32; 4], String> {
    let mut bounds = RECT::default();
    unsafe { GetClipCursor(&mut bounds) }
        .map_err(|error| format!("read cursor clip bounds: {error}"))?;
    Ok([bounds.left, bounds.top, bounds.right, bounds.bottom])
}

fn cursor_points_match(current: POINT, expected: POINT) -> bool {
    current.x.abs_diff(expected.x) <= 1 && current.y.abs_diff(expected.y) <= 1
}

fn cursor_restore_input_target(
    expected: POINT,
    (left, top, width, height): (i32, i32, i32, i32),
) -> Result<POINT, String> {
    if width <= 0 || height <= 0 {
        return Err("virtual desktop has invalid bounds for cursor restoration".into());
    }
    let clamp_inside = |position: i32, origin: i32, extent: i32| {
        let last = origin.saturating_add(extent.saturating_sub(1));
        if extent >= 3 {
            position.clamp(origin.saturating_add(1), last.saturating_sub(1))
        } else {
            position.clamp(origin, last)
        }
    };
    Ok(POINT {
        x: clamp_inside(expected.x, left, width),
        y: clamp_inside(expected.y, top, height),
    })
}

pub(super) fn input_modifiers_clear() -> Result<(), String> {
    input_modifiers_clear_except(&[])
}

fn input_modifiers_clear_except(allowed: &[VIRTUAL_KEY]) -> Result<(), String> {
    let held = held_modifiers_except(allowed);
    if held.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "refusing native input while modifiers are held: {held}"
        ))
    }
}

pub(super) struct UiAutomation {
    automation: IUIAutomation,
    _apartment: ComApartment,
}

struct ComApartment;

impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

#[derive(Clone)]
pub(super) struct SemanticControl {
    element: IUIAutomationElement,
    pub bounds: [i32; 4],
    pub process_id: u32,
    pub enabled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AuthoringControlTarget {
    NewMenu,
    MenuAfterAction,
    AfterActionOption,
    AddRing,
    MenuRow,
    RingSelector,
    RingOption,
    Slots,
    PreviewProposal,
    ApplyProposal,
    CancelProposal,
    MoveToOverflow,
    CancelResolution,
    DiscardCells,
    Canvas,
    CanvasCell,
    DiscardDraft,
    CellType,
    ActionTypeOption,
    ActionSearch,
    ActionRow,
    PopupApply,
    PopupCancel,
    PopupOpenInspector,
    PopupApplyAndOpen,
    PopupDiscardAndOpen,
    PopupKeepEditing,
    InspectorCell,
    SkinRow,
    SkinGlowEnabled,
    OpenDesktopPreview,
    StopDesktopPreview,
    Undo,
    Redo,
    Save,
    KeepEditing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AuthoringControlRole {
    Button,
    Selectable,
    ComboBox,
    DragValue,
    Region,
    TextEdit,
    Checkbox,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct AuthoringControlSnapshot {
    pub target: AuthoringControlTarget,
    pub role: AuthoringControlRole,
    pub index: Option<usize>,
    pub bounds: [i32; 4],
    pub client_size: [i32; 2],
    pub enabled: bool,
    pub selected: bool,
    pub focused: bool,
    pub clicked: bool,
    pub session_id: u64,
    pub generation: u64,
    pub menu_cell_ids_digest: Option<u64>,
    pub ring_index: Option<usize>,
    pub slot_index: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct DesignerCanvasAllocationSnapshot {
    pub allocated_rect: [i32; 4],
    pub clip_rect: [i32; 4],
    pub requested_size: [i32; 2],
    pub session_id: u64,
    pub generation: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ActionCatalogRankSnapshot {
    pub custom_action_index: usize,
    pub rank: usize,
    pub catalog_len: usize,
    pub session_id: u64,
    pub generation: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AuthoringProposalKind {
    None,
    NewRing,
    Resize,
    ResolvedResize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct GeometryStateSnapshot {
    pub session_id: u64,
    pub menu_count: usize,
    pub selected_menu_index: Option<usize>,
    pub selected_menu_after_action: Option<multi_launcher::radial::model::AfterActionPolicy>,
    pub ring_count: usize,
    pub selected_ring_index: Option<usize>,
    pub selected_cell_index: Option<usize>,
    pub selected_cell_id_digest: Option<u64>,
    pub selected_cell_custom_action_index: Option<usize>,
    pub selected_cell_custom_action_index_known: bool,
    pub selected_ring_slots: usize,
    pub requested_slots: usize,
    pub selected_ring_populated: usize,
    pub menu_populated: usize,
    pub draft_cell_ids_digest: u64,
    pub proposal_cell_ids_digest: u64,
    pub proposal_cell_ids_digest_available: bool,
    pub proposal_kind: AuthoringProposalKind,
    pub proposal_active: bool,
    pub proposal_ready: bool,
    pub proposal_slots: usize,
    pub proposal_candidate_rings: usize,
    pub proposal_resolution_populated: usize,
    pub proposal_cell_ids_preserved: bool,
    pub resize_prompt_open: bool,
    pub resize_prompt_populated: usize,
    pub generation: u64,
}

impl UiAutomation {
    pub fn new() -> Result<Self, String> {
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
            .ok()
            .map_err(|error| format!("initialize UI Automation COM apartment: {error}"))?;
        let apartment = ComApartment;
        let automation: IUIAutomation =
            unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }
                .map_err(|error| format!("create UI Automation client: {error}"))?;
        if let Ok(automation2) =
            automation.cast::<windows::Win32::UI::Accessibility::IUIAutomation2>()
        {
            let _ = unsafe { automation2.SetConnectionTimeout(750) };
            let _ = unsafe { automation2.SetTransactionTimeout(1_500) };
        }
        Ok(Self {
            automation,
            _apartment: apartment,
        })
    }

    pub fn root_is_queryable(&self, hwnd: HWND, expected_pid: u32) -> bool {
        let Ok(element) = (unsafe { self.automation.ElementFromHandle(hwnd) }) else {
            return false;
        };
        unsafe { element.CurrentProcessId() }
            .ok()
            .is_some_and(|pid| pid == expected_pid as i32)
    }

    pub fn describe_tree(&self, hwnd: HWND) -> Result<String, String> {
        let root = unsafe { self.automation.ElementFromHandle(hwnd) }
            .map_err(|error| format!("query UI Automation root for diagnostic: {error}"))?;
        let mut snapshot = String::new();
        writeln!(snapshot, "root {}", describe_uia_element(&root))
            .map_err(|error| format!("format UI Automation root diagnostic: {error}"))?;

        for (view, walker) in [
            (
                "raw",
                unsafe { self.automation.RawViewWalker() }
                    .map_err(|error| format!("create raw UIA diagnostic walker: {error}"))?,
            ),
            (
                "control",
                unsafe { self.automation.ControlViewWalker() }
                    .map_err(|error| format!("create control UIA diagnostic walker: {error}"))?,
            ),
        ] {
            writeln!(snapshot, "{view} view")
                .map_err(|error| format!("format UI Automation view diagnostic: {error}"))?;
            let mut visited = 0;
            append_uia_tree(&walker, &root, view, 1, &mut visited, &mut snapshot);
            if visited == 0 {
                writeln!(snapshot, "  <no descendant elements>")
                    .map_err(|error| format!("format empty UI Automation view: {error}"))?;
            }
        }
        Ok(snapshot)
    }

    pub fn find_named(
        &self,
        hwnd: HWND,
        expected_pid: u32,
        name: &str,
    ) -> Result<Option<SemanticControl>, String> {
        let root = unsafe { self.automation.ElementFromHandle(hwnd) }
            .map_err(|error| format!("query UI Automation root: {error}"))?;
        let condition = unsafe {
            self.automation
                .CreatePropertyCondition(UIA_NamePropertyId, &VARIANT::from(name))
        }
        .map_err(|error| format!("create semantic UI Automation condition: {error}"))?;
        let matches = unsafe { root.FindAll(TreeScope_Descendants, &condition) }
            .map_err(|error| format!("find semantic control: {error}"))?;
        let count = unsafe { matches.Length() }
            .map_err(|error| format!("read semantic match count: {error}"))?
            .min(2_048);
        for index in 0..count {
            let element = unsafe { matches.GetElement(index) }
                .map_err(|error| format!("read semantic match: {error}"))?;
            let process_id = unsafe { element.CurrentProcessId() }
                .map_err(|error| format!("read semantic control process: {error}"))?;
            if process_id != expected_pid as i32 {
                continue;
            }
            let bounds = unsafe { element.CurrentBoundingRectangle() }
                .map_err(|error| format!("read semantic control bounds: {error}"))?;
            let enabled = unsafe { element.CurrentIsEnabled() }
                .map_err(|error| format!("read semantic control enabled state: {error}"))?
                .as_bool();
            return Ok(Some(SemanticControl {
                element,
                bounds: [bounds.left, bounds.top, bounds.right, bounds.bottom],
                process_id: expected_pid,
                enabled,
            }));
        }
        Ok(None)
    }

    /// Wait for an owned UIA element whose accessible name contains a stable command label.
    /// Search-result containers may include profile-dependent descriptions or punctuation, so
    /// callers should match the production action label and then verify the action's outcome.
    pub fn wait_named_containing(
        &self,
        hwnd: HWND,
        expected_pid: u32,
        name_fragment: &str,
        timeout: Duration,
    ) -> Result<SemanticControl, String> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(control) = self.find_named_containing(hwnd, expected_pid, name_fragment)? {
                return Ok(control);
            }
            if Instant::now() >= deadline {
                return Err(
                    "UIA did not publish the expected radial command result before timeout".into(),
                );
            }
            std::thread::sleep(WINDOW_POLL);
        }
    }

    fn find_named_containing(
        &self,
        hwnd: HWND,
        expected_pid: u32,
        name_fragment: &str,
    ) -> Result<Option<SemanticControl>, String> {
        let root = unsafe { self.automation.ElementFromHandle(hwnd) }
            .map_err(|error| format!("query UI Automation root: {error}"))?;
        let condition = unsafe { self.automation.CreateTrueCondition() }
            .map_err(|error| format!("create UIA descendant condition: {error}"))?;
        let matches = unsafe { root.FindAll(TreeScope_Descendants, &condition) }
            .map_err(|error| format!("find UIA descendants: {error}"))?;
        let count = unsafe { matches.Length() }
            .map_err(|error| format!("read UIA descendant count: {error}"))?
            .min(2_048);
        let name_fragment = name_fragment.to_lowercase();
        let mut found: Option<SemanticControl> = None;
        for index in 0..count {
            let element = unsafe { matches.GetElement(index) }
                .map_err(|error| format!("read UIA descendant: {error}"))?;
            let process_id = unsafe { element.CurrentProcessId() }
                .map_err(|error| format!("read UIA descendant process: {error}"))?;
            if process_id != expected_pid as i32 {
                continue;
            }
            let name = unsafe { element.CurrentName() }
                .map_err(|error| format!("read UIA descendant name: {error}"))?
                .to_string();
            if !semantic_name_contains(&name, &name_fragment) {
                continue;
            }
            let bounds = unsafe { element.CurrentBoundingRectangle() }
                .map_err(|error| format!("read matching UIA bounds: {error}"))?;
            if bounds.right <= bounds.left || bounds.bottom <= bounds.top {
                continue;
            }
            let enabled = unsafe { element.CurrentIsEnabled() }
                .map_err(|error| format!("read matching UIA enabled state: {error}"))?
                .as_bool();
            let candidate = SemanticControl {
                element,
                bounds: [bounds.left, bounds.top, bounds.right, bounds.bottom],
                process_id: expected_pid,
                enabled,
            };
            if let Some(previous) = found.as_ref() {
                if previous.bounds != candidate.bounds {
                    return Err(
                        "UIA radial command label matched multiple distinct result controls".into(),
                    );
                }
            } else {
                found = Some(candidate);
            }
        }
        Ok(found)
    }

    pub fn edit_value_matches(
        &self,
        control: &SemanticControl,
        expected: &str,
    ) -> Result<bool, String> {
        let pattern = unsafe {
            control
                .element
                .GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
        }
        .map_err(|error| format!("read UIA edit value pattern: {error}"))?;
        let value = unsafe { pattern.CurrentValue() }
            .map_err(|error| format!("read UIA edit value: {error}"))?;
        Ok(value.to_string() == expected)
    }

    pub fn wait_edit_value(
        &self,
        control: &SemanticControl,
        expected: &str,
        timeout: Duration,
    ) -> Result<bool, String> {
        let deadline = Instant::now() + timeout;
        loop {
            if self.edit_value_matches(control, expected)? {
                return Ok(true);
            }
            if Instant::now() >= deadline {
                return Ok(false);
            }
            std::thread::sleep(WINDOW_POLL);
        }
    }

    pub fn find_first_edit(
        &self,
        hwnd: HWND,
        expected_pid: u32,
    ) -> Result<Option<SemanticControl>, String> {
        let root = unsafe { self.automation.ElementFromHandle(hwnd) }
            .map_err(|error| format!("query UI Automation root: {error}"))?;
        let condition = unsafe {
            self.automation.CreatePropertyCondition(
                windows::Win32::UI::Accessibility::UIA_ControlTypePropertyId,
                &VARIANT::from(UIA_EditControlTypeId.0),
            )
        }
        .map_err(|error| format!("create UIA edit condition: {error}"))?;
        let matches = unsafe { root.FindAll(TreeScope_Descendants, &condition) }
            .map_err(|error| format!("find UIA edit control: {error}"))?;
        let count = unsafe { matches.Length() }
            .map_err(|error| format!("read UIA edit-control count: {error}"))?
            .min(2_048);
        for index in 0..count {
            let element = unsafe { matches.GetElement(index) }
                .map_err(|error| format!("read UIA edit element: {error}"))?;
            let process_id = unsafe { element.CurrentProcessId() }
                .map_err(|error| format!("read UIA edit process: {error}"))?;
            if process_id != expected_pid as i32 {
                continue;
            }
            let bounds = unsafe { element.CurrentBoundingRectangle() }
                .map_err(|error| format!("read UIA edit bounds: {error}"))?;
            let enabled = unsafe { element.CurrentIsEnabled() }
                .map_err(|error| format!("read UIA edit enabled state: {error}"))?
                .as_bool();
            return Ok(Some(SemanticControl {
                element,
                bounds: [bounds.left, bounds.top, bounds.right, bounds.bottom],
                process_id: expected_pid,
                enabled,
            }));
        }
        Ok(None)
    }

    pub fn edit_focus_at_screen_point(
        &self,
        hwnd: HWND,
        expected_pid: u32,
        point: (i32, i32),
    ) -> Result<Option<bool>, String> {
        let root = unsafe { self.automation.ElementFromHandle(hwnd) }
            .map_err(|error| format!("query UI Automation root for edit focus: {error}"))?;
        let condition = unsafe {
            self.automation.CreatePropertyCondition(
                windows::Win32::UI::Accessibility::UIA_ControlTypePropertyId,
                &VARIANT::from(UIA_EditControlTypeId.0),
            )
        }
        .map_err(|error| format!("create UIA edit-focus condition: {error}"))?;
        let matches = unsafe { root.FindAll(TreeScope_Descendants, &condition) }
            .map_err(|error| format!("find UIA edit-focus target: {error}"))?;
        let count = unsafe { matches.Length() }
            .map_err(|error| format!("read UIA edit-focus count: {error}"))?
            .min(2_048);
        for index in 0..count {
            let element = unsafe { matches.GetElement(index) }
                .map_err(|error| format!("read UIA edit-focus element: {error}"))?;
            let process_id = unsafe { element.CurrentProcessId() }
                .map_err(|error| format!("read UIA edit-focus process: {error}"))?;
            if process_id != expected_pid as i32 {
                continue;
            }
            let bounds = unsafe { element.CurrentBoundingRectangle() }
                .map_err(|error| format!("read UIA edit-focus bounds: {error}"))?;
            if point.0 >= bounds.left
                && point.0 < bounds.right
                && point.1 >= bounds.top
                && point.1 < bounds.bottom
            {
                let focused = unsafe { element.CurrentHasKeyboardFocus() }
                    .map_err(|error| format!("read UIA edit-focus state: {error}"))?
                    .as_bool();
                return Ok(Some(focused));
            }
        }
        Ok(None)
    }

    pub fn focus(&self, control: &SemanticControl) -> Result<(), String> {
        if !control.enabled || control.process_id == 0 {
            return Err("refusing to focus a disabled or unowned UIA element".into());
        }
        unsafe { control.element.SetFocus() }
            .map_err(|error| format!("focus semantic UIA control: {error}"))
    }

    pub fn focused_element(&self) -> Result<IUIAutomationElement, String> {
        unsafe { self.automation.GetFocusedElement() }
            .map_err(|error| format!("read UIA keyboard focus: {error}"))
    }

    pub fn element_process_id(&self, element: &IUIAutomationElement) -> Option<u32> {
        unsafe { element.CurrentProcessId() }
            .ok()
            .and_then(|pid| u32::try_from(pid).ok())
    }

    pub fn same_element(
        &self,
        left: &IUIAutomationElement,
        right: &IUIAutomationElement,
    ) -> Result<bool, String> {
        unsafe { self.automation.CompareElements(left, right) }
            .map(|same| same.as_bool())
            .map_err(|error| format!("compare focused UIA elements: {error}"))
    }

    pub fn element_name(&self, element: &IUIAutomationElement) -> Option<String> {
        unsafe { element.CurrentName() }
            .ok()
            .map(|name| name.to_string())
    }

    pub fn element_focusable(&self, element: &IUIAutomationElement) -> bool {
        unsafe { element.CurrentIsKeyboardFocusable() }.is_ok_and(|value| value.as_bool())
    }

    pub fn element_has_focus(&self, element: &IUIAutomationElement) -> bool {
        unsafe { element.CurrentHasKeyboardFocus() }.is_ok_and(|value| value.as_bool())
    }

    pub fn selection_state(&self, control: &SemanticControl) -> Option<bool> {
        let pattern = unsafe {
            control
                .element
                .GetCurrentPatternAs::<IUIAutomationSelectionItemPattern>(
                    UIA_SelectionItemPatternId,
                )
        }
        .ok()?;
        unsafe { pattern.CurrentIsSelected() }
            .ok()
            .map(|value| value.as_bool())
    }

    pub fn toggle_state(&self, control: &SemanticControl) -> Option<ToggleState> {
        let pattern = unsafe {
            control
                .element
                .GetCurrentPatternAs::<IUIAutomationTogglePattern>(UIA_TogglePatternId)
        }
        .ok()?;
        unsafe { pattern.CurrentToggleState() }.ok()
    }
}

fn semantic_name_contains(name: &str, fragment: &str) -> bool {
    name.to_lowercase().contains(&fragment.to_lowercase())
}

fn describe_uia_element(element: &IUIAutomationElement) -> String {
    let process_id = unsafe { element.CurrentProcessId() }.unwrap_or_default();
    let control_type = unsafe { element.CurrentControlType() }
        .map(|value| value.0)
        .unwrap_or_default();
    let bounds = unsafe { element.CurrentBoundingRectangle() }
        .map(|value| [value.left, value.top, value.right, value.bottom])
        .unwrap_or_default();
    let focusable =
        unsafe { element.CurrentIsKeyboardFocusable() }.is_ok_and(|value| value.as_bool());
    let focused = unsafe { element.CurrentHasKeyboardFocus() }.is_ok_and(|value| value.as_bool());
    let control = unsafe { element.CurrentIsControlElement() }.is_ok_and(|value| value.as_bool());
    let content = unsafe { element.CurrentIsContentElement() }.is_ok_and(|value| value.as_bool());
    let name = unsafe { element.CurrentName() }
        .map(|value| value.to_string())
        .unwrap_or_default();
    let class = unsafe { element.CurrentClassName() }
        .map(|value| value.to_string())
        .unwrap_or_default();
    let framework = unsafe { element.CurrentFrameworkId() }
        .map(|value| value.to_string())
        .unwrap_or_default();
    let automation_id = unsafe { element.CurrentAutomationId() }
        .map(|value| value.to_string())
        .unwrap_or_default();
    format_uia_element_snapshot(
        process_id,
        control_type,
        bounds,
        focusable,
        focused,
        control,
        content,
        &name,
        &class,
        &framework,
        &automation_id,
    )
}

#[allow(clippy::too_many_arguments)]
fn format_uia_element_snapshot(
    process_id: i32,
    control_type: i32,
    bounds: [i32; 4],
    focusable: bool,
    focused: bool,
    control: bool,
    content: bool,
    _name: &str,
    _class: &str,
    _framework: &str,
    _automation_id: &str,
) -> String {
    // UIA names, class names and automation IDs can contain user-authored
    // labels. Keep their structural presence visible without persisting them.
    format!(
        "pid={process_id} type={control_type} bounds={bounds:?} focusable={focusable} focused={focused} control={control} content={content} name=<redacted> class=<redacted> framework=<redacted> automation_id=<redacted>"
    )
}

fn append_uia_tree(
    walker: &IUIAutomationTreeWalker,
    parent: &IUIAutomationElement,
    view: &str,
    depth: usize,
    visited: &mut usize,
    snapshot: &mut String,
) {
    const MAX_UIA_NODES: usize = 256;
    const MAX_UIA_DEPTH: usize = 12;
    if *visited >= MAX_UIA_NODES || depth > MAX_UIA_DEPTH {
        return;
    }
    let Ok(mut element) = (unsafe { walker.GetFirstChildElement(parent) }) else {
        return;
    };
    loop {
        *visited += 1;
        let indent = "  ".repeat(depth);
        let _ = writeln!(
            snapshot,
            "{indent}{view}: {}",
            describe_uia_element(&element)
        );
        append_uia_tree(walker, &element, view, depth + 1, visited, snapshot);
        if *visited >= MAX_UIA_NODES {
            break;
        }
        let Ok(next) = (unsafe { walker.GetNextSiblingElement(&element) }) else {
            break;
        };
        element = next;
    }
}

pub(super) fn click_semantic_control(
    child: &NativeChild,
    target: &WindowSnapshot,
    control: &SemanticControl,
    trace_path: &Path,
) -> Result<PointerClickEvidence, String> {
    child.validate_window(target.hwnd)?;
    if control.process_id != child.process_id || !control.enabled {
        return Err("refused semantic click for disabled or foreign-process control".into());
    }
    if control.bounds[2] <= control.bounds[0] || control.bounds[3] <= control.bounds[1] {
        return Err("semantic control has empty screen bounds".into());
    }
    let point = POINT {
        x: control.bounds[0] + (control.bounds[2] - control.bounds[0]) / 2,
        y: control.bounds[1] + (control.bounds[3] - control.bounds[1]) / 2,
    };
    let nudge = adjacent_pointer_point(
        point,
        [
            control.bounds[0],
            control.bounds[1],
            control.bounds[2],
            control.bounds[3],
        ],
    )?;
    click_screen_point(
        child,
        target,
        point,
        "semantic click",
        PointerMoveAcknowledgement {
            trace_path,
            kind: PointerTraceKind::RootScreen,
            nudge_screen_point: nudge,
            nudge_trace_point: (nudge.x, nudge.y),
            target_trace_point: (point.x, point.y),
        },
    )
}

pub(super) fn click_designer_client_bounds(
    child: &NativeChild,
    target: &WindowSnapshot,
    bounds: [i32; 4],
    trace_path: &Path,
) -> Result<PointerClickEvidence, String> {
    child.validate_window(target.hwnd)?;
    let mut client = RECT::default();
    unsafe { GetClientRect(target.hwnd, &mut client) }
        .map_err(|error| format!("read Designer client bounds: {error}"))?;
    let point = semantic_client_center(
        bounds,
        [client.left, client.top, client.right, client.bottom],
    )?;
    let client_point = (point.x, point.y);
    let nudge_client = adjacent_pointer_point(point, bounds)?;
    let mut nudge_screen = nudge_client;
    if !unsafe { ClientToScreen(target.hwnd, &mut nudge_screen) }.as_bool() {
        return Err("could not convert Designer pointer nudge to screen coordinates".into());
    }
    let mut screen_point = point;
    if !unsafe { ClientToScreen(target.hwnd, &mut screen_point) }.as_bool() {
        return Err("could not convert Designer semantic point to screen coordinates".into());
    }
    click_screen_point(
        child,
        target,
        screen_point,
        "Designer semantic click",
        PointerMoveAcknowledgement {
            trace_path,
            kind: PointerTraceKind::DesignerClient,
            nudge_screen_point: nudge_screen,
            nudge_trace_point: (nudge_client.x, nudge_client.y),
            target_trace_point: client_point,
        },
    )
}

#[derive(Clone, Copy)]
enum PointerTraceKind {
    RootScreen,
    DesignerClient,
}

struct PointerMoveAcknowledgement<'a> {
    trace_path: &'a Path,
    kind: PointerTraceKind,
    nudge_screen_point: POINT,
    nudge_trace_point: (i32, i32),
    target_trace_point: (i32, i32),
}

fn click_screen_point(
    child: &NativeChild,
    target: &WindowSnapshot,
    point: POINT,
    operation: &str,
    pointer_move_ack: PointerMoveAcknowledgement<'_>,
) -> Result<PointerClickEvidence, String> {
    child.validate_window(target.hwnd)?;
    let root_screen_click = matches!(pointer_move_ack.kind, PointerTraceKind::RootScreen);
    if root_screen_click {
        validate_root_pointer_geometry(target)?;
    }
    child.focus_window(target)?;
    if root_screen_click {
        validate_root_pointer_geometry(target)?;
    }
    let mut client = RECT::default();
    unsafe { GetClientRect(target.hwnd, &mut client) }
        .map_err(|error| format!("read target client bounds: {error}"))?;
    let mut top_left = POINT {
        x: client.left,
        y: client.top,
    };
    let mut bottom_right = POINT {
        x: client.right,
        y: client.bottom,
    };
    if !unsafe { ClientToScreen(target.hwnd, &mut top_left) }.as_bool()
        || !unsafe { ClientToScreen(target.hwnd, &mut bottom_right) }.as_bool()
        || point.x < top_left.x
        || point.y < top_left.y
        || point.x >= bottom_right.x
        || point.y >= bottom_right.y
    {
        return Err("semantic click point lies outside the target client area".into());
    }
    let cursor_before_move = cursor_position()?;
    let pointer_position_preexisting_ack = cursor_before_move.x == point.x
        && cursor_before_move.y == point.y
        && matches!(pointer_move_ack.kind, PointerTraceKind::RootScreen)
        && latest_root_pointer_acknowledged(
            pointer_move_ack.trace_path,
            pointer_move_ack.target_trace_point,
        )?;
    let nudge_screen_point = pointer_move_ack.nudge_screen_point;
    if nudge_screen_point.x < top_left.x
        || nudge_screen_point.y < top_left.y
        || nudge_screen_point.x >= bottom_right.x
        || nudge_screen_point.y >= bottom_right.y
    {
        return Err(format!(
            "{operation} pointer nudge {:?} lies outside the target client area",
            (nudge_screen_point.x, nudge_screen_point.y)
        ));
    }
    let trace_cursor = trace_line_count(pointer_move_ack.trace_path)?;
    if root_screen_click {
        validate_root_pointer_geometry(target)?;
    }
    if !unsafe { SetCursorPos(nudge_screen_point.x, nudge_screen_point.y) }.is_ok() {
        return Err("could not move cursor to the semantic control nudge point".into());
    }
    let nudge_under_cursor = validate_pointer_coverage(
        target.hwnd,
        child.process_id(),
        nudge_screen_point,
        operation,
    )?;
    let nudge_movement = [mouse_move_input(nudge_screen_point)?];
    if root_screen_click {
        validate_root_pointer_geometry(target)?;
    }
    let nudge_movement = send_validated_input(
        target.hwnd,
        child.process_id(),
        &nudge_movement,
        &format!("{operation} pointer nudge"),
    )?;
    let nudge_correction_events = wait_for_pointer_move_ack(
        child,
        target,
        operation,
        &pointer_move_ack,
        trace_cursor,
        pointer_move_ack.nudge_trace_point,
        Duration::from_secs(3),
    )?;

    let target_move_cursor = trace_line_count(pointer_move_ack.trace_path)?;
    if root_screen_click {
        validate_root_pointer_geometry(target)?;
    }
    if !unsafe { SetCursorPos(point.x, point.y) }.is_ok() {
        return Err("could not move cursor to the validated semantic point".into());
    }
    let under_cursor =
        validate_pointer_coverage(target.hwnd, child.process_id(), point, operation)?;
    let movement = [mouse_move_input(point)?];
    if root_screen_click {
        validate_root_pointer_geometry(target)?;
    }
    let movement = send_validated_input(
        target.hwnd,
        child.process_id(),
        &movement,
        &format!("{operation} pointer move"),
    )?;
    let target_correction_events = wait_for_pointer_move_ack(
        child,
        target,
        operation,
        &pointer_move_ack,
        target_move_cursor,
        pointer_move_ack.target_trace_point,
        Duration::from_secs(3),
    )?;
    let foreground_hwnd = unsafe { GetForegroundWindow() };
    if foreground_hwnd != target.hwnd {
        return Err(format!(
            "blocked precondition: target HWND={} is not foreground before {operation} (foreground HWND={})",
            hwnd_id(target.hwnd),
            hwnd_id(foreground_hwnd)
        ));
    }
    if root_screen_click {
        validate_root_pointer_geometry(target)?;
    }
    let down = [mouse_input(true)];
    let down_trace_cursor = trace_line_count(pointer_move_ack.trace_path)?;
    let down = send_validated_input(target.hwnd, child.process_id(), &down, operation)?;
    let left_button_state_after_down = unsafe { GetAsyncKeyState(VK_LBUTTON.0 as i32) };
    let mut button_guard = MouseButtonGuard::new(target.hwnd, child.process_id());
    button_guard.armed = true;
    wait_for_pointer_button_ack(
        &pointer_move_ack,
        down_trace_cursor,
        (point.x, point.y),
        true,
        Duration::from_secs(3),
    )
    .map_err(|error| {
        format!(
            "{error}; checked down=[{}], VK_LBUTTON async=0x{:04x}",
            down.describe(),
            left_button_state_after_down as u16
        )
    })?;
    let left_button_state_before_up = unsafe { GetAsyncKeyState(VK_LBUTTON.0 as i32) };
    let up_trace_cursor = trace_line_count(pointer_move_ack.trace_path)?;
    let up = button_guard.release()?;
    let left_button_state_after_up = unsafe { GetAsyncKeyState(VK_LBUTTON.0 as i32) };
    wait_for_pointer_button_ack(
        &pointer_move_ack,
        up_trace_cursor,
        (point.x, point.y),
        false,
        Duration::from_secs(3),
    )
    .map_err(|error| {
        format!(
            "{error}; checked up=[{}], VK_LBUTTON async after up=0x{:04x}",
            up.describe(),
            left_button_state_after_up as u16
        )
    })?;
    Ok(PointerClickEvidence {
        nudge_movement,
        movement,
        pointer_correction_events: nudge_correction_events + target_correction_events,
        pointer_position_preexisting_ack,
        pointer_move_acknowledged: true,
        down,
        down_acknowledged: true,
        left_button_state_after_down,
        left_button_state_before_up,
        up,
        up_acknowledged: true,
        left_button_state_after_up,
        target_hwnd: target.hwnd,
        nudge_under_cursor_hwnd: nudge_under_cursor,
        under_cursor_hwnd: under_cursor,
        foreground_hwnd,
        screen_point: (point.x, point.y),
    })
}

fn validate_root_pointer_geometry(target: &WindowSnapshot) -> Result<(), String> {
    let mut rect = RECT::default();
    unsafe { GetWindowRect(target.hwnd, &mut rect) }
        .map_err(|error| format!("refresh ROOT HWND bounds before pointer input: {error}"))?;
    let actual = [rect.left, rect.top, rect.right, rect.bottom];
    let displays = suite::native_display_bounds()
        .map_err(|error| format!("validate ROOT pointer monitor bounds: {error}"))?;
    if !suite::intersects_display_bounds(actual, &displays) {
        return Err(format!(
            "blocked precondition: ROOT is parked off physical displays; live HWND bounds={actual:?}, displays={displays:?}; no pointer input sent"
        ));
    }
    if actual != target.bounds {
        return Err(format!(
            "blocked precondition: ROOT geometry changed after semantic bounds were resolved; expected={:?}, live={actual:?}; reacquire UIA control before retry",
            target.bounds
        ));
    }
    Ok(())
}

fn wait_for_pointer_move_ack(
    child: &NativeChild,
    target_window: &WindowSnapshot,
    operation: &str,
    acknowledgement: &PointerMoveAcknowledgement<'_>,
    cursor: usize,
    expected_point: (i32, i32),
    timeout: Duration,
) -> Result<usize, String> {
    let deadline = Instant::now() + timeout;
    let mut event_cursor = cursor;
    let mut correction_events = 0;
    let mut last_observed = None;
    loop {
        if let Some(observed) = latest_pointer_move_after(
            acknowledgement.trace_path,
            event_cursor,
            acknowledgement.kind,
        )? {
            last_observed = Some(observed);
            let Some((dx, dy)) = pointer_correction_delta(observed, expected_point)? else {
                return Ok(correction_events);
            };
            if correction_events >= MAX_POINTER_CORRECTIONS {
                return Err(format!(
                    "{operation} pointer move remained inexact after {correction_events} bounded corrections: expected={expected_point:?} observed={observed:?}"
                ));
            }
            event_cursor = trace_line_count(acknowledgement.trace_path)?;
            let correction = relative_mouse_move_input(dx, dy);
            let evidence = send_validated_input(
                target_window.hwnd,
                child.process_id(),
                &[correction],
                &format!("{operation} exact pointer correction"),
            )?;
            correction_events = correction_events.saturating_add(evidence.inserted);
        }
        if Instant::now() >= deadline {
            let surface = match acknowledgement.kind {
                PointerTraceKind::RootScreen => "ROOT screen point",
                PointerTraceKind::DesignerClient => "Designer client point",
            };
            return Err(format!(
                "production {surface} pointer move did not reach exact point {expected_point:?} before click; last observed={last_observed:?}, bounded correction events={correction_events}"
            ));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn latest_pointer_move_after(
    trace_path: &Path,
    cursor: usize,
    kind: PointerTraceKind,
) -> Result<Option<(i32, i32)>, String> {
    let trace = std::fs::read_to_string(trace_path)
        .map_err(|error| format!("read production pointer-move trace: {error}"))?;
    let events = trace.lines().skip(cursor).collect::<Vec<_>>();
    Ok(events.iter().rev().find_map(|line| match kind {
        PointerTraceKind::RootScreen if line.contains("trace_event=\"root_pointer_moved\"") => {
            Some((
                trace_i32_field(line, "screen_x=")?,
                trace_i32_field(line, "screen_y=")?,
            ))
        }
        PointerTraceKind::DesignerClient
            if line.contains("trace_event=\"designer_pointer_moved\"") =>
        {
            Some((
                trace_i32_field(line, "client_x=")?,
                trace_i32_field(line, "client_y=")?,
            ))
        }
        _ => None,
    }))
}

fn pointer_correction_delta(
    observed: (i32, i32),
    target: (i32, i32),
) -> Result<Option<(i32, i32)>, String> {
    let dx = target.0.saturating_sub(observed.0);
    let dy = target.1.saturating_sub(observed.1);
    if dx == 0 && dy == 0 {
        return Ok(None);
    }
    if dx.unsigned_abs() > MAX_POINTER_CORRECTION_PIXELS
        || dy.unsigned_abs() > MAX_POINTER_CORRECTION_PIXELS
    {
        return Err(format!(
            "refused pointer correction outside the bounded rounding envelope: expected={target:?} observed={observed:?}"
        ));
    }
    Ok(Some((dx, dy)))
}

fn wait_for_pointer_button_ack(
    acknowledgement: &PointerMoveAcknowledgement<'_>,
    cursor: usize,
    screen_point: (i32, i32),
    pressed: bool,
    timeout: Duration,
) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    loop {
        let trace = std::fs::read_to_string(acknowledgement.trace_path)
            .map_err(|error| format!("read production pointer-button trace: {error}"))?;
        let acknowledged = trace
            .lines()
            .skip(cursor)
            .any(|line| match acknowledgement.kind {
                PointerTraceKind::RootScreen => {
                    line.contains("trace_event=\"root_pointer_button\"")
                        && if pressed {
                            line.contains("pressed=true")
                        } else {
                            line.contains("released=true")
                        }
                        && trace_i32_field(line, "screen_x=")
                            .is_some_and(|x| x.abs_diff(screen_point.0) <= 1)
                        && trace_i32_field(line, "screen_y=")
                            .is_some_and(|y| y.abs_diff(screen_point.1) <= 1)
                }
                PointerTraceKind::DesignerClient => {
                    line.contains("trace_event=\"designer_pointer\"")
                        && if pressed {
                            line.contains("pointer_down=true")
                        } else {
                            line.contains("pointer_up=true")
                        }
                        && trace_i32_field(line, "cursor_screen_x=")
                            .is_some_and(|x| x.abs_diff(screen_point.0) <= 1)
                        && trace_i32_field(line, "cursor_screen_y=")
                            .is_some_and(|y| y.abs_diff(screen_point.1) <= 1)
                }
            });
        if acknowledged {
            return Ok(());
        }
        if Instant::now() >= deadline {
            let edge = if pressed { "down" } else { "up" };
            let surface = match acknowledgement.kind {
                PointerTraceKind::RootScreen => "production ROOT",
                PointerTraceKind::DesignerClient => "production Designer",
            };
            return Err(format!(
                "{surface} did not acknowledge pointer-button {edge} at screen point {screen_point:?}"
            ));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn trace_line_count(trace_path: &Path) -> Result<usize, String> {
    std::fs::read_to_string(trace_path)
        .map(|trace| trace.lines().count())
        .map_err(|error| format!("read acceptance trace before native pointer edge: {error}"))
}

fn validate_pointer_coverage(
    target_hwnd: HWND,
    target_process_id: u32,
    point: POINT,
    operation: &str,
) -> Result<HWND, String> {
    let under_cursor = unsafe { WindowFromPoint(point) };
    if under_cursor.is_invalid() || window_process_id(under_cursor) != target_process_id {
        return Err(format!(
            "{operation} point ({},{}) targeting HWND={} is covered by HWND={} PID={} outside the child process",
            point.x,
            point.y,
            hwnd_id(target_hwnd),
            hwnd_id(under_cursor),
            window_process_id(under_cursor)
        ));
    }
    if under_cursor != target_hwnd && !unsafe { IsChild(target_hwnd, under_cursor) }.as_bool() {
        return Err(format!(
            "blocked precondition: {operation} target HWND={} is covered at screen point ({},{}) by HWND={} PID={} (same-process coverage is not target delivery)",
            hwnd_id(target_hwnd),
            point.x,
            point.y,
            hwnd_id(under_cursor),
            window_process_id(under_cursor)
        ));
    }
    Ok(under_cursor)
}

fn adjacent_pointer_point(point: POINT, bounds: [i32; 4]) -> Result<POINT, String> {
    let [left, top, right, bottom] = bounds;
    if right <= left
        || bottom <= top
        || point.x < left
        || point.y < top
        || point.x >= right
        || point.y >= bottom
    {
        return Err("cannot choose a pointer nudge outside empty semantic bounds".into());
    }
    if right - left > 1 {
        let x = if point.x + 1 < right {
            point.x + 1
        } else {
            point.x - 1
        };
        return Ok(POINT { x, y: point.y });
    }
    if bottom - top > 1 {
        let y = if point.y + 1 < bottom {
            point.y + 1
        } else {
            point.y - 1
        };
        return Ok(POINT { x: point.x, y });
    }
    Err("semantic bounds have no adjacent in-control point for a fresh pointer move".into())
}

fn latest_root_pointer_acknowledged(trace_path: &Path, target: (i32, i32)) -> Result<bool, String> {
    let trace = std::fs::read_to_string(trace_path)
        .map_err(|error| format!("read ROOT pointer-move trace: {error}"))?;
    let latest = trace
        .lines()
        .rev()
        .find(|line| line.contains("trace_event=\"root_pointer_moved\""));
    Ok(latest.is_some_and(|line| {
        trace_i32_field(line, "screen_x=").is_some_and(|x| x.abs_diff(target.0) <= 1)
            && trace_i32_field(line, "screen_y=").is_some_and(|y| y.abs_diff(target.1) <= 1)
    }))
}

fn trace_i32_field(line: &str, marker: &str) -> Option<i32> {
    let start = line.find(marker)?.saturating_add(marker.len());
    let remainder = &line[start..];
    let end = remainder
        .find(char::is_whitespace)
        .unwrap_or(remainder.len());
    remainder[..end].parse().ok()
}

fn trace_i64_field(line: &str, marker: &str) -> Option<i64> {
    let start = line.find(marker)?.saturating_add(marker.len());
    let remainder = &line[start..];
    let end = remainder
        .find(char::is_whitespace)
        .unwrap_or(remainder.len());
    remainder[..end].parse().ok()
}

fn trace_field<'a>(line: &'a str, name: &str) -> Option<&'a str> {
    line.split_ascii_whitespace()
        .find_map(|part| part.strip_prefix(&format!("{name}=")))
}

fn trace_bool_field(line: &str, name: &str) -> Option<bool> {
    match trace_field(line, name)? {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn parse_authoring_control(line: &str) -> Option<AuthoringControlSnapshot> {
    if !line.contains("trace_event=\"designer_authoring_control\"")
        || trace_field(line, "viewport")? != "Deferred"
    {
        return None;
    }
    let target = match trace_field(line, "target")? {
        "NewMenu" => AuthoringControlTarget::NewMenu,
        "MenuAfterAction" => AuthoringControlTarget::MenuAfterAction,
        "AfterActionOption" => AuthoringControlTarget::AfterActionOption,
        "AddRing" => AuthoringControlTarget::AddRing,
        "MenuRow" => AuthoringControlTarget::MenuRow,
        "RingSelector" => AuthoringControlTarget::RingSelector,
        "RingOption" => AuthoringControlTarget::RingOption,
        "Slots" => AuthoringControlTarget::Slots,
        "PreviewProposal" => AuthoringControlTarget::PreviewProposal,
        "ApplyProposal" => AuthoringControlTarget::ApplyProposal,
        "CancelProposal" => AuthoringControlTarget::CancelProposal,
        "MoveToOverflow" => AuthoringControlTarget::MoveToOverflow,
        "CancelResolution" => AuthoringControlTarget::CancelResolution,
        "DiscardCells" => AuthoringControlTarget::DiscardCells,
        "Canvas" => AuthoringControlTarget::Canvas,
        "CanvasCell" => AuthoringControlTarget::CanvasCell,
        "DiscardDraft" => AuthoringControlTarget::DiscardDraft,
        "CellType" => AuthoringControlTarget::CellType,
        "ActionTypeOption" => AuthoringControlTarget::ActionTypeOption,
        "ActionSearch" => AuthoringControlTarget::ActionSearch,
        "ActionRow" => AuthoringControlTarget::ActionRow,
        "PopupApply" => AuthoringControlTarget::PopupApply,
        "PopupCancel" => AuthoringControlTarget::PopupCancel,
        "PopupOpenInspector" => AuthoringControlTarget::PopupOpenInspector,
        "PopupApplyAndOpen" => AuthoringControlTarget::PopupApplyAndOpen,
        "PopupDiscardAndOpen" => AuthoringControlTarget::PopupDiscardAndOpen,
        "PopupKeepEditing" => AuthoringControlTarget::PopupKeepEditing,
        "InspectorCell" => AuthoringControlTarget::InspectorCell,
        "SkinRow" => AuthoringControlTarget::SkinRow,
        "SkinGlowEnabled" => AuthoringControlTarget::SkinGlowEnabled,
        "OpenDesktopPreview" => AuthoringControlTarget::OpenDesktopPreview,
        "StopDesktopPreview" => AuthoringControlTarget::StopDesktopPreview,
        "Undo" => AuthoringControlTarget::Undo,
        "Redo" => AuthoringControlTarget::Redo,
        "Save" => AuthoringControlTarget::Save,
        "KeepEditing" => AuthoringControlTarget::KeepEditing,
        _ => return None,
    };
    let role = match trace_field(line, "role")?.trim_matches('"') {
        "Button" => AuthoringControlRole::Button,
        "Selectable" => AuthoringControlRole::Selectable,
        "ComboBox" => AuthoringControlRole::ComboBox,
        "DragValue" => AuthoringControlRole::DragValue,
        "Region" => AuthoringControlRole::Region,
        "TextEdit" => AuthoringControlRole::TextEdit,
        "Checkbox" => AuthoringControlRole::Checkbox,
        _ => return None,
    };
    let control_index = trace_i32_field(line, "control_index=")?;
    Some(AuthoringControlSnapshot {
        target,
        role,
        index: usize::try_from(control_index).ok(),
        bounds: [
            trace_i32_field(line, "left_px=")?,
            trace_i32_field(line, "top_px=")?,
            trace_i32_field(line, "right_px=")?,
            trace_i32_field(line, "bottom_px=")?,
        ],
        client_size: [
            trace_i32_field(line, "client_width_px=")?,
            trace_i32_field(line, "client_height_px=")?,
        ],
        enabled: trace_bool_field(line, "enabled")?,
        selected: trace_bool_field(line, "selected")?,
        focused: trace_bool_field(line, "focused")?,
        clicked: trace_bool_field(line, "clicked")?,
        session_id: trace_field(line, "session_id")?.parse().ok()?,
        generation: trace_field(line, "generation")?.parse().ok()?,
        menu_cell_ids_digest: trace_field(line, "menu_cell_ids_digest")?
            .parse::<u64>()
            .ok()
            .filter(|digest| *digest != 0),
        ring_index: trace_i32_field(line, "cell_ring_index=")
            .and_then(|index| usize::try_from(index).ok()),
        slot_index: trace_i32_field(line, "cell_slot_index=")
            .and_then(|index| usize::try_from(index).ok()),
    })
}

fn parse_designer_canvas_allocation(line: &str) -> Option<DesignerCanvasAllocationSnapshot> {
    if !line.contains("trace_event=\"designer_canvas_allocation\"") {
        return None;
    }
    Some(DesignerCanvasAllocationSnapshot {
        allocated_rect: [
            trace_i32_field(line, "allocated_left_px=")?,
            trace_i32_field(line, "allocated_top_px=")?,
            trace_i32_field(line, "allocated_right_px=")?,
            trace_i32_field(line, "allocated_bottom_px=")?,
        ],
        clip_rect: [
            trace_i32_field(line, "clip_left_px=")?,
            trace_i32_field(line, "clip_top_px=")?,
            trace_i32_field(line, "clip_right_px=")?,
            trace_i32_field(line, "clip_bottom_px=")?,
        ],
        requested_size: [
            trace_i32_field(line, "requested_width_px=")?,
            trace_i32_field(line, "requested_height_px=")?,
        ],
        session_id: trace_field(line, "session_id")?.parse().ok()?,
        generation: trace_field(line, "generation")?.parse().ok()?,
    })
}

pub(super) fn designer_canvas_allocations_after(
    trace_path: &Path,
    first_line: usize,
    session_id: u64,
) -> Result<Vec<DesignerCanvasAllocationSnapshot>, String> {
    let trace = std::fs::read_to_string(trace_path)
        .map_err(|error| format!("read Designer canvas allocation trace: {error}"))?;
    Ok(trace
        .lines()
        .skip(first_line)
        .filter_map(parse_designer_canvas_allocation)
        .filter(|snapshot| snapshot.session_id == session_id)
        .collect())
}

fn parse_action_catalog_rank(line: &str) -> Option<ActionCatalogRankSnapshot> {
    if !line.contains("trace_event=\"designer_action_catalog_rank\"") {
        return None;
    }
    Some(ActionCatalogRankSnapshot {
        custom_action_index: trace_field(line, "custom_action_index")?.parse().ok()?,
        rank: trace_field(line, "rank")?.parse().ok()?,
        catalog_len: trace_field(line, "catalog_len")?.parse().ok()?,
        session_id: trace_field(line, "session_id")?.parse().ok()?,
        generation: trace_field(line, "generation")?.parse().ok()?,
    })
}

pub(super) fn action_catalog_ranks(
    trace_path: &Path,
) -> Result<Vec<ActionCatalogRankSnapshot>, String> {
    let trace = std::fs::read_to_string(trace_path)
        .map_err(|error| format!("read action catalog rank trace: {error}"))?;
    let mut ranks = Vec::new();
    for rank in trace.lines().filter_map(parse_action_catalog_rank) {
        if !ranks.contains(&rank) {
            ranks.push(rank);
        }
    }
    Ok(ranks)
}

fn parse_geometry_state(line: &str) -> Option<GeometryStateSnapshot> {
    if !line.contains("trace_event=\"designer_geometry_state\"") {
        return None;
    }
    let optional_index = |name: &str| {
        let value = trace_i32_field(line, &format!("{name}="))?;
        usize::try_from(value).ok()
    };
    Some(GeometryStateSnapshot {
        session_id: trace_field(line, "session_id")?.parse().ok()?,
        menu_count: trace_field(line, "menu_count")?.parse().ok()?,
        selected_menu_index: optional_index("selected_menu_index"),
        selected_menu_after_action: match trace_field(line, "selected_menu_after_action")? {
            "None" => None,
            "Some(Inherit)" => Some(multi_launcher::radial::model::AfterActionPolicy::Inherit),
            "Some(KeepOpen)" => Some(multi_launcher::radial::model::AfterActionPolicy::KeepOpen),
            "Some(CloseCurrentMenu)" => {
                Some(multi_launcher::radial::model::AfterActionPolicy::CloseCurrentMenu)
            }
            "Some(CloseTree)" => Some(multi_launcher::radial::model::AfterActionPolicy::CloseTree),
            _ => return None,
        },
        ring_count: trace_field(line, "ring_count")?.parse().ok()?,
        selected_ring_index: optional_index("selected_ring_index"),
        selected_cell_index: optional_index("selected_cell_index"),
        selected_cell_id_digest: trace_i64_field(line, "selected_cell_id_digest=")
            .and_then(|value| u64::try_from(value).ok()),
        selected_cell_custom_action_index: optional_index("selected_cell_custom_action_index"),
        selected_cell_custom_action_index_known: trace_bool_field(
            line,
            "selected_cell_custom_action_index_known",
        )?,
        selected_ring_slots: trace_field(line, "selected_ring_slots")?.parse().ok()?,
        requested_slots: trace_field(line, "requested_slots")?.parse().ok()?,
        selected_ring_populated: trace_field(line, "selected_ring_populated")?.parse().ok()?,
        menu_populated: trace_field(line, "menu_populated")?.parse().ok()?,
        draft_cell_ids_digest: trace_field(line, "draft_cell_ids_digest")?.parse().ok()?,
        proposal_cell_ids_digest: trace_field(line, "proposal_cell_ids_digest")?
            .parse()
            .ok()?,
        proposal_cell_ids_digest_available: trace_bool_field(
            line,
            "proposal_cell_ids_digest_available",
        )?,
        proposal_kind: match trace_field(line, "proposal_kind")? {
            "None" => AuthoringProposalKind::None,
            "NewRing" => AuthoringProposalKind::NewRing,
            "Resize" => AuthoringProposalKind::Resize,
            "ResolvedResize" => AuthoringProposalKind::ResolvedResize,
            _ => return None,
        },
        proposal_active: trace_bool_field(line, "proposal_active")?,
        proposal_ready: trace_bool_field(line, "proposal_ready")?,
        proposal_slots: trace_field(line, "proposal_slots")?.parse().ok()?,
        proposal_candidate_rings: trace_field(line, "proposal_candidate_rings")?
            .parse()
            .ok()?,
        proposal_resolution_populated: trace_field(line, "proposal_resolution_populated")?
            .parse()
            .ok()?,
        proposal_cell_ids_preserved: trace_bool_field(line, "proposal_cell_ids_preserved")?,
        resize_prompt_open: trace_bool_field(line, "resize_prompt_open")?,
        resize_prompt_populated: trace_field(line, "resize_prompt_populated")?.parse().ok()?,
        generation: trace_field(line, "generation")?.parse().ok()?,
    })
}

pub(super) fn geometry_states(trace_path: &Path) -> Result<Vec<GeometryStateSnapshot>, String> {
    let trace = std::fs::read_to_string(trace_path)
        .map_err(|error| format!("read Designer geometry trace: {error}"))?;
    Ok(trace.lines().filter_map(parse_geometry_state).collect())
}

pub(super) fn latest_geometry_state(
    trace_path: &Path,
) -> Result<Option<GeometryStateSnapshot>, String> {
    Ok(geometry_states(trace_path)?.into_iter().last())
}

fn latest_authoring_controls_after(
    lines: &[String],
    first_line: usize,
    session_id: u64,
) -> Vec<AuthoringControlSnapshot> {
    let mut controls = Vec::<AuthoringControlSnapshot>::new();
    for control in lines
        .iter()
        .skip(first_line)
        .filter_map(|line| parse_authoring_control(line))
    {
        if control.session_id != session_id {
            continue;
        }
        if let Some(existing) = controls.iter_mut().find(|item| {
            item.target == control.target
                && item.role == control.role
                && item.index == control.index
        }) {
            *existing = control;
        } else {
            controls.push(control);
        }
    }
    controls
}

fn trace_event_lines(trace: &str) -> Vec<String> {
    trace
        .lines()
        .filter(|line| line.contains("trace_event=\""))
        .map(str::to_owned)
        .collect()
}

fn unique_authoring_control(
    controls: &[AuthoringControlSnapshot],
    target: AuthoringControlTarget,
    index: Option<usize>,
    role: AuthoringControlRole,
) -> Result<Option<AuthoringControlSnapshot>, String> {
    let mut matches = controls.iter().copied().filter(|control| {
        control.target == target && control.index == index && control.role == role
    });
    let Some(control) = matches.next() else {
        return Ok(None);
    };
    if matches.next().is_some() {
        return Err(format!(
            "ambiguous Designer semantic target {target:?} index={index:?} role={role:?}"
        ));
    }
    Ok(Some(control))
}

pub(super) fn find_authoring_control(
    trace_path: &Path,
    session_id: u64,
    target: AuthoringControlTarget,
    index: Option<usize>,
    role: AuthoringControlRole,
) -> Result<Option<AuthoringControlSnapshot>, String> {
    find_authoring_control_after(trace_path, 0, session_id, target, index, role)
}

pub(super) fn list_authoring_controls(
    trace_path: &Path,
    session_id: u64,
) -> Result<Vec<AuthoringControlSnapshot>, String> {
    let trace = std::fs::read_to_string(trace_path)
        .map_err(|error| format!("read Designer control trace: {error}"))?;
    Ok(latest_authoring_controls_after(
        &trace_event_lines(&trace),
        0,
        session_id,
    ))
}

pub(super) fn fresh_canvas_cell_for_generation(
    controls: &[AuthoringControlSnapshot],
    session_id: u64,
    generation: u64,
    flat_index: usize,
    menu_cell_ids_digest: u64,
    ring_index: usize,
    slot_index: usize,
) -> Option<AuthoringControlSnapshot> {
    controls
        .iter()
        .copied()
        .filter(|control| {
            control.target == AuthoringControlTarget::CanvasCell
                && control.role == AuthoringControlRole::Region
                && control.enabled
                && control.session_id == session_id
                && control.generation == generation
                && control.index == Some(flat_index)
                && control.menu_cell_ids_digest == Some(menu_cell_ids_digest)
                && control.ring_index == Some(ring_index)
                && control.slot_index == Some(slot_index)
        })
        .next()
}

pub(super) fn find_authoring_control_after(
    trace_path: &Path,
    first_line: usize,
    session_id: u64,
    target: AuthoringControlTarget,
    index: Option<usize>,
    role: AuthoringControlRole,
) -> Result<Option<AuthoringControlSnapshot>, String> {
    let trace = std::fs::read_to_string(trace_path)
        .map_err(|error| format!("read Designer control trace: {error}"))?;
    // Suite cursors count trace records, not every logger line in the app log. Filter
    // first so the same cursor always denotes the same render epoch in both modules.
    let controls =
        latest_authoring_controls_after(&trace_event_lines(&trace), first_line, session_id);
    unique_authoring_control(&controls, target, index, role)
}

pub(super) fn authoring_control_click_finished(
    lines: &[String],
    session_id: u64,
    target: AuthoringControlTarget,
    index: Option<usize>,
    role: AuthoringControlRole,
) -> bool {
    let mut click_seen = false;
    for control in lines
        .iter()
        .filter_map(|line| parse_authoring_control(line))
        .filter(|control| {
            control.session_id == session_id
                && control.target == target
                && control.index == index
                && control.role == role
        })
    {
        if control.clicked {
            click_seen = true;
        } else if click_seen {
            return true;
        }
    }
    false
}

pub(super) fn semantic_client_center(
    bounds: [i32; 4],
    client_bounds: [i32; 4],
) -> Result<POINT, String> {
    let [left, top, right, bottom] = bounds;
    let [client_left, client_top, client_right, client_bottom] = client_bounds;
    if right <= left || bottom <= top || client_right <= client_left || client_bottom <= client_top
    {
        return Err("Designer semantic target has empty client bounds".into());
    }
    if left < client_left || top < client_top || right > client_right || bottom > client_bottom {
        return Err(format!(
            "Designer semantic target bounds {bounds:?} extend outside client bounds {client_bounds:?}"
        ));
    }
    let center_x = i64::from(left) + (i64::from(right) - i64::from(left)) / 2;
    let center_y = i64::from(top) + (i64::from(bottom) - i64::from(top)) / 2;
    let x = i32::try_from(center_x)
        .map_err(|_| "Designer semantic center cannot be represented as a client point")?;
    let y = i32::try_from(center_y)
        .map_err(|_| "Designer semantic center cannot be represented as a client point")?;
    let point = POINT { x, y };
    if point.x < left
        || point.y < top
        || point.x >= right
        || point.y >= bottom
        || point.x < client_left
        || point.y < client_top
        || point.x >= client_right
        || point.y >= client_bottom
    {
        return Err("Designer semantic center lies outside its target client area".into());
    }
    Ok(point)
}

pub(super) fn replace_text(
    child: &NativeChild,
    target: &WindowSnapshot,
    control: &SemanticControl,
    uia: &UiAutomation,
    text: &str,
) -> Result<(usize, usize), String> {
    child.validate_window(target.hwnd)?;
    if control.process_id != child.process_id || !control.enabled {
        return Err("refused text replacement for disabled or foreign-process UIA control".into());
    }
    child.focus_window(target)?;
    uia.focus(control)?;
    focus_is_validated(target.hwnd, child.process_id)?;

    let select_all = VIRTUAL_KEY(b'A' as u16);
    let select_events = [
        key_input(VK_CONTROL, false),
        key_input(select_all, false),
        key_input(select_all, true),
        key_input(VK_CONTROL, true),
    ];
    let selected = send_validated_input(
        target.hwnd,
        child.process_id,
        &select_events,
        "focused Ctrl+A before query replacement",
    )?
    .inserted;

    let text_events = unicode_text_events(text);
    if text_events.is_empty() {
        return Ok((selected, 0));
    }
    let typed = send_validated_input(
        target.hwnd,
        child.process_id,
        &text_events,
        "replacement Unicode text",
    )?
    .inserted;
    Ok((selected, typed))
}

pub(super) fn send_text_to_focused_window(
    child: &NativeChild,
    target: &WindowSnapshot,
    text: &str,
) -> Result<usize, String> {
    child.validate_window(target.hwnd)?;
    let events = unicode_text_events(text);
    if events.is_empty() {
        return Ok(0);
    }
    send_validated_input(
        target.hwnd,
        child.process_id,
        &events,
        "focused Unicode text",
    )
    .map(|evidence| evidence.inserted)
}

pub(super) fn send_select_all_to_focused_window(
    child: &NativeChild,
    target: &WindowSnapshot,
) -> Result<usize, String> {
    child.validate_window(target.hwnd)?;
    let select_all = VIRTUAL_KEY(b'A' as u16);
    let events = [
        key_input(VK_CONTROL, false),
        key_input(select_all, false),
        key_input(select_all, true),
        key_input(VK_CONTROL, true),
    ];
    send_validated_input(target.hwnd, child.process_id, &events, "focused Ctrl+A")
        .map(|evidence| evidence.inserted)
}

fn unicode_text_events(text: &str) -> Vec<INPUT> {
    let mut events = Vec::with_capacity(text.encode_utf16().count().saturating_mul(2));
    for code_unit in text.encode_utf16() {
        events.push(unicode_input(code_unit, false));
        events.push(unicode_input(code_unit, true));
    }
    events
}

pub(super) fn send_enter_current(
    child: &NativeChild,
    target: &WindowSnapshot,
) -> Result<usize, String> {
    child.validate_window(target.hwnd)?;
    focus_is_validated(target.hwnd, child.process_id)?;
    input_modifiers_clear()?;
    let enter = windows::Win32::UI::Input::KeyboardAndMouse::VK_RETURN;
    let events = [key_input(enter, false), key_input(enter, true)];
    send_input_checked(&events, "Enter")
}

pub(super) fn send_tab(child: &NativeChild, target: &WindowSnapshot) -> Result<usize, String> {
    child.validate_window(target.hwnd)?;
    child.focus_window(target)?;
    input_modifiers_clear()?;
    let events = [key_input(VK_TAB, false), key_input(VK_TAB, true)];
    send_input_checked(&events, "Tab")
}

pub(super) fn send_alt_f4(child: &NativeChild, target: &WindowSnapshot) -> Result<usize, String> {
    child.validate_window(target.hwnd)?;
    child.focus_window(target)?;
    input_modifiers_clear()?;
    let events = [
        key_input(VK_MENU, false),
        key_input(VK_F4, false),
        key_input(VK_F4, true),
        key_input(VK_MENU, true),
    ];
    send_input_checked(&events, "Alt+F4")
}

fn mouse_input(down: bool) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: 0,
                dy: 0,
                mouseData: 0,
                dwFlags: if down {
                    MOUSEEVENTF_LEFTDOWN
                } else {
                    MOUSEEVENTF_LEFTUP
                },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn normalized_absolute_coordinate(position: i32, origin: i32, extent: i32) -> i32 {
    let span = i64::from(extent.saturating_sub(1).max(1));
    let offset = (i64::from(position) - i64::from(origin)).clamp(0, span);
    ((offset * i64::from(u16::MAX) + span / 2) / span) as i32
}

fn mouse_move_input(point: POINT) -> Result<INPUT, String> {
    let (left, top, width, height) = virtual_screen_bounds();
    if width <= 0 || height <= 0 {
        return Err("virtual desktop has invalid bounds for a native pointer move".into());
    }
    Ok(INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: normalized_absolute_coordinate(point.x, left, width),
                dy: normalized_absolute_coordinate(point.y, top, height),
                mouseData: 0,
                dwFlags: MOUSEEVENTF_MOVE
                    | MOUSEEVENTF_ABSOLUTE
                    | MOUSEEVENTF_VIRTUALDESK
                    | MOUSEEVENTF_MOVE_NOCOALESCE,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    })
}

fn relative_mouse_move_input(dx: i32, dy: i32) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                mouseData: 0,
                dwFlags: MOUSEEVENTF_MOVE | MOUSEEVENTF_MOVE_NOCOALESCE,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

struct MouseButtonGuard {
    target_hwnd: HWND,
    target_process_id: u32,
    armed: bool,
}

impl MouseButtonGuard {
    fn new(target_hwnd: HWND, target_process_id: u32) -> Self {
        Self {
            target_hwnd,
            target_process_id,
            armed: false,
        }
    }

    fn release(&mut self) -> Result<NativeInputEdgeEvidence, String> {
        if !self.armed {
            return Err("semantic click release was requested before a down event".into());
        }
        if focus_is_validated(self.target_hwnd, self.target_process_id).is_err() {
            focus_owned_window(self.target_hwnd, self.target_process_id)?;
        }
        let up = [mouse_input(false)];
        let evidence = send_validated_input(
            self.target_hwnd,
            self.target_process_id,
            &up,
            "semantic click up",
        )?;
        self.armed = false;
        Ok(evidence)
    }
}

impl Drop for MouseButtonGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = self.release();
        }
    }
}

fn bounded_label(text: &str) -> &str {
    const LIMIT: usize = 80;
    if text.len() <= LIMIT {
        return text;
    }
    let mut end = LIMIT;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

#[cfg(test)]
mod tests {
    use super::{
        ACCEPTANCE_RUNNER_INPUT_COOKIE, ActionCatalogRankSnapshot, AuthoringControlRole,
        AuthoringControlSnapshot, AuthoringControlTarget, FocusAnchorCommand,
        FocusAnchorCommandKind, GetCurrentThreadId, INPUT_MOUSE, KEYEVENTF_KEYUP,
        KEYEVENTF_UNICODE, LPARAM, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_MOVE,
        MOUSEEVENTF_MOVE_NOCOALESCE, OwnedKeyboardKey, POINT, PostThreadMessageW,
        RUNNER_HOOK_EVENTS, RunnerChordEdge, RunnerChordKeyObservation, RunnerChordObservation,
        RunnerHookEdge, RunnerHookObserver, SS_NOTIFY, VK_END, VK_LMENU, VK_LSHIFT, VK_LWIN,
        WPARAM, adjacent_pointer_point, authoring_control_click_finished, cursor_points_match,
        cursor_restore_input_target, focus_anchor_candidate_positions, focus_anchor_window_style,
        format_uia_element_snapshot, forward_runner_hook_edge, fresh_canvas_cell_for_generation,
        keys_down_without_owned_keydowns, latest_authoring_controls_after,
        normalized_absolute_coordinate, parse_action_catalog_rank, parse_authoring_control,
        parse_geometry_state, pointer_correction_delta, record_runner_chord_edge,
        relative_mouse_move_input, retire_inserted_keyboard_ups, semantic_client_center,
        semantic_name_contains, spawn_focus_anchor_window, trace_event_lines,
        unique_authoring_control, window_process_id,
    };
    use std::time::Duration;

    #[test]
    fn chord_observer_counts_only_tagged_runner_edges_and_retains_foreign_edges() {
        let mut counts = std::collections::BTreeMap::from([
            (0xA0, [0; 4]),
            (0xA4, [0; 4]),
            (0x5B, [0; 4]),
            (0x23, [0; 4]),
        ]);
        let mut owned_edges = Vec::new();
        let mut foreign_edges = Vec::new();
        let ordered = [
            (0xA0, true),
            (0xA4, true),
            (0x5B, true),
            (0x23, true),
            (0x23, false),
            (0x5B, false),
            (0xA4, false),
            (0xA0, false),
        ];
        for (vk, down) in ordered {
            record_runner_chord_edge(
                RunnerHookEdge {
                    vk,
                    down,
                    injected: true,
                    extra_info: ACCEPTANCE_RUNNER_INPUT_COOKIE,
                    at: std::time::Instant::now(),
                },
                &mut counts,
                &mut owned_edges,
                &mut foreign_edges,
            );
        }
        for down in [true, false] {
            record_runner_chord_edge(
                RunnerHookEdge {
                    vk: 0xA4,
                    down,
                    injected: true,
                    extra_info: 0,
                    at: std::time::Instant::now(),
                },
                &mut counts,
                &mut owned_edges,
                &mut foreign_edges,
            );
        }
        let observation = RunnerChordObservation {
            desktop: "Default".into(),
            keys: counts
                .iter()
                .map(|(vk, edges)| RunnerChordKeyObservation {
                    vk: *vk,
                    down: edges[0],
                    up: edges[1],
                    injected_down: edges[2],
                    injected_up: edges[3],
                })
                .collect(),
            ordered_edges: owned_edges,
            foreign_edges,
        };
        assert!(observation.exact_injected_pairs(1));
        assert!(observation.exact_injected_sequence(&ordered));
        assert_eq!(observation.foreign_edges.len(), 2);
        assert!(observation.describe().contains("foreign_matching_edges=2"));
    }

    #[test]
    fn balanced_foreign_pair_in_released_gap_is_disclosed_without_contamination() {
        let start = std::time::Instant::now();
        let edge = |vk, down, at| RunnerChordEdge {
            vk,
            down,
            injected: true,
            extra_info: ACCEPTANCE_RUNNER_INPUT_COOKIE,
            at,
        };
        let foreign = |vk, down, at| RunnerChordEdge {
            vk,
            down,
            injected: true,
            extra_info: 0,
            at,
        };
        let mut ordered_edges = vec![
            edge(0xA0, true, start),
            edge(0xA4, true, start + Duration::from_millis(2)),
            edge(0x5B, true, start + Duration::from_millis(4)),
            edge(0x23, true, start + Duration::from_millis(6)),
            edge(0x23, false, start + Duration::from_millis(16)),
            edge(0x5B, false, start + Duration::from_millis(18)),
            edge(0xA4, false, start + Duration::from_millis(20)),
            edge(0xA0, false, start + Duration::from_millis(22)),
            edge(0xA0, true, start + Duration::from_millis(100)),
            edge(0xA4, true, start + Duration::from_millis(102)),
            edge(0x5B, true, start + Duration::from_millis(104)),
            edge(0x23, true, start + Duration::from_millis(106)),
            edge(0x23, false, start + Duration::from_millis(116)),
            edge(0x5B, false, start + Duration::from_millis(118)),
            edge(0xA4, false, start + Duration::from_millis(120)),
            edge(0xA0, false, start + Duration::from_millis(122)),
        ];
        ordered_edges.sort_by_key(|edge| edge.at);
        let foreign_edges = vec![
            foreign(0xA4, true, start + Duration::from_millis(50)),
            foreign(0xA4, false, start + Duration::from_millis(55)),
        ];
        let observation = RunnerChordObservation {
            desktop: "Default".into(),
            keys: Vec::new(),
            ordered_edges,
            foreign_edges,
        };

        assert!(
            observation
                .foreign_edges_interfering_with_owned_gestures()
                .is_empty()
        );
        assert_eq!(observation.foreign_edges.len(), 2);
    }

    #[test]
    fn orphan_foreign_release_in_released_gap_is_contamination() {
        let observation = two_tap_chord_observation(vec![(0xA4, false, 50)]);
        let contamination = observation.foreign_edges_interfering_with_owned_gestures();
        assert_eq!(contamination.len(), 1);
        assert!(!contamination[0].down);
    }

    #[test]
    fn foreign_up_after_next_gesture_begins_is_contamination() {
        let observation = two_tap_chord_observation(vec![(0xA4, true, 50), (0xA4, false, 105)]);
        let contamination = observation.foreign_edges_interfering_with_owned_gestures();
        assert_eq!(contamination.len(), 2);
        assert!(contamination[0].down);
        assert!(!contamination[1].down);
    }

    #[test]
    fn foreign_down_in_owned_span_is_contamination_when_release_is_in_gap() {
        let observation = two_tap_chord_observation(vec![(0xA4, true, 7), (0xA4, false, 50)]);
        let contamination = observation.foreign_edges_interfering_with_owned_gestures();
        assert_eq!(contamination.len(), 1);
        assert!(contamination[0].down);
    }

    #[test]
    fn unbalanced_foreign_down_in_released_gap_is_held_across_next_span() {
        let observation = two_tap_chord_observation(vec![(0xA4, true, 50)]);
        let contamination = observation.foreign_edges_interfering_with_owned_gestures();
        assert_eq!(contamination.len(), 1);
        assert!(contamination[0].down);
    }

    #[test]
    fn unbalanced_foreign_down_after_final_gesture_is_contamination() {
        let observation = two_tap_chord_observation(vec![(0xA4, true, 130)]);
        let contamination = observation.foreign_edges_interfering_with_owned_gestures();
        assert_eq!(contamination.len(), 1);
        assert!(contamination[0].down);
    }

    fn two_tap_chord_observation(foreign: Vec<(u32, bool, u64)>) -> RunnerChordObservation {
        let start = std::time::Instant::now();
        let owned = |vk, down, millis| RunnerChordEdge {
            vk,
            down,
            injected: true,
            extra_info: ACCEPTANCE_RUNNER_INPUT_COOKIE,
            at: start + Duration::from_millis(millis),
        };
        let mut ordered_edges = Vec::new();
        for offset in [0, 100] {
            ordered_edges.extend([
                owned(0xA0, true, offset),
                owned(0xA4, true, offset + 2),
                owned(0x5B, true, offset + 4),
                owned(0x23, true, offset + 6),
                owned(0x23, false, offset + 16),
                owned(0x5B, false, offset + 18),
                owned(0xA4, false, offset + 20),
                owned(0xA0, false, offset + 22),
            ]);
        }
        let foreign_edges = foreign
            .into_iter()
            .map(|(vk, down, millis)| RunnerChordEdge {
                vk,
                down,
                injected: true,
                extra_info: 0,
                at: start + Duration::from_millis(millis),
            })
            .collect();
        RunnerChordObservation {
            desktop: "Default".into(),
            keys: Vec::new(),
            ordered_edges,
            foreign_edges,
        }
    }

    #[test]
    fn key_quiet_preflight_resets_on_matching_foreign_edges() {
        let (sender, events) = std::sync::mpsc::channel();
        let (_probe_sender, probe_acks) = std::sync::mpsc::channel();
        let (_unhook_sender, unhook_result) = std::sync::mpsc::channel();
        let mut observer = RunnerHookObserver {
            events,
            probe_acks,
            unhook_result,
            thread_id: 0,
            hook_id: 0,
            join: None,
            desktop: "Default".into(),
        };
        sender
            .send(RunnerHookEdge {
                vk: 0xA4,
                down: false,
                injected: true,
                extra_info: 0,
                at: std::time::Instant::now(),
            })
            .unwrap();

        let started = std::time::Instant::now();
        let evidence = observer
            .wait_for_key_quiet(
                &[0xA0, 0xA4, 0x5B, 0x23],
                Duration::from_millis(25),
                Duration::from_millis(100),
            )
            .unwrap();
        assert_eq!(evidence.matching_edges, 1);
        assert!(evidence.quiet_ms >= 25);
        assert!(started.elapsed() >= Duration::from_millis(25));
    }

    #[test]
    fn runner_hook_forwards_direct_trigger_and_legacy_keyboard_edges() {
        let (sender, receiver) = std::sync::mpsc::channel();
        RUNNER_HOOK_EVENTS.with(|slot| *slot.borrow_mut() = Some(sender));
        let direct_trigger = [
            (0xA2, true),
            (0xA4, true),
            (0x54, true),
            (0x54, false),
            (0xA4, false),
            (0xA2, false),
        ];
        let legacy_trigger = [
            (0xA2, true),
            (0xA4, true),
            (0x59, true),
            (0x59, false),
            (0xA4, false),
            (0xA2, false),
        ];
        for (vk, down) in direct_trigger.into_iter().chain(legacy_trigger) {
            forward_runner_hook_edge(RunnerHookEdge {
                vk,
                down,
                injected: true,
                extra_info: ACCEPTANCE_RUNNER_INPUT_COOKIE,
                at: std::time::Instant::now(),
            });
        }
        RUNNER_HOOK_EVENTS.with(|slot| *slot.borrow_mut() = None);
        let observed = receiver
            .try_iter()
            .map(|edge| (edge.vk, edge.down))
            .collect::<Vec<_>>();
        let expected = direct_trigger
            .into_iter()
            .chain(legacy_trigger)
            .collect::<Vec<_>>();
        assert_eq!(observed, expected);
    }

    #[test]
    fn keyboard_ownership_follows_inserted_keyups_not_async_state() {
        let chord = vec![
            OwnedKeyboardKey {
                vk: VK_LSHIFT,
                extended: false,
            },
            OwnedKeyboardKey {
                vk: VK_LMENU,
                extended: false,
            },
            OwnedKeyboardKey {
                vk: VK_LWIN,
                extended: true,
            },
            OwnedKeyboardKey {
                vk: VK_END,
                extended: true,
            },
        ];
        let mut owned = chord.clone();
        let release_order = chord.iter().rev().copied().collect::<Vec<_>>();
        assert_eq!(
            retire_inserted_keyboard_ups(&mut owned, &release_order, 2),
            2
        );
        assert_eq!(owned, chord[..2]);

        let physical_down = vec![chord[1], chord[3]];
        let external = keys_down_without_owned_keydowns(&chord, &owned, &physical_down);
        assert_eq!(external, vec![chord[3]]);

        assert_eq!(
            retire_inserted_keyboard_ups(&mut owned, &release_order[2..], 2),
            2
        );
        assert!(owned.is_empty());
        assert_eq!(
            keys_down_without_owned_keydowns(&chord, &owned, &physical_down),
            physical_down
        );
    }

    #[test]
    fn focus_anchor_candidates_stay_inside_physical_displays_and_skip_gaps() {
        let displays = [[-1920, 0, 0, 1080], [400, 0, 2320, 1080]];
        let positions = focus_anchor_candidate_positions(&displays, 320, 96);
        assert_eq!(positions.len(), 18);
        assert!(positions.iter().all(|(x, y)| {
            let bounds = [*x, *y, *x + 320, *y + 96];
            displays.iter().any(|display| {
                bounds[0] >= display[0]
                    && bounds[1] >= display[1]
                    && bounds[2] <= display[2]
                    && bounds[3] <= display[3]
            })
        }));
        assert!(
            !positions
                .iter()
                .any(|(x, _)| *x < 400 && x.saturating_add(320) > 0)
        );
        assert!(focus_anchor_candidate_positions(&[[0, 0, 240, 120]], 320, 96).is_empty());
    }

    #[test]
    fn focus_anchor_static_window_accepts_hit_testing() {
        assert_ne!(focus_anchor_window_style().0 & SS_NOTIFY.0, 0);
    }

    #[test]
    fn focus_anchor_owns_a_live_message_pump_until_bounded_destroy() {
        let process_id = std::process::id();
        let (hwnd, thread_id, command_tx, ui_thread) =
            spawn_focus_anchor_window(process_id).expect("create message-pumped focus anchor");
        assert_eq!(window_process_id(hwnd), process_id);
        assert_ne!(thread_id, unsafe { GetCurrentThreadId() });

        let send_command = |kind| {
            let (reply_tx, reply_rx) = std::sync::mpsc::sync_channel(1);
            command_tx
                .send(FocusAnchorCommand {
                    kind,
                    reply: reply_tx,
                })
                .expect("send bounded focus anchor UI command");
            unsafe {
                PostThreadMessageW(
                    thread_id,
                    super::FOCUS_ANCHOR_COMMAND_MESSAGE,
                    WPARAM(0),
                    LPARAM(0),
                )
            }
            .expect("wake focus anchor UI message pump");
            reply_rx
                .recv_timeout(std::time::Duration::from_secs(1))
                .expect("focus anchor UI command reply")
                .expect("focus anchor UI command succeeds")
        };

        send_command(FocusAnchorCommandKind::Raise);
        send_command(FocusAnchorCommandKind::MoveTo { left: 64, top: 64 });
        send_command(FocusAnchorCommandKind::RestoreNonTopmost);
        send_command(FocusAnchorCommandKind::Destroy);
        ui_thread
            .join()
            .expect("focus anchor UI thread exits after destroy");
        assert_eq!(window_process_id(hwnd), 0);
    }

    fn authoring_control() -> AuthoringControlSnapshot {
        AuthoringControlSnapshot {
            target: AuthoringControlTarget::MenuRow,
            role: AuthoringControlRole::Selectable,
            index: Some(0),
            bounds: [20, 30, 60, 50],
            client_size: [640, 480],
            enabled: true,
            selected: false,
            focused: false,
            clicked: false,
            session_id: 9,
            generation: 4,
            menu_cell_ids_digest: None,
            ring_index: None,
            slot_index: None,
        }
    }

    #[test]
    fn action_catalog_rank_parser_accepts_only_numeric_identity_evidence() {
        let rank = parse_action_catalog_rank(
            "trace_event=\"designer_action_catalog_rank\" elapsed_ms=42 custom_action_index=67 rank=71 catalog_len=140 session_id=9 generation=4",
        )
        .expect("numeric rank evidence should parse");
        assert_eq!(
            rank,
            ActionCatalogRankSnapshot {
                custom_action_index: 67,
                rank: 71,
                catalog_len: 140,
                session_id: 9,
                generation: 4,
            }
        );
        assert!(parse_action_catalog_rank(
            "trace_event=\"designer_action_catalog_rank\" custom_action_index=private rank=71 catalog_len=140 session_id=9 generation=4"
        )
        .is_none());
    }

    #[test]
    fn geometry_state_parser_preserves_selected_cell_identity_and_resolution_confidence() {
        let resolved = parse_geometry_state(
            "trace_event=\"designer_geometry_state\" session_id=9 menu_count=2 selected_menu_index=1 selected_menu_after_action=Some(CloseTree) ring_count=1 selected_ring_index=0 selected_cell_index=3 selected_cell_id_digest=123456 selected_cell_custom_action_index=63 selected_cell_custom_action_index_known=true selected_ring_slots=8 requested_slots=8 selected_ring_populated=1 menu_populated=1 draft_cell_ids_digest=44 proposal_cell_ids_digest=0 proposal_cell_ids_digest_available=false proposal_kind=None proposal_active=false proposal_ready=false proposal_slots=0 proposal_candidate_rings=0 proposal_resolution_populated=0 proposal_cell_ids_preserved=true resize_prompt_open=false resize_prompt_populated=0 generation=5",
        )
        .expect("complete geometry event should parse");
        assert_eq!(resolved.session_id, 9);
        assert_eq!(resolved.selected_cell_index, Some(3));
        assert_eq!(resolved.selected_cell_id_digest, Some(123456));
        assert_eq!(resolved.selected_cell_custom_action_index, Some(63));
        assert!(resolved.selected_cell_custom_action_index_known);
        assert_eq!(
            resolved.selected_menu_after_action,
            Some(multi_launcher::radial::model::AfterActionPolicy::CloseTree)
        );
        assert_eq!(resolved.generation, 5);

        let unresolved = parse_geometry_state(
            "trace_event=\"designer_geometry_state\" session_id=9 menu_count=2 selected_menu_index=1 selected_menu_after_action=None ring_count=1 selected_ring_index=0 selected_cell_index=3 selected_cell_id_digest=123456 selected_cell_custom_action_index=-1 selected_cell_custom_action_index_known=false selected_ring_slots=8 requested_slots=8 selected_ring_populated=1 menu_populated=1 draft_cell_ids_digest=44 proposal_cell_ids_digest=0 proposal_cell_ids_digest_available=false proposal_kind=None proposal_active=false proposal_ready=false proposal_slots=0 proposal_candidate_rings=0 proposal_resolution_populated=0 proposal_cell_ids_preserved=true resize_prompt_open=false resize_prompt_populated=0 generation=5",
        )
        .expect("geometry event with unavailable action resolution should parse");
        assert_eq!(unresolved.selected_cell_id_digest, Some(123456));
        assert_eq!(unresolved.selected_cell_custom_action_index, None);
        assert!(!unresolved.selected_cell_custom_action_index_known);
        assert_eq!(unresolved.selected_menu_after_action, None);
    }

    #[test]
    fn authoring_lookup_rejects_ambiguous_scoped_targets() {
        let duplicate = authoring_control();
        let error = unique_authoring_control(
            &[duplicate, duplicate],
            AuthoringControlTarget::MenuRow,
            Some(0),
            AuthoringControlRole::Selectable,
        )
        .unwrap_err();
        assert!(error.contains("ambiguous Designer semantic target"));

        assert!(
            unique_authoring_control(
                &[authoring_control()],
                AuthoringControlTarget::MenuRow,
                Some(1),
                AuthoringControlRole::Selectable,
            )
            .unwrap()
            .is_none()
        );
    }

    #[test]
    fn fresh_canvas_cell_lookup_requires_current_menu_ring_slot_and_generation() {
        let stale = AuthoringControlSnapshot {
            target: AuthoringControlTarget::CanvasCell,
            role: AuthoringControlRole::Region,
            index: Some(8),
            generation: 8,
            menu_cell_ids_digest: Some(101),
            ring_index: Some(1),
            slot_index: Some(0),
            ..authoring_control()
        };
        let fresh = AuthoringControlSnapshot {
            menu_cell_ids_digest: Some(202),
            ..stale
        };
        assert_eq!(
            fresh_canvas_cell_for_generation(&[stale, fresh], 9, 8, 8, 202, 1, 0),
            Some(fresh)
        );
        assert_eq!(
            fresh_canvas_cell_for_generation(&[stale], 9, 8, 8, 202, 1, 0),
            None
        );
        assert_eq!(
            fresh_canvas_cell_for_generation(&[fresh], 9, 8, 8, 202, 0, 0),
            None
        );
    }

    #[test]
    fn authoring_parser_accepts_quoted_roles_and_unindexed_targets() {
        let control = parse_authoring_control(
            "trace_event=\"designer_authoring_control\" target=NewMenu role=\"Button\" viewport=Deferred control_index=-1 left_px=14 top_px=116 right_px=83 bottom_px=134 client_width_px=640 client_height_px=480 enabled=true selected=false focused=true clicked=false session_id=2 generation=2 menu_cell_ids_digest=0 cell_ring_index=-1 cell_slot_index=-1",
        )
        .expect("serialized New Menu control should parse");

        assert_eq!(control.target, AuthoringControlTarget::NewMenu);
        assert_eq!(control.role, AuthoringControlRole::Button);
        assert_eq!(control.index, None);
        assert!(control.focused);
        assert_eq!(control.session_id, 2);
        assert_eq!(control.bounds, [14, 116, 83, 134]);
        assert_eq!(control.client_size, [640, 480]);
    }

    #[test]
    fn canvas_cell_parser_retains_redacted_menu_ring_and_slot_scope() {
        let control = parse_authoring_control(
            "trace_event=\"designer_authoring_control\" target=CanvasCell role=\"Region\" viewport=Deferred control_index=8 left_px=20 top_px=30 right_px=28 bottom_px=38 client_width_px=640 client_height_px=480 enabled=true selected=false focused=false clicked=false session_id=2 generation=7 menu_cell_ids_digest=123456 cell_ring_index=1 cell_slot_index=0",
        )
        .expect("scoped CanvasCell evidence should parse");

        assert_eq!(control.menu_cell_ids_digest, Some(123456));
        assert_eq!(control.ring_index, Some(1));
        assert_eq!(control.slot_index, Some(0));
        assert_eq!(control.index, Some(8));
    }

    #[test]
    fn post_resize_authoring_lookup_does_not_reuse_a_disappeared_control() {
        let stale = "trace_event=\"designer_authoring_control\" target=Canvas role=\"Region\" viewport=Deferred control_index=-1 left_px=20 top_px=30 right_px=620 bottom_px=430 client_width_px=640 client_height_px=480 enabled=true selected=false focused=false clicked=false session_id=9 generation=4 menu_cell_ids_digest=0 cell_ring_index=-1 cell_slot_index=-1".to_owned();
        let fresh = "trace_event=\"designer_authoring_control\" target=Canvas role=\"Region\" viewport=Deferred control_index=-1 left_px=20 top_px=30 right_px=520 bottom_px=390 client_width_px=520 client_height_px=380 enabled=true selected=false focused=false clicked=false session_id=9 generation=4 menu_cell_ids_digest=0 cell_ring_index=-1 cell_slot_index=-1".to_owned();
        let post_resize_cursor = 1;
        let stale_trace = format!("logger startup line\n{stale}\n");
        let stale_events = trace_event_lines(&stale_trace);
        assert!(latest_authoring_controls_after(&stale_events, post_resize_cursor, 9).is_empty());

        let trace = format!("logger startup line\n{stale}\nlogger resize line\n{fresh}\n");
        let events = trace_event_lines(&trace);
        let controls = latest_authoring_controls_after(&events, post_resize_cursor, 9);
        let current = unique_authoring_control(
            &controls,
            AuthoringControlTarget::Canvas,
            None,
            AuthoringControlRole::Region,
        )
        .expect("fresh canvas should not be ambiguous")
        .expect("post-resize render should publish a current canvas");
        assert_eq!(current.bounds, [20, 30, 520, 390]);
    }

    #[test]
    fn authoring_control_click_requires_a_later_frame_after_activation() {
        let click = "trace_event=\"designer_authoring_control\" target=Slots role=\"DragValue\" viewport=Deferred control_index=-1 left_px=298 top_px=116 right_px=342 bottom_px=134 client_width_px=640 client_height_px=480 enabled=true selected=false focused=false clicked=true session_id=2 generation=4 menu_cell_ids_digest=0 cell_ring_index=-1 cell_slot_index=-1".to_owned();
        let after_click = "trace_event=\"designer_authoring_control\" target=Slots role=\"DragValue\" viewport=Deferred control_index=-1 left_px=298 top_px=116 right_px=342 bottom_px=134 client_width_px=640 client_height_px=480 enabled=true selected=false focused=false clicked=false session_id=2 generation=4 menu_cell_ids_digest=0 cell_ring_index=-1 cell_slot_index=-1".to_owned();

        assert!(!authoring_control_click_finished(
            std::slice::from_ref(&click),
            2,
            AuthoringControlTarget::Slots,
            None,
            AuthoringControlRole::DragValue,
        ));
        assert!(!authoring_control_click_finished(
            &[click.clone(), after_click.clone()],
            1,
            AuthoringControlTarget::Slots,
            None,
            AuthoringControlRole::DragValue,
        ));
        assert!(authoring_control_click_finished(
            &[click, after_click],
            2,
            AuthoringControlTarget::Slots,
            None,
            AuthoringControlRole::DragValue,
        ));
    }

    #[test]
    fn pointer_ack_correction_is_exact_and_bounded() {
        assert_eq!(
            pointer_correction_delta((90, 79), (88, 81)).unwrap(),
            Some((-2, 2))
        );
        assert_eq!(pointer_correction_delta((88, 81), (88, 81)).unwrap(), None);
        assert!(pointer_correction_delta((100, 81), (88, 81)).is_err());
    }

    #[test]
    fn semantic_client_center_requires_an_in_client_target_rectangle() {
        let point = semantic_client_center([20, 30, 60, 50], [0, 0, 100, 80]).unwrap();
        assert_eq!((point.x, point.y), (40, 40));

        assert!(semantic_client_center([80, 10, 120, 30], [0, 0, 100, 80]).is_err());
        assert!(semantic_client_center([10, 20, 10, 30], [0, 0, 100, 80]).is_err());
        assert!(semantic_client_center([i32::MIN, 0, i32::MAX, 20], [0, 0, 100, 80]).is_err());
    }

    #[test]
    fn absolute_mouse_coordinates_cover_a_negative_origin_virtual_desktop() {
        assert_eq!(normalized_absolute_coordinate(-1920, -1920, 5760), 0);
        assert_eq!(normalized_absolute_coordinate(959, -1920, 5760), 32762);
        assert_eq!(
            normalized_absolute_coordinate(3839, -1920, 5760),
            i32::from(u16::MAX)
        );
        assert_eq!(normalized_absolute_coordinate(-2500, -1920, 5760), 0);
        assert_eq!(
            normalized_absolute_coordinate(4000, -1920, 5760),
            i32::from(u16::MAX)
        );
    }

    #[test]
    fn cursor_restore_verification_allows_one_pixel_of_absolute_input_rounding() {
        let target = POINT { x: 408, y: 424 };
        assert!(cursor_points_match(target, target));
        assert!(cursor_points_match(POINT { x: 409, y: 423 }, target));
        assert!(!cursor_points_match(POINT { x: 410, y: 424 }, target));
    }

    #[test]
    fn cursor_restore_input_uses_an_interior_coordinate_for_desktop_edges() {
        let edge = POINT { x: 0, y: 535 };
        let input_target = cursor_restore_input_target(edge, (0, 0, 2560, 1440)).unwrap();
        assert_eq!((input_target.x, input_target.y), (1, 535));
        assert!(cursor_points_match(input_target, edge));

        let inside = POINT { x: 1200, y: 700 };
        let input_target = cursor_restore_input_target(inside, (0, 0, 2560, 1440)).unwrap();
        assert_eq!((input_target.x, input_target.y), (1200, 700));

        assert!(cursor_restore_input_target(edge, (0, 0, 0, 1440)).is_err());
    }

    #[test]
    fn cursor_restore_correction_is_a_relative_move_without_button_input() {
        let movement = relative_mouse_move_input(-104, 208);
        assert_eq!(movement.r#type, INPUT_MOUSE);
        let mouse = unsafe { movement.Anonymous.mi };
        assert_eq!((mouse.dx, mouse.dy), (-104, 208));
        assert_eq!(
            mouse.dwFlags,
            MOUSEEVENTF_MOVE | MOUSEEVENTF_MOVE_NOCOALESCE
        );
        assert!(!mouse.dwFlags.contains(MOUSEEVENTF_ABSOLUTE));
    }

    #[test]
    fn pointer_nudge_stays_inside_semantic_bounds_and_differs_from_target() {
        let target = POINT { x: 20, y: 30 };
        let bounds = [10, 20, 31, 41];
        let nudge = adjacent_pointer_point(target, bounds).unwrap();
        assert_ne!((nudge.x, nudge.y), (target.x, target.y));
        assert!(nudge.x >= bounds[0] && nudge.x < bounds[2]);
        assert!(nudge.y >= bounds[1] && nudge.y < bounds[3]);

        let narrow_target = POINT { x: 5, y: 8 };
        let narrow_bounds = [5, 2, 6, 15];
        let nudge = adjacent_pointer_point(narrow_target, narrow_bounds).unwrap();
        assert_eq!(nudge.x, narrow_target.x);
        assert_ne!(nudge.y, narrow_target.y);
        assert!(nudge.y >= narrow_bounds[1] && nudge.y < narrow_bounds[3]);
    }

    #[test]
    fn durable_uia_snapshot_redacts_free_form_properties_and_keeps_structure() {
        let snapshot = format_uia_element_snapshot(
            41,
            50004,
            [10, 20, 110, 44],
            true,
            true,
            true,
            true,
            "Private result label",
            "UserDefinedClass",
            "PrivateFrameworkMarker",
            "MenuName.PrivateAutomationId",
        );

        assert!(snapshot.contains("pid=41"));
        assert!(snapshot.contains("type=50004"));
        assert!(snapshot.contains("bounds=[10, 20, 110, 44]"));
        assert!(snapshot.contains("focusable=true focused=true control=true content=true"));
        assert!(snapshot.contains("name=<redacted>"));
        assert!(snapshot.contains("class=<redacted>"));
        assert!(snapshot.contains("framework=<redacted>"));
        assert!(snapshot.contains("automation_id=<redacted>"));
        for private_value in [
            "Private result label",
            "UserDefinedClass",
            "PrivateFrameworkMarker",
            "MenuName.PrivateAutomationId",
        ] {
            assert!(
                !snapshot.contains(private_value),
                "durable UIA snapshot exposed {private_value:?}: {snapshot}"
            );
        }
    }

    #[test]
    fn command_result_name_match_is_case_insensitive_and_label_scoped() {
        assert!(semantic_name_contains(
            "Edit radial skins : Radial menu",
            "Edit radial skins"
        ));
        assert!(semantic_name_contains(
            "Edit RADIAL SKINS",
            "Edit radial skins"
        ));
        assert!(!semantic_name_contains("Radial menu", "Edit radial skins"));
    }

    #[test]
    fn focused_unicode_input_is_generated_as_balanced_unicode_edges() {
        let events = super::unicode_text_events("Aé");
        assert_eq!(events.len(), 4);
        let keys = events
            .iter()
            .map(|event| unsafe { event.Anonymous.ki })
            .collect::<Vec<_>>();
        assert!(keys.iter().all(|key| key.wVk.0 == 0));
        assert_eq!(
            keys.iter().map(|key| key.wScan).collect::<Vec<_>>(),
            [b'A' as u16, b'A' as u16, 'é' as u16, 'é' as u16,]
        );
        assert_eq!(keys[0].dwFlags, KEYEVENTF_UNICODE);
        assert_eq!(keys[1].dwFlags, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP);
        assert_eq!(keys[2].dwFlags, KEYEVENTF_UNICODE);
        assert_eq!(keys[3].dwFlags, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP);
    }
}
