use crate::actions::Action;
use crate::plugin::Plugin;
use crate::watchlist::{
    self, parse_move_direction, parse_watch_add_input, preview_watch_add_item,
    watchlist_path_string, watchlist_snapshot, WatchItemConfig, WatchItemSnapshot, WatchStatus,
    WATCHLIST_DATA,
};
use chrono::Local;
use std::collections::HashMap;
use std::io::ErrorKind;

pub struct WatchlistPlugin;

impl WatchlistPlugin {
    fn collect_items() -> Vec<WatchItemConfig> {
        WATCHLIST_DATA
            .read()
            .map(|cfg| cfg.items.clone())
            .unwrap_or_default()
    }

    fn matches_watch_filter(item: &WatchItemConfig, filter: &str) -> bool {
        let id_match = item.id.to_lowercase().contains(filter);
        let label_match = item
            .label
            .as_ref()
            .map(|label| label.to_lowercase().contains(filter))
            .unwrap_or(false);
        id_match || label_match
    }

    fn filter_items(items: Vec<WatchItemConfig>, filter: &str) -> Vec<WatchItemConfig> {
        let filter = filter.trim().to_lowercase();
        if filter.is_empty() {
            return items;
        }
        items
            .into_iter()
            .filter(|item| Self::matches_watch_filter(item, &filter))
            .collect()
    }

    fn item_label(item: &WatchItemConfig) -> String {
        item.label.clone().unwrap_or_else(|| item.id.clone())
    }

    fn item_action_label(item: &WatchItemConfig) -> String {
        let label = item
            .label
            .as_ref()
            .map(|label| label.trim())
            .filter(|label| !label.is_empty());
        match label {
            Some(label) if !label.eq_ignore_ascii_case(&item.id) => {
                format!("{label} ({})", item.id)
            }
            _ => item.id.clone(),
        }
    }

    fn list_actions(filter: &str) -> Vec<Action> {
        let items = Self::collect_items();
        let snapshot = watchlist_snapshot();
        if let Some(actions) = Self::refresh_action_if_needed(&items, &snapshot) {
            return actions;
        }
        let snapshot_map = Self::snapshot_map(&snapshot);
        Self::filter_items(items, filter)
            .into_iter()
            .filter_map(|item| {
                let path = item.path.as_ref()?;
                let desc = snapshot_map
                    .get(item.id.as_str())
                    .map(|snapshot| Self::compact_desc(snapshot))
                    .unwrap_or_else(|| format!("Open {}", Self::item_label(&item)));
                Some(Action {
                    label: item.id.clone(),
                    desc,
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
            Ok(_) => vec![Self::watchlist_action(
                "Watchlist is valid (open config)",
                path,
            )],
            Err(err) => vec![Self::watchlist_action(
                format!("Watchlist invalid: {err} (open config)"),
                path,
            )],
        }
    }

    fn parse_add_action(input: &str) -> Vec<Action> {
        let Ok(req) = parse_watch_add_input(input) else {
            return Vec::new();
        };
        let preview = WATCHLIST_DATA
            .read()
            .ok()
            .and_then(|cfg| preview_watch_add_item(&cfg, &req).ok());
        let payload = match serde_json::to_string(&req) {
            Ok(payload) => payload,
            Err(_) => return Vec::new(),
        };
        let label = preview
            .map(|item| item.label.unwrap_or(item.id))
            .unwrap_or_else(|| "watch item".into());
        vec![Self::watchlist_action(
            format!("Add watch {label}"),
            format!("watch:add:{payload}"),
        )]
    }

    fn match_actions(filter: &str) -> Vec<WatchItemConfig> {
        Self::filter_items(Self::collect_items(), filter)
    }

    fn remove_actions(filter: &str) -> Vec<Action> {
        Self::match_actions(filter)
            .into_iter()
            .map(|item| {
                let label = Self::item_action_label(&item);
                Self::watchlist_action(
                    format!("Remove {label}"),
                    format!("watch:rm:{}", item.id),
                )
            })
            .collect()
    }

    fn enable_actions(filter: &str, enabled: bool) -> Vec<Action> {
        Self::match_actions(filter)
            .into_iter()
            .filter(|item| item.enabled != enabled)
            .map(|item| {
                let label = Self::item_action_label(&item);
                let verb = if enabled { "Enable" } else { "Disable" };
                Self::watchlist_action(
                    format!("{verb} {label}"),
                    format!(
                        "watch:{}:{}",
                        if enabled { "enable" } else { "disable" },
                        item.id
                    ),
                )
            })
            .collect()
    }

    fn set_refresh_actions(value: &str) -> Vec<Action> {
        let Ok(refresh_ms) = value.trim().parse::<u64>() else {
            return Vec::new();
        };
        vec![Self::watchlist_action(
            format!("Set watch refresh to {refresh_ms}ms"),
            format!("watch:set_refresh:{refresh_ms}"),
        )]
    }

    fn move_actions(id: &str, direction: &str) -> Vec<Action> {
        let Some(direction) = parse_move_direction(direction) else {
            return Vec::new();
        };
        let items = Self::collect_items();
        let Some(idx) = items
            .iter()
            .position(|item| item.id.eq_ignore_ascii_case(id))
        else {
            return Vec::new();
        };
        let can_move = match direction {
            watchlist::MoveDirection::Up => idx > 0,
            watchlist::MoveDirection::Down => idx + 1 < items.len(),
        };
        if !can_move {
            return Vec::new();
        }
        let verb = match direction {
            watchlist::MoveDirection::Up => "up",
            watchlist::MoveDirection::Down => "down",
        };
        vec![Self::watchlist_action(
            format!("Move {id} {verb}"),
            format!("watch:move:{id}|{verb}"),
        )]
    }

    fn refresh_action_if_needed(
        items: &[WatchItemConfig],
        snapshot: &std::sync::Arc<Vec<WatchItemSnapshot>>,
    ) -> Option<Vec<Action>> {
        if snapshot.is_empty() && !items.is_empty() {
            return Some(vec![Self::watchlist_action(
                "Refresh watchlist",
                "watch:refresh",
            )]);
        }
        None
    }

    fn snapshot_map<'a>(
        snapshot: &'a std::sync::Arc<Vec<WatchItemSnapshot>>,
    ) -> HashMap<&'a str, &'a WatchItemSnapshot> {
        snapshot
            .iter()
            .map(|item| (item.id.as_str(), item))
            .collect()
    }

    fn config_map<'a>(items: &'a [WatchItemConfig]) -> HashMap<&'a str, &'a WatchItemConfig> {
        items
            .iter()
            .map(|item| (item.id.as_str(), item))
            .collect()
    }

    fn matches_snapshot_filter(item: &WatchItemSnapshot, filter: &str) -> bool {
        let filter = filter.trim().to_lowercase();
        if filter.is_empty() {
            return true;
        }
        item.id.to_lowercase().contains(&filter) || item.label.to_lowercase().contains(&filter)
    }

    fn status_text(status: WatchStatus) -> &'static str {
        match status {
            WatchStatus::Ok => "OK",
            WatchStatus::Warn => "WARN",
            WatchStatus::Critical => "CRITICAL",
        }
    }

    fn age_text(snapshot: &WatchItemSnapshot) -> String {
        let now = Local::now();
        let age = now
            .signed_duration_since(snapshot.last_updated)
            .num_seconds()
            .max(0) as u64;
        format!("{} ago", Self::format_age_duration(age))
    }

    fn format_age_duration(secs: u64) -> String {
        if secs < 60 {
            format!("{secs}s")
        } else if secs < 3600 {
            format!("{}m", secs / 60)
        } else if secs < 86_400 {
            format!("{}h", secs / 3600)
        } else {
            format!("{}d", secs / 86_400)
        }
    }

    fn compact_label(snapshot: &WatchItemSnapshot) -> String {
        let label = snapshot.label.clone();
        if snapshot.last_updated.date_naive() == Local::now().date_naive() {
            format!("{label} (today)")
        } else {
            label
        }
    }

    fn compact_desc(snapshot: &WatchItemSnapshot) -> String {
        let delta = snapshot.delta_text.clone().unwrap_or_else(|| "—".into());
        format!(
            "latest: {} ({}) | {} | Δ {}",
            snapshot.value_text,
            Self::age_text(snapshot),
            Self::status_text(snapshot.status),
            delta
        )
    }

    fn snapshot_action(snapshot: &WatchItemSnapshot, path: &str) -> Action {
        Action {
            label: Self::compact_label(snapshot),
            desc: Self::compact_desc(snapshot),
            action: path.to_string(),
            args: None,
        }
    }

    fn ls_actions(filter: &str) -> Vec<Action> {
        let items = Self::collect_items();
        let snapshot = watchlist_snapshot();
        if let Some(actions) = Self::refresh_action_if_needed(&items, &snapshot) {
            return actions;
        }
        let config_map = Self::config_map(&items);
        let filter = filter.trim().to_lowercase();
        snapshot
            .iter()
            .filter(|item| Self::matches_snapshot_filter(item, &filter))
            .filter_map(|snapshot| {
                let item = config_map.get(snapshot.id.as_str())?;
                let path = item.path.as_ref()?;
                Some(Self::snapshot_action(snapshot, path))
            })
            .collect()
    }

    fn status_actions(level: Option<WatchStatus>, filter: &str) -> Vec<Action> {
        let items = Self::collect_items();
        let snapshot = watchlist_snapshot();
        if let Some(actions) = Self::refresh_action_if_needed(&items, &snapshot) {
            return actions;
        }
        let config_map = Self::config_map(&items);
        let filter = filter.trim().to_lowercase();
        snapshot
            .iter()
            .filter(|item| Self::matches_snapshot_filter(item, &filter))
            .filter(|item| match level {
                Some(WatchStatus::Warn) => item.status == WatchStatus::Warn,
                Some(WatchStatus::Critical) => item.status == WatchStatus::Critical,
                None => matches!(item.status, WatchStatus::Warn | WatchStatus::Critical),
                Some(WatchStatus::Ok) => false,
            })
            .filter_map(|snapshot| {
                let item = config_map.get(snapshot.id.as_str())?;
                let path = item.path.as_ref()?;
                Some(Self::snapshot_action(snapshot, path))
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
                    Self::watchlist_action("Open watchlist", "query:watch list"),
                    Self::watchlist_action("watch ls", "query:watch ls"),
                    Self::watchlist_action("watch status", "query:watch status"),
                    Self::watchlist_action("Refresh watchlist", "watch:refresh"),
                    Self::watchlist_action("watch path", "query:watch path"),
                    Self::watchlist_action("watch init", "query:watch init"),
                    Self::watchlist_action("watch validate", "query:watch validate"),
                    Self::watch_edit_action(),
                ];
            }
        }
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "watch ls") {
            return Self::ls_actions(rest);
        }
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "watch list") {
            return Self::list_actions(rest);
        }
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "watch status") {
            let mut parts = rest.trim().split_whitespace();
            let first = parts.next().unwrap_or("");
            let (level, filter) = if first.eq_ignore_ascii_case("warn") {
                (Some(WatchStatus::Warn), parts.collect::<Vec<_>>().join(" "))
            } else if first.eq_ignore_ascii_case("critical") {
                (
                    Some(WatchStatus::Critical),
                    parts.collect::<Vec<_>>().join(" "),
                )
            } else {
                (None, rest.to_string())
            };
            return Self::status_actions(level, &filter);
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
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "watch add") {
            let input = rest.trim();
            if input.is_empty() {
                return Vec::new();
            }
            return Self::parse_add_action(input);
        }
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "watch rm") {
            return Self::remove_actions(rest.trim());
        }
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "watch enable") {
            return Self::enable_actions(rest.trim(), true);
        }
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "watch disable") {
            return Self::enable_actions(rest.trim(), false);
        }
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "watch set refresh") {
            return Self::set_refresh_actions(rest.trim());
        }
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "watch mv") {
            let mut parts = rest.trim().split_whitespace();
            if let (Some(id), Some(direction)) = (parts.next(), parts.next()) {
                return Self::move_actions(id, direction);
            }
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
            Self::watchlist_action("watch ls", "query:watch ls"),
            Self::watchlist_action("watch status", "query:watch status"),
            Self::watchlist_action("watch open", "query:watch open "),
            Self::watchlist_action("watch refresh", "watch:refresh"),
            Self::watchlist_action("watch path", "query:watch path"),
            Self::watchlist_action("watch init", "query:watch init"),
            Self::watchlist_action("watch validate", "query:watch validate"),
            Self::watchlist_action("watch add", "query:watch add "),
            Self::watchlist_action("watch rm", "query:watch rm "),
            Self::watchlist_action("watch enable", "query:watch enable "),
            Self::watchlist_action("watch disable", "query:watch disable "),
            Self::watchlist_action("watch set refresh", "query:watch set refresh "),
            Self::watchlist_action("watch mv", "query:watch mv "),
            Self::watch_edit_action(),
        ]
    }
}
