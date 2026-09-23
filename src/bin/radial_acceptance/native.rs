//! Native Windows boundary helpers for the opt-in radial acceptance process.

#![cfg(windows)]

mod suite;
pub(super) use suite::{record_environment_failure, run_suite};

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
use windows::Win32::System::StationsAndDesktops::{
    CloseDesktop, DESKTOP_ACCESS_FLAGS, DESKTOP_CREATEWINDOW, DESKTOP_READOBJECTS,
    DESKTOP_WRITEOBJECTS, GetThreadDesktop, GetUserObjectInformationW, HDESK, OpenInputDesktop,
    SetThreadDesktop, UOI_NAME,
};
use windows::Win32::System::Threading::{
    AttachThreadInput, CREATE_UNICODE_ENVIRONMENT, CreateProcessW, GetCurrentThreadId,
    GetExitCodeProcess, PROCESS_INFORMATION, STARTF_USESTDHANDLES, STARTUPINFOW, TerminateProcess,
    WaitForSingleObject,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationInvokePattern,
    IUIAutomationSelectionItemPattern, IUIAutomationTogglePattern, ToggleState,
    TreeScope_Descendants, UIA_ButtonControlTypeId, UIA_EditControlTypeId, UIA_InvokePatternId,
    UIA_NamePropertyId, UIA_SelectionItemPatternId, UIA_TogglePatternId,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, HOT_KEY_MODIFIERS, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT,
    KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEINPUT,
    RegisterHotKey, SendInput, UnregisterHotKey, VIRTUAL_KEY, VK_CONTROL, VK_F4, VK_F11,
    VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_MENU, VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_RWIN,
    VK_SHIFT, VK_TAB,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, EnumWindows, GetClientRect, GetForegroundWindow,
    GetSystemMetrics, GetWindowRect, GetWindowTextLengthW, GetWindowTextW,
    GetWindowThreadProcessId, IsIconic, IsWindowVisible, PostMessageW, SM_CXVIRTUALSCREEN,
    SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SetCursorPos, SetForegroundWindow,
    WINDOW_STYLE, WM_CLOSE, WS_CAPTION, WS_EX_TOOLWINDOW, WS_SYSMENU, WS_VISIBLE, WindowFromPoint,
};
use windows::core::w;
use windows::core::{Interface, PCWSTR, PWSTR, VARIANT};

const TRACE_ENV: &str = "MULTI_LAUNCHER_RADIAL_ACCEPTANCE_TRACE";
const ROOT_TITLE: &str = "Multi Lnchr";
const DESIGNER_TITLE: &str = "Radial Designer";
const WINDOW_POLL: Duration = Duration::from_millis(25);
const STARTUP_TIMEOUT: Duration = Duration::from_secs(20);
const CASE_TIMEOUT: Duration = Duration::from_secs(4);
const MAX_ENUMERATED_WINDOWS: usize = 256;
const ACCEPTANCE_HOTKEY_ID: i32 = 0x4D4C;

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
}

impl NativeInputEdgeEvidence {
    pub fn describe(&self) -> String {
        format!(
            "inserted={} at_unix_ms={} foreground=HWND:{} PID:{} desktop={}",
            self.inserted,
            self.at_unix_ms,
            self.foreground_hwnd,
            self.foreground_pid,
            self.input_desktop
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
pub(super) struct PointerClickEvidence {
    pub down: NativeInputEdgeEvidence,
    pub up: NativeInputEdgeEvidence,
}

impl PointerClickEvidence {
    pub fn describe(&self) -> String {
        format!(
            "down=[{}], up=[{}]",
            self.down.describe(),
            self.up.describe()
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

pub(super) fn preflight_acceptance_hotkey() -> Result<(), String> {
    unsafe {
        RegisterHotKey(
            None,
            ACCEPTANCE_HOTKEY_ID,
            HOT_KEY_MODIFIERS(0),
            VK_F11.0 as u32,
        )
    }
    .map_err(|error| {
        format!("acceptance hotkey F11 is already registered or unavailable: {error}")
    })?;
    unsafe { UnregisterHotKey(None, ACCEPTANCE_HOTKEY_ID) }
        .map_err(|error| format!("release F11 acceptance hotkey preflight registration: {error}"))
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
}

impl FocusAnchor {
    pub fn create() -> Result<Self, String> {
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW,
                w!("STATIC"),
                w!("Radial acceptance focus anchor"),
                WINDOW_STYLE(WS_CAPTION.0 | WS_SYSMENU.0 | WS_VISIBLE.0),
                48,
                48,
                320,
                96,
                HWND::default(),
                None,
                None,
                None,
            )
        }
        .map_err(|error| format!("create runner-owned focus anchor: {error}"))?;
        let process_id = std::process::id();
        if window_process_id(hwnd) != process_id {
            let _ = unsafe { DestroyWindow(hwnd) };
            return Err("focus anchor HWND is not owned by the acceptance runner".into());
        }
        Ok(Self { hwnd, process_id })
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
            visible: unsafe { IsWindowVisible(self.hwnd) }.as_bool(),
            minimized: unsafe { IsIconic(self.hwnd) }.as_bool(),
            bounds: [rect.left, rect.top, rect.right, rect.bottom],
        })
    }

    pub fn focus(&self) -> Result<(), String> {
        focus_owned_window(self.hwnd, self.process_id)
    }
}

impl Drop for FocusAnchor {
    fn drop(&mut self) {
        if window_process_id(self.hwnd) == self.process_id {
            let _ = unsafe { DestroyWindow(self.hwnd) };
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
        let down = [key_input(VK_F11, false)];
        let down = send_validated_input(target_hwnd, target_process_id, &down, "F11 down")?;
        let mut release_guard = F11ReleaseGuard::new(target_hwnd, target_process_id);
        release_guard.armed = true;
        std::thread::sleep(down_time);
        let up = release_guard.release()?;
        Ok(F11TapEvidence { down, up })
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

    pub fn try_wait(&mut self) -> Result<Option<NativeExitStatus>, String> {
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
    let mut environment = acceptance_environment_block();
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

fn acceptance_environment_block() -> Vec<u16> {
    let mut entries = std::env::vars_os()
        .filter_map(|(name, value)| {
            let wide_name = name.encode_wide().collect::<Vec<_>>();
            if wide_key_eq_ascii(&wide_name, TRACE_ENV) {
                None
            } else {
                Some((wide_name, value.encode_wide().collect::<Vec<_>>()))
            }
        })
        .collect::<Vec<_>>();
    entries.push((TRACE_ENV.encode_utf16().collect(), vec![b'1' as u16]));
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
    let inserted = unsafe { SendInput(events, std::mem::size_of::<INPUT>() as i32) } as usize;
    if inserted != events.len() {
        // SendInput may report UIPI blocks without updating GetLastError, so retain both the
        // checked insertion count and the immediate last-error value for diagnosis.
        let last_error = unsafe { GetLastError() }.0;
        return Err(format!(
            "SendInput {operation} inserted {inserted}/{} events (immediate GetLastError=0x{last_error:08x}; UIPI may block without setting it)",
            events.len()
        ));
    }
    Ok(inserted)
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
    let input_desktop = input_desktop_evidence()?;
    focus_is_validated(target_hwnd, target_process_id)?;
    input_modifiers_clear()?;
    let (foreground, foreground_pid) = capture_foreground();
    if foreground != target_hwnd || foreground_pid != target_process_id {
        return Err(format!(
            "refused {operation}: target foreground changed before SendInput; expected HWND={} PID={target_process_id}, actual HWND={} PID={foreground_pid}; {input_desktop}",
            hwnd_id(target_hwnd),
            hwnd_id(foreground)
        ));
    }
    let at_unix_ms = unix_time_ms();
    let inserted = send_input_checked(events, operation)?;
    Ok(NativeInputEdgeEvidence {
        inserted,
        at_unix_ms,
        foreground_hwnd: hwnd_id(foreground),
        foreground_pid,
        input_desktop,
    })
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

fn key_input(key: VIRTUAL_KEY, key_up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: key,
                wScan: 0,
                dwFlags: if key_up {
                    KEYEVENTF_KEYUP
                } else {
                    Default::default()
                },
                time: 0,
                // Zero is ordinary SendInput provenance; never use the app's self-injection tag.
                dwExtraInfo: 0,
            },
        },
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

fn held_modifiers() -> String {
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
        .filter(|(_, key)| unsafe { GetAsyncKeyState(i32::from(key.0)) } < 0)
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

fn window_process_id(hwnd: HWND) -> u32 {
    if hwnd.is_invalid() {
        return 0;
    }
    let mut process_id = 0;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
    process_id
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

    let foreground = unsafe { GetForegroundWindow() };
    if foreground.is_invalid() {
        return focus_is_validated(hwnd, expected_pid);
    }
    let current_thread = unsafe { GetCurrentThreadId() };
    let foreground_thread = unsafe { GetWindowThreadProcessId(foreground, None) };
    if foreground_thread == 0 {
        return Err(
            "could not identify the current foreground thread for a validated focus transition"
                .into(),
        );
    }
    let attachment = InputThreadAttachment::attach(current_thread, foreground_thread)?;
    let _ = unsafe { SetForegroundWindow(hwnd) };
    let focus_result = focus_is_validated(hwnd, expected_pid);
    let detach_result = attachment.detach();
    focus_result?;
    detach_result
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
    unsafe { SetCursorPos(point.x, point.y) }
        .map_err(|error| format!("restore cursor position: {error}"))
}

pub(super) fn input_modifiers_clear() -> Result<(), String> {
    let held = held_modifiers();
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
    pub name: String,
    pub bounds: [i32; 4],
    pub process_id: u32,
    pub enabled: bool,
    pub button: bool,
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
            let control_type = unsafe { element.CurrentControlType() }
                .map_err(|error| format!("read semantic control role: {error}"))?;
            return Ok(Some(SemanticControl {
                element,
                name: name.to_string(),
                bounds: [bounds.left, bounds.top, bounds.right, bounds.bottom],
                process_id: expected_pid,
                enabled,
                button: control_type == UIA_ButtonControlTypeId,
            }));
        }
        Ok(None)
    }

    pub fn wait_named(
        &self,
        hwnd: HWND,
        expected_pid: u32,
        name: &str,
        timeout: Duration,
    ) -> Result<SemanticControl, String> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(control) = self.find_named(hwnd, expected_pid, name)? {
                return Ok(control);
            }
            if Instant::now() >= deadline {
                return Err(format!(
                    "UIA control '{}' was not published by process {} before timeout",
                    bounded_label(name),
                    expected_pid
                ));
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
            let name = unsafe { element.CurrentName() }
                .map_err(|error| format!("read UIA edit name: {error}"))?;
            return Ok(Some(SemanticControl {
                element,
                name: name.to_string(),
                bounds: [bounds.left, bounds.top, bounds.right, bounds.bottom],
                process_id: expected_pid,
                enabled,
                button: false,
            }));
        }
        Ok(None)
    }

    pub fn invoke(&self, control: &SemanticControl) -> Result<bool, String> {
        if !control.enabled {
            return Err(format!(
                "UIA control '{}' is disabled",
                bounded_label(&control.name)
            ));
        }
        let pattern = unsafe {
            control
                .element
                .GetCurrentPatternAs::<IUIAutomationInvokePattern>(UIA_InvokePatternId)
        };
        match pattern {
            Ok(pattern) => unsafe { pattern.Invoke() }
                .map(|()| true)
                .map_err(|error| format!("invoke semantic UIA control: {error}")),
            Err(_) => Ok(false),
        }
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

pub(super) fn click_semantic_control(
    child: &NativeChild,
    target: &WindowSnapshot,
    control: &SemanticControl,
) -> Result<PointerClickEvidence, String> {
    child.validate_window(target.hwnd)?;
    if control.process_id != child.process_id || !control.enabled {
        return Err("refused semantic click for disabled or foreign-process control".into());
    }
    if control.bounds[2] <= control.bounds[0] || control.bounds[3] <= control.bounds[1] {
        return Err("semantic control has empty screen bounds".into());
    }
    child.focus_window(target)?;

    let point = POINT {
        x: control.bounds[0] + (control.bounds[2] - control.bounds[0]) / 2,
        y: control.bounds[1] + (control.bounds[3] - control.bounds[1]) / 2,
    };
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
    if !unsafe { SetCursorPos(point.x, point.y) }.is_ok() {
        return Err("could not move cursor to the validated semantic point".into());
    }
    let under_cursor = unsafe { WindowFromPoint(point) };
    if under_cursor.is_invalid() || window_process_id(under_cursor) != child.process_id {
        return Err(
            "validated click point is covered by a window outside the child process".into(),
        );
    }
    let down = [mouse_input(true)];
    let down = send_validated_input(
        target.hwnd,
        child.process_id(),
        &down,
        "semantic click down",
    )?;
    let mut button_guard = MouseButtonGuard::new(target.hwnd, child.process_id());
    button_guard.armed = true;
    // Allow the target's native message loop to observe the pressed state before release.
    // This remains a real pointer click and is well below the fixture's hold threshold.
    std::thread::sleep(Duration::from_millis(120));
    let up = button_guard.release()?;
    Ok(PointerClickEvidence { down, up })
}

pub(super) fn send_text(
    child: &NativeChild,
    target: &WindowSnapshot,
    control: &SemanticControl,
    uia: &UiAutomation,
    text: &str,
) -> Result<usize, String> {
    child.validate_window(target.hwnd)?;
    if control.process_id != child.process_id || !control.enabled {
        return Err("refused text input for disabled or foreign-process UIA control".into());
    }
    child.focus_window(target)?;
    uia.focus(control)?;
    focus_is_validated(target.hwnd, child.process_id)?;
    input_modifiers_clear()?;
    let mut events = Vec::with_capacity(text.encode_utf16().count().saturating_mul(2));
    for code_unit in text.encode_utf16() {
        events.push(unicode_input(code_unit, false));
        events.push(unicode_input(code_unit, true));
    }
    let expected = events.len();
    if expected == 0 {
        return Ok(0);
    }
    send_input_checked(&events, "Unicode text")
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

struct MouseButtonGuard {
    target_hwnd: HWND,
    target_process_id: u32,
    armed: bool,
}

struct F11ReleaseGuard {
    target_hwnd: HWND,
    target_process_id: u32,
    armed: bool,
}

impl F11ReleaseGuard {
    fn new(target_hwnd: HWND, target_process_id: u32) -> Self {
        Self {
            target_hwnd,
            target_process_id,
            armed: false,
        }
    }

    fn release(&mut self) -> Result<NativeInputEdgeEvidence, String> {
        if !self.armed {
            return Err("F11 release was requested before a down event".into());
        }
        if focus_is_validated(self.target_hwnd, self.target_process_id).is_err() {
            focus_owned_window(self.target_hwnd, self.target_process_id)?;
        }
        let up = [key_input(VK_F11, true)];
        let evidence =
            send_validated_input(self.target_hwnd, self.target_process_id, &up, "F11 up")?;
        self.armed = false;
        Ok(evidence)
    }
}

impl Drop for F11ReleaseGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = self.release();
        }
    }
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
