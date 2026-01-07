use crate::dashboard::widgets::{merge_json, WidgetRegistry};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::{Path, PathBuf};

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
pub enum OverflowMode {
    Scroll,
    Clip,
    Auto,
}

impl Default for OverflowMode {
    fn default() -> Self {
        Self::Scroll
    }
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
                SlotConfig::with_widget("todos", 0, 2),
                SlotConfig::with_widget("recent_commands", 1, 0),
                SlotConfig::with_widget("frequent_commands", 1, 1),
                SlotConfig::with_widget("recent_notes", 1, 2),
                SlotConfig::with_widget("active_timers", 2, 0),
                SlotConfig::with_widget("clipboard_snippets", 2, 1),
            ],
        }
    }
}

impl DashboardConfig {
    /// Load a configuration from disk. Unknown widget types or invalid slots are
    /// filtered out using the provided registry.
    pub fn load(path: impl AsRef<Path>, registry: &WidgetRegistry) -> anyhow::Result<Self> {
        let path = path.as_ref();
        let content = std::fs::read_to_string(path).unwrap_or_default();
        if content.trim().is_empty() {
            return Ok(Self::default());
        }
        let mut cfg: DashboardConfig = serde_json::from_str(&content)?;
        let warnings = cfg.sanitize(registry);
        for w in warnings {
            tracing::warn!("{w}");
        }
        Ok(cfg)
    }

    /// Save the configuration to disk.
    pub fn save(&self, path: impl AsRef<Path>) -> anyhow::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Remove unsupported widgets and normalize empty settings.
    pub fn sanitize(&mut self, registry: &WidgetRegistry) -> Vec<String> {
        let mut warnings = Vec::new();
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
            let Some(default_settings) = registry.default_settings("todos") else {
                continue;
            };
            match slot.widget.as_str() {
                "todo" | "todo_list" | "todo_summary" | "todo_burndown" => {
                    let legacy_name = slot.widget.clone();
                    slot.widget = "todos".into();
                    slot.settings = merge_json(&default_settings, &slot.settings);
                    warnings.push(format!(
                        "dashboard widget '{}' migrated to 'todos'",
                        legacy_name
                    ));
                }
                _ => {}
            }
        }
    }
}
