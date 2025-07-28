use crate::actions::Action;
use crate::plugin::Plugin;
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use std::thread;
use std::process::Command;

/// Default command to run when no `ML_SPEEDTEST_CMD` env var is set.
const DEFAULT_CMD: &str = "speedtest-rs --simple";

/// Parse the command spec string into executable and arguments.
fn parse_command_spec(spec: &str) -> (String, Vec<String>) {
    let parts = shlex::split(spec).unwrap_or_else(|| spec.split_whitespace().map(|s| s.to_string()).collect());
    let mut iter = parts.into_iter();
    let cmd = iter.next().unwrap_or_else(|| "speedtest-rs".to_string());
    (cmd, iter.collect())
}

pub struct SpeedTestPlugin {
    cmd: String,
    args: Vec<String>,
    result: Arc<Mutex<Option<String>>>,
    running: Arc<AtomicBool>,
}

impl Default for SpeedTestPlugin {
    fn default() -> Self {
        let spec = std::env::var("ML_SPEEDTEST_CMD").unwrap_or_else(|_| DEFAULT_CMD.to_string());
        let (cmd, args) = parse_command_spec(&spec);
        Self {
            cmd,
            args,
            result: Arc::new(Mutex::new(None)),
            running: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl SpeedTestPlugin {
    fn spawn_test(&self) {
        if self.running.swap(true, Ordering::SeqCst) {
            return;
        }
        let cmd = self.cmd.clone();
        let args = self.args.clone();
        let result = self.result.clone();
        let running = self.running.clone();
        thread::spawn(move || {
            let output = Command::new(cmd).args(args).output();
            let out = match output {
                Ok(o) => String::from_utf8_lossy(&o.stdout).into_owned(),
                Err(_) => String::new(),
            };
            let mut dl = None;
            let mut ul = None;
            for line in out.lines() {
                let l = line.trim();
                if let Some(rest) = l.strip_prefix("Download:") {
                    if let Some(val) = rest.trim().split_whitespace().next() {
                        dl = val.parse::<f32>().ok();
                    }
                } else if let Some(rest) = l.strip_prefix("Upload:") {
                    if let Some(val) = rest.trim().split_whitespace().next() {
                        ul = val.parse::<f32>().ok();
                    }
                }
            }
            let label = match (dl, ul) {
                (Some(d), Some(u)) => format!("Down {d:.2} Mbit/s Up {u:.2} Mbit/s"),
                (Some(d), None) => format!("Down {d:.2} Mbit/s"),
                _ => "Speed test failed".to_string(),
            };
            if let Ok(mut lock) = result.lock() {
                *lock = Some(label);
            }
            running.store(false, Ordering::SeqCst);
        });
    }
}

impl Plugin for SpeedTestPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let trimmed = query.trim();
        if crate::common::strip_prefix_ci(trimmed, "speed").is_none()
            && crate::common::strip_prefix_ci(trimmed, "speedtest").is_none()
        {
            return Vec::new();
        }
        if let Ok(lock) = self.result.lock() {
            if let Some(label) = lock.clone() {
                return vec![Action {
                    label,
                    desc: "SpeedTest".into(),
                    action: "speedtest".into(),
                    args: None,
                }];
            }
        }
        if !self.running.load(Ordering::SeqCst) {
            self.spawn_test();
        }
        vec![Action {
            label: "Running speed test...".into(),
            desc: "SpeedTest".into(),
            action: "speedtest".into(),
            args: None,
        }]
    }

    fn name(&self) -> &str {
        "speedtest"
    }

    fn description(&self) -> &str {
        "Run internet speed test (prefix: `speed`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![Action {
            label: "speed".into(),
            desc: "SpeedTest".into(),
            action: "query:speed".into(),
            args: None,
        }]
    }
}

