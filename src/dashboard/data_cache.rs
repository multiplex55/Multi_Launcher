use crate::actions::Action;
use crate::plugin::PluginManager;
use crate::plugins::calendar::{
    build_snapshot, refresh_events_from_disk, CalendarSnapshot, CALENDAR_EVENTS_FILE,
};
use crate::plugins::clipboard::{load_history, CLIPBOARD_FILE};
use crate::plugins::fav::{load_favs, FavEntry, FAV_FILE};
use crate::plugins::note::{load_notes, Note};
use crate::plugins::snippets::{load_snippets, SnippetEntry, SNIPPETS_FILE};
use crate::plugins::todo::{load_todos, TodoEntry, TODO_FILE};
use crate::watchlist::{watchlist_snapshot, WatchItemSnapshot};
use crate::{launcher, launcher::RecycleBinInfo};
use chrono::Local;
use std::sync::{Arc, Mutex};
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

#[derive(Clone)]
pub struct DashboardDataSnapshot {
    pub clipboard_history: Arc<Vec<String>>,
    pub snippets: Arc<Vec<SnippetEntry>>,
    pub notes: Arc<Vec<Note>>,
    pub todos: Arc<Vec<TodoEntry>>,
    pub calendar: Arc<CalendarSnapshot>,
    pub processes: Arc<Vec<Action>>,
    pub favorites: Arc<Vec<FavEntry>>,
    pub process_error: Option<String>,
    pub system_status: Option<SystemStatusSnapshot>,
    pub recycle_bin: Option<RecycleBinSnapshot>,
    pub watchlist_snapshot: Arc<Vec<WatchItemSnapshot>>,
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
            process_error: None,
            system_status: None,
            recycle_bin: None,
            watchlist_snapshot: Arc::new(Vec::new()),
        }
    }
}

impl DashboardDataSnapshot {
    fn with_clipboard_history(&self, history: Vec<String>) -> Self {
        Self {
            clipboard_history: Arc::new(history),
            snippets: Arc::clone(&self.snippets),
            notes: Arc::clone(&self.notes),
            todos: Arc::clone(&self.todos),
            calendar: Arc::clone(&self.calendar),
            processes: Arc::clone(&self.processes),
            favorites: Arc::clone(&self.favorites),
            process_error: self.process_error.clone(),
            system_status: self.system_status.clone(),
            recycle_bin: self.recycle_bin.clone(),
            watchlist_snapshot: Arc::clone(&self.watchlist_snapshot),
        }
    }

    fn with_snippets(&self, snippets: Vec<SnippetEntry>) -> Self {
        Self {
            clipboard_history: Arc::clone(&self.clipboard_history),
            snippets: Arc::new(snippets),
            notes: Arc::clone(&self.notes),
            todos: Arc::clone(&self.todos),
            calendar: Arc::clone(&self.calendar),
            processes: Arc::clone(&self.processes),
            favorites: Arc::clone(&self.favorites),
            process_error: self.process_error.clone(),
            system_status: self.system_status.clone(),
            recycle_bin: self.recycle_bin.clone(),
            watchlist_snapshot: Arc::clone(&self.watchlist_snapshot),
        }
    }

    fn with_notes(&self, notes: Vec<Note>) -> Self {
        Self {
            clipboard_history: Arc::clone(&self.clipboard_history),
            snippets: Arc::clone(&self.snippets),
            notes: Arc::new(notes),
            todos: Arc::clone(&self.todos),
            calendar: Arc::clone(&self.calendar),
            processes: Arc::clone(&self.processes),
            favorites: Arc::clone(&self.favorites),
            process_error: self.process_error.clone(),
            system_status: self.system_status.clone(),
            recycle_bin: self.recycle_bin.clone(),
            watchlist_snapshot: Arc::clone(&self.watchlist_snapshot),
        }
    }

    fn with_todos(&self, todos: Vec<TodoEntry>) -> Self {
        Self {
            clipboard_history: Arc::clone(&self.clipboard_history),
            snippets: Arc::clone(&self.snippets),
            notes: Arc::clone(&self.notes),
            todos: Arc::new(todos),
            calendar: Arc::clone(&self.calendar),
            processes: Arc::clone(&self.processes),
            favorites: Arc::clone(&self.favorites),
            process_error: self.process_error.clone(),
            system_status: self.system_status.clone(),
            recycle_bin: self.recycle_bin.clone(),
            watchlist_snapshot: Arc::clone(&self.watchlist_snapshot),
        }
    }

    fn with_favorites(&self, favorites: Vec<FavEntry>) -> Self {
        Self {
            clipboard_history: Arc::clone(&self.clipboard_history),
            snippets: Arc::clone(&self.snippets),
            notes: Arc::clone(&self.notes),
            todos: Arc::clone(&self.todos),
            calendar: Arc::clone(&self.calendar),
            processes: Arc::clone(&self.processes),
            favorites: Arc::new(favorites),
            process_error: self.process_error.clone(),
            system_status: self.system_status.clone(),
            recycle_bin: self.recycle_bin.clone(),
            watchlist_snapshot: Arc::clone(&self.watchlist_snapshot),
        }
    }

    fn with_processes(&self, processes: Vec<Action>, process_error: Option<String>) -> Self {
        Self {
            clipboard_history: Arc::clone(&self.clipboard_history),
            snippets: Arc::clone(&self.snippets),
            notes: Arc::clone(&self.notes),
            todos: Arc::clone(&self.todos),
            calendar: Arc::clone(&self.calendar),
            processes: Arc::new(processes),
            favorites: Arc::clone(&self.favorites),
            process_error,
            system_status: self.system_status.clone(),
            recycle_bin: self.recycle_bin.clone(),
            watchlist_snapshot: Arc::clone(&self.watchlist_snapshot),
        }
    }

    fn with_system_status(&self, system_status: Option<SystemStatusSnapshot>) -> Self {
        Self {
            clipboard_history: Arc::clone(&self.clipboard_history),
            snippets: Arc::clone(&self.snippets),
            notes: Arc::clone(&self.notes),
            todos: Arc::clone(&self.todos),
            calendar: Arc::clone(&self.calendar),
            processes: Arc::clone(&self.processes),
            favorites: Arc::clone(&self.favorites),
            process_error: self.process_error.clone(),
            system_status,
            recycle_bin: self.recycle_bin.clone(),
            watchlist_snapshot: Arc::clone(&self.watchlist_snapshot),
        }
    }

    fn with_recycle_bin(&self, recycle_bin: Option<RecycleBinSnapshot>) -> Self {
        Self {
            clipboard_history: Arc::clone(&self.clipboard_history),
            snippets: Arc::clone(&self.snippets),
            notes: Arc::clone(&self.notes),
            todos: Arc::clone(&self.todos),
            calendar: Arc::clone(&self.calendar),
            processes: Arc::clone(&self.processes),
            favorites: Arc::clone(&self.favorites),
            process_error: self.process_error.clone(),
            system_status: self.system_status.clone(),
            recycle_bin,
            watchlist_snapshot: Arc::clone(&self.watchlist_snapshot),
        }
    }

    fn with_calendar(&self, calendar: CalendarSnapshot) -> Self {
        Self {
            clipboard_history: Arc::clone(&self.clipboard_history),
            snippets: Arc::clone(&self.snippets),
            notes: Arc::clone(&self.notes),
            todos: Arc::clone(&self.todos),
            calendar: Arc::new(calendar),
            processes: Arc::clone(&self.processes),
            favorites: Arc::clone(&self.favorites),
            process_error: self.process_error.clone(),
            system_status: self.system_status.clone(),
            recycle_bin: self.recycle_bin.clone(),
            watchlist_snapshot: Arc::clone(&self.watchlist_snapshot),
        }
    }

    fn with_watchlist_snapshot(&self, snapshot: Arc<Vec<WatchItemSnapshot>>) -> Self {
        Self {
            clipboard_history: Arc::clone(&self.clipboard_history),
            snippets: Arc::clone(&self.snippets),
            notes: Arc::clone(&self.notes),
            todos: Arc::clone(&self.todos),
            calendar: Arc::clone(&self.calendar),
            processes: Arc::clone(&self.processes),
            favorites: Arc::clone(&self.favorites),
            process_error: self.process_error.clone(),
            system_status: self.system_status.clone(),
            recycle_bin: self.recycle_bin.clone(),
            watchlist_snapshot: snapshot,
        }
    }
}

struct DashboardDataState {
    snapshot: Arc<DashboardDataSnapshot>,
    last_process_refresh: Instant,
    last_system_refresh: Instant,
    last_recycle_refresh: Instant,
    last_network_totals: (u64, u64),
    last_network_time: Instant,
}

pub struct DashboardDataCache {
    state: Mutex<DashboardDataState>,
}

impl DashboardDataCache {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(DashboardDataState {
                snapshot: Arc::new(DashboardDataSnapshot::default()),
                last_process_refresh: Instant::now() - Duration::from_secs(60),
                last_system_refresh: Instant::now() - Duration::from_secs(60),
                last_recycle_refresh: Instant::now() - Duration::from_secs(60),
                last_network_totals: (0, 0),
                last_network_time: Instant::now() - Duration::from_secs(60),
            }),
        }
    }

    pub fn snapshot(&self) -> Arc<DashboardDataSnapshot> {
        self.state
            .lock()
            .map(|state| Arc::clone(&state.snapshot))
            .unwrap_or_else(|_| Arc::new(DashboardDataSnapshot::default()))
    }

    pub fn refresh_all(&self, plugins: &PluginManager) {
        self.refresh_clipboard();
        self.refresh_snippets();
        self.refresh_notes();
        self.refresh_todos();
        self.refresh_calendar();
        self.refresh_favorites();
        self.refresh_processes(plugins);
        self.refresh_system_status();
        self.refresh_recycle_bin();
    }

    pub fn watchlist_snapshot(&self) -> Arc<Vec<WatchItemSnapshot>> {
        self.state
            .lock()
            .map(|state| Arc::clone(&state.snapshot.watchlist_snapshot))
            .unwrap_or_else(|_| Arc::new(Vec::new()))
    }

    pub fn maybe_refresh_watchlist(&self, _refresh_ms: u64) {
        if let Ok(mut state) = self.state.lock() {
            let snapshot = watchlist_snapshot();
            state.snapshot = Arc::new(state.snapshot.with_watchlist_snapshot(snapshot));
        }
    }

    pub fn refresh_clipboard(&self) {
        let history = load_history(CLIPBOARD_FILE)
            .unwrap_or_default()
            .into_iter()
            .collect();
        if let Ok(mut state) = self.state.lock() {
            state.snapshot = Arc::new(state.snapshot.with_clipboard_history(history));
        }
    }

    pub fn refresh_snippets(&self) {
        let snippets = load_snippets(SNIPPETS_FILE).unwrap_or_default();
        if let Ok(mut state) = self.state.lock() {
            state.snapshot = Arc::new(state.snapshot.with_snippets(snippets));
        }
    }

    pub fn refresh_notes(&self) {
        let notes = load_notes().unwrap_or_default();
        if let Ok(mut state) = self.state.lock() {
            state.snapshot = Arc::new(state.snapshot.with_notes(notes));
        }
    }

    pub fn refresh_todos(&self) {
        let todos = load_todos(TODO_FILE).unwrap_or_default();
        if let Ok(mut state) = self.state.lock() {
            state.snapshot = Arc::new(state.snapshot.with_todos(todos));
        }
    }

    pub fn refresh_calendar(&self) {
        let _ = refresh_events_from_disk(CALENDAR_EVENTS_FILE);
        let snapshot = build_snapshot(Local::now().naive_local());
        if let Ok(mut state) = self.state.lock() {
            state.snapshot = Arc::new(state.snapshot.with_calendar(snapshot));
        }
    }

    pub fn refresh_favorites(&self) {
        let favorites = load_favs(FAV_FILE).unwrap_or_default();
        if let Ok(mut state) = self.state.lock() {
            state.snapshot = Arc::new(state.snapshot.with_favorites(favorites));
        }
    }

    pub fn maybe_refresh_processes(&self, plugins: &PluginManager, interval: Duration) {
        let should_refresh = self
            .state
            .lock()
            .map(|state| state.last_process_refresh.elapsed() >= interval)
            .unwrap_or(false);
        if should_refresh {
            self.refresh_processes(plugins);
        }
    }

    pub fn refresh_processes(&self, plugins: &PluginManager) {
        let (processes, error) = Self::load_processes(plugins);
        if let Ok(mut state) = self.state.lock() {
            state.snapshot = Arc::new(state.snapshot.with_processes(processes, error));
            state.last_process_refresh = Instant::now();
        }
    }

    pub fn maybe_refresh_system_status(&self, interval: Duration) {
        let should_refresh = self
            .state
            .lock()
            .map(|state| state.last_system_refresh.elapsed() >= interval)
            .unwrap_or(false);
        if should_refresh {
            self.refresh_system_status();
        }
    }

    pub fn refresh_system_status(&self) {
        let mut system = System::new_all();
        system.refresh_cpu_usage();
        system.refresh_memory();
        let disks = Disks::new_with_refreshed_list();
        let mut nets = Networks::new_with_refreshed_list();
        nets.refresh(true);

        let cpu_percent = system.global_cpu_usage();
        let total_mem = system.total_memory() as f32;
        let used_mem = system.used_memory() as f32;
        let mem_percent = if total_mem > 0.0 {
            used_mem / total_mem * 100.0
        } else {
            0.0
        };

        let mut total_disk = 0u64;
        let mut avail_disk = 0u64;
        for d in disks.list() {
            total_disk += d.total_space();
            avail_disk += d.available_space();
        }
        let disk_percent = if total_disk > 0 {
            (total_disk.saturating_sub(avail_disk)) as f32 / total_disk as f32 * 100.0
        } else {
            0.0
        };

        let mut total_rx = 0u64;
        let mut total_tx = 0u64;
        for (_, data) in nets.iter() {
            total_rx += data.total_received();
            total_tx += data.total_transmitted();
        }

        let now = Instant::now();
        let (last_totals, last_time) = if let Ok(state) = self.state.lock() {
            (state.last_network_totals, state.last_network_time)
        } else {
            ((0, 0), now - Duration::from_secs(1))
        };
        let dt = now.duration_since(last_time).as_secs_f64().max(0.001);
        let rx_rate = (total_rx.saturating_sub(last_totals.0)) as f64 / dt;
        let tx_rate = (total_tx.saturating_sub(last_totals.1)) as f64 / dt;

        let snapshot = SystemStatusSnapshot {
            cpu_percent,
            mem_percent,
            disk_percent,
            net_rx_per_sec: rx_rate,
            net_tx_per_sec: tx_rate,
            volume_percent: get_system_volume(),
            brightness_percent: get_main_display_brightness(),
        };

        if let Ok(mut state) = self.state.lock() {
            state.snapshot = Arc::new(state.snapshot.with_system_status(Some(snapshot)));
            state.last_system_refresh = now;
            state.last_network_totals = (total_rx, total_tx);
            state.last_network_time = now;
        }
    }

    pub fn maybe_refresh_recycle_bin(&self, interval: Duration) {
        let should_refresh = self
            .state
            .lock()
            .map(|state| state.last_recycle_refresh.elapsed() >= interval)
            .unwrap_or(false);
        if should_refresh {
            self.refresh_recycle_bin();
        }
    }

    pub fn refresh_recycle_bin(&self) {
        let snapshot = launcher::query_recycle_bin().map(|data| RecycleBinSnapshot::from(data));
        if let Ok(mut state) = self.state.lock() {
            state.snapshot = Arc::new(state.snapshot.with_recycle_bin(snapshot));
            state.last_recycle_refresh = Instant::now();
        }
    }

    fn load_processes(plugins: &PluginManager) -> (Vec<Action>, Option<String>) {
        let plugin = plugins.iter().find(|p| p.name() == "processes");
        if let Some(plugin) = plugin {
            (plugin.search("ps"), None)
        } else {
            (Vec::new(), Some("Processes plugin not available.".into()))
        }
    }
}

#[cfg(target_os = "windows")]
fn get_system_volume() -> Option<u8> {
    use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
    use windows::Win32::Media::Audio::{
        eMultimedia, eRender, IMMDeviceEnumerator, MMDeviceEnumerator,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
    };

    unsafe {
        let mut percent = None;
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        if let Ok(enm) =
            CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL)
        {
            if let Ok(device) = enm.GetDefaultAudioEndpoint(eRender, eMultimedia) {
                if let Ok(vol) = device.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None) {
                    if let Ok(val) = vol.GetMasterVolumeLevelScalar() {
                        percent = Some((val * 100.0).round() as u8);
                    }
                }
            }
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
        if GetNumberOfPhysicalMonitorsFromHMONITOR(hmonitor, &mut count).is_ok() {
            let mut monitors = vec![PHYSICAL_MONITOR::default(); count as usize];
            if GetPhysicalMonitorsFromHMONITOR(hmonitor, &mut monitors).is_ok() {
                if let Some(m) = monitors.first() {
                    let mut min = 0u32;
                    let mut cur = 0u32;
                    let mut max = 0u32;
                    if GetMonitorBrightness(m.hPhysicalMonitor, &mut min, &mut cur, &mut max) != 0 {
                        if max > min {
                            *percent_ptr = ((cur - min) * 100 / (max - min)) as u32;
                        } else {
                            *percent_ptr = 0;
                        }
                    }
                }
                let _ = DestroyPhysicalMonitors(&monitors);
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

impl Default for DashboardDataCache {
    fn default() -> Self {
        Self::new()
    }
}
