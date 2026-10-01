//! Native Windows boundary helpers for the opt-in radial acceptance process.

#![cfg(windows)]

mod suite;
use super::{AcceptanceHotkey, foreign_edge_indices_interfering_owned_spans, owned_gesture_spans};
pub(super) use suite::{
    CopiedAuthoringOptions, record_environment_failure, run_copied_profile_suite, run_gate_c_suite,
    run_gate_d_suite, run_gate_s_suite, run_hotkey_suite, run_query_suite, run_suite,
};

use multi_launcher::radial::acceptance_trace::{
    DesignerAuthoringScrollOwner as GateDControlScrollOwner, DesignerAuthoringScrollViewport,
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
use windows::Win32::Graphics::Gdi::{ClientToScreen, InvalidateRect, ScreenToClient};
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
    TreeScope_Descendants, UIA_ButtonControlTypeId, UIA_CONTROLTYPE_ID, UIA_ComboBoxControlTypeId,
    UIA_EditControlTypeId, UIA_ListItemControlTypeId, UIA_NamePropertyId,
    UIA_SelectionItemPatternId, UIA_TogglePatternId, UIA_ValuePatternId,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, HOT_KEY_MODIFIERS, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT,
    KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, MOUSEEVENTF_ABSOLUTE,
    MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MOVE, MOUSEEVENTF_MOVE_NOCOALESCE,
    MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_VIRTUALDESK, MOUSEEVENTF_WHEEL,
    MOUSEINPUT, RegisterHotKey, SendInput, UnregisterHotKey, VIRTUAL_KEY, VK_CONTROL, VK_END,
    VK_ESCAPE, VK_F4, VK_F11, VK_F24, VK_LBUTTON, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN,
    VK_MENU, VK_RBUTTON, VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SHIFT, VK_TAB,
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
const TRACE_BUDGET_PROFILE_ENV: &str = "MULTI_LAUNCHER_RADIAL_ACCEPTANCE_TRACE_PROFILE";
const QUERY_OBSERVATION_ENV: &str = "MULTI_LAUNCHER_RADIAL_ACCEPTANCE_OBSERVATION_FILE";
const PREPARE_HOLD_ENV: &str = "MULTI_LAUNCHER_RADIAL_ACCEPTANCE_PREPARE_HOLD_FILE";
const PREPARE_HOLD_FILE_NAME: &str = "radial-acceptance-prepare.hold";
const AUTHORING_SEARCH_HOLD_ENV: &str =
    "MULTI_LAUNCHER_RADIAL_ACCEPTANCE_AUTHORING_SEARCH_HOLD_FILE";
pub(super) const AUTHORING_SEARCH_HOLD_FILE_NAME: &str = "radial-acceptance-authoring-search.hold";
const ROOT_TITLE: &str = "Multi Lnchr";
const DESIGNER_TITLE: &str = "Radial Designer";
pub(super) const RADIAL_HOST_WINDOW_CLASS: &str = "MultiLauncherRadialHost";
const WINDOW_POLL: Duration = Duration::from_millis(25);
const ACTION_EDITOR_SCROLL_REFRESH_INTERVAL: Duration = Duration::from_millis(600);
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AcceptanceTraceBudgetProfile {
    Standard,
    GateC,
    GateD,
    GateS,
}

impl AcceptanceTraceBudgetProfile {
    fn environment_value(self) -> Option<&'static str> {
        match self {
            Self::Standard => None,
            Self::GateC => Some("gate_c_v1"),
            Self::GateD => Some("gate_d_v1"),
            Self::GateS => Some("gate_s_v1"),
        }
    }
}

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
    // Preserve physical-edge precision for admission. Millisecond conversion
    // belongs to the bounded report summary after the timing checks pass.
    pub primary_holds: Vec<Duration>,
    pub released_gaps: Vec<Duration>,
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
        let mut primary_holds = Vec::with_capacity(taps);
        let mut released_gaps = Vec::with_capacity(taps.saturating_sub(1));
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
            primary_holds.push(hold);
            if tap + 1 < taps {
                let next_down = self.ordered_edges[(tap + 1) * edges_per_tap + primary_down_offset];
                if !next_down.down || next_down.vk != down.vk {
                    return Err("next primary press was missing from cadence trace".into());
                }
                let gap = next_down
                    .at
                    .checked_duration_since(up.at)
                    .ok_or_else(|| "next primary press preceded the prior release".to_string())?;
                released_gaps.push(gap);
            }
        }
        Ok(RunnerChordTiming {
            primary_holds,
            released_gaps,
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
    pub button: PointerButton,
    pub button_state_after_down: i16,
    pub button_state_before_up: i16,
    pub up: NativeInputEdgeEvidence,
    pub up_acknowledged: bool,
    pub button_state_after_up: i16,
    pub target_hwnd: HWND,
    pub nudge_under_cursor_hwnd: HWND,
    pub under_cursor_hwnd: HWND,
    pub foreground_hwnd: HWND,
    pub screen_point: (i32, i32),
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum PointerClickPreDownError {
    StaleGeometry(String),
    Input(String),
}

impl From<String> for PointerClickPreDownError {
    fn from(error: String) -> Self {
        Self::Input(error)
    }
}

impl From<&'static str> for PointerClickPreDownError {
    fn from(error: &'static str) -> Self {
        Self::Input(error.to_owned())
    }
}

impl PointerClickPreDownError {
    fn into_message(self) -> String {
        match self {
            Self::StaleGeometry(error) | Self::Input(error) => error,
        }
    }
}

pub(super) fn dispatch_pointer_down_after_preflight<T>(
    preflight: impl FnOnce() -> Result<(), PointerClickPreDownError>,
    send_down: impl FnOnce() -> Result<T, String>,
) -> Result<T, PointerClickPreDownError> {
    preflight()?;
    send_down().map_err(PointerClickPreDownError::Input)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PointerButton {
    Left,
    Right,
}

impl PointerButton {
    fn virtual_key(self) -> u16 {
        match self {
            Self::Left => VK_LBUTTON.0,
            Self::Right => VK_RBUTTON.0,
        }
    }
}

impl PointerClickEvidence {
    pub fn describe(&self) -> String {
        format!(
            "screen_point=({},{}), target_hwnd={}, nudge_under_cursor_hwnd={}, under_cursor_hwnd={}, foreground_hwnd={}, button={:?}, preexisting_egui_pointer_ack={}, fresh_egui_pointer_move_ack={}, nudge_move=[{}], move=[{}], pointer_correction_events={}, down=[{}], root_or_designer_down_ack={}, button_async_after_down=0x{:04x}, button_async_before_up=0x{:04x}, up=[{}], root_or_designer_up_ack={}, button_async_after_up=0x{:04x}",
            self.screen_point.0,
            self.screen_point.1,
            hwnd_id(self.target_hwnd),
            hwnd_id(self.nudge_under_cursor_hwnd),
            hwnd_id(self.under_cursor_hwnd),
            hwnd_id(self.foreground_hwnd),
            self.button,
            self.pointer_position_preexisting_ack,
            self.pointer_move_acknowledged,
            self.nudge_movement.describe(),
            self.movement.describe(),
            self.pointer_correction_events,
            self.down.describe(),
            self.down_acknowledged,
            self.button_state_after_down as u16,
            self.button_state_before_up as u16,
            self.up.describe(),
            self.up_acknowledged,
            self.button_state_after_up as u16
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
    close_input_desktop_after_thread_exit(handle, "native driver thread")
}

fn close_input_desktop_after_thread_exit(handle: isize, thread_name: &str) -> Result<(), String> {
    unsafe { CloseDesktop(HDESK(handle as *mut std::ffi::c_void)) }
        .map_err(|error| format!("close input desktop after {thread_name} exit: {error}"))
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

fn active_input_desktop_name() -> Result<String, String> {
    let desktop = unsafe {
        OpenInputDesktop(
            Default::default(),
            false,
            DESKTOP_ACCESS_FLAGS(DESKTOP_READOBJECTS.0),
        )
    }
    .map_err(|error| format!("open active input desktop for focus-anchor verification: {error}"))?;
    let name_result = desktop_name(desktop);
    let close_result = unsafe { CloseDesktop(desktop) }
        .map_err(|error| format!("close focus-anchor input desktop verification handle: {error}"));
    let name = name_result?;
    close_result?;
    Ok(name)
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
    ui_thread: Option<std::thread::JoinHandle<FocusAnchorThreadExit>>,
}

#[derive(Debug)]
struct FocusAnchorWindowReady {
    hwnd: usize,
    thread_id: u32,
    owner_desktop_name: String,
    input_desktop_name: String,
}

struct FocusAnchorThreadExit {
    attached_desktop_handle: Option<isize>,
    teardown_error: Option<String>,
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
        let (ready, command_tx, ui_thread) = spawn_focus_anchor_window(process_id)?;
        let hwnd = HWND(ready.hwnd as *mut std::ffi::c_void);
        let ui_thread_id = ready.thread_id;
        if !ready.owner_desktop_name.eq_ignore_ascii_case("Default")
            || !ready
                .owner_desktop_name
                .eq_ignore_ascii_case(&ready.input_desktop_name)
        {
            let _ = unsafe { PostThreadMessageW(ui_thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) };
            let cleanup = finish_focus_anchor_ui_thread(ui_thread);
            return Err(format!(
                "focus anchor owner desktop '{}' did not match active input desktop '{}'; cleanup={cleanup:?}",
                ready.owner_desktop_name, ready.input_desktop_name
            ));
        }
        if window_process_id(hwnd) != process_id {
            let _ = unsafe { PostThreadMessageW(ui_thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) };
            let cleanup = finish_focus_anchor_ui_thread(ui_thread);
            return Err(format!(
                "focus anchor HWND is not owned by the acceptance runner; cleanup={cleanup:?}"
            ));
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
            let mut button_guard =
                MouseButtonGuard::new(self.hwnd, self.process_id, PointerButton::Left);
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
        FocusAnchorWindowReady,
        std::sync::mpsc::SyncSender<FocusAnchorCommand>,
        std::thread::JoinHandle<FocusAnchorThreadExit>,
    ),
    String,
> {
    spawn_focus_anchor_window_with_startup_hooks(
        process_id,
        Duration::from_secs(2),
        || {},
        || {},
        || {},
    )
}

fn spawn_focus_anchor_window_with_startup_hooks<BeforeAttach, BeforeCreate, OnTimeout>(
    process_id: u32,
    startup_timeout: Duration,
    before_attach: BeforeAttach,
    before_create: BeforeCreate,
    on_startup_timeout: OnTimeout,
) -> Result<
    (
        FocusAnchorWindowReady,
        std::sync::mpsc::SyncSender<FocusAnchorCommand>,
        std::thread::JoinHandle<FocusAnchorThreadExit>,
    ),
    String,
>
where
    BeforeAttach: FnOnce() + Send + 'static,
    BeforeCreate: FnOnce() + Send + 'static,
    OnTimeout: FnOnce(),
{
    let (thread_id_tx, thread_id_rx) = std::sync::mpsc::sync_channel::<Result<u32, String>>(1);
    let (window_tx, window_rx) = std::sync::mpsc::sync_channel(1);
    let (command_tx, command_rx) = std::sync::mpsc::sync_channel::<FocusAnchorCommand>(1);
    let mut on_startup_timeout = Some(on_startup_timeout);
    let ui_thread = std::thread::Builder::new()
        .name("radial-acceptance-focus-anchor".into())
        .spawn(move || {
            let thread_id = unsafe { GetCurrentThreadId() };
            before_attach();
            let (attachment_observation, desktop_attachment) =
                match attach_to_input_desktop() {
                    Ok(attached) => attached,
                    Err(error) => {
                        let _ = thread_id_tx.send(Err(format!(
                            "focus anchor could not attach to the active input desktop before USER initialization: {error}"
                        )));
                        return FocusAnchorThreadExit {
                            attached_desktop_handle: None,
                            teardown_error: None,
                        };
                    }
                };

            let desktop_identity = (|| {
                let owner_desktop = unsafe { GetThreadDesktop(thread_id) }
                    .map_err(|error| format!("read focus-anchor owner thread desktop: {error}"))?;
                let owner_desktop_name = desktop_name(owner_desktop)?;
                let input_desktop_name = active_input_desktop_name()?;
                if !owner_desktop_name.eq_ignore_ascii_case("Default")
                    || !owner_desktop_name.eq_ignore_ascii_case(&input_desktop_name)
                {
                    return Err(format!(
                        "focus-anchor owner thread desktop '{owner_desktop_name}' does not match active input desktop '{input_desktop_name}' (attachment={attachment_observation})"
                    ));
                }
                Ok((owner_desktop_name, input_desktop_name))
            })();
            let (owner_desktop_name, input_desktop_name) = match desktop_identity {
                Ok(names) => names,
                Err(error) => {
                    let _ = thread_id_tx.send(Err(error));
                    return focus_anchor_thread_exit(desktop_attachment, None);
                }
            };

            // SetThreadDesktop must happen before this thread creates its USER queue or
            // any HWND. Keep the attachment guard alive until the anchor is destroyed.
            let mut queue_probe = windows::Win32::UI::WindowsAndMessaging::MSG::default();
            let _ = unsafe { PeekMessageW(&mut queue_probe, None, 0, 0, PM_NOREMOVE) };
            if thread_id_tx.send(Ok(thread_id)).is_err() {
                return focus_anchor_thread_exit(desktop_attachment, None);
            }
            before_create();

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
                    let _ = window_tx.send(Err(format!(
                        "create runner-owned focus anchor on active input desktop '{input_desktop_name}': {error}"
                    )));
                    return focus_anchor_thread_exit(desktop_attachment, None);
                }
            };
            let ready = FocusAnchorWindowReady {
                hwnd: hwnd.0 as usize,
                thread_id,
                owner_desktop_name,
                input_desktop_name,
            };
            if window_tx.send(Ok(ready)).is_err() {
                let teardown_error = destroy_focus_anchor_window(hwnd, process_id).err();
                return focus_anchor_thread_exit(desktop_attachment, teardown_error);
            }

            let mut message = windows::Win32::UI::WindowsAndMessaging::MSG::default();
            let mut pump_error = None;
            loop {
                let result = unsafe { GetMessageW(&mut message, None, 0, 0) };
                if result.0 == 0 {
                    break;
                }
                if result.0 < 0 {
                    pump_error = Some(format!(
                        "focus anchor message pump failed: {:?}",
                        unsafe { GetLastError() }
                    ));
                    break;
                }
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
            let teardown_error = destroy_focus_anchor_window(hwnd, process_id)
                .err()
                .or(pump_error);
            focus_anchor_thread_exit(desktop_attachment, teardown_error)
        })
        .map_err(|error| format!("start focus anchor UI thread: {error}"))?;

    let thread_id = match thread_id_rx.recv_timeout(startup_timeout) {
        Ok(Ok(thread_id)) => thread_id,
        Ok(Err(error)) => {
            drop(window_rx);
            let cleanup = finish_focus_anchor_ui_thread(ui_thread);
            return Err(format!("{error}; cleanup={cleanup:?}"));
        }
        Err(error) => {
            drop(window_rx);
            drop(thread_id_rx);
            if let Some(on_timeout) = on_startup_timeout.take() {
                on_timeout();
            }
            let cleanup = finish_focus_anchor_ui_thread(ui_thread);
            return Err(format!(
                "focus anchor UI thread did not start: {error}; cleanup={cleanup:?}"
            ));
        }
    };
    drop(thread_id_rx);
    let ready = match window_rx.recv_timeout(startup_timeout) {
        Ok(Ok(ready)) => ready,
        Ok(Err(error)) => {
            let cleanup = finish_focus_anchor_ui_thread(ui_thread);
            return Err(format!("{error}; cleanup={cleanup:?}"));
        }
        Err(error) => {
            drop(window_rx);
            let _ = unsafe { PostThreadMessageW(thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) };
            if let Some(on_timeout) = on_startup_timeout.take() {
                on_timeout();
            }
            let cleanup = finish_focus_anchor_ui_thread(ui_thread);
            return Err(format!(
                "focus anchor window creation timed out: {error}; cleanup={cleanup:?}"
            ));
        }
    };

    Ok((ready, command_tx, ui_thread))
}

fn focus_anchor_thread_exit(
    desktop_attachment: InputDesktopAttachment,
    teardown_error: Option<String>,
) -> FocusAnchorThreadExit {
    FocusAnchorThreadExit {
        attached_desktop_handle: desktop_attachment.release_for_thread_exit(),
        teardown_error,
    }
}

fn finish_focus_anchor_ui_thread(
    ui_thread: std::thread::JoinHandle<FocusAnchorThreadExit>,
) -> Result<(), String> {
    let exit = ui_thread
        .join()
        .map_err(|_| "focus anchor UI thread panicked during cleanup".to_string())?;
    let mut errors = Vec::new();
    if let Some(error) = exit.teardown_error {
        errors.push(error);
    }
    if let Some(handle) = exit.attached_desktop_handle {
        if let Err(error) = close_input_desktop_after_thread_exit(handle, "focus anchor UI thread")
        {
            errors.push(error);
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

fn destroy_focus_anchor_window(hwnd: HWND, process_id: u32) -> Result<(), String> {
    match window_process_id(hwnd) {
        0 => return Ok(()),
        owner if owner == process_id => {}
        owner => {
            return Err(format!(
                "refused to destroy focus-anchor HWND owned by process {owner}"
            ));
        }
    }
    unsafe { DestroyWindow(hwnd) }
        .map_err(|error| format!("destroy runner-owned focus anchor HWND: {error}"))?;
    if window_process_id(hwnd) != 0 {
        return Err("focus anchor HWND remained after DestroyWindow".into());
    }
    Ok(())
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
            if let Err(error) = finish_focus_anchor_ui_thread(join) {
                tracing::warn!(%error, "focus anchor UI thread cleanup was incomplete");
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
        Self::launch_with_trace_profile(
            executable,
            profile,
            log_path,
            stdout_path,
            stderr_path,
            AcceptanceTraceBudgetProfile::Standard,
        )
    }

    pub fn launch_gate_c(
        executable: &Path,
        profile: &Path,
        log_path: &Path,
        stdout_path: &Path,
        stderr_path: &Path,
    ) -> Result<Self, NativeLaunchFailure> {
        Self::launch_with_trace_profile(
            executable,
            profile,
            log_path,
            stdout_path,
            stderr_path,
            AcceptanceTraceBudgetProfile::GateC,
        )
    }

    pub fn launch_gate_d(
        executable: &Path,
        profile: &Path,
        log_path: &Path,
        stdout_path: &Path,
        stderr_path: &Path,
    ) -> Result<Self, NativeLaunchFailure> {
        Self::launch_with_trace_profile(
            executable,
            profile,
            log_path,
            stdout_path,
            stderr_path,
            AcceptanceTraceBudgetProfile::GateD,
        )
    }

    pub fn launch_gate_s(
        executable: &Path,
        profile: &Path,
        log_path: &Path,
        stdout_path: &Path,
        stderr_path: &Path,
    ) -> Result<Self, NativeLaunchFailure> {
        Self::launch_with_trace_profile(
            executable,
            profile,
            log_path,
            stdout_path,
            stderr_path,
            AcceptanceTraceBudgetProfile::GateS,
        )
    }

    fn launch_with_trace_profile(
        executable: &Path,
        profile: &Path,
        log_path: &Path,
        stdout_path: &Path,
        stderr_path: &Path,
        trace_profile: AcceptanceTraceBudgetProfile,
    ) -> Result<Self, NativeLaunchFailure> {
        let stdout = File::create(stdout_path)
            .map_err(|error| launch_failure(format!("create child stdout log: {error}")))?;
        let stderr = File::create(stderr_path)
            .map_err(|error| launch_failure(format!("create child stderr log: {error}")))?;
        let started = SystemTime::now();
        let process_information =
            create_acceptance_process(executable, profile, &stdout, &stderr, trace_profile)
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

    pub fn request_designer_repaint(&self, designer: &WindowSnapshot) -> Result<[i32; 2], String> {
        self.validate_window(designer.hwnd)?;
        if designer.role != WindowRole::Designer || designer.process_id != self.process_id {
            return Err("refused repaint for a window outside the owned Designer".into());
        }
        let current = self
            .designer()
            .filter(|current| current.hwnd == designer.hwnd)
            .ok_or_else(|| "owned Designer changed before repaint request".to_string())?;
        let before = self.client_bounds(&current)?;
        let client_size = [before[2] - before[0], before[3] - before[1]];
        if client_size[0] <= 0 || client_size[1] <= 0 || !current.visible || current.minimized {
            return Err("refused repaint for a hidden or zero-sized Designer".into());
        }
        if !unsafe { InvalidateRect(designer.hwnd, None, BOOL(0)) }.as_bool() {
            return Err("could not request an owned Designer repaint".into());
        }
        self.validate_window(designer.hwnd)?;
        let after = self.client_bounds(designer)?;
        if [after[2] - after[0], after[3] - after[1]] != client_size {
            return Err(
                "Designer client size changed while requesting a fresh canvas observation".into(),
            );
        }
        Ok(client_size)
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
    trace_profile: AcceptanceTraceBudgetProfile,
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
    let mut environment = acceptance_environment_block(profile, trace_profile);
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

fn acceptance_environment_block(
    profile: &Path,
    trace_profile: AcceptanceTraceBudgetProfile,
) -> Vec<u16> {
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
                || wide_key_eq_ascii(&wide_name, TRACE_BUDGET_PROFILE_ENV)
                || wide_key_eq_ascii(&wide_name, QUERY_OBSERVATION_ENV)
                || wide_key_eq_ascii(&wide_name, PREPARE_HOLD_ENV)
                || wide_key_eq_ascii(&wide_name, AUTHORING_SEARCH_HOLD_ENV)
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
    if let Some(value) = trace_profile.environment_value() {
        entries.push((
            TRACE_BUDGET_PROFILE_ENV.encode_utf16().collect(),
            value.encode_utf16().collect(),
        ));
    }
    entries.push((
        QUERY_OBSERVATION_ENV.encode_utf16().collect(),
        profile
            .join("radial-query-observation")
            .as_os_str()
            .encode_wide()
            .collect(),
    ));
    entries.push((
        PREPARE_HOLD_ENV.encode_utf16().collect(),
        profile
            .join(PREPARE_HOLD_FILE_NAME)
            .as_os_str()
            .encode_wide()
            .collect(),
    ));
    entries.push((
        AUTHORING_SEARCH_HOLD_ENV.encode_utf16().collect(),
        profile
            .join(AUTHORING_SEARCH_HOLD_FILE_NAME)
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
        self.release_with(
            send_owned_keyboard_release,
            verify_no_acceptance_hotkey_keys_held,
            cleanup_owned_keyboard_keys,
            keys_still_down,
        )
    }

    fn release_with(
        &mut self,
        send_release: impl FnOnce(&[INPUT], &str) -> Result<NativeInputEdgeEvidence, (usize, String)>,
        verify_release: impl FnOnce(&[OwnedKeyboardKey]) -> Result<String, String>,
        cleanup: impl FnOnce(&mut Vec<OwnedKeyboardKey>) -> String,
        physically_down: impl FnOnce(&[OwnedKeyboardKey]) -> Vec<OwnedKeyboardKey>,
    ) -> Result<NativeInputEdgeEvidence, String> {
        if self.owned.is_empty() {
            return Err("owned keyboard release was requested with no inserted key-downs".into());
        }
        let release_order = self.owned.iter().rev().copied().collect::<Vec<_>>();
        let events = release_order
            .iter()
            .copied()
            .map(OwnedKeyboardKey::up)
            .collect::<Vec<_>>();
        match send_release(&events, "owned chord key-up") {
            Ok(mut evidence) => {
                let retired = retire_inserted_keyboard_ups(
                    &mut self.owned,
                    &release_order,
                    evidence.inserted,
                );
                if !self.owned.is_empty() {
                    return Err(format!(
                        "inserted all {} release events but retired only {retired} owned keydowns; remaining={}",
                        release_order.len(),
                        describe_owned_keyboard_keys(&self.owned)
                    ));
                }
                let cleanup_status = verify_release(&release_order)?;
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
                let cleanup = cleanup(&mut self.owned);
                let held_after_cleanup = self
                    .owned
                    .iter()
                    .map(|key| format!("{:?}", key.vk))
                    .collect::<Vec<_>>();
                let physically_down = physically_down(&release_order);
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

struct OwnedModifierGuard {
    modifier: OwnedKeyboardKey,
    release_guard: OwnedKeyboardReleaseGuard,
    down_evidence: NativeInputEdgeEvidence,
}

fn owned_selection_modifier(modifier: VIRTUAL_KEY) -> Result<OwnedKeyboardKey, String> {
    let vk = match modifier {
        VK_CONTROL => VK_LCONTROL,
        VK_SHIFT => VK_LSHIFT,
        _ => return Err("selection modifier must be Control or Shift".into()),
    };
    Ok(OwnedKeyboardKey {
        vk,
        extended: false,
    })
}

impl OwnedModifierGuard {
    fn press(
        child: &NativeChild,
        target: &WindowSnapshot,
        modifier: VIRTUAL_KEY,
    ) -> Result<Self, String> {
        let modifier = owned_selection_modifier(modifier)?;
        child.validate_window(target.hwnd)?;
        if target.process_id != child.process_id() || target.role != WindowRole::Designer {
            return Err("selection modifier target is not the candidate-owned Designer".into());
        }
        child.focus_window(target)?;
        let mut release_guard = OwnedKeyboardReleaseGuard::new();
        let down_evidence = match send_validated_input_allowing_owned_keys(
            target.hwnd,
            child.process_id(),
            &[modifier.down()],
            "owned Designer selection modifier down",
            &[],
        ) {
            Ok(evidence) => {
                release_guard.owned.push(modifier);
                evidence
            }
            Err((inserted, error)) => {
                release_guard
                    .owned
                    .extend(std::iter::once(modifier).take(inserted));
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
        Ok(Self {
            modifier,
            release_guard,
            down_evidence,
        })
    }

    fn release(&mut self) -> Result<NativeInputEdgeEvidence, String> {
        // An inserted key-down owns its key-up even if focus or foreign modifiers change.
        let evidence = self.release_guard.release()?;
        input_modifiers_clear()?;
        Ok(evidence)
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
    keys.iter()
        .filter(|(_, key)| {
            (unsafe { GetAsyncKeyState(i32::from(key.0)) } < 0)
                && !modifier_is_allowed_by_owned_keys(*key, allowed)
        })
        .map(|(name, _)| *name)
        .collect::<Vec<_>>()
        .join(", ")
}

fn modifier_is_allowed_by_owned_keys(key: VIRTUAL_KEY, allowed: &[VIRTUAL_KEY]) -> bool {
    allowed.iter().any(|owned| {
        *owned == key
            || (key == VK_SHIFT && matches!(*owned, VK_LSHIFT | VK_RSHIFT))
            || (key == VK_CONTROL && matches!(*owned, VK_LCONTROL | VK_RCONTROL))
            || (key == VK_MENU && matches!(*owned, VK_LMENU | VK_RMENU))
    })
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

#[derive(Debug, PartialEq, Eq)]
pub(super) enum VisibleTextLookupError {
    TransientElementUnavailable(String),
    Other(String),
}

impl VisibleTextLookupError {
    fn into_message(self) -> String {
        match self {
            Self::TransientElementUnavailable(message) | Self::Other(message) => message,
        }
    }
}

const UIA_ELEMENT_NOT_AVAILABLE_HRESULT: u32 = 0x8004_0201;
const UIA_NOT_SUPPORTED_HRESULT: u32 = 0x8004_0200;

fn classify_element_property_error(
    error: windows::core::Error,
    property: &str,
) -> VisibleTextLookupError {
    classify_element_property_hresult(
        error.code().0 as u32,
        format!("read UIA {property}: {error}"),
    )
}

fn classify_element_property_hresult(code: u32, message: String) -> VisibleTextLookupError {
    if code == UIA_ELEMENT_NOT_AVAILABLE_HRESULT {
        VisibleTextLookupError::TransientElementUnavailable(message)
    } else {
        VisibleTextLookupError::Other(message)
    }
}

fn validate_uia_root_owner(
    root: &IUIAutomationElement,
    expected_pid: u32,
) -> Result<(), VisibleTextLookupError> {
    let process_id = unsafe { root.CurrentProcessId() }
        .map_err(|error| classify_element_property_error(error, "root process ID"))?;
    if process_id != expected_pid as i32 {
        return Err(VisibleTextLookupError::Other(format!(
            "UIA root belongs to process {process_id}, expected candidate process {expected_pid}"
        )));
    }
    Ok(())
}

fn wait_visible_text_with<T>(
    expected: &str,
    timeout: Duration,
    mut lookup: impl FnMut() -> Result<Option<T>, VisibleTextLookupError>,
) -> Result<(), String> {
    wait_uia_control_with(&format!("owned visible text {expected:?}"), timeout, || {
        lookup().map(|found| found.map(|_| ()))
    })
}

fn wait_uia_control_with<T>(
    description: &str,
    timeout: Duration,
    mut lookup: impl FnMut() -> Result<Option<T>, VisibleTextLookupError>,
) -> Result<T, String> {
    let deadline = Instant::now() + timeout;
    let mut last_transient = None;
    loop {
        match lookup() {
            Ok(Some(control)) => return Ok(control),
            Ok(None) => {}
            Err(VisibleTextLookupError::TransientElementUnavailable(error)) => {
                last_transient = Some(error);
            }
            Err(VisibleTextLookupError::Other(error)) => return Err(error),
        }
        if Instant::now() >= deadline {
            let base = format!("{description} was not exposed");
            return Err(last_transient.map_or(base.clone(), |error| {
                format!("{base}; last transient UIA property failure: {error}")
            }));
        }
        std::thread::sleep(WINDOW_POLL);
    }
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
    ProjectedCell,
    TreeSearch,
    TreeSearchClear,
    TreeSearchResult,
    BulkLabel,
    BulkSetLabel,
    DesignerBack,
    DesignerBreadcrumb,
    EditDynamicSource,
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
    InspectorDiscardAndContinue,
    InspectorCell,
    SkinRow,
    AppearanceTile,
    AppearanceApply,
    AppearanceCancel,
    SimpleAccent,
    SimpleOpacity,
    SimpleScale,
    SimpleSpacing,
    SimpleLabelSize,
    SimpleLabels,
    SimpleBold,
    SimpleShadow,
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
    pub trace_sequence: u64,
    pub bounds: [i32; 4],
    pub clip_bounds: Option<[i32; 4]>,
    pub frame_nr: Option<u64>,
    pub scroll_viewport: Option<AuthoringScrollViewportSnapshot>,
    pub text_undo: Option<TextEditUndoSnapshot>,
    pub client_size: [i32; 2],
    pub enabled: bool,
    pub selected: bool,
    pub focused: bool,
    pub clicked: bool,
    pub session_id: u64,
    pub generation: u64,
    pub menu_cell_ids_digest: Option<u64>,
    pub menu_id_digest: Option<u64>,
    pub ring_id_digest: Option<u64>,
    pub cell_id_digest: Option<u64>,
    pub authored_target_digest: Option<u64>,
    pub cell_label_digest: Option<u64>,
    pub projected_source_target_digest: Option<u64>,
    pub projected_result_index: Option<usize>,
    pub edit_source_target_digest: Option<u64>,
    pub edit_source_result_index: Option<usize>,
    pub breadcrumb_menu_id_digest: Option<u64>,
    pub ring_index: Option<usize>,
    pub slot_index: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct AuthoringScrollViewportSnapshot {
    pub measured: DesignerAuthoringScrollViewport,
    pub trace_sequence: u64,
}

fn gate_d_control_scroll_owner(
    control: &AuthoringControlSnapshot,
) -> Option<GateDControlScrollOwner> {
    use AuthoringControlRole::{Button, Checkbox, DragValue, Selectable, TextEdit};
    use AuthoringControlTarget::*;
    match (control.target, control.role) {
        (BulkLabel, TextEdit) | (BulkSetLabel | EditDynamicSource, Button) => {
            Some(GateDControlScrollOwner::Inspector)
        }
        (TreeSearch, TextEdit) | (MenuRow | TreeSearchResult, Selectable) => {
            Some(GateDControlScrollOwner::MenuTree)
        }
        (AppearanceTile, Selectable)
        | (AppearanceApply | AppearanceCancel | SimpleAccent, Button)
        | (SimpleOpacity | SimpleScale | SimpleSpacing | SimpleLabelSize, Button | DragValue)
        | (SimpleLabels | SimpleBold | SimpleShadow, Checkbox) => {
            Some(GateDControlScrollOwner::Resources)
        }
        // Toolbar, canvas, popup, and nested Advanced editors are distinct owners.
        _ => None,
    }
}

fn authoring_scroll_viewport_matches(
    control: &AuthoringControlSnapshot,
    viewport: &AuthoringScrollViewportSnapshot,
) -> bool {
    let measured = viewport.measured;
    measured.is_valid()
        && gate_d_control_scroll_owner(control) == Some(measured.owner)
        && control.frame_nr == Some(measured.frame_nr)
        && control.session_id == measured.session_id
        && control.generation == measured.generation
        && control.client_size == measured.client_size
        && control.clip_bounds == Some(measured.paint_clip_bounds)
        && viewport.trace_sequence > control.trace_sequence
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TextEditUndoSnapshot {
    pub field_id_digest: u64,
    pub value_digest: u64,
    pub in_flux: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ActionEditorTraceIdentity {
    pub surface: ActionEditorSurface,
    pub session_id: u64,
    pub draft_generation: u64,
    pub stable_target_digest: u64,
    pub editor_epoch: u64,
    pub edit_generation: u64,
    pub query_generation: u64,
    pub query_request_generation: u64,
    pub search_request_generation: u64,
    pub test_request_generation: u64,
    pub query_digest: u64,
    pub assigned_binding_digest: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ActionEditorSurface {
    Properties,
    Inspector,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ActionEditorControlSnapshot {
    pub identity: ActionEditorTraceIdentity,
    pub control: String,
    pub index: Option<usize>,
    pub target_digest: u64,
    pub title_digest: u64,
    pub type_digest: u64,
    pub disambiguator_digest: u64,
    pub action_digest: u64,
    pub binding_digest: u64,
    pub value_digest: u64,
    pub displayed_text_digest: u64,
    pub trace_sequence: u64,
    pub bounds: [i32; 4],
    pub full_bounds: [i32; 4],
    pub client_size: [i32; 2],
    pub fully_visible: bool,
    pub enabled: bool,
    pub selected: bool,
    pub focused: bool,
    pub clicked: bool,
    pub changed: bool,
    pub enter_pressed: bool,
    pub visible: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ActionEditorScrollSnapshot {
    pub identity: ActionEditorTraceIdentity,
    pub scroll_id: u64,
    pub frame_nr: u64,
    pub trace_sequence: u64,
    pub offset_y_milli: i64,
    pub velocity_y_milli: i64,
    pub content_height_milli: i64,
    pub inner_height_milli: i64,
    pub pixels_per_point_milli: i64,
    pub handle_min_length_milli: i64,
    pub inner_bounds: [i32; 4],
    pub inner_visible_bounds: [i32; 4],
    pub track_bounds: [i32; 4],
    pub track_visible_bounds: [i32; 4],
    pub thumb_bounds: [i32; 4],
    pub thumb_visible_bounds: [i32; 4],
    pub painted_thumb_bounds: [i32; 4],
    pub painted_thumb_visible_bounds: [i32; 4],
    pub paint_clip_bounds: [i32; 4],
    pub client_size: [i32; 2],
}

impl ActionEditorScrollSnapshot {
    fn is_well_formed(&self) -> bool {
        let positive = |rect: [i32; 4]| rect[2] > rect[0] && rect[3] > rect[1];
        let contains = |outer: [i32; 4], inner: [i32; 4]| {
            outer[0] <= inner[0]
                && outer[1] <= inner[1]
                && outer[2] >= inner[2]
                && outer[3] >= inner[3]
        };
        let client = [0, 0, self.client_size[0], self.client_size[1]];
        if self.identity.session_id == 0
            || self.identity.stable_target_digest == 0
            || self.identity.editor_epoch == 0
            || self.scroll_id == 0
            || self.trace_sequence == 0
            || self.client_size[0] <= 0
            || self.client_size[1] <= 0
            || self.content_height_milli <= self.inner_height_milli
            || self.inner_height_milli <= 0
            || self.pixels_per_point_milli <= 0
            || self.handle_min_length_milli <= 0
            || self.offset_y_milli < 0
            || self.offset_y_milli > self.content_height_milli - self.inner_height_milli
            || !positive(self.inner_bounds)
            || !positive(self.inner_visible_bounds)
            || !positive(self.track_bounds)
            || !positive(self.track_visible_bounds)
            || !positive(self.thumb_bounds)
            || !positive(self.thumb_visible_bounds)
            || !positive(self.painted_thumb_bounds)
            || !positive(self.painted_thumb_visible_bounds)
            || !positive(self.paint_clip_bounds)
            || !contains(self.inner_bounds, self.inner_visible_bounds)
            || !contains(self.track_bounds, self.track_visible_bounds)
            || !contains(self.thumb_bounds, self.thumb_visible_bounds)
            || !contains(self.track_visible_bounds, self.thumb_visible_bounds)
            || !contains(self.painted_thumb_bounds, self.painted_thumb_visible_bounds)
            || !contains(client, self.inner_visible_bounds)
            || !contains(client, self.track_visible_bounds)
            || !contains(client, self.thumb_visible_bounds)
            || !contains(client, self.painted_thumb_visible_bounds)
            || !contains(client, self.paint_clip_bounds)
            || !contains(self.paint_clip_bounds, self.painted_thumb_visible_bounds)
            || self.track_bounds[1] != self.inner_bounds[1]
            || self.track_bounds[3] != self.inner_bounds[3]
            || self.thumb_bounds[0] != self.track_bounds[0]
            || self.thumb_bounds[2] != self.track_bounds[2]
            || self.painted_thumb_bounds[0] != self.track_bounds[0]
            || self.painted_thumb_bounds[2] != self.track_bounds[2]
        {
            return false;
        }
        let inner_height_px = i128::from(self.inner_bounds[3] - self.inner_bounds[1]);
        let content_height = i128::from(self.content_height_milli);
        let expected_top = i128::from(self.inner_bounds[1])
            + (i128::from(self.offset_y_milli) * inner_height_px + content_height / 2)
                / content_height;
        let expected_bottom = i128::from(self.inner_bounds[1])
            + ((i128::from(self.offset_y_milli) + i128::from(self.inner_height_milli))
                * inner_height_px
                + content_height / 2)
                / content_height;
        let raw_height = i128::from(self.thumb_bounds[3] - self.thumb_bounds[1]);
        let min_height_px = (i128::from(self.handle_min_length_milli)
            * i128::from(self.pixels_per_point_milli)
            + 500_000)
            / 1_000_000;
        let expected_painted_height = raw_height.max(min_height_px);
        let painted_height =
            i128::from(self.painted_thumb_bounds[3] - self.painted_thumb_bounds[1]);
        let expected_painted_visible = [
            self.painted_thumb_bounds[0].max(self.paint_clip_bounds[0]),
            self.painted_thumb_bounds[1].max(self.paint_clip_bounds[1]),
            self.painted_thumb_bounds[2].min(self.paint_clip_bounds[2]),
            self.painted_thumb_bounds[3].min(self.paint_clip_bounds[3]),
        ];
        (i128::from(self.thumb_bounds[1]) - expected_top).abs() <= 2
            && (i128::from(self.thumb_bounds[3]) - expected_bottom).abs() <= 2
            && (i128::from(self.painted_thumb_bounds[1]) + i128::from(self.painted_thumb_bounds[3])
                - i128::from(self.thumb_bounds[1])
                - i128::from(self.thumb_bounds[3]))
            .abs()
                <= 2
            && (painted_height - expected_painted_height).abs() <= 2
            && self.painted_thumb_visible_bounds == expected_painted_visible
    }

    fn same_scroll_owner(&self, other: &Self) -> bool {
        self.identity == other.identity
            && self.scroll_id == other.scroll_id
            && self.client_size == other.client_size
            && self.content_height_milli == other.content_height_milli
            && self.inner_height_milli == other.inner_height_milli
            && self.pixels_per_point_milli == other.pixels_per_point_milli
            && self.handle_min_length_milli == other.handle_min_length_milli
            && self.inner_bounds == other.inner_bounds
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct InspectorCellTextEditSnapshot {
    pub target_digest: u64,
    pub session_id: u64,
    pub generation: u64,
    pub value_digest: u64,
    pub trace_sequence: u64,
    pub bounds: [i32; 4],
    pub clip_bounds: [i32; 4],
    pub client_size: [i32; 2],
    pub visible: bool,
    pub fully_visible: bool,
    pub focused: bool,
    pub clicked: bool,
    pub changed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ActionEditorProviderTraceSnapshot {
    pub identity: ActionEditorTraceIdentity,
    pub edge: ActionEditorProviderEdge,
    pub kind: ActionEditorProviderKind,
    pub query_digest: u64,
    pub binding_digest: u64,
    pub provider_revision: Option<u64>,
    pub trace_sequence: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ActionEditorProviderEdge {
    Queued,
    WorkerStarted,
    WorkerCompleted,
    WorkerFailed,
    Applied,
    Rejected,
    Retired,
    Cancelled,
    RetryQueued,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ActionEditorProviderKind {
    Search,
    Test,
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

    /// Capture private, bounded structure/value diagnostics for a single
    /// candidate-owned HWND. Callers must retain this only as a private case
    /// artifact; raw UIA text is intentionally excluded from the public report.
    pub(super) fn describe_candidate_private_tree(
        &self,
        hwnd: HWND,
        expected_pid: u32,
    ) -> Result<String, String> {
        let root = unsafe { self.automation.ElementFromHandle(hwnd) }
            .map_err(|error| format!("query candidate UIA root: {error}"))?;
        let root_pid = unsafe { root.CurrentProcessId() }
            .map_err(|error| format!("read candidate UIA root process: {error}"))?;
        if root_pid != expected_pid as i32 {
            return Err("private UIA diagnostic root is not owned by the candidate".into());
        }
        let walker = unsafe { self.automation.ControlViewWalker() }
            .map_err(|error| format!("create candidate UIA diagnostic walker: {error}"))?;
        let mut snapshot = String::from("private candidate-owned UIA control tree\n");
        let mut visited = 0usize;
        append_candidate_private_uia_tree(
            &walker,
            &root,
            expected_pid,
            1,
            &mut visited,
            &mut snapshot,
        );
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

    /// Return one current, visible, candidate-owned text control whose name contains
    /// the requested fixture label. Duplicate UIA descendants with the same bounds
    /// are aliases for one rendered control; distinct bounds are ambiguous.
    pub fn find_visible_text_containing(
        &self,
        hwnd: HWND,
        expected_pid: u32,
        name_fragment: &str,
    ) -> Result<Option<SemanticControl>, VisibleTextLookupError> {
        self.find_visible_named_containing_classified(hwnd, expected_pid, name_fragment, None)
    }

    /// Return one current, visible Button whose name contains the requested
    /// fixture label. This is intended for unique fixture rows and never picks
    /// one of several separately rendered matching buttons.
    pub fn find_visible_button_containing(
        &self,
        hwnd: HWND,
        expected_pid: u32,
        name_fragment: &str,
    ) -> Result<Option<SemanticControl>, String> {
        self.find_visible_named_containing(
            hwnd,
            expected_pid,
            name_fragment,
            Some(UIA_ButtonControlTypeId),
        )
    }

    pub fn find_visible_combo_box_containing(
        &self,
        hwnd: HWND,
        expected_pid: u32,
        name_fragment: &str,
    ) -> Result<Option<SemanticControl>, String> {
        self.find_visible_named_containing(
            hwnd,
            expected_pid,
            name_fragment,
            Some(UIA_ComboBoxControlTypeId),
        )
    }

    pub fn find_visible_list_item_containing(
        &self,
        hwnd: HWND,
        expected_pid: u32,
        name_fragment: &str,
    ) -> Result<Option<SemanticControl>, String> {
        self.find_visible_named_containing(
            hwnd,
            expected_pid,
            name_fragment,
            Some(UIA_ListItemControlTypeId),
        )
    }

    pub fn wait_visible_button_containing(
        &self,
        hwnd: HWND,
        expected_pid: u32,
        name_fragment: &str,
        timeout: Duration,
    ) -> Result<SemanticControl, String> {
        self.wait_visible_named_containing_type(
            hwnd,
            expected_pid,
            name_fragment,
            Some(UIA_ButtonControlTypeId),
            timeout,
        )
    }

    pub fn wait_visible_combo_box_containing(
        &self,
        hwnd: HWND,
        expected_pid: u32,
        name_fragment: &str,
        timeout: Duration,
    ) -> Result<SemanticControl, String> {
        self.wait_visible_named_containing_type(
            hwnd,
            expected_pid,
            name_fragment,
            Some(UIA_ComboBoxControlTypeId),
            timeout,
        )
    }

    pub fn wait_visible_list_item_containing(
        &self,
        hwnd: HWND,
        expected_pid: u32,
        name_fragment: &str,
        timeout: Duration,
    ) -> Result<SemanticControl, String> {
        self.wait_visible_named_containing_type(
            hwnd,
            expected_pid,
            name_fragment,
            Some(UIA_ListItemControlTypeId),
            timeout,
        )
    }

    fn wait_visible_named_containing_type(
        &self,
        hwnd: HWND,
        expected_pid: u32,
        name_fragment: &str,
        control_type: Option<UIA_CONTROLTYPE_ID>,
        timeout: Duration,
    ) -> Result<SemanticControl, String> {
        let description = format!("owned visible UIA control {name_fragment:?}");
        wait_uia_control_with(&description, timeout, || {
            self.find_visible_named_containing_classified(
                hwnd,
                expected_pid,
                name_fragment,
                control_type,
            )
        })
    }

    fn find_visible_named_containing(
        &self,
        hwnd: HWND,
        expected_pid: u32,
        name_fragment: &str,
        required_control_type: Option<UIA_CONTROLTYPE_ID>,
    ) -> Result<Option<SemanticControl>, String> {
        self.find_visible_named_containing_classified(
            hwnd,
            expected_pid,
            name_fragment,
            required_control_type,
        )
        .map_err(VisibleTextLookupError::into_message)
    }

    fn find_visible_named_containing_classified(
        &self,
        hwnd: HWND,
        expected_pid: u32,
        name_fragment: &str,
        required_control_type: Option<UIA_CONTROLTYPE_ID>,
    ) -> Result<Option<SemanticControl>, VisibleTextLookupError> {
        let root = unsafe { self.automation.ElementFromHandle(hwnd) }
            .map_err(|error| classify_element_property_error(error, "visible-name root"))?;
        validate_uia_root_owner(&root, expected_pid)?;
        let condition = unsafe { self.automation.CreateTrueCondition() }.map_err(|error| {
            VisibleTextLookupError::Other(format!("create visible-name UIA condition: {error}"))
        })?;
        let matches = unsafe { root.FindAll(TreeScope_Descendants, &condition) }
            .map_err(|error| classify_element_property_error(error, "visible-name descendants"))?;
        let count = unsafe { matches.Length() }
            .map_err(|error| classify_element_property_error(error, "visible-name count"))?
            .min(2_048);
        let expected = name_fragment.to_lowercase();
        let mut found: Option<SemanticControl> = None;
        for index in 0..count {
            let element = unsafe { matches.GetElement(index) }
                .map_err(|error| classify_element_property_error(error, "visible-name element"))?;
            let process_id = unsafe { element.CurrentProcessId() }
                .map_err(|error| classify_element_property_error(error, "process ID"))?;
            if process_id != expected_pid as i32 {
                continue;
            }
            let name = unsafe { element.CurrentName() }
                .map_err(|error| classify_element_property_error(error, "name"))?
                .to_string();
            if !semantic_name_contains(&name, &expected) {
                continue;
            }
            if unsafe { element.CurrentIsOffscreen() }
                .map_err(|error| classify_element_property_error(error, "offscreen state"))?
                .as_bool()
            {
                continue;
            }
            let control_type = unsafe { element.CurrentControlType() }
                .map_err(|error| classify_element_property_error(error, "control type"))?;
            let enabled = unsafe { element.CurrentIsEnabled() }
                .map_err(|error| classify_element_property_error(error, "enabled state"))?
                .as_bool();
            if required_control_type.is_some_and(|expected| expected != control_type)
                || (required_control_type == Some(UIA_ButtonControlTypeId) && !enabled)
            {
                continue;
            }
            let bounds = unsafe { element.CurrentBoundingRectangle() }
                .map_err(|error| classify_element_property_error(error, "bounding rectangle"))?;
            let bounds = [bounds.left, bounds.top, bounds.right, bounds.bottom];
            if bounds[2] <= bounds[0] || bounds[3] <= bounds[1] {
                continue;
            }
            let candidate = SemanticControl {
                element,
                bounds,
                process_id: expected_pid,
                enabled,
            };
            if found
                .as_ref()
                .is_some_and(|previous| previous.bounds != candidate.bounds)
            {
                return Err(VisibleTextLookupError::Other(format!(
                    "visible UIA label fragment {name_fragment:?} matched multiple distinct controls"
                )));
            }
            found = Some(candidate);
        }
        Ok(found)
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

    /// Read-only text assertions may match both an inline label and its toast.
    pub fn wait_visible_text(
        &self,
        hwnd: HWND,
        expected_pid: u32,
        expected: &str,
        timeout: Duration,
    ) -> Result<(), String> {
        wait_visible_text_with(expected, timeout, || {
            self.find_visible_named_classified(hwnd, expected_pid, expected, false)
        })
    }

    pub fn find_visible_button(
        &self,
        hwnd: HWND,
        expected_pid: u32,
        expected: &str,
    ) -> Result<Option<SemanticControl>, String> {
        self.find_visible_named(hwnd, expected_pid, expected, true)
    }

    fn find_visible_named(
        &self,
        hwnd: HWND,
        expected_pid: u32,
        expected: &str,
        require_button: bool,
    ) -> Result<Option<SemanticControl>, String> {
        self.find_visible_named_classified(hwnd, expected_pid, expected, require_button)
            .map_err(VisibleTextLookupError::into_message)
    }

    fn find_visible_named_classified(
        &self,
        hwnd: HWND,
        expected_pid: u32,
        expected: &str,
        require_button: bool,
    ) -> Result<Option<SemanticControl>, VisibleTextLookupError> {
        let root = unsafe { self.automation.ElementFromHandle(hwnd) }
            .map_err(|error| classify_element_property_error(error, "semantic root"))?;
        validate_uia_root_owner(&root, expected_pid)?;
        let condition = unsafe {
            self.automation
                .CreatePropertyCondition(UIA_NamePropertyId, &VARIANT::from(expected))
        }
        .map_err(|error| classify_element_property_error(error, "visible name condition"))?;
        let matches = unsafe { root.FindAll(TreeScope_Descendants, &condition) }
            .map_err(|error| classify_element_property_error(error, "visible name descendants"))?;
        let count = unsafe { matches.Length() }
            .map_err(|error| classify_element_property_error(error, "visible name count"))?
            .min(2_048);
        let mut found: Option<SemanticControl> = None;
        for index in 0..count {
            let element = unsafe { matches.GetElement(index) }
                .map_err(|error| classify_element_property_error(error, "visible name element"))?;
            let process_id = unsafe { element.CurrentProcessId() }
                .map_err(|error| classify_element_property_error(error, "process ID"))?;
            let name = unsafe { element.CurrentName() }
                .map_err(|error| classify_element_property_error(error, "name"))?
                .to_string();
            let offscreen = unsafe { element.CurrentIsOffscreen() }
                .map_err(|error| classify_element_property_error(error, "offscreen state"))?
                .as_bool();
            let enabled = unsafe { element.CurrentIsEnabled() }
                .map_err(|error| classify_element_property_error(error, "enabled state"))?
                .as_bool();
            let bounds = unsafe { element.CurrentBoundingRectangle() }
                .map_err(|error| classify_element_property_error(error, "bounding rectangle"))?;
            let bounds = [bounds.left, bounds.top, bounds.right, bounds.bottom];
            if !owned_visible_name_matches(
                &name,
                expected,
                process_id,
                expected_pid,
                offscreen,
                bounds,
            ) || (require_button
                && (!enabled
                    || unsafe { element.CurrentControlType() }
                        .map_err(|error| classify_element_property_error(error, "button type"))?
                        != windows::Win32::UI::Accessibility::UIA_ButtonControlTypeId))
            {
                continue;
            }
            let candidate = SemanticControl {
                element,
                bounds,
                process_id: expected_pid,
                enabled,
            };
            if require_button && found.as_ref().is_some_and(|prior| prior.bounds != bounds) {
                return Err(VisibleTextLookupError::Other(format!(
                    "multiple distinct visible buttons named {expected:?}"
                )));
            }
            found = Some(candidate);
        }
        Ok(found)
    }

    pub fn edit_value_matches(
        &self,
        control: &SemanticControl,
        expected: &str,
    ) -> Result<bool, String> {
        self.edit_value_matches_classified(control, expected)
            .map_err(VisibleTextLookupError::into_message)
    }

    pub(super) fn edit_value_matches_classified(
        &self,
        control: &SemanticControl,
        expected: &str,
    ) -> Result<bool, VisibleTextLookupError> {
        let pattern = unsafe {
            control
                .element
                .GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
        }
        .map_err(|error| classify_element_property_error(error, "edit value pattern"))?;
        let value = unsafe { pattern.CurrentValue() }
            .map_err(|error| classify_element_property_error(error, "edit value"))?;
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
        self.find_first_edit_classified(hwnd, expected_pid)
            .map_err(VisibleTextLookupError::into_message)
    }

    pub fn wait_first_edit(
        &self,
        hwnd: HWND,
        expected_pid: u32,
        timeout: Duration,
    ) -> Result<SemanticControl, String> {
        wait_uia_control_with("owned UIA Edit", timeout, || {
            self.find_first_edit_classified(hwnd, expected_pid)
        })
    }

    fn find_first_edit_classified(
        &self,
        hwnd: HWND,
        expected_pid: u32,
    ) -> Result<Option<SemanticControl>, VisibleTextLookupError> {
        let root = unsafe { self.automation.ElementFromHandle(hwnd) }
            .map_err(|error| classify_element_property_error(error, "first edit root"))?;
        validate_uia_root_owner(&root, expected_pid)?;
        let condition = unsafe {
            self.automation.CreatePropertyCondition(
                windows::Win32::UI::Accessibility::UIA_ControlTypePropertyId,
                &VARIANT::from(UIA_EditControlTypeId.0),
            )
        }
        .map_err(|error| classify_element_property_error(error, "first edit condition"))?;
        let matches = unsafe { root.FindAll(TreeScope_Descendants, &condition) }
            .map_err(|error| classify_element_property_error(error, "first edit controls"))?;
        let count = unsafe { matches.Length() }
            .map_err(|error| classify_element_property_error(error, "first edit count"))?
            .min(2_048);
        for index in 0..count {
            let element = unsafe { matches.GetElement(index) }
                .map_err(|error| classify_element_property_error(error, "first edit element"))?;
            let process_id = unsafe { element.CurrentProcessId() }
                .map_err(|error| classify_element_property_error(error, "first edit process ID"))?;
            if process_id != expected_pid as i32 {
                continue;
            }
            let bounds = unsafe { element.CurrentBoundingRectangle() }
                .map_err(|error| classify_element_property_error(error, "first edit bounds"))?;
            let enabled = unsafe { element.CurrentIsEnabled() }
                .map_err(|error| {
                    classify_element_property_error(error, "first edit enabled state")
                })?
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

    pub fn find_edit_with_value_fragment(
        &self,
        hwnd: HWND,
        expected_pid: u32,
        fragment: &str,
    ) -> Result<Option<SemanticControl>, String> {
        self.find_edit_with_value_fragment_classified(hwnd, expected_pid, fragment)
            .map_err(VisibleTextLookupError::into_message)
    }

    pub(super) fn find_edit_with_value_fragment_classified(
        &self,
        hwnd: HWND,
        expected_pid: u32,
        fragment: &str,
    ) -> Result<Option<SemanticControl>, VisibleTextLookupError> {
        let root = unsafe { self.automation.ElementFromHandle(hwnd) }
            .map_err(|error| classify_element_property_error(error, "edit root"))?;
        validate_uia_root_owner(&root, expected_pid)?;
        let condition = unsafe {
            self.automation.CreatePropertyCondition(
                windows::Win32::UI::Accessibility::UIA_ControlTypePropertyId,
                &VARIANT::from(UIA_EditControlTypeId.0),
            )
        }
        .map_err(|error| {
            VisibleTextLookupError::Other(format!("create UIA edit condition: {error}"))
        })?;
        let matches = unsafe { root.FindAll(TreeScope_Descendants, &condition) }
            .map_err(|error| classify_element_property_error(error, "edit controls"))?;
        let count = unsafe { matches.Length() }
            .map_err(|error| classify_element_property_error(error, "edit-control count"))?
            .min(2_048);
        let mut found = None;
        for index in 0..count {
            let element = unsafe { matches.GetElement(index) }
                .map_err(|error| classify_element_property_error(error, "edit element"))?;
            let process_id = unsafe { element.CurrentProcessId() }
                .map_err(|error| classify_element_property_error(error, "edit process ID"))?;
            if process_id != expected_pid as i32 {
                continue;
            }
            let value_pattern = match unsafe {
                element.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
            } {
                Ok(pattern) => pattern,
                Err(error) if error.code().0 as u32 == UIA_NOT_SUPPORTED_HRESULT => continue,
                Err(error) => {
                    return Err(classify_element_property_error(error, "edit value pattern"));
                }
            };
            let value = unsafe { value_pattern.CurrentValue() }
                .map_err(|error| classify_element_property_error(error, "edit value"))?
                .to_string();
            if !value.contains(fragment) {
                continue;
            }
            let bounds = unsafe { element.CurrentBoundingRectangle() }
                .map_err(|error| classify_element_property_error(error, "edit bounds"))?;
            let offscreen = unsafe { element.CurrentIsOffscreen() }
                .map_err(|error| classify_element_property_error(error, "edit offscreen state"))?
                .as_bool();
            let enabled = unsafe { element.CurrentIsEnabled() }
                .map_err(|error| classify_element_property_error(error, "edit enabled state"))?
                .as_bool();
            if offscreen || !enabled || bounds.right <= bounds.left || bounds.bottom <= bounds.top {
                continue;
            }
            let candidate = SemanticControl {
                element,
                bounds: [bounds.left, bounds.top, bounds.right, bounds.bottom],
                process_id: expected_pid,
                enabled,
            };
            if found.is_some() {
                return Err(VisibleTextLookupError::Other(
                    "UIA value fragment matched multiple owned edit controls".into(),
                ));
            }
            found = Some(candidate);
        }
        Ok(found)
    }

    pub fn control_has_focus(&self, control: &SemanticControl) -> bool {
        self.element_has_focus(&control.element)
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

fn owned_visible_name_matches(
    name: &str,
    expected: &str,
    process_id: i32,
    expected_pid: u32,
    offscreen: bool,
    bounds: [i32; 4],
) -> bool {
    name == expected
        && process_id == expected_pid as i32
        && !offscreen
        && bounds[2] > bounds[0]
        && bounds[3] > bounds[1]
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

fn append_candidate_private_uia_tree(
    walker: &IUIAutomationTreeWalker,
    parent: &IUIAutomationElement,
    expected_pid: u32,
    depth: usize,
    visited: &mut usize,
    snapshot: &mut String,
) {
    const MAX_PRIVATE_UIA_NODES: usize = 128;
    const MAX_PRIVATE_UIA_DEPTH: usize = 8;
    const MAX_PRIVATE_UIA_BYTES: usize = 32 * 1024;
    if *visited >= MAX_PRIVATE_UIA_NODES || depth > MAX_PRIVATE_UIA_DEPTH {
        return;
    }
    let Ok(mut element) = (unsafe { walker.GetFirstChildElement(parent) }) else {
        return;
    };
    loop {
        if *visited >= MAX_PRIVATE_UIA_NODES || snapshot.len() >= MAX_PRIVATE_UIA_BYTES {
            break;
        }
        *visited += 1;
        let process_id = unsafe { element.CurrentProcessId() }
            .map(|value| value as u32)
            .unwrap_or_default();
        if process_id == expected_pid {
            let control_type = unsafe { element.CurrentControlType() }
                .map(|value| value.0)
                .unwrap_or_default();
            let bounds = unsafe { element.CurrentBoundingRectangle() }
                .map(|value| [value.left, value.top, value.right, value.bottom])
                .unwrap_or_default();
            let offscreen = unsafe { element.CurrentIsOffscreen() }
                .map(|value| value.as_bool())
                .unwrap_or(true);
            let name = unsafe { element.CurrentName() }
                .map(|value| value.to_string())
                .unwrap_or_else(|error| format!("<uia-error-{:08x}>", error.code().0 as u32));
            let value = match unsafe {
                element.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
            } {
                Ok(pattern) => unsafe { pattern.CurrentValue() }
                    .map(|value| value.to_string())
                    .unwrap_or_else(|error| format!("<uia-error-{:08x}>", error.code().0 as u32)),
                Err(error) if error.code().0 as u32 == UIA_NOT_SUPPORTED_HRESULT => {
                    "<no-value-pattern>".to_owned()
                }
                Err(error) => format!("<uia-error-{:08x}>", error.code().0 as u32),
            };
            let indent = "  ".repeat(depth);
            let line = format!(
                "{indent}pid={process_id} type={control_type} bounds={bounds:?} offscreen={offscreen} name={:?} value={:?}\n",
                bounded_private_uia_field(&name),
                bounded_private_uia_field(&value),
            );
            if snapshot.len().saturating_add(line.len()) <= MAX_PRIVATE_UIA_BYTES {
                snapshot.push_str(&line);
            } else {
                const CAP_MARKER: &str = "<diagnostic byte cap reached>\n";
                let remaining = MAX_PRIVATE_UIA_BYTES.saturating_sub(snapshot.len());
                snapshot.push_str(&CAP_MARKER[..remaining.min(CAP_MARKER.len())]);
                break;
            }
            append_candidate_private_uia_tree(
                walker,
                &element,
                expected_pid,
                depth + 1,
                visited,
                snapshot,
            );
        }
        if *visited >= MAX_PRIVATE_UIA_NODES || snapshot.len() >= MAX_PRIVATE_UIA_BYTES {
            break;
        }
        let Ok(next) = (unsafe { walker.GetNextSiblingElement(&element) }) else {
            break;
        };
        element = next;
    }
}

fn bounded_private_uia_field(value: &str) -> String {
    value
        .chars()
        .take(160)
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect()
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
    click_semantic_control_with_button(child, target, control, trace_path, PointerButton::Left)
}

pub(super) fn click_semantic_control_secondary(
    child: &NativeChild,
    target: &WindowSnapshot,
    control: &SemanticControl,
    trace_path: &Path,
) -> Result<PointerClickEvidence, String> {
    click_semantic_control_with_button(child, target, control, trace_path, PointerButton::Right)
}

pub(super) fn click_designer_semantic_control(
    child: &NativeChild,
    target: &WindowSnapshot,
    control: &SemanticControl,
    trace_path: &Path,
) -> Result<PointerClickEvidence, String> {
    child.validate_window(target.hwnd)?;
    if target.role != WindowRole::Designer
        || target.process_id != child.process_id()
        || control.process_id != child.process_id()
        || !control.enabled
        || control.bounds[2] <= control.bounds[0]
        || control.bounds[3] <= control.bounds[1]
    {
        return Err("refused Designer click for a disabled, invalid, or foreign control".into());
    }
    let mut top_left = POINT {
        x: control.bounds[0],
        y: control.bounds[1],
    };
    let mut bottom_right = POINT {
        x: control.bounds[2],
        y: control.bounds[3],
    };
    if !unsafe { ScreenToClient(target.hwnd, &mut top_left) }.as_bool()
        || !unsafe { ScreenToClient(target.hwnd, &mut bottom_right) }.as_bool()
    {
        return Err("could not convert Designer UIA screen bounds to client coordinates".into());
    }
    click_designer_client_bounds(
        child,
        target,
        [top_left.x, top_left.y, bottom_right.x, bottom_right.y],
        trace_path,
    )
}

fn click_semantic_control_with_button(
    child: &NativeChild,
    target: &WindowSnapshot,
    control: &SemanticControl,
    trace_path: &Path,
    button: PointerButton,
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
        button,
        "semantic UIA control click",
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
    click_designer_client_bounds_with_button(child, target, bounds, trace_path, PointerButton::Left)
}

pub(super) fn click_designer_client_bounds_with_modifier(
    child: &NativeChild,
    target: &WindowSnapshot,
    bounds: [i32; 4],
    trace_path: &Path,
    modifier: VIRTUAL_KEY,
) -> Result<
    (
        PointerClickEvidence,
        NativeInputEdgeEvidence,
        NativeInputEdgeEvidence,
    ),
    String,
> {
    let mut modifier_guard = OwnedModifierGuard::press(child, target, modifier)?;
    let down = modifier_guard.down_evidence.clone();
    let owned_vk = modifier_guard.modifier.vk;
    let (click, up) = finish_designer_click_with_modifier_release(
        || {
            click_designer_client_bounds_with_button_and_owned_keys(
                child,
                target,
                bounds,
                trace_path,
                PointerButton::Left,
                &[owned_vk],
            )
            .map_err(PointerClickPreDownError::into_message)
        },
        || modifier_guard.release(),
    )?;
    Ok((click, down, up))
}

fn finish_designer_click_with_modifier_release<T>(
    click: impl FnOnce() -> Result<T, String>,
    release: impl FnOnce() -> Result<NativeInputEdgeEvidence, String>,
) -> Result<(T, NativeInputEdgeEvidence), String> {
    let click_result = click();
    let up_result = release();
    match (click_result, up_result) {
        (Ok(click), Ok(up)) => Ok((click, up)),
        (Err(error), Ok(_)) => Err(format!("{error}; owned selection modifier released")),
        (Ok(_), Err(error)) => Err(format!(
            "Designer pointer click completed but owned selection modifier cleanup failed: {error}"
        )),
        (Err(click_error), Err(release_error)) => Err(format!(
            "{click_error}; owned selection modifier cleanup failed: {release_error}"
        )),
    }
}

pub(super) fn click_designer_secondary_bounds(
    child: &NativeChild,
    target: &WindowSnapshot,
    bounds: [i32; 4],
    trace_path: &Path,
) -> Result<PointerClickEvidence, String> {
    click_designer_client_bounds_with_button(
        child,
        target,
        bounds,
        trace_path,
        PointerButton::Right,
    )
}

pub(super) fn scroll_designer_client_bounds(
    child: &NativeChild,
    target: &WindowSnapshot,
    bounds: [i32; 4],
    delta: i16,
) -> Result<usize, String> {
    child.validate_window(target.hwnd)?;
    if target.role != WindowRole::Designer
        || target.process_id != child.process_id()
        || !target.visible
        || target.minimized
        || bounds[2] <= bounds[0]
        || bounds[3] <= bounds[1]
        || delta == 0
    {
        return Err("refused scroll outside a visible candidate-owned Designer control".into());
    }
    let mut client = RECT::default();
    unsafe { GetClientRect(target.hwnd, &mut client) }
        .map_err(|error| format!("read Designer client bounds before scroll: {error}"))?;
    let point = semantic_client_center(
        bounds,
        [client.left, client.top, client.right, client.bottom],
    )?;
    let mut screen_point = point;
    if !unsafe { ClientToScreen(target.hwnd, &mut screen_point) }.as_bool() {
        return Err("could not convert Designer scroll point to screen coordinates".into());
    }
    child.focus_window(target)?;
    unsafe { SetCursorPos(screen_point.x, screen_point.y) }
        .map_err(|error| format!("position pointer over owned Designer result list: {error}"))?;
    let hit = unsafe { WindowFromPoint(screen_point) };
    if hit.is_invalid() || window_process_id(hit) != child.process_id() {
        return Err(
            "refused scroll because the hit-tested window is not owned by the candidate".into(),
        );
    }
    input_modifiers_clear()?;
    send_input_checked(
        &[INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: 0,
                    dy: 0,
                    mouseData: i32::from(delta) as u32,
                    dwFlags: MOUSEEVENTF_WHEEL,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }],
        "owned Designer result-list scroll",
    )
}

fn click_designer_client_bounds_with_button(
    child: &NativeChild,
    target: &WindowSnapshot,
    bounds: [i32; 4],
    trace_path: &Path,
    button: PointerButton,
) -> Result<PointerClickEvidence, String> {
    click_designer_client_bounds_with_button_and_owned_keys(
        child,
        target,
        bounds,
        trace_path,
        button,
        &[],
    )
    .map_err(PointerClickPreDownError::into_message)
}

fn click_designer_client_bounds_with_button_and_owned_keys(
    child: &NativeChild,
    target: &WindowSnapshot,
    bounds: [i32; 4],
    trace_path: &Path,
    button: PointerButton,
    owned_keys: &[VIRTUAL_KEY],
) -> Result<PointerClickEvidence, PointerClickPreDownError> {
    click_designer_client_bounds_with_pre_down_check_and_owned_keys(
        child,
        target,
        bounds,
        trace_path,
        button,
        owned_keys,
        |_, _| Ok(()),
    )
}

pub(super) fn click_designer_client_bounds_with_pre_down_check(
    child: &NativeChild,
    target: &WindowSnapshot,
    bounds: [i32; 4],
    trace_path: &Path,
    pre_down_check: impl FnOnce((i32, i32), usize) -> Result<(), PointerClickPreDownError>,
) -> Result<PointerClickEvidence, PointerClickPreDownError> {
    click_designer_client_bounds_with_pre_down_check_and_button(
        child,
        target,
        bounds,
        trace_path,
        PointerButton::Left,
        pre_down_check,
    )
}

fn click_designer_client_bounds_with_pre_down_check_and_button(
    child: &NativeChild,
    target: &WindowSnapshot,
    bounds: [i32; 4],
    trace_path: &Path,
    button: PointerButton,
    pre_down_check: impl FnOnce((i32, i32), usize) -> Result<(), PointerClickPreDownError>,
) -> Result<PointerClickEvidence, PointerClickPreDownError> {
    click_designer_client_bounds_with_pre_down_check_and_owned_keys(
        child,
        target,
        bounds,
        trace_path,
        button,
        &[],
        pre_down_check,
    )
}

fn click_designer_client_bounds_with_pre_down_check_and_owned_keys(
    child: &NativeChild,
    target: &WindowSnapshot,
    bounds: [i32; 4],
    trace_path: &Path,
    button: PointerButton,
    owned_keys: &[VIRTUAL_KEY],
    pre_down_check: impl FnOnce((i32, i32), usize) -> Result<(), PointerClickPreDownError>,
) -> Result<PointerClickEvidence, PointerClickPreDownError> {
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
    click_screen_point_with_pre_down_check_and_owned_keys(
        child,
        target,
        screen_point,
        button,
        "Designer semantic click",
        PointerMoveAcknowledgement {
            trace_path,
            kind: PointerTraceKind::DesignerClient,
            nudge_screen_point: nudge_screen,
            nudge_trace_point: (nudge_client.x, nudge_client.y),
            target_trace_point: client_point,
        },
        owned_keys,
        pre_down_check,
    )
}

pub(super) fn click_owned_radial_point(
    child: &NativeChild,
    surface: &WindowSnapshot,
    point: POINT,
    trace_path: &Path,
    trace_cursor: usize,
    layout_generation: u64,
) -> Result<super::QueryPointerClickEvidence, String> {
    child.validate_window(surface.hwnd)?;
    if surface.process_id != child.process_id()
        || surface.class_name != RADIAL_HOST_WINDOW_CLASS
        || !surface.visible
        || surface.minimized
        || point.x < surface.bounds[0]
        || point.y < surface.bounds[1]
        || point.x >= surface.bounds[2]
        || point.y >= surface.bounds[3]
    {
        return Err("refused radial cell input outside an active candidate-owned surface".into());
    }
    child.focus_window(surface)?;
    unsafe { SetCursorPos(point.x, point.y) }
        .map_err(|error| format!("position pointer over acknowledged radial cell: {error}"))?;
    let hit = unsafe { WindowFromPoint(point) };
    let hit_pid = window_process_id(hit);
    let hit_is_owned_surface = hit == surface.hwnd
        || (hit_pid == child.process_id() && unsafe { IsChild(surface.hwnd, hit).as_bool() });
    if !hit_is_owned_surface {
        return Err(format!(
            "radial selection point is covered by an unowned/non-surface HWND={} PID={hit_pid}",
            hwnd_id(hit)
        ));
    }
    if layout_generation == 0 {
        return Err("refused radial click without a production layout generation".into());
    }
    focus_is_validated(surface.hwnd, child.process_id())?;
    let mut button_guard =
        MouseButtonGuard::new(surface.hwnd, child.process_id(), PointerButton::Left);
    let down = match send_validated_input_allowing_owned_keys(
        surface.hwnd,
        child.process_id(),
        &[mouse_input(true)],
        "radial cell primary down",
        &[],
    ) {
        Ok(evidence) => {
            button_guard.armed = evidence.inserted > 0;
            if evidence.inserted != 1 {
                return Err(format!(
                    "radial primary down inserted {} events; expected exactly one",
                    evidence.inserted
                ));
            }
            evidence
        }
        Err((inserted, error)) => {
            button_guard.armed = inserted > 0;
            if button_guard.armed {
                let cleanup = button_guard
                    .release()
                    .map(|evidence| format!("released={}", evidence.inserted))
                    .unwrap_or_else(|cleanup| format!("release_failed={cleanup}"));
                return Err(format!("{error}; radial button cleanup={cleanup}"));
            }
            return Err(error);
        }
    };
    let up = button_guard.release()?;
    if up.inserted != 1 {
        return Err(format!(
            "radial primary up inserted {} events; expected exactly one",
            up.inserted
        ));
    }
    let (ack, release_settle_ms) = wait_for_radial_primary_release(
        trace_path,
        trace_cursor,
        hwnd_id(surface.hwnd),
        layout_generation,
        Duration::from_secs(2),
    )
    .map_err(|error| {
        format!(
            "{error}; runner down=[{}]; runner up=[{}]",
            down.describe(),
            up.describe()
        )
    })?;
    Ok(super::QueryPointerClickEvidence {
        down_inserted: down.inserted,
        up_inserted: up.inserted,
        layout_generation,
        down_event_ordinal: ack.down_event_ordinal,
        up_event_ordinal: ack.up_event_ordinal,
        release_settle_ms,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RadialPointerReleaseAck {
    down_event_ordinal: usize,
    up_event_ordinal: usize,
}

fn radial_pointer_release_ack_after(
    lines: &[String],
    cursor: usize,
    hwnd: u64,
    generation: u64,
) -> Option<RadialPointerReleaseAck> {
    let mut down_event_ordinal = None;
    for (ordinal, line) in lines.iter().enumerate().skip(cursor) {
        if !line.contains("trace_event=\"native_pointer\"")
            || trace_field(line, "button") != Some("Primary")
            || trace_field(line, "owner") != Some("PreviewInput")
            || trace_field(line, "hwnd").and_then(|value| value.parse::<u64>().ok()) != Some(hwnd)
            || trace_field(line, "generation").and_then(|value| value.parse::<u64>().ok())
                != Some(generation)
        {
            continue;
        }
        match trace_field(line, "transition") {
            Some("Down") if down_event_ordinal.is_none() => {
                down_event_ordinal = Some(ordinal.saturating_add(1));
            }
            Some("Down") => return None,
            Some("Up") => {
                let Some(down_event_ordinal) = down_event_ordinal else {
                    continue;
                };
                let up_event_ordinal = ordinal.saturating_add(1);
                if up_event_ordinal > down_event_ordinal {
                    return Some(RadialPointerReleaseAck {
                        down_event_ordinal,
                        up_event_ordinal,
                    });
                }
            }
            _ => {}
        }
    }
    None
}

fn wait_for_radial_primary_release(
    trace_path: &Path,
    trace_cursor: usize,
    hwnd: u64,
    generation: u64,
    timeout: Duration,
) -> Result<(RadialPointerReleaseAck, u64), String> {
    let (ack, elapsed) = wait_for_pointer_release_settle(timeout, || {
        let trace = std::fs::read_to_string(trace_path)
            .map_err(|error| format!("read radial pointer release trace: {error}"))?;
        let lines = trace_event_lines(&trace);
        let ack = radial_pointer_release_ack_after(&lines, trace_cursor, hwnd, generation);
        let async_state = unsafe { GetAsyncKeyState(VK_LBUTTON.0 as i32) };
        Ok((ack, async_state < 0, async_state as u16))
    })
    .map_err(|error| match error {
        PointerReleaseWaitError::Sample(error) => error,
        PointerReleaseWaitError::TimedOut(failure) => match failure.last_ack {
        Some(ack) => format!(
            "production acknowledged radial primary Down/Up ordinals {}/{} for HWND={} generation={}, but left-button release did not settle before {}ms (async_state=0x{:04x})",
            ack.down_event_ordinal,
            ack.up_event_ordinal,
            hwnd,
            generation,
            timeout.as_millis(),
            failure.last_async_state.unwrap_or_default(),
        ),
        None => format!(
            "no fresh ordered production radial primary Down/Up acknowledgment for HWND={} generation={} before {}ms",
            hwnd,
            generation,
            timeout.as_millis(),
        ),
        },
    })?;
    Ok((ack, elapsed))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PointerReleaseWaitFailure {
    last_ack: Option<RadialPointerReleaseAck>,
    last_async_state: Option<u16>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PointerReleaseWaitError {
    Sample(String),
    TimedOut(PointerReleaseWaitFailure),
}

fn wait_for_pointer_release_settle(
    timeout: Duration,
    mut sample: impl FnMut() -> Result<(Option<RadialPointerReleaseAck>, bool, u16), String>,
) -> Result<(RadialPointerReleaseAck, u64), PointerReleaseWaitError> {
    let started = Instant::now();
    let deadline = started + timeout;
    let mut last_ack = None;
    let mut last_async_state = None;
    loop {
        let (ack, button_down, async_state) = sample().map_err(PointerReleaseWaitError::Sample)?;
        if ack.is_some() {
            last_ack = ack;
            last_async_state = Some(async_state);
        }
        if let Some(ack) = ack.filter(|_| !button_down) {
            return Ok((
                ack,
                u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            ));
        }
        let now = Instant::now();
        if now >= deadline {
            return Err(PointerReleaseWaitError::TimedOut(
                PointerReleaseWaitFailure {
                    last_ack,
                    last_async_state,
                },
            ));
        }
        std::thread::sleep(WINDOW_POLL.min(deadline.saturating_duration_since(now)));
    }
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
    button: PointerButton,
    operation: &str,
    pointer_move_ack: PointerMoveAcknowledgement<'_>,
) -> Result<PointerClickEvidence, String> {
    click_screen_point_with_pre_down_check(
        child,
        target,
        point,
        button,
        operation,
        pointer_move_ack,
        |_, _| Ok(()),
    )
    .map_err(PointerClickPreDownError::into_message)
}

fn click_screen_point_with_pre_down_check(
    child: &NativeChild,
    target: &WindowSnapshot,
    point: POINT,
    button: PointerButton,
    operation: &str,
    pointer_move_ack: PointerMoveAcknowledgement<'_>,
    pre_down_check: impl FnOnce((i32, i32), usize) -> Result<(), PointerClickPreDownError>,
) -> Result<PointerClickEvidence, PointerClickPreDownError> {
    click_screen_point_with_pre_down_check_and_owned_keys(
        child,
        target,
        point,
        button,
        operation,
        pointer_move_ack,
        &[],
        pre_down_check,
    )
}

fn click_screen_point_with_pre_down_check_and_owned_keys(
    child: &NativeChild,
    target: &WindowSnapshot,
    point: POINT,
    button: PointerButton,
    operation: &str,
    pointer_move_ack: PointerMoveAcknowledgement<'_>,
    owned_keys: &[VIRTUAL_KEY],
    pre_down_check: impl FnOnce((i32, i32), usize) -> Result<(), PointerClickPreDownError>,
) -> Result<PointerClickEvidence, PointerClickPreDownError> {
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
        )
        .into());
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
    let nudge_movement = send_validated_input_allowing_owned_keys(
        target.hwnd,
        child.process_id(),
        &nudge_movement,
        &format!("{operation} pointer nudge"),
        owned_keys,
    )
    .map_err(|(_, error)| error)?;
    let nudge_correction_events = wait_for_pointer_move_ack(
        child,
        target,
        operation,
        &pointer_move_ack,
        trace_cursor,
        pointer_move_ack.nudge_trace_point,
        nudge_screen_point,
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
    let movement = send_validated_input_allowing_owned_keys(
        target.hwnd,
        child.process_id(),
        &movement,
        &format!("{operation} pointer move"),
        owned_keys,
    )
    .map_err(|(_, error)| error)?;
    let target_correction_events = wait_for_pointer_move_ack(
        child,
        target,
        operation,
        &pointer_move_ack,
        target_move_cursor,
        pointer_move_ack.target_trace_point,
        point,
        Duration::from_secs(3),
    )?;
    let foreground_hwnd = unsafe { GetForegroundWindow() };
    if foreground_hwnd != target.hwnd {
        return Err(format!(
            "blocked precondition: target HWND={} is not foreground before {operation} (foreground HWND={})",
            hwnd_id(target.hwnd),
            hwnd_id(foreground_hwnd)
        )
        .into());
    }
    if root_screen_click {
        validate_root_pointer_geometry(target)?;
    }
    let down = [mouse_button_input(button, true)];
    let (down_trace_cursor, down) = dispatch_pointer_down_after_preflight(
        || pre_down_check(pointer_move_ack.target_trace_point, target_move_cursor),
        || {
            child.validate_window(target.hwnd)?;
            if unsafe { GetForegroundWindow() } != target.hwnd {
                return Err(format!(
                    "blocked precondition: target HWND={} lost foreground before {operation} button-down",
                    hwnd_id(target.hwnd)
                ));
            }
            if root_screen_click {
                validate_root_pointer_geometry(target)?;
            }
            let mut current_client = RECT::default();
            unsafe { GetClientRect(target.hwnd, &mut current_client) }.map_err(|error| {
                format!("refresh target client bounds before {operation} down: {error}")
            })?;
            let mut current_top_left = POINT {
                x: current_client.left,
                y: current_client.top,
            };
            let mut current_bottom_right = POINT {
                x: current_client.right,
                y: current_client.bottom,
            };
            if !unsafe { ClientToScreen(target.hwnd, &mut current_top_left) }.as_bool()
                || !unsafe { ClientToScreen(target.hwnd, &mut current_bottom_right) }.as_bool()
                || point.x < current_top_left.x
                || point.y < current_top_left.y
                || point.x >= current_bottom_right.x
                || point.y >= current_bottom_right.y
            {
                return Err(format!(
                    "{operation} target client geometry changed before button-down"
                ));
            }
            validate_pointer_coverage(target.hwnd, child.process_id(), point, operation)?;
            let trace_cursor = trace_line_count(pointer_move_ack.trace_path)?;
            let evidence = send_validated_input_allowing_owned_keys(
                target.hwnd,
                child.process_id(),
                &down,
                operation,
                owned_keys,
            )
            .map_err(|(_, error)| error)?;
            Ok((trace_cursor, evidence))
        },
    )?;
    let button_state_after_down = unsafe { GetAsyncKeyState(button.virtual_key() as i32) };
    let mut button_guard =
        MouseButtonGuard::new_with_owned_keys(target.hwnd, child.process_id(), button, owned_keys);
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
            "{error}; checked {:?} down=[{}], async=0x{:04x}",
            button,
            down.describe(),
            button_state_after_down as u16
        )
    })?;
    let button_state_before_up = unsafe { GetAsyncKeyState(button.virtual_key() as i32) };
    let up_trace_cursor = trace_line_count(pointer_move_ack.trace_path)?;
    let up = button_guard.release()?;
    let button_state_after_up = unsafe { GetAsyncKeyState(button.virtual_key() as i32) };
    wait_for_pointer_button_ack(
        &pointer_move_ack,
        up_trace_cursor,
        (point.x, point.y),
        false,
        Duration::from_secs(3),
    )
    .map_err(|error| {
        format!(
            "{error}; checked {:?} up=[{}], async after up=0x{:04x}",
            button,
            up.describe(),
            button_state_after_up as u16
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
        button,
        button_state_after_down,
        button_state_before_up,
        up,
        up_acknowledged: true,
        button_state_after_up,
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
    expected_screen_point: POINT,
    timeout: Duration,
) -> Result<usize, String> {
    let surface = match acknowledgement.kind {
        PointerTraceKind::RootScreen => "ROOT screen point",
        PointerTraceKind::DesignerClient => "Designer client point",
    };
    let event_cursor = std::cell::Cell::new(cursor);
    wait_for_pointer_move_ack_with(
        operation,
        surface,
        expected_point,
        timeout,
        || {
            latest_pointer_move_after(
                acknowledgement.trace_path,
                event_cursor.get(),
                acknowledgement.kind,
            )
        },
        || {
            child.validate_window(target_window.hwnd)?;
            let cursor_position = cursor_position()?;
            if cursor_position.x != expected_screen_point.x
                || cursor_position.y != expected_screen_point.y
            {
                return Err(format!(
                    "{operation} exact pointer trace did not match physical cursor position: expected screen point=({},{}), current=({},{})",
                    expected_screen_point.x,
                    expected_screen_point.y,
                    cursor_position.x,
                    cursor_position.y
                ));
            }
            validate_pointer_coverage(
                target_window.hwnd,
                child.process_id(),
                expected_screen_point,
                operation,
            )?;
            Ok(())
        },
        |dx, dy| {
            let correction = relative_mouse_move_input(dx, dy);
            send_pointer_correction_after_cursor(acknowledgement.trace_path, &event_cursor, || {
                send_validated_input(
                    target_window.hwnd,
                    child.process_id(),
                    &[correction],
                    &format!("{operation} exact pointer correction"),
                )
                .map(|evidence| evidence.inserted)
            })
        },
        std::thread::sleep,
    )
}

fn send_pointer_correction_after_cursor(
    trace_path: &Path,
    event_cursor: &std::cell::Cell<usize>,
    send: impl FnOnce() -> Result<usize, String>,
) -> Result<usize, String> {
    event_cursor.set(trace_line_count(trace_path)?);
    send()
}

fn wait_for_pointer_move_ack_with(
    operation: &str,
    surface: &str,
    expected_point: (i32, i32),
    timeout: Duration,
    mut read_latest: impl FnMut() -> Result<Option<(i32, i32)>, String>,
    mut verify_physical_owner: impl FnMut() -> Result<(), String>,
    mut send_correction: impl FnMut(i32, i32) -> Result<usize, String>,
    mut wait: impl FnMut(Duration),
) -> Result<usize, String> {
    let deadline = Instant::now() + timeout;
    let mut correction_events = 0usize;
    let mut last_observed = None;
    loop {
        if let Some(observed) = read_latest()? {
            last_observed = Some(observed);
            match pointer_correction_delta(observed, expected_point) {
                Ok(None) => {
                    verify_physical_owner()?;
                    return Ok(correction_events);
                }
                Ok(Some((dx, dy))) => {
                    if correction_events >= MAX_POINTER_CORRECTIONS {
                        return Err(format!(
                            "{operation} pointer move remained inexact after {correction_events} bounded corrections: expected={expected_point:?} observed={observed:?}"
                        ));
                    }
                    correction_events = correction_events.saturating_add(send_correction(dx, dy)?);
                }
                // Large changes belong to another physical move or to a stale
                // surface coordinate frame. They are not a rounding error to
                // correct. Keep waiting for the exact owner trace; the physical
                // cursor is checked only when that exact trace arrives.
                Err(_) => {}
            }
        }
        let now = Instant::now();
        if now >= deadline {
            return Err(format!(
                "production {surface} pointer move did not reach exact point {expected_point:?} before click; last observed={last_observed:?}, bounded correction events={correction_events}"
            ));
        }
        wait(Duration::from_millis(10).min(deadline.saturating_duration_since(now)));
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
        let acknowledged = trace.lines().skip(cursor).any(|line| {
            pointer_button_ack_matches(line, acknowledgement.kind, screen_point, pressed)
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PointerButtonAckOrder {
    Pending,
    Down,
    UpBeforeDown,
    UpAfterDown,
}

fn pointer_button_ack_matches(
    line: &str,
    kind: PointerTraceKind,
    screen_point: (i32, i32),
    pressed: bool,
) -> bool {
    match kind {
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
    }
}

fn pointer_button_ack_order_after(
    trace: &str,
    cursor: usize,
    kind: PointerTraceKind,
    screen_point: (i32, i32),
) -> PointerButtonAckOrder {
    let mut down = None;
    let mut up = None;
    for (index, line) in trace.lines().skip(cursor).enumerate() {
        if down.is_none() && pointer_button_ack_matches(line, kind, screen_point, true) {
            down = Some(index);
        }
        if up.is_none() && pointer_button_ack_matches(line, kind, screen_point, false) {
            up = Some(index);
        }
    }
    match (down, up) {
        (None, None) => PointerButtonAckOrder::Pending,
        (Some(_), None) => PointerButtonAckOrder::Down,
        (None, Some(_)) => PointerButtonAckOrder::UpBeforeDown,
        (Some(down), Some(up)) if up < down => PointerButtonAckOrder::UpBeforeDown,
        (Some(_), Some(_)) => PointerButtonAckOrder::UpAfterDown,
    }
}

fn pointer_button_down_acknowledged(order: PointerButtonAckOrder) -> Result<bool, String> {
    match order {
        PointerButtonAckOrder::Pending => Ok(false),
        PointerButtonAckOrder::Down => Ok(true),
        PointerButtonAckOrder::UpBeforeDown => {
            Err("production pointer-button up arrived before its fresh down acknowledgement".into())
        }
        PointerButtonAckOrder::UpAfterDown => Err(
            "production pointer-button was already released after its fresh down acknowledgement"
                .into(),
        ),
    }
}

fn wait_for_pointer_button_down_ack(
    acknowledgement: &PointerMoveAcknowledgement<'_>,
    cursor: usize,
    screen_point: (i32, i32),
    timeout: Duration,
) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    loop {
        let trace = std::fs::read_to_string(acknowledgement.trace_path)
            .map_err(|error| format!("read production pointer-button trace: {error}"))?;
        match pointer_button_down_acknowledged(pointer_button_ack_order_after(
            &trace,
            cursor,
            acknowledgement.kind,
            screen_point,
        ))? {
            true => return Ok(()),
            false => {}
        }
        if Instant::now() >= deadline {
            let surface = match acknowledgement.kind {
                PointerTraceKind::RootScreen => "production ROOT",
                PointerTraceKind::DesignerClient => "production Designer",
            };
            return Err(format!(
                "{surface} did not acknowledge pointer-button down at screen point {screen_point:?}"
            ));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn wait_for_pointer_button_async_down_with(
    timeout: Duration,
    mut now: impl FnMut() -> Instant,
    mut sleep: impl FnMut(Duration),
    mut sample: impl FnMut() -> Result<(i16, bool), String>,
) -> Result<i16, String> {
    let deadline = now() + timeout;
    loop {
        let (async_state, release_acknowledged) = sample()?;
        if release_acknowledged {
            return Err(format!(
                "production pointer-button up arrived before physical down was verified (async=0x{:04x})",
                async_state as u16
            ));
        }
        if async_state < 0 {
            return Ok(async_state);
        }
        let current = now();
        if current >= deadline {
            return Err(format!(
                "physical pointer-button down was not observed before timeout (async=0x{:04x})",
                async_state as u16
            ));
        }
        sleep(WINDOW_POLL.min(deadline.saturating_duration_since(current)));
    }
}

fn wait_for_pointer_button_down_ready_with(
    wait_for_acknowledgement: impl FnOnce() -> Result<(), String>,
    timeout: Duration,
    now: impl FnMut() -> Instant,
    sleep: impl FnMut(Duration),
    sample: impl FnMut() -> Result<(i16, bool), String>,
) -> Result<i16, String> {
    wait_for_acknowledgement()?;
    wait_for_pointer_button_async_down_with(timeout, now, sleep, sample)
}

fn wait_for_pointer_button_down_ready(
    acknowledgement: &PointerMoveAcknowledgement<'_>,
    cursor: usize,
    screen_point: (i32, i32),
    virtual_key: i32,
    timeout: Duration,
) -> Result<i16, String> {
    wait_for_pointer_button_down_ready_with(
        || wait_for_pointer_button_down_ack(acknowledgement, cursor, screen_point, timeout),
        timeout,
        Instant::now,
        std::thread::sleep,
        || {
            let trace = std::fs::read_to_string(acknowledgement.trace_path)
                .map_err(|error| format!("read production pointer-button trace: {error}"))?;
            let order =
                pointer_button_ack_order_after(&trace, cursor, acknowledgement.kind, screen_point);
            let released = matches!(
                order,
                PointerButtonAckOrder::UpBeforeDown | PointerButtonAckOrder::UpAfterDown
            );
            Ok((unsafe { GetAsyncKeyState(virtual_key) }, released))
        },
    )
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

pub(super) fn trace_static_enum_field<'a>(line: &'a str, name: &str) -> Option<&'a str> {
    let value = trace_field(line, name)?;
    if let Some(quoted) = value.strip_prefix('"') {
        let value = quoted.strip_suffix('"')?;
        (!value.contains('"')).then_some(value)
    } else if value.contains('"') {
        None
    } else {
        Some(value)
    }
}

fn trace_bool_field(line: &str, name: &str) -> Option<bool> {
    match trace_field(line, name)? {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct DesignerPointerReleaseBoundary {
    pub first_line: usize,
    pub after_trace_sequence: u64,
    pub through_trace_sequence: u64,
    pub session_id: u64,
    pub generation: u64,
    pub target_hwnd: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct DesignerPointerReleaseReceipt {
    pub trace_sequence: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DesignerPointerEdgeSnapshot {
    trace_sequence: u64,
    session_id: u64,
    generation: u64,
    window_under_cursor_hwnd: u64,
    screen_point: (i32, i32),
    down: bool,
    up: bool,
}

impl DesignerPointerEdgeSnapshot {
    fn parse(line: &str) -> Option<Self> {
        if trace_static_enum_field(line, "trace_event")? != "designer_pointer" {
            return None;
        }
        Some(Self {
            trace_sequence: trace_field(line, "trace_sequence")?.parse().ok()?,
            session_id: trace_field(line, "session_id")?.parse().ok()?,
            generation: trace_field(line, "generation")?.parse().ok()?,
            window_under_cursor_hwnd: trace_field(line, "window_under_cursor_hwnd")?
                .parse()
                .ok()?,
            screen_point: (
                trace_field(line, "cursor_screen_x")?.parse().ok()?,
                trace_field(line, "cursor_screen_y")?.parse().ok()?,
            ),
            down: trace_bool_field(line, "pointer_down")?,
            up: trace_bool_field(line, "pointer_up")?,
        })
    }
}

/// Correlate the producer's aggregate pointer edges with one checked primary
/// click. The producer has no button field, so the checked injection owns that
/// fact; it acknowledges down before sending up, requiring distinct ordered
/// edges even when clicking mutates the draft later in the release frame.
pub(super) fn owned_designer_pointer_release_after(
    lines: &[String],
    boundary: DesignerPointerReleaseBoundary,
    click: &PointerClickEvidence,
) -> Result<DesignerPointerReleaseReceipt, String> {
    if boundary.session_id == 0
        || boundary.target_hwnd == 0
        || boundary.after_trace_sequence == 0
        || boundary.through_trace_sequence <= boundary.after_trace_sequence
        || boundary.first_line > lines.len()
        || hwnd_id(click.target_hwnd) != boundary.target_hwnd
        || hwnd_id(click.under_cursor_hwnd) == 0
        || click.foreground_hwnd != click.target_hwnd
        || click.down.foreground_hwnd != boundary.target_hwnd
        || click.up.foreground_hwnd != boundary.target_hwnd
        || click.button != PointerButton::Left
        || click.down.inserted != 1
        || click.up.inserted != 1
        || !click.pointer_move_acknowledged
        || !click.down_acknowledged
        || !click.up_acknowledged
    {
        return Err("invalid captured Designer owner or checked primary click".into());
    }
    let mut down_sequence = None;
    for edge in lines
        .iter()
        .skip(boundary.first_line)
        .filter_map(|line| DesignerPointerEdgeSnapshot::parse(line))
    {
        if edge.trace_sequence <= boundary.after_trace_sequence
            || edge.trace_sequence > boundary.through_trace_sequence
            || edge.session_id != boundary.session_id
            || edge.generation != boundary.generation
            || edge.window_under_cursor_hwnd != hwnd_id(click.under_cursor_hwnd)
            || !cursor_points_match(
                POINT {
                    x: edge.screen_point.0,
                    y: edge.screen_point.1,
                },
                POINT {
                    x: click.screen_point.0,
                    y: click.screen_point.1,
                },
            )
        {
            continue;
        }
        match (edge.down, edge.up) {
            (true, false) if down_sequence.is_none() => {
                down_sequence = Some(edge.trace_sequence);
            }
            (false, true) if down_sequence.is_some_and(|down| down < edge.trace_sequence) => {
                return Ok(DesignerPointerReleaseReceipt {
                    trace_sequence: edge.trace_sequence,
                });
            }
            _ => return Err("Designer pointer edges did not form one fresh ordered click".into()),
        }
    }
    Err("checked click lacked a fresh owned Designer pointer release".into())
}

fn parse_action_editor_surface(value: &str) -> Option<ActionEditorSurface> {
    match value {
        "properties" => Some(ActionEditorSurface::Properties),
        "inspector" => Some(ActionEditorSurface::Inspector),
        _ => None,
    }
}

fn parse_action_editor_control_name(value: &str) -> Option<&'static str> {
    const CONTROLS: &[&str] = &[
        "query_tab",
        "query",
        "query_field",
        "search",
        "query_mode",
        "result_count",
        "result",
        "result_target",
        "pin",
        "pin_result",
        "save_query",
        "mode",
        "test_query",
        "test_result",
        "contextual_target",
        "pin_contextual",
        "test_assigned",
        "advanced",
        "advanced_tab",
        "exact_command",
        "arguments",
        "exact_command_field",
        "exact_args_field",
        "use_exact_command",
        "test_exact_command",
        "test",
        "apply",
        "open_inspector",
        "close",
        "save",
        "undo",
        "redo",
        "keep_editing",
        "discard",
        "reopen",
        "add_to_radial",
        "menu",
        "ring",
        "cell",
        "spacer",
        "append",
        "replace",
        "cancel",
        "confirm_close_tree",
    ];
    CONTROLS.iter().copied().find(|control| *control == value)
}

fn parse_action_editor_identity(line: &str) -> Option<ActionEditorTraceIdentity> {
    Some(ActionEditorTraceIdentity {
        surface: parse_action_editor_surface(trace_static_enum_field(line, "editor_surface")?)?,
        session_id: trace_field(line, "editor_session_id")?.parse().ok()?,
        draft_generation: trace_field(line, "draft_generation")?.parse().ok()?,
        stable_target_digest: trace_field(line, "stable_target_digest")?.parse().ok()?,
        editor_epoch: trace_field(line, "editor_epoch")?.parse().ok()?,
        edit_generation: trace_field(line, "edit_generation")?.parse().ok()?,
        query_generation: trace_field(line, "query_generation")?.parse().ok()?,
        query_request_generation: trace_field(line, "query_request_generation")?
            .parse()
            .ok()?,
        search_request_generation: trace_field(line, "search_request_generation")?
            .parse()
            .ok()?,
        test_request_generation: trace_field(line, "test_request_generation")?.parse().ok()?,
        query_digest: trace_field(line, "query_digest")?.parse().ok()?,
        assigned_binding_digest: trace_field(line, "editor_assigned_binding_digest")?
            .parse()
            .ok()?,
    })
}

pub(super) fn parse_action_editor_control(line: &str) -> Option<ActionEditorControlSnapshot> {
    if !line.contains("trace_event=\"designer_action_editor_control\"") {
        return None;
    }
    let index = trace_i32_field(line, "control_index=")?;
    Some(ActionEditorControlSnapshot {
        identity: parse_action_editor_identity(line)?,
        control: parse_action_editor_control_name(trace_static_enum_field(
            line,
            "editor_control",
        )?)?
        .to_owned(),
        index: usize::try_from(index).ok(),
        target_digest: trace_field(line, "target_digest")?.parse().ok()?,
        title_digest: trace_field(line, "title_digest")?.parse().ok()?,
        type_digest: trace_field(line, "type_digest")?.parse().ok()?,
        disambiguator_digest: trace_field(line, "disambiguator_digest")?.parse().ok()?,
        action_digest: trace_field(line, "action_digest")?.parse().ok()?,
        binding_digest: trace_field(line, "binding_digest")?.parse().ok()?,
        value_digest: trace_field(line, "value_digest")?.parse().ok()?,
        displayed_text_digest: trace_field(line, "displayed_text_digest")?.parse().ok()?,
        trace_sequence: multi_launcher::radial::acceptance_trace::trace_line_sequence(line)?,
        bounds: [
            trace_i32_field(line, "left_px=")?,
            trace_i32_field(line, "top_px=")?,
            trace_i32_field(line, "right_px=")?,
            trace_i32_field(line, "bottom_px=")?,
        ],
        full_bounds: [
            trace_i32_field(line, "full_left_px=")?,
            trace_i32_field(line, "full_top_px=")?,
            trace_i32_field(line, "full_right_px=")?,
            trace_i32_field(line, "full_bottom_px=")?,
        ],
        client_size: [
            trace_i32_field(line, "client_width_px=")?,
            trace_i32_field(line, "client_height_px=")?,
        ],
        fully_visible: trace_bool_field(line, "fully_visible")?,
        enabled: trace_bool_field(line, "enabled")?,
        selected: trace_bool_field(line, "selected")?,
        focused: trace_bool_field(line, "focused")?,
        clicked: trace_bool_field(line, "clicked")?,
        changed: trace_bool_field(line, "changed")?,
        enter_pressed: trace_bool_field(line, "enter_pressed")?,
        visible: trace_bool_field(line, "visible")?,
    })
}

pub(super) fn parse_action_editor_scroll(line: &str) -> Option<ActionEditorScrollSnapshot> {
    if !line.contains("trace_event=\"designer_action_editor_scroll\"") {
        return None;
    }
    let snapshot = ActionEditorScrollSnapshot {
        identity: parse_action_editor_identity(line)?,
        scroll_id: trace_field(line, "scroll_id")?.parse().ok()?,
        frame_nr: trace_field(line, "frame_nr")?.parse().ok()?,
        trace_sequence: trace_field(line, "trace_sequence")?.parse().ok()?,
        offset_y_milli: trace_field(line, "offset_y_milli")?.parse().ok()?,
        velocity_y_milli: trace_field(line, "velocity_y_milli")?.parse().ok()?,
        content_height_milli: trace_field(line, "content_height_milli")?.parse().ok()?,
        inner_height_milli: trace_field(line, "inner_height_milli")?.parse().ok()?,
        pixels_per_point_milli: trace_field(line, "pixels_per_point_milli")?.parse().ok()?,
        handle_min_length_milli: trace_field(line, "handle_min_length_milli")?.parse().ok()?,
        inner_bounds: [
            trace_i32_field(line, "inner_left_px=")?,
            trace_i32_field(line, "inner_top_px=")?,
            trace_i32_field(line, "inner_right_px=")?,
            trace_i32_field(line, "inner_bottom_px=")?,
        ],
        inner_visible_bounds: [
            trace_i32_field(line, "inner_visible_left_px=")?,
            trace_i32_field(line, "inner_visible_top_px=")?,
            trace_i32_field(line, "inner_visible_right_px=")?,
            trace_i32_field(line, "inner_visible_bottom_px=")?,
        ],
        track_bounds: [
            trace_i32_field(line, "track_left_px=")?,
            trace_i32_field(line, "track_top_px=")?,
            trace_i32_field(line, "track_right_px=")?,
            trace_i32_field(line, "track_bottom_px=")?,
        ],
        track_visible_bounds: [
            trace_i32_field(line, "track_visible_left_px=")?,
            trace_i32_field(line, "track_visible_top_px=")?,
            trace_i32_field(line, "track_visible_right_px=")?,
            trace_i32_field(line, "track_visible_bottom_px=")?,
        ],
        thumb_bounds: [
            trace_i32_field(line, "thumb_left_px=")?,
            trace_i32_field(line, "thumb_top_px=")?,
            trace_i32_field(line, "thumb_right_px=")?,
            trace_i32_field(line, "thumb_bottom_px=")?,
        ],
        thumb_visible_bounds: [
            trace_i32_field(line, "thumb_visible_left_px=")?,
            trace_i32_field(line, "thumb_visible_top_px=")?,
            trace_i32_field(line, "thumb_visible_right_px=")?,
            trace_i32_field(line, "thumb_visible_bottom_px=")?,
        ],
        painted_thumb_bounds: [
            trace_i32_field(line, "painted_thumb_left_px=")?,
            trace_i32_field(line, "painted_thumb_top_px=")?,
            trace_i32_field(line, "painted_thumb_right_px=")?,
            trace_i32_field(line, "painted_thumb_bottom_px=")?,
        ],
        painted_thumb_visible_bounds: [
            trace_i32_field(line, "painted_thumb_visible_left_px=")?,
            trace_i32_field(line, "painted_thumb_visible_top_px=")?,
            trace_i32_field(line, "painted_thumb_visible_right_px=")?,
            trace_i32_field(line, "painted_thumb_visible_bottom_px=")?,
        ],
        paint_clip_bounds: [
            trace_i32_field(line, "paint_clip_left_px=")?,
            trace_i32_field(line, "paint_clip_top_px=")?,
            trace_i32_field(line, "paint_clip_right_px=")?,
            trace_i32_field(line, "paint_clip_bottom_px=")?,
        ],
        client_size: [
            trace_i32_field(line, "client_width_px=")?,
            trace_i32_field(line, "client_height_px=")?,
        ],
    };
    snapshot.is_well_formed().then_some(snapshot)
}

pub(super) fn parse_inspector_cell_text_edit(line: &str) -> Option<InspectorCellTextEditSnapshot> {
    if !line.contains("trace_event=\"designer_inspector_cell_text_edit\"") {
        return None;
    }
    Some(InspectorCellTextEditSnapshot {
        target_digest: trace_field(line, "target_digest")?.parse().ok()?,
        session_id: trace_field(line, "session_id")?.parse().ok()?,
        generation: trace_field(line, "generation")?.parse().ok()?,
        value_digest: trace_field(line, "value_digest")?.parse().ok()?,
        trace_sequence: trace_field(line, "trace_sequence")?.parse().ok()?,
        bounds: [
            trace_i32_field(line, "left_px=")?,
            trace_i32_field(line, "top_px=")?,
            trace_i32_field(line, "right_px=")?,
            trace_i32_field(line, "bottom_px=")?,
        ],
        clip_bounds: [
            trace_i32_field(line, "clip_left_px=")?,
            trace_i32_field(line, "clip_top_px=")?,
            trace_i32_field(line, "clip_right_px=")?,
            trace_i32_field(line, "clip_bottom_px=")?,
        ],
        client_size: [
            trace_i32_field(line, "client_width_px=")?,
            trace_i32_field(line, "client_height_px=")?,
        ],
        visible: trace_bool_field(line, "visible")?,
        fully_visible: trace_bool_field(line, "fully_visible")?,
        focused: trace_bool_field(line, "focused")?,
        clicked: trace_bool_field(line, "clicked")?,
        changed: trace_bool_field(line, "changed")?,
    })
}

pub(super) fn inspector_cell_text_edits_after(
    trace_path: &Path,
    first_line: usize,
) -> Result<Vec<InspectorCellTextEditSnapshot>, String> {
    let trace = std::fs::read_to_string(trace_path)
        .map_err(|error| format!("read Inspector text-edit trace: {error}"))?;
    Ok(trace_event_lines(&trace)
        .into_iter()
        .skip(first_line)
        .filter_map(|line| parse_inspector_cell_text_edit(&line))
        .collect())
}

#[cfg(test)]
#[test]
fn inspector_cell_text_edit_reader_requires_complete_owned_identity_and_geometry() {
    let line = "WARN target trace_event=\"designer_inspector_cell_text_edit\" target_digest=101 session_id=7 generation=9 value_digest=103 trace_sequence=105 left_px=1 top_px=2 right_px=91 bottom_px=30 clip_left_px=1 clip_top_px=2 clip_right_px=91 clip_bottom_px=30 client_width_px=640 client_height_px=480 visible=true fully_visible=true focused=true clicked=true changed=false";
    let snapshot = parse_inspector_cell_text_edit(line).unwrap();
    assert_eq!(snapshot.target_digest, 101);
    assert_eq!(snapshot.session_id, 7);
    assert_eq!(snapshot.generation, 9);
    assert_eq!(snapshot.value_digest, 103);
    assert_eq!(snapshot.trace_sequence, 105);
    assert!(snapshot.visible && snapshot.fully_visible && snapshot.focused && snapshot.clicked);
    assert!(!snapshot.changed);
    for malformed in [
        line.replace("target_digest=101", "target_digest=private"),
        line.replace("generation=9", "generation=invalid"),
        line.replace("trace_sequence=105", "trace_sequence=invalid"),
        line.replace("fully_visible=true", "fully_visible=maybe"),
        line.replace("clicked=true", "clicked=unknown"),
    ] {
        assert!(
            parse_inspector_cell_text_edit(&malformed).is_none(),
            "{malformed}"
        );
    }
}

#[cfg(test)]
#[test]
fn action_editor_scroll_reader_requires_fresh_typed_thumb_geometry() {
    let line = "WARN target trace_event=\"designer_action_editor_scroll\" editor_surface=\"inspector\" editor_session_id=7 draft_generation=3 stable_target_digest=11 editor_epoch=5 edit_generation=9 query_generation=4 query_request_generation=6 search_request_generation=8 test_request_generation=2 query_digest=13 editor_assigned_binding_digest=17 scroll_id=19 frame_nr=23 trace_sequence=29 offset_y_milli=210000 velocity_y_milli=0 content_height_milli=1000000 inner_height_milli=190000 pixels_per_point_milli=1000 handle_min_length_milli=12000 inner_left_px=20 inner_top_px=40 inner_right_px=300 inner_bottom_px=230 inner_visible_left_px=20 inner_visible_top_px=40 inner_visible_right_px=280 inner_visible_bottom_px=230 track_left_px=285 track_top_px=40 track_right_px=300 track_bottom_px=230 track_visible_left_px=285 track_visible_top_px=40 track_visible_right_px=300 track_visible_bottom_px=230 thumb_left_px=285 thumb_top_px=80 thumb_right_px=300 thumb_bottom_px=116 thumb_visible_left_px=285 thumb_visible_top_px=80 thumb_visible_right_px=300 thumb_visible_bottom_px=116 painted_thumb_left_px=285 painted_thumb_top_px=80 painted_thumb_right_px=300 painted_thumb_bottom_px=116 painted_thumb_visible_left_px=285 painted_thumb_visible_top_px=80 painted_thumb_visible_right_px=300 painted_thumb_visible_bottom_px=116 paint_clip_left_px=0 paint_clip_top_px=0 paint_clip_right_px=640 paint_clip_bottom_px=480 client_width_px=640 client_height_px=480";
    let snapshot = parse_action_editor_scroll(line).unwrap();
    assert_eq!(snapshot.identity.surface, ActionEditorSurface::Inspector);
    assert_eq!(snapshot.identity.session_id, 7);
    assert_eq!(snapshot.identity.query_digest, 13);
    assert_eq!(snapshot.scroll_id, 19);
    assert_eq!(snapshot.frame_nr, 23);
    assert_eq!(snapshot.trace_sequence, 29);
    assert_eq!(snapshot.offset_y_milli, 210_000);
    assert_eq!(snapshot.thumb_bounds, [285, 80, 300, 116]);
    assert_eq!(snapshot.handle_min_length_milli, 12_000);
    assert_eq!(snapshot.painted_thumb_bounds, [285, 80, 300, 116]);

    let dense_line = line
        .replace(
            "content_height_milli=1000000",
            "content_height_milli=4000000",
        )
        .replace(" thumb_top_px=80 ", " thumb_top_px=50 ")
        .replace(" thumb_bottom_px=116 ", " thumb_bottom_px=59 ")
        .replace(" thumb_visible_top_px=80 ", " thumb_visible_top_px=50 ")
        .replace(
            " thumb_visible_bottom_px=116 ",
            " thumb_visible_bottom_px=59 ",
        )
        .replace(" painted_thumb_top_px=80 ", " painted_thumb_top_px=48 ")
        .replace(
            " painted_thumb_bottom_px=116 ",
            " painted_thumb_bottom_px=60 ",
        )
        .replace(
            " painted_thumb_visible_top_px=80 ",
            " painted_thumb_visible_top_px=48 ",
        )
        .replace(
            " painted_thumb_visible_bottom_px=116 ",
            " painted_thumb_visible_bottom_px=60 ",
        );
    let dense = parse_action_editor_scroll(&dense_line).unwrap();
    assert!(dense.thumb_bounds[3] - dense.thumb_bounds[1] < 12);
    assert_eq!(
        dense.painted_thumb_bounds[3] - dense.painted_thumb_bounds[1],
        12
    );
    assert!(
        ((dense.thumb_bounds[1] + dense.thumb_bounds[3])
            - (dense.painted_thumb_bounds[1] + dense.painted_thumb_bounds[3]))
            .abs()
            <= 1
    );

    for malformed in [
        line.replace("frame_nr=23", "frame_nr=bad"),
        line.replace("trace_sequence=29", "trace_sequence=0"),
        line.replace(" trace_sequence=29", ""),
        line.replace("offset_y_milli=210000", "offset_y_milli=900001"),
        line.replace("thumb_top_px=80", "thumb_top_px=60"),
        line.replace("thumb_visible_right_px=300", "thumb_visible_right_px=641"),
        line.replace("painted_thumb_bottom_px=116", "painted_thumb_bottom_px=111"),
        line.replace("editor_surface=\"inspector\"", "editor_surface=\"unknown\""),
    ] {
        assert!(
            parse_action_editor_scroll(&malformed).is_none(),
            "malformed scrollbar receipt accepted: {malformed}"
        );
    }

    // Captured from candidate28's D04 private trace before the publication
    // sequence field was added. Adding the sequence emitted by the production
    // fence makes the real producer shape acceptable to this strict reader.
    let candidate28_d04_scroll = "2026-09-29T05:03:26.267541Z WARN multi_launcher.radial_acceptance: radial acceptance trace trace_event=\"designer_action_editor_scroll\" elapsed_ms=72093 editor_surface=\"properties\" editor_session_id=6 draft_generation=2 stable_target_digest=10658348712107610701 editor_epoch=17 edit_generation=1 query_generation=1 query_request_generation=1 search_request_generation=2 test_request_generation=0 query_digest=16462946472559960157 editor_assigned_binding_digest=10622473845116688956 scroll_id=9840682517781602468 frame_nr=86 offset_y_milli=0 velocity_y_milli=0 content_height_milli=537000 inner_height_milli=190000 pixels_per_point_milli=1000 handle_min_length_milli=12000 inner_left_px=23 inner_top_px=318 inner_right_px=336 inner_bottom_px=508 inner_visible_left_px=23 inner_visible_top_px=318 inner_visible_right_px=336 inner_visible_bottom_px=508 track_left_px=335 track_top_px=318 track_right_px=336 track_bottom_px=508 track_visible_left_px=335 track_visible_top_px=318 track_visible_right_px=336 track_visible_bottom_px=508 thumb_left_px=335 thumb_top_px=318 thumb_right_px=336 thumb_bottom_px=385 thumb_visible_left_px=335 thumb_visible_top_px=318 thumb_visible_right_px=336 thumb_visible_bottom_px=385 painted_thumb_left_px=335 painted_thumb_top_px=318 painted_thumb_right_px=336 painted_thumb_bottom_px=385 painted_thumb_visible_left_px=335 painted_thumb_visible_top_px=318 painted_thumb_visible_right_px=336 painted_thumb_visible_bottom_px=385 paint_clip_left_px=20 paint_clip_top_px=52 paint_clip_right_px=339 paint_clip_bottom_px=630 client_width_px=900 client_height_px=650";
    assert!(parse_action_editor_scroll(candidate28_d04_scroll).is_none());
    let candidate28_with_sequence = format!("{candidate28_d04_scroll} trace_sequence=73");
    let captured = parse_action_editor_scroll(&candidate28_with_sequence)
        .expect("candidate28 D04 scroll record parses when publication sequence is present");
    assert_eq!(captured.identity.surface, ActionEditorSurface::Properties);
    assert_eq!(captured.identity.session_id, 6);
    assert_eq!(captured.trace_sequence, 73);
    assert!(
        parse_action_editor_scroll(&format!("{candidate28_d04_scroll} trace_sequence=0")).is_none()
    );
}

#[cfg(test)]
#[test]
fn scrollbar_hover_revalidation_rejects_stale_owner_or_moving_geometry() {
    let line = "WARN target trace_event=\"designer_action_editor_scroll\" editor_surface=\"inspector\" editor_session_id=7 draft_generation=3 stable_target_digest=11 editor_epoch=5 edit_generation=9 query_generation=4 query_request_generation=6 search_request_generation=8 test_request_generation=2 query_digest=13 editor_assigned_binding_digest=17 scroll_id=19 frame_nr=23 trace_sequence=29 offset_y_milli=210000 velocity_y_milli=0 content_height_milli=1000000 inner_height_milli=190000 pixels_per_point_milli=1000 handle_min_length_milli=12000 inner_left_px=20 inner_top_px=40 inner_right_px=300 inner_bottom_px=230 inner_visible_left_px=20 inner_visible_top_px=40 inner_visible_right_px=300 inner_visible_bottom_px=230 track_left_px=285 track_top_px=40 track_right_px=300 track_bottom_px=230 track_visible_left_px=285 track_visible_top_px=40 track_visible_right_px=300 track_visible_bottom_px=230 thumb_left_px=285 thumb_top_px=80 thumb_right_px=300 thumb_bottom_px=116 thumb_visible_left_px=285 thumb_visible_top_px=80 thumb_visible_right_px=300 thumb_visible_bottom_px=116 painted_thumb_left_px=285 painted_thumb_top_px=80 painted_thumb_right_px=300 painted_thumb_bottom_px=116 painted_thumb_visible_left_px=285 painted_thumb_visible_top_px=80 painted_thumb_visible_right_px=300 painted_thumb_visible_bottom_px=116 paint_clip_left_px=0 paint_clip_top_px=0 paint_clip_right_px=640 paint_clip_bottom_px=480 client_width_px=640 client_height_px=480";
    let expected = parse_action_editor_scroll(line).unwrap();
    let hovered = ActionEditorScrollSnapshot {
        frame_nr: 24,
        trace_sequence: 30,
        ..expected.clone()
    };
    assert!(action_editor_scrollbar_hover_matches(
        &expected,
        &hovered,
        [292, 90],
        [292, 120]
    ));
    let wrong_owner = ActionEditorScrollSnapshot {
        identity: ActionEditorTraceIdentity {
            session_id: 8,
            ..expected.identity
        },
        frame_nr: 24,
        trace_sequence: 30,
        ..expected.clone()
    };
    assert!(!action_editor_scrollbar_hover_matches(
        &expected,
        &wrong_owner,
        [292, 90],
        [292, 120]
    ));
    let moved_offset = ActionEditorScrollSnapshot {
        offset_y_milli: 211_000,
        frame_nr: 24,
        trace_sequence: 30,
        ..expected.clone()
    };
    assert!(!action_editor_scrollbar_hover_matches(
        &expected,
        &moved_offset,
        [292, 90],
        [292, 120]
    ));
    assert!(!async_button_is_released(i16::MIN));
    assert!(async_button_is_released(0));
}

pub(super) fn parse_action_editor_provider_event(
    line: &str,
) -> Option<ActionEditorProviderTraceSnapshot> {
    if !line.contains("trace_event=\"authoring_provider_search\"") {
        return None;
    }
    let edge = match trace_static_enum_field(line, "authoring_request_edge")? {
        "queued" => ActionEditorProviderEdge::Queued,
        "worker_started" => ActionEditorProviderEdge::WorkerStarted,
        "worker_completed" => ActionEditorProviderEdge::WorkerCompleted,
        "worker_failed" => ActionEditorProviderEdge::WorkerFailed,
        "applied" => ActionEditorProviderEdge::Applied,
        "rejected" => ActionEditorProviderEdge::Rejected,
        "retired" => ActionEditorProviderEdge::Retired,
        "cancelled" => ActionEditorProviderEdge::Cancelled,
        "retry_queued" => ActionEditorProviderEdge::RetryQueued,
        _ => return None,
    };
    let kind = match trace_static_enum_field(line, "authoring_search_kind")? {
        "search" => ActionEditorProviderKind::Search,
        "test" => ActionEditorProviderKind::Test,
        _ => return None,
    };
    let provider_revision = trace_field(line, "provider_revision")?
        .parse::<i64>()
        .ok()
        .and_then(|revision| u64::try_from(revision).ok());
    Some(ActionEditorProviderTraceSnapshot {
        identity: parse_action_editor_identity(line)?,
        edge,
        kind,
        query_digest: trace_field(line, "query_digest")?.parse().ok()?,
        binding_digest: trace_field(line, "binding_digest")?.parse().ok()?,
        provider_revision,
        trace_sequence: trace_field(line, "trace_sequence")?.parse().ok()?,
    })
}

pub(super) fn action_editor_controls_after(
    trace_path: &Path,
    first_line: usize,
) -> Result<Vec<ActionEditorControlSnapshot>, String> {
    let trace = std::fs::read_to_string(trace_path)
        .map_err(|error| format!("read action-editor control trace: {error}"))?;
    Ok(trace
        .lines()
        .filter(|line| line.contains("trace_event=\""))
        .skip(first_line)
        .filter_map(parse_action_editor_control)
        .collect())
}

fn latest_action_editor_scroll_after(
    trace_path: &Path,
    first_event: usize,
    surface: ActionEditorSurface,
) -> Result<Option<ActionEditorScrollSnapshot>, String> {
    let trace = std::fs::read_to_string(trace_path)
        .map_err(|error| format!("read action-editor scrollbar trace: {error}"))?;
    let mut latest = None;
    for line in trace_event_lines(&trace).iter().skip(first_event) {
        if !line.contains("trace_event=\"designer_action_editor_scroll\"") {
            continue;
        }
        let snapshot = parse_action_editor_scroll(line).ok_or_else(|| {
            "latest action-editor scrollbar receipt is malformed or outside its finite schema"
                .to_string()
        })?;
        if snapshot.identity.surface == surface {
            latest = Some(snapshot);
        }
    }
    Ok(latest)
}

pub(super) fn action_editor_provider_events_after(
    trace_path: &Path,
    first_line: usize,
) -> Result<Vec<ActionEditorProviderTraceSnapshot>, String> {
    let trace = std::fs::read_to_string(trace_path)
        .map_err(|error| format!("read action-editor provider trace: {error}"))?;
    Ok(trace
        .lines()
        .filter(|line| line.contains("trace_event=\""))
        .skip(first_line)
        .filter_map(parse_action_editor_provider_event)
        .collect())
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
        "ProjectedCell" => AuthoringControlTarget::ProjectedCell,
        "TreeSearch" => AuthoringControlTarget::TreeSearch,
        "TreeSearchClear" => AuthoringControlTarget::TreeSearchClear,
        "TreeSearchResult" => AuthoringControlTarget::TreeSearchResult,
        "BulkLabel" => AuthoringControlTarget::BulkLabel,
        "BulkSetLabel" => AuthoringControlTarget::BulkSetLabel,
        "DesignerBack" => AuthoringControlTarget::DesignerBack,
        "DesignerBreadcrumb" => AuthoringControlTarget::DesignerBreadcrumb,
        "EditDynamicSource" => AuthoringControlTarget::EditDynamicSource,
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
        "InspectorDiscardAndContinue" => AuthoringControlTarget::InspectorDiscardAndContinue,
        "InspectorCell" => AuthoringControlTarget::InspectorCell,
        "SkinRow" => AuthoringControlTarget::SkinRow,
        "AppearanceTile" => AuthoringControlTarget::AppearanceTile,
        "AppearanceApply" => AuthoringControlTarget::AppearanceApply,
        "AppearanceCancel" => AuthoringControlTarget::AppearanceCancel,
        "SimpleAccent" => AuthoringControlTarget::SimpleAccent,
        "SimpleOpacity" => AuthoringControlTarget::SimpleOpacity,
        "SimpleScale" => AuthoringControlTarget::SimpleScale,
        "SimpleSpacing" => AuthoringControlTarget::SimpleSpacing,
        "SimpleLabelSize" => AuthoringControlTarget::SimpleLabelSize,
        "SimpleLabels" => AuthoringControlTarget::SimpleLabels,
        "SimpleBold" => AuthoringControlTarget::SimpleBold,
        "SimpleShadow" => AuthoringControlTarget::SimpleShadow,
        "SkinGlowEnabled" => AuthoringControlTarget::SkinGlowEnabled,
        "OpenDesktopPreview" => AuthoringControlTarget::OpenDesktopPreview,
        "StopDesktopPreview" => AuthoringControlTarget::StopDesktopPreview,
        "Undo" => AuthoringControlTarget::Undo,
        "Redo" => AuthoringControlTarget::Redo,
        "Save" => AuthoringControlTarget::Save,
        "KeepEditing" => AuthoringControlTarget::KeepEditing,
        _ => return None,
    };
    let role = match trace_static_enum_field(line, "role")? {
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
    let optional_digest = |field: &str| {
        trace_field(line, field)
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|digest| *digest != 0)
    };
    let optional_index = |field: &str| {
        trace_i32_field(line, &format!("{field}=")).and_then(|index| usize::try_from(index).ok())
    };
    let coordinate =
        |field: &str| trace_field(line, field).and_then(|value| value.parse::<i32>().ok());
    let clip_bounds = if [
        "clip_left_px",
        "clip_top_px",
        "clip_right_px",
        "clip_bottom_px",
    ]
    .into_iter()
    .any(|field| trace_field(line, field).is_some())
    {
        Some([
            coordinate("clip_left_px")?,
            coordinate("clip_top_px")?,
            coordinate("clip_right_px")?,
            coordinate("clip_bottom_px")?,
        ])
    } else {
        // Gate C's retained trace packets precede pane clip measurements.
        None
    };
    let undo_fields = [
        "text_edit_field_digest",
        "text_edit_value_digest",
        "text_edit_undo_in_flux",
    ];
    let text_undo = if undo_fields
        .into_iter()
        .any(|field| trace_field(line, field).is_some())
    {
        let field_id_digest = trace_field(line, undo_fields[0])?.parse::<u64>().ok()?;
        let value_digest = trace_field(line, undo_fields[1])?.parse::<u64>().ok()?;
        match coordinate(undo_fields[2])? {
            -1 if field_id_digest == 0 && value_digest == 0 => None,
            in_flux @ (0 | 1)
                if target == AuthoringControlTarget::TreeSearch
                    && role == AuthoringControlRole::TextEdit
                    && control_index == -1
                    && field_id_digest != 0
                    && value_digest != 0 =>
            {
                Some(TextEditUndoSnapshot {
                    field_id_digest,
                    value_digest,
                    in_flux: in_flux == 1,
                })
            }
            _ => return None,
        }
    } else {
        // Older Gate C controls do not observe the framework's local undo state.
        None
    };
    Some(AuthoringControlSnapshot {
        target,
        role,
        index: usize::try_from(control_index).ok(),
        trace_sequence: trace_field(line, "trace_sequence")?.parse().ok()?,
        bounds: [
            coordinate("left_px")?,
            coordinate("top_px")?,
            coordinate("right_px")?,
            coordinate("bottom_px")?,
        ],
        clip_bounds,
        frame_nr: if trace_field(line, "frame_nr").is_some() {
            Some(trace_field(line, "frame_nr")?.parse().ok()?)
        } else {
            None
        },
        scroll_viewport: None,
        text_undo,
        client_size: [
            coordinate("client_width_px")?,
            coordinate("client_height_px")?,
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
        menu_id_digest: optional_digest("menu_id_digest"),
        ring_id_digest: optional_digest("ring_id_digest"),
        cell_id_digest: optional_digest("cell_id_digest"),
        authored_target_digest: trace_field(line, "authored_target_digest")
            .and_then(|digest| digest.parse::<u64>().ok())
            .filter(|digest| *digest != 0),
        cell_label_digest: optional_digest("cell_label_digest"),
        projected_source_target_digest: optional_digest("projected_source_target_digest"),
        projected_result_index: optional_index("projected_result_index"),
        edit_source_target_digest: optional_digest("edit_source_target_digest"),
        edit_source_result_index: optional_index("edit_source_result_index"),
        breadcrumb_menu_id_digest: optional_digest("breadcrumb_menu_id_digest"),
        ring_index: trace_i32_field(line, "cell_ring_index=")
            .and_then(|index| usize::try_from(index).ok()),
        slot_index: trace_i32_field(line, "cell_slot_index=")
            .and_then(|index| usize::try_from(index).ok()),
    })
}

fn parse_authoring_scroll_viewport(line: &str) -> Option<AuthoringScrollViewportSnapshot> {
    if !line.contains("trace_event=\"designer_authoring_scroll_viewport\"")
        || trace_field(line, "viewport")? != "Deferred"
    {
        return None;
    }
    let measured = DesignerAuthoringScrollViewport {
        owner: GateDControlScrollOwner::from_trace_label(trace_static_enum_field(
            line,
            "scroll_owner",
        )?)?,
        scroll_id: trace_field(line, "scroll_id")?.parse().ok()?,
        frame_nr: trace_field(line, "frame_nr")?.parse().ok()?,
        session_id: trace_field(line, "session_id")?.parse().ok()?,
        generation: trace_field(line, "generation")?.parse().ok()?,
        input_bounds: [
            trace_i32_field(line, "input_left_px=")?,
            trace_i32_field(line, "input_top_px=")?,
            trace_i32_field(line, "input_right_px=")?,
            trace_i32_field(line, "input_bottom_px=")?,
        ],
        paint_clip_bounds: [
            trace_i32_field(line, "clip_left_px=")?,
            trace_i32_field(line, "clip_top_px=")?,
            trace_i32_field(line, "clip_right_px=")?,
            trace_i32_field(line, "clip_bottom_px=")?,
        ],
        client_size: [
            trace_i32_field(line, "client_width_px=")?,
            trace_i32_field(line, "client_height_px=")?,
        ],
    };
    measured
        .is_valid()
        .then_some(AuthoringScrollViewportSnapshot {
            measured,
            trace_sequence: trace_field(line, "trace_sequence")?.parse().ok()?,
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
    // Keep only current control slots and one head per finite scroll owner.
    // Controls refresh less often than viewports, so correlate in publication
    // order and retain the original completed-frame pair while geometry agrees.
    let mut controls = Vec::<AuthoringControlSlot>::new();
    let mut viewports = [None::<AuthoringScrollViewportHead>; 3];
    for line in lines.iter().skip(first_line) {
        if let Some(control) = parse_authoring_control(line) {
            if control.session_id != session_id {
                continue;
            }
            if let Some(existing) = controls.iter_mut().find(|item| {
                item.control.target == control.target
                    && item.control.role == control.role
                    && item.control.index == control.index
            }) {
                // A new real control publication needs its own following
                // exact-frame receipt, including when its geometry is unchanged.
                *existing = AuthoringControlSlot {
                    control,
                    invalidated: false,
                };
            } else {
                controls.push(AuthoringControlSlot {
                    control,
                    invalidated: false,
                });
            }
            continue;
        }
        let Some(viewport) = parse_authoring_scroll_viewport(line) else {
            continue;
        };
        let slot = match viewport.measured.owner {
            GateDControlScrollOwner::Inspector => 0,
            GateDControlScrollOwner::MenuTree => 1,
            GateDControlScrollOwner::Resources => 2,
        };
        let ordered = if let Some(head) = viewports[slot].as_mut() {
            head.advance(viewport)
        } else {
            viewports[slot] = Some(AuthoringScrollViewportHead {
                latest: viewport,
                sequence_high_water: viewport.trace_sequence,
            });
            true
        };
        for control_slot in controls.iter_mut().filter(|slot| {
            gate_d_control_scroll_owner(&slot.control) == Some(viewport.measured.owner)
        }) {
            let control = &mut control_slot.control;
            if !ordered {
                // Duplicate-frame or stale publications invalidate every pair
                // from this owner; later viewport-only frames cannot restore it.
                control.scroll_viewport = None;
                control_slot.invalidated = true;
                continue;
            }
            if control_slot.invalidated {
                continue;
            }
            if let Some(attached) = control.scroll_viewport {
                let mut current = viewport.measured;
                current.frame_nr = attached.measured.frame_nr;
                if current != attached.measured {
                    control.scroll_viewport = None;
                    control_slot.invalidated = true;
                }
            } else if authoring_scroll_viewport_matches(control, &viewport) {
                control.scroll_viewport = Some(viewport);
            } else {
                // This slot missed its own following exact-frame receipt.
                // An owner/lifetime round trip must not revive the old control.
                control_slot.invalidated = true;
            }
        }
    }
    controls.into_iter().map(|slot| slot.control).collect()
}

struct AuthoringControlSlot {
    control: AuthoringControlSnapshot,
    invalidated: bool,
}

#[derive(Clone, Copy)]
struct AuthoringScrollViewportHead {
    latest: AuthoringScrollViewportSnapshot,
    sequence_high_water: u64,
}

impl AuthoringScrollViewportHead {
    fn advance(&mut self, viewport: AuthoringScrollViewportSnapshot) -> bool {
        // Frame counters can restart with a viewport lifetime. Physical trace
        // sequence ordering spans lifetimes; frame ordering applies within one.
        let same_lifetime = viewport.measured.session_id == self.latest.measured.session_id
            && viewport.measured.generation == self.latest.measured.generation;
        let ordered = viewport.trace_sequence > self.sequence_high_water
            && (!same_lifetime || viewport.measured.frame_nr > self.latest.measured.frame_nr);
        self.sequence_high_water = self.sequence_high_water.max(viewport.trace_sequence);
        if ordered {
            self.latest = viewport;
        }
        ordered
    }
}

fn trace_event_lines(trace: &str) -> Vec<String> {
    trace
        .lines()
        .filter(|line| line.contains("trace_event=\""))
        .map(str::to_owned)
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct AuthoringObservationBoundarySnapshot {
    pub phase: &'static str,
    pub request_id: u64,
    pub baseline_request_id: Option<u64>,
    pub captured_trace_sequence: u64,
    pub trace_sequence: u64,
}

fn parse_authoring_observation_boundary(
    line: &str,
) -> Option<AuthoringObservationBoundarySnapshot> {
    if !line.contains("trace_event=\"authoring_observation_boundary\"") {
        return None;
    }
    let phase = match trace_static_enum_field(line, "phase")? {
        "baseline" => "baseline",
        "snapshot" => "snapshot",
        "terminal" => "terminal",
        _ => return None,
    };
    let request_id = trace_field(line, "request_id")?.parse().ok()?;
    let baseline = trace_field(line, "baseline_request_id")?
        .parse::<u64>()
        .ok()?;
    let captured_trace_sequence = trace_field(line, "captured_trace_sequence")?.parse().ok()?;
    let trace_sequence = trace_field(line, "trace_sequence")?.parse().ok()?;
    (request_id > 0 && captured_trace_sequence > 0 && trace_sequence > captured_trace_sequence)
        .then_some(AuthoringObservationBoundarySnapshot {
            phase,
            request_id,
            baseline_request_id: (baseline > 0).then_some(baseline),
            captured_trace_sequence,
            trace_sequence,
        })
}

/// Wait until the candidate's buffered tracing sink has published the exact
/// GUI-owner receipt referenced by the mailbox ACK. A receipt with the same
/// request ID but a different phase, baseline, or cursor is a protocol error,
/// not a stale record to skip.
pub(super) fn wait_for_authoring_observation_boundary(
    trace_path: &Path,
    expected: AuthoringObservationBoundarySnapshot,
    timeout: Duration,
) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    loop {
        let trace = std::fs::read_to_string(trace_path)
            .map_err(|error| format!("read authoring observation trace boundary: {error}"))?;
        let mut matched = None;
        for line in trace.lines() {
            if !line.contains("trace_event=\"authoring_observation_boundary\"") {
                continue;
            }
            let parsed = parse_authoring_observation_boundary(line).ok_or_else(|| {
                "authoring observation trace contains a malformed boundary receipt".to_string()
            })?;
            if parsed.request_id != expected.request_id {
                continue;
            }
            if parsed != expected {
                return Err(
                    "authoring observation boundary identity or cursor is stale or swapped".into(),
                );
            }
            if matched.replace(parsed).is_some() {
                return Err("authoring observation boundary receipt is duplicated".into());
            }
        }
        if matched.is_some() {
            return Ok(());
        }
        let now = Instant::now();
        if now >= deadline {
            return Err(
                "timed out waiting for the mailbox-correlated authoring trace boundary".into(),
            );
        }
        std::thread::sleep(Duration::from_millis(10).min(deadline.saturating_duration_since(now)));
    }
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

pub(super) fn authoring_control_events_after(
    trace_path: &Path,
    first_line: usize,
    session_id: u64,
) -> Result<Vec<AuthoringControlSnapshot>, String> {
    let trace = std::fs::read_to_string(trace_path)
        .map_err(|error| format!("read Designer control event trace: {error}"))?;
    Ok(authoring_control_events_in_lines_after(
        &trace_event_lines(&trace),
        first_line,
        session_id,
    ))
}

fn authoring_control_events_in_lines_after(
    lines: &[String],
    first_line: usize,
    session_id: u64,
) -> Vec<AuthoringControlSnapshot> {
    lines
        .iter()
        .skip(first_line)
        .filter_map(|line| parse_authoring_control(line))
        .filter(|control| control.session_id == session_id)
        .collect()
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
    authoring_control_after_snapshot(trace_path, first_line, session_id, target, index, role)
        .map(|(_, control)| control)
}

pub(super) fn authoring_control_after_snapshot(
    trace_path: &Path,
    first_line: usize,
    session_id: u64,
    target: AuthoringControlTarget,
    index: Option<usize>,
    role: AuthoringControlRole,
) -> Result<(usize, Option<AuthoringControlSnapshot>), String> {
    let trace = std::fs::read_to_string(trace_path)
        .map_err(|error| format!("read Designer control trace: {error}"))?;
    // Suite cursors count trace records, not every logger line in the app log. Filter
    // first so the same cursor always denotes the same render epoch in both modules.
    let trace_records = trace_event_lines(&trace);
    let next_cursor = trace_records.len();
    let controls = latest_authoring_controls_after(&trace_records, first_line, session_id);
    Ok((
        next_cursor,
        unique_authoring_control(&controls, target, index, role)?,
    ))
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

pub(super) fn send_control_z_to_focused_window(
    child: &NativeChild,
    target: &WindowSnapshot,
) -> Result<usize, String> {
    child.validate_window(target.hwnd)?;
    focus_is_validated(target.hwnd, child.process_id)?;
    input_modifiers_clear()?;

    let chord = [
        OwnedKeyboardKey {
            vk: VK_LCONTROL,
            extended: false,
        },
        OwnedKeyboardKey {
            vk: VIRTUAL_KEY(b'Z' as u16),
            extended: false,
        },
    ];
    if chord
        .iter()
        .any(|key| unsafe { GetAsyncKeyState(i32::from(key.vk.0)) } < 0)
    {
        return Err("refusing focused Ctrl+Z while one of its keys is already held".into());
    }

    let mut release_guard = OwnedKeyboardReleaseGuard::new();
    let down_events = chord.map(OwnedKeyboardKey::down);
    let down = match send_validated_input_allowing_owned_keys(
        target.hwnd,
        child.process_id,
        &down_events,
        "focused Ctrl+Z chord down",
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
    let up = release_guard.release()?;
    input_modifiers_clear()?;
    Ok(down.inserted.saturating_add(up.inserted))
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

pub(super) fn send_escape_to_focused_window(
    child: &NativeChild,
    target: &WindowSnapshot,
) -> Result<usize, String> {
    child.validate_window(target.hwnd)?;
    focus_is_validated(target.hwnd, child.process_id())?;
    input_modifiers_clear()?;
    send_input_checked(
        &[key_input(VK_ESCAPE, false), key_input(VK_ESCAPE, true)],
        "focused Escape",
    )
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
    mouse_button_input(PointerButton::Left, down)
}

fn mouse_button_input(button: PointerButton, down: bool) -> INPUT {
    let dw_flags = match (button, down) {
        (PointerButton::Left, true) => MOUSEEVENTF_LEFTDOWN,
        (PointerButton::Left, false) => MOUSEEVENTF_LEFTUP,
        (PointerButton::Right, true) => MOUSEEVENTF_RIGHTDOWN,
        (PointerButton::Right, false) => MOUSEEVENTF_RIGHTUP,
    };
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: 0,
                dy: 0,
                mouseData: 0,
                dwFlags: dw_flags,
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
    button: PointerButton,
    owned_keys: Vec<VIRTUAL_KEY>,
    armed: bool,
}

impl MouseButtonGuard {
    fn new(target_hwnd: HWND, target_process_id: u32, button: PointerButton) -> Self {
        Self {
            target_hwnd,
            target_process_id,
            button,
            owned_keys: Vec::new(),
            armed: false,
        }
    }

    fn new_with_owned_keys(
        target_hwnd: HWND,
        target_process_id: u32,
        button: PointerButton,
        owned_keys: &[VIRTUAL_KEY],
    ) -> Self {
        Self {
            target_hwnd,
            target_process_id,
            button,
            owned_keys: owned_keys.to_vec(),
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
        let up = [mouse_button_input(self.button, false)];
        let evidence = send_validated_input_allowing_owned_keys(
            self.target_hwnd,
            self.target_process_id,
            &up,
            "semantic click up",
            &self.owned_keys,
        )
        .map_err(|(_, error)| error)?;
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DesignerScrollbarDragEvidence {
    pub scroll_id: u64,
    pub source_sequence: u64,
    pub hover_sequence: u64,
    pub hover_frame: u64,
    pub start_client: [i32; 2],
    pub end_client: [i32; 2],
    pub down_inserted: usize,
    pub movement_inserted: usize,
    pub up_inserted: usize,
    pub release_fallback_used: bool,
    pub async_state_after_release: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct VerifiedMouseRelease {
    inserted: usize,
    fallback_used: bool,
    async_state_after: u16,
}

fn scrollbar_drag_activation_excursion(
    scroll: &ActionEditorScrollSnapshot,
    start_client: [i32; 2],
    end_client: [i32; 2],
) -> Option<[i32; 2]> {
    const EXCURSION_POINTS_MILLI: i128 = 8_000;
    const DRAG_THRESHOLD_POINTS_MILLI: i128 = 6_000;
    const POINTS_MILLI_SCALE: i128 = 1_000_000;

    if !scroll.is_well_formed()
        || start_client[0] != end_client[0]
        || start_client[1] == end_client[1]
        || !point_in_rect(start_client, scroll.track_visible_bounds)
        || !point_in_rect(end_client, scroll.track_visible_bounds)
    {
        return None;
    }
    let pixels_per_point_milli = i128::from(scroll.pixels_per_point_milli);
    let required_pixels = EXCURSION_POINTS_MILLI
        .saturating_mul(pixels_per_point_milli)
        .saturating_add(POINTS_MILLI_SCALE - 1)
        .checked_div(POINTS_MILLI_SCALE)?;
    let required_pixels = i32::try_from(required_pixels).ok()?;
    let preferred_direction = (end_client[1] - start_client[1]).signum();
    let safe_client = [0, 0, scroll.client_size[0], scroll.client_size[1]];
    for direction in [preferred_direction, -preferred_direction] {
        let excursion = [
            end_client[0],
            end_client[1].saturating_add(direction.saturating_mul(required_pixels)),
        ];
        let movement_milli =
            i128::from(excursion[1].abs_diff(start_client[1])) * POINTS_MILLI_SCALE;
        if point_in_rect(excursion, scroll.track_visible_bounds)
            && point_in_rect(excursion, safe_client)
            && movement_milli > DRAG_THRESHOLD_POINTS_MILLI * pixels_per_point_milli
        {
            return Some(excursion);
        }
    }
    None
}

fn finish_scrollbar_drag_with_verified_release<T>(
    action: impl FnOnce() -> Result<T, String>,
    before_release: impl FnOnce() -> Result<usize, String>,
    release: impl FnOnce() -> Result<VerifiedMouseRelease, String>,
) -> Result<(T, usize, VerifiedMouseRelease), String> {
    let action_result = action();
    let cursor_result = before_release();
    let release_result = release();
    match (action_result, cursor_result, release_result) {
        (Ok(action), Ok(cursor), Ok(release)) => Ok((action, cursor, release)),
        (action, cursor, Ok(release)) => {
            let errors = [action.err(), cursor.err()]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>();
            Err(format!(
                "{}; scrollbar left-up verified (fallback={}, async=0x{:04x})",
                errors.join("; "),
                release.fallback_used,
                release.async_state_after
            ))
        }
        (action, cursor, Err(release_error)) => {
            let errors = [action.err(), cursor.err()]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>();
            if errors.is_empty() {
                Err(release_error)
            } else {
                Err(format!(
                    "{}; scrollbar cleanup failed: {release_error}",
                    errors.join("; ")
                ))
            }
        }
    }
}

fn point_in_rect(point: [i32; 2], rect: [i32; 4]) -> bool {
    point[0] >= rect[0] && point[1] >= rect[1] && point[0] < rect[2] && point[1] < rect[3]
}

fn action_editor_scrollbar_hover_matches(
    expected: &ActionEditorScrollSnapshot,
    latest: &ActionEditorScrollSnapshot,
    start_client: [i32; 2],
    end_client: [i32; 2],
) -> bool {
    latest.is_well_formed()
        && latest.identity == expected.identity
        && latest.scroll_id == expected.scroll_id
        && latest.trace_sequence > expected.trace_sequence
        && latest.frame_nr > expected.frame_nr
        && latest.client_size == expected.client_size
        && latest.same_scroll_owner(expected)
        && latest.offset_y_milli == expected.offset_y_milli
        && latest.velocity_y_milli.unsigned_abs() <= 5_000
        && latest.thumb_bounds == latest.thumb_visible_bounds
        && latest.track_bounds == latest.track_visible_bounds
        && point_in_rect(start_client, latest.thumb_visible_bounds)
        && point_in_rect(end_client, latest.track_visible_bounds)
}

fn async_button_is_released(async_state: i16) -> bool {
    async_state >= 0
}

fn release_mouse_button_verified(
    guard: &mut MouseButtonGuard,
) -> Result<VerifiedMouseRelease, String> {
    let mut normal_release_inserted = 0usize;
    let mut normal_release_error = None;
    match guard.release() {
        Ok(evidence) => normal_release_inserted = evidence.inserted,
        Err(error) => normal_release_error = Some(error),
    }
    // `release` records a successful SendInput edge, not yet a physical-up
    // observation. Keep Drop armed until GetAsyncKeyState verifies the release.
    guard.armed = true;

    let mut state = unsafe { GetAsyncKeyState(VK_LBUTTON.0 as i32) };
    let mut fallback_used = false;
    let mut fallback_inserted = 0usize;
    if state < 0 {
        fallback_used = true;
        match send_input_checked(
            &[mouse_button_input(PointerButton::Left, false)],
            "best-effort scrollbar left-button cleanup",
        ) {
            Ok(inserted) => fallback_inserted = inserted,
            Err(error) => {
                normal_release_error.get_or_insert_with(|| {
                    format!("best-effort left-button cleanup failed: {error}")
                });
            }
        }
    }

    let deadline = Instant::now() + Duration::from_millis(300);
    loop {
        state = unsafe { GetAsyncKeyState(VK_LBUTTON.0 as i32) };
        if async_button_is_released(state) {
            // The explicit async-state observation disarms Drop's retry only
            // after the system confirms that the physical button is up.
            guard.armed = false;
            return Ok(VerifiedMouseRelease {
                inserted: normal_release_inserted.saturating_add(fallback_inserted),
                fallback_used,
                async_state_after: state as u16,
            });
        }
        let now = Instant::now();
        if now >= deadline {
            break;
        }
        std::thread::sleep(WINDOW_POLL.min(deadline.saturating_duration_since(now)));
    }
    Err(format!(
        "scrollbar drag left-button release could not be verified (normal_release={normal_release_error:?}, fallback_used={fallback_used}, fallback_inserted={fallback_inserted}, async_state=0x{:04x}); guarded Drop will retry",
        state as u16
    ))
}

fn designer_client_point_to_screen(
    child: &NativeChild,
    target: &WindowSnapshot,
    point: [i32; 2],
) -> Result<POINT, String> {
    child.validate_window(target.hwnd)?;
    let bounds = child.client_screen_bounds(target)?;
    let client = child.client_bounds(target)?;
    if !point_in_rect(point, [client[0], client[1], client[2], client[3]]) {
        return Err("scrollbar pointer point is outside the live Designer client".into());
    }
    Ok(POINT {
        x: bounds[0].saturating_add(point[0]),
        y: bounds[1].saturating_add(point[1]),
    })
}

fn send_designer_pointer_move(
    child: &NativeChild,
    target: &WindowSnapshot,
    trace_path: &Path,
    point: [i32; 2],
    operation: &str,
) -> Result<(usize, POINT), String> {
    child.validate_window(target.hwnd)?;
    if target.role != WindowRole::Designer || target.process_id != child.process_id() {
        return Err("scrollbar pointer move is outside the candidate-owned Designer".into());
    }
    if unsafe { GetForegroundWindow() } != target.hwnd {
        return Err("Designer lost foreground ownership before scrollbar pointer movement".into());
    }
    focus_is_validated(target.hwnd, child.process_id())?;
    let screen = designer_client_point_to_screen(child, target, point)?;
    validate_pointer_coverage(target.hwnd, child.process_id(), screen, operation)?;
    let trace_cursor = trace_line_count(trace_path)?;
    unsafe { SetCursorPos(screen.x, screen.y) }
        .map_err(|error| format!("position pointer over measured scrollbar thumb: {error}"))?;
    let movement = [mouse_move_input(screen)?];
    let inserted =
        send_validated_input(target.hwnd, child.process_id(), &movement, operation)?.inserted;
    let acknowledgement = PointerMoveAcknowledgement {
        trace_path,
        kind: PointerTraceKind::DesignerClient,
        nudge_screen_point: screen,
        nudge_trace_point: (point[0], point[1]),
        target_trace_point: (point[0], point[1]),
    };
    wait_for_pointer_move_ack(
        child,
        target,
        operation,
        &acknowledgement,
        trace_cursor,
        (point[0], point[1]),
        screen,
        Duration::from_secs(2),
    )?;
    Ok((inserted, screen))
}

fn designer_pointer_up_matches_owner(line: &str, session_id: u64, hwnd: u64) -> bool {
    line.contains("trace_event=\"designer_pointer\"")
        && trace_bool_field(line, "pointer_up") == Some(true)
        && trace_field(line, "session_id").and_then(|value| value.parse::<u64>().ok())
            == Some(session_id)
        && trace_field(line, "window_under_cursor_hwnd").and_then(|value| value.parse::<u64>().ok())
            == Some(hwnd)
}

fn scrollbar_pointer_button_ack_order_after(
    trace: &str,
    cursor: usize,
    screen_point: (i32, i32),
    owned_hwnd: u64,
    expected_session_id: u64,
    expected_draft_generation: u64,
) -> Result<PointerButtonAckOrder, String> {
    let mut down = None;
    let mut up_before_down = false;
    for (index, line) in trace.lines().enumerate().skip(cursor) {
        if down.is_none() {
            if pointer_button_ack_matches(
                line,
                PointerTraceKind::DesignerClient,
                screen_point,
                true,
            ) {
                let hwnd = trace_field(line, "window_under_cursor_hwnd")
                    .and_then(|value| value.parse::<u64>().ok())
                    .ok_or_else(|| {
                        "fresh Designer scrollbar down acknowledgement has no owned HWND".to_owned()
                    })?;
                if hwnd != owned_hwnd {
                    return Err(format!(
                        "fresh Designer scrollbar down acknowledgement belonged to HWND={hwnd}, expected owned HWND={owned_hwnd}"
                    ));
                }
                let session_id = trace_field(line, "session_id")
                    .and_then(|value| value.parse::<u64>().ok())
                    .ok_or_else(|| {
                        "fresh Designer scrollbar down acknowledgement has no owned session"
                            .to_owned()
                    })?;
                let draft_generation = trace_field(line, "generation")
                    .and_then(|value| value.parse::<u64>().ok())
                    .ok_or_else(|| {
                        "fresh Designer scrollbar down acknowledgement has no draft generation"
                            .to_owned()
                    })?;
                if session_id != expected_session_id
                    || draft_generation != expected_draft_generation
                {
                    return Err(format!(
                        "fresh Designer scrollbar down acknowledgement belonged to session={session_id} generation={draft_generation}, expected session={expected_session_id} generation={expected_draft_generation}"
                    ));
                }
                down = Some((index, session_id, hwnd));
                if designer_pointer_up_matches_owner(line, session_id, hwnd) {
                    return Ok(PointerButtonAckOrder::UpAfterDown);
                }
            } else if pointer_button_ack_matches(
                line,
                PointerTraceKind::DesignerClient,
                screen_point,
                false,
            ) {
                up_before_down = true;
            }
            continue;
        }

        let Some((_, session_id, hwnd)) = down else {
            continue;
        };
        if designer_pointer_up_matches_owner(line, session_id, hwnd) {
            return Ok(PointerButtonAckOrder::UpAfterDown);
        }
    }

    Ok(match (down, up_before_down) {
        (Some(_), true) => PointerButtonAckOrder::UpBeforeDown,
        (Some(_), false) => PointerButtonAckOrder::Down,
        (None, true) => PointerButtonAckOrder::UpBeforeDown,
        (None, false) => PointerButtonAckOrder::Pending,
    })
}

fn ensure_scrollbar_button_still_down_with(
    trace: &str,
    cursor: usize,
    screen_point: (i32, i32),
    owned_hwnd: u64,
    expected_session_id: u64,
    expected_draft_generation: u64,
    async_state: i16,
) -> Result<(), String> {
    match scrollbar_pointer_button_ack_order_after(
        trace,
        cursor,
        screen_point,
        owned_hwnd,
        expected_session_id,
        expected_draft_generation,
    )? {
        PointerButtonAckOrder::Down => {}
        PointerButtonAckOrder::Pending => {
            return Err("fresh production scrollbar down acknowledgement disappeared".into());
        }
        PointerButtonAckOrder::UpBeforeDown => {
            return Err("production scrollbar up preceded its down acknowledgement".into());
        }
        PointerButtonAckOrder::UpAfterDown => {
            return Err(
                "production scrollbar button was externally released during the drag".into(),
            );
        }
    }
    if async_state >= 0 {
        return Err(format!(
            "physical scrollbar left-button was released during the drag (async=0x{:04x})",
            async_state as u16
        ));
    }
    Ok(())
}

fn ensure_scrollbar_button_still_down(
    acknowledgement: &PointerMoveAcknowledgement<'_>,
    cursor: usize,
    screen_point: (i32, i32),
    owned_hwnd: u64,
    expected: &ActionEditorScrollSnapshot,
) -> Result<(), String> {
    let trace = std::fs::read_to_string(acknowledgement.trace_path)
        .map_err(|error| format!("read production pointer-button trace: {error}"))?;
    ensure_scrollbar_button_still_down_with(
        &trace,
        cursor,
        screen_point,
        owned_hwnd,
        expected.identity.session_id,
        expected.identity.draft_generation,
        unsafe { GetAsyncKeyState(VK_LBUTTON.0 as i32) },
    )
}

fn send_held_scrollbar_drag_move(
    child: &NativeChild,
    target: &WindowSnapshot,
    acknowledgement: &PointerMoveAcknowledgement<'_>,
    down_cursor: usize,
    down_screen_point: (i32, i32),
    owned_hwnd: u64,
    expected: &ActionEditorScrollSnapshot,
    trace_path: &Path,
    point: [i32; 2],
    operation: &str,
) -> Result<(usize, POINT), String> {
    child.validate_window(target.hwnd)?;
    if unsafe { GetForegroundWindow() } != target.hwnd {
        return Err("Designer lost foreground ownership during scrollbar thumb drag".into());
    }
    ensure_scrollbar_button_still_down(
        acknowledgement,
        down_cursor,
        down_screen_point,
        owned_hwnd,
        expected,
    )?;
    let (inserted, screen) =
        send_designer_pointer_move(child, target, trace_path, point, operation)?;
    ensure_scrollbar_button_still_down(
        acknowledgement,
        down_cursor,
        down_screen_point,
        owned_hwnd,
        expected,
    )?;
    Ok((inserted, screen))
}

pub(super) fn drag_designer_scrollbar_thumb(
    child: &NativeChild,
    target: &WindowSnapshot,
    expected: &ActionEditorScrollSnapshot,
    start_client: [i32; 2],
    end_client: [i32; 2],
    trace_path: &Path,
    timeout: Duration,
) -> Result<Option<DesignerScrollbarDragEvidence>, String> {
    if !expected.is_well_formed()
        || target.role != WindowRole::Designer
        || target.process_id != child.process_id()
        || !target.visible
        || target.minimized
        || expected.identity.surface != ActionEditorSurface::Inspector
        || expected.thumb_bounds != expected.thumb_visible_bounds
        || expected.track_bounds != expected.track_visible_bounds
        || !point_in_rect(start_client, expected.thumb_visible_bounds)
        || !point_in_rect(end_client, expected.track_visible_bounds)
        || start_client == end_client
    {
        return Err("refused scrollbar drag without a complete owned thumb/track receipt".into());
    }
    child.validate_window(target.hwnd)?;
    if child.designer().is_none_or(|live| live.hwnd != target.hwnd) {
        return Err("the scrollbar owner is no longer the live child Designer window".into());
    }
    let initial_bounds = child.client_bounds(target)?;
    let live_size = [
        initial_bounds[2] - initial_bounds[0],
        initial_bounds[3] - initial_bounds[1],
    ];
    if live_size != expected.client_size {
        return Err("scrollbar receipt client size differs from the live Designer client".into());
    }
    if unsafe { GetForegroundWindow() } != target.hwnd {
        return Err("Designer lost foreground ownership before scrollbar drag".into());
    }
    focus_is_validated(target.hwnd, child.process_id())?;
    if unsafe { GetAsyncKeyState(VK_LBUTTON.0 as i32) } < 0 {
        return Err("refused scrollbar drag while the left mouse button was already down".into());
    }

    let hover_event_cursor = trace_event_lines(
        &std::fs::read_to_string(trace_path)
            .map_err(|error| format!("read trace before scrollbar hover: {error}"))?,
    )
    .len();
    let _ = send_designer_pointer_move(
        child,
        target,
        trace_path,
        start_client,
        "hover measured scrollbar thumb",
    )?;
    let requested_size = child.request_designer_repaint(target)?;
    if requested_size != expected.client_size {
        return Err("Designer client changed during scrollbar hover refresh".into());
    }

    let deadline = Instant::now() + timeout;
    let mut last_refresh = Instant::now();
    let hovered = loop {
        if let Some(latest) = latest_action_editor_scroll_after(
            trace_path,
            hover_event_cursor,
            expected.identity.surface,
        )? {
            if !action_editor_scrollbar_hover_matches(expected, &latest, start_client, end_client) {
                return Ok(None);
            }
            break latest;
        }
        let now = Instant::now();
        if now >= deadline {
            return Ok(None);
        }
        if last_refresh.elapsed() >= ACTION_EDITOR_SCROLL_REFRESH_INTERVAL {
            child.validate_window(target.hwnd)?;
            if !child.foreground_is_child() {
                return Err(
                    "Designer lost process foreground during scrollbar hover refresh".into(),
                );
            }
            if child.request_designer_repaint(target)? != expected.client_size {
                return Err("Designer client changed during scrollbar hover refresh".into());
            }
            last_refresh = Instant::now();
        }
        std::thread::sleep(WINDOW_POLL.min(deadline.saturating_duration_since(Instant::now())));
    };

    // Re-read the latest same-surface event immediately before button down so
    // a newer frame that moved the floating bar cannot be hidden by the
    // earlier hover receipt.
    let latest_now = latest_action_editor_scroll_after(
        trace_path,
        hover_event_cursor,
        expected.identity.surface,
    )?
    .ok_or_else(|| "fresh scrollbar hover receipt disappeared before button down".to_string())?;
    if latest_now != hovered
        || !action_editor_scrollbar_hover_matches(expected, &latest_now, start_client, end_client)
    {
        return Ok(None);
    }
    child.validate_window(target.hwnd)?;
    if unsafe { GetForegroundWindow() } != target.hwnd {
        return Err("Designer lost foreground ownership immediately before scrollbar down".into());
    }
    let start_screen = designer_client_point_to_screen(child, target, start_client)?;
    let owned_pointer_hwnd = validate_pointer_coverage(
        target.hwnd,
        child.process_id(),
        start_screen,
        "scrollbar thumb down",
    )?;
    let button_ack = PointerMoveAcknowledgement {
        trace_path,
        kind: PointerTraceKind::DesignerClient,
        nudge_screen_point: start_screen,
        nudge_trace_point: (start_client[0], start_client[1]),
        target_trace_point: (end_client[0], end_client[1]),
    };
    let down_cursor = trace_line_count(trace_path)?;
    let mut guard = MouseButtonGuard::new(target.hwnd, child.process_id(), PointerButton::Left);
    let down = send_validated_input(
        target.hwnd,
        child.process_id(),
        &[mouse_button_input(PointerButton::Left, true)],
        "measured scrollbar thumb down",
    )?;
    guard.armed = down.inserted > 0;
    if down.inserted != 1 {
        return Err(format!(
            "scrollbar thumb down inserted {} events instead of one",
            down.inserted
        ));
    }
    let ((movement_inserted, end_screen), up_cursor, release) =
        finish_scrollbar_drag_with_verified_release(
            || {
                wait_for_pointer_button_down_ready(
                    &button_ack,
                    down_cursor,
                    (start_screen.x, start_screen.y),
                    VK_LBUTTON.0 as i32,
                    Duration::from_secs(2),
                )?;
                let excursion =
                    scrollbar_drag_activation_excursion(expected, start_client, end_client)
                        .ok_or_else(|| {
                            "no safe scrollbar activation excursion exceeds the egui drag threshold"
                                .to_owned()
                        })?;
                let (excursion_inserted, _) = send_held_scrollbar_drag_move(
                    child,
                    target,
                    &button_ack,
                    down_cursor,
                    (start_screen.x, start_screen.y),
                    hwnd_id(owned_pointer_hwnd),
                    expected,
                    trace_path,
                    excursion,
                    "decisive measured scrollbar drag excursion",
                )?;
                let (end_inserted, end_screen) = send_held_scrollbar_drag_move(
                    child,
                    target,
                    &button_ack,
                    down_cursor,
                    (start_screen.x, start_screen.y),
                    hwnd_id(owned_pointer_hwnd),
                    expected,
                    trace_path,
                    end_client,
                    "measured scrollbar drag endpoint",
                )?;
                Ok((excursion_inserted.saturating_add(end_inserted), end_screen))
            },
            || trace_line_count(trace_path),
            || release_mouse_button_verified(&mut guard),
        )?;
    // The button-up trace must correspond to the final physical cursor point.
    wait_for_pointer_button_ack(
        &button_ack,
        up_cursor,
        (end_screen.x, end_screen.y),
        false,
        Duration::from_secs(2),
    )?;
    if release.async_state_after & 0x8000 != 0 {
        return Err("left-button release returned with the async state still down".into());
    }
    Ok(Some(DesignerScrollbarDragEvidence {
        scroll_id: expected.scroll_id,
        source_sequence: expected.trace_sequence,
        hover_sequence: hovered.trace_sequence,
        hover_frame: hovered.frame_nr,
        start_client,
        end_client,
        down_inserted: down.inserted,
        movement_inserted,
        up_inserted: release.inserted,
        release_fallback_used: release.fallback_used,
        async_state_after_release: release.async_state_after,
    }))
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
        ACCEPTANCE_RUNNER_INPUT_COOKIE, AcceptanceTraceBudgetProfile, ActionCatalogRankSnapshot,
        ActionEditorProviderEdge, ActionEditorProviderKind, ActionEditorSurface,
        AuthoringControlRole, AuthoringControlSnapshot, AuthoringControlTarget,
        AuthoringObservationBoundarySnapshot, FocusAnchorCommand, FocusAnchorCommandKind,
        GetCurrentThreadId, INPUT_MOUSE, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, LPARAM,
        MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_MOVE, MOUSEEVENTF_MOVE_NOCOALESCE, OwnedKeyboardKey,
        POINT, PointerReleaseWaitError, PointerTraceKind, PostThreadMessageW, RUNNER_HOOK_EVENTS,
        RadialPointerReleaseAck, RunnerChordEdge, RunnerChordKeyObservation,
        RunnerChordObservation, RunnerHookEdge, RunnerHookObserver, SS_NOTIFY,
        UIA_ELEMENT_NOT_AVAILABLE_HRESULT, UIA_NOT_SUPPORTED_HRESULT, VK_END, VK_LMENU, VK_LSHIFT,
        VK_LWIN, VisibleTextLookupError, WPARAM, acceptance_environment_block,
        active_input_desktop_name, adjacent_pointer_point, attach_to_input_desktop,
        authoring_control_click_finished, authoring_control_events_after,
        classify_element_property_hresult, cursor_points_match, cursor_restore_input_target,
        desktop_name, finish_focus_anchor_ui_thread, focus_anchor_candidate_positions,
        focus_anchor_window_style, format_uia_element_snapshot, forward_runner_hook_edge,
        fresh_canvas_cell_for_generation, hwnd_id, keys_down_without_owned_keydowns,
        latest_authoring_controls_after, latest_pointer_move_after, normalized_absolute_coordinate,
        parse_action_catalog_rank, parse_action_editor_control, parse_action_editor_provider_event,
        parse_authoring_control, parse_geometry_state, pointer_correction_delta,
        radial_pointer_release_ack_after, record_runner_chord_edge, relative_mouse_move_input,
        retire_inserted_keyboard_ups, semantic_client_center, semantic_name_contains,
        send_pointer_correction_after_cursor, spawn_focus_anchor_window,
        spawn_focus_anchor_window_with_startup_hooks, trace_event_lines, unique_authoring_control,
        wait_for_authoring_observation_boundary, wait_for_pointer_move_ack_with,
        wait_for_pointer_release_settle, wait_visible_text_with, window_process_id,
    };
    use std::time::Duration;

    fn owned_pointer_release_fixture() -> (
        super::DesignerPointerReleaseBoundary,
        super::PointerClickEvidence,
        Vec<String>,
    ) {
        let hwnd = super::HWND(201197638usize as *mut _);
        let edge = super::NativeInputEdgeEvidence {
            inserted: 1,
            at_unix_ms: 1,
            foreground_hwnd: super::hwnd_id(hwnd),
            foreground_pid: 7,
            input_desktop: "Default".into(),
            cleanup_status: "verified".into(),
            keyboard_input: None,
        };
        let click = super::PointerClickEvidence {
            nudge_movement: edge.clone(),
            movement: edge.clone(),
            pointer_correction_events: 0,
            pointer_position_preexisting_ack: false,
            pointer_move_acknowledged: true,
            down: edge.clone(),
            down_acknowledged: true,
            button: super::PointerButton::Left,
            button_state_after_down: i16::MIN,
            button_state_before_up: i16::MIN,
            up: edge,
            up_acknowledged: true,
            button_state_after_up: 0,
            target_hwnd: hwnd,
            nudge_under_cursor_hwnd: hwnd,
            under_cursor_hwnd: hwnd,
            foreground_hwnd: hwnd,
            screen_point: (874, 366),
        };
        let boundary = super::DesignerPointerReleaseBoundary {
            first_line: 0,
            after_trace_sequence: 580,
            through_trace_sequence: 608,
            session_id: 1,
            generation: 2,
            target_hwnd: super::hwnd_id(hwnd),
        };
        // Retained candidate-1 S02 producer records, with no authored text,
        // paths, process nonce or other private document content.
        let lines = [
            "trace_event=designer_pointer elapsed_ms=7547 pointer_down=true pointer_up=false trace_sequence=588 window_under_cursor_hwnd=201197638 window_under_cursor_owner=Other cursor_screen_x=874 cursor_screen_y=366 request_id=0 request_kind=None session_id=1 generation=2 terminal=false",
            "trace_event=designer_pointer elapsed_ms=7599 pointer_down=false pointer_up=true trace_sequence=607 window_under_cursor_hwnd=201197638 window_under_cursor_owner=Other cursor_screen_x=874 cursor_screen_y=366 request_id=0 request_kind=None session_id=1 generation=2 terminal=false",
        ].map(str::to_owned).to_vec();
        (boundary, click, lines)
    }

    #[test]
    fn owned_designer_pointer_release_accepts_retained_producer_and_live_quoted_format() {
        let (boundary, click, lines) = owned_pointer_release_fixture();
        for lines in [
            lines.clone(),
            lines
                .iter()
                .map(|line| {
                    line.replacen(
                        "trace_event=designer_pointer",
                        "trace_event=\"designer_pointer\"",
                        1,
                    )
                })
                .collect(),
        ] {
            let receipt = super::owned_designer_pointer_release_after(&lines, boundary, &click)
                .expect("actual producer fields form the checked owned click");
            assert_eq!(receipt.trace_sequence, 607);
            let after_rename = super::DesignerPointerReleaseBoundary {
                through_trace_sequence: 682,
                ..boundary
            };
            assert_eq!(
                super::owned_designer_pointer_release_after(&lines, after_rename, &click).unwrap(),
                receipt,
                "post-input generation changes do not replace the captured generation"
            );
        }
        let mut quantized = lines;
        quantized[1] = quantized[1].replace("cursor_screen_x=874", "cursor_screen_x=875");
        assert!(super::owned_designer_pointer_release_after(&quantized, boundary, &click).is_ok());
    }

    #[test]
    fn owned_designer_pointer_release_rejects_wrong_owner_and_stale_boundaries() {
        let (boundary, click, lines) = owned_pointer_release_fixture();
        for (field, wrong) in [
            ("session_id=1", "session_id=2"),
            ("generation=2", "generation=3"),
            (
                "window_under_cursor_hwnd=201197638",
                "window_under_cursor_hwnd=201197639",
            ),
            ("cursor_screen_x=874", "cursor_screen_x=876"),
            ("cursor_screen_y=366", "cursor_screen_y=368"),
            ("trace_sequence=607", "trace_sequence=580"),
            ("trace_sequence=607", "trace_sequence=609"),
        ] {
            let mut wrong_lines = lines.clone();
            wrong_lines[1] = wrong_lines[1].replace(field, wrong);
            assert!(
                super::owned_designer_pointer_release_after(&wrong_lines, boundary, &click)
                    .is_err(),
                "wrong release must be rejected: {wrong}"
            );
        }
        for stale in [
            super::DesignerPointerReleaseBoundary {
                first_line: 1,
                ..boundary
            },
            super::DesignerPointerReleaseBoundary {
                after_trace_sequence: 607,
                ..boundary
            },
            super::DesignerPointerReleaseBoundary {
                through_trace_sequence: 606,
                ..boundary
            },
            super::DesignerPointerReleaseBoundary {
                generation: 3,
                ..boundary
            },
            super::DesignerPointerReleaseBoundary {
                session_id: 0,
                ..boundary
            },
            super::DesignerPointerReleaseBoundary {
                target_hwnd: 0,
                ..boundary
            },
        ] {
            assert!(super::owned_designer_pointer_release_after(&lines, stale, &click).is_err());
        }
    }

    #[test]
    fn owned_designer_pointer_release_rejects_missing_malformed_and_unordered_edges() {
        let (boundary, click, lines) = owned_pointer_release_fixture();
        for field in [
            "trace_event",
            "trace_sequence",
            "session_id",
            "generation",
            "window_under_cursor_hwnd",
            "cursor_screen_x",
            "cursor_screen_y",
            "pointer_down",
            "pointer_up",
        ] {
            let mut missing = lines.clone();
            missing[1] = missing[1]
                .split_ascii_whitespace()
                .filter(|part| !part.starts_with(&format!("{field}=")))
                .collect::<Vec<_>>()
                .join(" ");
            assert!(
                super::owned_designer_pointer_release_after(&missing, boundary, &click).is_err(),
                "missing {field} cannot prove a release"
            );
        }
        for (field, wrong) in [
            (
                "trace_event=designer_pointer",
                "trace_event=root_pointer_button",
            ),
            (
                "trace_event=designer_pointer",
                "trace_event=\"designer_pointer_moved\"",
            ),
            ("pointer_up=true", "up=true"),
            ("pointer_up=true", "pointer_up=1"),
            ("pointer_down=false", "pointer_down=\"false\""),
            ("pointer_down=false", "pointer_down=true"),
            ("pointer_up=true", "pointer_up=false"),
            ("session_id=1", "session_id=1.0"),
            ("generation=2", "generation=unknown"),
            (
                "window_under_cursor_hwnd=201197638",
                "window_under_cursor_hwnd=Other",
            ),
            ("cursor_screen_x=874", "cursor_screen_x=874.0"),
            ("trace_sequence=607", "trace_sequence=607.0"),
            ("trace_sequence=607", "trace_sequence=588"),
        ] {
            let mut malformed = lines.clone();
            malformed[1] = malformed[1].replace(field, wrong);
            assert!(
                super::owned_designer_pointer_release_after(&malformed, boundary, &click).is_err(),
                "malformed edge cannot prove a release: {wrong}"
            );
        }
        for unordered in [
            vec![lines[1].clone(), lines[0].clone()],
            vec![lines[0].clone(), lines[0].clone(), lines[1].clone()],
            vec![lines[0].clone()],
            vec![lines[1].clone()],
        ] {
            assert!(
                super::owned_designer_pointer_release_after(&unordered, boundary, &click).is_err()
            );
        }
    }

    #[test]
    fn owned_designer_pointer_release_requires_the_checked_primary_input_owner() {
        let (boundary, click, lines) = owned_pointer_release_fixture();
        let mutations: &[fn(&mut super::PointerClickEvidence)] = &[
            |click| click.button = super::PointerButton::Right,
            |click| click.pointer_move_acknowledged = false,
            |click| click.down_acknowledged = false,
            |click| click.up_acknowledged = false,
            |click| click.down.inserted = 0,
            |click| click.down.inserted = 2,
            |click| click.up.inserted = 0,
            |click| click.up.inserted = 2,
            |click| click.target_hwnd = super::HWND(9usize as *mut _),
            |click| click.under_cursor_hwnd = super::HWND::default(),
            |click| click.foreground_hwnd = super::HWND(9usize as *mut _),
            |click| click.down.foreground_hwnd = 9,
            |click| click.up.foreground_hwnd = 9,
        ];
        for mutate in mutations {
            let mut wrong_click = click.clone();
            mutate(&mut wrong_click);
            assert!(
                super::owned_designer_pointer_release_after(&lines, boundary, &wrong_click)
                    .is_err()
            );
        }
    }

    #[test]
    fn scrollbar_down_waits_for_fresh_ack_then_accepts_delayed_physical_down() {
        let started = std::time::Instant::now();
        let elapsed = std::cell::Cell::new(Duration::ZERO);
        let acknowledged = std::cell::Cell::new(false);
        let mut samples = [0, 0, i16::MIN].into_iter();
        let state = super::wait_for_pointer_button_down_ready_with(
            || {
                acknowledged.set(true);
                Ok(())
            },
            Duration::from_millis(50),
            || started + elapsed.get(),
            |duration| elapsed.set(elapsed.get() + duration),
            || {
                assert!(
                    acknowledged.get(),
                    "physical state is sampled after GUI ack"
                );
                Ok((samples.next().expect("three deterministic samples"), false))
            },
        )
        .expect("a physical down that arrives after the production ack is accepted");
        assert_eq!(state, i16::MIN);
    }

    #[test]
    fn scrollbar_down_rejects_persistent_up_state_and_up_before_down_ack() {
        let started = std::time::Instant::now();
        let elapsed = std::cell::Cell::new(Duration::ZERO);
        let acknowledged = std::cell::Cell::new(false);
        let mut sample_count = 0;
        let persistent_up = super::wait_for_pointer_button_down_ready_with(
            || {
                acknowledged.set(true);
                Ok(())
            },
            Duration::from_millis(50),
            || started + elapsed.get(),
            |duration| elapsed.set(elapsed.get() + duration),
            || {
                assert!(acknowledged.get());
                sample_count += 1;
                Ok((0, false))
            },
        );
        assert!(persistent_up.unwrap_err().contains("not observed"));
        assert_eq!(sample_count, 3);

        let release_before_down = concat!(
            "trace_event=\"designer_pointer\" pointer_up=true cursor_screen_x=959 cursor_screen_y=604\n",
            "trace_event=\"designer_pointer\" pointer_down=true cursor_screen_x=959 cursor_screen_y=604\n"
        );
        let order = super::pointer_button_ack_order_after(
            release_before_down,
            0,
            super::PointerTraceKind::DesignerClient,
            (959, 604),
        );
        assert_eq!(order, super::PointerButtonAckOrder::UpBeforeDown);
        assert!(super::pointer_button_down_acknowledged(order).is_err());
        assert_eq!(
            super::pointer_button_ack_order_after(
                release_before_down,
                0,
                super::PointerTraceKind::DesignerClient,
                (961, 604),
            ),
            super::PointerButtonAckOrder::Pending,
            "a fresh edge at another screen point is not this thumb's acknowledgement"
        );
    }

    #[test]
    fn failed_scrollbar_down_ack_still_runs_verified_release() {
        let mut release_attempted = false;
        let result = super::finish_scrollbar_drag_with_verified_release(
            || Err::<(), _>("fresh down ack timed out".into()),
            || Ok(12),
            || {
                release_attempted = true;
                Ok(super::VerifiedMouseRelease {
                    inserted: 1,
                    fallback_used: false,
                    async_state_after: 0,
                })
            },
        );
        let error = result.unwrap_err();
        assert!(error.contains("fresh down ack timed out"));
        assert!(error.contains("left-up verified"));
        assert!(release_attempted);
    }

    #[test]
    fn scrollbar_external_up_after_down_ack_fails_closed_and_releases() {
        let mut release_attempted = false;
        let started = std::time::Instant::now();
        let result = super::finish_scrollbar_drag_with_verified_release(
            || {
                super::wait_for_pointer_button_down_ready_with(
                    || Ok(()),
                    Duration::from_secs(1),
                    || started,
                    |_| {},
                    || Ok((i16::MIN, true)),
                )
            },
            || Ok(13),
            || {
                release_attempted = true;
                Ok(super::VerifiedMouseRelease {
                    inserted: 1,
                    fallback_used: false,
                    async_state_after: 0,
                })
            },
        );
        let error = result.unwrap_err();
        assert!(error.contains("pointer-button up arrived"));
        assert!(error.contains("left-up verified"));
        assert!(release_attempted);
    }

    #[test]
    fn scrollbar_drag_rejects_mismatched_down_identity_and_releases_before_excursion() {
        for (down_session, down_generation) in [(16, 3), (15, 4)] {
            let trace = format!(
                "trace_event=\"designer_pointer\" pointer_down=true pointer_up=false window_under_cursor_hwnd=41 cursor_screen_x=960 cursor_screen_y=600 session_id={down_session} generation={down_generation}\n"
            );
            let mut excursion_started = false;
            let mut release_attempted = false;
            let result = super::finish_scrollbar_drag_with_verified_release(
                || {
                    super::ensure_scrollbar_button_still_down_with(
                        &trace,
                        0,
                        (960, 600),
                        41,
                        15,
                        3,
                        i16::MIN,
                    )?;
                    excursion_started = true;
                    Ok(())
                },
                || Ok(4),
                || {
                    release_attempted = true;
                    Ok(super::VerifiedMouseRelease {
                        inserted: 1,
                        fallback_used: false,
                        async_state_after: 0,
                    })
                },
            );

            let error = result.unwrap_err();
            assert!(
                error.contains(&format!(
                    "belonged to session={down_session} generation={down_generation}, expected session=15 generation=3"
                )),
                "{error}"
            );
            assert!(error.contains("left-up verified"), "{error}");
            assert!(
                !excursion_started,
                "mismatched Down must fail before movement"
            );
            assert!(
                release_attempted,
                "rejected Down must still trigger cleanup"
            );
        }
    }

    #[test]
    fn scrollbar_drag_rejects_owned_up_at_excursion_or_endpoint_even_if_async_is_down() {
        for (release_x, release_y) in [(964, 620), (970, 650)] {
            let trace = format!(
                concat!(
                    "trace_event=\"designer_pointer\" pointer_down=true pointer_up=false ",
                    "window_under_cursor_hwnd=41 cursor_screen_x=960 cursor_screen_y=600 session_id=9 generation=3\n",
                    "trace_event=\"designer_pointer_moved\" client_x=964 client_y=620\n",
                    "trace_event=\"designer_pointer\" pointer_down=false pointer_up=true ",
                    "window_under_cursor_hwnd=41 cursor_screen_x={} cursor_screen_y={} session_id=9 generation=3\n",
                ),
                release_x, release_y
            );
            let mut release_attempted = false;
            let result = super::finish_scrollbar_drag_with_verified_release(
                || {
                    super::ensure_scrollbar_button_still_down_with(
                        &trace,
                        0,
                        (960, 600),
                        41,
                        9,
                        3,
                        i16::MIN,
                    )
                },
                || Ok(7),
                || {
                    release_attempted = true;
                    Ok(super::VerifiedMouseRelease {
                        inserted: 1,
                        fallback_used: false,
                        async_state_after: 0,
                    })
                },
            );
            let error = result.unwrap_err();
            assert!(error.contains("externally released during the drag"));
            assert!(error.contains("left-up verified"));
            assert!(
                release_attempted,
                "failed hold validation must release the button"
            );
        }
    }

    #[test]
    fn scrollbar_drag_accepts_held_move_and_ignores_other_pointer_owners() {
        let trace = concat!(
            "trace_event=\"designer_pointer\" pointer_down=true pointer_up=false window_under_cursor_hwnd=41 cursor_screen_x=960 cursor_screen_y=600 session_id=9 generation=3\n",
            "trace_event=\"designer_pointer_moved\" client_x=964 client_y=620\n",
            "trace_event=\"designer_pointer\" pointer_down=false pointer_up=true window_under_cursor_hwnd=42 cursor_screen_x=964 cursor_screen_y=620 session_id=9 generation=3\n",
            "trace_event=\"designer_pointer\" pointer_down=false pointer_up=true window_under_cursor_hwnd=41 cursor_screen_x=970 cursor_screen_y=650 session_id=8 generation=3\n",
            "trace_event=\"designer_pointer_moved\" client_x=970 client_y=650\n",
        );

        super::ensure_scrollbar_button_still_down_with(trace, 0, (960, 600), 41, 9, 3, i16::MIN)
            .expect("an exact-owner held move with no later Up remains valid");
    }

    #[test]
    fn scrollbar_drag_preserves_pointer_edge_ordering() {
        let trace = concat!(
            "trace_event=\"designer_pointer\" pointer_down=false pointer_up=true window_under_cursor_hwnd=41 cursor_screen_x=960 cursor_screen_y=600 session_id=9 generation=3\n",
            "trace_event=\"designer_pointer\" pointer_down=true pointer_up=false window_under_cursor_hwnd=41 cursor_screen_x=960 cursor_screen_y=600 session_id=9 generation=3\n",
        );

        assert_eq!(
            super::scrollbar_pointer_button_ack_order_after(trace, 0, (960, 600), 41, 9, 3),
            Ok(super::PointerButtonAckOrder::UpBeforeDown)
        );

        let same_sample = concat!(
            "trace_event=\"designer_pointer\" pointer_down=true pointer_up=true window_under_cursor_hwnd=41 cursor_screen_x=960 cursor_screen_y=600 session_id=9 generation=3\n",
        );
        assert_eq!(
            super::scrollbar_pointer_button_ack_order_after(same_sample, 0, (960, 600), 41, 9, 3),
            Ok(super::PointerButtonAckOrder::UpAfterDown)
        );
    }

    #[test]
    fn gate_c_trace_budget_profile_is_opt_in_only_for_its_child_environment() {
        fn values(block: &[u16], key: &str) -> Vec<String> {
            block
                .split(|unit| *unit == 0)
                .filter_map(|entry| {
                    let value = String::from_utf16(entry).ok()?;
                    let (name, value) = value.split_once('=')?;
                    name.eq_ignore_ascii_case(key).then(|| value.to_owned())
                })
                .collect()
        }

        let profile = tempfile::tempdir().unwrap();
        let standard =
            acceptance_environment_block(profile.path(), AcceptanceTraceBudgetProfile::Standard);
        let gate_c =
            acceptance_environment_block(profile.path(), AcceptanceTraceBudgetProfile::GateC);
        let gate_d =
            acceptance_environment_block(profile.path(), AcceptanceTraceBudgetProfile::GateD);
        assert!(values(&standard, super::TRACE_BUDGET_PROFILE_ENV).is_empty());
        assert_eq!(
            values(&gate_c, super::TRACE_BUDGET_PROFILE_ENV),
            vec!["gate_c_v1".to_owned()]
        );
        assert_eq!(
            values(&gate_d, super::TRACE_BUDGET_PROFILE_ENV),
            vec!["gate_d_v1".to_owned()]
        );
        assert_eq!(
            super::AcceptanceTraceBudgetProfile::Standard.environment_value(),
            None
        );
    }

    #[test]
    fn authoring_boundary_wait_requires_exact_ordered_mailbox_receipt() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("acceptance.log");
        let expected = AuthoringObservationBoundarySnapshot {
            phase: "terminal",
            request_id: 12,
            baseline_request_id: Some(9),
            captured_trace_sequence: 44,
            trace_sequence: 45,
        };
        std::fs::write(
            &path,
            "trace_event=\"authoring_observation_boundary\" phase=\"terminal\" request_id=12 baseline_request_id=9 captured_trace_sequence=44 trace_sequence=45\n",
        )
        .unwrap();
        wait_for_authoring_observation_boundary(&path, expected, Duration::from_millis(20))
            .unwrap();

        let snapshot = AuthoringObservationBoundarySnapshot {
            phase: "snapshot",
            request_id: 13,
            baseline_request_id: None,
            captured_trace_sequence: 47,
            trace_sequence: 48,
        };
        std::fs::write(
            &path,
            "trace_event=\"authoring_observation_boundary\" phase=\"snapshot\" request_id=13 baseline_request_id=0 captured_trace_sequence=47 trace_sequence=48\n",
        )
        .unwrap();
        wait_for_authoring_observation_boundary(&path, snapshot, Duration::from_millis(20))
            .unwrap();

        let write = |phase: &str, request: u64, baseline: u64, captured: u64, sequence: u64| {
            std::fs::write(
                &path,
                format!(
                    "trace_event=\"authoring_observation_boundary\" phase=\"{phase}\" request_id={request} baseline_request_id={baseline} captured_trace_sequence={captured} trace_sequence={sequence}\n"
                ),
            )
            .unwrap();
        };
        for (line, needle) in [
            (("terminal", 12, 8, 44, 45), "stale or swapped"),
            (("baseline", 12, 0, 44, 45), "stale or swapped"),
            (("terminal", 12, 9, 43, 45), "stale or swapped"),
            (("terminal", 12, 9, 44, 44), "malformed"),
        ] {
            write(line.0, line.1, line.2, line.3, line.4);
            let error =
                wait_for_authoring_observation_boundary(&path, expected, Duration::from_millis(20))
                    .unwrap_err();
            assert!(error.contains(needle), "unexpected error: {error}");
        }

        write("terminal", 11, 9, 44, 45);
        let error =
            wait_for_authoring_observation_boundary(&path, expected, Duration::ZERO).unwrap_err();
        assert!(error.contains("timed out"));

        write("terminal", 12, 9, 44, 45);
        let line = std::fs::read_to_string(&path).unwrap();
        std::fs::write(&path, format!("{line}{line}")).unwrap();
        let error =
            wait_for_authoring_observation_boundary(&path, expected, Duration::from_millis(20))
                .unwrap_err();
        assert!(error.contains("duplicated"));

        std::fs::write(
            &path,
            "trace_event=\"authoring_observation_boundary\" phase=\"terminal\" request_id=12 captured_trace_sequence=44 trace_sequence=45\n",
        )
        .unwrap();
        let error =
            wait_for_authoring_observation_boundary(&path, expected, Duration::from_millis(20))
                .unwrap_err();
        assert!(error.contains("malformed"));

        for invalid_phase in ["unknown", "\"terminal", "terminal\""] {
            std::fs::write(
                &path,
                format!(
                    "trace_event=\"authoring_observation_boundary\" phase={invalid_phase} request_id=12 baseline_request_id=9 captured_trace_sequence=44 trace_sequence=45\n"
                ),
            )
            .unwrap();
            assert!(
                wait_for_authoring_observation_boundary(&path, expected, Duration::ZERO).is_err(),
                "invalid phase encoding {invalid_phase:?} must be rejected"
            );
        }
    }

    #[test]
    fn action_editor_trace_readers_keep_semantic_identity_and_provider_lifecycle() {
        let control_line = "WARN target trace_event=\"designer_action_editor_control\" editor_surface=\"properties\" editor_control=\"result_target\" control_index=2 target_digest=31 title_digest=36 type_digest=37 disambiguator_digest=38 action_digest=32 binding_digest=33 query_digest=34 value_digest=34 displayed_text_digest=45 editor_assigned_binding_digest=40 editor_session_id=7 draft_generation=9 stable_target_digest=35 editor_epoch=10 edit_generation=11 query_generation=12 query_request_generation=13 search_request_generation=14 test_request_generation=15 trace_sequence=16 left_px=1 top_px=2 right_px=90 bottom_px=30 full_left_px=1 full_top_px=2 full_right_px=90 full_bottom_px=30 client_width_px=800 client_height_px=600 fully_visible=true enabled=true selected=false focused=false clicked=true changed=false enter_pressed=false visible=true";
        let control = parse_action_editor_control(control_line).unwrap();
        assert_eq!(
            control.identity.surface,
            super::ActionEditorSurface::Properties
        );
        assert_eq!(control.identity.session_id, 7);
        assert_eq!(control.identity.query_digest, 34);
        assert_eq!(control.control, "result_target");
        assert_eq!(control.index, Some(2));
        assert_eq!(control.binding_digest, 33);
        assert_eq!(control.value_digest, 34);
        assert_eq!(control.displayed_text_digest, 45);
        assert_eq!(control.full_bounds, [1, 2, 90, 30]);
        assert!(control.fully_visible);
        assert_eq!(control.title_digest, 36);
        assert_eq!(control.type_digest, 37);
        assert_eq!(control.disambiguator_digest, 38);
        assert!(control.clicked && control.enabled && control.visible);
        assert!(!control.changed && !control.enter_pressed);
        for malformed in [
            "editor_surface=\"properties",
            "editor_surface=properties\"",
            "editor_surface=\"properties\"\"",
            "editor_surface=\"unknown\"",
            "editor_control=\"query_field",
            "editor_control=query_field\"",
            "editor_control=\"unknown\"",
        ] {
            let line = if malformed.starts_with("editor_surface") {
                control_line.replace("editor_surface=\"properties\"", malformed)
            } else {
                control_line.replace("editor_control=\"result_target\"", malformed)
            };
            assert!(parse_action_editor_control(&line).is_none(), "{malformed}");
        }
        let inspector_control_line = control_line.replace(
            "editor_surface=\"properties\"",
            "editor_surface=\"inspector\"",
        );
        assert_eq!(
            parse_action_editor_control(&inspector_control_line)
                .unwrap()
                .identity
                .surface,
            ActionEditorSurface::Inspector
        );
        for control in ["query_tab", "test_query"] {
            assert!(
                parse_action_editor_control(&control_line.replace(
                    "editor_control=\"result_target\"",
                    &format!("editor_control=\"{control}\"")
                ))
                .is_some()
            );
        }

        let lifecycle_line = "WARN target trace_event=\"authoring_provider_search\" authoring_request_edge=\"retry_queued\" authoring_search_kind=\"search\" editor_surface=\"properties\" editor_session_id=7 draft_generation=9 stable_target_digest=35 editor_epoch=10 edit_generation=11 query_generation=12 query_request_generation=13 search_request_generation=14 test_request_generation=15 query_digest=34 binding_digest=0 editor_assigned_binding_digest=40 provider_revision=-1 trace_sequence=17";
        let lifecycle = parse_action_editor_provider_event(lifecycle_line).unwrap();
        assert_eq!(lifecycle.edge, ActionEditorProviderEdge::RetryQueued);
        assert_eq!(lifecycle.kind, ActionEditorProviderKind::Search);
        assert_eq!(lifecycle.identity, control.identity);
        assert_eq!(lifecycle.trace_sequence, 17);
        assert_eq!(lifecycle.provider_revision, None);

        let inspector_line = lifecycle_line.replace(
            "editor_surface=\"properties\"",
            "editor_surface=\"inspector\"",
        );
        let inspector = parse_action_editor_provider_event(&inspector_line).unwrap();
        assert_eq!(inspector.identity.surface, ActionEditorSurface::Inspector);
        assert_ne!(inspector.identity, control.identity);
        assert!(
            parse_action_editor_provider_event(
                &lifecycle_line.replace("editor_surface=\"properties\" ", "")
            )
            .is_none()
        );
        for edge in [
            "queued",
            "worker_started",
            "worker_completed",
            "worker_failed",
            "applied",
            "rejected",
            "retired",
            "cancelled",
            "retry_queued",
        ] {
            let line = lifecycle_line.replace("\"retry_queued\"", &format!("\"{edge}\""));
            assert!(
                parse_action_editor_provider_event(&line).is_some(),
                "quoted producer edge {edge:?} should parse"
            );
        }
        let test_kind = lifecycle_line.replace("\"search\"", "\"test\"");
        assert_eq!(
            parse_action_editor_provider_event(&test_kind).unwrap().kind,
            ActionEditorProviderKind::Test
        );
        for malformed in [
            lifecycle_line.replace("\"retry_queued\"", "\"unknown\""),
            lifecycle_line.replace("\"retry_queued\"", "\"queued"),
            lifecycle_line.replace("\"retry_queued\"", "queued\""),
            lifecycle_line.replace("\"search\"", "\"unknown\""),
        ] {
            assert!(parse_action_editor_provider_event(&malformed).is_none());
        }
        let malformed_surface = lifecycle_line.replace(
            "editor_surface=\"properties\"",
            "editor_surface=\"properties",
        );
        assert!(parse_action_editor_provider_event(&malformed_surface).is_none());
        let unknown_surface = lifecycle_line.replace(
            "editor_surface=\"properties\"",
            "editor_surface=\"unknown\"",
        );
        assert!(parse_action_editor_provider_event(&unknown_surface).is_none());
    }

    #[test]
    fn uia_property_retry_classification_is_limited_to_element_not_available() {
        for property in [
            "process ID",
            "name",
            "offscreen state",
            "control type",
            "enabled state",
            "bounding rectangle",
            "edit value",
        ] {
            let message = format!("read UIA {property}: element was recycled");
            assert_eq!(
                classify_element_property_hresult(
                    UIA_ELEMENT_NOT_AVAILABLE_HRESULT,
                    message.clone()
                ),
                VisibleTextLookupError::TransientElementUnavailable(message),
                "recognized stale-element HRESULT should be retryable for {property} reads"
            );
        }
        assert_eq!(
            classify_element_property_hresult(0x8007_0005, "access denied".into()),
            VisibleTextLookupError::Other("access denied".into())
        );
        assert_eq!(
            classify_element_property_hresult(
                UIA_NOT_SUPPORTED_HRESULT,
                "value pattern is not supported".into()
            ),
            VisibleTextLookupError::Other("value pattern is not supported".into())
        );
    }

    #[test]
    fn exact_visible_text_wait_reacquires_after_transient_owner_property_reads() {
        let mut calls = 0;
        wait_visible_text_with("Shared Acceptance Note", Duration::from_secs(1), || {
            calls += 1;
            if calls == 1 {
                Err(classify_element_property_hresult(
                    UIA_ELEMENT_NOT_AVAILABLE_HRESULT,
                    "read UIA process ID: element was recycled".into(),
                ))
            } else {
                Ok(Some(()))
            }
        })
        .unwrap();
        assert_eq!(calls, 2);

        let mut unknown_calls = 0;
        assert_eq!(
            wait_visible_text_with::<()>("Shared Acceptance Note", Duration::from_secs(1), || {
                unknown_calls += 1;
                Err(classify_element_property_hresult(
                    0x8007_0005,
                    "read UIA process ID: access denied".into(),
                ))
            })
            .unwrap_err(),
            "read UIA process ID: access denied"
        );
        assert_eq!(unknown_calls, 1);

        let mut timeout_calls = 0;
        let timeout =
            wait_visible_text_with::<()>("Shared Acceptance Note", Duration::ZERO, || {
                timeout_calls += 1;
                Err(classify_element_property_hresult(
                    UIA_ELEMENT_NOT_AVAILABLE_HRESULT,
                    "read UIA process ID: element disappeared".into(),
                ))
            })
            .unwrap_err();
        assert!(timeout.contains("last transient UIA property failure"));
        assert!(timeout.contains("process ID"));
        assert_eq!(timeout_calls, 1);
    }

    #[test]
    fn explanation_match_requires_owned_visible_full_query_text() {
        let expected = "No launcher result is currently available for \"fixture query\"";
        let matches = |name, pid, hidden, bounds| {
            super::owned_visible_name_matches(name, expected, pid, 7, hidden, bounds)
        };
        // Inline and toast copies at different positions are both valid read-only evidence.
        assert!(matches(expected, 7, false, [0, 0, 200, 30]));
        assert!(matches(expected, 7, false, [100, 200, 300, 230]));
        assert!(!matches("No launcher result", 7, false, [0, 0, 200, 30]));
        assert!(!matches(
            "No launcher result is currently available for \"other query\"",
            7,
            false,
            [0, 0, 200, 30]
        ));
        assert!(!matches(expected, 8, false, [0, 0, 200, 30]));
        assert!(!matches(expected, 7, true, [0, 0, 200, 30]));
        assert!(!matches(expected, 7, false, [0, 0, 0, 0]));
    }

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
    fn selection_modifier_normalizes_only_semantic_control_and_shift() {
        for (semantic, side) in [
            (super::VK_CONTROL, super::VK_LCONTROL),
            (super::VK_SHIFT, super::VK_LSHIFT),
        ] {
            assert_eq!(
                super::owned_selection_modifier(semantic).unwrap(),
                OwnedKeyboardKey {
                    vk: side,
                    extended: false,
                }
            );
        }
        for unsupported in [
            super::VK_LCONTROL,
            super::VK_RCONTROL,
            super::VK_LSHIFT,
            super::VK_RSHIFT,
            super::VK_MENU,
            super::VK_F11,
        ] {
            assert!(super::owned_selection_modifier(unsupported).is_err());
        }
    }

    #[test]
    fn owned_modifier_allowlist_accepts_aggregate_and_left_but_rejects_right() {
        for (aggregate, left, right) in [
            (super::VK_CONTROL, super::VK_LCONTROL, super::VK_RCONTROL),
            (super::VK_SHIFT, super::VK_LSHIFT, super::VK_RSHIFT),
        ] {
            let allowed = [super::owned_selection_modifier(aggregate).unwrap().vk];
            assert!(super::modifier_is_allowed_by_owned_keys(
                aggregate, &allowed
            ));
            assert!(super::modifier_is_allowed_by_owned_keys(left, &allowed));
            assert!(!super::modifier_is_allowed_by_owned_keys(right, &allowed));
            assert!(!super::modifier_is_allowed_by_owned_keys(
                super::VK_MENU,
                &allowed
            ));
            assert!(!super::modifier_is_allowed_by_owned_keys(
                super::VK_LWIN,
                &allowed
            ));
            assert!(!super::modifier_is_allowed_by_owned_keys(
                left,
                &[aggregate]
            ));
            assert!(!super::modifier_is_allowed_by_owned_keys(
                right,
                &[aggregate]
            ));
            assert!(!super::modifier_is_allowed_by_owned_keys(aggregate, &[]));
            assert!(!super::modifier_is_allowed_by_owned_keys(left, &[]));
            assert!(!super::modifier_is_allowed_by_owned_keys(right, &[]));
        }
    }

    fn synthetic_keyboard_release_evidence(inserted: usize) -> super::NativeInputEdgeEvidence {
        super::NativeInputEdgeEvidence {
            inserted,
            at_unix_ms: 1,
            foreground_hwnd: 99,
            foreground_pid: 100,
            input_desktop: "thread=Default;active=Default".into(),
            cleanup_status: "release_inserted;async_state_to_be_verified_by_owner".into(),
            keyboard_input: None,
        }
    }

    #[test]
    fn selection_modifier_down_allowed_and_up_use_same_cookie_owned_left_key() {
        for semantic in [super::VK_CONTROL, super::VK_SHIFT] {
            let key = super::owned_selection_modifier(semantic).unwrap();
            let down = key.down();
            assert_eq!(down.r#type, super::INPUT_KEYBOARD);
            let keyboard = unsafe { down.Anonymous.ki };
            assert_eq!(keyboard.wVk, key.vk);
            assert_eq!(keyboard.dwFlags.0, 0);
            assert_eq!(keyboard.dwExtraInfo, ACCEPTANCE_RUNNER_INPUT_COOKIE);
            assert!(super::modifier_is_allowed_by_owned_keys(key.vk, &[key.vk]));

            // These are synthetic owners: a panic must never invoke a real Win32 Drop cleanup.
            let mut guard = std::mem::ManuallyDrop::new(super::OwnedKeyboardReleaseGuard::new());
            guard.owned.push(key);
            let phases = std::cell::RefCell::new(Vec::new());
            let evidence = guard
                .release_with(
                    |events, _| {
                        phases.borrow_mut().push("insert_up");
                        assert_eq!(events.len(), 1);
                        assert_eq!(events[0].r#type, super::INPUT_KEYBOARD);
                        let up = unsafe { events[0].Anonymous.ki };
                        assert_eq!(up.wVk, keyboard.wVk);
                        assert_eq!(up.dwFlags, KEYEVENTF_KEYUP);
                        assert_eq!(up.dwExtraInfo, keyboard.dwExtraInfo);
                        Ok(synthetic_keyboard_release_evidence(1))
                    },
                    |released| {
                        phases.borrow_mut().push("verify_physical_clear");
                        assert_eq!(released, &[key]);
                        Ok("async_state_clear_after_owned_release".into())
                    },
                    |_| panic!("a fully inserted release needs no cleanup"),
                    |_| panic!("a successful release uses its physical verification seam"),
                )
                .unwrap();
            assert_eq!(phases.into_inner(), ["insert_up", "verify_physical_clear"]);
            assert_eq!(evidence.inserted, 1);
            assert_eq!(
                evidence.cleanup_status,
                "async_state_clear_after_owned_release"
            );
            assert!(guard.owned.is_empty());
        }
    }

    #[test]
    fn selection_modifier_inserted_up_retires_owner_even_if_physical_key_remains_down() {
        let key = super::owned_selection_modifier(super::VK_CONTROL).unwrap();
        let mut guard = std::mem::ManuallyDrop::new(super::OwnedKeyboardReleaseGuard::new());
        guard.owned.push(key);
        let sends = std::cell::Cell::new(0);
        let error = guard
            .release_with(
                |_, _| {
                    sends.set(sends.get() + 1);
                    Ok(synthetic_keyboard_release_evidence(1))
                },
                |released| {
                    assert_eq!(released, &[key]);
                    Err("external physical key remains down after inserted up".into())
                },
                |_| panic!("retired ownership cannot authorize another synthetic up"),
                |_| panic!("the physical verification reported the failure"),
            )
            .unwrap_err();
        assert!(error.contains("external physical key remains down"));
        assert_eq!(sends.get(), 1);
        assert!(guard.owned.is_empty());
        assert!(
            guard
                .release_with(
                    |_, _| panic!("an inserted up already retired ownership"),
                    |_| panic!("no owned key remains"),
                    |_| panic!("no owned key remains"),
                    |_| panic!("no owned key remains"),
                )
                .unwrap_err()
                .contains("no inserted key-downs")
        );
    }

    #[test]
    fn keyboard_partial_release_cleans_only_keydowns_without_inserted_ups() {
        let control = super::owned_selection_modifier(super::VK_CONTROL).unwrap();
        let shift = super::owned_selection_modifier(super::VK_SHIFT).unwrap();
        let mut guard = std::mem::ManuallyDrop::new(super::OwnedKeyboardReleaseGuard::new());
        guard.owned.extend([control, shift]);
        let phases = std::cell::RefCell::new(Vec::new());
        let error = guard
            .release_with(
                |events, _| {
                    phases.borrow_mut().push("insert_shift_up");
                    let released_vks = events
                        .iter()
                        .map(|event| unsafe { event.Anonymous.ki.wVk })
                        .collect::<Vec<_>>();
                    assert_eq!(released_vks, [shift.vk, control.vk]);
                    Err((1, "partial key-up insertion".into()))
                },
                |_| panic!("a partial insertion uses the cleanup path"),
                |owned| {
                    phases.borrow_mut().push("cleanup_control_only");
                    assert_eq!(owned, &[control]);
                    assert_eq!(retire_inserted_keyboard_ups(owned, &[control], 1), 1);
                    "synthetic_control_cleanup_inserted".into()
                },
                |released| {
                    phases.borrow_mut().push("sample_after_cleanup");
                    assert_eq!(released, &[shift, control]);
                    vec![shift]
                },
            )
            .unwrap_err();
        assert_eq!(
            phases.into_inner(),
            [
                "insert_shift_up",
                "cleanup_control_only",
                "sample_after_cleanup"
            ]
        );
        assert!(error.contains("partial key-up insertion"));
        assert!(error.contains("release_inserted=1/2; retired=1"));
        assert!(error.contains("synthetic_control_cleanup_inserted"));
        assert!(guard.owned.is_empty());
    }

    #[test]
    fn designer_modifier_click_always_releases_once_and_preserves_both_errors() {
        for pointer_failed in [false, true] {
            for release_failed in [false, true] {
                let phases = std::cell::RefCell::new(Vec::new());
                let result = super::finish_designer_click_with_modifier_release(
                    || {
                        phases.borrow_mut().push("pointer");
                        if pointer_failed {
                            Err("pointer refused: foreground changed or RightControl held".into())
                        } else {
                            Ok(42)
                        }
                    },
                    || {
                        phases.borrow_mut().push("release");
                        if release_failed {
                            Err("owned key-up insertion failed".into())
                        } else {
                            Ok(synthetic_keyboard_release_evidence(1))
                        }
                    },
                );
                assert_eq!(phases.into_inner(), ["pointer", "release"]);
                match (pointer_failed, release_failed) {
                    (false, false) => {
                        let (click, up) = result.unwrap();
                        assert_eq!(click, 42);
                        assert_eq!(up.inserted, 1);
                    }
                    (true, false) => {
                        let error = result.unwrap_err();
                        assert!(error.contains("foreground changed or RightControl held"));
                        assert!(error.contains("owned selection modifier released"));
                    }
                    (false, true) => {
                        let error = result.unwrap_err();
                        assert!(error.contains("Designer pointer click completed"));
                        assert!(error.contains("owned key-up insertion failed"));
                    }
                    (true, true) => {
                        let error = result.unwrap_err();
                        assert!(error.contains("foreground changed or RightControl held"));
                        assert!(error.contains("owned key-up insertion failed"));
                    }
                }
            }
        }
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
        let (_caller_attachment_observation, caller_attachment) =
            attach_to_input_desktop().expect("attach focus-anchor test caller to input desktop");
        let process_id = std::process::id();
        let (ready, command_tx, ui_thread) =
            spawn_focus_anchor_window(process_id).expect("create message-pumped focus anchor");
        let hwnd = super::HWND(ready.hwnd as *mut std::ffi::c_void);
        let thread_id = ready.thread_id;

        let send_command = |kind| -> Result<(), String> {
            let (reply_tx, reply_rx) = std::sync::mpsc::sync_channel(1);
            command_tx
                .send(FocusAnchorCommand {
                    kind,
                    reply: reply_tx,
                })
                .map_err(|error| format!("send bounded focus anchor UI command: {error}"))?;
            unsafe {
                PostThreadMessageW(
                    thread_id,
                    super::FOCUS_ANCHOR_COMMAND_MESSAGE,
                    WPARAM(0),
                    LPARAM(0),
                )
            }
            .map_err(|error| format!("wake focus anchor UI message pump: {error}"))?;
            reply_rx
                .recv_timeout(std::time::Duration::from_secs(1))
                .map_err(|error| format!("focus anchor UI command reply: {error}"))?
        };

        let observation = (|| -> Result<(), String> {
            if ready.owner_desktop_name != "Default"
                || ready.input_desktop_name != "Default"
                || active_input_desktop_name()? != ready.owner_desktop_name
            {
                return Err(format!(
                    "focus-anchor ready desktop mismatch: owner='{}' input='{}'",
                    ready.owner_desktop_name, ready.input_desktop_name
                ));
            }
            let owner_desktop = unsafe { super::GetThreadDesktop(thread_id) }
                .map_err(|error| format!("read focus-anchor thread desktop: {error}"))?;
            if desktop_name(owner_desktop)? != ready.owner_desktop_name {
                return Err("focus-anchor owner thread desktop changed after ready".into());
            }
            if window_process_id(hwnd) != process_id {
                return Err("focus anchor HWND is not runner-owned".into());
            }
            if thread_id == unsafe { GetCurrentThreadId() } {
                return Err("focus anchor unexpectedly shares the caller thread".into());
            }

            send_command(FocusAnchorCommandKind::Raise)?;
            send_command(FocusAnchorCommandKind::MoveTo { left: 64, top: 64 })?;
            let hit = unsafe { super::WindowFromPoint(POINT { x: 224, y: 112 }) };
            if hit != hwnd {
                return Err(format!(
                    "caller WindowFromPoint did not see the input-desktop anchor: hit={} expected={}",
                    hwnd_id(hit),
                    hwnd_id(hwnd)
                ));
            }
            send_command(FocusAnchorCommandKind::RestoreNonTopmost)?;
            Ok(())
        })();

        let destroy_result = send_command(FocusAnchorCommandKind::Destroy);
        if destroy_result.is_err() {
            let _ = unsafe { PostThreadMessageW(thread_id, super::WM_QUIT, WPARAM(0), LPARAM(0)) };
        }
        let thread_result = finish_focus_anchor_ui_thread(ui_thread);
        let window_gone = window_process_id(hwnd) == 0;
        drop(caller_attachment);

        assert!(observation.is_ok(), "{observation:?}");
        assert!(destroy_result.is_ok(), "{destroy_result:?}");
        assert!(thread_result.is_ok(), "{thread_result:?}");
        assert!(window_gone, "focus anchor HWND survived owner-thread exit");
    }

    #[test]
    fn focus_anchor_startup_timeout_drops_receivers_before_window_creation() {
        let (before_attach_tx, before_attach_rx) = std::sync::mpsc::sync_channel(1);
        let (resume_tx, resume_rx) = std::sync::mpsc::sync_channel(1);
        let (timeout_tx, timeout_rx) = std::sync::mpsc::sync_channel(1);
        let window_creation_attempted =
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let attempted = window_creation_attempted.clone();
        let process_id = std::process::id();
        let startup = std::thread::spawn(move || {
            spawn_focus_anchor_window_with_startup_hooks(
                process_id,
                Duration::from_millis(40),
                move || {
                    before_attach_tx
                        .send(())
                        .expect("notify test before input desktop attachment");
                    resume_rx
                        .recv()
                        .expect("resume delayed focus-anchor startup");
                },
                move || attempted.store(true, std::sync::atomic::Ordering::SeqCst),
                move || {
                    timeout_tx
                        .send(())
                        .expect("notify observed startup timeout")
                },
            )
        });

        let before_attach = before_attach_rx.recv_timeout(Duration::from_secs(1));
        let timeout_observed = timeout_rx.recv_timeout(Duration::from_secs(1));
        let release = resume_tx.send(());
        let startup_result = startup.join().expect("startup waiter thread exits");

        assert!(
            before_attach.is_ok(),
            "worker did not reach delayed startup"
        );
        assert!(
            timeout_observed.is_ok(),
            "startup did not take the bounded timeout path"
        );
        assert!(release.is_ok(), "delayed startup worker was already gone");
        assert!(
            startup_result
                .as_ref()
                .is_err_and(|error| error.contains("focus anchor UI thread did not start")),
            "unexpected startup result: {startup_result:?}"
        );
        assert!(!window_creation_attempted.load(std::sync::atomic::Ordering::SeqCst));
    }

    fn authoring_control() -> AuthoringControlSnapshot {
        AuthoringControlSnapshot {
            frame_nr: None,
            scroll_viewport: None,
            target: AuthoringControlTarget::MenuRow,
            role: AuthoringControlRole::Selectable,
            index: Some(0),
            trace_sequence: 1,
            bounds: [20, 30, 60, 50],
            clip_bounds: None,
            text_undo: None,
            client_size: [640, 480],
            enabled: true,
            selected: false,
            focused: false,
            clicked: false,
            session_id: 9,
            generation: 4,
            menu_cell_ids_digest: None,
            authored_target_digest: None,
            menu_id_digest: None,
            ring_id_digest: None,
            cell_id_digest: None,
            cell_label_digest: None,
            projected_source_target_digest: None,
            projected_result_index: None,
            edit_source_target_digest: None,
            edit_source_result_index: None,
            breadcrumb_menu_id_digest: None,
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
        let line = "trace_event=\"designer_authoring_control\" target=NewMenu role=\"Button\" viewport=Deferred control_index=-1 left_px=14 top_px=116 right_px=83 bottom_px=134 client_width_px=640 client_height_px=480 trace_sequence=8 enabled=true selected=false focused=true clicked=false session_id=2 generation=2 menu_cell_ids_digest=0 cell_ring_index=-1 cell_slot_index=-1";
        let control =
            parse_authoring_control(line).expect("serialized New Menu control should parse");

        assert_eq!(control.target, AuthoringControlTarget::NewMenu);
        assert_eq!(control.role, AuthoringControlRole::Button);
        assert_eq!(control.index, None);
        assert!(control.focused);
        assert_eq!(control.trace_sequence, 8);
        assert_eq!(control.session_id, 2);
        assert_eq!(control.bounds, [14, 116, 83, 134]);
        assert_eq!(control.client_size, [640, 480]);
        assert_eq!(
            control.clip_bounds, None,
            "legacy Gate C evidence stays readable"
        );
        for malformed_role in [
            "role=Button\"",
            "role=\"Button",
            "role=\"Button\"\"",
            "role=\"Unknown\"",
        ] {
            let malformed = line.replace("role=\"Button\"", malformed_role);
            assert!(
                parse_authoring_control(&malformed).is_none(),
                "{malformed_role}"
            );
        }
    }

    #[test]
    fn authoring_parser_retains_owned_inspector_discard_guard_receipt() {
        let line = "trace_event=\"designer_authoring_control\" target=InspectorDiscardAndContinue role=\"Button\" viewport=Deferred control_index=-1 left_px=170 top_px=160 right_px=310 bottom_px=182 clip_left_px=150 clip_top_px=140 clip_right_px=480 clip_bottom_px=260 client_width_px=640 client_height_px=480 trace_sequence=43 enabled=true selected=false focused=false clicked=true session_id=7 generation=9 menu_cell_ids_digest=0 cell_ring_index=-1 cell_slot_index=-1";
        let receipt = parse_authoring_control(line).unwrap();
        assert_eq!(
            receipt.target,
            AuthoringControlTarget::InspectorDiscardAndContinue
        );
        assert_eq!(receipt.role, AuthoringControlRole::Button);
        assert_eq!(receipt.index, None);
        assert_eq!(
            (
                receipt.session_id,
                receipt.generation,
                receipt.trace_sequence
            ),
            (7, 9, 43)
        );
        assert_eq!(receipt.bounds, [170, 160, 310, 182]);
        assert_eq!(receipt.clip_bounds, Some([150, 140, 480, 260]));
        assert_eq!(receipt.client_size, [640, 480]);
        assert!(receipt.enabled && receipt.clicked);
        assert_ne!(receipt.target, AuthoringControlTarget::PopupDiscardAndOpen);
    }

    #[test]
    fn authoring_parser_retains_actual_pane_clip_and_rejects_partial_measurements() {
        let legacy = "trace_event=\"designer_authoring_control\" target=BulkLabel role=\"TextEdit\" viewport=Deferred control_index=-1 left_px=320 top_px=50 right_px=500 bottom_px=75 client_width_px=520 client_height_px=380 trace_sequence=11 enabled=true selected=false focused=false clicked=false session_id=4 generation=7 menu_cell_ids_digest=0 cell_ring_index=-1 cell_slot_index=-1";
        let clip = "clip_left_px=310 clip_top_px=80 clip_right_px=510 clip_bottom_px=340";
        for line in [format!("{legacy} {clip}"), format!("{clip} {legacy}")] {
            let control = parse_authoring_control(&line).expect("complete pane geometry parses");
            assert_eq!(control.bounds, [320, 50, 500, 75]);
            assert_eq!(control.clip_bounds, Some([310, 80, 510, 340]));
            for missing in [
                "clip_left_px=310",
                "clip_top_px=80",
                "clip_right_px=510",
                "clip_bottom_px=340",
            ] {
                assert!(parse_authoring_control(&line.replace(missing, "")).is_none());
                assert!(
                    parse_authoring_control(&line.replace(missing, "clip_left_px=bad")).is_none()
                );
            }
        }
        assert_eq!(parse_authoring_control(legacy).unwrap().clip_bounds, None);
    }

    #[test]
    fn authoring_parser_distinguishes_missing_and_actual_text_undo_checkpoint_state() {
        let legacy = "trace_event=\"designer_authoring_control\" target=TreeSearch role=\"TextEdit\" viewport=Deferred control_index=-1 left_px=20 top_px=50 right_px=200 bottom_px=75 client_width_px=520 client_height_px=380 trace_sequence=11 enabled=true selected=false focused=true clicked=false session_id=4 generation=7 menu_cell_ids_digest=0 cell_ring_index=-1 cell_slot_index=-1";
        assert_eq!(parse_authoring_control(legacy).unwrap().text_undo, None);
        let unknown = "text_edit_field_digest=0 text_edit_value_digest=0 text_edit_undo_in_flux=-1";
        assert_eq!(
            parse_authoring_control(&format!("{legacy} {unknown}"))
                .unwrap()
                .text_undo,
            None
        );
        for in_flux in [false, true] {
            let state = format!(
                "text_edit_field_digest=41 text_edit_value_digest=42 text_edit_undo_in_flux={}",
                i32::from(in_flux)
            );
            for line in [format!("{legacy} {state}"), format!("{state} {legacy}")] {
                let control = parse_authoring_control(&line).unwrap();
                assert_eq!(
                    control.text_undo,
                    Some(super::TextEditUndoSnapshot {
                        field_id_digest: 41,
                        value_digest: 42,
                        in_flux,
                    })
                );
                for field in state.split_whitespace() {
                    assert!(parse_authoring_control(&line.replace(field, "")).is_none());
                }
            }
        }
        let valid = format!(
            "{legacy} text_edit_field_digest=41 text_edit_value_digest=42 text_edit_undo_in_flux=0"
        );
        for (from, to) in [
            ("text_edit_field_digest=41", "text_edit_field_digest=0"),
            ("text_edit_value_digest=42", "text_edit_value_digest=0"),
            ("text_edit_value_digest=42", "text_edit_value_digest=bad"),
            ("text_edit_undo_in_flux=0", "text_edit_undo_in_flux=2"),
            ("text_edit_undo_in_flux=0", "text_edit_undo_in_flux=-1"),
            ("target=TreeSearch", "target=BulkLabel"),
            ("role=\"TextEdit\"", "role=\"Button\""),
            ("control_index=-1", "control_index=0"),
        ] {
            assert!(
                parse_authoring_control(&valid.replace(from, to)).is_none(),
                "{from} -> {to}"
            );
        }
    }

    #[test]
    fn canvas_cell_parser_retains_redacted_menu_ring_and_slot_scope() {
        let control = parse_authoring_control(
            "trace_event=\"designer_authoring_control\" target=CanvasCell role=\"Region\" viewport=Deferred control_index=8 left_px=20 top_px=30 right_px=28 bottom_px=38 client_width_px=640 client_height_px=480 trace_sequence=9 enabled=true selected=false focused=false clicked=false session_id=2 generation=7 menu_cell_ids_digest=123456 cell_ring_index=1 cell_slot_index=0",
        )
        .expect("scoped CanvasCell evidence should parse");

        assert_eq!(control.menu_cell_ids_digest, Some(123456));
        assert_eq!(control.ring_index, Some(1));
        assert_eq!(control.slot_index, Some(0));
        assert_eq!(control.index, Some(8));
    }

    #[test]
    fn authoring_control_event_reader_retains_clicked_owner_event_after_trace_cursor() {
        let directory = tempfile::tempdir().unwrap();
        let trace_path = directory.path().join("candidate.log");
        std::fs::write(
            &trace_path,
            concat!(
                "logger startup line\n",
                "trace_event=\"designer_authoring_control\" target=KeepEditing role=\"Button\" viewport=Deferred control_index=-1 left_px=14 top_px=116 right_px=83 bottom_px=134 client_width_px=640 client_height_px=480 trace_sequence=8 enabled=true selected=false focused=true clicked=false session_id=2 generation=4 menu_cell_ids_digest=0 cell_ring_index=-1 cell_slot_index=-1\n",
                "logger unrelated line\n",
                "trace_event=\"designer_authoring_control\" target=KeepEditing role=\"Button\" viewport=Deferred control_index=-1 left_px=14 top_px=116 right_px=83 bottom_px=134 client_width_px=640 client_height_px=480 trace_sequence=9 enabled=true selected=false focused=true clicked=true session_id=2 generation=4 menu_cell_ids_digest=0 cell_ring_index=-1 cell_slot_index=-1\n",
                "trace_event=\"designer_authoring_control\" target=KeepEditing role=\"Button\" viewport=Deferred control_index=-1 left_px=14 top_px=116 right_px=83 bottom_px=134 client_width_px=640 client_height_px=480 trace_sequence=10 enabled=false selected=false focused=false clicked=false session_id=2 generation=4 menu_cell_ids_digest=0 cell_ring_index=-1 cell_slot_index=-1\n",
                "trace_event=\"designer_authoring_control\" target=KeepEditing role=\"Button\" viewport=Deferred control_index=-1 left_px=14 top_px=116 right_px=83 bottom_px=134 client_width_px=640 client_height_px=480 trace_sequence=11 enabled=true selected=false focused=true clicked=true session_id=3 generation=4 menu_cell_ids_digest=0 cell_ring_index=-1 cell_slot_index=-1\n"
            ),
        )
        .unwrap();

        let controls = authoring_control_events_after(&trace_path, 1, 2).unwrap();
        assert_eq!(controls.len(), 2);
        assert_eq!(controls[0].target, AuthoringControlTarget::KeepEditing);
        assert_eq!(controls[0].trace_sequence, 9);
        assert!(controls[0].clicked);
        assert_eq!(controls[1].trace_sequence, 10);
        assert!(!controls[1].clicked && !controls[1].enabled);

        let records = trace_event_lines(&std::fs::read_to_string(&trace_path).unwrap());
        assert_eq!(
            super::authoring_control_events_in_lines_after(&records, 1, 2),
            controls
        );
        let captured_cursor = records.len();
        let mut appended = records.clone();
        appended.push(records[1].replace("trace_sequence=9", "trace_sequence=12"));
        assert!(
            super::authoring_control_events_in_lines_after(&records, captured_cursor, 2).is_empty()
        );
        let following =
            super::authoring_control_events_in_lines_after(&appended, captured_cursor, 2);
        assert_eq!(following.len(), 1);
        assert_eq!(following[0].trace_sequence, 12);
        assert!(following[0].clicked);
        let latest = latest_authoring_controls_after(&records, 1, 2);
        assert_eq!(
            latest,
            vec![controls[1]],
            "state lookup remains latest-state, while receipts retain every event"
        );
    }

    #[test]
    fn post_resize_authoring_lookup_does_not_reuse_a_disappeared_control() {
        let stale = "trace_event=\"designer_authoring_control\" target=Canvas role=\"Region\" viewport=Deferred control_index=-1 left_px=20 top_px=30 right_px=620 bottom_px=430 client_width_px=640 client_height_px=480 trace_sequence=10 enabled=true selected=false focused=false clicked=false session_id=9 generation=4 menu_cell_ids_digest=0 cell_ring_index=-1 cell_slot_index=-1".to_owned();
        let fresh = "trace_event=\"designer_authoring_control\" target=Canvas role=\"Region\" viewport=Deferred control_index=-1 left_px=20 top_px=30 right_px=520 bottom_px=390 client_width_px=520 client_height_px=380 trace_sequence=11 enabled=true selected=false focused=false clicked=false session_id=9 generation=4 menu_cell_ids_digest=0 cell_ring_index=-1 cell_slot_index=-1".to_owned();
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

    pub(super) fn cadence_menu_row_lines(
        sequence: u64,
        frame: u64,
        top: i32,
        elapsed_ms: u64,
    ) -> [String; 2] {
        // Candidate 6's row10/input/paint/client measurements, restored to the
        // actual producer schema (the public excerpt omits target and quotes).
        [
            format!(
                "trace_event=\"designer_authoring_control\" elapsed_ms={elapsed_ms} target=MenuRow role=\"Selectable\" viewport=Deferred control_index=10 left_px=8 top_px={top} right_px=128 bottom_px={} clip_left_px=0 clip_top_px=185 clip_right_px=900 clip_bottom_px=638 client_width_px=900 client_height_px=650 trace_sequence={sequence} enabled=true selected=false focused=false clicked=false session_id=1 generation=9 frame_nr={frame} menu_cell_ids_digest=0 cell_ring_index=-1 cell_slot_index=-1",
                top + 18
            ),
            format!(
                "trace_event=\"designer_authoring_scroll_viewport\" elapsed_ms={elapsed_ms} viewport=Deferred scroll_owner=\"MenuTree\" scroll_id=10543418435404023934 frame_nr={frame} session_id=1 generation=9 input_left_px=8 input_top_px=189 input_right_px=184 input_bottom_px=635 clip_left_px=0 clip_top_px=185 clip_right_px=900 clip_bottom_px=638 client_width_px=900 client_height_px=650 trace_sequence={}",
                sequence + 2
            ),
        ]
    }

    #[test]
    fn authoring_controls_retain_completed_pair_across_actual_viewport_only_cadence() {
        let [control, viewport] = cadence_menu_row_lines(13894, 1274, 773, 126930);
        let first = latest_authoring_controls_after(&[control.clone(), viewport.clone()], 0, 1);
        assert_eq!(first.len(), 1);
        let attached = first[0].scroll_viewport.unwrap();
        assert_eq!(
            (first[0].trace_sequence, first[0].frame_nr),
            (13894, Some(1274))
        );
        assert_eq!(
            (attached.trace_sequence, attached.measured.frame_nr),
            (13896, 1274)
        );
        assert_eq!(attached.measured.input_bounds, [8, 189, 184, 635]);
        let records = [
            control,
            viewport.clone(),
            viewport
                .replace("frame_nr=1274", "frame_nr=1275")
                .replace("13896", "13901"),
            viewport
                .replace("frame_nr=1274", "frame_nr=1276")
                .replace("13896", "13918"),
        ];
        for end in 2..=records.len() {
            assert_eq!(
                latest_authoring_controls_after(&records[..end], 0, 1),
                first
            );
        }
        assert!(latest_authoring_controls_after(&records, 1, 1).is_empty());
        assert!(latest_authoring_controls_after(&records, 0, 2).is_empty());
    }

    #[test]
    fn authoring_controls_invalidate_conflicting_viewports_until_fresh_control_proof() {
        let first = cadence_menu_row_lines(13894, 1274, 773, 126930);
        let second = cadence_menu_row_lines(13945, 1279, 773, 127464);
        let later = first[1]
            .replace("frame_nr=1274", "frame_nr=1275")
            .replace("13896", "13901");
        for (field, replacement) in [
            ("session_id=1", "session_id=2"),
            ("generation=9", "generation=10"),
            (
                "scroll_id=10543418435404023934",
                "scroll_id=10543418435404023935",
            ),
            ("input_left_px=8", "input_left_px=9"),
            ("input_top_px=189", "input_top_px=190"),
            ("input_right_px=184", "input_right_px=183"),
            ("input_bottom_px=635", "input_bottom_px=634"),
            ("clip_top_px=185", "clip_top_px=184"),
            ("clip_bottom_px=638", "clip_bottom_px=639"),
            ("client_width_px=900", "client_width_px=901"),
            ("client_height_px=650", "client_height_px=651"),
            ("frame_nr=1275", "frame_nr=1273"),
            ("trace_sequence=13901", "trace_sequence=13896"),
        ] {
            let changed = later.replace(field, replacement);
            assert!(
                super::parse_authoring_scroll_viewport(&changed).is_some(),
                "{field}"
            );
            let mut records = vec![first[0].clone(), first[1].clone(), changed];
            assert_eq!(
                latest_authoring_controls_after(&records, 0, 1)[0].scroll_viewport,
                None,
                "{field}"
            );
            records.push(
                later
                    .replace("frame_nr=1275", "frame_nr=1276")
                    .replace("13901", "13918"),
            );
            assert_eq!(
                latest_authoring_controls_after(&records, 0, 1)[0].scroll_viewport,
                None,
                "viewport-only recovery: {field}"
            );
            records.extend(second.clone());
            let recovered = latest_authoring_controls_after(&records, 0, 1);
            assert_eq!(recovered.len(), 1);
            assert_eq!(recovered[0].trace_sequence, 13945);
            assert_eq!(recovered[0].scroll_viewport.unwrap().trace_sequence, 13947);
        }
        let wrong_owner =
            first[1].replace("scroll_owner=\"MenuTree\"", "scroll_owner=\"Inspector\"");
        assert_eq!(
            latest_authoring_controls_after(&[first[0].clone(), wrong_owner.clone()], 0, 1)[0]
                .scroll_viewport,
            None
        );
        // A different legitimate pane must not invalidate the MenuTree owner.
        let matched = latest_authoring_controls_after(&first, 0, 1);
        assert_eq!(
            latest_authoring_controls_after(
                &[first[0].clone(), first[1].clone(), wrong_owner],
                0,
                1
            ),
            matched
        );
    }

    #[test]
    fn authoring_controls_require_new_slot_receipt_and_reject_ambiguous_frames() {
        let first = cadence_menu_row_lines(13894, 1274, 773, 126930);
        let second = cadence_menu_row_lines(13945, 1279, 773, 127464);
        let mut records = first.to_vec();
        records.push(second[0].clone());
        let partial = latest_authoring_controls_after(&records, 0, 1);
        assert_eq!(partial.len(), 1);
        assert_eq!(
            (partial[0].trace_sequence, partial[0].frame_nr),
            (13945, Some(1279))
        );
        assert_eq!(partial[0].scroll_viewport, None);
        records.push(second[1].clone());
        let complete = latest_authoring_controls_after(&records, 0, 1);
        assert_eq!(complete[0].scroll_viewport.unwrap().trace_sequence, 13947);
        // Even identical measurements from the same owner/frame are ambiguous.
        records.push(second[1].replace("13947", "13948"));
        records.push(
            second[1]
                .replace("frame_nr=1279", "frame_nr=1280")
                .replace("13947", "13952"),
        );
        assert_eq!(
            latest_authoring_controls_after(&records, 0, 1)[0].scroll_viewport,
            None
        );
        let fresh = cadence_menu_row_lines(13994, 1283, 773, 127971);
        records.extend(fresh.clone());
        assert_eq!(
            latest_authoring_controls_after(&records, 0, 1)[0]
                .scroll_viewport
                .unwrap()
                .trace_sequence,
            13996
        );
        for altered in [
            second[1].replace("frame_nr=1279", "frame_nr=1280"),
            second[1].replace("generation=9", "generation=10"),
            second[1].replace("trace_sequence=13947", "trace_sequence=13945"),
        ] {
            let mut records = vec![
                first[0].clone(),
                first[1].clone(),
                second[0].clone(),
                altered,
            ];
            assert_eq!(
                latest_authoring_controls_after(&records, 0, 1)[0].scroll_viewport,
                None
            );
            records.push(second[1].replace("13947", "13950"));
            assert_eq!(
                latest_authoring_controls_after(&records, 0, 1)[0].scroll_viewport,
                None,
                "a mismatched receipt cannot recover from viewport-only evidence"
            );
            records.extend(fresh.clone());
            assert_eq!(
                latest_authoring_controls_after(&records, 0, 1)[0]
                    .scroll_viewport
                    .unwrap()
                    .trace_sequence,
                13996
            );
        }
        let reversed = [first[1].clone(), first[0].clone()];
        assert_eq!(
            latest_authoring_controls_after(&reversed, 0, 1)[0].scroll_viewport,
            None
        );
        let missing_frame = [first[0].replace(" frame_nr=1274", ""), first[1].clone()];
        assert_eq!(
            latest_authoring_controls_after(&missing_frame, 0, 1)[0].scroll_viewport,
            None
        );
    }

    #[test]
    fn authoring_controls_recover_after_foreign_lifetime_high_frame_conflict() {
        let first = cadence_menu_row_lines(13894, 1274, 773, 126930);
        let restarted = cadence_menu_row_lines(13945, 3, 773, 127464);
        for (field, replacement) in [
            ("session_id=1", "session_id=2"),
            ("generation=9", "generation=10"),
        ] {
            let foreign = first[1]
                .replace("frame_nr=1274", "frame_nr=9999")
                .replace("13896", "13901")
                .replace(field, replacement);
            let mut records = vec![first[0].clone(), first[1].clone(), foreign];
            assert_eq!(
                latest_authoring_controls_after(&records, 0, 1)[0].scroll_viewport,
                None,
                "foreign {field} must invalidate the requested owner"
            );
            let mut old_frame_return = records.clone();
            old_frame_return.push(first[1].replace("13896", "13910"));
            assert_eq!(
                latest_authoring_controls_after(&old_frame_return, 0, 1)[0].scroll_viewport,
                None,
                "returning to the original owner's original frame cannot revive the invalidated control"
            );
            old_frame_return.extend(cadence_menu_row_lines(13945, 1279, 773, 127464));
            let fresh_publication = latest_authoring_controls_after(&old_frame_return, 0, 1);
            assert_eq!(fresh_publication.len(), 1);
            assert_eq!(fresh_publication[0].trace_sequence, 13945);
            assert_eq!(
                fresh_publication[0].scroll_viewport.unwrap().trace_sequence,
                13947
            );
            records.push(restarted[0].clone());
            assert_eq!(
                latest_authoring_controls_after(&records, 0, 1)[0].scroll_viewport,
                None
            );
            let mut stale_sequence = records.clone();
            stale_sequence.push(restarted[1].replace("13947", "13900"));
            assert_eq!(
                latest_authoring_controls_after(&stale_sequence, 0, 1)[0].scroll_viewport,
                None,
                "physical sequence must remain fresh across lifetimes"
            );
            stale_sequence.push(restarted[1].clone());
            assert_eq!(
                latest_authoring_controls_after(&stale_sequence, 0, 1)[0].scroll_viewport,
                None,
                "stale sequence invalidation requires a new control publication"
            );
            stale_sequence.extend(cadence_menu_row_lines(13950, 4, 773, 127971));
            assert_eq!(
                latest_authoring_controls_after(&stale_sequence, 0, 1)[0]
                    .scroll_viewport
                    .unwrap()
                    .trace_sequence,
                13952
            );
            records.push(restarted[1].clone());
            let recovered = latest_authoring_controls_after(&records, 0, 1);
            assert_eq!(recovered.len(), 1);
            assert_eq!(
                (recovered[0].trace_sequence, recovered[0].frame_nr),
                (13945, Some(3))
            );
            assert_eq!(
                (
                    recovered[0].scroll_viewport.unwrap().trace_sequence,
                    recovered[0].scroll_viewport.unwrap().measured.frame_nr
                ),
                (13947, 3)
            );
            for frame in [2, 3] {
                let mut conflicting = records.clone();
                conflicting.push(
                    restarted[1]
                        .replace("frame_nr=3", &format!("frame_nr={frame}"))
                        .replace("13947", "13948"),
                );
                conflicting.push(
                    restarted[1]
                        .replace("frame_nr=3", "frame_nr=4")
                        .replace("13947", "13952"),
                );
                assert_eq!(
                    latest_authoring_controls_after(&conflicting, 0, 1)[0].scroll_viewport,
                    None,
                    "same-lifetime duplicate/backward frames cannot recover without a fresh control"
                );
            }
        }
    }

    #[test]
    fn authoring_controls_keep_only_latest_slots_for_three_viewport_owners() {
        let mut records = Vec::new();
        let mut expected = Vec::new();
        for frame in 1..=256 {
            let sequence = 14000 + frame * 10;
            let [control, viewport] = cadence_menu_row_lines(sequence, frame, 773, 127971);
            for (offset, target, role, index, owner) in [
                (0, "MenuRow", "Selectable", 10, "MenuTree"),
                (1, "BulkLabel", "TextEdit", -1, "Inspector"),
                (2, "SimpleScale", "Button", 1, "Resources"),
            ] {
                let control = control
                    .replace("target=MenuRow", &format!("target={target}"))
                    .replace("role=\"Selectable\"", &format!("role=\"{role}\""))
                    .replace("control_index=10", &format!("control_index={index}"))
                    .replace(
                        &format!("trace_sequence={sequence}"),
                        &format!("trace_sequence={}", sequence + offset * 3),
                    );
                let viewport = viewport
                    .replace(
                        "scroll_owner=\"MenuTree\"",
                        &format!("scroll_owner=\"{owner}\""),
                    )
                    .replace(
                        &format!("trace_sequence={}", sequence + 2),
                        &format!("trace_sequence={}", sequence + 2 + offset * 3),
                    );
                if frame % 5 == 1 {
                    records.push(control.clone());
                    let pair = latest_authoring_controls_after(&[control, viewport.clone()], 0, 1);
                    assert_eq!(pair.len(), 1);
                    if frame == 256 {
                        expected.extend(pair);
                    }
                }
                records.push(viewport);
            }
            assert_eq!(latest_authoring_controls_after(&records, 0, 1).len(), 3);
        }
        assert_eq!(latest_authoring_controls_after(&records, 0, 1), expected);
        assert_eq!(expected.len(), 3);
        assert!(expected.iter().all(|control| control.frame_nr == Some(256)));
    }

    #[test]
    fn authoring_control_click_requires_a_later_frame_after_activation() {
        let click = "trace_event=\"designer_authoring_control\" target=Slots role=\"DragValue\" viewport=Deferred control_index=-1 left_px=298 top_px=116 right_px=342 bottom_px=134 client_width_px=640 client_height_px=480 trace_sequence=12 enabled=true selected=false focused=false clicked=true session_id=2 generation=4 menu_cell_ids_digest=0 cell_ring_index=-1 cell_slot_index=-1".to_owned();
        let after_click = "trace_event=\"designer_authoring_control\" target=Slots role=\"DragValue\" viewport=Deferred control_index=-1 left_px=298 top_px=116 right_px=342 bottom_px=134 client_width_px=640 client_height_px=480 trace_sequence=13 enabled=true selected=false focused=false clicked=false session_id=2 generation=4 menu_cell_ids_digest=0 cell_ring_index=-1 cell_slot_index=-1".to_owned();

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
    fn pointer_correction_trace_cursor_precedes_the_send() {
        let directory = tempfile::tempdir().unwrap();
        let trace_path = directory.path().join("acceptance.log");
        std::fs::write(
            &trace_path,
            "trace_event=\"designer_pointer_moved\" client_x=383 client_y=155\n",
        )
        .unwrap();
        let cursor = std::cell::Cell::new(0);

        let inserted = send_pointer_correction_after_cursor(&trace_path, &cursor, || {
            use std::io::Write as _;

            let mut trace = std::fs::OpenOptions::new()
                .append(true)
                .open(&trace_path)
                .unwrap();
            writeln!(
                trace,
                "trace_event=\"designer_pointer_moved\" client_x=358 client_y=98"
            )
            .unwrap();
            Ok(1)
        })
        .unwrap();

        assert_eq!(inserted, 1);
        assert_eq!(
            cursor.get(),
            1,
            "cursor is captured before the send closure"
        );
        assert_eq!(
            latest_pointer_move_after(&trace_path, cursor.get(), PointerTraceKind::DesignerClient,)
                .unwrap(),
            Some((358, 98)),
            "the exact acknowledgment emitted during SendInput remains after the cursor"
        );
    }

    #[test]
    fn pointer_move_ack_ignores_stale_far_samples_until_exact_owned_point() {
        let mut observations =
            std::collections::VecDeque::from([Some((383, 155)), Some((358, 98))]);
        let mut corrections = 0;
        let mut physical_checks = 0;
        let result = wait_for_pointer_move_ack_with(
            "Designer test click",
            "Designer client point",
            (358, 98),
            Duration::from_millis(100),
            || Ok(observations.pop_front().flatten()),
            || {
                physical_checks += 1;
                Ok(())
            },
            |_, _| {
                corrections += 1;
                Ok(1)
            },
            |_| {},
        )
        .expect("the fresh exact owner event follows the stale ROOT-coordinate sample");
        assert_eq!(result, 0);
        assert_eq!(corrections, 0, "a far stale sample is never corrected");
        assert_eq!(
            physical_checks, 1,
            "physical ownership is checked on exact trace"
        );
    }

    #[test]
    fn pointer_move_ack_corrects_only_near_samples_and_requires_physical_match() {
        let mut observations =
            std::collections::VecDeque::from([Some((356, 100)), Some((358, 98))]);
        let mut corrections = Vec::new();
        let mut physical_checks = 0;
        let result = wait_for_pointer_move_ack_with(
            "ROOT test click",
            "ROOT screen point",
            (358, 98),
            Duration::from_millis(100),
            || Ok(observations.pop_front().flatten()),
            || {
                physical_checks += 1;
                Ok(())
            },
            |dx, dy| {
                corrections.push((dx, dy));
                Ok(1)
            },
            |_| {},
        )
        .unwrap();
        assert_eq!(result, 1);
        assert_eq!(corrections, [(2, -2)]);
        assert_eq!(physical_checks, 1);

        let mut exact = std::collections::VecDeque::from([Some((358, 98))]);
        let mismatch = wait_for_pointer_move_ack_with(
            "Designer test click",
            "Designer client point",
            (358, 98),
            Duration::from_millis(100),
            || Ok(exact.pop_front().flatten()),
            || Err("physical cursor or owned hit did not match".into()),
            |_, _| panic!("an exact trace must not generate a correction"),
            |_| {},
        )
        .unwrap_err();
        assert!(mismatch.contains("physical cursor or owned hit"));
    }

    #[test]
    fn pointer_move_ack_times_out_on_far_samples_without_correction() {
        let mut corrections = 0;
        let error = wait_for_pointer_move_ack_with(
            "Designer test click",
            "Designer client point",
            (358, 98),
            Duration::from_millis(15),
            || Ok(Some((383, 155))),
            || panic!("far samples are not physical acknowledgements"),
            |_, _| {
                corrections += 1;
                Ok(1)
            },
            std::thread::sleep,
        )
        .unwrap_err();
        assert!(error.contains("did not reach exact point"));
        assert!(error.contains("last observed=Some((383, 155))"));
        assert_eq!(corrections, 0);
    }

    #[test]
    fn radial_release_ack_requires_fresh_ordered_owned_primary_edges() {
        let lines = vec![
            "trace_event=\"native_pointer\" transition=Down button=Primary owner=PreviewInput hwnd=41 generation=7".into(),
            "trace_event=\"native_pointer\" transition=Up button=Primary owner=PreviewInput hwnd=41 generation=7".into(),
            "trace_event=\"native_pointer\" transition=Down button=Primary owner=PreviewInput hwnd=42 generation=8".into(),
            "trace_event=\"native_pointer\" transition=Up button=Primary owner=PreviewInput hwnd=42 generation=8".into(),
            "trace_event=\"native_pointer\" transition=Down button=Secondary owner=PreviewInput hwnd=41 generation=7".into(),
            "trace_event=\"native_pointer\" transition=Up button=Secondary owner=PreviewInput hwnd=41 generation=7".into(),
            "trace_event=\"native_pointer\" transition=Down button=Primary owner=Other hwnd=41 generation=7".into(),
            "trace_event=\"native_pointer\" transition=Up button=Primary owner=Other hwnd=41 generation=7".into(),
            "trace_event=\"native_pointer\" transition=Up button=Primary owner=PreviewInput hwnd=41 generation=7".into(),
            "trace_event=\"native_pointer\" transition=Down button=Primary owner=PreviewInput hwnd=41 generation=7".into(),
            "trace_event=\"native_pointer\" transition=Up button=Primary owner=PreviewInput hwnd=41 generation=7".into(),
        ];

        let ack = radial_pointer_release_ack_after(&lines, 9, 41, 7)
            .expect("fresh Down/Up should acknowledge the exact hovered layout");
        assert_eq!(ack.down_event_ordinal, 10);
        assert_eq!(ack.up_event_ordinal, 11);
        assert!(radial_pointer_release_ack_after(&lines, 11, 41, 7).is_none());
        assert!(radial_pointer_release_ack_after(&lines, 9, 41, 8).is_none());
        assert!(radial_pointer_release_ack_after(&lines, 9, 42, 7).is_none());
    }

    #[test]
    fn radial_release_ack_does_not_require_surface_to_survive_release() {
        let ack = RadialPointerReleaseAck {
            down_event_ordinal: 3,
            up_event_ordinal: 4,
        };
        let mut samples = std::collections::VecDeque::from([
            (None, true, 0x8000),
            (Some(ack), true, 0x8000),
            (Some(ack), false, 0),
        ]);
        let mut surface_destroyed = false;
        let (observed_ack, _) = wait_for_pointer_release_settle(Duration::from_millis(100), || {
            let sample = samples.pop_front().unwrap_or((Some(ack), false, 0));
            if sample.0.is_some() {
                surface_destroyed = true;
            }
            Ok(sample)
        })
        .expect("release ACK and physical settle remain valid after surface closes");
        assert_eq!(observed_ack, ack);
        assert!(surface_destroyed);
    }

    #[test]
    fn radial_release_wait_accepts_delayed_async_clear_and_bounds_stuck_button() {
        let ack = RadialPointerReleaseAck {
            down_event_ordinal: 1,
            up_event_ordinal: 2,
        };
        let mut samples = std::collections::VecDeque::from([
            (Some(ack), true, 0x8000),
            (Some(ack), true, 0x8000),
            (Some(ack), false, 0),
        ]);
        assert_eq!(
            wait_for_pointer_release_settle(Duration::from_millis(100), || {
                Ok(samples.pop_front().expect("three asynchronous samples"))
            })
            .unwrap()
            .0,
            ack
        );

        let error = wait_for_pointer_release_settle(Duration::from_millis(5), || {
            Ok((Some(ack), true, 0x8000))
        })
        .unwrap_err();
        assert!(matches!(error, PointerReleaseWaitError::TimedOut(_)));
        assert!(
            wait_for_pointer_release_settle(Duration::ZERO, || { Ok((None, false, 0)) }).is_err()
        );
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
