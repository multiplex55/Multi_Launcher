//! Isolated, feature-gated UI fixture for headless dashboard measurements.

use super::{Dashboard, DashboardContext, WidgetActivation};
use crate::actions::{Action, ActionIdLookup};
use crate::dashboard::config::{DashboardConfig, GridConfig, SlotConfig};
use crate::dashboard::data_cache::DashboardDataCache;
use crate::dashboard::widgets::WidgetRegistry;
use crate::perf_harness::{ProbeFailure, ProbeStatus};
use crate::plugin::PluginManager;
use eframe::egui;
use serde::Serialize;
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

const VIEWPORT_WIDTH_POINTS: f32 = 1280.0;
const VIEWPORT_HEIGHT_POINTS: f32 = 720.0;
const PIXELS_PER_POINT: f32 = 1.0;
const FRAME_DELTA_SECONDS: f32 = 1.0 / 60.0;
const SYNTHETIC_ACTIONS: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DashboardTopology {
    Minimal,
    Typical,
    Dense,
}

impl DashboardTopology {
    fn for_scenario(scenario: crate::perf_harness::Scenario) -> Result<Self, String> {
        use crate::perf_harness::Scenario;

        match scenario {
            Scenario::DashboardMinimal => Ok(Self::Minimal),
            Scenario::DashboardTypical => Ok(Self::Typical),
            Scenario::DashboardDense => Ok(Self::Dense),
            _ => Err(format!(
                "dashboard fixture does not accept scenario {}",
                scenario.as_str()
            )),
        }
    }

    fn grid(self) -> (u8, u8) {
        match self {
            Self::Minimal => (1, 1),
            Self::Typical => (2, 2),
            Self::Dense => (3, 3),
        }
    }

    fn widgets(self) -> &'static [&'static str] {
        match self {
            Self::Minimal => &["quick_tools"],
            Self::Typical => &[
                "quick_tools",
                "pinned_commands",
                "frequent_commands",
                "quick_tools",
            ],
            Self::Dense => &[
                "quick_tools",
                "pinned_commands",
                "frequent_commands",
                "quick_tools",
                "frequent_commands",
                "pinned_commands",
                "quick_tools",
                "pinned_commands",
                "frequent_commands",
            ],
        }
    }
}

#[derive(Serialize)]
struct UsageIdentity<'a> {
    action: &'a str,
    count: u32,
}

#[derive(Serialize)]
struct FixtureIdentity<'a> {
    schema: &'static str,
    config_json: &'a [u8],
    actions: &'a [Action],
    usage: Vec<UsageIdentity<'a>>,
    viewport_width_points: f32,
    viewport_height_points: f32,
    pixels_per_point: f32,
    frame_delta_seconds: f32,
}

struct FixtureDefinition {
    config_json: Vec<u8>,
    expected_slots: Vec<(String, String)>,
    actions: Vec<Action>,
    usage: HashMap<String, u32>,
    ordered_usage: Vec<(String, u32)>,
}

impl FixtureDefinition {
    fn new(scenario: crate::perf_harness::Scenario) -> Result<Self, String> {
        let topology = DashboardTopology::for_scenario(scenario)?;
        let registry = WidgetRegistry::with_defaults();
        let (rows, cols) = topology.grid();
        let mut expected_slots = Vec::with_capacity(topology.widgets().len());
        let mut slots = Vec::with_capacity(topology.widgets().len());

        for (index, widget) in topology.widgets().iter().enumerate() {
            let id = format!("perf-dashboard-slot-{index:02}");
            let mut slot = SlotConfig::with_widget(
                widget,
                (index / cols as usize) as i32,
                (index % cols as usize) as i32,
            );
            slot.id = Some(id.clone());
            slot.settings = registry
                .default_settings(widget)
                .ok_or_else(|| format!("dashboard registry lacks safe widget {widget}"))?;
            expected_slots.push((id, (*widget).to_string()));
            slots.push(slot);
        }

        let config = DashboardConfig {
            version: 1,
            grid: GridConfig { rows, cols },
            slots,
        };
        let config_json = serde_json::to_vec(&config)
            .map_err(|error| format!("serialize dashboard fixture config: {error}"))?;
        let actions: Vec<Action> = (0..SYNTHETIC_ACTIONS)
            .map(|index| Action {
                label: format!("Fixture command {index:02}"),
                desc: "Synthetic dashboard benchmark action".into(),
                action: format!("perf-command-{index:02}"),
                args: None,
            })
            .collect();
        let ordered_usage: Vec<(String, u32)> = actions
            .iter()
            .enumerate()
            .map(|(index, action)| (action.action.clone(), index as u32 + 1))
            .collect();
        let usage = ordered_usage.iter().cloned().collect();

        Ok(Self {
            config_json,
            expected_slots,
            actions,
            usage,
            ordered_usage,
        })
    }

    fn identity_bytes(&self) -> Result<Vec<u8>, String> {
        let usage = self
            .ordered_usage
            .iter()
            .map(|(action, count)| UsageIdentity {
                action,
                count: *count,
            })
            .collect();
        serde_json::to_vec(&FixtureIdentity {
            schema: "dashboard-egui-frame-fixture-v1",
            config_json: &self.config_json,
            actions: &self.actions,
            usage,
            viewport_width_points: VIEWPORT_WIDTH_POINTS,
            viewport_height_points: VIEWPORT_HEIGHT_POINTS,
            pixels_per_point: PIXELS_PER_POINT,
            frame_delta_seconds: FRAME_DELTA_SECONDS,
        })
        .map_err(|error| format!("serialize dashboard fixture identity: {error}"))
    }
}

/// Feature-gated owner for an inert Dashboard and its synthetic frame inputs.
pub struct DashboardCpuFixture {
    dashboard: Dashboard,
    actions: Arc<Vec<Action>>,
    actions_by_id: ActionIdLookup,
    usage: HashMap<String, u32>,
    plugins: PluginManager,
    data_cache: DashboardDataCache,
    expected_slots: Vec<(String, String)>,
    rendered_frames: u64,
}

/// One validated, CPU-only egui frame result.
pub struct DashboardFrame {
    pub elapsed: Duration,
    pub shape_count: usize,
}

impl DashboardCpuFixture {
    /// Deterministic identity material for the exact serialized config, actions,
    /// usage order, and fixed viewport/frame inputs used by `new`.
    pub fn identity_bytes(scenario: crate::perf_harness::Scenario) -> Result<Vec<u8>, String> {
        FixtureDefinition::new(scenario)?.identity_bytes()
    }

    /// Create a fresh private fixture directory and load only the safe widget set.
    pub fn new(
        fixture_directory: impl AsRef<Path>,
        scenario: crate::perf_harness::Scenario,
    ) -> Result<Self, ProbeFailure> {
        let definition = FixtureDefinition::new(scenario).map_err(ProbeFailure::error)?;
        let fixture_directory = fixture_directory.as_ref().to_path_buf();
        fs::create_dir(&fixture_directory).map_err(|error| {
            fixture_io_failure("create fresh dashboard fixture directory", error)
        })?;

        let config_path = fixture_directory.join("dashboard.json");
        let mut config_file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&config_path)
            .map_err(|error| fixture_io_failure("create synthetic dashboard config", error))?;
        config_file
            .write_all(&definition.config_json)
            .map_err(|error| fixture_io_failure("write synthetic dashboard config", error))?;
        config_file
            .flush()
            .map_err(|error| fixture_io_failure("flush synthetic dashboard config", error))?;

        let registry = WidgetRegistry::with_defaults();
        let dashboard = Dashboard::new(&config_path, registry, None);
        if !dashboard.warnings.is_empty() {
            return Err(ProbeFailure::error(format!(
                "synthetic dashboard config produced warnings: {}",
                dashboard.warnings.join("; ")
            )));
        }
        let actions = Arc::new(definition.actions);
        let actions_by_id = ActionIdLookup::new(Arc::clone(&actions));
        let plugins = PluginManager::new_for_dashboard_fixture(&fixture_directory);
        let fixture = Self {
            dashboard,
            actions,
            actions_by_id,
            usage: definition.usage,
            plugins,
            data_cache: DashboardDataCache::new(),
            expected_slots: definition.expected_slots,
            rendered_frames: 0,
        };
        fixture.validate_state().map_err(ProbeFailure::error)?;
        Ok(fixture)
    }

    pub fn widget_count(&self) -> usize {
        self.expected_slots.len()
    }

    /// Run exactly one fixed headless egui frame and validate its output.
    pub fn render_frame(
        &mut self,
        context: &egui::Context,
        frame_index: u64,
    ) -> Result<DashboardFrame, String> {
        self.validate_state()?;
        let time = frame_index as f64 * FRAME_DELTA_SECONDS as f64;
        let viewport_rect = egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(VIEWPORT_WIDTH_POINTS, VIEWPORT_HEIGHT_POINTS),
        );
        let mut raw_input = egui::RawInput {
            screen_rect: Some(viewport_rect),
            time: Some(time),
            predicted_dt: FRAME_DELTA_SECONDS,
            ..Default::default()
        };
        if let Some(viewport) = raw_input.viewports.get_mut(&raw_input.viewport_id) {
            viewport.native_pixels_per_point = Some(PIXELS_PER_POINT);
            viewport.inner_rect = Some(viewport_rect);
        }

        let mut activated = false;
        let start = Instant::now();
        let output = context.run_ui(raw_input, |ui| {
            let context = DashboardContext {
                actions: self.actions.as_slice(),
                actions_by_id: &self.actions_by_id,
                usage: &self.usage,
                plugins: &self.plugins,
                enabled_plugins: None,
                default_location: None,
                data_cache: &self.data_cache,
                actions_version: 1,
                fav_version: 0,
                notes_version: 0,
                todo_version: 0,
                calendar_version: 0,
                clipboard_version: 0,
                snippets_version: 0,
                dashboard_visible: true,
                dashboard_focused: true,
                reduce_dashboard_work_when_unfocused: false,
                diagnostics: None,
                show_diagnostics_widget: false,
                jump_mode_scope: None,
                jump_mode_activation_allowed: true,
            };
            activated = self
                .dashboard
                .ui(ui, &context, WidgetActivation::Click)
                .is_some();
        });
        let elapsed = start.elapsed();

        let shape_count = output.shapes.len();
        let validation = if activated {
            Err("synthetic dashboard frame unexpectedly activated a widget".to_string())
        } else if shape_count == 0 {
            Err("synthetic dashboard frame produced no egui shapes".to_string())
        } else if output
            .shapes
            .iter()
            .any(|shape| !rect_is_finite(shape.clip_rect))
        {
            Err("synthetic dashboard frame contains a non-finite clip rectangle".to_string())
        } else {
            Ok(())
        };
        output.drop_without_applying_deltas();
        validation?;
        self.rendered_frames = self.rendered_frames.saturating_add(1);
        self.validate_state()?;

        Ok(DashboardFrame {
            elapsed,
            shape_count,
        })
    }

    fn validate_state(&self) -> Result<(), String> {
        if !self.dashboard.warnings.is_empty() {
            return Err(format!(
                "dashboard fixture has warnings: {}",
                self.dashboard.warnings.join("; ")
            ));
        }
        if self.dashboard.slots.len() != self.expected_slots.len() {
            return Err(format!(
                "dashboard normalized {} slots; expected {}",
                self.dashboard.slots.len(),
                self.expected_slots.len()
            ));
        }
        for (index, (slot, (expected_id, expected_widget))) in self
            .dashboard
            .slots
            .iter()
            .zip(&self.expected_slots)
            .enumerate()
        {
            if slot.id.as_deref() != Some(expected_id.as_str())
                || slot.widget != *expected_widget
                || slot.row != index / self.dashboard.config.grid.cols as usize
                || slot.col != index % self.dashboard.config.grid.cols as usize
            {
                return Err(format!(
                    "normalized dashboard slot {index} differs from the synthetic fixture"
                ));
            }
        }

        if self.rendered_frames > 0 {
            let mut actual_keys: Vec<String> = self
                .dashboard
                .diagnostics_snapshot()
                .widget_refreshes
                .into_iter()
                .map(|refresh| refresh.label)
                .collect();
            let mut expected_keys: Vec<String> = self
                .expected_slots
                .iter()
                .map(|(id, _)| id.clone())
                .collect();
            actual_keys.sort();
            expected_keys.sort();
            if actual_keys != expected_keys {
                return Err(format!(
                    "dashboard refresh diagnostics had {} widget keys; expected {}",
                    actual_keys.len(),
                    expected_keys.len()
                ));
            }
        }
        Ok(())
    }
}

fn fixture_io_failure(operation: &str, error: std::io::Error) -> ProbeFailure {
    let reason = format!("{operation}: {error}");
    #[cfg(windows)]
    if error.raw_os_error() == Some(5) {
        return ProbeFailure::new(ProbeStatus::BlockedEnvironment, reason);
    }
    ProbeFailure::error(reason)
}

fn rect_is_finite(rect: egui::Rect) -> bool {
    rect.min.x.is_finite()
        && rect.min.y.is_finite()
        && rect.max.x.is_finite()
        && rect.max.y.is_finite()
}
