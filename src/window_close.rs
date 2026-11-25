//! Checked, one-shot close requests for invocation-scoped Window Search rows.
use crate::window_catalog::{WindowDescriptor, WindowNativeIdentity, WindowSwitchIdentity};
use std::{fmt, sync::Arc};

#[derive(Clone)]
pub struct CheckedWindowClose {
    pub initiating: WindowNativeIdentity,
    pub selected: WindowSwitchIdentity,
    /// Exact title published by the source row; Switch deliberately does not use it.
    pub published_title: String,
    pub is_current: Arc<dyn Fn() -> bool + Send + Sync>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WindowCloseError {
    Superseded,
    TargetChanged,
    WrongDesktop,
    DesktopUnavailable(String),
    PostFailed(String),
    Unsupported,
}

impl fmt::Display for WindowCloseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Superseded => {
                f.write_str("The launcher or Window Search request is no longer current")
            }
            Self::TargetChanged => {
                f.write_str("The selected window is no longer the published target")
            }
            Self::WrongDesktop => f.write_str("The selected window is on another virtual desktop"),
            Self::DesktopUnavailable(error) => {
                write!(f, "Could not check the target desktop: {error}")
            }
            Self::PostFailed(error) => {
                write!(f, "Could not post the window close request: {error}")
            }
            Self::Unsupported => f.write_str("Window close is available only on Windows"),
        }
    }
}
impl std::error::Error for WindowCloseError {}

/// Success means the close message was queued; the target decides how to respond.
pub fn post_checked_window_close(request: CheckedWindowClose) -> Result<(), WindowCloseError> {
    #[cfg(windows)]
    {
        checked_close_with(&mut NativeBackend, &request)
    }
    #[cfg(not(windows))]
    {
        let _ = request;
        Err(WindowCloseError::Unsupported)
    }
}

trait CloseBackend {
    fn identity(&mut self, hwnd: usize) -> Option<WindowNativeIdentity>;
    fn target(&mut self, hwnd: usize) -> Option<(WindowDescriptor, WindowSwitchIdentity)>;
    fn title(&mut self, hwnd: usize) -> Option<String>;
    fn foreground(&mut self) -> Option<WindowNativeIdentity>;
    fn on_current_desktop(&mut self, hwnd: usize) -> Result<bool, String>;
    fn post_close(&mut self, hwnd: usize) -> Result<(), String>;
}

fn validate_current(
    backend: &mut impl CloseBackend,
    request: &CheckedWindowClose,
) -> Result<(), WindowCloseError> {
    if !(request.is_current)()
        || backend.identity(request.initiating.hwnd) != Some(request.initiating)
        || backend.foreground() != Some(request.initiating)
    {
        return Err(WindowCloseError::Superseded);
    }
    let selected = &request.selected;
    if selected.native.hwnd == 0
        || selected.native.pid == 0
        || selected.native.tid == 0
        || selected.native.pid == request.initiating.pid
        || selected.native.hwnd == request.initiating.hwnd
    {
        return Err(WindowCloseError::TargetChanged);
    }
    let Some((descriptor, identity)) = backend.target(selected.native.hwnd) else {
        return Err(WindowCloseError::TargetChanged);
    };
    if &identity != selected
        || !selected.descriptor.matches(&descriptor)
        || descriptor.hwnd != selected.native.hwnd
        || descriptor.pid != selected.native.pid
        || descriptor.title != request.published_title
    {
        return Err(WindowCloseError::TargetChanged);
    }
    if backend.identity(request.initiating.hwnd) != Some(request.initiating)
        || backend.foreground() != Some(request.initiating)
        || !(request.is_current)()
    {
        return Err(WindowCloseError::Superseded);
    }
    Ok(())
}

fn checked_close_with(
    backend: &mut impl CloseBackend,
    request: &CheckedWindowClose,
) -> Result<(), WindowCloseError> {
    validate_current(backend, request)?;
    if !backend
        .on_current_desktop(request.selected.native.hwnd)
        .map_err(WindowCloseError::DesktopUnavailable)?
    {
        return Err(WindowCloseError::WrongDesktop);
    }
    // Enrichment precedes the desktop check; re-read only the cheap caption
    // and native identities before the single post.
    if backend.title(request.selected.native.hwnd).as_deref()
        != Some(request.published_title.as_str())
    {
        return Err(WindowCloseError::TargetChanged);
    }
    if backend.identity(request.selected.native.hwnd) != Some(request.selected.native) {
        return Err(WindowCloseError::TargetChanged);
    }
    if backend.identity(request.initiating.hwnd) != Some(request.initiating)
        || backend.foreground() != Some(request.initiating)
        || !(request.is_current)()
    {
        return Err(WindowCloseError::Superseded);
    }
    backend
        .post_close(request.selected.native.hwnd)
        .map_err(WindowCloseError::PostFailed)
}

#[cfg(windows)]
struct NativeBackend;
#[cfg(windows)]
impl CloseBackend for NativeBackend {
    fn identity(&mut self, hwnd: usize) -> Option<WindowNativeIdentity> {
        crate::window_catalog::native_window_identity(hwnd)
    }
    fn target(&mut self, hwnd: usize) -> Option<(WindowDescriptor, WindowSwitchIdentity)> {
        crate::window_catalog::current_switch_window(hwnd)
    }
    fn title(&mut self, hwnd: usize) -> Option<String> {
        crate::window_catalog::current_window_title(hwnd)
    }
    fn foreground(&mut self) -> Option<WindowNativeIdentity> {
        use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
        crate::window_catalog::native_window_identity(unsafe { GetForegroundWindow() }.0 as usize)
    }
    fn on_current_desktop(&mut self, hwnd: usize) -> Result<bool, String> {
        use windows::Win32::Foundation::HWND;
        crate::virtual_desktop::VirtualDesktopService
            .is_window_on_current_desktop(HWND(hwnd as *mut _))
            .map_err(|error| error.to_string())
    }
    fn post_close(&mut self, hwnd: usize) -> Result<(), String> {
        use windows::Win32::{
            Foundation::{HWND, LPARAM, WPARAM},
            UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE},
        };
        // SAFETY: the exact HWND was checked immediately before this asynchronous post.
        unsafe { PostMessageW(HWND(hwnd as *mut _), WM_CLOSE, WPARAM(0), LPARAM(0)) }
            .map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::{CheckedWindowClose, CloseBackend, WindowCloseError, checked_close_with};
    use crate::window_catalog::{
        WindowDescriptor, WindowNativeIdentity, WindowSwitchIdentity, WindowTargetIdentity,
    };
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    struct Fake {
        root: WindowNativeIdentity,
        foreground: Option<WindowNativeIdentity>,
        target: Option<(WindowDescriptor, WindowSwitchIdentity)>,
        desktop: Result<bool, String>,
        invalidate_at_desktop: Option<Arc<AtomicBool>>,
        change_at_desktop: bool,
        leave_desktop_at_enrichment: bool,
        change_title_at_desktop: bool,
        calls: Vec<&'static str>,
        posts: Vec<usize>,
        fail_post: bool,
    }
    impl CloseBackend for Fake {
        fn identity(&mut self, hwnd: usize) -> Option<WindowNativeIdentity> {
            if hwnd == self.root.hwnd {
                Some(self.root)
            } else {
                self.target
                    .as_ref()
                    .map(|(_, id)| id.native)
                    .filter(|id| id.hwnd == hwnd)
            }
        }
        fn target(&mut self, _: usize) -> Option<(WindowDescriptor, WindowSwitchIdentity)> {
            self.calls.push("target");
            if self.leave_desktop_at_enrichment {
                self.desktop = Ok(false);
            }
            self.target.clone()
        }
        fn title(&mut self, _: usize) -> Option<String> {
            self.calls.push("title");
            self.target
                .as_ref()
                .map(|(descriptor, _)| descriptor.title.clone())
        }
        fn foreground(&mut self) -> Option<WindowNativeIdentity> {
            self.foreground
        }
        fn on_current_desktop(&mut self, _: usize) -> Result<bool, String> {
            self.calls.push("desktop");
            if let Some(current) = &self.invalidate_at_desktop {
                current.store(false, Ordering::Release);
            }
            if self.change_at_desktop {
                self.foreground = None;
            }
            if self.change_title_at_desktop {
                if let Some((descriptor, _)) = &mut self.target {
                    descriptor.title = "Changed".into();
                }
            }
            self.desktop.clone()
        }
        fn post_close(&mut self, hwnd: usize) -> Result<(), String> {
            self.posts.push(hwnd);
            if self.fail_post {
                Err("denied".into())
            } else {
                Ok(())
            }
        }
    }
    fn fixture() -> (Fake, CheckedWindowClose, Arc<AtomicBool>) {
        let root = WindowNativeIdentity {
            hwnd: 1,
            pid: 10,
            tid: 11,
        };
        let descriptor = WindowDescriptor {
            title: "Editor".into(),
            hwnd: 2,
            pid: 20,
            executable: Some("editor.exe".into()),
            process_path: None,
            class_name: Some("Editor".into()),
        };
        let selected = WindowSwitchIdentity {
            native: WindowNativeIdentity {
                hwnd: 2,
                pid: 20,
                tid: 21,
            },
            descriptor: WindowTargetIdentity::from_descriptor(&descriptor),
        };
        let current = Arc::new(AtomicBool::new(true));
        let guard = current.clone();
        let request = CheckedWindowClose {
            initiating: root,
            selected: selected.clone(),
            published_title: descriptor.title.clone(),
            is_current: Arc::new(move || guard.load(Ordering::Acquire)),
        };
        let fake = Fake {
            root,
            foreground: Some(root),
            target: Some((descriptor, selected)),
            desktop: Ok(true),
            invalidate_at_desktop: None,
            change_at_desktop: false,
            leave_desktop_at_enrichment: false,
            change_title_at_desktop: false,
            calls: Vec::new(),
            posts: Vec::new(),
            fail_post: false,
        };
        (fake, request, current)
    }

    #[test]
    fn checked_close_posts_exactly_once_and_propagates_failure() {
        let (mut fake, request, _) = fixture();
        assert_eq!(checked_close_with(&mut fake, &request), Ok(()));
        assert_eq!(fake.posts, vec![2]);
        assert_eq!(fake.calls, vec!["target", "desktop", "title"]);
        let (mut fake, request, _) = fixture();
        fake.fail_post = true;
        assert_eq!(
            checked_close_with(&mut fake, &request),
            Err(WindowCloseError::PostFailed("denied".into()))
        );
        assert_eq!(fake.posts, vec![2]);
    }

    #[test]
    fn checked_close_rejects_stale_foreground_target_and_desktop_without_posting() {
        let (mut fake, request, current) = fixture();
        fake.invalidate_at_desktop = Some(current);
        assert_eq!(
            checked_close_with(&mut fake, &request),
            Err(WindowCloseError::Superseded)
        );
        assert!(fake.posts.is_empty());
        let (mut fake, request, _) = fixture();
        fake.change_at_desktop = true;
        assert_eq!(
            checked_close_with(&mut fake, &request),
            Err(WindowCloseError::Superseded)
        );
        assert!(fake.posts.is_empty());
        let (mut fake, request, _) = fixture();
        fake.foreground = Some(WindowNativeIdentity {
            hwnd: 3,
            pid: 30,
            tid: 31,
        });
        assert_eq!(
            checked_close_with(&mut fake, &request),
            Err(WindowCloseError::Superseded)
        );
        assert!(fake.posts.is_empty());
        let (mut fake, request, _) = fixture();
        fake.target.as_mut().unwrap().1.native.tid += 1;
        assert_eq!(
            checked_close_with(&mut fake, &request),
            Err(WindowCloseError::TargetChanged)
        );
        assert!(fake.posts.is_empty());
        let (mut fake, mut request, _) = fixture();
        request.selected.native.hwnd = 99;
        assert_eq!(
            checked_close_with(&mut fake, &request),
            Err(WindowCloseError::TargetChanged)
        );
        assert!(fake.posts.is_empty());
        let (mut fake, request, _) = fixture();
        fake.target.as_mut().unwrap().0.title = "Changed".into();
        assert_eq!(
            checked_close_with(&mut fake, &request),
            Err(WindowCloseError::TargetChanged)
        );
        assert!(fake.posts.is_empty());
        let (mut fake, request, _) = fixture();
        fake.desktop = Ok(false);
        assert_eq!(
            checked_close_with(&mut fake, &request),
            Err(WindowCloseError::WrongDesktop)
        );
        assert!(fake.posts.is_empty());
        let (mut fake, request, _) = fixture();
        fake.desktop = Err("unavailable".into());
        assert_eq!(
            checked_close_with(&mut fake, &request),
            Err(WindowCloseError::DesktopUnavailable("unavailable".into()))
        );
        assert!(fake.posts.is_empty());
        let (mut fake, mut request, _) = fixture();
        request.selected.native.pid = request.initiating.pid;
        assert_eq!(
            checked_close_with(&mut fake, &request),
            Err(WindowCloseError::TargetChanged)
        );
        assert!(fake.posts.is_empty());
    }

    #[test]
    fn desktop_change_during_enrichment_prevents_post() {
        let (mut fake, request, _) = fixture();
        fake.leave_desktop_at_enrichment = true;
        assert_eq!(
            checked_close_with(&mut fake, &request),
            Err(WindowCloseError::WrongDesktop)
        );
        assert!(fake.posts.is_empty());
        assert_eq!(fake.calls, vec!["target", "desktop"]);
    }

    #[test]
    fn caption_change_during_desktop_check_prevents_post() {
        let (mut fake, request, _) = fixture();
        fake.change_title_at_desktop = true;
        assert_eq!(
            checked_close_with(&mut fake, &request),
            Err(WindowCloseError::TargetChanged)
        );
        assert_eq!(fake.calls, vec!["target", "desktop", "title"]);
        assert!(fake.posts.is_empty());
    }
}
