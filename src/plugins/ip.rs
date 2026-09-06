use crate::actions::Action;
use crate::plugin::{Plugin, PluginSearchUpdates};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const SUCCESS_TTL: Duration = Duration::from_secs(5 * 60);
const FAILURE_BACKOFF: Duration = Duration::from_secs(30);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(3);

trait PublicIpProvider: Send + Sync + 'static {
    fn lookup(&self) -> Result<String, String>;
}

struct HttpPublicIpProvider {
    client: reqwest::blocking::Client,
}

impl HttpPublicIpProvider {
    fn new() -> Self {
        let client = reqwest::blocking::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .build()
            .expect("build public IP HTTP client");
        Self { client }
    }
}

impl PublicIpProvider for HttpPublicIpProvider {
    fn lookup(&self) -> Result<String, String> {
        let response = self
            .client
            .get("https://api.ipify.org")
            .send()
            .and_then(reqwest::blocking::Response::error_for_status)
            .map_err(|error| error.to_string())?;
        let text = response.text().map_err(|error| error.to_string())?;
        let value = text.trim();
        if value.is_empty() {
            Err("public IP provider returned an empty response".into())
        } else {
            Ok(value.to_owned())
        }
    }
}

trait Clock: Send + Sync + 'static {
    fn now(&self) -> Instant;
}

struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

#[derive(Default)]
struct CacheState {
    last_good: Option<String>,
    fresh_until: Option<Instant>,
    retry_after: Option<Instant>,
    in_flight: bool,
    in_flight_ticket: Option<u64>,
    shutting_down: bool,
}

struct PublicIpCache {
    state: Arc<Mutex<CacheState>>,
    publication: Arc<Mutex<()>>,
    wake: SyncSender<()>,
    worker: Option<JoinHandle<()>>,
    clock: Arc<dyn Clock>,
    updates: Arc<PluginSearchUpdates>,
}

impl PublicIpCache {
    fn start(
        provider: Arc<dyn PublicIpProvider>,
        clock: Arc<dyn Clock>,
        updates: Arc<PluginSearchUpdates>,
    ) -> Self {
        let state = Arc::new(Mutex::new(CacheState::default()));
        let (wake, receiver) = sync_channel(1);
        let publication = Arc::new(Mutex::new(()));
        let worker_state = Arc::clone(&state);
        let worker_clock = Arc::clone(&clock);
        let worker = thread::Builder::new()
            .name("public-ip-refresh".into())
            .spawn({
                let publication = Arc::clone(&publication);
                let worker_updates = Arc::clone(&updates);
                move || {
                    run_worker(
                        receiver,
                        worker_state,
                        publication,
                        provider,
                        worker_clock,
                        worker_updates,
                    )
                }
            })
            .expect("start public IP refresh worker");
        Self {
            state,
            publication,
            wake,
            worker: Some(worker),
            clock,
            updates,
        }
    }

    fn snapshot_and_refresh(&self) -> Option<String> {
        let now = self.clock.now();
        let mut state = self.state.lock().ok()?;
        let fresh = state.fresh_until.is_some_and(|deadline| now < deadline);
        let backing_off = state.retry_after.is_some_and(|deadline| now < deadline);
        if !fresh && !backing_off && !state.in_flight {
            state.in_flight = true;
            state.in_flight_ticket = Some(self.updates.begin_refresh("ip"));
            if self.wake.try_send(()).is_err() {
                state.in_flight = false;
                state.in_flight_ticket = None;
            }
        }
        state.last_good.clone()
    }
}

impl Drop for PublicIpCache {
    fn drop(&mut self) {
        {
            let _publication = self.publication.lock().ok();
            if let Ok(mut state) = self.state.lock() {
                state.shutting_down = true;
            }
        }
        let _ = self.wake.try_send(());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn run_worker(
    receiver: Receiver<()>,
    state: Arc<Mutex<CacheState>>,
    publication: Arc<Mutex<()>>,
    provider: Arc<dyn PublicIpProvider>,
    clock: Arc<dyn Clock>,
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
        let result = provider.lookup();
        let now = clock.now();
        let Ok(_publication) = publication.lock() else {
            break;
        };
        let ticket = if let Ok(mut state) = state.lock() {
            if state.shutting_down {
                break;
            }
            state.in_flight = false;
            match result {
                Ok(value) => {
                    state.last_good = Some(value);
                    state.fresh_until = Some(now + SUCCESS_TTL);
                    state.retry_after = None;
                }
                Err(error) => {
                    tracing::debug!(?error, "public IP refresh failed");
                    state.retry_after = Some(now + FAILURE_BACKOFF);
                }
            }
            state.in_flight_ticket.take()
        } else {
            None
        };
        if let Some(ticket) = ticket {
            updates.notify_ticket("ip", ticket);
        }
    }
}

pub struct IpPlugin {
    public_ip: PublicIpCache,
}

impl IpPlugin {
    pub(crate) fn with_updates(updates: Arc<PluginSearchUpdates>) -> Self {
        Self {
            public_ip: PublicIpCache::start(
                Arc::new(HttpPublicIpProvider::new()),
                Arc::new(SystemClock),
                updates,
            ),
        }
    }

    #[cfg(test)]
    fn with_provider(
        provider: Arc<dyn PublicIpProvider>,
        clock: Arc<dyn Clock>,
        updates: Arc<PluginSearchUpdates>,
    ) -> Self {
        Self {
            public_ip: PublicIpCache::start(provider, clock, updates),
        }
    }
}

impl Default for IpPlugin {
    fn default() -> Self {
        Self::with_updates(Arc::new(PluginSearchUpdates::default()))
    }
}

impl Plugin for IpPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        if crate::common::strip_prefix_ci(query.trim(), "ip").is_none() {
            return Vec::new();
        }
        let mut out = Vec::new();
        if let Ok(adapters) = ipconfig::get_adapters() {
            for adapter in adapters {
                let name = adapter.friendly_name();
                for ip in adapter.ip_addresses() {
                    out.push(Action {
                        label: format!("{name}: {ip}"),
                        desc: "IP".into(),
                        action: format!("clipboard:{ip}"),
                        args: None,
                    });
                }
            }
        }
        if let Some(ip) = self.public_ip.snapshot_and_refresh() {
            out.push(Action {
                label: format!("Public: {ip}"),
                desc: "IP".into(),
                action: format!("clipboard:{ip}"),
                args: None,
            });
        }
        out
    }

    fn name(&self) -> &str {
        "ip"
    }

    fn description(&self) -> &str {
        "Show local and public IP addresses (prefix: `ip`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![Action {
            label: "ip".into(),
            desc: "IP".into(),
            action: "query:ip".into(),
            args: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::mpsc::{Sender, TryRecvError, channel};

    struct ManualClock(Mutex<Instant>);

    impl ManualClock {
        fn new() -> Self {
            Self(Mutex::new(Instant::now()))
        }

        fn advance(&self, duration: Duration) {
            *self.0.lock().unwrap() += duration;
        }
    }

    impl Clock for ManualClock {
        fn now(&self) -> Instant {
            *self.0.lock().unwrap()
        }
    }

    struct ControlledProvider {
        started: Sender<()>,
        releases: Mutex<Receiver<()>>,
        results: Mutex<VecDeque<Result<String, String>>>,
    }

    impl PublicIpProvider for ControlledProvider {
        fn lookup(&self) -> Result<String, String> {
            self.started.send(()).unwrap();
            self.releases.lock().unwrap().recv().unwrap();
            self.results.lock().unwrap().pop_front().unwrap()
        }
    }

    fn fixture(
        results: impl IntoIterator<Item = Result<String, String>>,
    ) -> (
        IpPlugin,
        Arc<ManualClock>,
        Receiver<()>,
        Sender<()>,
        Receiver<()>,
    ) {
        let (started_tx, started_rx) = channel();
        let (release_tx, release_rx) = channel();
        let (repaint_tx, repaint_rx) = channel();
        let updates = Arc::new(PluginSearchUpdates::default());
        updates.set_repaint_callback(Arc::new(move || {
            repaint_tx.send(()).unwrap();
        }));
        let clock = Arc::new(ManualClock::new());
        let provider = Arc::new(ControlledProvider {
            started: started_tx,
            releases: Mutex::new(release_rx),
            results: Mutex::new(results.into_iter().collect()),
        });
        let plugin = IpPlugin::with_provider(provider, clock.clone(), updates);
        (plugin, clock, started_rx, release_tx, repaint_rx)
    }

    fn public_values(actions: &[Action]) -> Vec<&str> {
        actions
            .iter()
            .filter_map(|action| action.label.strip_prefix("Public: "))
            .collect()
    }

    #[test]
    fn search_is_non_blocking_and_refresh_is_single_flight() {
        let (plugin, _clock, started, release, repaint) = fixture([Ok("203.0.113.7".into())]);

        assert!(public_values(&plugin.search("ip")).is_empty());
        started.recv().unwrap();
        assert!(public_values(&plugin.search("ip")).is_empty());
        assert!(matches!(started.try_recv(), Err(TryRecvError::Empty)));

        release.send(()).unwrap();
        repaint.recv().unwrap();
        assert_eq!(public_values(&plugin.search("ip")), ["203.0.113.7"]);
        assert!(matches!(started.try_recv(), Err(TryRecvError::Empty)));
    }

    #[test]
    fn success_ttl_and_failure_backoff_use_last_good_value() {
        let (plugin, clock, started, release, repaint) = fixture([
            Ok("203.0.113.8".into()),
            Err("offline".into()),
            Ok("203.0.113.9".into()),
        ]);

        plugin.search("ip");
        started.recv().unwrap();
        release.send(()).unwrap();
        repaint.recv().unwrap();
        assert_eq!(public_values(&plugin.search("ip")), ["203.0.113.8"]);

        clock.advance(SUCCESS_TTL);
        assert_eq!(public_values(&plugin.search("ip")), ["203.0.113.8"]);
        started.recv().unwrap();
        release.send(()).unwrap();
        repaint.recv().unwrap();
        assert_eq!(public_values(&plugin.search("ip")), ["203.0.113.8"]);
        assert!(matches!(started.try_recv(), Err(TryRecvError::Empty)));

        clock.advance(FAILURE_BACKOFF);
        assert_eq!(public_values(&plugin.search("ip")), ["203.0.113.8"]);
        started.recv().unwrap();
        release.send(()).unwrap();
        repaint.recv().unwrap();
        assert_eq!(public_values(&plugin.search("ip")), ["203.0.113.9"]);
    }

    #[test]
    fn dropping_an_idle_plugin_joins_its_worker() {
        let (plugin, _clock, _started, _release, _repaint) = fixture([Ok("203.0.113.10".into())]);
        drop(plugin);
    }

    #[test]
    fn drop_during_active_lookup_suppresses_publication_and_repaint() {
        let (started_tx, started_rx) = channel();
        let (release_tx, release_rx) = channel();
        let (repaint_tx, repaint_rx) = channel();
        let updates = Arc::new(PluginSearchUpdates::default());
        updates.set_repaint_callback(Arc::new(move || repaint_tx.send(()).unwrap()));
        let provider = Arc::new(ControlledProvider {
            started: started_tx,
            releases: Mutex::new(release_rx),
            results: Mutex::new([Ok("stale".into())].into_iter().collect()),
        });
        let plugin =
            IpPlugin::with_provider(provider, Arc::new(ManualClock::new()), Arc::clone(&updates));
        let state = Arc::clone(&plugin.public_ip.state);
        plugin.search("ip");
        started_rx.recv().unwrap();
        let (dropped_tx, dropped_rx) = channel();
        std::thread::spawn(move || {
            drop(plugin);
            dropped_tx.send(()).unwrap();
        });
        while !state.lock().unwrap().shutting_down {
            std::thread::yield_now();
        }
        release_tx.send(()).unwrap();
        dropped_rx.recv().unwrap();
        assert_eq!(updates.generation(), 0);
        assert!(matches!(repaint_rx.try_recv(), Err(TryRecvError::Empty)));
    }
}
