//! Window discovery and matching. Persist matchers, never native handles.
use super::{
    DiagnosticKind, ExecResult, ExecutionDiagnostic, MkWindowMatcher, MkWindowPayload,
    WindowBackend,
};
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowCandidate {
    pub handle: usize,
    pub title: String,
    pub executable: String,
    pub process_path: String,
    pub class_name: String,
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
            "matching window is missing",
        )),
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
                format!("window matcher is ambiguous: {summary}"),
            )
            .context("candidate_count", found.len().to_string()))
        }
    }
}
#[derive(Default)]
pub struct Win32WindowBackend;
impl Win32WindowBackend {
    fn candidates(&self) -> ExecResult<Vec<WindowCandidate>> {
        crate::multi_manager::win::enumerate_top_level_windows()
            .map(|v| {
                v.into_iter()
                    .map(|w| WindowCandidate {
                        handle: w.hwnd,
                        title: w.title,
                        executable: w.executable,
                        process_path: w.process_path,
                        class_name: w.class_name,
                    })
                    .collect()
            })
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
        candidate_matches(
            m,
            &WindowCandidate {
                handle: w.hwnd,
                title: w.title,
                executable: w.executable,
                process_path: w.process_path,
                class_name: w.class_name,
            },
        )
    }
    fn activate(&self, p: &MkWindowPayload) -> ExecResult {
        activate_handle(self.one(&p.matcher)?.handle)
    }
    fn close(&self, m: &MkWindowMatcher) -> ExecResult {
        close_handle(self.one(m)?.handle)
    }
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
    #[test]
    fn matching_and_ambiguity() {
        let m = MkWindowMatcher {
            title: Some("Doc".into()),
            title_regex: None,
            process: Some("app.exe".into()),
            class: Some("class".into()),
        };
        assert!(candidate_matches(&m, &c("Document", "app.exe")).unwrap());
        assert_eq!(
            resolve_window(
                &m,
                &[c("Document", "app.exe"), c("Doc 2", "app.exe")],
                AmbiguityPolicy::Error
            )
            .unwrap_err()
            .kind,
            DiagnosticKind::AmbiguousTarget
        );
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
