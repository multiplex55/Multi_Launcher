use crate::actions::Action;
use crate::plugin::Plugin;
use sysinfo::System;

pub struct VolumePlugin;

impl Plugin for VolumePlugin {
    #[cfg(target_os = "windows")]
    fn search(&self, query: &str) -> Vec<Action> {
        let trimmed = query.trim();
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "vol") {
            if rest.is_empty() {
                return vec![Action {
                    label: "vol: edit volume".into(),
                    desc: "Volume".into(),
                    action: "volume:dialog".into(),
                    args: None,
                }];
            }
        }
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "vol ") {
            let parts: Vec<&str> = rest.trim().split_whitespace().collect();
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
                    if let Ok(val) = level.parse::<u8>() {
                        if val <= 100 {
                            return vec![Action {
                                label: format!("Set volume to {val}%"),
                                desc: "Volume".into(),
                                action: format!("volume:set:{val}"),
                                args: None,
                            }];
                        }
                    }
                }
                ["pid", pid_str, level_str] => {
                    if let (Ok(pid), Ok(level)) = (pid_str.parse::<u32>(), level_str.parse::<u32>())
                    {
                        if level <= 100 {
                            return vec![Action {
                                label: format!("Set PID {pid} volume to {level}%"),
                                desc: "Volume".into(),
                                action: format!("volume:pid:{pid}:{level}"),
                                args: None,
                            }];
                        }
                    }
                }
                ["name", exe, level_str] => {
                    if let Ok(level) = level_str.parse::<u32>() {
                        if level <= 100 {
                            let system = System::new_all();
                            if let Some(proc) = system
                                .processes()
                                .values()
                                .find(|p| p.name().to_string_lossy().eq_ignore_ascii_case(exe))
                            {
                                let pid = proc.pid().as_u32();
                                return vec![Action {
                                    label: format!("Set {exe} volume to {level}%"),
                                    desc: format!("PID {pid}"),
                                    action: format!("volume:pid:{pid}:{level}"),
                                    args: None,
                                }];
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        Vec::new()
    }

    #[cfg(not(target_os = "windows"))]
    fn search(&self, _query: &str) -> Vec<Action> {
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
