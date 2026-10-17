use super::{
    ClipboardRestoreOutcome, ClipboardRestorePlan, ClipboardSnapshot, ClipboardWriteFailure,
    SequencePlaybackError, SequencePlaybackPlatform, SequenceTarget, TargetObservation,
};
use crate::mkmacro::MkKey;
use crate::mkmacro::input::{LiveInputOptIn, Win32InputBackend};
use std::ptr;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{
    ERROR_SUCCESS, GetLastError, GlobalFree, HGLOBAL, HWND, SetLastError,
};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData,
    GetClipboardSequenceNumber, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{
    GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock,
};
use windows::Win32::System::Ole::{CF_LOCALE, CF_OEMTEXT, CF_TEXT, CF_UNICODETEXT};
use windows::Win32::UI::WindowsAndMessaging::{
    ES_PASSWORD, GUITHREADINFO, GWL_STYLE, GetForegroundWindow, GetGUIThreadInfo,
    GetWindowLongPtrW, GetWindowThreadProcessId, IsWindow,
};

const MAX_PRESERVED_TEXT_BYTES: usize = 16 * 1024 * 1024;

pub(crate) struct NativeSequencePlaybackPlatform {
    clipboard_owner: Option<HWND>,
    input: Win32InputBackend,
}

impl NativeSequencePlaybackPlatform {
    pub(crate) fn new(clipboard_owner_hwnd: Option<usize>) -> Self {
        Self {
            clipboard_owner: clipboard_owner_hwnd.map(|hwnd| HWND(hwnd as *mut std::ffi::c_void)),
            input: Win32InputBackend::system(LiveInputOptIn::production()),
        }
    }

    /// Shutdown is the only path that waits, and it waits on a short-lived
    /// cleanup worker which never touches egui. The worker is always joined.
    pub(crate) fn restore_after_shutdown(
        plan: ClipboardRestorePlan,
        clipboard_owner_hwnd: Option<usize>,
    ) -> Result<(), SequencePlaybackError> {
        let ClipboardRestorePlan {
            due,
            expected_sequence,
            original,
        } = plan;
        let remaining = due.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            let mut platform = Self::new(clipboard_owner_hwnd);
            return match platform.restore_clipboard_if_owned(expected_sequence, &original) {
                ClipboardRestoreOutcome::Restored | ClipboardRestoreOutcome::OwnershipLost => {
                    Ok(())
                }
                ClipboardRestoreOutcome::Failed => {
                    Err(SequencePlaybackError::ClipboardRestoreFailed)
                }
            };
        }

        let worker = std::thread::Builder::new()
            .name("paste-sequence-clipboard-cleanup".into())
            .spawn(move || {
                std::thread::sleep(remaining);
                let mut platform = Self::new(clipboard_owner_hwnd);
                platform.restore_clipboard_if_owned(expected_sequence, &original)
            })
            .map_err(|_| SequencePlaybackError::ClipboardRestoreFailed)?;
        match worker.join() {
            Ok(ClipboardRestoreOutcome::Restored | ClipboardRestoreOutcome::OwnershipLost) => {
                Ok(())
            }
            Ok(ClipboardRestoreOutcome::Failed) | Err(_) => {
                Err(SequencePlaybackError::ClipboardRestoreFailed)
            }
        }
    }

    fn open_clipboard(&self) -> Result<ClipboardGuard, SequencePlaybackError> {
        let owner = self
            .clipboard_owner
            .ok_or(SequencePlaybackError::ClipboardUnavailable)?;
        ClipboardGuard::open(owner)
    }
}

impl SequencePlaybackPlatform for NativeSequencePlaybackPlatform {
    fn now(&mut self) -> Instant {
        Instant::now()
    }

    fn capture_clipboard(&mut self) -> Result<ClipboardSnapshot, SequencePlaybackError> {
        let _clipboard = self.open_clipboard()?;
        let sequence_number = unsafe { GetClipboardSequenceNumber() };
        if sequence_number == 0 {
            return Err(SequencePlaybackError::ClipboardUnavailable);
        }

        let mut current = 0;
        let mut has_unicode_text = false;
        loop {
            unsafe { SetLastError(ERROR_SUCCESS) };
            let next = unsafe { EnumClipboardFormats(current) };
            if next == 0 {
                let error = unsafe { GetLastError() };
                if error != ERROR_SUCCESS {
                    return Err(SequencePlaybackError::ClipboardUnavailable);
                }
                break;
            }
            current = next;
            match next {
                format if format == u32::from(CF_UNICODETEXT.0) => has_unicode_text = true,
                format
                    if format == u32::from(CF_TEXT.0)
                        || format == u32::from(CF_OEMTEXT.0)
                        || format == u32::from(CF_LOCALE.0) => {}
                _ => return Err(SequencePlaybackError::UnsupportedClipboard),
            }
        }

        let text = if has_unicode_text {
            Some(read_unicode_text()?)
        } else if current == 0 {
            None
        } else {
            // CF_TEXT-only clipboard data can depend on a code page. Refuse it
            // rather than silently changing bytes when restoring as Unicode.
            return Err(SequencePlaybackError::UnsupportedClipboard);
        };
        Ok(ClipboardSnapshot::new(sequence_number, text))
    }

    fn observe_target(&mut self, target: &SequenceTarget) -> TargetObservation {
        let target_hwnd = target_hwnd(target);
        if target_hwnd.0.is_null() || !unsafe { IsWindow(target_hwnd) }.as_bool() {
            return TargetObservation::Invalid;
        }

        let mut target_process = 0;
        let target_thread =
            unsafe { GetWindowThreadProcessId(target_hwnd, Some(&mut target_process)) };
        if target_process != target.process_id() || target_thread != target.thread_id() {
            return TargetObservation::Invalid;
        }

        let foreground = unsafe { GetForegroundWindow() };
        if foreground.0.is_null() {
            return TargetObservation::Unavailable;
        }
        let mut foreground_process = 0;
        let foreground_thread =
            unsafe { GetWindowThreadProcessId(foreground, Some(&mut foreground_process)) };
        if foreground_process == 0 || foreground_thread == 0 {
            return TargetObservation::Unavailable;
        }
        if foreground_process == std::process::id() {
            return TargetObservation::Launcher;
        }
        if foreground == target_hwnd
            && foreground_process == target.process_id()
            && foreground_thread == target.thread_id()
        {
            return match focused_control_is_password(foreground_thread) {
                Some(password_control) => TargetObservation::Target { password_control },
                None => TargetObservation::Unavailable,
            };
        }
        TargetObservation::Other
    }

    fn current_target(&mut self) -> Result<SequenceTarget, SequencePlaybackError> {
        let hwnd = unsafe { GetForegroundWindow() };
        if hwnd.0.is_null() || !unsafe { IsWindow(hwnd) }.as_bool() {
            return Err(SequencePlaybackError::FocusChanged);
        }
        let mut process_id = 0;
        let thread_id = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
        if process_id == 0 || thread_id == 0 || process_id == std::process::id() {
            return Err(SequencePlaybackError::FocusChanged);
        }
        // Titles are invocation-time template inputs. Target switching must not
        // perform another title read or change any already-rendered step.
        SequenceTarget::from_external_window(
            crate::active_window::ExternalForegroundWindow {
                hwnd: hwnd.0 as isize,
                process_id,
                thread_id,
                title: None,
            },
            std::process::id(),
        )
        .map_err(|_| SequencePlaybackError::InvalidTarget)
    }

    fn publish_text(
        &mut self,
        expected_sequence: u32,
        rollback_text: Option<&str>,
        text: &str,
    ) -> Result<u32, ClipboardWriteFailure> {
        let fail = |error, owned_sequence| ClipboardWriteFailure {
            error,
            owned_sequence,
        };
        let _clipboard = self.open_clipboard().map_err(|error| fail(error, None))?;
        if unsafe { GetClipboardSequenceNumber() } != expected_sequence {
            return Err(fail(SequencePlaybackError::ClipboardChanged, None));
        }
        if unsafe { EmptyClipboard() }.is_err() {
            return Err(fail(SequencePlaybackError::ClipboardWriteFailed, None));
        }

        match write_unicode_text(text) {
            Ok(()) => Ok(unsafe { GetClipboardSequenceNumber() }),
            Err(()) => {
                // We still own the clipboard lock. Restore the previously
                // observed text (or emptiness) before releasing it, then return
                // the resulting sequence number for conditional final cleanup.
                if let Some(previous) = rollback_text {
                    let _ = write_unicode_text(previous);
                }
                let owned_sequence = Some(unsafe { GetClipboardSequenceNumber() });
                Err(fail(
                    SequencePlaybackError::ClipboardWriteFailed,
                    owned_sequence,
                ))
            }
        }
    }

    fn clipboard_sequence_matches(&mut self, expected_sequence: u32) -> bool {
        self.open_clipboard()
            .is_ok_and(|_clipboard| (unsafe { GetClipboardSequenceNumber() }) == expected_sequence)
    }

    fn restore_clipboard_if_owned(
        &mut self,
        expected_sequence: u32,
        original: &ClipboardSnapshot,
    ) -> ClipboardRestoreOutcome {
        let Ok(_clipboard) = self.open_clipboard() else {
            return ClipboardRestoreOutcome::Failed;
        };
        if unsafe { GetClipboardSequenceNumber() } != expected_sequence {
            return ClipboardRestoreOutcome::OwnershipLost;
        }
        if unsafe { EmptyClipboard() }.is_err() {
            return ClipboardRestoreOutcome::Failed;
        }
        if original
            .text
            .as_deref()
            .is_some_and(|text| write_unicode_text(text).is_err())
        {
            return ClipboardRestoreOutcome::Failed;
        }
        ClipboardRestoreOutcome::Restored
    }

    fn paste_chord(&mut self) -> Result<(), SequencePlaybackError> {
        self.input
            .chord(&[MkKey::Control, MkKey::Character("V".into())])
            .map_err(|_| SequencePlaybackError::InputRejected)
    }
}

fn target_hwnd(target: &SequenceTarget) -> HWND {
    HWND(target.hwnd() as *mut std::ffi::c_void)
}

struct ClipboardGuard;

impl ClipboardGuard {
    fn open(owner: HWND) -> Result<Self, SequencePlaybackError> {
        unsafe { OpenClipboard(owner) }.map_err(|_| SequencePlaybackError::ClipboardUnavailable)?;
        Ok(Self)
    }
}

impl Drop for ClipboardGuard {
    fn drop(&mut self) {
        let _ = unsafe { CloseClipboard() };
    }
}

struct GlobalLockGuard(HGLOBAL);

impl Drop for GlobalLockGuard {
    fn drop(&mut self) {
        let _ = unsafe { GlobalUnlock(self.0) };
    }
}

fn read_unicode_text() -> Result<String, SequencePlaybackError> {
    let data = unsafe { GetClipboardData(u32::from(CF_UNICODETEXT.0)) }
        .map_err(|_| SequencePlaybackError::UnsupportedClipboard)?;
    let memory = HGLOBAL(data.0);
    let size = unsafe { GlobalSize(memory) };
    if size < std::mem::size_of::<u16>() || size > MAX_PRESERVED_TEXT_BYTES {
        return Err(SequencePlaybackError::UnsupportedClipboard);
    }
    let pointer = unsafe { GlobalLock(memory) }.cast::<u16>();
    if pointer.is_null() {
        return Err(SequencePlaybackError::ClipboardUnavailable);
    }
    let _locked = GlobalLockGuard(memory);
    let length = size / std::mem::size_of::<u16>();
    // SAFETY: GlobalSize bounds the locked HGLOBAL and the pointer is valid
    // until GlobalUnlock in the guard's Drop.
    let units = unsafe { std::slice::from_raw_parts(pointer, length) };
    let end = units
        .iter()
        .position(|unit| *unit == 0)
        .ok_or(SequencePlaybackError::UnsupportedClipboard)?;
    String::from_utf16(&units[..end]).map_err(|_| SequencePlaybackError::UnsupportedClipboard)
}

fn write_unicode_text(text: &str) -> Result<(), ()> {
    let mut units = text.encode_utf16().collect::<Vec<_>>();
    units.push(0);
    let byte_count = units.len().saturating_mul(std::mem::size_of::<u16>());
    if byte_count > MAX_PRESERVED_TEXT_BYTES {
        return Err(());
    }
    let memory = unsafe { GlobalAlloc(GMEM_MOVEABLE, byte_count) }.map_err(|_| ())?;
    let pointer = unsafe { GlobalLock(memory) }.cast::<u16>();
    if pointer.is_null() {
        let _ = unsafe { GlobalFree(memory) };
        return Err(());
    }
    // SAFETY: the allocation has byte_count bytes and units contains exactly
    // byte_count / 2 initialized UTF-16 code units.
    unsafe { ptr::copy_nonoverlapping(units.as_ptr(), pointer, units.len()) };
    let _ = unsafe { GlobalUnlock(memory) };
    let handle = windows::Win32::Foundation::HANDLE(memory.0);
    if unsafe { SetClipboardData(u32::from(CF_UNICODETEXT.0), handle) }.is_err() {
        let _ = unsafe { GlobalFree(memory) };
        return Err(());
    }
    Ok(())
}

fn focused_control_is_password(thread_id: u32) -> Option<bool> {
    let mut information = GUITHREADINFO {
        cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
        ..Default::default()
    };
    if unsafe { GetGUIThreadInfo(thread_id, &mut information) }.is_err()
        || information.hwndFocus.0.is_null()
    {
        return None;
    }
    // A failed style query is unknown, never evidence of an unprotected control.
    unsafe { SetLastError(ERROR_SUCCESS) };
    let style = unsafe { GetWindowLongPtrW(information.hwndFocus, GWL_STYLE) };
    if style == 0 && unsafe { GetLastError() } != ERROR_SUCCESS {
        return None;
    }
    Some(style as u32 & ES_PASSWORD as u32 != 0)
}
