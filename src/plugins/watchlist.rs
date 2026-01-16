use crate::actions::Action;
use crate::plugin::Plugin;
use crate::watchlist::{WatchItemConfig, WATCHLIST_DATA, WATCHLIST_FILE};

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
}

impl Plugin for WatchlistPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let trimmed = query.trim();
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "watch") {
            if rest.is_empty() {
                return vec![
                    Action {
                        label: "Open watchlist".into(),
                        desc: "Watchlist".into(),
                        action: "query:watch list".into(),
                        args: None,
                    },
                    Action {
                        label: "Refresh watchlist".into(),
                        desc: "Watchlist".into(),
                        action: "watch:refresh".into(),
                        args: None,
                    },
                    Action {
                        label: "Edit watchlist.json".into(),
                        desc: "Watchlist".into(),
                        action: WATCHLIST_FILE.into(),
                        args: None,
                    },
                ];
            }
        }
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "watch list") {
            return Self::list_actions(rest);
        }
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "watch refresh") {
            if rest.trim().is_empty() {
                return vec![Action {
                    label: "Refresh watchlist".into(),
                    desc: "Watchlist".into(),
                    action: "watch:refresh".into(),
                    args: None,
                }];
            }
        }
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "watch edit") {
            if rest.trim().is_empty() {
                return vec![Action {
                    label: "Edit watchlist.json".into(),
                    desc: "Watchlist".into(),
                    action: WATCHLIST_FILE.into(),
                    args: None,
                }];
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
            Action {
                label: "watch".into(),
                desc: "Watchlist".into(),
                action: "query:watch ".into(),
                args: None,
            },
            Action {
                label: "watch list".into(),
                desc: "List watchlist items".into(),
                action: "query:watch list".into(),
                args: None,
            },
            Action {
                label: "watch open".into(),
                desc: "Open a watchlist path".into(),
                action: "query:watch open ".into(),
                args: None,
            },
            Action {
                label: "watch refresh".into(),
                desc: "Refresh watchlist cache".into(),
                action: "watch:refresh".into(),
                args: None,
            },
            Action {
                label: "watch edit".into(),
                desc: "Edit watchlist.json".into(),
                action: WATCHLIST_FILE.into(),
                args: None,
            },
        ]
    }
}
