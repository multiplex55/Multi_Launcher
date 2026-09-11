use crate::plugin::{PluginSearchUpdates, record_search_refresh_ticket};
use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const REFRESH_TTL: Duration = Duration::from_secs(2);
const UPDATE_SOURCE: &str = "windows";
static PRODUCTION_ENUMERATION_ACTIVE: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowDescriptor {
    pub title: String,
    pub hwnd: usize,
    pub pid: u32,
    pub executable: Option<String>,
    pub process_path: Option<String>,
    pub class_name: Option<String>,
}

#[derive(Clone)]
pub struct WindowCatalogSnapshot {
    pub windows: Arc<Vec<WindowDescriptor>>,
    pub desktop_ids: Arc<HashMap<usize, Option<crate::virtual_desktop::VirtualDesktopId>>>,
    pub desktop_ids_ready: bool,
    pub generation: u64,
}

pub(crate) trait WindowProvider: Send + 'static {
    fn enumerate(&mut self) -> Vec<WindowDescriptor>;
    fn desktop_memberships(
        &mut self,
        _windows: &[WindowDescriptor],
    ) -> HashMap<usize, Option<crate::virtual_desktop::VirtualDesktopId>> {
        HashMap::new()
    }
}

struct ProductionWindowProvider;

impl WindowProvider for ProductionWindowProvider {
    fn enumerate(&mut self) -> Vec<WindowDescriptor> {
        if PRODUCTION_ENUMERATION_ACTIVE
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Vec::new();
        }
        struct Guard;
        impl Drop for Guard {
            fn drop(&mut self) {
                PRODUCTION_ENUMERATION_ACTIVE.store(false, Ordering::Release);
            }
        }
        let _guard = Guard;
        enumerate_windows()
    }

    fn desktop_memberships(
        &mut self,
        windows: &[WindowDescriptor],
    ) -> HashMap<usize, Option<crate::virtual_desktop::VirtualDesktopId>> {
        crate::virtual_desktop::VirtualDesktopService
            .desktops_for_windows(&windows.iter().map(|window| window.hwnd).collect::<Vec<_>>())
            .map(|memberships| memberships.into_iter().collect())
            .unwrap_or_default()
    }
}

struct CatalogState {
    windows: Arc<Vec<WindowDescriptor>>,
    fresh_until: Option<Instant>,
    in_flight: bool,
    in_flight_ticket: Option<u64>,
    shutting_down: bool,
    terminal: bool,
    generation: u64,
    desktop_ids: Arc<HashMap<usize, Option<crate::virtual_desktop::VirtualDesktopId>>>,
    desktop_ids_ready: bool,
    enrichment_requested: bool,
    refresh_after_flight: bool,
}

pub struct WindowCatalog {
    state: Arc<Mutex<CatalogState>>,
    wake: SyncSender<()>,
    worker: Mutex<Option<JoinHandle<()>>>,
    publication: Arc<Mutex<()>>,
    changed: Arc<Condvar>,
    updates: Arc<PluginSearchUpdates>,
}

impl WindowCatalog {
    pub(crate) fn production(updates: Arc<PluginSearchUpdates>) -> Arc<Self> {
        Arc::new(Self::start(ProductionWindowProvider, updates))
    }

    pub(crate) fn start(provider: impl WindowProvider, updates: Arc<PluginSearchUpdates>) -> Self {
        let state = Arc::new(Mutex::new(CatalogState {
            windows: Arc::new(Vec::new()),
            fresh_until: None,
            in_flight: false,
            in_flight_ticket: None,
            shutting_down: false,
            terminal: false,
            generation: 0,
            desktop_ids: Arc::new(HashMap::new()),
            desktop_ids_ready: false,
            enrichment_requested: false,
            refresh_after_flight: false,
        }));
        let (wake, receiver) = sync_channel(1);
        let publication = Arc::new(Mutex::new(()));
        let worker_state = Arc::clone(&state);
        let worker_publication = Arc::clone(&publication);
        let worker_updates = Arc::clone(&updates);
        let changed = Arc::new(Condvar::new());
        let worker_changed = Arc::clone(&changed);
        let worker = thread::Builder::new()
            .name("window-catalog-refresh".into())
            .spawn(move || {
                let cleanup_state = Arc::clone(&worker_state);
                let cleanup_publication = Arc::clone(&worker_publication);
                let cleanup_updates = Arc::clone(&worker_updates);
                let cleanup_changed = Arc::clone(&worker_changed);
                let _ = catch_unwind(AssertUnwindSafe(|| {
                    run_worker(
                        receiver,
                        worker_state,
                        worker_publication,
                        provider,
                        worker_updates,
                        worker_changed,
                    )
                }));
                terminate_worker(
                    &cleanup_state,
                    &cleanup_publication,
                    &cleanup_updates,
                    &cleanup_changed,
                );
            })
            .expect("start window catalog worker");
        Self {
            state,
            wake,
            worker: Mutex::new(Some(worker)),
            publication,
            changed,
            updates,
        }
    }

    #[cfg(test)]
    pub(crate) fn from_snapshot(windows: Vec<WindowDescriptor>) -> Arc<Self> {
        Self::from_test_snapshot(windows, HashMap::new(), false)
    }

    #[cfg(test)]
    pub(crate) fn from_enriched_snapshot(
        windows: Vec<WindowDescriptor>,
        desktop_ids: HashMap<usize, Option<crate::virtual_desktop::VirtualDesktopId>>,
    ) -> Arc<Self> {
        Self::from_test_snapshot(windows, desktop_ids, true)
    }

    #[cfg(test)]
    fn from_test_snapshot(
        windows: Vec<WindowDescriptor>,
        desktop_ids: HashMap<usize, Option<crate::virtual_desktop::VirtualDesktopId>>,
        desktop_ids_ready: bool,
    ) -> Arc<Self> {
        let (wake, receiver) = sync_channel(1);
        drop(receiver);
        Arc::new(Self {
            state: Arc::new(Mutex::new(CatalogState {
                windows: Arc::new(windows),
                fresh_until: Some(Instant::now() + Duration::from_secs(60)),
                in_flight: false,
                in_flight_ticket: None,
                shutting_down: false,
                terminal: true,
                generation: 0,
                desktop_ids: Arc::new(desktop_ids),
                desktop_ids_ready,
                enrichment_requested: false,
                refresh_after_flight: false,
            })),
            wake,
            worker: Mutex::new(None),
            publication: Arc::new(Mutex::new(())),
            changed: Arc::new(Condvar::new()),
            updates: Arc::new(PluginSearchUpdates::default()),
        })
    }

    /// Return the last published snapshot and request one refresh if it is stale.
    /// This method never waits for native enumeration.
    pub fn snapshot_and_refresh(&self) -> Arc<Vec<WindowDescriptor>> {
        let Ok(mut state) = self.state.lock() else {
            return Arc::new(Vec::new());
        };
        if !state.terminal
            && !state
                .fresh_until
                .is_some_and(|deadline| Instant::now() < deadline)
        {
            self.schedule(&mut state);
        }
        Arc::clone(&state.windows)
    }

    pub fn snapshot(&self) -> Arc<Vec<WindowDescriptor>> {
        self.state
            .lock()
            .map(|state| Arc::clone(&state.windows))
            .unwrap_or_else(|_| Arc::new(Vec::new()))
    }

    pub fn generation(&self) -> u64 {
        self.state.lock().map(|state| state.generation).unwrap_or(0)
    }

    /// Explicitly request a fresh publication, used by bounded background discovery.
    pub fn request_refresh(&self) {
        if let Ok(mut state) = self.state.lock() {
            if state.in_flight {
                state.refresh_after_flight = true;
                return;
            }
            state.fresh_until = None;
            self.schedule(&mut state);
        }
    }

    pub fn refresh_and_wait(
        &self,
        timeout: Duration,
    ) -> Result<WindowCatalogSnapshot, &'static str> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "window catalog lock poisoned")?;
        if state.terminal {
            return Err("window catalog worker is unavailable");
        }
        let target_generation = if state.in_flight {
            // An enumeration already in progress may have sampled before this request. Queue one
            // more pass so the returned baseline is necessarily post-request.
            state.refresh_after_flight = true;
            state.generation.saturating_add(2)
        } else {
            state.fresh_until = None;
            self.schedule(&mut state);
            state.generation.saturating_add(1)
        };
        let (state, wait) = self
            .changed
            .wait_timeout_while(state, timeout, |state| {
                !state.terminal && state.generation < target_generation
            })
            .map_err(|_| "window catalog lock poisoned")?;
        if state.terminal {
            return Err("window catalog worker is unavailable");
        }
        if wait.timed_out() && state.generation < target_generation {
            return Err("window catalog refresh timed out");
        }
        Ok(snapshot_from_state(&state))
    }

    pub fn snapshot_with_desktops_and_refresh(&self) -> WindowCatalogSnapshot {
        let Ok(mut state) = self.state.lock() else {
            return WindowCatalogSnapshot {
                windows: Arc::new(Vec::new()),
                desktop_ids: Arc::new(HashMap::new()),
                desktop_ids_ready: false,
                generation: 0,
            };
        };
        let stale = !state
            .fresh_until
            .is_some_and(|deadline| Instant::now() < deadline);
        if !state.terminal && (stale || !state.desktop_ids_ready) {
            state.enrichment_requested = true;
            if stale {
                state.fresh_until = None;
            }
            self.schedule(&mut state);
        }
        snapshot_from_state(&state)
    }

    pub fn invalidate_desktop_memberships(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.desktop_ids_ready = false;
            state.desktop_ids = Arc::new(HashMap::new());
        }
    }

    fn schedule(&self, state: &mut CatalogState) {
        let ticket = self.updates.schedule_or_join(UPDATE_SOURCE);
        record_search_refresh_ticket(UPDATE_SOURCE, ticket.id);
        if ticket.start {
            state.in_flight = true;
            state.in_flight_ticket = Some(ticket.id);
            if self.wake.try_send(()).is_err() {
                state.in_flight = false;
                state.in_flight_ticket = None;
                state.terminal = true;
                self.updates.cancel_ticket(UPDATE_SOURCE, ticket.id);
                self.changed.notify_all();
            }
        }
    }
}

fn snapshot_from_state(state: &CatalogState) -> WindowCatalogSnapshot {
    WindowCatalogSnapshot {
        windows: Arc::clone(&state.windows),
        desktop_ids: Arc::clone(&state.desktop_ids),
        desktop_ids_ready: state.desktop_ids_ready,
        generation: state.generation,
    }
}

impl Drop for WindowCatalog {
    fn drop(&mut self) {
        {
            let _publication = self.publication.lock().ok();
            if let Ok(mut state) = self.state.lock() {
                state.shutting_down = true;
                if let Some(ticket) = state.in_flight_ticket.take() {
                    self.updates.cancel_ticket(UPDATE_SOURCE, ticket);
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
                let _ = thread::Builder::new()
                    .name("window-catalog-reaper".into())
                    .spawn(move || {
                        let _ = worker.join();
                    });
            }
        }
    }
}

fn run_worker(
    receiver: Receiver<()>,
    state: Arc<Mutex<CatalogState>>,
    publication: Arc<Mutex<()>>,
    mut provider: impl WindowProvider,
    updates: Arc<PluginSearchUpdates>,
    changed: Arc<Condvar>,
) {
    'worker: while receiver.recv().is_ok() {
        loop {
            if state.lock().map(|s| s.shutting_down).unwrap_or(true) {
                break 'worker;
            }
            let (enumerate, enrich, prior) = match state.lock() {
                Ok(mut state) => {
                    let enumerate = !state
                        .fresh_until
                        .is_some_and(|deadline| Instant::now() < deadline);
                    let enrich = state.enrichment_requested;
                    state.enrichment_requested = false;
                    (enumerate, enrich, Arc::clone(&state.windows))
                }
                Err(_) => break 'worker,
            };
            let result = catch_unwind(AssertUnwindSafe(|| {
                let windows = if enumerate {
                    provider.enumerate()
                } else {
                    (*prior).clone()
                };
                let desktops = enrich.then(|| provider.desktop_memberships(&windows));
                (windows, desktops)
            }));
            let panicked = result.is_err();
            let Ok(publication_guard) = publication.lock() else {
                break 'worker;
            };
            let (published, cancelled, followup) = if let Ok(mut state) = state.lock() {
                if state.shutting_down {
                    break 'worker;
                }
                if let Ok((windows, desktops)) = result {
                    state.windows = Arc::new(windows);
                    if enumerate {
                        state.fresh_until = Some(Instant::now() + REFRESH_TTL);
                    }
                    if let Some(desktops) = desktops {
                        state.desktop_ids = Arc::new(desktops);
                        state.desktop_ids_ready = true;
                    } else if enumerate {
                        state.desktop_ids = Arc::new(HashMap::new());
                        state.desktop_ids_ready = false;
                    }
                    state.generation = state.generation.wrapping_add(1).max(1);
                } else {
                    state.terminal = true;
                    tracing::error!("window catalog provider panicked");
                }
                let followup = !panicked && state.refresh_after_flight;
                state.refresh_after_flight = false;
                if followup {
                    state.fresh_until = None;
                }
                let ticket = state.in_flight_ticket.take();
                if panicked {
                    state.in_flight = false;
                    (false, ticket, false)
                } else if let Some(ticket) = ticket {
                    let (published, next) =
                        updates.publish_ticket_with_followup(UPDATE_SOURCE, ticket, followup);
                    state.in_flight = next.is_some();
                    state.in_flight_ticket = next.map(|ticket| ticket.id);
                    (published, None, next.is_some())
                } else {
                    state.in_flight = false;
                    (false, None, false)
                }
            } else {
                (false, None, false)
            };
            drop(publication_guard);
            if let Some(ticket) = cancelled {
                updates.cancel_ticket(UPDATE_SOURCE, ticket);
            }
            if published {
                updates.notify(UPDATE_SOURCE);
            }
            changed.notify_all();
            if panicked {
                break 'worker;
            }
            if !followup {
                break;
            }
        }
    }
}

fn terminate_worker(
    state: &Mutex<CatalogState>,
    publication: &Mutex<()>,
    updates: &PluginSearchUpdates,
    changed: &Condvar,
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
        updates.cancel_ticket(UPDATE_SOURCE, ticket);
    }
    changed.notify_all();
}

#[cfg(windows)]
fn enumerate_windows() -> Vec<WindowDescriptor> {
    use windows::Win32::Foundation::{BOOL, CloseHandle, HWND, LPARAM};
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION,
        QueryFullProcessImageNameW,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GW_OWNER, GetClassNameW, GetWindow, GetWindowTextLengthW, GetWindowTextW,
        GetWindowThreadProcessId, IsWindowVisible,
    };

    unsafe extern "system" fn enum_cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let out = unsafe { &mut *(lparam.0 as *mut Vec<WindowDescriptor>) };
        if !unsafe { IsWindowVisible(hwnd) }.as_bool()
            || !unsafe { GetWindow(hwnd, GW_OWNER) }
                .unwrap_or_default()
                .0
                .is_null()
        {
            return BOOL(1);
        }
        let len = unsafe { GetWindowTextLengthW(hwnd) };
        if len <= 0 {
            return BOOL(1);
        }
        let mut title_buf = vec![0u16; len as usize + 1];
        let read = unsafe { GetWindowTextW(hwnd, &mut title_buf) };
        let title = String::from_utf16_lossy(&title_buf[..read as usize]);
        let mut pid = 0;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        let mut class_buf = [0u16; 256];
        let class_len = unsafe { GetClassNameW(hwnd, &mut class_buf) } as usize;
        let class_name = (class_len > 0).then(|| String::from_utf16_lossy(&class_buf[..class_len]));
        let process_path = unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok();
            handle.and_then(|handle| {
                let mut path = vec![0u16; 32768];
                let mut size = path.len() as u32;
                let result = QueryFullProcessImageNameW(
                    handle,
                    PROCESS_NAME_FORMAT(0),
                    windows::core::PWSTR(path.as_mut_ptr()),
                    &mut size,
                );
                let _ = CloseHandle(handle);
                result
                    .ok()
                    .map(|_| String::from_utf16_lossy(&path[..size as usize]))
            })
        };
        let executable = process_path.as_deref().and_then(|path| {
            std::path::Path::new(path)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
        });
        out.push(WindowDescriptor {
            title,
            hwnd: hwnd.0 as usize,
            pid,
            executable,
            process_path,
            class_name,
        });
        BOOL(1)
    }
    let mut out = Vec::new();
    unsafe {
        let out_ptr = &mut out as *mut Vec<WindowDescriptor>;
        let _ = EnumWindows(Some(enum_cb), LPARAM(out_ptr as isize));
    }
    out
}

#[cfg(not(windows))]
fn enumerate_windows() -> Vec<WindowDescriptor> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::sync::mpsc::{Sender, TryRecvError, channel};

    struct ControlledProvider {
        started: Sender<()>,
        release: Receiver<Vec<WindowDescriptor>>,
    }

    struct DropControlledProvider {
        started: Sender<()>,
        release: Receiver<Vec<WindowDescriptor>>,
        finished: Sender<()>,
    }

    impl WindowProvider for DropControlledProvider {
        fn enumerate(&mut self) -> Vec<WindowDescriptor> {
            self.started.send(()).unwrap();
            let windows = self.release.recv().unwrap();
            self.finished.send(()).unwrap();
            windows
        }
    }

    impl WindowProvider for ControlledProvider {
        fn enumerate(&mut self) -> Vec<WindowDescriptor> {
            self.started.send(()).unwrap();
            self.release.recv().unwrap()
        }
    }

    fn window(title: &str, hwnd: usize) -> WindowDescriptor {
        WindowDescriptor {
            title: title.into(),
            hwnd,
            pid: 1,
            executable: None,
            process_path: None,
            class_name: None,
        }
    }

    #[test]
    fn shared_consumers_are_nonblocking_and_single_flight() {
        let (started_tx, started_rx) = channel();
        let (release_tx, release_rx) = channel();
        let (repaint_tx, repaint_rx) = channel();
        let updates = Arc::new(PluginSearchUpdates::default());
        updates.set_repaint_callback(Arc::new(move || repaint_tx.send(()).unwrap()));
        let catalog = Arc::new(WindowCatalog::start(
            ControlledProvider {
                started: started_tx,
                release: release_rx,
            },
            Arc::clone(&updates),
        ));
        let consumer_two = Arc::clone(&catalog);
        assert!(catalog.snapshot_and_refresh().is_empty());
        started_rx.recv().unwrap();
        assert!(consumer_two.snapshot_and_refresh().is_empty());
        assert!(matches!(started_rx.try_recv(), Err(TryRecvError::Empty)));
        release_tx.send(vec![window("Editor", 42)]).unwrap();
        repaint_rx.recv().unwrap();
        assert_eq!(catalog.snapshot().as_slice(), [window("Editor", 42)]);
        assert_eq!(catalog.generation(), 1);
    }

    #[test]
    fn drop_while_blocked_resolves_ticket_and_suppresses_stale_publication() {
        let (started_tx, started_rx) = channel();
        let (release_tx, release_rx) = channel();
        let (finished_tx, finished_rx) = channel();
        let (repaint_tx, repaint_rx) = channel();
        let updates = Arc::new(PluginSearchUpdates::default());
        updates.set_repaint_callback(Arc::new(move || repaint_tx.send(()).unwrap()));
        let catalog = WindowCatalog::start(
            DropControlledProvider {
                started: started_tx,
                release: release_rx,
                finished: finished_tx,
            },
            Arc::clone(&updates),
        );
        assert!(catalog.snapshot_and_refresh().is_empty());
        started_rx.recv().unwrap();
        drop(catalog);
        repaint_rx.recv().unwrap();
        release_tx.send(vec![window("stale", 9)]).unwrap();
        finished_rx.recv().unwrap();
        while Arc::strong_count(&updates) > 1 {
            std::thread::yield_now();
        }
        assert_eq!(updates.generation(), 1);
        assert!(matches!(repaint_rx.try_recv(), Err(TryRecvError::Empty)));
    }

    struct PanicProvider;
    impl WindowProvider for PanicProvider {
        fn enumerate(&mut self) -> Vec<WindowDescriptor> {
            panic!("controlled provider panic")
        }
    }

    #[test]
    fn provider_panic_resolves_ticket_and_disables_rearming() {
        let (repaint_tx, repaint_rx) = channel();
        let updates = Arc::new(PluginSearchUpdates::default());
        updates.set_repaint_callback(Arc::new(move || repaint_tx.send(()).unwrap()));
        let catalog = WindowCatalog::start(PanicProvider, Arc::clone(&updates));
        catalog.snapshot_and_refresh();
        repaint_rx.recv().unwrap();
        assert!(catalog.state.lock().unwrap().terminal);
        assert!(!catalog.state.lock().unwrap().in_flight);
        assert_eq!(updates.active_ticket(UPDATE_SOURCE), None);
        let generation = updates.generation();
        for _ in 0..4 {
            catalog.snapshot_and_refresh();
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
        let catalog = WindowCatalog {
            state: Arc::new(Mutex::new(CatalogState {
                windows: Arc::new(Vec::new()),
                fresh_until: None,
                in_flight: false,
                in_flight_ticket: None,
                shutting_down: false,
                terminal: false,
                generation: 0,
                desktop_ids: Arc::new(HashMap::new()),
                desktop_ids_ready: false,
                enrichment_requested: false,
                refresh_after_flight: false,
            })),
            wake,
            worker: Mutex::new(None),
            publication: Arc::new(Mutex::new(())),
            changed: Arc::new(Condvar::new()),
            updates: Arc::clone(&updates),
        };
        catalog.snapshot_and_refresh();
        repaint_rx.recv().unwrap();
        let generation = updates.generation();
        for _ in 0..4 {
            catalog.snapshot_and_refresh();
        }
        assert!(catalog.state.lock().unwrap().terminal);
        assert_eq!(updates.generation(), generation);
        assert!(matches!(repaint_rx.try_recv(), Err(TryRecvError::Empty)));
    }

    #[test]
    fn refresh_and_wait_returns_only_after_a_new_generation_is_published() {
        let (started_tx, started_rx) = channel();
        let (release_tx, release_rx) = channel();
        let catalog = Arc::new(WindowCatalog::start(
            ControlledProvider {
                started: started_tx,
                release: release_rx,
            },
            Arc::new(PluginSearchUpdates::default()),
        ));
        let waiter = {
            let catalog = Arc::clone(&catalog);
            std::thread::spawn(move || catalog.refresh_and_wait(Duration::from_secs(2)).unwrap())
        };
        started_rx.recv().unwrap();
        assert_eq!(catalog.generation(), 0);
        release_tx.send(vec![window("fresh", 77)]).unwrap();
        let snapshot = waiter.join().unwrap();
        assert_eq!(snapshot.generation, 1);
        assert_eq!(snapshot.windows[0].hwnd, 77);
    }

    #[test]
    fn refresh_and_wait_queues_a_post_request_generation_when_work_is_already_in_flight() {
        let (started_tx, started_rx) = channel();
        let (release_tx, release_rx) = channel();
        let catalog = Arc::new(WindowCatalog::start(
            ControlledProvider {
                started: started_tx,
                release: release_rx,
            },
            Arc::new(PluginSearchUpdates::default()),
        ));
        catalog.snapshot_and_refresh();
        started_rx.recv().unwrap();
        let waiter = {
            let catalog = Arc::clone(&catalog);
            std::thread::spawn(move || catalog.refresh_and_wait(Duration::from_secs(2)).unwrap())
        };
        release_tx.send(vec![window("pre-request", 11)]).unwrap();
        started_rx.recv().unwrap();
        assert_eq!(catalog.generation(), 1);
        assert!(!waiter.is_finished());
        release_tx.send(vec![window("post-request", 22)]).unwrap();
        let snapshot = waiter.join().unwrap();
        assert_eq!(snapshot.generation, 2);
        assert_eq!(snapshot.windows[0].hwnd, 22);
    }

    #[test]
    fn repaint_during_ticket_handoff_joins_the_reserved_post_request_generation() {
        let (started_tx, started_rx) = channel();
        let (release_tx, release_rx) = channel();
        let (callback_tx, callback_rx) = channel();
        let catalog_slot = Arc::new(Mutex::new(std::sync::Weak::<WindowCatalog>::new()));
        let callback_slot = Arc::clone(&catalog_slot);
        let callback_count = Arc::new(AtomicUsize::new(0));
        let callback_counter = Arc::clone(&callback_count);
        let updates = Arc::new(PluginSearchUpdates::default());
        updates.set_repaint_callback(Arc::new(move || {
            if callback_counter.fetch_add(1, Ordering::SeqCst) == 0 {
                if let Some(catalog) = callback_slot.lock().unwrap().upgrade() {
                    catalog.snapshot_and_refresh();
                }
                callback_tx.send(()).unwrap();
            }
        }));
        let catalog = Arc::new(WindowCatalog::start(
            ControlledProvider {
                started: started_tx,
                release: release_rx,
            },
            updates,
        ));
        *catalog_slot.lock().unwrap() = Arc::downgrade(&catalog);
        catalog.snapshot_and_refresh();
        started_rx.recv().unwrap();
        let waiter = {
            let catalog = Arc::clone(&catalog);
            std::thread::spawn(move || catalog.refresh_and_wait(Duration::from_secs(2)).unwrap())
        };
        release_tx.send(vec![window("pre-request", 11)]).unwrap();
        callback_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        started_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        release_tx.send(vec![window("post-request", 22)]).unwrap();
        let snapshot = waiter.join().unwrap();
        assert_eq!(snapshot.generation, 2);
        assert_eq!(snapshot.windows[0].hwnd, 22);
        assert_eq!(callback_count.load(Ordering::SeqCst), 2);
    }

    struct EnrichmentProvider {
        enumerations: Arc<AtomicUsize>,
    }
    impl WindowProvider for EnrichmentProvider {
        fn enumerate(&mut self) -> Vec<WindowDescriptor> {
            self.enumerations.fetch_add(1, Ordering::Relaxed);
            vec![window("Editor", 42)]
        }
        fn desktop_memberships(
            &mut self,
            windows: &[WindowDescriptor],
        ) -> HashMap<usize, Option<crate::virtual_desktop::VirtualDesktopId>> {
            windows.iter().map(|window| (window.hwnd, None)).collect()
        }
    }

    #[test]
    fn lazy_desktop_enrichment_reuses_fresh_basic_snapshot_without_reenumerating_windows() {
        let enumerations = Arc::new(AtomicUsize::new(0));
        let (repaint_tx, repaint_rx) = channel();
        let updates = Arc::new(PluginSearchUpdates::default());
        updates.set_repaint_callback(Arc::new(move || repaint_tx.send(()).unwrap()));
        let catalog = WindowCatalog::start(
            EnrichmentProvider {
                enumerations: Arc::clone(&enumerations),
            },
            updates,
        );
        catalog.snapshot_and_refresh();
        repaint_rx.recv().unwrap();
        assert_eq!(enumerations.load(Ordering::Relaxed), 1);
        assert!(
            !catalog
                .snapshot_with_desktops_and_refresh()
                .desktop_ids_ready
        );
        repaint_rx.recv().unwrap();
        assert_eq!(enumerations.load(Ordering::Relaxed), 1);
        assert!(
            catalog
                .snapshot_with_desktops_and_refresh()
                .desktop_ids_ready
        );
    }
}
