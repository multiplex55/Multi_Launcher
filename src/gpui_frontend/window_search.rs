//! Scoped stock Window Switch search and exact result receipts.
use super::*;
use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::windows::{WindowSwitchCatalog, WindowSwitchRow};

#[derive(Default)]
pub(super) struct Owner {
    catalog: Option<Arc<WindowSwitchCatalog>>,
    epoch: Arc<AtomicU64>,
    generation: u64,
    rows: HashMap<usize, WindowSwitchRow>,
    deferred_update: bool,
}
impl Owner {
    pub fn defer_update(&mut self) {
        self.deferred_update = true;
    }

    pub fn take_deferred_update(&mut self) -> bool {
        std::mem::take(&mut self.deferred_update)
    }

    pub fn retire(&mut self) {
        self.epoch.fetch_add(1, Ordering::AcqRel);
        self.rows.clear();
    }
}
impl Drop for Owner {
    fn drop(&mut self) {
        self.retire();
    }
}

pub(super) fn query_is_windows(query: &str) -> bool {
    let (filtered, _) = multi_launcher::common::query::split_action_filters(query);
    filtered
        .split_whitespace()
        .next()
        .is_some_and(|word| word.eq_ignore_ascii_case("win"))
}
pub(super) fn enabled(
    plugins: Option<&HashSet<String>>,
    caps: Option<&HashMap<String, Vec<String>>>,
) -> bool {
    cfg!(windows)
        && plugins.is_none_or(|plugins| plugins.contains("windows"))
        && multi_launcher::settings::provider_search_capability_enabled(caps, "windows")
}
pub(super) fn action_is_switch(name: &str, action: &Action) -> bool {
    name == "windows"
        && action.desc == "Windows"
        && action.args.is_none()
        && matches!(
            parse_action(action),
            Ok(Command::System(
                multi_launcher::commands::SystemCommand::WindowSwitch(_)
            ))
        )
}
fn publication_current(
    expected_epoch: u64,
    epoch: u64,
    expected_generation: u64,
    generation: u64,
    expected_query: &str,
    result_query: &str,
    live_query: &str,
    active: bool,
    fenced: bool,
    enabled: bool,
) -> bool {
    expected_epoch == epoch
        && expected_generation == generation
        && expected_query == result_query
        && expected_query == live_query
        && active
        && !fenced
        && enabled
}

struct SnapshotPlugin<'a>(&'a [WindowSwitchRow]);
impl Plugin for SnapshotPlugin<'_> {
    fn name(&self) -> &str {
        "windows"
    }
    fn description(&self) -> &str {
        "Switch windows"
    }
    fn capabilities(&self) -> &[&str] {
        &["search"]
    }
    fn query_prefixes(&self) -> &[&str] {
        &["win"]
    }
    fn search(&self, _: &str) -> Vec<Action> {
        self.0.iter().map(|row| row.action.clone()).collect()
    }
}

impl RootWindow {
    pub(super) fn start_window_search(
        &mut self,
        query: String,
        other: Vec<(String, Action)>,
        scroll: bool,
        cx: &mut Context<Self>,
    ) {
        let names: Vec<_> = self
            .search_catalog
            .providers
            .plugins()
            .map(|plugin| plugin.name().to_owned())
            .collect();
        let position = names
            .iter()
            .position(|name| name == "windows")
            .unwrap_or(names.len());
        let insert_at = other
            .iter()
            .position(|(name, _)| {
                names
                    .iter()
                    .position(|candidate| candidate == name)
                    .is_some_and(|index| index > position)
            })
            .unwrap_or(other.len());
        self.publish_launcher_provider_results(query.clone(), other.clone(), scroll);
        let generation = self.launcher_results.generation;
        let epoch = self.window_search.epoch.load(Ordering::Acquire);
        let catalog = self
            .window_search
            .catalog
            .get_or_insert_with(|| {
                let catalog = Arc::new(WindowSwitchCatalog::new());
                let visibility = self.root_visibility.clone();
                catalog.set_update_callback(Arc::new(move || visibility.notify_window_search()));
                catalog
            })
            .clone();
        let plugins = self.enabled_plugins.clone();
        let caps = self.enabled_capabilities.clone();
        let task_query = query.clone();
        let task_epoch = self.window_search.epoch.clone();
        let task_visibility = self.root_visibility.clone();
        let task = cx.background_executor().spawn(async move {
            let (filtered, _) = multi_launcher::common::query::split_action_filters(&task_query);
            let rows = catalog
                .search_switches_if_current(&filtered, || {
                    task_epoch.load(Ordering::Acquire) == epoch
                        && task_visibility.active.load(Ordering::Acquire)
                        && task_visibility.visible.load(Ordering::Acquire)
                })
                .into_iter()
                .filter(|row| row.identity.native.pid != std::process::id())
                .collect::<Vec<_>>();
            let routed = super::search_providers::search_routed(
                [&SnapshotPlugin(&rows) as &dyn Plugin],
                &task_query,
                plugins.as_ref(),
                caps.as_ref(),
            );
            let rows = rows
                .into_iter()
                .filter(|row| routed.iter().any(|(_, action)| action == &row.action))
                .collect::<Vec<_>>();
            (rows, routed)
        });
        let owner = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let (rows, routed) = task.await;
            let _ = owner.update(cx, |root, cx| {
                if !publication_current(
                    epoch,
                    root.window_search.epoch.load(Ordering::Acquire),
                    generation,
                    root.launcher_results.generation,
                    &query,
                    &root.launcher_results.query,
                    root.query.read(cx).value().as_str(),
                    root.root_visibility.active.load(Ordering::Acquire)
                        && root.root_visibility.visible.load(Ordering::Acquire),
                    root.restart_admission.is_fenced(),
                    enabled(
                        root.enabled_plugins.as_ref(),
                        root.enabled_capabilities.as_ref(),
                    ),
                ) {
                    return;
                }
                let mut all = other;
                all.splice(insert_at..insert_at, routed);
                root.publish_launcher_provider_results(query, all, false);
                root.window_search.generation = root.launcher_results.generation;
                root.window_search.rows = root.launcher_results.actions.iter().enumerate()
                    .filter_map(|(index, result)| {
                        if !matches!(&result.origin, LauncherResultOrigin::Provider(name) if name == "windows") { return None; }
                        rows.iter().find(|row| row.action == result.action).cloned().map(|row| (index, row))
                    }).collect();
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn activate_window_search_result(
        &mut self,
        result: &LauncherSearchResult,
        index: usize,
        query: String,
        source: ActivationSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !enabled(
            self.enabled_plugins.as_ref(),
            self.enabled_capabilities.as_ref(),
        ) || self.window_search.generation != self.launcher_results.generation
        {
            return;
        }
        let Some(row) = self
            .window_search
            .rows
            .get(&index)
            .filter(|row| row.action == result.action)
            .cloned()
        else {
            return;
        };
        #[cfg(windows)]
        let Some(native) = self.native_window else {
            return;
        };
        #[cfg(not(windows))]
        {
            let _ = (row, query, source, window, cx);
            return;
        }
        #[cfg(windows)]
        {
            let initiating = multi_launcher::window_catalog::WindowNativeIdentity {
                hwnd: native.identity.hwnd,
                pid: native.identity.process_id,
                tid: native.identity.thread_id,
            };
            if current_foreground_identity() != Some(native.identity) {
                return;
            }
            let epoch = self.window_search.epoch.load(Ordering::Acquire);
            let epoch_ref = self.window_search.epoch.clone();
            let visibility = self.root_visibility.clone();
            let revision = visibility.request_revision.load(Ordering::Acquire);
            let checked = multi_launcher::window_activation::CheckedWindowSwitch {
                initiating,
                selected: row.identity,
                is_current: Arc::new(move || {
                    epoch_ref.load(Ordering::Acquire) == epoch
                        && visibility.active.load(Ordering::Acquire)
                        && visibility.visible.load(Ordering::Acquire)
                        && visibility.request_revision.load(Ordering::Acquire) == revision
                }),
            };
            self.start_window_switch(row.action, checked, query, source, window, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{action_is_switch, enabled, publication_current, query_is_windows};
    use multi_launcher::actions::Action;
    use std::collections::{HashMap, HashSet};

    #[test]
    fn deferred_catalog_wakes_coalesce_until_receipt() {
        let mut owner = super::Owner::default();
        assert!(!owner.take_deferred_update());
        owner.defer_update();
        owner.defer_update();
        assert!(owner.take_deferred_update());
        assert!(!owner.take_deferred_update());
    }

    #[test]
    fn windows_query_gate_and_publication_require_current_identity() {
        assert!(query_is_windows("kind:windows WIN Editor"));
        assert!(!query_is_windows("window Editor"));
        assert!(!query_is_windows("= 3+4"));
        let plugins = HashSet::from(["windows".to_owned()]);
        let disabled = HashMap::from([("windows".to_owned(), Vec::new())]);
        assert_eq!(enabled(Some(&plugins), None), cfg!(windows));
        assert!(!enabled(Some(&plugins), Some(&disabled)));
        assert!(!enabled(Some(&HashSet::new()), None));
        assert!(publication_current(
            1, 1, 3, 3, "win x", "win x", "win x", true, false, true
        ));
        assert!(!publication_current(
            1, 2, 3, 3, "win x", "win x", "win x", true, false, true
        ));
        assert!(!publication_current(
            1, 1, 3, 4, "win x", "win x", "win x", true, false, true
        ));
        assert!(!publication_current(
            1, 1, 3, 3, "win x", "win x", "win y", true, false, true
        ));
        assert!(!publication_current(
            1, 1, 3, 3, "win x", "win x", "win x", true, false, false
        ));
        let action = Action {
            label: "Switch to Editor".into(),
            desc: "Windows".into(),
            action: "window:switch:42".into(),
            args: None,
        };
        assert!(action_is_switch("windows", &action));
        assert!(!action_is_switch("other", &action));
        let mut close = action;
        close.action = "window:close:42".into();
        assert!(!action_is_switch("windows", &close));
    }
}
