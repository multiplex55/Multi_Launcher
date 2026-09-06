use crate::actions::Action;
use crate::plugin::{Plugin, PluginSearchUpdates};
use crate::plugins::system_data::{SystemDataCache, SystemDataRuntime};
use std::sync::Arc;

pub struct VolumePlugin {
    cache: SystemDataCache,
    _runtime: Option<SystemDataRuntime>,
}

impl VolumePlugin {
    pub(crate) fn new(cache: SystemDataCache) -> Self {
        Self {
            cache,
            _runtime: None,
        }
    }
}

impl Default for VolumePlugin {
    fn default() -> Self {
        let runtime = SystemDataRuntime::start(Arc::new(PluginSearchUpdates::default()));
        let cache = runtime.cache();
        Self {
            cache,
            _runtime: Some(runtime),
        }
    }
}

impl Plugin for VolumePlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let trimmed = query.trim();
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "vol")
            && rest.is_empty()
        {
            return vec![Action {
                label: "vol: edit volume".into(),
                desc: "Volume".into(),
                action: "volume:dialog".into(),
                args: None,
            }];
        }
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "vol ") {
            let parts: Vec<&str> = rest.split_whitespace().collect();
            match parts.as_slice() {
                ["ma"] => {
                    return vec![Action {
                        label: "Mute active window".into(),
                        desc: "Volume".into(),
                        action: "volume:mute_active".into(),
                        args: None,
                    }];
                }
                [level] => {
                    if let Ok(val) = level.parse::<u8>()
                        && val <= 100
                    {
                        return vec![Action {
                            label: format!("Set volume to {val}%"),
                            desc: "Volume".into(),
                            action: format!("volume:set:{val}"),
                            args: None,
                        }];
                    }
                }
                ["pid", pid_str, level_str] => {
                    if let (Ok(pid), Ok(level)) = (pid_str.parse::<u32>(), level_str.parse::<u32>())
                        && level <= 100
                    {
                        return vec![Action {
                            label: format!("Set PID {pid} volume to {level}%"),
                            desc: "Volume".into(),
                            action: format!("volume:pid:{pid}:{level}"),
                            args: None,
                        }];
                    }
                }
                ["name", exe, level_str] => {
                    if let Ok(level) = level_str.parse::<u32>()
                        && level <= 100
                    {
                        let snapshot = self.cache.snapshot_and_refresh();
                        if let Some(process) = snapshot
                            .processes
                            .iter()
                            .find(|process| process.name.eq_ignore_ascii_case(exe))
                        {
                            return vec![Action {
                                label: format!("Set {exe} volume to {level}%"),
                                desc: format!("PID {}", process.pid),
                                action: format!("volume:pid:{}:{level}", process.pid),
                                args: None,
                            }];
                        }
                    }
                }
                _ => {}
            }
        }
        Vec::new()
    }

    fn name(&self) -> &str {
        "volume"
    }

    fn description(&self) -> &str {
        "Change system or process volume and mute active window (prefix: `vol`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![
            Action {
                label: "vol".into(),
                desc: "Volume".into(),
                action: "query:vol ".into(),
                args: None,
            },
            Action {
                label: "vol ma".into(),
                desc: "Volume".into(),
                action: "query:vol ma".into(),
                args: None,
            },
        ]
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::system_data::{ProcessSnapshot, SystemDataSnapshot};

    #[test]
    fn name_lookup_uses_cached_process_snapshot() {
        let plugin = VolumePlugin::new(SystemDataCache::from_snapshot(SystemDataSnapshot {
            processes: Arc::new(vec![ProcessSnapshot {
                name: "sample.exe".into(),
                pid: 42,
            }]),
            ..SystemDataSnapshot::default()
        }));
        let actions = plugin.search("vol name SAMPLE.EXE 20");
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].action, "volume:pid:42:20");
    }
}
