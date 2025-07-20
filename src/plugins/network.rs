use crate::actions::Action;
use crate::plugin::Plugin;
use sysinfo::Networks;

/// Display network usage per interface using the `net` prefix.
pub struct NetworkPlugin;

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
        let mut nets = Networks::new_with_refreshed_list();
        // refresh to get current values and generate diff
        nets.refresh(true);
        nets
            .iter()
            .map(|(name, data)| {
                let rx = data.total_received();
                let tx = data.total_transmitted();
                Action {
                    label: format!("{name} Rx {} B Tx {} B", rx, tx),
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
