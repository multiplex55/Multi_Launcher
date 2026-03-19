use super::{WidgetAction, WidgetSettingsContext, WidgetSettingsUiResult};
use crate::actions::Action;
use crate::dashboard::dashboard::DashboardContext;
use crate::mouse_gestures::engine::DirMode;
use crate::mouse_gestures::selection::{GestureFocusArgs, GestureToggleArgs};
use eframe::egui;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

pub(crate) fn merge_json(base: &Value, updates: &Value) -> Value {
    match (base, updates) {
        (Value::Object(a), Value::Object(b)) => {
            let mut merged = a.clone();
            for (k, v) in b {
                merged.insert(k.clone(), v.clone());
            }
            Value::Object(merged)
        }
        _ => updates.clone(),
    }
}

pub(crate) fn plugin_names(ctx: &WidgetSettingsContext<'_>) -> Vec<String> {
    if let Some(infos) = ctx.plugin_infos {
        let mut names: Vec<String> = infos
            .iter()
            .filter(|(name, _, _)| {
                ctx.enabled_plugins
                    .map(|set| set.contains(name))
                    .unwrap_or(true)
            })
            .map(|(name, _, _)| name.clone())
            .collect();
        names.sort();
        names.dedup();
        return names;
    }
    if let Some(manager) = ctx.plugins {
        let mut names = manager.plugin_names();
        if let Some(enabled) = ctx.enabled_plugins {
            names.retain(|name| enabled.contains(name));
        }
        names.sort();
        names
    } else {
        Vec::new()
    }
}

pub(crate) fn find_plugin<'a>(
    ctx: &'a DashboardContext<'a>,
    name: &str,
) -> Option<&'a dyn crate::plugin::Plugin> {
    ctx.plugins
        .iter()
        .find_map(|p| if p.name() == name { Some(&**p) } else { None })
}

pub(crate) fn gesture_focus_action(
    label: &str,
    tokens: &str,
    dir_mode: DirMode,
    binding_idx: Option<usize>,
) -> WidgetAction {
    let args = GestureFocusArgs {
        label: label.to_string(),
        tokens: tokens.to_string(),
        dir_mode,
        binding_idx,
    };
    WidgetAction {
        action: Action {
            label: label.to_string(),
            desc: "Mouse gestures".into(),
            action: "mg:dialog:focus".into(),
            args: serde_json::to_string(&args).ok(),
        },
        query_override: None,
    }
}

pub(crate) fn gesture_toggle_action(
    label: &str,
    tokens: &str,
    dir_mode: DirMode,
    enabled: bool,
) -> WidgetAction {
    let args = GestureToggleArgs {
        label: label.to_string(),
        tokens: tokens.to_string(),
        dir_mode,
        enabled,
    };
    WidgetAction {
        action: Action {
            label: format!("Toggle {label}"),
            desc: "Mouse gestures".into(),
            action: "mg:toggle".into(),
            args: serde_json::to_string(&args).ok(),
        },
        query_override: None,
    }
}

pub(crate) fn edit_typed_settings<C: DeserializeOwned + Serialize + Default>(
    ui: &mut egui::Ui,
    value: &mut Value,
    ctx: &WidgetSettingsContext<'_>,
    render: impl FnOnce(&mut egui::Ui, &mut C, &WidgetSettingsContext<'_>) -> bool,
) -> WidgetSettingsUiResult {
    let mut changed = false;
    let mut error = None;
    if value.is_null() {
        *value = serde_json::to_value(C::default()).unwrap_or_else(|_| json!({}));
        changed = true;
    }

    let original = value.clone();
    let mut cfg: C = match serde_json::from_value(original.clone()) {
        Ok(cfg) => cfg,
        Err(e) => {
            error = Some(format!("Failed to parse settings: {e}"));
            C::default()
        }
    };

    let before = serde_json::to_value(&cfg).unwrap_or_else(|_| json!({}));
    let ui_changed = render(ui, &mut cfg, ctx);
    changed |= ui_changed;
    let serialized = serde_json::to_value(&cfg).unwrap_or_else(|_| json!({}));
    let merged = merge_json(&original, &serialized);
    if merged != *value {
        *value = merged;
        changed = true;
    } else if ui_changed && serialized != before {
        changed = true;
    }

    WidgetSettingsUiResult { changed, error }
}

#[derive(Debug, Clone)]
pub struct TimedCache<T> {
    pub data: T,
    pub last_refresh: Instant,
    pub interval: Duration,
}

impl<T> TimedCache<T> {
    pub fn new(data: T, interval: Duration) -> Self {
        Self {
            data,
            last_refresh: Instant::now() - interval,
            interval,
        }
    }
    pub fn should_refresh(&self) -> bool {
        self.last_refresh.elapsed() >= self.interval
    }
    pub fn refresh(&mut self, update: impl FnOnce(&mut T)) {
        update(&mut self.data);
        self.last_refresh = Instant::now();
    }
    pub fn touch(&mut self) {
        self.last_refresh = Instant::now();
    }
    pub fn set_interval(&mut self, interval: Duration) {
        self.interval = interval;
    }
    pub fn invalidate(&mut self) {
        self.last_refresh = Instant::now() - self.interval;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum RefreshMode {
    #[default]
    Auto,
    Manual,
    Throttled,
}

impl std::fmt::Display for RefreshMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RefreshMode::Auto => write!(f, "Auto"),
            RefreshMode::Manual => write!(f, "Manual"),
            RefreshMode::Throttled => write!(f, "Throttled"),
        }
    }
}

pub(crate) fn default_refresh_throttle_secs() -> f32 {
    5.0
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct RefreshSchedule {
    pub interval: Duration,
    pub mode: RefreshMode,
    pub throttle: Duration,
}

impl RefreshSchedule {
    pub fn effective_interval(&self) -> Duration {
        match self.mode {
            RefreshMode::Throttled => self.interval.max(self.throttle),
            _ => self.interval,
        }
    }
}

pub(crate) fn refresh_schedule(
    interval: Duration,
    refresh_mode: RefreshMode,
    manual_refresh_only: bool,
    throttle_secs: f32,
) -> RefreshSchedule {
    let mode = if manual_refresh_only && refresh_mode == RefreshMode::Auto {
        RefreshMode::Manual
    } else {
        refresh_mode
    };
    RefreshSchedule {
        interval,
        mode,
        throttle: Duration::from_secs_f32(throttle_secs.max(0.0)),
    }
}

pub(crate) fn run_refresh_schedule(
    ctx: &DashboardContext<'_>,
    schedule: RefreshSchedule,
    refresh_pending: &mut bool,
    last_refresh: &mut Instant,
) -> bool {
    if ctx.reduce_dashboard_work_when_unfocused
        && (!ctx.dashboard_visible || !ctx.dashboard_focused)
    {
        *last_refresh = Instant::now();
        return false;
    }
    let elapsed = last_refresh.elapsed();
    let should_auto = match schedule.mode {
        RefreshMode::Auto => elapsed >= schedule.interval,
        RefreshMode::Manual => false,
        RefreshMode::Throttled => elapsed >= schedule.effective_interval(),
    };
    let should_refresh = *refresh_pending || should_auto;
    if !should_refresh {
        return false;
    }
    if schedule.mode == RefreshMode::Throttled && elapsed < schedule.throttle {
        return false;
    }
    *refresh_pending = false;
    true
}

pub(crate) fn refresh_settings_ui(
    ui: &mut egui::Ui,
    seconds: &mut f32,
    refresh_mode: &mut RefreshMode,
    throttle_secs: &mut f32,
    manual_refresh_only: Option<&mut bool>,
    tooltip: &str,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label("Refresh every");
        let resp = ui
            .add(
                egui::DragValue::new(seconds)
                    .clamp_range(1.0..=300.0)
                    .speed(0.5),
            )
            .on_hover_text(tooltip);
        changed |= resp.changed();
        ui.label("seconds");
    });
    ui.horizontal(|ui| {
        ui.label("Refresh mode");
        let selected = refresh_mode.to_string();
        egui::ComboBox::from_id_source(ui.id().with("refresh_mode"))
            .selected_text(selected)
            .show_ui(ui, |ui| {
                changed |= ui
                    .selectable_value(refresh_mode, RefreshMode::Auto, "Auto")
                    .changed();
                changed |= ui
                    .selectable_value(refresh_mode, RefreshMode::Manual, "Manual")
                    .changed();
                changed |= ui
                    .selectable_value(refresh_mode, RefreshMode::Throttled, "Throttled")
                    .changed();
            });
    });
    if *refresh_mode == RefreshMode::Throttled {
        ui.horizontal(|ui| {
            ui.label("Minimum interval");
            changed |= ui
                .add(
                    egui::DragValue::new(throttle_secs)
                        .clamp_range(1.0..=300.0)
                        .speed(0.5),
                )
                .changed();
            ui.label("seconds");
        });
    }
    if let Some(manual_refresh_only) = manual_refresh_only {
        if *manual_refresh_only && *refresh_mode == RefreshMode::Auto {
            *refresh_mode = RefreshMode::Manual;
            changed = true;
        }
        if *manual_refresh_only != (*refresh_mode == RefreshMode::Manual) {
            *manual_refresh_only = *refresh_mode == RefreshMode::Manual;
            changed = true;
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::merge_json;
    use serde_json::json;

    #[test]
    fn merge_json_preserves_unknown_fields() {
        let base = json!({"known": 1, "extra": {"keep": true}});
        let updates = json!({"known": 2});
        let merged = merge_json(&base, &updates);
        assert_eq!(merged["known"], json!(2));
        assert_eq!(merged["extra"], json!({"keep": true}));
    }
}
