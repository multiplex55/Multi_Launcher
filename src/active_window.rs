/// A direct snapshot of the foreground window before launcher activation.
#[derive(Clone, PartialEq, Eq)]
pub struct ExternalForegroundWindow {
    pub(crate) hwnd: isize,
    pub(crate) title: Option<String>,
}

pub(crate) enum ForegroundWindowObservation {
    External(ExternalForegroundWindow),
    Launcher,
    Unavailable,
}

/// Capture the actual external foreground window without searching z-order.
/// An unavailable title remains `None`; callers resolve it to empty template text.
pub fn capture_external_foreground_window() -> Option<ExternalForegroundWindow> {
    match observe_foreground_window() {
        ForegroundWindowObservation::External(window) => Some(window),
        ForegroundWindowObservation::Launcher | ForegroundWindowObservation::Unavailable => None,
    }
}

pub(crate) fn observe_foreground_window() -> ForegroundWindowObservation {
    #[cfg(windows)]
    {
        use windows::Win32::UI::WindowsAndMessaging::{
            GetForegroundWindow, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
        };

        let hwnd = unsafe { GetForegroundWindow() };
        if hwnd.0.is_null() {
            return ForegroundWindowObservation::Unavailable;
        }

        let mut process_id = 0;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
        if process_id == 0 {
            return ForegroundWindowObservation::Unavailable;
        }
        if process_id == std::process::id() {
            return ForegroundWindowObservation::Launcher;
        }

        let length = unsafe { GetWindowTextLengthW(hwnd) }.max(0) as usize;
        let title = if length == 0 {
            None
        } else {
            let mut title_buf = vec![0u16; length + 1];
            let read = unsafe { GetWindowTextW(hwnd, &mut title_buf) }.max(0) as usize;
            (read > 0).then(|| String::from_utf16_lossy(&title_buf[..read]))
        };
        ForegroundWindowObservation::External(ExternalForegroundWindow {
            hwnd: hwnd.0 as isize,
            title,
        })
    }
    #[cfg(not(windows))]
    {
        ForegroundWindowObservation::Unavailable
    }
}

/// Resolve the user's previously active eligible application window while the launcher owns
/// foreground focus. The native implementation walks downward in z-order and excludes every
/// window owned by this process.
pub fn resolve_previous_active_window() -> Result<usize, String> {
    #[cfg(windows)]
    {
        resolve_with(&mut WindowsActiveWindowBackend)
    }
    #[cfg(not(windows))]
    {
        Err("Active-window resolution is available only on Windows".into())
    }
}

trait ActiveWindowBackend {
    fn foreground(&mut self) -> Option<usize>;
    fn next_in_z_order(&mut self, hwnd: usize) -> Option<usize>;
    fn is_eligible(&mut self, hwnd: usize) -> bool;
    fn process_id(&mut self, hwnd: usize) -> u32;
    fn current_process_id(&mut self) -> u32;
}

fn resolve_with(backend: &mut impl ActiveWindowBackend) -> Result<usize, String> {
    let mut candidate = backend.foreground();
    let own_pid = backend.current_process_id();
    for _ in 0..256 {
        let Some(hwnd) = candidate else { break };
        if backend.is_eligible(hwnd) && backend.process_id(hwnd) != own_pid {
            return Ok(hwnd);
        }
        candidate = backend.next_in_z_order(hwnd);
    }
    Err("No eligible non-launcher active window is available".into())
}

#[cfg(windows)]
struct WindowsActiveWindowBackend;

#[cfg(windows)]
impl ActiveWindowBackend for WindowsActiveWindowBackend {
    fn foreground(&mut self) -> Option<usize> {
        let hwnd = unsafe { windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow() };
        (!hwnd.0.is_null()).then_some(hwnd.0 as usize)
    }

    fn next_in_z_order(&mut self, hwnd: usize) -> Option<usize> {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{GW_HWNDNEXT, GetWindow};
        let next = unsafe { GetWindow(HWND(hwnd as *mut _), GW_HWNDNEXT) }.unwrap_or_default();
        (!next.0.is_null()).then_some(next.0 as usize)
    }

    fn is_eligible(&mut self, hwnd: usize) -> bool {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{GW_OWNER, GetWindow, IsWindowVisible};
        let hwnd = HWND(hwnd as *mut _);
        unsafe { IsWindowVisible(hwnd) }.as_bool()
            && unsafe { GetWindow(hwnd, GW_OWNER) }
                .unwrap_or_default()
                .0
                .is_null()
    }

    fn process_id(&mut self, hwnd: usize) -> u32 {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;
        let mut pid = 0;
        unsafe { GetWindowThreadProcessId(HWND(hwnd as *mut _), Some(&mut pid)) };
        pid
    }

    fn current_process_id(&mut self) -> u32 {
        std::process::id()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    struct FakeBackend {
        foreground: Option<usize>,
        next: HashMap<usize, usize>,
        eligible: HashSet<usize>,
        pids: HashMap<usize, u32>,
        own_pid: u32,
    }

    impl ActiveWindowBackend for FakeBackend {
        fn foreground(&mut self) -> Option<usize> {
            self.foreground
        }
        fn next_in_z_order(&mut self, hwnd: usize) -> Option<usize> {
            self.next.get(&hwnd).copied()
        }
        fn is_eligible(&mut self, hwnd: usize) -> bool {
            self.eligible.contains(&hwnd)
        }
        fn process_id(&mut self, hwnd: usize) -> u32 {
            self.pids[&hwnd]
        }
        fn current_process_id(&mut self) -> u32 {
            self.own_pid
        }
    }

    #[test]
    fn foreground_external_window_is_used_directly() {
        let mut backend = FakeBackend {
            foreground: Some(7),
            next: HashMap::new(),
            eligible: HashSet::from([7]),
            pids: HashMap::from([(7, 20)]),
            own_pid: 10,
        };
        assert_eq!(resolve_with(&mut backend).unwrap(), 7);
    }

    #[test]
    fn launcher_foreground_falls_back_deterministically_in_z_order() {
        let mut backend = FakeBackend {
            foreground: Some(1),
            next: HashMap::from([(1, 2), (2, 3)]),
            eligible: HashSet::from([1, 3]),
            pids: HashMap::from([(1, 10), (2, 30), (3, 40)]),
            own_pid: 10,
        };
        assert_eq!(resolve_with(&mut backend).unwrap(), 3);
    }

    #[test]
    fn own_process_is_never_selected() {
        let mut backend = FakeBackend {
            foreground: Some(1),
            next: HashMap::new(),
            eligible: HashSet::from([1]),
            pids: HashMap::from([(1, 10)]),
            own_pid: 10,
        };
        assert!(resolve_with(&mut backend).is_err());
    }
}
