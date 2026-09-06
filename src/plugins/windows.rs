use crate::actions::Action;
use crate::plugin::{Plugin, PluginSearchUpdates};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const REFRESH_TTL: Duration = Duration::from_secs(2);

#[derive(Clone, Debug)]
struct WindowInfo {
    title: String,
    hwnd: usize,
}

trait WindowProvider: Send + 'static {
    fn enumerate(&mut self) -> Vec<WindowInfo>;
}

struct ProductionWindowProvider;

impl WindowProvider for ProductionWindowProvider {
    fn enumerate(&mut self) -> Vec<WindowInfo> {
        enumerate_windows()
    }
}

struct CacheState {
    windows: Arc<Vec<WindowInfo>>,
    fresh_until: Option<Instant>,
    in_flight: bool,
}

struct WindowCache {
    state: Arc<Mutex<CacheState>>,
    wake: SyncSender<()>,
    shutting_down: Arc<AtomicBool>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl WindowCache {
    fn start(provider: impl WindowProvider, updates: Arc<PluginSearchUpdates>) -> Self {
        let state = Arc::new(Mutex::new(CacheState {
            windows: Arc::new(Vec::new()),
            fresh_until: None,
            in_flight: false,
        }));
        let shutting_down = Arc::new(AtomicBool::new(false));
        let (wake, receiver) = sync_channel(1);
        let worker_state = Arc::clone(&state);
        let worker_shutdown = Arc::clone(&shutting_down);
        let worker = thread::Builder::new()
            .name("window-enumeration-refresh".into())
            .spawn(move || run_worker(receiver, worker_state, worker_shutdown, provider, updates))
            .expect("start window enumeration worker");
        Self {
            state,
            wake,
            shutting_down,
            worker: Mutex::new(Some(worker)),
        }
    }

    fn snapshot_and_refresh(&self) -> Arc<Vec<WindowInfo>> {
        let Ok(mut state) = self.state.lock() else {
            return Arc::new(Vec::new());
        };
        if !state.in_flight
            && !state
                .fresh_until
                .is_some_and(|deadline| Instant::now() < deadline)
        {
            state.in_flight = true;
            if self.wake.try_send(()).is_err() {
                state.in_flight = false;
            }
        }
        Arc::clone(&state.windows)
    }
}

impl Drop for WindowCache {
    fn drop(&mut self) {
        self.shutting_down.store(true, Ordering::Release);
        let _ = self.wake.try_send(());
        if let Ok(worker) = self.worker.get_mut()
            && let Some(worker) = worker.take()
        {
            if worker.is_finished() {
                let _ = worker.join();
            } else {
                // EnumWindows is synchronous but bounded to one OS enumeration. Reap it away from
                // egui if a widget/plugin is removed while enumeration is active.
                let _ = thread::Builder::new()
                    .name("window-enumeration-reaper".into())
                    .spawn(move || {
                        let _ = worker.join();
                    });
            }
        }
    }
}

fn run_worker(
    receiver: Receiver<()>,
    state: Arc<Mutex<CacheState>>,
    shutting_down: Arc<AtomicBool>,
    mut provider: impl WindowProvider,
    updates: Arc<PluginSearchUpdates>,
) {
    while receiver.recv().is_ok() {
        if shutting_down.load(Ordering::Acquire) {
            break;
        }
        let windows = provider.enumerate();
        if let Ok(mut state) = state.lock() {
            state.windows = Arc::new(windows);
            state.fresh_until = Some(Instant::now() + REFRESH_TTL);
            state.in_flight = false;
        }
        updates.notify();
    }
}

fn actions_from_windows(windows: &[WindowInfo], filter: &str) -> Vec<Action> {
    windows
        .iter()
        .filter(|window| filter.is_empty() || window.title.to_lowercase().contains(filter))
        .flat_map(|window| {
            [
                Action {
                    label: format!("Switch to {}", window.title),
                    desc: "Windows".into(),
                    action: format!("window:switch:{}", window.hwnd),
                    args: None,
                },
                Action {
                    label: format!("Close {}", window.title),
                    desc: "Windows".into(),
                    action: format!("window:close:{}", window.hwnd),
                    args: None,
                },
            ]
        })
        .collect()
}

fn enumerate_windows() -> Vec<WindowInfo> {
    use windows::Win32::Foundation::{BOOL, HWND, LPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GW_OWNER, GetWindow, GetWindowTextLengthW, GetWindowTextW, IsWindowVisible,
    };
    unsafe extern "system" fn enum_cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let out = unsafe { &mut *(lparam.0 as *mut Vec<WindowInfo>) };
        if unsafe { IsWindowVisible(hwnd) }.as_bool()
            && unsafe { GetWindow(hwnd, GW_OWNER) }
                .unwrap_or_default()
                .0
                .is_null()
        {
            let len = unsafe { GetWindowTextLengthW(hwnd) };
            if len > 0 {
                let mut buf = vec![0u16; len as usize + 1];
                let read = unsafe { GetWindowTextW(hwnd, &mut buf) };
                let title = String::from_utf16_lossy(&buf[..read as usize]);
                out.push(WindowInfo {
                    title,
                    hwnd: hwnd.0 as usize,
                });
            }
        }
        BOOL(1)
    }
    let mut out = Vec::new();
    unsafe {
        let out_ptr = &mut out as *mut Vec<WindowInfo>;
        let _ = EnumWindows(Some(enum_cb), LPARAM(out_ptr as isize));
    }
    out
}

pub struct WindowsPlugin {
    cache: WindowCache,
}

impl WindowsPlugin {
    pub(crate) fn with_updates(updates: Arc<PluginSearchUpdates>) -> Self {
        Self {
            cache: WindowCache::start(ProductionWindowProvider, updates),
        }
    }
}

impl Default for WindowsPlugin {
    fn default() -> Self {
        Self::with_updates(Arc::new(PluginSearchUpdates::default()))
    }
}

impl Plugin for WindowsPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let trimmed = query.trim();
        let Some(rest) = crate::common::strip_prefix_ci(trimmed, "win") else {
            return Vec::new();
        };
        let windows = self.cache.snapshot_and_refresh();
        actions_from_windows(&windows, &rest.trim().to_lowercase())
    }

    fn name(&self) -> &str {
        "windows"
    }

    fn description(&self) -> &str {
        "Switch or close windows (prefix: `win`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![Action {
            label: "win".into(),
            desc: "Windows".into(),
            action: "query:win ".into(),
            args: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::{Sender, TryRecvError, channel};

    struct ControlledProvider {
        started: Sender<()>,
        release: Receiver<Vec<WindowInfo>>,
    }

    impl WindowProvider for ControlledProvider {
        fn enumerate(&mut self) -> Vec<WindowInfo> {
            self.started.send(()).unwrap();
            self.release.recv().unwrap()
        }
    }

    #[test]
    fn blocked_enumeration_is_nonblocking_single_flight_and_notifies() {
        let (started_tx, started_rx) = channel();
        let (release_tx, release_rx) = channel();
        let (repaint_tx, repaint_rx) = channel();
        let updates = Arc::new(PluginSearchUpdates::default());
        updates.set_repaint_callback(Arc::new(move || repaint_tx.send(()).unwrap()));
        let plugin = WindowsPlugin {
            cache: WindowCache::start(
                ControlledProvider {
                    started: started_tx,
                    release: release_rx,
                },
                Arc::clone(&updates),
            ),
        };
        assert!(plugin.search("win").is_empty());
        started_rx.recv().unwrap();
        assert!(plugin.search("win editor").is_empty());
        assert!(matches!(started_rx.try_recv(), Err(TryRecvError::Empty)));
        release_tx
            .send(vec![WindowInfo {
                title: "Editor".into(),
                hwnd: 42,
            }])
            .unwrap();
        repaint_rx.recv().unwrap();
        assert_eq!(updates.generation(), 1);
        assert_eq!(plugin.search("win editor").len(), 2);
    }
}
