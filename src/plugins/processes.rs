use crate::actions::Action;
use crate::plugin::{Plugin, PluginSearchUpdates};
use crate::plugins::system_data::{ProcessSnapshot, SystemDataCache, SystemDataRuntime};
use std::sync::Arc;

pub struct ProcessesPlugin {
    cache: SystemDataCache,
    _runtime: Option<SystemDataRuntime>,
}

impl ProcessesPlugin {
    pub(crate) fn new(cache: SystemDataCache) -> Self {
        Self {
            cache,
            _runtime: None,
        }
    }
}

impl Default for ProcessesPlugin {
    fn default() -> Self {
        let runtime = SystemDataRuntime::start(Arc::new(PluginSearchUpdates::default()));
        let cache = runtime.cache();
        Self {
            cache,
            _runtime: Some(runtime),
        }
    }
}

enum Mode {
    Both,
    Kill,
    Switch,
}

fn parse_query(query: &str) -> Option<(Mode, String)> {
    let (mode, rest) = if let Some(rest) = crate::common::strip_prefix_ci(query, "psk") {
        (Mode::Kill, rest)
    } else if let Some(rest) = crate::common::strip_prefix_ci(query, "pss") {
        (Mode::Switch, rest)
    } else if let Some(rest) = crate::common::strip_prefix_ci(query, "ps") {
        (Mode::Both, rest)
    } else {
        return None;
    };
    Some((mode, rest.trim().to_lowercase()))
}

fn actions_from_processes(
    mode: Mode,
    filter: &str,
    processes: impl IntoIterator<Item = (String, u32)>,
) -> Vec<Action> {
    processes
        .into_iter()
        .filter(|(name, _)| filter.is_empty() || name.to_lowercase().contains(filter))
        .flat_map(|(name, pid)| {
            let switch_action = Action {
                label: format!("Switch to {name}"),
                desc: format!("PID {pid}"),
                action: format!("process:switch:{pid}"),
                args: None,
            };
            let kill_action = Action {
                label: format!("Kill {name}"),
                desc: format!("PID {pid}"),
                action: format!("process:kill:{pid}"),
                args: None,
            };
            match mode {
                Mode::Both => vec![switch_action, kill_action],
                Mode::Kill => vec![kill_action],
                Mode::Switch => vec![switch_action],
            }
        })
        .collect()
}

pub(crate) fn actions_from_snapshot(query: &str, processes: &[ProcessSnapshot]) -> Vec<Action> {
    let Some((mode, filter)) = parse_query(query) else {
        return Vec::new();
    };
    actions_from_processes(
        mode,
        &filter,
        processes
            .iter()
            .map(|process| (process.name.clone(), process.pid)),
    )
}

impl Plugin for ProcessesPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let Some((mode, filter)) = parse_query(query) else {
            return Vec::new();
        };
        let Some(snapshot) = self.cache.snapshot_and_refresh() else {
            return Vec::new();
        };
        actions_from_processes(
            mode,
            &filter,
            snapshot
                .processes
                .iter()
                .map(|ProcessSnapshot { name, pid }| (name.clone(), *pid)),
        )
    }

    fn name(&self) -> &str {
        "processes"
    }

    fn description(&self) -> &str {
        "Enumerate running processes (prefixes: `ps`, `psk`, `pss`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![
            Action {
                label: "ps".into(),
                desc: "Processes".into(),
                action: "query:ps ".into(),
                args: None,
            },
            Action {
                label: "psk".into(),
                desc: "Kill process".into(),
                action: "query:psk ".into(),
                args: None,
            },
            Action {
                label: "pss".into(),
                desc: "Switch process".into(),
                action: "query:pss ".into(),
                args: None,
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::system_data::SystemDataSnapshot;

    fn plugin() -> ProcessesPlugin {
        ProcessesPlugin::new(SystemDataCache::from_snapshot(SystemDataSnapshot {
            processes: Arc::new(vec![
                ProcessSnapshot {
                    name: "alpha.exe".into(),
                    pid: 7,
                },
                ProcessSnapshot {
                    name: "beta.exe".into(),
                    pid: 9,
                },
            ]),
            ..SystemDataSnapshot::default()
        }))
    }

    #[test]
    fn cached_process_results_preserve_modes_and_filtering() {
        let plugin = plugin();
        let both = plugin.search("ps alpha");
        assert_eq!(both.len(), 2);
        assert!(both.iter().all(|action| action.desc == "PID 7"));

        let kill = plugin.search("psk beta");
        assert_eq!(kill.len(), 1);
        assert_eq!(kill[0].action, "process:kill:9");

        let switch = plugin.search("pss beta");
        assert_eq!(switch.len(), 1);
        assert_eq!(switch[0].action, "process:switch:9");
    }
}
