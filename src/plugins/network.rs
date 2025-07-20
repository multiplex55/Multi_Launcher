use crate::actions::Action;
use crate::plugin::Plugin;
use sysinfo::Networks;
use std::sync::Mutex;
use std::time::Instant;

/// Display network usage per interface using the `net` prefix.
pub struct NetworkPlugin {
    state: Mutex<(Networks, Instant)>,
}

impl Default for NetworkPlugin {
    fn default() -> Self {
        let mut nets = Networks::new_with_refreshed_list();
        nets.refresh(true);
        Self { state: Mutex::new((nets, Instant::now())) }
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
        let (nets, last) = &mut *guard;
        let now = Instant::now();
        nets.refresh(true);
        let dt = now.duration_since(*last).as_secs_f64().max(0.001);
        *last = now;
        nets
            .iter()
            .map(|(name, data)| {
                let rx = data.received() as f64 / dt / 1_048_576.0;
                let tx = data.transmitted() as f64 / dt / 1_048_576.0;
                Action {
                    label: format!("{name} Rx {:.1} MB/s Tx {:.1} MB/s", rx, tx),
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
        vec![Action { label: "net".into(), desc: "Network".into(), action: "query:net".into(), args: None }]
    }
}
