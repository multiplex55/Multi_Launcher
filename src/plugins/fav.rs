use crate::actions::Action;
use crate::common::json_watch::{watch_json, JsonWatcher};
use crate::launcher::launch_action;
use crate::plugin::Plugin;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use serde::{Deserialize, Serialize};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};

pub const FAV_FILE: &str = "fav.json";

static FAV_VERSION: AtomicU64 = AtomicU64::new(0);

#[derive(Serialize, Deserialize, Clone)]
pub struct FavEntry {
    pub label: String,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<String>,
}

pub fn load_favs(path: &str) -> anyhow::Result<Vec<FavEntry>> {
    let content = std::fs::read_to_string(path).unwrap_or_default();
    if content.trim().is_empty() {
        return Ok(Vec::new());
    }
    let list: Vec<FavEntry> = serde_json::from_str(&content)?;
    Ok(list)
}

pub fn save_favs(path: &str, favs: &[FavEntry]) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(favs)?;
    std::fs::write(path, json)?;
    bump_fav_version();
    Ok(())
}

pub fn set_fav(path: &str, label: &str, action: &str, args: Option<&str>) -> anyhow::Result<()> {
    let mut list = load_favs(path).unwrap_or_default();
    if let Some(item) = list
        .iter_mut()
        .find(|e| e.label.eq_ignore_ascii_case(label))
    {
        item.action = action.to_string();
        item.args = args.map(|s| s.to_string());
    } else {
        list.push(FavEntry {
            label: label.to_string(),
            action: action.to_string(),
            args: args.map(|s| s.to_string()),
        });
    }
    save_favs(path, &list)
}

pub fn remove_fav(path: &str, label: &str) -> anyhow::Result<()> {
    let mut list = load_favs(path).unwrap_or_default();
    if let Some(pos) = list
        .iter()
        .position(|e| e.label.eq_ignore_ascii_case(label))
    {
        list.remove(pos);
        save_favs(path, &list)?;
    }
    Ok(())
}

pub fn fav_version() -> u64 {
    FAV_VERSION.load(Ordering::SeqCst)
}

fn bump_fav_version() {
    FAV_VERSION.fetch_add(1, Ordering::SeqCst);
}

/// Resolve a command and optional arguments against a plugin's search results.
///
/// The `command` and `args` are concatenated and passed to `plugin.search`.
/// If the plugin returns a result, its `action` and `args` are used; otherwise
/// the original `command` and `args` are returned unchanged.
pub fn resolve_with_plugin(
    plugin: &dyn Plugin,
    command: &str,
    args: Option<&str>,
) -> (String, Option<String>) {
    let query = join_command_args(command, args);
    if let Some(res) = plugin.search(&query).into_iter().next() {
        (res.action, res.args)
    } else {
        (command.to_string(), args.map(|s| s.to_string()))
    }
}

pub fn join_command_args(command: &str, args: Option<&str>) -> String {
    let command = command.trim_end();
    let Some(args) = args else {
        return command.to_string();
    };

    let args = args.trim_start();
    if args.is_empty() {
        command.to_string()
    } else {
        format!("{command} {args}")
    }
}

pub fn run_fav(label: &str) -> anyhow::Result<()> {
    let list = load_favs(FAV_FILE).unwrap_or_default();
    if let Some(entry) = list.iter().find(|e| e.label.eq_ignore_ascii_case(label)) {
        let act = Action {
            label: entry.label.clone(),
            desc: String::new(),
            action: entry.action.clone(),
            args: entry.args.clone(),
        };
        launch_action(&act)?;
    }
    Ok(())
}

pub struct FavPlugin {
    matcher: SkimMatcherV2,
    data: Arc<Mutex<Vec<FavEntry>>>,
    #[allow(dead_code)]
    watcher: Option<JsonWatcher>,
}

impl FavPlugin {
    pub fn new() -> Self {
        let data = Arc::new(Mutex::new(load_favs(FAV_FILE).unwrap_or_default()));
        let data_clone = data.clone();
        let path = FAV_FILE.to_string();
        let watch_path = path.clone();
        let watcher = watch_json(&watch_path, {
            let watch_path = watch_path.clone();
            move || {
                if let Ok(list) = load_favs(&watch_path) {
                    if let Ok(mut lock) = data_clone.lock() {
                        *lock = list;
                    }
                    bump_fav_version();
                }
            }
        })
        .ok();
        Self {
            matcher: SkimMatcherV2::default(),
            data,
            watcher,
        }
    }

    fn list(&self, filter: &str) -> Vec<Action> {
        let guard = match self.data.lock() {
            Ok(g) => g,
            Err(_) => return Vec::new(),
        };
        guard
            .iter()
            .filter(|f| self.matcher.fuzzy_match(&f.label, filter).is_some())
            .map(|f| Action {
                label: f.label.clone(),
                desc: "Fav".into(),
                action: f.action.clone(),
                args: f.args.clone(),
            })
            .collect()
    }
}

impl Default for FavPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for FavPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let trimmed = query.trim();
        if trimmed.eq_ignore_ascii_case("fav") {
            return vec![Action {
                label: "Favorites".into(),
                desc: "Fav".into(),
                action: "fav:dialog:".into(),
                args: None,
            }];
        }

        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "fav add") {
            let label = rest.trim();
            return vec![Action {
                label: if label.is_empty() {
                    "fav: add".into()
                } else {
                    format!("Add fav {label}")
                },
                desc: "Fav".into(),
                action: format!("fav:dialog:{label}"),
                args: None,
            }];
        }

        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "fav rm") {
            let filter = rest.trim();
            let guard = match self.data.lock() {
                Ok(g) => g,
                Err(_) => return Vec::new(),
            };
            return guard
                .iter()
                .filter(|f| self.matcher.fuzzy_match(&f.label, filter).is_some())
                .map(|f| Action {
                    label: format!("Remove fav {}", f.label),
                    desc: "Fav".into(),
                    action: format!("fav:remove:{}", f.label),
                    args: None,
                })
                .collect();
        }

        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "fav list") {
            return self.list(rest.trim());
        }

        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "fav ") {
            return self.list(rest.trim());
        }

        Vec::new()
    }

    fn name(&self) -> &str {
        "favorites"
    }

    fn description(&self) -> &str {
        "Run saved favorite commands (prefix: `fav`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![
            Action {
                label: "fav".into(),
                desc: "Fav".into(),
                action: "query:fav ".into(),
                args: None,
            },
            Action {
                label: "fav add".into(),
                desc: "Fav".into(),
                action: "query:fav add ".into(),
                args: None,
            },
            Action {
                label: "fav rm".into(),
                desc: "Fav".into(),
                action: "query:fav rm ".into(),
                args: None,
            },
            Action {
                label: "fav list".into(),
                desc: "Fav".into(),
                action: "query:fav list".into(),
                args: None,
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::{join_command_args, resolve_with_plugin};
    use crate::{actions::Action, plugin::Plugin};

    struct TestPlugin;

    impl Plugin for TestPlugin {
        fn search(&self, query: &str) -> Vec<Action> {
            vec![Action {
                label: query.to_string(),
                desc: String::new(),
                action: query.to_string(),
                args: None,
            }]
        }

        fn name(&self) -> &str {
            "test"
        }

        fn description(&self) -> &str {
            "test plugin"
        }

        fn capabilities(&self) -> &[&str] {
            &["search"]
        }

        fn commands(&self) -> Vec<Action> {
            Vec::new()
        }
    }

    #[test]
    fn join_command_and_args_for_tokenized_query() {
        assert_eq!(join_command_args("todo", Some("list")), "todo list");
    }

    #[test]
    fn join_keeps_command_when_args_empty() {
        assert_eq!(join_command_args("todo", None), "todo");
        assert_eq!(join_command_args("todo", Some("   ")), "todo");
    }

    #[test]
    fn join_normalizes_whitespace_between_command_and_args() {
        assert_eq!(
            join_command_args("todo   ", Some("   list now")),
            "todo list now"
        );
    }

    #[test]
    fn resolve_with_plugin_uses_safe_joined_query() {
        let plugin = TestPlugin;
        let (action, args) = resolve_with_plugin(&plugin, "todo  ", Some("  list"));
        assert_eq!(action, "todo list");
        assert!(args.is_none());
    }
}
