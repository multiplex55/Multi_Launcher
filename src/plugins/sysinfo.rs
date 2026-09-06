use crate::actions::Action;
use crate::plugin::{Plugin, PluginSearchUpdates};
use crate::plugins::system_data::{SystemDataCache, SystemDataRuntime, SystemDataSnapshot};
use std::sync::Arc;

/// Display basic system usage statistics using the `info` prefix.
pub struct SysInfoPlugin {
    cache: SystemDataCache,
    _runtime: Option<SystemDataRuntime>,
}

impl SysInfoPlugin {
    pub(crate) fn new(cache: SystemDataCache) -> Self {
        Self {
            cache,
            _runtime: None,
        }
    }

    fn cpu_action(snapshot: &SystemDataSnapshot) -> Action {
        Action {
            label: format!("CPU usage {:.0}%", snapshot.cpu_usage),
            desc: "SysInfo".into(),
            action: "sysinfo:cpu".into(),
            args: None,
        }
    }

    fn mem_action(snapshot: &SystemDataSnapshot) -> Action {
        let percent = if snapshot.total_memory > 0 {
            snapshot.used_memory as f64 / snapshot.total_memory as f64 * 100.0
        } else {
            0.0
        };
        Action {
            label: format!("Memory usage {:.0}%", percent),
            desc: "SysInfo".into(),
            action: "sysinfo:mem".into(),
            args: None,
        }
    }

    fn disk_action(snapshot: &SystemDataSnapshot) -> Action {
        let used = snapshot.total_disk.saturating_sub(snapshot.available_disk);
        let percent = if snapshot.total_disk > 0 {
            used as f64 / snapshot.total_disk as f64 * 100.0
        } else {
            0.0
        };
        Action {
            label: format!("Disk usage {:.0}%", percent),
            desc: "SysInfo".into(),
            action: "sysinfo:disk".into(),
            args: None,
        }
    }

    fn cpu_list_action(count: usize) -> Action {
        Action {
            label: format!("Top {count} CPU processes"),
            desc: "SysInfo".into(),
            action: format!("sysinfo:cpu_list:{count}"),
            args: None,
        }
    }
}

impl Default for SysInfoPlugin {
    fn default() -> Self {
        let runtime = SystemDataRuntime::start(Arc::new(PluginSearchUpdates::default()));
        let cache = runtime.cache();
        Self {
            cache,
            _runtime: Some(runtime),
        }
    }
}

impl Plugin for SysInfoPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        if !query.starts_with("info") {
            return Vec::new();
        }
        let trimmed = query.trim().to_lowercase();
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if let ["info", "cpu", "list", count] = parts.as_slice() {
            return count
                .parse::<usize>()
                .ok()
                .map(Self::cpu_list_action)
                .into_iter()
                .collect();
        }
        let snapshot = self.cache.snapshot_and_refresh();
        match parts.as_slice() {
            ["info"] => vec![
                Self::cpu_action(&snapshot),
                Self::mem_action(&snapshot),
                Self::disk_action(&snapshot),
            ],
            ["info", "cpu"] => vec![Self::cpu_action(&snapshot)],
            ["info", "mem"] => vec![Self::mem_action(&snapshot)],
            ["info", "disk"] => vec![Self::disk_action(&snapshot)],
            _ => Vec::new(),
        }
    }

    fn name(&self) -> &str {
        "sysinfo"
    }

    fn description(&self) -> &str {
        "Show CPU, memory and disk usage (prefix: `info`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![
            Action {
                label: "info".into(),
                desc: "SysInfo".into(),
                action: "query:info".into(),
                args: None,
            },
            Action {
                label: "info cpu".into(),
                desc: "SysInfo".into(),
                action: "query:info cpu".into(),
                args: None,
            },
            Action {
                label: "info mem".into(),
                desc: "SysInfo".into(),
                action: "query:info mem".into(),
                args: None,
            },
            Action {
                label: "info disk".into(),
                desc: "SysInfo".into(),
                action: "query:info disk".into(),
                args: None,
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actions_materialize_from_cached_snapshot() {
        let snapshot = SystemDataSnapshot {
            cpu_usage: 12.4,
            total_memory: 100,
            used_memory: 25,
            total_disk: 200,
            available_disk: 50,
            ..SystemDataSnapshot::default()
        };
        assert_eq!(SysInfoPlugin::cpu_action(&snapshot).label, "CPU usage 12%");
        assert_eq!(
            SysInfoPlugin::mem_action(&snapshot).label,
            "Memory usage 25%"
        );
        assert_eq!(
            SysInfoPlugin::disk_action(&snapshot).label,
            "Disk usage 75%"
        );
        let plugin = SysInfoPlugin::new(SystemDataCache::from_snapshot(snapshot));
        let actions = plugin.search("info");
        assert_eq!(actions.len(), 3);
        assert_eq!(actions[0].label, "CPU usage 12%");
    }
}
