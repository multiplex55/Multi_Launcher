use crate::actions::Action;
use crate::plugin::Plugin;
use sysinfo::{Disks, System};

/// Display basic system usage statistics using the `info` prefix.
pub struct SysInfoPlugin;

impl SysInfoPlugin {
    fn cpu_action(system: &System) -> Action {
        let usage = system.global_cpu_usage();
        Action {
            label: format!("CPU usage {:.0}%", usage),
            desc: "SysInfo".into(),
            action: "sysinfo:cpu".into(),
            args: None,
        }
    }

    fn mem_action(system: &System) -> Action {
        let total = system.total_memory();
        let used = system.used_memory();
        let percent = if total > 0 {
            used as f64 / total as f64 * 100.0
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

    fn disk_action() -> Action {
        let disks = Disks::new_with_refreshed_list();
        let mut total = 0u64;
        let mut avail = 0u64;
        for d in disks.list() {
            total += d.total_space();
            avail += d.available_space();
        }
        let used = total.saturating_sub(avail);
        let percent = if total > 0 {
            used as f64 / total as f64 * 100.0
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

impl Plugin for SysInfoPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        if !query.starts_with("info") {
            return Vec::new();
        }
        let trimmed = query.trim().to_lowercase();
        let mut system = System::new_all();
        system.refresh_cpu_usage();
        system.refresh_memory();
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        match parts.as_slice() {
            ["info"] => vec![
                Self::cpu_action(&system),
                Self::mem_action(&system),
                Self::disk_action(),
            ],
            ["info", "cpu"] => vec![Self::cpu_action(&system)],
            ["info", "mem"] => vec![Self::mem_action(&system)],
            ["info", "disk"] => vec![Self::disk_action()],
            ["info", "cpu", "list", n] => {
                if let Ok(count) = n.parse::<usize>() {
                    vec![Self::cpu_list_action(count)]
                } else {
                    Vec::new()
                }
            }
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
}
