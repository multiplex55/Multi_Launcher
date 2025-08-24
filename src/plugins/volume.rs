use crate::actions::Action;
use crate::plugin::Plugin;
use once_cell::sync::Lazy;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use sysinfo::{ProcessesToUpdate, System};

pub struct VolumePlugin;

impl Plugin for VolumePlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        static SYSTEM_CACHE: Lazy<Mutex<(System, Instant)>> =
            Lazy::new(|| Mutex::new((System::new_all(), Instant::now())));
        const CACHE_TIMEOUT: Duration = Duration::from_secs(5);
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
                            let pid_opt = {
                                let mut guard = SYSTEM_CACHE.lock().unwrap();
                                if guard.1.elapsed() > CACHE_TIMEOUT {
                                    guard.0.refresh_processes(ProcessesToUpdate::All, true);
                                    guard.1 = Instant::now();
                                }
                                guard
                                    .0
                                    .processes()
                                    .values()
                                    .find(|p| p.name().to_string_lossy().eq_ignore_ascii_case(exe))
                                    .map(|p| p.pid().as_u32())
                            };
                            if let Some(pid) = pid_opt {
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
