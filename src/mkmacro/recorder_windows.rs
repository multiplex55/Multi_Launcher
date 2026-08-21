//! Immediate, best-effort window metadata enrichment for recorder events.
use super::{EventContext, EventEnricher, HookEvent, MkPoint, WindowContext};

/// Injectable native-window seam. Implementations must treat every metadata failure as absent data.
pub trait WindowMetadata: Send {
    fn foreground_root(&self) -> Option<usize>;
    fn root_under_point(&self, point: MkPoint) -> Option<usize>;
    fn context(&self, root: usize) -> Option<WindowContext>;
}

pub struct WindowsEventEnricher<M = SystemWindowMetadata> {
    metadata: M,
}
impl Default for WindowsEventEnricher<SystemWindowMetadata> {
    fn default() -> Self {
        Self {
            metadata: SystemWindowMetadata,
        }
    }
}
impl<M> WindowsEventEnricher<M> {
    pub fn with_metadata(metadata: M) -> Self {
        Self { metadata }
    }
}
impl<M: WindowMetadata> EventEnricher for WindowsEventEnricher<M> {
    fn enrich(&mut self, event: &HookEvent) -> Option<EventContext> {
        let foreground = self
            .metadata
            .foreground_root()
            .and_then(|h| self.metadata.context(h));
        let under = match *event {
            HookEvent::Mouse { x, y, .. } => self
                .metadata
                .root_under_point(MkPoint { x, y })
                .and_then(|h| self.metadata.context(h)),
            HookEvent::Key { .. } => None,
        };
        foreground
            .map(|foreground| EventContext {
                foreground,
                window_under_point: under.clone(),
            })
            .or_else(|| {
                under.clone().map(|foreground| EventContext {
                    foreground,
                    window_under_point: under,
                })
            })
    }
}

pub struct SystemWindowMetadata;
#[cfg(windows)]
impl WindowMetadata for SystemWindowMetadata {
    fn foreground_root(&self) -> Option<usize> {
        use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
        let h = unsafe { GetForegroundWindow() };
        (!h.0.is_null()).then_some(h.0 as usize)
    }
    fn root_under_point(&self, point: MkPoint) -> Option<usize> {
        use windows::Win32::{
            Foundation::POINT,
            UI::WindowsAndMessaging::{GA_ROOT, GetAncestor, WindowFromPoint},
        };
        let child = unsafe {
            WindowFromPoint(POINT {
                x: point.x,
                y: point.y,
            })
        };
        if child.0.is_null() {
            return None;
        }
        let root = unsafe { GetAncestor(child, GA_ROOT) };
        (!root.0.is_null()).then_some(root.0 as usize)
    }
    fn context(&self, root: usize) -> Option<WindowContext> {
        use windows::Win32::{
            Foundation::{HWND, POINT},
            UI::WindowsAndMessaging::ClientToScreen,
        };
        let rect =
            crate::multi_manager::win::window_rect(root).map(|r| (r.x, r.y, r.x + r.w, r.y + r.h));
        let mut origin = POINT::default();
        let client_origin = unsafe { ClientToScreen(HWND(root as *mut _), &mut origin) }
            .is_ok()
            .then_some(MkPoint {
                x: origin.x,
                y: origin.y,
            });
        let process_path = crate::multi_manager::win::window_process_path(root).unwrap_or_default();
        let executable = crate::multi_manager::win::window_executable(root).unwrap_or_default();
        let title = crate::multi_manager::win::window_title(root).unwrap_or_default();
        let class = crate::multi_manager::win::window_class_name(root).unwrap_or_default();
        if process_path.is_empty()
            && executable.is_empty()
            && title.is_empty()
            && class.is_empty()
            && rect.is_none()
        {
            return None;
        }
        Some(WindowContext {
            executable,
            process_path,
            title,
            class,
            rect,
            client_origin,
            native_root_id: Some(root),
        })
    }
}
#[cfg(not(windows))]
impl WindowMetadata for SystemWindowMetadata {
    fn foreground_root(&self) -> Option<usize> {
        None
    }
    fn root_under_point(&self, _: MkPoint) -> Option<usize> {
        None
    }
    fn context(&self, _: usize) -> Option<WindowContext> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fake;
    impl WindowMetadata for Fake {
        fn foreground_root(&self) -> Option<usize> {
            Some(1)
        }
        fn root_under_point(&self, _: MkPoint) -> Option<usize> {
            Some(2)
        }
        fn context(&self, root: usize) -> Option<WindowContext> {
            Some(WindowContext {
                executable: format!("{root}.exe"),
                title: format!("window {root}"),
                native_root_id: Some(root),
                ..Default::default()
            })
        }
    }
    #[test]
    fn mouse_gets_foreground_and_root_under_point_but_keyboard_only_foreground() {
        let mut e = WindowsEventEnricher::with_metadata(Fake);
        let mouse = HookEvent::Mouse {
            timestamp_us: 1,
            message: super::super::MouseMessage::Move,
            x: 4,
            y: 5,
            flags: 0,
            extra_info: 0,
        };
        let c = e.enrich(&mouse).unwrap();
        assert_eq!(c.foreground.native_root_id, Some(1));
        assert_eq!(c.window_under_point.unwrap().native_root_id, Some(2));
        let key = HookEvent::Key {
            timestamp_us: 2,
            transition: super::super::KeyTransition::Down,
            vk: 65,
            scan_code: 0,
            flags: 0,
            extra_info: 0,
        };
        assert!(e.enrich(&key).unwrap().window_under_point.is_none());
    }
}
