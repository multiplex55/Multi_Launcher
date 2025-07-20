use crate::actions::Action;
use crate::plugin::Plugin;
use crate::settings::NetUnit;
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::Instant;
use sysinfo::Networks;

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
    state: Mutex<(
        Networks,
        Instant,
        VecDeque<(Instant, HashMap<String, (u64, u64)>)>,
    )>,
    unit: NetUnit,
}

impl NetworkPlugin {
    pub fn new(unit: NetUnit) -> Self {
        let mut nets = Networks::new_with_refreshed_list();
        nets.refresh(true);
        let now = Instant::now();
        let mut totals = HashMap::new();
        for (name, data) in nets.iter() {
            totals.insert(
                name.clone(),
                (data.total_received(), data.total_transmitted()),
            );
        }
        let mut history = VecDeque::new();
        history.push_back((now, totals));
        Self {
            state: Mutex::new((nets, now, history)),
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

        let mut totals = HashMap::new();
        for (name, data) in nets.iter() {
            totals.insert(
                name.clone(),
                (data.total_received(), data.total_transmitted()),
            );
        }
        history.push_back((now, totals));
        while let Some((t, _)) = history.front() {
            if now.duration_since(*t).as_secs_f64() > 10.0 {
                history.pop_front();
            } else {
                break;
            }
        }

        let (first_time, first_totals) = history.front().unwrap();
        let avg_dt = now.duration_since(*first_time).as_secs_f64().max(0.001);

        nets.iter()
            .map(|(name, data)| {
                let rx = data.received() as f64 / dt;
                let tx = data.transmitted() as f64 / dt;
                let (avg_rx, avg_tx) = match first_totals.get(name) {
                    Some((rx0, tx0)) => (
                        (data.total_received() - *rx0) as f64 / avg_dt,
                        (data.total_transmitted() - *tx0) as f64 / avg_dt,
                    ),
                    None => (
                        data.total_received() as f64 / avg_dt,
                        data.total_transmitted() as f64 / avg_dt,
                    ),
                };
                Action {
                    label: format!(
                        "{name} Rx {} Tx {} AvgRx {} AvgTx {}",
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
