//! Immediate, best-effort window metadata enrichment for recorder events.
use super::{EventContext, EventEnricher, HookEvent, MkPoint, WindowContext};
use std::collections::HashMap;

/// Injectable native-window seam. Implementations must treat every metadata failure as absent data.
pub trait WindowMetadata: Send {
    fn foreground_root(&self) -> Option<usize>;
    fn root_under_point(&self, point: MkPoint) -> Option<usize>;
    fn context(&self, root: usize) -> Option<WindowContext>;
    fn refresh_context(&self, root: usize, _cached: &WindowContext) -> Option<WindowContext> {
        self.context(root)
    }
    fn process_thread(&self, _root: usize) -> Option<(u32, u32)> {
        None
    }
    fn keyboard_layout(&self, _root: usize) -> Option<isize> {
        None
    }
    fn is_recordable_top_level(&self, _root: usize) -> bool {
        true
    }
}

pub struct WindowsEventEnricher<M = SystemWindowMetadata> {
    metadata: M,
    contexts: HashMap<(usize, u32), WindowContext>,
}
impl Default for WindowsEventEnricher<SystemWindowMetadata> {
    fn default() -> Self {
        Self {
            metadata: SystemWindowMetadata,
            contexts: HashMap::new(),
        }
    }
}
impl<M> WindowsEventEnricher<M> {
    pub fn with_metadata(metadata: M) -> Self {
        Self {
            metadata,
            contexts: HashMap::new(),
        }
    }
}
impl<M: WindowMetadata> EventEnricher for WindowsEventEnricher<M> {
    fn enrich(&mut self, event: &HookEvent) -> Option<EventContext> {
        let foreground_root = self.metadata.foreground_root();
        // Layout capture is event-correlated and best effort: hook transport
        // timestamps the packet first, then this worker queries the owning
        // foreground thread when it consumes that packet.
        let keyboard_layout = matches!(event, HookEvent::Key { .. })
            .then(|| foreground_root.and_then(|h| self.metadata.keyboard_layout(h)))
            .flatten();
        let refresh_window = matches!(event, HookEvent::Mouse { message, .. }
            if !matches!(message, super::MouseMessage::Move));
        let foreground = foreground_root.and_then(|h| self.context(h, refresh_window));
        let under = match *event {
            HookEvent::Mouse { x, y, .. } => self
                .metadata
                .root_under_point(MkPoint { x, y })
                .and_then(|h| self.context(h, refresh_window)),
            HookEvent::Key { .. } => None,
        };
        foreground
            .map(|foreground| EventContext {
                foreground,
                window_under_point: under.clone(),
                keyboard_layout,
            })
            .or_else(|| {
                under.clone().map(|foreground| EventContext {
                    foreground,
                    window_under_point: under,
                    keyboard_layout,
                })
            })
    }
    fn enrich_keyboard_layout(&mut self) -> Option<EventContext> {
        let root = self.metadata.foreground_root()?;
        Some(EventContext {
            foreground: WindowContext {
                native_root_id: Some(root),
                ..Default::default()
            },
            window_under_point: None,
            keyboard_layout: self.metadata.keyboard_layout(root),
        })
    }
    fn context_for_root(&mut self, root: usize) -> Option<WindowContext> {
        self.context(root, true)
    }
    fn is_recordable_top_level(&self, root: usize) -> bool {
        self.metadata.is_recordable_top_level(root)
    }
    fn invalidate_root(&mut self, root: usize) {
        self.contexts
            .retain(|(cached_root, _), _| *cached_root != root);
    }
}

impl<M: WindowMetadata> WindowsEventEnricher<M> {
    fn context(&mut self, root: usize, refresh: bool) -> Option<WindowContext> {
        let process = self.metadata.process_thread(root).map_or(0, |(pid, _)| pid);
        let key = (root, process);
        if let Some(context) = self.contexts.get(&key) {
            if !refresh {
                return Some(context.clone());
            }
            let context = self.metadata.refresh_context(root, context)?;
            self.contexts.insert(key, context.clone());
            return Some(context);
        }
        let context = self.metadata.context(root)?;
        if self.contexts.len() >= 128 {
            self.contexts.clear();
        }
        self.contexts.insert(key, context.clone());
        Some(context)
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
            Graphics::Gdi::ClientToScreen,
            UI::WindowsAndMessaging::GetWindowThreadProcessId,
        };
        let mut process_id = 0;
        let thread_id =
            unsafe { GetWindowThreadProcessId(HWND(root as *mut _), Some(&mut process_id)) };
        let rect =
            crate::multi_manager::win::window_rect(root).map(|r| (r.x, r.y, r.x + r.w, r.y + r.h));
        let mut origin = POINT::default();
        let client_origin = unsafe { ClientToScreen(HWND(root as *mut _), &mut origin) }
            .as_bool()
            .then_some(MkPoint {
                x: origin.x,
                y: origin.y,
            });
        let process_path = crate::multi_manager::win::window_process_path(root).unwrap_or_default();
        let executable = std::path::Path::new(&process_path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_owned();
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
            process_id: (process_id != 0).then_some(process_id),
            thread_id: (thread_id != 0).then_some(thread_id),
            process_started_at: None,
        })
    }
    fn refresh_context(&self, root: usize, cached: &WindowContext) -> Option<WindowContext> {
        use windows::Win32::{
            Foundation::{HWND, POINT},
            Graphics::Gdi::ClientToScreen,
            UI::WindowsAndMessaging::GetWindowThreadProcessId,
        };
        let hwnd = HWND(root as *mut _);
        let mut process_id = 0;
        let thread_id = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
        let rect =
            crate::multi_manager::win::window_rect(root).map(|r| (r.x, r.y, r.x + r.w, r.y + r.h));
        let mut origin = POINT::default();
        let client_origin = unsafe { ClientToScreen(hwnd, &mut origin) }
            .as_bool()
            .then_some(MkPoint {
                x: origin.x,
                y: origin.y,
            });
        let title = crate::multi_manager::win::window_title(root).unwrap_or_default();
        let class = crate::multi_manager::win::window_class_name(root).unwrap_or_default();
        if cached.process_path.is_empty()
            && cached.executable.is_empty()
            && title.is_empty()
            && class.is_empty()
            && rect.is_none()
        {
            return None;
        }
        Some(WindowContext {
            executable: cached.executable.clone(),
            process_path: cached.process_path.clone(),
            title,
            class,
            rect,
            client_origin,
            native_root_id: Some(root),
            process_id: (process_id != 0).then_some(process_id),
            thread_id: (thread_id != 0).then_some(thread_id),
            process_started_at: cached.process_started_at,
        })
    }
    fn keyboard_layout(&self, root: usize) -> Option<isize> {
        use windows::Win32::{
            Foundation::HWND,
            UI::{
                Input::KeyboardAndMouse::GetKeyboardLayout,
                WindowsAndMessaging::GetWindowThreadProcessId,
            },
        };
        let thread = unsafe { GetWindowThreadProcessId(HWND(root as *mut _), None) };
        (thread != 0).then(|| unsafe { GetKeyboardLayout(thread).0 as isize })
    }
    fn process_thread(&self, root: usize) -> Option<(u32, u32)> {
        use windows::Win32::{Foundation::HWND, UI::WindowsAndMessaging::GetWindowThreadProcessId};
        let mut pid = 0;
        let thread = unsafe { GetWindowThreadProcessId(HWND(root as *mut _), Some(&mut pid)) };
        (thread != 0 && pid != 0).then_some((pid, thread))
    }
    fn is_recordable_top_level(&self, root: usize) -> bool {
        use windows::Win32::{
            Foundation::HWND,
            UI::WindowsAndMessaging::{
                GA_ROOT, GWL_EXSTYLE, GetAncestor, GetWindowLongW, IsWindowVisible,
                WS_EX_TOOLWINDOW,
            },
        };
        let hwnd = HWND(root as *mut _);
        if hwnd.0.is_null() || !unsafe { IsWindowVisible(hwnd) }.as_bool() {
            return false;
        }
        if unsafe { GetAncestor(hwnd, GA_ROOT) } != hwnd {
            return false;
        }
        let ex_style = unsafe { GetWindowLongW(hwnd, GWL_EXSTYLE) } as u32;
        ex_style & WS_EX_TOOLWINDOW.0 == 0
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
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
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

    struct Counting(Arc<AtomicUsize>);
    impl WindowMetadata for Counting {
        fn foreground_root(&self) -> Option<usize> {
            Some(1)
        }
        fn root_under_point(&self, _: MkPoint) -> Option<usize> {
            Some(2)
        }
        fn context(&self, root: usize) -> Option<WindowContext> {
            self.0.fetch_add(1, Ordering::Relaxed);
            Some(WindowContext {
                title: format!("window {root}"),
                ..Default::default()
            })
        }
    }
    #[test]
    fn consecutive_mouse_moves_reuse_resolved_window_metadata() {
        let calls = Arc::new(AtomicUsize::new(0));
        let mut enricher = WindowsEventEnricher::with_metadata(Counting(calls.clone()));
        for timestamp_us in [1, 2] {
            enricher.enrich(&HookEvent::Mouse {
                timestamp_us,
                message: super::super::MouseMessage::Move,
                x: 4,
                y: 5,
                flags: 0,
                extra_info: 0,
            });
        }
        assert_eq!(calls.load(Ordering::Relaxed), 2);
    }
}
