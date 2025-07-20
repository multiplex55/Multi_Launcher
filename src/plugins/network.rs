use crate::actions::Action;
use crate::plugin::Plugin;
use crate::settings::NetUnit;
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::Instant;
use sysinfo::Networks;

const AVG_WINDOW: f64 = 10.0;

fn fmt_speed(bytes_per_sec: f64, unit: NetUnit) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    match unit {
        NetUnit::Auto => {
            if bytes_per_sec >= MB {
                format!("{:.2} MB/s", bytes_per_sec / MB)
            } else if bytes_per_sec >= KB {
                format!("{:.1} kB/s", bytes_per_sec / KB)
            } else {
                format!("{:.0} B/s", bytes_per_sec)
            }
        }
        NetUnit::B => format!("{:.0} B/s", bytes_per_sec),
        NetUnit::Kb => format!("{:.1} kB/s", bytes_per_sec / KB),
        NetUnit::Mb => format!("{:.2} MB/s", bytes_per_sec / MB),
    }
}

/// Display network usage per interface using the `net` prefix.
pub struct NetworkPlugin {
    state: Mutex<(Networks, Instant, HashMap<String, VecDeque<(Instant, u64, u64, f64)>>)>,
    unit: NetUnit,
}

impl NetworkPlugin {
    pub fn new(unit: NetUnit) -> Self {
        let mut nets = Networks::new_with_refreshed_list();
        nets.refresh(true);
        Self {
            state: Mutex::new((nets, Instant::now(), HashMap::new())),
            unit,
        }
    }
}

impl Default for NetworkPlugin {
    fn default() -> Self {
        Self::new(NetUnit::Auto)
    }
}

impl Plugin for NetworkPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        const PREFIX: &str = "net";
        let rest = match crate::common::strip_prefix_ci(query, PREFIX) {
            Some(r) => r,
            None => return Vec::new(),
        };
        if !rest.trim().is_empty() {
            return Vec::new();
        }
        let mut guard = match self.state.lock() {
            Ok(g) => g,
            Err(_) => return Vec::new(),
        };
        let (nets, last, history) = &mut *guard;
        let now = Instant::now();
        nets.refresh(true);
        let dt = now.duration_since(*last).as_secs_f64().max(0.001);
        *last = now;
        nets.iter()
            .map(|(name, data)| {
                let rx_bytes = data.received();
                let tx_bytes = data.transmitted();
                let rx = rx_bytes as f64 / dt;
                let tx = tx_bytes as f64 / dt;

                let hist = history.entry(name.to_string()).or_default();
                hist.push_back((now, rx_bytes, tx_bytes, dt));
                while let Some((t, _, _, _)) = hist.front() {
                    if now.duration_since(*t).as_secs_f64() > AVG_WINDOW {
                        hist.pop_front();
                    } else {
                        break;
                    }
                }
                let total_dt: f64 = hist.iter().map(|(_, _, _, d)| *d).sum::<f64>().max(dt);
                let total_rx: u64 = hist.iter().map(|(_, r, _, _)| *r).sum();
                let total_tx: u64 = hist.iter().map(|(_, _, t, _)| *t).sum();
                let avg_rx = total_rx as f64 / total_dt;
                let avg_tx = total_tx as f64 / total_dt;
                Action {
                    label: format!(
                        "{name} Rx {} Tx {} (10s avg Rx {} Tx {})",
                        fmt_speed(rx, self.unit),
                        fmt_speed(tx, self.unit),
                        fmt_speed(avg_rx, self.unit),
                        fmt_speed(avg_tx, self.unit)
                    ),
                    desc: "Network".into(),
                    action: format!("net:{name}"),
                    args: None,
                }
            })
            .collect()
    }

    fn name(&self) -> &str {
        "network"
    }

    fn description(&self) -> &str {
        "Show network usage per interface (prefix: `net`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![Action {
            label: "net".into(),
            desc: "Network".into(),
            action: "query:net".into(),
            args: None,
        }]
    }
}
