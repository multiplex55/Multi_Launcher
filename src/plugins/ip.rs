use crate::actions::Action;
use crate::plugin::Plugin;

pub struct IpPlugin;

impl Plugin for IpPlugin {
    #[cfg(target_os = "windows")]
    fn search(&self, query: &str) -> Vec<Action> {
        if crate::common::strip_prefix_ci(query.trim(), "ip").is_none() {
            return Vec::new();
        }
        let mut out = Vec::new();
        if let Ok(adapters) = ipconfig::get_adapters() {
            for adapter in adapters {
                let name = adapter.friendly_name();
                for ip in adapter.ip_addresses() {
                    out.push(Action {
                        label: format!("{name}: {ip}"),
                        desc: "IP".into(),
                        action: format!("clipboard:{ip}"),
                        args: None,
                    });
                }
            }
        }
        if let Ok(resp) = reqwest::blocking::get("https://api.ipify.org") {
            if let Ok(text) = resp.text() {
                let ip = text.trim();
                if !ip.is_empty() {
                    out.push(Action {
                        label: format!("Public: {ip}"),
                        desc: "IP".into(),
                        action: format!("clipboard:{ip}"),
                        args: None,
                    });
                }
            }
        }
        out
    }

    #[cfg(not(target_os = "windows"))]
    fn search(&self, _query: &str) -> Vec<Action> {
        Vec::new()
    }

    fn name(&self) -> &str {
        "ip"
    }

    fn description(&self) -> &str {
        "Show local and public IP addresses (prefix: `ip`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![Action {
            label: "ip".into(),
            desc: "IP".into(),
            action: "query:ip".into(),
            args: None,
        }]
    }
}
