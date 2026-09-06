use crate::actions::Action;
use crate::plugin::{Plugin, PluginSearchUpdates};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const REFRESH_TTL: Duration = Duration::from_secs(2);
static PRODUCTION_ENUMERATION_ACTIVE: AtomicBool = AtomicBool::new(false);

struct ProductionEnumerationGuard(&'static AtomicBool);

impl Drop for ProductionEnumerationGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

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
        if PRODUCTION_ENUMERATION_ACTIVE
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Vec::new();
        }
        let _guard = ProductionEnumerationGuard(&PRODUCTION_ENUMERATION_ACTIVE);
        enumerate_windows()
    }
}

struct CacheState {
    windows: Arc<Vec<WindowInfo>>,
    fresh_until: Option<Instant>,
    in_flight: bool,
    in_flight_ticket: Option<u64>,
    shutting_down: bool,
    terminal: bool,
}

struct WindowCache {
    state: Arc<Mutex<CacheState>>,
    wake: SyncSender<()>,
    worker: Mutex<Option<JoinHandle<()>>>,
    publication: Arc<Mutex<()>>,
    updates: Arc<PluginSearchUpdates>,
}

impl WindowCache {
    fn start(provider: impl WindowProvider, updates: Arc<PluginSearchUpdates>) -> Self {
        let state = Arc::new(Mutex::new(CacheState {
            windows: Arc::new(Vec::new()),
            fresh_until: None,
            in_flight: false,
            in_flight_ticket: None,
            shutting_down: false,
            terminal: false,
        }));
        let (wake, receiver) = sync_channel(1);
        let publication = Arc::new(Mutex::new(()));
        let worker_state = Arc::clone(&state);
        let worker = thread::Builder::new()
            .name("window-enumeration-refresh".into())
            .spawn({
                let publication = Arc::clone(&publication);
                let worker_updates = Arc::clone(&updates);
                move || {
                    let cleanup_state = Arc::clone(&worker_state);
                    let cleanup_publication = Arc::clone(&publication);
                    let cleanup_updates = Arc::clone(&worker_updates);
                    let _ = catch_unwind(AssertUnwindSafe(|| {
                        run_worker(
                            receiver,
                            worker_state,
                            publication,
                            provider,
                            worker_updates,
                        )
                    }));
                    terminate_worker(&cleanup_state, &cleanup_publication, &cleanup_updates);
                }
            })
            .expect("start window enumeration worker");
        Self {
            state,
            wake,
            worker: Mutex::new(Some(worker)),
            publication,
            updates,
        }
    }

    fn snapshot_and_refresh(&self) -> Arc<Vec<WindowInfo>> {
        let Ok(mut state) = self.state.lock() else {
            return Arc::new(Vec::new());
        };
        if !state.terminal
            && !state
                .fresh_until
                .is_some_and(|deadline| Instant::now() < deadline)
        {
            let ticket = self.updates.schedule_or_join("windows");
            crate::plugin::record_search_refresh_ticket("windows", ticket.id);
            if ticket.start {
                state.in_flight = true;
                state.in_flight_ticket = Some(ticket.id);
            }
            if ticket.start && self.wake.try_send(()).is_err() {
                state.in_flight = false;
                state.in_flight_ticket = None;
                state.terminal = true;
                self.updates.cancel_ticket("windows", ticket.id);
            }
        }
        Arc::clone(&state.windows)
    }
}

impl Drop for WindowCache {
    fn drop(&mut self) {
        {
            let _publication = self.publication.lock().ok();
            if let Ok(mut state) = self.state.lock() {
                state.shutting_down = true;
                if let Some(ticket) = state.in_flight_ticket.take() {
                    self.updates.cancel_ticket("windows", ticket);
                }
            }
        }
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
    publication: Arc<Mutex<()>>,
    mut provider: impl WindowProvider,
    updates: Arc<PluginSearchUpdates>,
) {
    while receiver.recv().is_ok() {
        if state
            .lock()
            .map(|state| state.shutting_down)
            .unwrap_or(true)
        {
            break;
        }
        let result = catch_unwind(AssertUnwindSafe(|| provider.enumerate()));
        let panicked = result.is_err();
        let Ok(_publication) = publication.lock() else {
            break;
        };
        let ticket = if let Ok(mut state) = state.lock() {
            if state.shutting_down {
                break;
            }
            state.in_flight = false;
            if let Ok(windows) = result {
                state.windows = Arc::new(windows);
                state.fresh_until = Some(Instant::now() + REFRESH_TTL);
            } else {
                state.terminal = true;
                tracing::error!("window enumeration provider panicked");
            }
            state.in_flight_ticket.take()
        } else {
            None
        };
        if let Some(ticket) = ticket {
            if panicked {
                updates.cancel_ticket("windows", ticket);
            } else {
                updates.publish_ticket("windows", ticket);
            }
        }
        if panicked {
            break;
        }
    }
}

fn terminate_worker(
    state: &Mutex<CacheState>,
    publication: &Mutex<()>,
    updates: &PluginSearchUpdates,
) {
    let _publication = publication
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let ticket = {
        let mut state = state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.in_flight = false;
        state.terminal = true;
        state.in_flight_ticket.take()
    };
    if let Some(ticket) = ticket {
        updates.cancel_ticket("windows", ticket);
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

    struct DropControlledProvider {
        started: Sender<()>,
        release: Receiver<Vec<WindowInfo>>,
        finished: Sender<()>,
    }

    impl WindowProvider for DropControlledProvider {
        fn enumerate(&mut self) -> Vec<WindowInfo> {
            self.started.send(()).unwrap();
            let windows = self.release.recv().unwrap();
            self.finished.send(()).unwrap();
            windows
        }
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

    #[test]
    fn drop_while_blocked_resolves_ticket_and_suppresses_stale_publication() {
        let (started_tx, started_rx) = channel();
        let (release_tx, release_rx) = channel();
        let (finished_tx, finished_rx) = channel();
        let (repaint_tx, repaint_rx) = channel();
        let updates = Arc::new(PluginSearchUpdates::default());
        updates.set_repaint_callback(Arc::new(move || repaint_tx.send(()).unwrap()));
        let plugin = WindowsPlugin {
            cache: WindowCache::start(
                DropControlledProvider {
                    started: started_tx,
                    release: release_rx,
                    finished: finished_tx,
                },
                Arc::clone(&updates),
            ),
        };
        assert!(plugin.search("win").is_empty());
        started_rx.recv().unwrap();
        drop(plugin);
        repaint_rx.recv().unwrap();
        release_tx
            .send(vec![WindowInfo {
                title: "stale".into(),
                hwnd: 9,
            }])
            .unwrap();
        finished_rx.recv().unwrap();
        while Arc::strong_count(&updates) > 1 {
            std::thread::yield_now();
        }
        assert_eq!(updates.generation(), 1);
        assert!(matches!(repaint_rx.try_recv(), Err(TryRecvError::Empty)));
    }

    struct PanicProvider;

    impl WindowProvider for PanicProvider {
        fn enumerate(&mut self) -> Vec<WindowInfo> {
            panic!("controlled window provider panic")
        }
    }

    #[test]
    fn provider_panic_resolves_ticket_and_disables_rearming() {
        let (repaint_tx, repaint_rx) = channel();
        let updates = Arc::new(PluginSearchUpdates::default());
        updates.set_repaint_callback(Arc::new(move || repaint_tx.send(()).unwrap()));
        let cache = WindowCache::start(PanicProvider, Arc::clone(&updates));

        cache.snapshot_and_refresh();
        repaint_rx.recv().unwrap();
        assert!(cache.state.lock().unwrap().terminal);
        assert!(!cache.state.lock().unwrap().in_flight);
        assert_eq!(updates.active_ticket("windows"), None);
        let generation = updates.generation();
        for _ in 0..4 {
            cache.snapshot_and_refresh();
        }
        assert_eq!(updates.generation(), generation);
        assert!(matches!(repaint_rx.try_recv(), Err(TryRecvError::Empty)));
    }

    #[test]
    fn disconnected_request_channel_cancels_once_and_stays_terminal() {
        let (wake, receiver) = sync_channel(1);
        drop(receiver);
        let (repaint_tx, repaint_rx) = channel();
        let updates = Arc::new(PluginSearchUpdates::default());
        updates.set_repaint_callback(Arc::new(move || repaint_tx.send(()).unwrap()));
        let cache = WindowCache {
            state: Arc::new(Mutex::new(CacheState {
                windows: Arc::new(Vec::new()),
                fresh_until: None,
                in_flight: false,
                in_flight_ticket: None,
                shutting_down: false,
                terminal: false,
            })),
            wake,
            worker: Mutex::new(None),
            publication: Arc::new(Mutex::new(())),
            updates: Arc::clone(&updates),
        };

        cache.snapshot_and_refresh();
        repaint_rx.recv().unwrap();
        let generation = updates.generation();
        for _ in 0..4 {
            cache.snapshot_and_refresh();
        }
        assert!(cache.state.lock().unwrap().terminal);
        assert_eq!(updates.generation(), generation);
        assert!(matches!(repaint_rx.try_recv(), Err(TryRecvError::Empty)));
    }
}
