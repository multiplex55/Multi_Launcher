//! Browser tab search and switching on Windows using UI Automation.
//!
//! The plugin enumerates `TabItem` elements exposed by browsers via the
//! Windows UI Automation (UIA) tree. This currently works reliably with
//! Chromium-based browsers such as Microsoft Edge and Google Chrome; other
//! browsers may not expose their tabs as `TabItem` controls and will therefore
//! not appear in search results.
//!
//! Only top-level windows on the active desktop session are scanned. Tabs in
//! minimized or non‑UIA compliant windows might be missed, and changes in a
//! browser's accessibility implementation could break enumeration.
//!
//! When activation patterns like `SelectionItem` or `Invoke` are missing or
//! fail, the plugin falls back to simulating a mouse click on the tab's center.
//! This requires the window to be visible and may briefly move the cursor before
//! restoring its position.
//!
//! The plugin is Windows-only; on other platforms it returns no results.
use crate::actions::Action;
use crate::plugin::{Plugin, PluginSearchUpdates};
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub struct BrowserTabsPlugin {
    recalc_each_query: bool,
    cache: imp::BrowserTabsCache,
    last_forced_refresh: Mutex<Option<Instant>>,
}

mod imp {
    use super::*;
    use once_cell::sync::Lazy;
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
    use std::sync::{Mutex, Weak};
    use std::thread::{self, JoinHandle};
    use std::time::{Duration, Instant};
    use tracing::{error, warn};

    const CACHE_TTL: Duration = Duration::from_secs(2);
    static PRODUCTION_ENUMERATION_ACTIVE: AtomicBool = AtomicBool::new(false);

    struct ProductionEnumerationGuard(&'static AtomicBool);

    impl Drop for ProductionEnumerationGuard {
        fn drop(&mut self) {
            self.0.store(false, Ordering::Release);
        }
    }

    #[derive(Clone, Debug)]
    pub(super) struct TabInfo {
        pub(super) title: String,
        pub(super) url: String,
        pub(super) runtime_id: Vec<i32>,
    }

    trait TabProvider: Send + 'static {
        fn enumerate(&mut self) -> Vec<TabInfo>;
    }

    struct ProductionTabProvider;

    impl TabProvider for ProductionTabProvider {
        fn enumerate(&mut self) -> Vec<TabInfo> {
            if PRODUCTION_ENUMERATION_ACTIVE
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
            {
                return Vec::new();
            }
            let _guard = ProductionEnumerationGuard(&PRODUCTION_ENUMERATION_ACTIVE);
            enumerate_tabs()
        }
    }

    struct CacheState {
        tabs: Arc<Vec<TabInfo>>,
        last_refresh: Instant,
        in_flight: bool,
        in_flight_ticket: Option<u64>,
        refresh_disabled: bool,
        messages: Vec<String>,
        shutting_down: bool,
    }

    struct Shared {
        state: Mutex<CacheState>,
        publication: Mutex<()>,
        wake: SyncSender<()>,
        updates: Arc<PluginSearchUpdates>,
    }

    static CURRENT: Lazy<Mutex<Weak<Shared>>> = Lazy::new(|| Mutex::new(Weak::new()));

    pub(super) struct BrowserTabsCache {
        shared: Arc<Shared>,
        worker: Mutex<Option<JoinHandle<()>>>,
    }

    impl BrowserTabsCache {
        pub(super) fn start(updates: Arc<PluginSearchUpdates>) -> Self {
            Self::start_with_provider(ProductionTabProvider, updates)
        }

        fn start_with_provider(
            provider: impl TabProvider,
            updates: Arc<PluginSearchUpdates>,
        ) -> Self {
            let (wake, receiver) = sync_channel(1);
            let shared = Arc::new(Shared {
                state: Mutex::new(CacheState {
                    tabs: Arc::new(Vec::new()),
                    last_refresh: Instant::now() - Duration::from_secs(60),
                    in_flight: false,
                    in_flight_ticket: None,
                    refresh_disabled: false,
                    shutting_down: false,
                    messages: Vec::new(),
                }),
                publication: Mutex::new(()),
                wake,
                updates,
            });
            let worker_shared = Arc::clone(&shared);
            let worker = thread::Builder::new()
                .name("browser-tabs-refresh".into())
                .spawn(move || {
                    let cleanup = Arc::clone(&worker_shared);
                    let _ = catch_unwind(AssertUnwindSafe(|| {
                        run_worker(receiver, worker_shared, provider)
                    }));
                    terminate_worker(&cleanup);
                })
                .expect("start Browser Tabs refresh worker");
            if let Ok(mut current) = CURRENT.lock() {
                *current = Arc::downgrade(&shared);
            }
            Self {
                shared,
                worker: Mutex::new(Some(worker)),
            }
        }

        pub(super) fn cached_actions(&self, filter: &str, force: bool) -> Vec<Action> {
            self.request_refresh(force);
            let tabs = self
                .shared
                .state
                .lock()
                .map(|state| Arc::clone(&state.tabs))
                .unwrap_or_default();
            materialize_actions(&tabs, filter)
        }

        fn request_refresh(&self, force: bool) -> Option<u64> {
            let Ok(mut state) = self.shared.state.lock() else {
                return None;
            };
            if state.refresh_disabled || (!force && state.last_refresh.elapsed() <= CACHE_TTL) {
                return None;
            }
            let ticket = self.shared.updates.schedule_or_join("browser_tabs");
            crate::plugin::record_search_refresh_ticket("browser_tabs", ticket.id);
            if !ticket.start {
                return Some(ticket.id);
            }
            state.in_flight = true;
            state.in_flight_ticket = Some(ticket.id);
            if self.shared.wake.try_send(()).is_err() {
                state.in_flight = false;
                state.in_flight_ticket = None;
                state.refresh_disabled = true;
                self.shared.updates.cancel_ticket("browser_tabs", ticket.id);
            }
            Some(ticket.id)
        }

        fn clear(&self) {
            clear_shared(&self.shared);
        }

        pub(super) fn take_messages(&self) -> Vec<String> {
            self.shared
                .state
                .lock()
                .map(|mut state| std::mem::take(&mut state.messages))
                .unwrap_or_default()
        }

        pub(super) fn install_snapshot(&self, tabs: Vec<TabInfo>) {
            let _publication = self.shared.publication.lock().ok();
            if let Ok(mut state) = self.shared.state.lock() {
                state.tabs = Arc::new(tabs);
                state.last_refresh = Instant::now();
                state.in_flight = false;
                if let Some(ticket) = state.in_flight_ticket.take() {
                    self.shared.updates.cancel_ticket("browser_tabs", ticket);
                }
                state.refresh_disabled = true;
            }
        }
    }

    impl Drop for BrowserTabsCache {
        fn drop(&mut self) {
            {
                let _publication = self.shared.publication.lock().ok();
                if let Ok(mut state) = self.shared.state.lock() {
                    state.shutting_down = true;
                    if let Some(ticket) = state.in_flight_ticket.take() {
                        self.shared.updates.cancel_ticket("browser_tabs", ticket);
                    }
                }
            }
            let _ = self.shared.wake.try_send(());
            if let Ok(worker) = self.worker.get_mut()
                && let Some(worker) = worker.take()
            {
                // UI Automation FindAll has no cancellation or timeout API. Shutdown is owned and
                // deterministic once an in-progress provider call returns. Reap away from egui so
                // plugin removal cannot block the UI, while acknowledging that Windows cannot
                // preempt a UIA call that is itself hung.
                if worker.is_finished() {
                    let _ = worker.join();
                } else {
                    let _ = thread::Builder::new()
                        .name("browser-tabs-reaper".into())
                        .spawn(move || {
                            let _ = worker.join();
                        });
                }
            }
        }
    }

    fn run_worker(receiver: Receiver<()>, shared: Arc<Shared>, mut provider: impl TabProvider) {
        while receiver.recv().is_ok() {
            if shared
                .state
                .lock()
                .map(|state| state.shutting_down)
                .unwrap_or(true)
            {
                break;
            }
            let result = catch_unwind(AssertUnwindSafe(|| provider.enumerate()));
            let panicked = result.is_err();
            let Ok(_publication) = shared.publication.lock() else {
                break;
            };
            let ticket = if let Ok(mut state) = shared.state.lock() {
                if state.shutting_down {
                    break;
                }
                state.in_flight = false;
                if let Ok(tabs) = result {
                    state.tabs = Arc::new(tabs);
                    state.last_refresh = Instant::now();
                    state.messages.push("Tab cache refreshed".into());
                } else {
                    state.refresh_disabled = true;
                    state
                        .messages
                        .push("Tab cache refresh stopped unexpectedly".into());
                    error!("browser tab provider panicked");
                }
                state.in_flight_ticket.take()
            } else {
                None
            };
            if let Some(ticket) = ticket {
                if panicked {
                    shared.updates.cancel_ticket("browser_tabs", ticket);
                } else {
                    shared.updates.publish_ticket("browser_tabs", ticket);
                }
            }
            if panicked {
                break;
            }
        }
    }

    fn terminate_worker(shared: &Shared) {
        let _publication = shared
            .publication
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let ticket = {
            let mut state = shared
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.in_flight = false;
            state.refresh_disabled = true;
            state.in_flight_ticket.take()
        };
        if let Some(ticket) = ticket {
            shared.updates.cancel_ticket("browser_tabs", ticket);
        }
    }
    fn materialize_actions(tabs: &[TabInfo], filter: &str) -> Vec<Action> {
        tabs.iter()
            .filter(|tab| {
                !tab.runtime_id.is_empty()
                    && (filter.is_empty()
                        || tab.title.to_lowercase().contains(filter)
                        || tab.url.to_lowercase().contains(filter))
            })
            .map(|tab| {
                let id = tab
                    .runtime_id
                    .iter()
                    .map(i32::to_string)
                    .collect::<Vec<_>>()
                    .join("_");
                Action {
                    label: format!("Switch to {}", tab.title),
                    desc: if tab.url.is_empty() {
                        "Browser Tab".into()
                    } else {
                        tab.url.clone()
                    },
                    action: format!("tab:switch:{id}"),
                    args: None,
                }
            })
            .collect()
    }

    fn current() -> Option<Arc<Shared>> {
        CURRENT.lock().ok()?.upgrade()
    }

    pub(super) fn take_current_messages() -> Vec<String> {
        let Some(shared) = current() else {
            return Vec::new();
        };
        shared
            .state
            .lock()
            .map(|mut state| std::mem::take(&mut state.messages))
            .unwrap_or_default()
    }

    pub(super) fn rebuild_current() {
        if let Some(shared) = current()
            && let Ok(mut state) = shared.state.lock()
            && !state.in_flight
        {
            let ticket = shared.updates.schedule_or_join("browser_tabs");
            if !ticket.start {
                return;
            }
            state.in_flight = true;
            state.in_flight_ticket = Some(ticket.id);
            if shared.wake.try_send(()).is_err() {
                state.in_flight = false;
                state.in_flight_ticket = None;
                shared.updates.cancel_ticket("browser_tabs", ticket.id);
            }
        }
    }

    fn clear_shared(shared: &Shared) {
        if let Ok(mut state) = shared.state.lock() {
            state.tabs = Arc::new(Vec::new());
            state.last_refresh = Instant::now() - Duration::from_secs(60);
            state.messages.push("Tab cache cleared".into());
        }
        shared.updates.notify("browser_tabs");
    }

    pub(super) fn clear_current() {
        if let Some(shared) = current() {
            clear_shared(&shared);
        }
    }

    static LAST_ENUM_ERR: Lazy<Mutex<Instant>> =
        Lazy::new(|| Mutex::new(Instant::now() - Duration::from_secs(60)));

    fn log_enum_error(msg: &str, err: windows::core::Error) {
        if let Ok(mut last) = LAST_ENUM_ERR.lock()
            && last.elapsed() > Duration::from_secs(30)
        {
            error!(?err, "BrowserTabsPlugin: {msg}");
            *last = Instant::now();
        }
    }

    fn enumerate_tabs() -> Vec<TabInfo> {
        use windows::Win32::System::Com::{
            CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
            CoUninitialize,
        };
        use windows::Win32::UI::Accessibility::*;
        use windows::core::{BSTR, VARIANT};

        let mut out = Vec::new();
        unsafe {
            if let Err(e) = CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok() {
                log_enum_error("CoInitializeEx failed", e);
                return out;
            }
            let automation = match CoCreateInstance::<_, IUIAutomation>(
                &CUIAutomation,
                None,
                CLSCTX_INPROC_SERVER,
            ) {
                Ok(value) => value,
                Err(e) => {
                    log_enum_error("CoCreateInstance(IUIAutomation) failed", e);
                    CoUninitialize();
                    return out;
                }
            };
            let root = match automation.GetRootElement() {
                Ok(value) => value,
                Err(e) => {
                    log_enum_error("GetRootElement failed", e);
                    CoUninitialize();
                    return out;
                }
            };
            let cond = match automation.CreatePropertyCondition(
                UIA_ControlTypePropertyId,
                &VARIANT::from(UIA_TabItemControlTypeId.0),
            ) {
                Ok(value) => value,
                Err(e) => {
                    warn!(?e, "BrowserTabsPlugin: CreatePropertyCondition failed");
                    CoUninitialize();
                    return out;
                }
            };
            let tabs = match root.FindAll(TreeScope_Subtree, &cond) {
                Ok(value) => value,
                Err(e) => {
                    warn!(?e, "BrowserTabsPlugin: FindAll failed");
                    CoUninitialize();
                    return out;
                }
            };
            let count = match tabs.Length() {
                Ok(value) => value,
                Err(e) => {
                    warn!(?e, "BrowserTabsPlugin: tabs.Length failed");
                    CoUninitialize();
                    return out;
                }
            };
            for i in 0..count {
                let elem = match tabs.GetElement(i) {
                    Ok(value) => value,
                    Err(e) => {
                        warn!(?e, "BrowserTabsPlugin: GetElement failed");
                        continue;
                    }
                };
                let title = elem.CurrentName().unwrap_or_default().to_string();
                let mut url = String::new();
                if let Ok(value) =
                    elem.GetCurrentPropertyValue(UIA_LegacyIAccessibleValuePropertyId)
                    && let Ok(bstr) = BSTR::try_from(&value)
                {
                    url = bstr.to_string();
                }
                let mut runtime_id = Vec::new();
                if let Ok(sa_ptr) = elem.GetRuntimeId() {
                    use windows::Win32::System::Ole::{
                        SafeArrayDestroy, SafeArrayLock, SafeArrayUnlock,
                    };
                    if !sa_ptr.is_null() {
                        let psa = sa_ptr as *const _;
                        if SafeArrayLock(psa).is_ok() {
                            let len = (*psa).rgsabound[0].cElements as usize;
                            let data = (*psa).pvData as *const i32;
                            if !data.is_null() {
                                runtime_id = std::slice::from_raw_parts(data, len).to_vec();
                            }
                            let _ = SafeArrayUnlock(psa);
                        }
                        let _ = SafeArrayDestroy(psa);
                    }
                }
                if !runtime_id.is_empty() {
                    out.push(TabInfo {
                        title,
                        url,
                        runtime_id,
                    });
                }
            }
            CoUninitialize();
        }
        out
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::sync::mpsc::{Sender, TryRecvError, channel};

        struct ControlledProvider {
            started: Sender<()>,
            release: Receiver<Vec<TabInfo>>,
        }

        struct DropControlledProvider {
            started: Sender<()>,
            release: Receiver<Vec<TabInfo>>,
            finished: Sender<()>,
        }

        impl TabProvider for DropControlledProvider {
            fn enumerate(&mut self) -> Vec<TabInfo> {
                self.started.send(()).unwrap();
                let tabs = self.release.recv().unwrap();
                self.finished.send(()).unwrap();
                tabs
            }
        }

        impl TabProvider for ControlledProvider {
            fn enumerate(&mut self) -> Vec<TabInfo> {
                self.started.send(()).unwrap();
                self.release.recv().unwrap()
            }
        }

        #[test]
        fn refresh_is_nonblocking_single_flight_notifies_and_shuts_down() {
            let (started_tx, started_rx) = channel();
            let (release_tx, release_rx) = channel();
            let (repaint_tx, repaint_rx) = channel();
            let updates = Arc::new(PluginSearchUpdates::default());
            updates.set_repaint_callback(Arc::new(move || repaint_tx.send(()).unwrap()));
            let cache = BrowserTabsCache::start_with_provider(
                ControlledProvider {
                    started: started_tx,
                    release: release_rx,
                },
                Arc::clone(&updates),
            );
            assert!(cache.cached_actions("", false).is_empty());
            let ticket = updates.active_ticket("browser_tabs").unwrap();
            started_rx.recv().unwrap();
            assert!(cache.cached_actions("", true).is_empty());
            assert_eq!(updates.active_ticket("browser_tabs"), Some(ticket));
            assert!(matches!(started_rx.try_recv(), Err(TryRecvError::Empty)));
            release_tx
                .send(vec![TabInfo {
                    title: "Documentation".into(),
                    url: "https://example.test/docs".into(),
                    runtime_id: vec![1, 2],
                }])
                .unwrap();
            repaint_rx.recv().unwrap();
            assert_eq!(updates.generation(), 1);
            assert_eq!(updates.active_ticket("browser_tabs"), None);
            assert_eq!(updates.published_ticket("browser_tabs"), Some(ticket));
            let actions = cache.cached_actions("doc", false);
            assert_eq!(actions.len(), 1);
            assert_eq!(actions[0].action, "tab:switch:1_2");
            assert_eq!(updates.active_ticket("browser_tabs"), None);
            drop(cache);
        }

        #[test]
        fn populated_snapshot_filters_without_discovery() {
            let updates = Arc::new(PluginSearchUpdates::default());
            let cache = BrowserTabsCache::start_with_provider(
                ControlledProvider {
                    started: channel().0,
                    release: channel().1,
                },
                updates,
            );
            cache.install_snapshot(vec![TabInfo {
                title: "Release notes".into(),
                url: "https://example.test/release".into(),
                runtime_id: vec![7],
            }]);
            assert_eq!(cache.cached_actions("release", false).len(), 1);
            drop(cache);
        }

        #[test]
        fn explicit_rebuild_acquires_publishes_and_notifies_ticket() {
            let (started_tx, started_rx) = channel();
            let (release_tx, release_rx) = channel();
            let (repaint_tx, repaint_rx) = channel();
            let updates = Arc::new(PluginSearchUpdates::default());
            updates.set_repaint_callback(Arc::new(move || repaint_tx.send(()).unwrap()));
            let cache = BrowserTabsCache::start_with_provider(
                ControlledProvider {
                    started: started_tx,
                    release: release_rx,
                },
                Arc::clone(&updates),
            );
            rebuild_current();
            let ticket = updates.active_ticket("browser_tabs").unwrap();
            started_rx.recv().unwrap();
            release_tx.send(Vec::new()).unwrap();
            repaint_rx.recv().unwrap();
            assert_eq!(updates.published_ticket("browser_tabs"), Some(ticket));
            assert!(updates.ticket_resolved("browser_tabs", ticket));
            drop(cache);
        }

        #[test]
        fn drop_while_blocked_resolves_ticket_and_suppresses_stale_publication() {
            let (started_tx, started_rx) = channel();
            let (release_tx, release_rx) = channel();
            let (finished_tx, finished_rx) = channel();
            let (repaint_tx, repaint_rx) = channel();
            let updates = Arc::new(PluginSearchUpdates::default());
            updates.set_repaint_callback(Arc::new(move || repaint_tx.send(()).unwrap()));
            let cache = BrowserTabsCache::start_with_provider(
                DropControlledProvider {
                    started: started_tx,
                    release: release_rx,
                    finished: finished_tx,
                },
                Arc::clone(&updates),
            );
            assert!(cache.cached_actions("", false).is_empty());
            started_rx.recv().unwrap();
            drop(cache);
            repaint_rx.recv().unwrap();
            release_tx
                .send(vec![TabInfo {
                    title: "stale".into(),
                    url: String::new(),
                    runtime_id: vec![9],
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

        impl TabProvider for PanicProvider {
            fn enumerate(&mut self) -> Vec<TabInfo> {
                panic!("controlled browser tab provider panic")
            }
        }

        #[test]
        fn provider_panic_resolves_ticket_and_disables_rearming() {
            let (repaint_tx, repaint_rx) = channel();
            let updates = Arc::new(PluginSearchUpdates::default());
            updates.set_repaint_callback(Arc::new(move || repaint_tx.send(()).unwrap()));
            let cache = BrowserTabsCache::start_with_provider(PanicProvider, Arc::clone(&updates));

            cache.cached_actions("", false);
            repaint_rx.recv().unwrap();
            let state = cache.shared.state.lock().unwrap();
            assert!(state.refresh_disabled);
            assert!(!state.in_flight);
            drop(state);
            assert_eq!(updates.active_ticket("browser_tabs"), None);
            let generation = updates.generation();
            for _ in 0..4 {
                cache.cached_actions("", false);
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
            let cache = BrowserTabsCache {
                shared: Arc::new(Shared {
                    state: Mutex::new(CacheState {
                        tabs: Arc::new(Vec::new()),
                        last_refresh: Instant::now() - Duration::from_secs(60),
                        in_flight: false,
                        in_flight_ticket: None,
                        refresh_disabled: false,
                        messages: Vec::new(),
                        shutting_down: false,
                    }),
                    publication: Mutex::new(()),
                    wake,
                    updates: Arc::clone(&updates),
                }),
                worker: Mutex::new(None),
            };

            cache.cached_actions("", false);
            repaint_rx.recv().unwrap();
            let generation = updates.generation();
            for _ in 0..4 {
                cache.cached_actions("", false);
            }
            assert!(cache.shared.state.lock().unwrap().refresh_disabled);
            assert_eq!(updates.generation(), generation);
            assert!(matches!(repaint_rx.try_recv(), Err(TryRecvError::Empty)));
        }
        #[test]
        fn recalc_each_query_coalesces_many_consumer_filters_per_cache_epoch() {
            let (started_tx, started_rx) = channel();
            let (release_tx, release_rx) = channel();
            let updates = Arc::new(PluginSearchUpdates::default());
            let plugin = super::super::BrowserTabsPlugin {
                recalc_each_query: true,
                cache: BrowserTabsCache::start_with_provider(
                    ControlledProvider {
                        started: started_tx,
                        release: release_rx,
                    },
                    Arc::clone(&updates),
                ),
                last_forced_refresh: Mutex::new(None),
            };

            assert!(plugin.search("tab docs").is_empty());
            started_rx.recv().unwrap();
            release_tx
                .send(vec![TabInfo {
                    title: "Docs".into(),
                    url: "https://example.test/docs".into(),
                    runtime_id: vec![1],
                }])
                .unwrap();
            while updates.generation() == 0 {
                std::thread::yield_now();
            }

            // Launcher notifier requery repeats the exact query.
            assert_eq!(plugin.search("tab docs").len(), 1);
            // PluginHome uses this exact helper to materialize its pinned search preview.
            assert_eq!(
                crate::dashboard::widgets::plugin_home::search_plugin_actions(&plugin, "tab docs")
                    .len(),
                1
            );
            assert!(matches!(started_rx.try_recv(), Err(TryRecvError::Empty)));

            for index in 0..40 {
                let _ = plugin.search(&format!("tab consumer-{index}"));
            }
            assert!(matches!(started_rx.try_recv(), Err(TryRecvError::Empty)));
        }
    }
}

impl BrowserTabsPlugin {
    pub(crate) fn with_updates(updates: Arc<PluginSearchUpdates>) -> Self {
        Self {
            recalc_each_query: false,
            cache: imp::BrowserTabsCache::start(updates),
            last_forced_refresh: Mutex::new(None),
        }
    }

    #[doc(hidden)]
    pub fn with_cached_tabs_for_benchmark(
        tabs: impl IntoIterator<Item = (String, String, Vec<i32>)>,
    ) -> Self {
        let plugin = Self::default();
        plugin.cache.install_snapshot(
            tabs.into_iter()
                .map(|(title, url, runtime_id)| imp::TabInfo {
                    title,
                    url,
                    runtime_id,
                })
                .collect(),
        );
        plugin
    }
}

impl Default for BrowserTabsPlugin {
    fn default() -> Self {
        Self::with_updates(Arc::new(PluginSearchUpdates::default()))
    }
}

impl Plugin for BrowserTabsPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        const PREFIX: &str = "tab";
        let trimmed = query.trim();
        let rest = match crate::common::strip_prefix_ci(trimmed, PREFIX) {
            Some(r) => r.trim(),
            None => return Vec::new(),
        };

        if rest.eq_ignore_ascii_case("clear") {
            return vec![Action {
                label: "Clear tab cache".into(),
                desc: "Remove cached browser tabs".into(),
                action: "tab:clear".into(),
                args: None,
            }];
        }
        if rest.eq_ignore_ascii_case("cache") {
            return vec![Action {
                label: "Rebuild tab cache".into(),
                desc: "Enumerate browser tabs".into(),
                action: "tab:cache".into(),
                args: None,
            }];
        }

        let filter = rest.to_lowercase();

        let force = self.recalc_each_query
            && self
                .last_forced_refresh
                .lock()
                .map(|mut last| {
                    const FORCE_COOLDOWN: Duration = Duration::from_secs(2);
                    let due = last.is_none_or(|instant| instant.elapsed() >= FORCE_COOLDOWN);
                    if due {
                        *last = Some(Instant::now());
                    }
                    due
                })
                .unwrap_or(false);
        self.cache.cached_actions(&filter, force)
    }

    fn name(&self) -> &str {
        "browser_tabs"
    }

    fn description(&self) -> &str {
        "Switch between browser tabs (prefix: `tab`). Uses UI Automation and may simulate a mouse click when activation patterns are unsupported"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![
            Action {
                label: "tab".into(),
                desc: "Browser tabs".into(),
                action: "query:tab ".into(),
                args: None,
            },
            Action {
                label: "tab cache".into(),
                desc: "Rebuild browser tab cache".into(),
                action: "tab:cache".into(),
                args: None,
            },
            Action {
                label: "tab clear".into(),
                desc: "Clear browser tab cache".into(),
                action: "tab:clear".into(),
                args: None,
            },
        ]
    }

    fn default_settings(&self) -> Option<serde_json::Value> {
        serde_json::to_value(BrowserTabsPluginSettings {
            recalc_each_query: self.recalc_each_query,
        })
        .ok()
    }

    fn apply_settings(&mut self, value: &serde_json::Value) {
        if let Ok(cfg) = serde_json::from_value::<BrowserTabsPluginSettings>(value.clone()) {
            self.recalc_each_query = cfg.recalc_each_query;
            if let Ok(last) = self.last_forced_refresh.get_mut() {
                *last = None;
            }
        }
    }

    fn settings_ui(&mut self, ui: &mut egui::Ui, value: &mut serde_json::Value) {
        let mut cfg: BrowserTabsPluginSettings =
            serde_json::from_value(value.clone()).unwrap_or_default();
        ui.checkbox(
            &mut cfg.recalc_each_query,
            "Recalculate cache on each query",
        );
        ui.label(
            "If UI Automation can't activate a tab, a mouse click is simulated and the cursor may briefly move",
        );
        self.recalc_each_query = cfg.recalc_each_query;
        if let Ok(v) = serde_json::to_value(&cfg) {
            *value = v;
        } else {
            tracing::error!("failed to serialize browser tabs settings");
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct BrowserTabsPluginSettings {
    #[serde(default)]
    pub recalc_each_query: bool,
}

pub fn take_cache_messages() -> Vec<String> {
    imp::take_current_messages()
}

pub fn rebuild_cache() {
    imp::rebuild_current();
}

pub fn clear_cache() {
    imp::clear_current();
}
