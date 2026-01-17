use crate::actions::Action;
use crate::plugin::Plugin;
use crate::watchlist::{self, watchlist_path_string, WatchItemConfig, WATCHLIST_DATA};
use std::io::ErrorKind;

pub struct WatchlistPlugin;

impl WatchlistPlugin {
    fn collect_items() -> Vec<WatchItemConfig> {
        WATCHLIST_DATA
            .read()
            .map(|cfg| cfg.items.clone())
            .unwrap_or_default()
    }

    fn filter_items(items: Vec<WatchItemConfig>, filter: &str) -> Vec<WatchItemConfig> {
        let filter = filter.trim().to_lowercase();
        if filter.is_empty() {
            return items;
        }
        items
            .into_iter()
            .filter(|item| {
                let id_match = item.id.to_lowercase().contains(&filter);
                let label_match = item
                    .label
                    .as_ref()
                    .map(|label| label.to_lowercase().contains(&filter))
                    .unwrap_or(false);
                id_match || label_match
            })
            .collect()
    }

    fn item_label(item: &WatchItemConfig) -> String {
        item.label.clone().unwrap_or_else(|| item.id.clone())
    }

    fn list_actions(filter: &str) -> Vec<Action> {
        Self::filter_items(Self::collect_items(), filter)
            .into_iter()
            .filter_map(|item| {
                let path = item.path.as_ref()?;
                Some(Action {
                    label: item.id.clone(),
                    desc: format!("Open {}", Self::item_label(&item)),
                    action: path.clone(),
                    args: None,
                })
            })
            .collect()
    }

    fn open_actions(filter: &str) -> Vec<Action> {
        Self::filter_items(Self::collect_items(), filter)
            .into_iter()
            .filter_map(|item| {
                let path = item.path.as_ref()?;
                Some(Action {
                    label: format!("Open {}", Self::item_label(&item)),
                    desc: "Watchlist".into(),
                    action: path.clone(),
                    args: None,
                })
            })
            .collect()
    }

    fn watchlist_action(label: impl Into<String>, action: impl Into<String>) -> Action {
        Action {
            label: label.into(),
            desc: "Watchlist".into(),
            action: action.into(),
            args: None,
        }
    }

    fn watch_path_action() -> Action {
        let path = watchlist_path_string();
        Action {
            label: path.clone(),
            desc: "Watchlist path".into(),
            action: format!("clipboard:{path}"),
            args: None,
        }
    }

    fn watch_edit_action() -> Action {
        Self::watchlist_action("Edit watchlist", watchlist_path_string())
    }

    fn watch_init_actions(force: bool) -> Vec<Action> {
        let path = watchlist_path_string();
        if force {
            return vec![Self::watchlist_action(
                "Initialize watchlist (--force)",
                "watch:init:force",
            )];
        }
        match std::fs::read_to_string(&path) {
            Ok(content) => {
                if content.trim().is_empty() {
                    return vec![Self::watchlist_action(
                        "Watchlist is empty. Overwrite with watch init --force",
                        "watch:init:force",
                    )];
                }
                if let Err(err) = watchlist::load_watchlist(&path) {
                    return vec![Self::watchlist_action(
                        format!("Watchlist invalid ({err}). Overwrite with watch init --force"),
                        "watch:init:force",
                    )];
                }
                vec![Self::watchlist_action("Watchlist already exists", path)]
            }
            Err(err) if err.kind() == ErrorKind::NotFound => {
                vec![Self::watchlist_action("Initialize watchlist", "watch:init")]
            }
            Err(err) => vec![Self::watchlist_action(
                format!("Failed to read watchlist ({err}). Initialize watchlist"),
                "watch:init",
            )],
        }
    }

    fn watch_validate_actions() -> Vec<Action> {
        let path = watchlist_path_string();
        match watchlist::load_watchlist(&path) {
            Ok(_) => vec![Self::watchlist_action("Watchlist is valid (open config)", path)],
            Err(err) => vec![Self::watchlist_action(
                format!("Watchlist invalid: {err} (open config)"),
                path,
            )],
        }
    }
}

impl Plugin for WatchlistPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let trimmed = query.trim();
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "watch") {
            if rest.is_empty() {
                return vec![
                    Self::watchlist_action("Open watchlist", "query:watch list"),
                    Self::watchlist_action("Refresh watchlist", "watch:refresh"),
                    Self::watchlist_action("watch path", "query:watch path"),
                    Self::watchlist_action("watch init", "query:watch init"),
                    Self::watchlist_action("watch validate", "query:watch validate"),
                    Self::watch_edit_action(),
                ];
            }
        }
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "watch list") {
            return Self::list_actions(rest);
        }
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "watch refresh") {
            if rest.trim().is_empty() {
                return vec![Self::watchlist_action("Refresh watchlist", "watch:refresh")];
            }
        }
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "watch init") {
            let args = rest.trim();
            let force = args.eq_ignore_ascii_case("--force");
            if args.is_empty() || force {
                return Self::watch_init_actions(force);
            }
        }
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "watch validate") {
            if rest.trim().is_empty() {
                return Self::watch_validate_actions();
            }
        }
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "watch edit") {
            if rest.trim().is_empty() {
                return vec![Self::watch_edit_action()];
            }
        }
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "watch path") {
            if rest.trim().is_empty() {
                return vec![Self::watch_path_action()];
            }
        }
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "watch open") {
            return Self::open_actions(rest);
        }
        Vec::new()
    }

    fn name(&self) -> &str {
        "watchlist"
    }

    fn description(&self) -> &str {
        "Watchlist commands and shortcuts (prefix: `watch`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![
            Self::watchlist_action("watch", "query:watch "),
            Self::watchlist_action("watch list", "query:watch list"),
            Self::watchlist_action("watch open", "query:watch open "),
            Self::watchlist_action("watch refresh", "watch:refresh"),
            Self::watchlist_action("watch path", "query:watch path"),
            Self::watchlist_action("watch init", "query:watch init"),
            Self::watchlist_action("watch validate", "query:watch validate"),
            Self::watch_edit_action(),
        ]
    }
}
