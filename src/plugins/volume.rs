use crate::actions::Action;
use crate::plugin::Plugin;

pub struct VolumePlugin;

impl Plugin for VolumePlugin {
    #[cfg(target_os = "windows")]
    fn search(&self, query: &str) -> Vec<Action> {
        let trimmed = query.trim();
        if trimmed.eq_ignore_ascii_case("vol") {
            return vec![Action {
                label: "vol: edit volume".into(),
                desc: "Volume".into(),
                action: "volume:dialog".into(),
                args: None,
            }];
        }
        if let Some(rest) = trimmed.strip_prefix("vol ") {
            let rest = rest.trim();
            if rest.eq_ignore_ascii_case("ma") {
                return vec![Action {
                    label: "Mute active window".into(),
                    desc: "Volume".into(),
                    action: "volume:mute_active".into(),
                    args: None,
                }];
            }
            if let Ok(val) = rest.parse::<u8>() {
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
        "Change system volume or mute active window (prefix: `vol`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }
}
