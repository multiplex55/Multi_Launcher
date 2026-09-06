use crate::actions::Action;
use crate::mouse_gestures::db::{GESTURES_FILE, GestureDb, load_gestures};
use crate::mouse_gestures::usage::{GESTURES_USAGE_FILE, GestureUsageEntry, load_usage};
use crate::plugins::calendar::{
    CALENDAR_EVENTS_FILE, CalendarSnapshot, build_snapshot, refresh_events_from_disk,
};
use crate::plugins::clipboard::{CLIPBOARD_FILE, load_history};
use crate::plugins::fav::{FAV_FILE, FavEntry, load_favs};
use crate::plugins::note::{Note, load_notes};
use crate::plugins::snippets::{SNIPPETS_FILE, SnippetEntry, load_snippets};
use crate::plugins::todo::{TODO_FILE, TodoEntry, load_todos};
use crate::{launcher, launcher::RecycleBinInfo};
use chrono::Local;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use sysinfo::{Disks, Networks, System};

#[derive(Clone, Debug, Default)]
pub struct SystemStatusSnapshot {
    pub cpu_percent: f32,
    pub mem_percent: f32,
    pub disk_percent: f32,
    pub net_rx_per_sec: f64,
    pub net_tx_per_sec: f64,
    pub volume_percent: Option<u8>,
    pub brightness_percent: Option<u8>,
}

#[derive(Clone, Debug, Default)]
pub struct RecycleBinSnapshot {
    pub size_bytes: u64,
    pub items: u64,
}

impl From<RecycleBinInfo> for RecycleBinSnapshot {
    fn from(info: RecycleBinInfo) -> Self {
        Self {
            size_bytes: info.size_bytes,
            items: info.items,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct GestureSnapshot {
    pub db: Arc<GestureDb>,
    pub usage: Arc<Vec<GestureUsageEntry>>,
}

#[derive(Clone)]
pub struct DashboardDataSnapshot {
    pub clipboard_history: Arc<Vec<String>>,
    pub snippets: Arc<Vec<SnippetEntry>>,
    pub notes: Arc<Vec<Note>>,
    pub todos: Arc<Vec<TodoEntry>>,
    pub calendar: Arc<CalendarSnapshot>,
    pub processes: Arc<Vec<Action>>,
    pub favorites: Arc<Vec<FavEntry>>,
    pub gestures: Arc<GestureSnapshot>,
    pub process_error: Option<String>,
    pub system_status: Option<SystemStatusSnapshot>,
    pub recycle_bin: Option<RecycleBinSnapshot>,
}

impl Default for DashboardDataSnapshot {
    fn default() -> Self {
        Self {
            clipboard_history: Arc::new(Vec::new()),
            snippets: Arc::new(Vec::new()),
            notes: Arc::new(Vec::new()),
            todos: Arc::new(Vec::new()),
            calendar: Arc::new(CalendarSnapshot::default()),
            processes: Arc::new(Vec::new()),
            favorites: Arc::new(Vec::new()),
            gestures: Arc::new(GestureSnapshot::default()),
            process_error: None,
            system_status: None,
            recycle_bin: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DashboardRefreshRequest {
    All,
    Clipboard,
    Snippets,
    Notes,
    Todos,
    Calendar,
    Processes,
    Favorites,
    Gestures,
    SystemStatus,
    RecycleBin,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct RefreshBatch {
    all: bool,
    clipboard: bool,
    snippets: bool,
    notes: bool,
    todos: bool,
    calendar: bool,
    processes: bool,
    favorites: bool,
    gestures: bool,
    system_status: bool,
    recycle_bin: bool,
}

impl RefreshBatch {
    fn insert(&mut self, request: DashboardRefreshRequest) {
        if request == DashboardRefreshRequest::All {
            *self = Self {
                all: true,
                ..Self::default()
            };
            return;
        }
        if self.all {
            return;
        }
        match request {
            DashboardRefreshRequest::All => unreachable!(),
            DashboardRefreshRequest::Clipboard => self.clipboard = true,
            DashboardRefreshRequest::Snippets => self.snippets = true,
            DashboardRefreshRequest::Notes => self.notes = true,
            DashboardRefreshRequest::Todos => self.todos = true,
            DashboardRefreshRequest::Calendar => self.calendar = true,
            DashboardRefreshRequest::Processes => self.processes = true,
            DashboardRefreshRequest::Favorites => self.favorites = true,
            DashboardRefreshRequest::Gestures => self.gestures = true,
            DashboardRefreshRequest::SystemStatus => self.system_status = true,
            DashboardRefreshRequest::RecycleBin => self.recycle_bin = true,
        }
    }

    fn contains(&self, request: DashboardRefreshRequest) -> bool {
        self.all
            || match request {
                DashboardRefreshRequest::All => self.all,
                DashboardRefreshRequest::Clipboard => self.clipboard,
                DashboardRefreshRequest::Snippets => self.snippets,
                DashboardRefreshRequest::Notes => self.notes,
                DashboardRefreshRequest::Todos => self.todos,
                DashboardRefreshRequest::Calendar => self.calendar,
                DashboardRefreshRequest::Processes => self.processes,
                DashboardRefreshRequest::Favorites => self.favorites,
                DashboardRefreshRequest::Gestures => self.gestures,
                DashboardRefreshRequest::SystemStatus => self.system_status,
                DashboardRefreshRequest::RecycleBin => self.recycle_bin,
            }
    }

    fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct PendingRefresh {
    batch: RefreshBatch,
    generation: u64,
}

struct DashboardShared {
    snapshot: Mutex<Arc<DashboardDataSnapshot>>,
    pending: Mutex<PendingRefresh>,
    next_generation: AtomicU64,
    completed_generation: Mutex<u64>,
    completion: Condvar,
    wake_tx: SyncSender<()>,
    shutting_down: AtomicBool,
}

/// Cheap, cloneable dashboard read/request handle. It never executes refresh work.
#[derive(Clone)]
pub struct DashboardDataCache {
    shared: Arc<DashboardShared>,
}

impl DashboardDataCache {
    fn disconnected() -> Self {
        let (wake_tx, _wake_rx) = sync_channel(1);
        Self {
            shared: Arc::new(DashboardShared {
                snapshot: Mutex::new(Arc::new(DashboardDataSnapshot::default())),
                pending: Mutex::new(PendingRefresh::default()),
                next_generation: AtomicU64::new(0),
                completed_generation: Mutex::new(0),
                completion: Condvar::new(),
                wake_tx,
                shutting_down: AtomicBool::new(true),
            }),
        }
    }

    pub fn new() -> Self {
        Self::disconnected()
    }

    pub fn snapshot(&self) -> Arc<DashboardDataSnapshot> {
        self.shared
            .snapshot
            .lock()
            .map(|snapshot| Arc::clone(&snapshot))
            .unwrap_or_else(|_| Arc::new(DashboardDataSnapshot::default()))
    }

    pub fn request_refresh(&self, request: DashboardRefreshRequest) {
        if self.shared.shutting_down.load(Ordering::Acquire) {
            return;
        }
        let generation = self.shared.next_generation.fetch_add(1, Ordering::AcqRel) + 1;
        if let Ok(mut pending) = self.shared.pending.lock() {
            pending.batch.insert(request);
            pending.generation = pending.generation.max(generation);
        } else {
            return;
        }
        match self.shared.wake_tx.try_send(()) {
            Ok(()) | Err(TrySendError::Full(())) => {}
            Err(TrySendError::Disconnected(())) => {
                self.shared.shutting_down.store(true, Ordering::Release);
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn wait_for_refresh(&self) {
        let target = self.shared.next_generation.load(Ordering::Acquire);
        let mut completed = self.shared.completed_generation.lock().unwrap();
        while *completed < target {
            completed = self.shared.completion.wait(completed).unwrap();
        }
    }
}

impl Default for DashboardDataCache {
    fn default() -> Self {
        Self::new()
    }
}

trait DashboardDataBackend: Send + 'static {
    fn refresh(
        &mut self,
        batch: RefreshBatch,
        current: &DashboardDataSnapshot,
    ) -> DashboardDataSnapshot;
}

#[derive(Default)]
struct ProductionDashboardBackend {
    last_network_totals: (u64, u64),
    last_network_time: Option<Instant>,
}

fn publish_loaded_or_retain<T>(
    current: &mut Arc<Vec<T>>,
    loaded: anyhow::Result<Vec<T>>,
    store: &'static str,
) {
    match loaded {
        Ok(entries) => *current = Arc::new(entries),
        Err(error) => tracing::error!(%error, store, "dashboard retained last-good snapshot"),
    }
}

impl ProductionDashboardBackend {
    fn system_status(&mut self, system: &mut System) -> SystemStatusSnapshot {
        system.refresh_cpu_usage();
        system.refresh_memory();
        let disks = Disks::new_with_refreshed_list();
        let mut networks = Networks::new_with_refreshed_list();
        networks.refresh(true);

        let total_memory = system.total_memory() as f32;
        let total_disk = disks
            .list()
            .iter()
            .map(|disk| disk.total_space())
            .sum::<u64>();
        let available_disk = disks
            .list()
            .iter()
            .map(|disk| disk.available_space())
            .sum::<u64>();
        let totals = networks.values().fold((0_u64, 0_u64), |totals, data| {
            (
                totals.0 + data.total_received(),
                totals.1 + data.total_transmitted(),
            )
        });
        let now = Instant::now();
        let elapsed = self
            .last_network_time
            .map(|last| now.duration_since(last).as_secs_f64().max(0.001))
            .unwrap_or(1.0);
        let snapshot = SystemStatusSnapshot {
            cpu_percent: system.global_cpu_usage(),
            mem_percent: if total_memory > 0.0 {
                system.used_memory() as f32 / total_memory * 100.0
            } else {
                0.0
            },
            disk_percent: if total_disk > 0 {
                total_disk.saturating_sub(available_disk) as f32 / total_disk as f32 * 100.0
            } else {
                0.0
            },
            net_rx_per_sec: totals.0.saturating_sub(self.last_network_totals.0) as f64 / elapsed,
            net_tx_per_sec: totals.1.saturating_sub(self.last_network_totals.1) as f64 / elapsed,
            volume_percent: get_system_volume(),
            brightness_percent: get_main_display_brightness(),
        };
        self.last_network_totals = totals;
        self.last_network_time = Some(now);
        snapshot
    }
}

impl DashboardDataBackend for ProductionDashboardBackend {
    fn refresh(
        &mut self,
        batch: RefreshBatch,
        current: &DashboardDataSnapshot,
    ) -> DashboardDataSnapshot {
        let mut next = current.clone();
        let needs_system = batch.contains(DashboardRefreshRequest::Processes)
            || batch.contains(DashboardRefreshRequest::SystemStatus);
        let mut system = needs_system.then(System::new_all);
        if batch.contains(DashboardRefreshRequest::Clipboard) {
            next.clipboard_history = Arc::new(
                load_history(CLIPBOARD_FILE)
                    .unwrap_or_default()
                    .into_iter()
                    .collect(),
            );
        }
        if batch.contains(DashboardRefreshRequest::Snippets) {
            publish_loaded_or_retain(&mut next.snippets, load_snippets(SNIPPETS_FILE), "snippets");
        }
        if batch.contains(DashboardRefreshRequest::Notes) {
            next.notes = Arc::new(load_notes().unwrap_or_default());
        }
        if batch.contains(DashboardRefreshRequest::Todos) {
            publish_loaded_or_retain(&mut next.todos, load_todos(TODO_FILE), "todos");
        }
        if batch.contains(DashboardRefreshRequest::Calendar) {
            let _ = refresh_events_from_disk(CALENDAR_EVENTS_FILE);
            next.calendar = Arc::new(build_snapshot(Local::now().naive_local()));
        }
        if batch.contains(DashboardRefreshRequest::Processes) {
            let processes = system
                .as_ref()
                .expect("system collected for process refresh")
                .processes()
                .values()
                .map(|process| crate::plugins::system_data::ProcessSnapshot {
                    name: process.name().to_string_lossy().into_owned(),
                    pid: process.pid().as_u32(),
                })
                .collect::<Vec<_>>();
            next.processes = Arc::new(crate::plugins::processes::actions_from_snapshot(
                "ps", &processes,
            ));
            next.process_error = None;
        }
        if batch.contains(DashboardRefreshRequest::Favorites) {
            publish_loaded_or_retain(&mut next.favorites, load_favs(FAV_FILE), "favorites");
        }
        if batch.contains(DashboardRefreshRequest::Gestures) {
            next.gestures = Arc::new(GestureSnapshot {
                db: Arc::new(load_gestures(GESTURES_FILE).unwrap_or_default()),
                usage: Arc::new(load_usage(GESTURES_USAGE_FILE)),
            });
        }
        if batch.contains(DashboardRefreshRequest::SystemStatus) {
            next.system_status = Some(
                self.system_status(
                    system
                        .as_mut()
                        .expect("system collected for status refresh"),
                ),
            );
        }
        if batch.contains(DashboardRefreshRequest::RecycleBin) {
            next.recycle_bin = launcher::query_recycle_bin().map(RecycleBinSnapshot::from);
        }
        next
    }
}

/// Owns the dashboard worker. Dropping it signals shutdown and joins deterministically.
pub struct DashboardRuntime {
    cache: DashboardDataCache,
    worker: Option<JoinHandle<()>>,
}

impl DashboardRuntime {
    pub fn start(repaint: impl Fn() + Send + Sync + 'static) -> Self {
        Self::start_with_backend(ProductionDashboardBackend::default(), repaint)
    }

    fn start_with_backend(
        backend: impl DashboardDataBackend,
        repaint: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        let (wake_tx, wake_rx) = sync_channel(1);
        let shared = Arc::new(DashboardShared {
            snapshot: Mutex::new(Arc::new(DashboardDataSnapshot::default())),
            pending: Mutex::new(PendingRefresh::default()),
            next_generation: AtomicU64::new(0),
            completed_generation: Mutex::new(0),
            completion: Condvar::new(),
            wake_tx,
            shutting_down: AtomicBool::new(false),
        });
        let cache = DashboardDataCache {
            shared: Arc::clone(&shared),
        };
        let worker = thread::Builder::new()
            .name("dashboard-refresh".into())
            .spawn(move || run_worker(shared, wake_rx, backend, Arc::new(repaint)))
            .expect("failed to start dashboard refresh worker");
        Self {
            cache,
            worker: Some(worker),
        }
    }

    pub fn cache(&self) -> DashboardDataCache {
        self.cache.clone()
    }
}

impl Drop for DashboardRuntime {
    fn drop(&mut self) {
        self.cache
            .shared
            .shutting_down
            .store(true, Ordering::Release);
        let _ = self.cache.shared.wake_tx.try_send(());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn run_worker(
    shared: Arc<DashboardShared>,
    wake_rx: Receiver<()>,
    mut backend: impl DashboardDataBackend,
    repaint: Arc<dyn Fn() + Send + Sync>,
) {
    while wake_rx.recv().is_ok() {
        if shared.shutting_down.load(Ordering::Acquire) {
            break;
        }
        let pending = shared
            .pending
            .lock()
            .map(|mut pending| std::mem::take(&mut *pending))
            .unwrap_or_default();
        let batch = pending.batch;
        if batch.is_empty() {
            continue;
        }
        let timer = batch.all.then(crate::performance::Timer::start);
        let current = shared
            .snapshot
            .lock()
            .map(|snapshot| Arc::clone(&snapshot))
            .unwrap_or_default();
        let next = backend.refresh(batch, &current);
        if let Ok(mut snapshot) = shared.snapshot.lock() {
            *snapshot = Arc::new(next);
        }
        if let Some(timer) = timer {
            timer.finish("startup.dashboard_initial_refresh_complete");
        }
        if let Ok(mut completed) = shared.completed_generation.lock() {
            *completed = (*completed).max(pending.generation);
            shared.completion.notify_all();
        }
        repaint();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::{Sender, TryRecvError, channel};

    struct ControlledBackend {
        started: Sender<RefreshBatch>,
        release: Receiver<()>,
        generation: usize,
    }

    impl DashboardDataBackend for ControlledBackend {
        fn refresh(
            &mut self,
            batch: RefreshBatch,
            current: &DashboardDataSnapshot,
        ) -> DashboardDataSnapshot {
            self.started.send(batch).unwrap();
            self.release.recv().unwrap();
            self.generation += 1;
            let mut next = current.clone();
            next.clipboard_history = Arc::new(vec![self.generation.to_string()]);
            next
        }
    }

    #[test]
    fn all_subsumes_specific_requests() {
        let mut batch = RefreshBatch::default();
        batch.insert(DashboardRefreshRequest::Notes);
        batch.insert(DashboardRefreshRequest::All);
        batch.insert(DashboardRefreshRequest::Todos);
        assert_eq!(
            batch,
            RefreshBatch {
                all: true,
                ..RefreshBatch::default()
            }
        );
    }

    #[test]
    fn invalid_store_refresh_retains_last_good_then_valid_refresh_recovers() {
        let initial = Arc::new(vec![SnippetEntry {
            alias: "saved".into(),
            text: "value".into(),
        }]);
        let mut current = Arc::clone(&initial);
        publish_loaded_or_retain(
            &mut current,
            Err(anyhow::anyhow!("malformed snippets")),
            "snippets",
        );
        assert!(Arc::ptr_eq(&current, &initial));

        let recovered = vec![SnippetEntry {
            alias: "recovered".into(),
            text: "value".into(),
        }];
        publish_loaded_or_retain(&mut current, Ok(recovered.clone()), "snippets");
        assert_eq!(current.as_ref(), &recovered);
    }

    #[test]
    fn requests_arriving_during_work_form_a_followup_batch() {
        let (started_tx, started_rx) = channel();
        let (release_tx, release_rx) = channel();
        let (repaint_tx, repaint_rx) = channel();
        let runtime = DashboardRuntime::start_with_backend(
            ControlledBackend {
                started: started_tx,
                release: release_rx,
                generation: 0,
            },
            move || {
                repaint_tx.send(()).unwrap();
            },
        );
        let cache = runtime.cache();
        cache.request_refresh(DashboardRefreshRequest::Notes);
        assert!(
            started_rx
                .recv()
                .unwrap()
                .contains(DashboardRefreshRequest::Notes)
        );
        cache.request_refresh(DashboardRefreshRequest::Todos);
        cache.request_refresh(DashboardRefreshRequest::Calendar);
        assert!(cache.snapshot().clipboard_history.is_empty());
        release_tx.send(()).unwrap();
        repaint_rx.recv().unwrap();
        let followup = started_rx.recv().unwrap();
        assert!(followup.contains(DashboardRefreshRequest::Todos));
        assert!(followup.contains(DashboardRefreshRequest::Calendar));
        release_tx.send(()).unwrap();
        repaint_rx.recv().unwrap();
        assert!(matches!(repaint_rx.try_recv(), Err(TryRecvError::Empty)));
        assert_eq!(cache.snapshot().clipboard_history.as_ref(), &["2"]);
        drop(runtime);
    }

    #[test]
    fn dropping_runtime_joins_an_idle_worker() {
        let (started_tx, _started_rx) = channel();
        let (_release_tx, release_rx) = channel();
        let runtime = DashboardRuntime::start_with_backend(
            ControlledBackend {
                started: started_tx,
                release: release_rx,
                generation: 0,
            },
            || {},
        );
        drop(runtime);
    }
}
#[cfg(target_os = "windows")]
fn get_system_volume() -> Option<u8> {
    use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
    use windows::Win32::Media::Audio::{
        IMMDeviceEnumerator, MMDeviceEnumerator, eMultimedia, eRender,
    };
    use windows::Win32::System::Com::{
        CLSCTX_ALL, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
    };

    unsafe {
        let mut percent = None;
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        if let Ok(enm) =
            CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL)
            && let Ok(device) = enm.GetDefaultAudioEndpoint(eRender, eMultimedia)
            && let Ok(vol) = device.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None)
            && let Ok(val) = vol.GetMasterVolumeLevelScalar()
        {
            percent = Some((val * 100.0).round() as u8);
        }
        CoUninitialize();
        percent
    }
}

#[cfg(not(target_os = "windows"))]
fn get_system_volume() -> Option<u8> {
    None
}

#[cfg(target_os = "windows")]
fn get_main_display_brightness() -> Option<u8> {
    use windows::Win32::Devices::Display::{
        DestroyPhysicalMonitors, GetMonitorBrightness, GetNumberOfPhysicalMonitorsFromHMONITOR,
        GetPhysicalMonitorsFromHMONITOR, PHYSICAL_MONITOR,
    };
    use windows::Win32::Foundation::{BOOL, LPARAM, RECT};
    use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, HDC, HMONITOR};

    unsafe extern "system" fn enum_monitors(
        hmonitor: HMONITOR,
        _hdc: HDC,
        _rect: *mut RECT,
        lparam: LPARAM,
    ) -> BOOL {
        let percent_ptr = lparam.0 as *mut u32;
        let mut count: u32 = 0;
        if unsafe { GetNumberOfPhysicalMonitorsFromHMONITOR(hmonitor, &mut count) }.is_ok() {
            let mut monitors = vec![PHYSICAL_MONITOR::default(); count as usize];
            if unsafe { GetPhysicalMonitorsFromHMONITOR(hmonitor, &mut monitors) }.is_ok() {
                if let Some(m) = monitors.first() {
                    let mut min = 0u32;
                    let mut cur = 0u32;
                    let mut max = 0u32;
                    if unsafe {
                        GetMonitorBrightness(m.hPhysicalMonitor, &mut min, &mut cur, &mut max)
                    } != 0
                    {
                        if max > min {
                            unsafe {
                                *percent_ptr = (cur - min) * 100 / (max - min);
                            }
                        } else {
                            unsafe {
                                *percent_ptr = 0;
                            }
                        }
                    }
                }
                let _ = unsafe { DestroyPhysicalMonitors(&monitors) };
            }
        }
        false.into()
    }

    let mut percent: u32 = 50;
    unsafe {
        let _ = EnumDisplayMonitors(
            HDC(std::ptr::null_mut()),
            None,
            Some(enum_monitors),
            LPARAM(&mut percent as *mut u32 as isize),
        );
    }
    Some(percent as u8)
}

#[cfg(not(target_os = "windows"))]
fn get_main_display_brightness() -> Option<u8> {
    None
}
