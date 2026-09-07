use crate::common::persistence::{LoadState, PersistenceError, load_json, save_json_atomic};
use crate::dashboard::widgets::{WidgetRegistry, merge_json};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

static DASHBOARD_CONFIG_TRANSACTION: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

fn default_version() -> u32 {
    1
}

fn default_rows() -> u8 {
    3
}

fn default_cols() -> u8 {
    3
}

fn default_span() -> u8 {
    1
}

fn default_overflow_mode() -> OverflowMode {
    OverflowMode::Scroll
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum OverflowMode {
    #[default]
    Scroll,
    Clip,
    Auto,
}

impl OverflowMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            OverflowMode::Scroll => "scroll",
            OverflowMode::Clip => "clip",
            OverflowMode::Auto => "auto",
        }
    }
}

/// Grid definition for the dashboard layout.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GridConfig {
    #[serde(default = "default_rows")]
    pub rows: u8,
    #[serde(default = "default_cols")]
    pub cols: u8,
}

impl Default for GridConfig {
    fn default() -> Self {
        Self {
            rows: default_rows(),
            cols: default_cols(),
        }
    }
}

/// Widget slot configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SlotConfig {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub widget: String,
    pub row: i32,
    pub col: i32,
    #[serde(default = "default_span")]
    pub row_span: u8,
    #[serde(default = "default_span")]
    pub col_span: u8,
    #[serde(default)]
    pub settings: serde_json::Value,
    #[serde(default = "default_overflow_mode")]
    pub overflow: OverflowMode,
}

impl SlotConfig {
    pub fn with_widget(widget: &str, row: i32, col: i32) -> Self {
        Self {
            id: None,
            widget: widget.to_string(),
            row,
            col,
            row_span: default_span(),
            col_span: default_span(),
            settings: serde_json::Value::Object(Default::default()),
            overflow: default_overflow_mode(),
        }
    }
}

/// Primary dashboard configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DashboardConfig {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub grid: GridConfig,
    #[serde(default)]
    pub slots: Vec<SlotConfig>,
}

impl Default for DashboardConfig {
    fn default() -> Self {
        Self {
            version: default_version(),
            grid: GridConfig::default(),
            slots: vec![
                SlotConfig::with_widget("weather_site", 0, 0),
                SlotConfig::with_widget("pinned_commands", 0, 1),
                SlotConfig::with_widget("todo", 0, 2),
                SlotConfig::with_widget("recent_commands", 1, 0),
                SlotConfig::with_widget("frequent_commands", 1, 1),
                SlotConfig::with_widget("recent_notes", 1, 2),
                SlotConfig::with_widget("timers", 2, 0),
                SlotConfig::with_widget("clipboard_snippets", 2, 1),
            ],
        }
    }
}

impl DashboardConfig {
    /// Load a configuration from disk. Unknown widget types or invalid slots are
    /// filtered out using the provided registry.
    pub fn load(path: impl AsRef<Path>, registry: &WidgetRegistry) -> anyhow::Result<Self> {
        let _transaction = dashboard_config_transaction_guard();
        let (cfg, warnings) = Self::load_plan(path.as_ref(), registry)?;
        for w in warnings {
            tracing::warn!("{w}");
        }
        Ok(cfg)
    }

    pub fn load_typed(path: impl AsRef<Path>) -> Result<LoadState<Self>, PersistenceError> {
        load_json(path)
    }

    /// Save the configuration to disk.
    pub fn save(&self, path: impl AsRef<Path>) -> anyhow::Result<()> {
        let path = path.as_ref();
        let _transaction = dashboard_config_transaction_guard();
        // A full replacement must not silently destroy a malformed or unreadable
        // document. Missing and empty files remain legitimate first-save states.
        let _ = Self::load_typed(path)?;
        save_json_atomic(path, self)?;
        Ok(())
    }

    pub fn replace(
        path: impl AsRef<Path>,
        registry: &WidgetRegistry,
        replacement: Self,
    ) -> anyhow::Result<Self> {
        Self::update(path, registry, move |config| {
            *config = replacement;
            Ok(true)
        })
    }

    pub fn update(
        path: impl AsRef<Path>,
        registry: &WidgetRegistry,
        mutate: impl FnOnce(&mut Self) -> anyhow::Result<bool>,
    ) -> anyhow::Result<Self> {
        Self::update_with_save(path.as_ref(), registry, mutate, |path, config| {
            save_json_atomic(path, config).map_err(Into::into)
        })
    }

    fn update_with_save(
        path: &Path,
        registry: &WidgetRegistry,
        mutate: impl FnOnce(&mut Self) -> anyhow::Result<bool>,
        save: impl FnOnce(&Path, &Self) -> anyhow::Result<()>,
    ) -> anyhow::Result<Self> {
        let _transaction = dashboard_config_transaction_guard();
        let (mut config, existing_warnings) = Self::load_plan(path, registry)?;
        let changed = mutate(&mut config)?;
        let candidate_warnings = config.sanitize(registry);
        if changed || !existing_warnings.is_empty() || !candidate_warnings.is_empty() {
            save(path, &config)?;
        }
        Ok(config)
    }

    fn load_plan(path: &Path, registry: &WidgetRegistry) -> anyhow::Result<(Self, Vec<String>)> {
        let mut config = match Self::load_typed(path)? {
            LoadState::Missing | LoadState::Empty => return Ok((Self::default(), Vec::new())),
            LoadState::Loaded(config) => config,
        };
        let warnings = config.sanitize(registry);
        Ok((config, warnings))
    }

    /// Remove unsupported widgets and normalize empty settings.
    pub fn sanitize(&mut self, registry: &WidgetRegistry) -> Vec<String> {
        let mut warnings = Vec::new();
        if self.version == 0 {
            self.version = default_version();
            warnings.push("dashboard config version migrated from 0 to 1".to_string());
        }
        self.migrate_active_timers_widgets(registry, &mut warnings);
        self.migrate_todo_widgets(registry, &mut warnings);
        self.slots.retain(|slot| {
            if slot.widget.is_empty() {
                return false;
            }
            if !registry.contains(&slot.widget) {
                let msg = format!("unknown dashboard widget '{}' dropped", slot.widget);
                tracing::warn!(widget = %slot.widget, "unknown dashboard widget dropped");
                warnings.push(msg);
                return false;
            }
            true
        });
        for slot in &mut self.slots {
            if slot.settings.is_null() {
                slot.settings = registry
                    .default_settings(&slot.widget)
                    .unwrap_or_else(|| json!({}));
            }
        }
        warnings
    }

    pub fn path_for(base: &str) -> PathBuf {
        let base = Path::new(base);
        if base.is_dir() {
            base.join("dashboard.json")
        } else {
            PathBuf::from(base)
        }
    }

    fn migrate_todo_widgets(&mut self, registry: &WidgetRegistry, warnings: &mut Vec<String>) {
        for slot in &mut self.slots {
            let Some(default_settings) = registry.default_settings("todo") else {
                continue;
            };
            match slot.widget.as_str() {
                "todo" | "todo_list" | "todo_summary" | "todo_burndown" => {
                    let legacy_name = slot.widget.clone();
                    slot.widget = "todo".into();
                    slot.settings = merge_json(&default_settings, &slot.settings);
                    warnings.push(format!(
                        "dashboard widget '{}' migrated to 'todo'",
                        legacy_name
                    ));
                }
                _ => {}
            }
        }
    }

    fn migrate_active_timers_widgets(
        &mut self,
        registry: &WidgetRegistry,
        warnings: &mut Vec<String>,
    ) {
        for slot in &mut self.slots {
            let Some(default_settings) = registry.default_settings("timers") else {
                continue;
            };
            if slot.widget == "active_timers" {
                slot.widget = "timers".into();
                slot.settings = merge_json(&default_settings, &slot.settings);
                warnings.push("dashboard widget 'active_timers' migrated to 'timers'".to_string());
            }
        }
    }
}

fn dashboard_config_transaction_guard() -> MutexGuard<'static, ()> {
    DASHBOARD_CONFIG_TRANSACTION
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};

    fn registry() -> WidgetRegistry {
        WidgetRegistry::with_defaults()
    }

    #[test]
    fn typed_load_distinguishes_missing_empty_valid_malformed_and_unreadable() {
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("missing.json");
        assert!(matches!(
            DashboardConfig::load_typed(&missing).unwrap(),
            LoadState::Missing
        ));
        assert_eq!(
            DashboardConfig::load(&missing, &registry()).unwrap(),
            DashboardConfig::default()
        );

        let empty = directory.path().join("empty.json");
        std::fs::write(&empty, " \r\n\t").unwrap();
        assert!(matches!(
            DashboardConfig::load_typed(&empty).unwrap(),
            LoadState::Empty
        ));
        assert_eq!(
            DashboardConfig::load(&empty, &registry()).unwrap(),
            DashboardConfig::default()
        );

        let valid = directory.path().join("valid.json");
        save_json_atomic(&valid, &DashboardConfig::default()).unwrap();
        assert!(matches!(
            DashboardConfig::load_typed(&valid).unwrap(),
            LoadState::Loaded(_)
        ));

        let malformed = directory.path().join("malformed.json");
        std::fs::write(&malformed, b"{broken").unwrap();
        assert!(matches!(
            DashboardConfig::load_typed(&malformed).unwrap_err(),
            PersistenceError::MalformedJson { .. }
        ));
        assert!(matches!(
            DashboardConfig::load_typed(directory.path()).unwrap_err(),
            PersistenceError::Read { .. }
        ));
        assert!(
            DashboardConfig::replace(directory.path(), &registry(), DashboardConfig::default())
                .is_err()
        );
        assert!(directory.path().is_dir());
    }

    #[test]
    fn replacement_rejects_corruption_and_failed_save_retains_original() {
        let directory = tempfile::tempdir().unwrap();
        let malformed = directory.path().join("dashboard.json");
        let original = b"{broken";
        std::fs::write(&malformed, original).unwrap();
        assert!(
            DashboardConfig::replace(&malformed, &registry(), DashboardConfig::default()).is_err()
        );
        assert_eq!(std::fs::read(&malformed).unwrap(), original);

        let valid = directory.path().join("valid.json");
        save_json_atomic(&valid, &DashboardConfig::default()).unwrap();
        let before = std::fs::read(&valid).unwrap();
        let result = DashboardConfig::update_with_save(
            &valid,
            &registry(),
            |config| {
                config.grid.rows = 9;
                Ok(true)
            },
            |_path, _config| anyhow::bail!("injected save failure"),
        );
        assert!(result.is_err());
        assert_eq!(std::fs::read(&valid).unwrap(), before);
    }

    #[test]
    fn update_persists_sanitization_and_concurrent_changes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("nested").join("dashboard.json");
        let legacy = DashboardConfig {
            version: 0,
            grid: GridConfig::default(),
            slots: vec![SlotConfig::with_widget("active_timers", 0, 0)],
        };
        save_json_atomic(&path, &legacy).unwrap();
        let migrated = DashboardConfig::update(&path, &registry(), |_| Ok(false)).unwrap();
        assert_eq!(migrated.version, 1);
        assert_eq!(migrated.slots[0].widget, "timers");
        assert_eq!(
            DashboardConfig::load_typed(&path).unwrap(),
            LoadState::Loaded(migrated)
        );

        let path = Arc::new(directory.path().join("concurrent.json"));
        let barrier = Arc::new(Barrier::new(3));
        let mut threads = Vec::new();
        for id in ["left", "right"] {
            let path = Arc::clone(&path);
            let barrier = Arc::clone(&barrier);
            threads.push(std::thread::spawn(move || {
                barrier.wait();
                DashboardConfig::update(path.as_path(), &registry(), |config| {
                    let mut slot = SlotConfig::with_widget("weather_site", 0, 0);
                    slot.id = Some(id.to_string());
                    config.slots.push(slot);
                    Ok(true)
                })
            }));
        }
        barrier.wait();
        for thread in threads {
            thread.join().unwrap().unwrap();
        }
        let loaded = match DashboardConfig::load_typed(path.as_path()).unwrap() {
            LoadState::Loaded(config) => config,
            state => panic!("unexpected state: {state:?}"),
        };
        assert!(
            loaded
                .slots
                .iter()
                .any(|slot| slot.id.as_deref() == Some("left"))
        );
        assert!(
            loaded
                .slots
                .iter()
                .any(|slot| slot.id.as_deref() == Some("right"))
        );
    }

    #[test]
    fn path_for_preserves_file_directory_and_external_paths() {
        let directory = tempfile::tempdir().unwrap();
        assert_eq!(
            DashboardConfig::path_for(directory.path().to_str().unwrap()),
            directory.path().join("dashboard.json")
        );
        let external = directory.path().join("outside-config.json");
        assert_eq!(
            DashboardConfig::path_for(external.to_str().unwrap()),
            external
        );
    }
}
