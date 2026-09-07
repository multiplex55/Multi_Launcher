use crate::actions::Action;
use crate::common::persistence::{LoadState, PersistenceError, load_json, save_json_atomic};
use crate::plugin::Plugin;
use eframe::egui;
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::{
    Mutex, MutexGuard,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

pub const SHELL_CMDS_FILE: &str = "shell_cmds.json";

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct ShellCmdEntry {
    pub name: String,
    pub args: String,
    /// When false this command will not be suggested when typing `sh <query>`.
    #[serde(default = "default_autocomplete")]
    pub autocomplete: bool,
    #[serde(default)]
    pub keep_open: bool,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct ShellPluginSettings {
    pub open_in_wezterm: bool,
}

static USE_WEZTERM: AtomicBool = AtomicBool::new(false);
static SHELL_VERSION: AtomicU64 = AtomicU64::new(0);
static SHELL_TRANSACTION: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

pub fn shell_version() -> u64 {
    SHELL_VERSION.load(Ordering::Acquire)
}

pub fn use_wezterm() -> bool {
    USE_WEZTERM.load(Ordering::Relaxed)
}

fn default_autocomplete() -> bool {
    true
}

/// Load saved shell commands from `path`.
pub fn load_shell_cmds(path: &str) -> anyhow::Result<Vec<ShellCmdEntry>> {
    match load_shell_cmds_typed(path)? {
        LoadState::Missing | LoadState::Empty => Ok(Vec::new()),
        LoadState::Loaded(commands) => Ok(commands),
    }
}

pub fn load_shell_cmds_typed(
    path: impl AsRef<Path>,
) -> Result<LoadState<Vec<ShellCmdEntry>>, PersistenceError> {
    load_json(path)
}

/// Save the list of shell command entries to `path`.
pub fn save_shell_cmds(path: &str, cmds: &[ShellCmdEntry]) -> anyhow::Result<()> {
    replace_shell_cmds(path, cmds.to_vec()).map(|_| ())
}

pub fn replace_shell_cmds(
    path: &str,
    replacement: Vec<ShellCmdEntry>,
) -> anyhow::Result<Vec<ShellCmdEntry>> {
    update_shell_cmds(path, move |commands| {
        *commands = replacement;
        Ok(true)
    })
}

pub fn update_shell_cmds(
    path: &str,
    mutate: impl FnOnce(&mut Vec<ShellCmdEntry>) -> anyhow::Result<bool>,
) -> anyhow::Result<Vec<ShellCmdEntry>> {
    update_shell_cmds_with_save(path, mutate, |path, commands| {
        save_json_atomic(path, commands).map_err(Into::into)
    })
}

fn update_shell_cmds_with_save(
    path: &str,
    mutate: impl FnOnce(&mut Vec<ShellCmdEntry>) -> anyhow::Result<bool>,
    save: impl FnOnce(&str, &[ShellCmdEntry]) -> anyhow::Result<()>,
) -> anyhow::Result<Vec<ShellCmdEntry>> {
    let _transaction = shell_transaction_guard();
    let mut commands = load_shell_cmds(path)?;
    if mutate(&mut commands)? {
        save(path, &commands)?;
        SHELL_VERSION.fetch_add(1, Ordering::Release);
    }
    Ok(commands)
}

fn shell_transaction_guard() -> MutexGuard<'static, ()> {
    SHELL_TRANSACTION
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Append a new saved command to `path` if the name is unique.
pub fn append_shell_cmd(path: &str, name: &str, args: &str) -> anyhow::Result<()> {
    let name = name.to_owned();
    let args = args.to_owned();
    update_shell_cmds(path, move |commands| {
        if commands.iter().any(|command| command.name == name) {
            return Ok(false);
        }
        commands.push(ShellCmdEntry {
            name,
            args,
            autocomplete: true,
            keep_open: false,
        });
        Ok(true)
    })?;
    Ok(())
}

/// Remove the command identified by `name` from `path`.
pub fn remove_shell_cmd(path: &str, name: &str) -> anyhow::Result<()> {
    let name = name.to_owned();
    update_shell_cmds(path, move |commands| {
        let Some(position) = commands.iter().position(|command| command.name == name) else {
            return Ok(false);
        };
        commands.remove(position);
        Ok(true)
    })?;
    Ok(())
}

pub struct ShellPlugin;

impl Plugin for ShellPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let trimmed = query.trim();
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "sh")
            && rest.is_empty()
        {
            return vec![Action {
                label: "sh: edit saved commands".into(),
                desc: "Shell".into(),
                action: "shell:dialog".into(),
                args: None,
            }];
        }

        const ADD_PREFIX: &str = "sh add ";
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, ADD_PREFIX) {
            let mut parts = rest.trim().splitn(2, ' ');
            let name = parts.next().unwrap_or("").trim();
            let args = parts.next().unwrap_or("").trim();
            if !name.is_empty() && !args.is_empty() {
                return vec![Action {
                    label: format!("Add shell command {name}"),
                    desc: "Shell".into(),
                    action: format!("shell:add:{name}|{args}"),
                    args: None,
                }];
            }
        }

        const RM_PREFIX: &str = "sh rm";
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, RM_PREFIX) {
            let filter = rest.trim();
            if let Ok(list) = load_shell_cmds(SHELL_CMDS_FILE) {
                let matcher = SkimMatcherV2::default();
                return list
                    .into_iter()
                    .filter(|c| {
                        filter.is_empty()
                            || matcher.fuzzy_match(&c.name, filter).is_some()
                            || matcher.fuzzy_match(&c.args, filter).is_some()
                    })
                    .map(|c| Action {
                        label: format!("Remove shell command {}", c.name),
                        desc: "Shell".into(),
                        action: format!("shell:remove:{}", c.name),
                        args: None,
                    })
                    .collect();
            }
        }

        const LIST_PREFIX: &str = "sh list";
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, LIST_PREFIX) {
            let filter = rest.trim();
            if let Ok(list) = load_shell_cmds(SHELL_CMDS_FILE) {
                let matcher = SkimMatcherV2::default();
                return list
                    .into_iter()
                    .filter(|c| {
                        matcher.fuzzy_match(&c.name, filter).is_some()
                            || matcher.fuzzy_match(&c.args, filter).is_some()
                    })
                    .map(|c| {
                        let prefix = if c.keep_open { "shell_keep:" } else { "shell:" };
                        Action {
                            label: c.name,
                            desc: "Shell".into(),
                            action: format!("{}{}", prefix, c.args),
                            args: None,
                        }
                    })
                    .collect();
            }
        }

        const CMD_PREFIX: &str = "sh ";
        if let Some(cmd) = crate::common::strip_prefix_ci(trimmed, CMD_PREFIX) {
            let arg = cmd.trim();
            if arg.is_empty() {
                return Vec::new();
            }
            if let Ok(list) = load_shell_cmds(SHELL_CMDS_FILE) {
                let matcher = SkimMatcherV2::default();
                let mut best: Option<(ShellCmdEntry, i64)> = None;
                for entry in list.into_iter().filter(|e| e.autocomplete) {
                    if let Some(score) = matcher.fuzzy_match(&entry.name, arg)
                        && best.as_ref().map(|(_, s)| score > *s).unwrap_or(true)
                    {
                        best = Some((entry, score));
                    }
                }
                if let Some((entry, _)) = best {
                    let prefix = if entry.keep_open {
                        "shell_keep:"
                    } else {
                        "shell:"
                    };
                    return vec![Action {
                        label: format!("Run {}", entry.name),
                        desc: "Shell".into(),
                        action: format!("{}{}", prefix, entry.args),
                        args: None,
                    }];
                }
            }
            return vec![Action {
                label: format!("Run `{}`", arg),
                desc: "Shell".into(),
                action: format!("shell:{}", arg),
                args: None,
            }];
        }
        Vec::new()
    }

    fn name(&self) -> &str {
        "shell"
    }

    fn description(&self) -> &str {
        "Run arbitrary shell commands (prefix: `sh`; type `sh` to edit presets)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![
            Action {
                label: "sh".into(),
                desc: "Shell".into(),
                action: "query:sh".into(),
                args: None,
            },
            Action {
                label: "sh add".into(),
                desc: "Shell".into(),
                action: "query:sh add ".into(),
                args: None,
            },
            Action {
                label: "sh rm".into(),
                desc: "Shell".into(),
                action: "query:sh rm ".into(),
                args: None,
            },
            Action {
                label: "sh list".into(),
                desc: "Shell".into(),
                action: "query:sh list".into(),
                args: None,
            },
        ]
    }

    fn default_settings(&self) -> Option<serde_json::Value> {
        serde_json::to_value(ShellPluginSettings::default()).ok()
    }

    fn apply_settings(&mut self, value: &serde_json::Value) {
        if let Ok(cfg) = serde_json::from_value::<ShellPluginSettings>(value.clone()) {
            USE_WEZTERM.store(cfg.open_in_wezterm, Ordering::Relaxed);
        }
    }

    fn settings_ui(&mut self, ui: &mut egui::Ui, value: &mut serde_json::Value) {
        let mut cfg: ShellPluginSettings =
            serde_json::from_value(value.clone()).unwrap_or_default();
        ui.checkbox(&mut cfg.open_in_wezterm, "Open commands in WezTerm");
        USE_WEZTERM.store(cfg.open_in_wezterm, Ordering::Relaxed);
        match serde_json::to_value(&cfg) {
            Ok(v) => *value = v,
            Err(e) => tracing::error!("failed to serialize shell settings: {e}"),
        }
    }
}

#[cfg(test)]
mod persistence_tests {
    use super::*;
    use std::sync::{Arc, Barrier};

    static TEST_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

    fn command(name: &str) -> ShellCmdEntry {
        ShellCmdEntry {
            name: name.into(),
            args: format!("echo {name}"),
            autocomplete: true,
            keep_open: false,
        }
    }

    #[test]
    fn typed_states_and_serde_defaults_remain_compatible() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("missing.json");
        assert_eq!(load_shell_cmds_typed(&missing).unwrap(), LoadState::Missing);
        let empty = directory.path().join("empty.json");
        std::fs::write(&empty, " \r\n\t").unwrap();
        assert_eq!(load_shell_cmds_typed(&empty).unwrap(), LoadState::Empty);
        append_shell_cmd(empty.to_str().unwrap(), "initialized", "echo initialized").unwrap();
        assert_eq!(load_shell_cmds(empty.to_str().unwrap()).unwrap().len(), 1);
        let legacy = directory.path().join("legacy.json");
        std::fs::write(&legacy, r#"[{"name":"old","args":"dir"}]"#).unwrap();
        let loaded = load_shell_cmds(legacy.to_str().unwrap()).unwrap();
        assert!(loaded[0].autocomplete);
        assert!(!loaded[0].keep_open);
        let saved = directory.path().join("nested").join("commands.json");
        save_shell_cmds(saved.to_str().unwrap(), &loaded).unwrap();
        assert_eq!(
            std::fs::read_to_string(saved).unwrap(),
            serde_json::to_string_pretty(&loaded).unwrap()
        );
        let malformed = directory.path().join("malformed.json");
        std::fs::write(&malformed, "{").unwrap();
        assert!(matches!(
            load_shell_cmds_typed(&malformed).unwrap_err(),
            PersistenceError::MalformedJson { .. }
        ));
        assert!(matches!(
            load_shell_cmds_typed(directory.path()).unwrap_err(),
            PersistenceError::Read { .. }
        ));
    }

    #[test]
    fn malformed_store_rejects_all_mutations_unchanged() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("commands.json");
        let invalid = b"not commands";
        std::fs::write(&path, invalid).unwrap();
        let path = path.to_str().unwrap();
        for result in [
            append_shell_cmd(path, "new", "echo new"),
            remove_shell_cmd(path, "old"),
            save_shell_cmds(path, &[command("replacement")]),
        ] {
            assert!(result.is_err());
            assert_eq!(std::fs::read(path).unwrap(), invalid);
        }
        assert!(append_shell_cmd(directory.path().to_str().unwrap(), "new", "echo").is_err());
        assert!(remove_shell_cmd(directory.path().to_str().unwrap(), "old").is_err());
        assert!(save_shell_cmds(directory.path().to_str().unwrap(), &[command("new")]).is_err());
    }

    #[test]
    fn concurrent_adds_both_survive() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = Arc::new(
            directory
                .path()
                .join("commands.json")
                .to_string_lossy()
                .into_owned(),
        );
        let barrier = Arc::new(Barrier::new(3));
        let handles = ["first", "second"].map(|name| {
            let path = Arc::clone(&path);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                append_shell_cmd(&path, name, &format!("echo {name}")).unwrap();
            })
        });
        barrier.wait();
        for handle in handles {
            handle.join().unwrap();
        }
        let commands = load_shell_cmds(&path).unwrap();
        assert!(commands.contains(&command("first")));
        assert!(commands.contains(&command("second")));
    }

    #[test]
    fn failed_save_retains_bytes_and_version() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("commands.json");
        let original = vec![command("saved")];
        std::fs::write(&path, serde_json::to_vec_pretty(&original).unwrap()).unwrap();
        let version = shell_version();
        let result = update_shell_cmds_with_save(
            path.to_str().unwrap(),
            |commands| {
                commands.push(command("lost"));
                Ok(true)
            },
            |_path, _commands| anyhow::bail!("deterministic save failure"),
        );
        assert!(result.is_err());
        assert_eq!(load_shell_cmds(path.to_str().unwrap()).unwrap(), original);
        assert_eq!(shell_version(), version);
    }
}
