//! Window discovery and matching. Persist matchers, never native handles.
use super::{
    DiagnosticKind, ExecResult, ExecutionDiagnostic, MkWindowMatcher, MkWindowMoveResizePayload,
    MkWindowPayload, MkWindowState, WindowBackend,
};
use crate::multi_manager::model::MmRect;

/// Combines optional operations with the current rectangle. The platform mover
/// restores minimized windows before applying this rectangle; it does not
/// explicitly restore maximized windows.
pub(crate) fn merge_window_geometry(
    current: MmRect,
    p: &MkWindowMoveResizePayload,
) -> ExecResult<MmRect> {
    let (x, y): (i32, i32) = match (p.x, p.y) {
        (Some(x), Some(y)) => (x, y),
        (None, None) => (current.x, current.y),
        _ => {
            return Err(ExecutionDiagnostic::new(
                DiagnosticKind::InvalidTarget,
                "Move requires both X and Y",
            ));
        }
    };
    let (w, h): (i32, i32) = match (p.width, p.height) {
        (Some(w), Some(h)) if w > 0 && h > 0 => (
            i32::try_from(w).map_err(|_| {
                ExecutionDiagnostic::new(
                    DiagnosticKind::InvalidTarget,
                    "window width exceeds the platform range",
                )
            })?,
            i32::try_from(h).map_err(|_| {
                ExecutionDiagnostic::new(
                    DiagnosticKind::InvalidTarget,
                    "window height exceeds the platform range",
                )
            })?,
        ),
        (None, None) => (current.w, current.h),
        (Some(0), _) | (_, Some(0)) => {
            return Err(ExecutionDiagnostic::new(
                DiagnosticKind::InvalidTarget,
                "window dimensions must be positive",
            ));
        }
        _ => {
            return Err(ExecutionDiagnostic::new(
                DiagnosticKind::InvalidTarget,
                "Resize requires both Width and Height",
            ));
        }
    };
    x.checked_add(w)
        .and_then(|_| y.checked_add(h))
        .ok_or_else(|| {
            ExecutionDiagnostic::new(
                DiagnosticKind::InvalidTarget,
                "window rectangle arithmetic overflow",
            )
        })?;
    Ok(MmRect { x, y, w, h })
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowCandidate {
    pub handle: usize,
    pub title: String,
    pub executable: String,
    pub process_path: String,
    pub class_name: String,
}
impl From<crate::multi_manager::win::EnumeratedWindow> for WindowCandidate {
    fn from(w: crate::multi_manager::win::EnumeratedWindow) -> Self {
        Self {
            handle: w.hwnd,
            title: w.title,
            executable: w.executable,
            process_path: w.process_path,
            class_name: w.class_name,
        }
    }
}
impl From<crate::multi_manager::win::CapturedWindow> for WindowCandidate {
    fn from(w: crate::multi_manager::win::CapturedWindow) -> Self {
        Self {
            handle: w.hwnd,
            title: w.title,
            executable: w.executable,
            process_path: w.process_path,
            class_name: w.class_name,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmbiguityPolicy {
    Error,
    First,
}
fn same_path(a: &str, b: &str) -> bool {
    a.replace('/', "\\")
        .eq_ignore_ascii_case(&b.replace('/', "\\"))
}
fn basename(s: &str) -> &str {
    s.rsplit(['/', '\\']).next().unwrap_or(s)
}
pub fn candidate_matches(m: &MkWindowMatcher, c: &WindowCandidate) -> ExecResult<bool> {
    if let Some(p) = m.process.as_deref() {
        let yes = if p.contains(['/', '\\']) {
            same_path(p, &c.process_path)
        } else {
            basename(p).eq_ignore_ascii_case(&c.executable)
                || basename(p).eq_ignore_ascii_case(basename(&c.process_path))
        };
        if !yes {
            return Ok(false);
        }
    }
    if let Some(class) = m.class.as_deref()
        && !class.eq_ignore_ascii_case(&c.class_name)
    {
        return Ok(false);
    }
    if let Some(pattern) = m.title_regex.as_deref() {
        let r = regex::Regex::new(pattern).map_err(|e| {
            ExecutionDiagnostic::new(
                DiagnosticKind::InvalidTarget,
                format!("invalid window title regex: {e}"),
            )
        })?;
        if !r.is_match(&c.title) {
            return Ok(false);
        }
    } else if let Some(title) = m.title.as_deref()
        && !c.title.contains(title)
    {
        return Ok(false);
    }
    Ok(true)
}
pub fn resolve_window(
    m: &MkWindowMatcher,
    candidates: &[WindowCandidate],
    policy: AmbiguityPolicy,
) -> ExecResult<WindowCandidate> {
    let mut found = Vec::new();
    for c in candidates {
        if candidate_matches(m, c)? {
            found.push(c.clone())
        }
    }
    match found.len() {
        0 => Err(ExecutionDiagnostic::new(
            DiagnosticKind::TargetNotFound,
            "Window search target was not found",
        )
        .context("match_count", "0")),
        1 => Ok(found.remove(0)),
        _ if policy == AmbiguityPolicy::First => Ok(found.remove(0)),
        _ => {
            let summary = found
                .iter()
                .take(4)
                .map(|c| format!("{} ({})", c.title, c.executable))
                .collect::<Vec<_>>()
                .join(", ");
            Err(ExecutionDiagnostic::new(
                DiagnosticKind::AmbiguousTarget,
                "Window search target matched multiple windows",
            )
            .context("matches", summary)
            .context("match_count", found.len().to_string())
            .context("candidate_count", found.len().to_string()))
        }
    }
}
#[derive(Default)]
pub struct Win32WindowBackend;
impl Win32WindowBackend {
    fn candidates(&self) -> ExecResult<Vec<WindowCandidate>> {
        crate::multi_manager::win::enumerate_top_level_windows()
            .map(|v| v.into_iter().map(WindowCandidate::from).collect())
            .map_err(|e| {
                ExecutionDiagnostic::new(
                    DiagnosticKind::Backend,
                    format!("window enumeration failed: {e}"),
                )
            })
    }
    fn one(&self, m: &MkWindowMatcher) -> ExecResult<WindowCandidate> {
        resolve_window(m, &self.candidates()?, AmbiguityPolicy::Error)
    }
}
impl WindowBackend for Win32WindowBackend {
    fn exists(&self, m: &MkWindowMatcher) -> ExecResult<bool> {
        let c = self.candidates()?;
        for x in &c {
            if candidate_matches(m, x)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
    fn is_active(&self, m: &MkWindowMatcher) -> ExecResult<bool> {
        let Some(w) = crate::multi_manager::win::active_window() else {
            return Ok(false);
        };
        candidate_matches(m, &WindowCandidate::from(w))
    }
    fn activate(&self, p: &MkWindowPayload) -> ExecResult {
        activate_handle(self.one(&p.matcher)?.handle)
    }
    fn close(&self, m: &MkWindowMatcher) -> ExecResult {
        close_handle(self.one(m)?.handle)
    }
    fn move_resize(&self, p: &MkWindowMoveResizePayload) -> ExecResult {
        let handle = self.one(&p.matcher)?.handle;
        let current = crate::multi_manager::win::window_rect(handle).ok_or_else(|| {
            ExecutionDiagnostic::new(
                DiagnosticKind::Backend,
                "failed to read the resolved window rectangle",
            )
        })?;
        let rect = merge_window_geometry(current, p)?;
        crate::multi_manager::win::move_window_to_rect(handle, rect).map_err(|e| {
            ExecutionDiagnostic::new(
                DiagnosticKind::Backend,
                format!("failed to move/resize window: {e}"),
            )
        })
    }
    fn set_state(&self, m: &MkWindowMatcher, state: MkWindowState) -> ExecResult {
        set_state_handle(self.one(m)?.handle, state)
    }
}
#[cfg(not(windows))]
fn set_state_handle(_: usize, _: MkWindowState) -> ExecResult {
    Err(ExecutionDiagnostic::new(
        DiagnosticKind::UnsupportedOperation,
        "window state changes are available only on Windows",
    ))
}
#[cfg(windows)]
fn set_state_handle(h: usize, state: MkWindowState) -> ExecResult {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        IsWindow, SW_MAXIMIZE, SW_MINIMIZE, SW_RESTORE, ShowWindow,
    };
    let hwnd = HWND(h as *mut _);
    if !unsafe { IsWindow(hwnd) }.as_bool() {
        return Err(ExecutionDiagnostic::new(
            DiagnosticKind::InvalidTarget,
            "resolved window handle is no longer valid",
        ));
    }
    let command = match state {
        MkWindowState::Minimize => SW_MINIMIZE,
        MkWindowState::Maximize => SW_MAXIMIZE,
        MkWindowState::Restore => SW_RESTORE,
    };
    // ShowWindow returns the previous visibility state, not operation success.
    let _previously_visible = unsafe { ShowWindow(hwnd, command) };
    Ok(())
}
#[cfg(not(windows))]
fn activate_handle(_: usize) -> ExecResult {
    Err(ExecutionDiagnostic::new(
        DiagnosticKind::UnsupportedOperation,
        "window activation is available only on Windows",
    ))
}
#[cfg(not(windows))]
fn close_handle(_: usize) -> ExecResult {
    Err(ExecutionDiagnostic::new(
        DiagnosticKind::UnsupportedOperation,
        "window close is available only on Windows",
    ))
}
#[cfg(windows)]
fn activate_handle(h: usize) -> ExecResult {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, SetForegroundWindow};
    let hwnd = HWND(h as *mut _);
    if !unsafe { SetForegroundWindow(hwnd) }.as_bool() || unsafe { GetForegroundWindow() } != hwnd {
        return Err(ExecutionDiagnostic::new(
            DiagnosticKind::Backend,
            "Windows foreground-activation policy denied the request",
        ));
    }
    Ok(())
}
#[cfg(windows)]
fn close_handle(h: usize) -> ExecResult {
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};
    unsafe { PostMessageW(HWND(h as *mut _), WM_CLOSE, WPARAM(0), LPARAM(0)) }.map_err(|e| {
        ExecutionDiagnostic::new(
            DiagnosticKind::Backend,
            format!("failed to close window: {e}"),
        )
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn c(t: &str, e: &str) -> WindowCandidate {
        WindowCandidate {
            handle: 1,
            title: t.into(),
            executable: e.into(),
            process_path: format!("C:\\bin\\{e}"),
            class_name: "Class".into(),
        }
    }
    fn geometry(
        x: Option<i32>,
        y: Option<i32>,
        width: Option<u32>,
        height: Option<u32>,
    ) -> MkWindowMoveResizePayload {
        MkWindowMoveResizePayload {
            matcher: MkWindowMatcher::default(),
            x,
            y,
            width,
            height,
        }
    }
    #[test]
    fn geometry_merge_operations_and_overflow() {
        let old = MmRect {
            x: 10,
            y: 20,
            w: 300,
            h: 200,
        };
        assert_eq!(
            merge_window_geometry(old, &geometry(Some(-5), Some(-6), None, None)).unwrap(),
            MmRect {
                x: -5,
                y: -6,
                w: 300,
                h: 200
            }
        );
        assert_eq!(
            merge_window_geometry(old, &geometry(None, None, Some(40), Some(50))).unwrap(),
            MmRect {
                x: 10,
                y: 20,
                w: 40,
                h: 50
            }
        );
        assert_eq!(
            merge_window_geometry(old, &geometry(Some(1), Some(2), Some(3), Some(4))).unwrap(),
            MmRect {
                x: 1,
                y: 2,
                w: 3,
                h: 4
            }
        );
        assert!(
            merge_window_geometry(old, &geometry(Some(i32::MAX), Some(0), Some(1), Some(1)))
                .is_err()
        );
        assert!(
            merge_window_geometry(old, &geometry(None, None, Some(u32::MAX), Some(1))).is_err()
        );
    }
    #[test]
    fn matching_and_ambiguity() {
        let m = MkWindowMatcher {
            title: Some("Doc".into()),
            title_regex: None,
            process: Some("app.exe".into()),
            class: Some("class".into()),
        };
        assert!(candidate_matches(&m, &c("Document", "app.exe")).unwrap());
        let ambiguous = resolve_window(
            &m,
            &[c("Document", "app.exe"), c("Doc 2", "app.exe")],
            AmbiguityPolicy::Error,
        )
        .unwrap_err();
        assert_eq!(ambiguous.kind, DiagnosticKind::AmbiguousTarget);
        assert_eq!(
            ambiguous.message,
            "Window search target matched multiple windows"
        );
        assert_eq!(
            ambiguous.context.get("match_count").map(String::as_str),
            Some("2")
        );
        let missing = resolve_window(&m, &[], AmbiguityPolicy::Error).unwrap_err();
        assert_eq!(missing.kind, DiagnosticKind::TargetNotFound);
        assert_eq!(missing.message, "Window search target was not found");
        assert!(
            resolve_window(
                &m,
                &[c("Document", "app.exe"), c("Doc 2", "app.exe")],
                AmbiguityPolicy::First
            )
            .is_ok()
        )
    }
    #[test]
    fn invalid_regex() {
        let m = MkWindowMatcher {
            title: None,
            title_regex: Some("[".into()),
            process: None,
            class: None,
        };
        assert_eq!(
            candidate_matches(&m, &c("x", "x")).unwrap_err().kind,
            DiagnosticKind::InvalidTarget
        )
    }
}
