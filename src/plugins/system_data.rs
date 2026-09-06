use crate::plugin::PluginSearchUpdates;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use sysinfo::{Disks, System};

const REFRESH_TTL: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct SystemDataSnapshot {
    pub processes: Arc<Vec<ProcessSnapshot>>,
    pub cpu_usage: f32,
    pub total_memory: u64,
    pub used_memory: u64,
    pub total_disk: u64,
    pub available_disk: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProcessSnapshot {
    pub name: String,
    pub pid: u32,
}

trait SystemDataProvider: Send + 'static {
    fn refresh(&mut self) -> SystemDataSnapshot;
}

#[derive(Default)]
struct ProductionSystemDataProvider;

impl SystemDataProvider for ProductionSystemDataProvider {
    fn refresh(&mut self) -> SystemDataSnapshot {
        let mut system = System::new_all();
        system.refresh_cpu_usage();
        system.refresh_memory();
        let processes = system
            .processes()
            .values()
            .map(|process| ProcessSnapshot {
                name: process.name().to_string_lossy().into_owned(),
                pid: process.pid().as_u32(),
            })
            .collect();
        let disks = Disks::new_with_refreshed_list();
        let (total_disk, available_disk) =
            disks
                .list()
                .iter()
                .fold((0_u64, 0_u64), |(total, available), disk| {
                    (
                        total.saturating_add(disk.total_space()),
                        available.saturating_add(disk.available_space()),
                    )
                });
        SystemDataSnapshot {
            processes: Arc::new(processes),
            cpu_usage: system.global_cpu_usage(),
            total_memory: system.total_memory(),
            used_memory: system.used_memory(),
            total_disk,
            available_disk,
        }
    }
}

#[derive(Default)]
struct CacheState {
    snapshot: Option<Arc<SystemDataSnapshot>>,
    fresh_until: Option<Instant>,
    in_flight: bool,
}

#[derive(Clone)]
pub(crate) struct SystemDataCache {
    state: Arc<Mutex<CacheState>>,
    wake: SyncSender<()>,
}

impl SystemDataCache {
    #[cfg(test)]
    pub(crate) fn from_snapshot(snapshot: SystemDataSnapshot) -> Self {
        let (wake, _receiver) = sync_channel(1);
        Self {
            state: Arc::new(Mutex::new(CacheState {
                snapshot: Some(Arc::new(snapshot)),
                fresh_until: Some(Instant::now() + REFRESH_TTL),
                in_flight: false,
            })),
            wake,
        }
    }
    pub(crate) fn snapshot_and_refresh(&self) -> Option<Arc<SystemDataSnapshot>> {
        let now = Instant::now();
        let Ok(mut state) = self.state.lock() else {
            return None;
        };
        if !state.in_flight && !state.fresh_until.is_some_and(|deadline| now < deadline) {
            state.in_flight = true;
            if self.wake.try_send(()).is_err() {
                state.in_flight = false;
            }
        }
        state.snapshot.as_ref().map(Arc::clone)
    }
}

pub(crate) struct SystemDataRuntime {
    cache: SystemDataCache,
    shutting_down: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl SystemDataRuntime {
    pub(crate) fn start(updates: Arc<PluginSearchUpdates>) -> Self {
        Self::start_with_provider(ProductionSystemDataProvider, updates)
    }

    fn start_with_provider(
        provider: impl SystemDataProvider,
        updates: Arc<PluginSearchUpdates>,
    ) -> Self {
        let (wake, receiver) = sync_channel(1);
        let state = Arc::new(Mutex::new(CacheState::default()));
        let shutting_down = Arc::new(AtomicBool::new(false));
        let worker_state = Arc::clone(&state);
        let worker_shutdown = Arc::clone(&shutting_down);
        let worker = thread::Builder::new()
            .name("plugin-system-data-refresh".into())
            .spawn(move || run_worker(receiver, worker_state, worker_shutdown, provider, updates))
            .expect("start plugin system-data worker");
        let runtime = Self {
            cache: SystemDataCache { state, wake },
            shutting_down,
            worker: Some(worker),
        };
        runtime
    }

    pub(crate) fn cache(&self) -> SystemDataCache {
        self.cache.clone()
    }
}

impl Drop for SystemDataRuntime {
    fn drop(&mut self) {
        self.shutting_down.store(true, Ordering::Release);
        let _ = self.cache.wake.try_send(());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn run_worker(
    receiver: Receiver<()>,
    state: Arc<Mutex<CacheState>>,
    shutting_down: Arc<AtomicBool>,
    mut provider: impl SystemDataProvider,
    updates: Arc<PluginSearchUpdates>,
) {
    while receiver.recv().is_ok() {
        if shutting_down.load(Ordering::Acquire) {
            break;
        }
        let snapshot = provider.refresh();
        if shutting_down.load(Ordering::Acquire) {
            break;
        }
        if let Ok(mut state) = state.lock() {
            state.snapshot = Some(Arc::new(snapshot));
            state.fresh_until = Some(Instant::now() + REFRESH_TTL);
            state.in_flight = false;
        }
        updates.notify("system_data");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::{Sender, TryRecvError, channel};

    struct ControlledProvider {
        started: Sender<()>,
        release: Receiver<()>,
        snapshot: SystemDataSnapshot,
    }

    impl SystemDataProvider for ControlledProvider {
        fn refresh(&mut self) -> SystemDataSnapshot {
            self.started.send(()).unwrap();
            self.release.recv().unwrap();
            self.snapshot.clone()
        }
    }

    #[test]
    fn refresh_is_single_flight_and_publishes_one_snapshot() {
        let (started_tx, started_rx) = channel();
        let (release_tx, release_rx) = channel();
        let (repaint_tx, repaint_rx) = channel();
        let updates = Arc::new(PluginSearchUpdates::default());
        updates.set_repaint_callback(Arc::new(move || repaint_tx.send(()).unwrap()));
        let runtime = SystemDataRuntime::start_with_provider(
            ControlledProvider {
                started: started_tx,
                release: release_rx,
                snapshot: SystemDataSnapshot {
                    processes: Arc::new(vec![ProcessSnapshot {
                        name: "sample.exe".into(),
                        pid: 42,
                    }]),
                    cpu_usage: 12.0,
                    total_memory: 100,
                    used_memory: 40,
                    total_disk: 200,
                    available_disk: 50,
                },
            },
            Arc::clone(&updates),
        );
        let cache = runtime.cache();
        cache.snapshot_and_refresh();
        started_rx.recv().unwrap();
        assert!(cache.snapshot_and_refresh().is_none());
        assert!(matches!(started_rx.try_recv(), Err(TryRecvError::Empty)));
        release_tx.send(()).unwrap();
        repaint_rx.recv().unwrap();
        assert_eq!(updates.generation(), 1);
        let snapshot = cache.snapshot_and_refresh().unwrap();
        assert_eq!(snapshot.processes[0].pid, 42);
        assert_eq!(snapshot.used_memory, 40);
        drop(runtime);
    }

    #[test]
    fn dropping_idle_runtime_joins_worker() {
        let (started_tx, started_rx) = channel();
        let (release_tx, release_rx) = channel();
        let (repaint_tx, repaint_rx) = channel();
        let updates = Arc::new(PluginSearchUpdates::default());
        updates.set_repaint_callback(Arc::new(move || repaint_tx.send(()).unwrap()));
        let runtime = SystemDataRuntime::start_with_provider(
            ControlledProvider {
                started: started_tx,
                release: release_rx,
                snapshot: SystemDataSnapshot::default(),
            },
            updates,
        );
        runtime.cache().snapshot_and_refresh();
        started_rx.recv().unwrap();
        release_tx.send(()).unwrap();
        repaint_rx.recv().unwrap();
        drop(runtime);
    }
}
