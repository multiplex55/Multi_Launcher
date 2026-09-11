use crate::actions::Action;
use crate::commands::{
    Command, ExternalCommand, VirtualDesktopLaunchPayload, VirtualDesktopMoveActivePayload,
    VirtualDesktopMoveWindowPayload, VirtualDesktopRenamePayload, VirtualDesktopTargetPayload,
    VirtualDesktopWindowPayload, VirtualDesktopWorkspacePayload,
};
use crate::plugin::Plugin;
use crate::virtual_desktop::rules::{RuleRuntimeController, RuleRuntimeStatus, VirtualDesktopRule};
use crate::virtual_desktop::{VirtualDesktopService, VirtualDesktopSnapshot};
use crate::window_catalog::WindowCatalog;
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Default)]
struct WindowDetailCache {
    generation: u64,
    windows: std::collections::HashMap<usize, Option<crate::window_catalog::WindowDescriptor>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct VirtualDesktopPluginSettings {
    pub launch_timeout_ms: u64,
    pub auto_switch_rules: Vec<VirtualDesktopRule>,
}

impl Default for VirtualDesktopPluginSettings {
    fn default() -> Self {
        Self {
            launch_timeout_ms: 10_000,
            auto_switch_rules: Vec::new(),
        }
    }
}

trait DesktopSource: Send + Sync {
    fn snapshot(&self) -> Result<VirtualDesktopSnapshot, String>;
}

trait WorkspaceSource: Send + Sync {
    fn snapshot(&self) -> Vec<crate::multi_manager::workspace_catalog::WorkspaceDescriptor>;
}

trait WindowDetailsSource: Send + Sync {
    fn describe(&self, hwnd: usize) -> Option<crate::window_catalog::WindowDescriptor>;
}

struct ProductionWindowDetailsSource;
impl WindowDetailsSource for ProductionWindowDetailsSource {
    fn describe(&self, hwnd: usize) -> Option<crate::window_catalog::WindowDescriptor> {
        crate::window_catalog::describe_window(hwnd)
    }
}

struct ProductionWorkspaceSource {
    catalog: Arc<crate::multi_manager::workspace_catalog::WorkspaceCatalog>,
}

impl WorkspaceSource for ProductionWorkspaceSource {
    fn snapshot(&self) -> Vec<crate::multi_manager::workspace_catalog::WorkspaceDescriptor> {
        self.catalog.snapshot()
    }
}

struct ProductionDesktopSource;

impl DesktopSource for ProductionDesktopSource {
    fn snapshot(&self) -> Result<VirtualDesktopSnapshot, String> {
        VirtualDesktopService
            .snapshot()
            .map_err(|error| error.to_string())
    }
}

pub struct VirtualDesktopPlugin {
    desktops: Arc<dyn DesktopSource>,
    workspaces: Arc<dyn WorkspaceSource>,
    catalog: Arc<WindowCatalog>,
    window_details: Arc<dyn WindowDetailsSource>,
    window_detail_cache: std::sync::Mutex<WindowDetailCache>,
    actions: Arc<Vec<Action>>,
    settings: VirtualDesktopPluginSettings,
    rule_runtime: RuleRuntimeController,
    runtime_enabled: bool,
    settings_desktops: Option<Result<VirtualDesktopSnapshot, String>>,
}

impl VirtualDesktopPlugin {
    pub(crate) fn new(
        workspace_catalog: Arc<crate::multi_manager::workspace_catalog::WorkspaceCatalog>,
        catalog: Arc<WindowCatalog>,
        actions: Arc<Vec<Action>>,
    ) -> Self {
        Self {
            desktops: Arc::new(ProductionDesktopSource),
            workspaces: Arc::new(ProductionWorkspaceSource {
                catalog: workspace_catalog,
            }),
            catalog,
            window_details: Arc::new(ProductionWindowDetailsSource),
            window_detail_cache: std::sync::Mutex::new(WindowDetailCache::default()),
            actions,
            settings: VirtualDesktopPluginSettings::default(),
            rule_runtime: RuleRuntimeController::default(),
            runtime_enabled: false,
            settings_desktops: None,
        }
    }

    #[cfg(test)]
    fn with_source(
        desktops: Arc<dyn DesktopSource>,
        workspaces: Arc<dyn WorkspaceSource>,
        catalog: Arc<WindowCatalog>,
        actions: Arc<Vec<Action>>,
    ) -> Self {
        Self {
            desktops,
            workspaces,
            catalog,
            window_details: Arc::new(ProductionWindowDetailsSource),
            window_detail_cache: std::sync::Mutex::new(WindowDetailCache::default()),
            actions,
            settings: VirtualDesktopPluginSettings::default(),
            rule_runtime: RuleRuntimeController::default(),
            runtime_enabled: false,
            settings_desktops: None,
        }
    }

    fn overview(&self, snapshot: &VirtualDesktopSnapshot) -> Vec<Action> {
        let mut rows = Vec::new();
        rows.extend(capability_diagnostics(snapshot));
        if let Ok(current) = snapshot.current() {
            rows.push(simple_action(
                format!(
                    "Current Desktop: {} — Desktop {}",
                    current.display_name(),
                    current.index
                ),
                "Current virtual desktop",
                "vd:current",
            ));
        }
        for desktop in &snapshot.desktops {
            rows.push(simple_action(
                format!(
                    "Desktop {} — {}{}",
                    desktop.index,
                    desktop.display_name(),
                    if desktop.is_current { " [Current]" } else { "" }
                ),
                "Virtual Desktop",
                "vd:list",
            ));
        }
        if snapshot.capabilities.creation {
            rows.push(simple_action(
                "Create Virtual Desktop",
                "Virtual Desktop",
                "vd:create",
            ));
        }
        if snapshot.capabilities.direct_switching {
            rows.push(simple_action(
                "Switch Previous",
                "Virtual Desktop",
                "vd:previous",
            ));
            rows.push(simple_action("Switch Next", "Virtual Desktop", "vd:next"));
        }
        rows.extend(self.desktop_actions(snapshot, "", false));
        if snapshot.capabilities.window_membership {
            rows.push(query_action(
                "Search Windows Across Desktops",
                "vd windows ",
            ));
        }
        rows.push(simple_action(
            "Virtual Desktop Settings",
            "Virtual Desktop",
            "vd:settings",
        ));
        rows.push(query_action("Configure Auto-Switch Rules", "vd rules "));
        rows
    }

    fn desktop_actions(
        &self,
        snapshot: &VirtualDesktopSnapshot,
        filter: &str,
        move_only: bool,
    ) -> Vec<Action> {
        let filter = filter.trim().to_lowercase();
        let mut rows = Vec::new();
        for desktop in snapshot.desktops.iter().filter(|desktop| {
            desktop.display_name().to_lowercase().contains(&filter)
                || desktop.index.to_string().contains(&filter)
                || desktop.id.as_str().contains(&filter)
        }) {
            let target = desktop.id.to_string();
            if !move_only && snapshot.capabilities.direct_switching {
                rows.push(json_action(
                    format!("Switch to {}", desktop.display_name()),
                    format!("Desktop {}", desktop.index),
                    "vd:switch",
                    &VirtualDesktopTargetPayload {
                        target: target.clone(),
                    },
                ));
            }
            if snapshot.capabilities.window_movement {
                rows.push(json_action(
                    format!("Move Active Window to {}", desktop.display_name()),
                    "Move without switching desktops".into(),
                    "vd:move-active",
                    &VirtualDesktopMoveActivePayload {
                        target: target.clone(),
                        follow: false,
                    },
                ));
                rows.push(json_action(
                    format!(
                        "Move Active Window to {} and Follow",
                        desktop.display_name()
                    ),
                    "Move, switch, and activate".into(),
                    "vd:move-active",
                    &VirtualDesktopMoveActivePayload {
                        target,
                        follow: true,
                    },
                ));
            }
        }
        rows
    }

    fn window_actions(&self, snapshot: &VirtualDesktopSnapshot, filter: &str) -> Vec<Action> {
        let filter = filter.trim().to_lowercase();
        let catalog = self.catalog.snapshot_with_desktops_and_refresh();
        let windows = &catalog.windows;
        let mut rows = Vec::new();
        for window in windows.iter().filter(|window| {
            filter.is_empty()
                || window.title.to_lowercase().contains(&filter)
                || window
                    .executable
                    .as_deref()
                    .is_some_and(|value| value.to_lowercase().contains(&filter))
        }) {
            let desktop_label = catalog
                .desktop_ids
                .get(&window.hwnd)
                .and_then(|desktop| desktop.as_ref())
                .and_then(|id| snapshot.desktops.iter().find(|entry| &entry.id == id))
                .map(|entry| {
                    format!(
                        "{} • Desktop {}{}",
                        entry.display_name(),
                        entry.index,
                        if entry.is_current { " • Current" } else { "" }
                    )
                })
                .unwrap_or_else(|| {
                    if catalog.desktop_ids_ready {
                        "Unknown Desktop".into()
                    } else {
                        "Loading desktop details…".into()
                    }
                });
            rows.push(json_action(
                format!("Activate {}", window.title),
                desktop_label.clone(),
                "vd:activate-window",
                &VirtualDesktopWindowPayload { hwnd: window.hwnd },
            ));
            rows.push(query_action(
                format!("Move {} to…", window.title),
                format!("vd windows move {} ", window.hwnd),
            ));
        }
        if !filter.is_empty()
            && windows.iter().any(|window| {
                window.executable.is_none() && !window.title.to_lowercase().contains(&filter)
            })
        {
            rows.push(query_action(
                format!("Search executable names for {filter:?}"),
                format!("vd windows details {filter}"),
            ));
        }
        rows
    }

    fn window_detail_actions(
        &self,
        snapshot: &VirtualDesktopSnapshot,
        filter: &str,
    ) -> Vec<Action> {
        let filter = filter.trim().to_lowercase();
        if filter.is_empty() {
            return Vec::new();
        }
        let catalog = self.catalog.snapshot_with_desktops_and_refresh();
        let mut cache = match self.window_detail_cache.lock() {
            Ok(cache) => cache,
            Err(_) => return Vec::new(),
        };
        if cache.generation != catalog.generation {
            cache.generation = catalog.generation;
            cache.windows.clear();
        }
        let enriched = catalog
            .windows
            .iter()
            .filter_map(|minimal| {
                if let Some(window) = cache.windows.get(&minimal.hwnd) {
                    return window.clone();
                }
                let window = self
                    .window_details
                    .describe(minimal.hwnd)
                    .or_else(|| minimal.executable.is_some().then(|| minimal.clone()));
                cache.windows.insert(minimal.hwnd, window.clone());
                window
            })
            .filter(|window| {
                window.title.to_lowercase().contains(&filter)
                    || window
                        .executable
                        .as_deref()
                        .is_some_and(|value| value.to_lowercase().contains(&filter))
            })
            .collect::<Vec<_>>();
        let mut rows = Vec::new();
        for window in enriched {
            let desktop_label = catalog
                .desktop_ids
                .get(&window.hwnd)
                .and_then(|desktop| desktop.as_ref())
                .and_then(|id| snapshot.desktops.iter().find(|entry| &entry.id == id))
                .map(|entry| format!("{} • Desktop {}", entry.display_name(), entry.index))
                .unwrap_or_else(|| "Desktop details unavailable".into());
            rows.push(json_action(
                format!("Activate {}", window.title),
                desktop_label,
                "vd:activate-window",
                &VirtualDesktopWindowPayload { hwnd: window.hwnd },
            ));
            rows.push(query_action(
                format!("Move {} to…", window.title),
                format!("vd windows move {} ", window.hwnd),
            ));
        }
        rows
    }

    fn window_move_actions(&self, snapshot: &VirtualDesktopSnapshot, input: &str) -> Vec<Action> {
        let (hwnd, filter) = input
            .trim()
            .split_once(char::is_whitespace)
            .unwrap_or((input.trim(), ""));
        let Ok(hwnd) = hwnd.parse::<usize>() else {
            return Vec::new();
        };
        let windows = self.catalog.snapshot();
        let Some(window) = windows.iter().find(|window| window.hwnd == hwnd) else {
            return Vec::new();
        };
        let filter = filter.trim().to_lowercase();
        snapshot
            .desktops
            .iter()
            .filter(|desktop| {
                desktop.display_name().to_lowercase().contains(&filter)
                    || desktop.index.to_string().contains(&filter)
                    || desktop.id.as_str().contains(&filter)
            })
            .flat_map(|target| {
                [false, true].map(|follow| {
                    json_action(
                        format!(
                            "Move {} to {}{}",
                            window.title,
                            target.display_name(),
                            if follow { " and Follow" } else { "" }
                        ),
                        "Virtual Desktop".into(),
                        "vd:move-window",
                        &VirtualDesktopMoveWindowPayload {
                            hwnd,
                            target: target.id.to_string(),
                            follow,
                        },
                    )
                })
            })
            .collect()
    }

    fn launch_actions(&self, snapshot: &VirtualDesktopSnapshot, input: &str) -> Vec<Action> {
        let input = input.trim();
        if input.is_empty() {
            return snapshot
                .desktops
                .iter()
                .map(|desktop| {
                    query_action(
                        format!("Launch application on {}", desktop.display_name()),
                        format!("vd launch {} ", desktop.id),
                    )
                })
                .collect();
        }
        let (target_text, filter) = input.split_once(char::is_whitespace).unwrap_or((input, ""));
        let Ok(selector) = crate::virtual_desktop::VirtualDesktopSelector::parse(target_text)
        else {
            return Vec::new();
        };
        let Ok(target) = snapshot.resolve(&selector) else {
            return Vec::new();
        };
        let filter = filter.trim().to_lowercase();
        self.actions
            .iter()
            .filter_map(|action| {
                let Command::External(ExternalCommand {
                    target: application,
                    args,
                    namespace,
                }) = crate::commands::parse_action(action).ok()?
                else {
                    return None;
                };
                if namespace.is_some()
                    || !is_direct_executable_target(&application)
                    || (!filter.is_empty() && !action.label.to_lowercase().contains(&filter))
                {
                    return None;
                }
                let base = VirtualDesktopLaunchPayload {
                    target: target.id.to_string(),
                    application,
                    args,
                    follow: false,
                    timeout_ms: self.settings.launch_timeout_ms,
                };
                let mut follow = base.clone();
                follow.follow = true;
                Some([
                    json_action(
                        format!("Launch {} on {}", action.label, target.display_name()),
                        "Launch and move without switching desktops".into(),
                        "vd:launch",
                        &base,
                    ),
                    json_action(
                        format!(
                            "Launch {} on {} and Follow",
                            action.label,
                            target.display_name()
                        ),
                        "Launch, move, switch, and activate".into(),
                        "vd:launch",
                        &follow,
                    ),
                ])
            })
            .flatten()
            .collect()
    }

    fn workspace_bind_actions(
        &self,
        snapshot: &VirtualDesktopSnapshot,
        input: &str,
    ) -> Vec<Action> {
        let input = input.trim();
        let workspaces = self.workspaces.snapshot();
        let mut full_matches = workspaces
            .iter()
            .filter_map(|workspace| {
                let label = workspace_label(workspace);
                workspace_target_remainder(input, &label)
                    .or_else(|| workspace_target_remainder(input, &workspace.id))
                    .map(|target| (workspace, label, target))
            })
            .collect::<Vec<_>>();
        if !full_matches.is_empty() {
            let longest = full_matches
                .iter()
                .map(|(_, label, _)| label.len())
                .max()
                .unwrap_or(0);
            full_matches.retain(|(_, label, _)| label.len() == longest);
            if full_matches.len() != 1 {
                return Vec::new();
            }
            let (workspace, label, target) = full_matches.remove(0);
            let Ok(selector) = crate::virtual_desktop::VirtualDesktopSelector::parse(target) else {
                return Vec::new();
            };
            let Ok(desktop) = snapshot.resolve(&selector) else {
                return Vec::new();
            };
            return vec![json_action(
                format!("Bind workspace {label} to {}", desktop.display_name()),
                "MultiManager virtual desktop binding".into(),
                "vd:bind-workspace",
                &VirtualDesktopWorkspacePayload {
                    workspace_id: workspace.id.clone(),
                    target: Some(desktop.id.to_string()),
                    cached_name: desktop.name.clone(),
                },
            )];
        }

        let filter = input.to_ascii_lowercase();
        workspaces
            .iter()
            .filter_map(|workspace| {
                let label = workspace_label(workspace);
                (filter.is_empty()
                    || label.to_ascii_lowercase().contains(&filter)
                    || workspace.id.to_ascii_lowercase().contains(&filter))
                .then(|| {
                    query_action(
                        format!("Bind workspace {label}"),
                        format!("vd bind workspace {label} "),
                    )
                })
            })
            .collect()
    }

    fn workspace_unbind_actions(&self, input: &str) -> Vec<Action> {
        let filter = input.trim().to_ascii_lowercase();
        self.workspaces
            .snapshot()
            .into_iter()
            .filter_map(|workspace| {
                let label = workspace_label(&workspace);
                (filter.is_empty()
                    || label.to_ascii_lowercase().contains(&filter)
                    || workspace.id.to_ascii_lowercase().contains(&filter))
                .then(|| {
                    json_action(
                        format!("Clear desktop binding for workspace {label}"),
                        "MultiManager virtual desktop binding".into(),
                        "vd:unbind-workspace",
                        &VirtualDesktopWorkspacePayload {
                            workspace_id: workspace.id,
                            target: None,
                            cached_name: None,
                        },
                    )
                })
            })
            .collect()
    }
}

impl Plugin for VirtualDesktopPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let trimmed = query.trim();
        let Some(rest) = strip_command(trimmed, "vd") else {
            return Vec::new();
        };
        let snapshot = match self.desktops.snapshot() {
            Ok(snapshot) => snapshot,
            Err(error) => {
                return vec![diagnostic_action(
                    "Virtual desktops are unavailable",
                    format!("Could not enumerate Windows virtual desktops: {error}"),
                )];
            }
        };
        if let Some(row) = capability_diagnostic_for_query(rest, &snapshot) {
            return vec![row];
        }
        if rest.is_empty() || rest.eq_ignore_ascii_case("list") {
            return self.overview(&snapshot);
        }
        if rest.eq_ignore_ascii_case("current") {
            return snapshot
                .current()
                .ok()
                .map(|current| {
                    vec![simple_action(
                        format!(
                            "Current Desktop: {} — Desktop {}",
                            current.display_name(),
                            current.index
                        ),
                        "Current virtual desktop",
                        "vd:current",
                    )]
                })
                .unwrap_or_default();
        }
        if rest.eq_ignore_ascii_case("next") {
            return vec![simple_action("Switch Next", "Virtual Desktop", "vd:next")];
        }
        if rest.eq_ignore_ascii_case("previous") {
            return vec![simple_action(
                "Switch Previous",
                "Virtual Desktop",
                "vd:previous",
            )];
        }
        if rest.eq_ignore_ascii_case("create") {
            return vec![simple_action(
                "Create Virtual Desktop",
                "Virtual Desktop",
                "vd:create",
            )];
        }
        if rest.eq_ignore_ascii_case("close") || rest.eq_ignore_ascii_case("close current") {
            return vec![simple_action(
                "Close Current Virtual Desktop",
                "Virtual Desktop",
                "vd:close-current",
            )];
        }
        if rest.eq_ignore_ascii_case("settings") {
            return vec![simple_action(
                "Virtual Desktop Settings",
                "Virtual Desktop",
                "vd:settings",
            )];
        }
        if rest.eq_ignore_ascii_case("rules") {
            return vec![simple_action(
                "Virtual Desktop Auto-Switch Rules",
                "Open Virtual Desktop settings to configure application rules",
                "vd:settings",
            )];
        }
        if let Some(filter) = strip_command(rest, "switch") {
            return self
                .desktop_actions(&snapshot, filter, false)
                .into_iter()
                .filter(|row| row.action == "vd:switch")
                .collect();
        }
        if let Some(filter) = strip_command(rest, "move active") {
            return self.desktop_actions(&snapshot, filter, true);
        }
        if let Some(filter) = strip_command(rest, "windows details") {
            return self.window_detail_actions(&snapshot, filter);
        }
        if let Some(input) = strip_command(rest, "windows move") {
            return self.window_move_actions(&snapshot, input);
        }
        if let Some(filter) = strip_command(rest, "windows") {
            return self.window_actions(&snapshot, filter);
        }
        if let Some(input) = strip_command(rest, "launch") {
            return self.launch_actions(&snapshot, input);
        }
        if let Some(input) = strip_command(rest, "bind workspace") {
            return self.workspace_bind_actions(&snapshot, input);
        }
        if let Some(input) = strip_command(rest, "unbind workspace") {
            return self.workspace_unbind_actions(input);
        }
        if let Some(input) = strip_command(rest, "rename") {
            let Some((target, name)) = input.trim().split_once(char::is_whitespace) else {
                return Vec::new();
            };
            if name.trim().is_empty() {
                return Vec::new();
            }
            return vec![json_action(
                format!("Rename desktop {target} to {}", name.trim()),
                "Rename the actual Windows virtual desktop".into(),
                "vd:rename",
                &VirtualDesktopRenamePayload {
                    target: target.into(),
                    name: name.trim().into(),
                },
            )];
        }
        Vec::new()
    }

    fn name(&self) -> &str {
        "virtual_desktop"
    }
    fn description(&self) -> &str {
        "Manage Windows virtual desktops (prefix: `vd`)"
    }
    fn capabilities(&self) -> &[&str] {
        &["search"]
    }
    fn query_prefixes(&self) -> &[&str] {
        &["vd"]
    }
    fn commands(&self) -> Vec<Action> {
        [
            "vd",
            "vd list",
            "vd current",
            "vd switch",
            "vd next",
            "vd previous",
            "vd create",
            "vd close current",
            "vd rename",
            "vd windows",
            "vd move active",
            "vd launch",
            "vd bind workspace",
            "vd unbind workspace",
            "vd rules",
            "vd settings",
        ]
        .into_iter()
        .map(|query| query_action(query, format!("{query} ")))
        .collect()
    }
    fn default_settings(&self) -> Option<serde_json::Value> {
        serde_json::to_value(VirtualDesktopPluginSettings::default()).ok()
    }
    fn apply_settings(&mut self, value: &serde_json::Value) {
        if let Ok(settings) = serde_json::from_value::<VirtualDesktopPluginSettings>(value.clone())
        {
            if self.runtime_enabled {
                self.rule_runtime.reconcile(&settings.auto_switch_rules);
            }
            self.settings = settings;
        }
    }
    fn set_enabled(&mut self, enabled: bool) {
        self.runtime_enabled = enabled;
        self.rule_runtime.reconcile(if enabled {
            &self.settings.auto_switch_rules
        } else {
            &[]
        });
    }
    fn settings_ui(&mut self, ui: &mut egui::Ui, value: &mut serde_json::Value) {
        let mut settings: VirtualDesktopPluginSettings =
            serde_json::from_value(value.clone()).unwrap_or_default();
        ui.label("Launch window discovery timeout");
        ui.add(egui::Slider::new(&mut settings.launch_timeout_ms, 1_000..=30_000).suffix(" ms"));
        ui.separator();
        ui.heading("Application auto-switch rules");
        ui.small("Rules are opt-in and react only when a matching application becomes foreground. Empty or fully disabled rules start no background runtime.");
        if ui.button("Refresh desktops").clicked() || self.settings_desktops.is_none() {
            self.settings_desktops = Some(self.desktops.snapshot());
        }
        let desktop_snapshot = self
            .settings_desktops
            .clone()
            .unwrap_or_else(|| Err("Desktop list has not been loaded".into()));
        if let Err(error) = &desktop_snapshot {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                format!("Desktop list unavailable: {error}"),
            );
        }
        let mut remove = None;
        for (index, rule) in settings.auto_switch_rules.iter_mut().enumerate() {
            ui.push_id((index, &rule.id), |ui| {
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut rule.enabled, "Enabled");
                        if ui.button("Remove").clicked() {
                            remove = Some(index);
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Application/process");
                        ui.text_edit_singleline(&mut rule.executable);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Process path (optional)");
                        ui.text_edit_singleline(&mut rule.process_path);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Window title contains (optional)");
                        ui.text_edit_singleline(&mut rule.title);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Window class (optional)");
                        ui.text_edit_singleline(&mut rule.class_name);
                    });
                    let selected = rule
                        .target
                        .as_ref()
                        .map(|binding| {
                            binding
                                .cached_name
                                .clone()
                                .unwrap_or_else(|| binding.id.to_string())
                        })
                        .unwrap_or_else(|| "Select desktop".into());
                    egui::ComboBox::from_label("Target desktop")
                        .selected_text(selected)
                        .show_ui(ui, |ui| {
                            if let Ok(snapshot) = &desktop_snapshot {
                                for desktop in &snapshot.desktops {
                                    let selected = rule
                                        .target
                                        .as_ref()
                                        .is_some_and(|binding| binding.id == desktop.id);
                                    if ui
                                        .selectable_label(
                                            selected,
                                            format!(
                                                "Desktop {} — {}",
                                                desktop.index,
                                                desktop.display_name()
                                            ),
                                        )
                                        .clicked()
                                    {
                                        rule.target =
                                            Some(crate::virtual_desktop::VirtualDesktopBinding {
                                                id: desktop.id.clone(),
                                                cached_name: desktop.name.clone(),
                                            });
                                    }
                                }
                            }
                        });
                    match (&rule.target, &desktop_snapshot) {
                        (None, _) => {
                            ui.colored_label(
                                ui.visuals().warn_fg_color,
                                "No target desktop selected",
                            );
                        }
                        (Some(binding), Ok(snapshot)) => {
                            if let Err(error) = snapshot.resolve_binding(binding) {
                                ui.colored_label(
                                    ui.visuals().error_fg_color,
                                    format!("Stale target: {error}"),
                                );
                            }
                        }
                        _ => {}
                    }
                });
            });
        }
        if let Some(index) = remove {
            settings.auto_switch_rules.remove(index);
        }
        if ui.button("Add rule").clicked() {
            let id = next_rule_id(&settings.auto_switch_rules);
            let target = desktop_snapshot
                .as_ref()
                .ok()
                .and_then(|snapshot| snapshot.current().ok())
                .map(|desktop| crate::virtual_desktop::VirtualDesktopBinding {
                    id: desktop.id.clone(),
                    cached_name: desktop.name.clone(),
                });
            settings.auto_switch_rules.push(VirtualDesktopRule {
                id,
                target,
                ..VirtualDesktopRule::default()
            });
        }
        match (self.runtime_enabled, self.rule_runtime.status()) {
            (false, _) => {
                ui.small("Auto-switch runtime: plugin disabled");
            }
            (true, RuleRuntimeStatus::Disabled) => {
                ui.small("Auto-switch runtime: disabled (no enabled rules)");
            }
            (true, RuleRuntimeStatus::Running) => {
                ui.small("Auto-switch runtime: listening for foreground changes");
            }
            (true, RuleRuntimeStatus::Unavailable) => {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    "Auto-switch runtime could not install the Windows foreground hook",
                );
            }
        }
        *value = serde_json::to_value(&settings).unwrap_or_default();
    }
}

fn next_rule_id(existing: &[VirtualDesktopRule]) -> String {
    (1u64..)
        .map(|number| format!("vd-rule-{number}"))
        .find(|candidate| existing.iter().all(|rule| rule.id != *candidate))
        .expect("virtual desktop rule id space exhausted")
}

fn strip_command<'a>(input: &'a str, command: &str) -> Option<&'a str> {
    if input.eq_ignore_ascii_case(command) {
        Some("")
    } else if input
        .get(..command.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(command))
        && input
            .get(command.len()..)
            .and_then(|tail| tail.chars().next())
            .is_some_and(char::is_whitespace)
    {
        Some(input.get(command.len()..)?.trim())
    } else {
        None
    }
}

fn diagnostic_action(label: impl Into<String>, detail: impl Into<String>) -> Action {
    Action {
        label: label.into(),
        desc: detail.into(),
        action: "query:vd ".into(),
        args: None,
    }
}

fn capability_diagnostics(snapshot: &VirtualDesktopSnapshot) -> Vec<Action> {
    let capabilities = [
        (snapshot.capabilities.direct_switching, "desktop switching"),
        (snapshot.capabilities.creation, "desktop creation"),
        (snapshot.capabilities.closing, "desktop closing"),
        (snapshot.capabilities.renaming, "desktop renaming"),
        (snapshot.capabilities.window_membership, "window membership"),
        (
            snapshot.capabilities.window_movement,
            "external window movement",
        ),
    ];
    capabilities
        .into_iter()
        .filter_map(|(available, name)| {
            (!available).then(|| {
                diagnostic_action(
                    format!("Unavailable: {name}"),
                    format!("This Windows build does not expose a verified ABI for {name}"),
                )
            })
        })
        .collect()
}

fn capability_diagnostic_for_query(
    query: &str,
    snapshot: &VirtualDesktopSnapshot,
) -> Option<Action> {
    let query = query.trim();
    let required = if query.eq_ignore_ascii_case("next")
        || query.eq_ignore_ascii_case("previous")
        || strip_command(query, "switch").is_some()
    {
        Some((snapshot.capabilities.direct_switching, "desktop switching"))
    } else if query.eq_ignore_ascii_case("create") {
        Some((snapshot.capabilities.creation, "desktop creation"))
    } else if query.eq_ignore_ascii_case("close") || query.eq_ignore_ascii_case("close current") {
        Some((snapshot.capabilities.closing, "desktop closing"))
    } else if strip_command(query, "rename").is_some() {
        Some((snapshot.capabilities.renaming, "desktop renaming"))
    } else if strip_command(query, "move active").is_some()
        || strip_command(query, "windows move").is_some()
        || strip_command(query, "launch").is_some()
    {
        Some((
            snapshot.capabilities.window_movement,
            "external window movement",
        ))
    } else if strip_command(query, "windows").is_some() {
        Some((snapshot.capabilities.window_membership, "window membership"))
    } else {
        None
    };
    required.and_then(|(available, capability)| {
        (!available).then(|| {
            diagnostic_action(
                format!("Unavailable: {capability}"),
                format!("This Windows build does not expose a verified ABI for {capability}"),
            )
        })
    })
}

fn workspace_label(
    workspace: &crate::multi_manager::workspace_catalog::WorkspaceDescriptor,
) -> String {
    let name = workspace.name.trim();
    if name.is_empty() {
        workspace.id.clone()
    } else {
        name.to_string()
    }
}

fn is_direct_executable_target(target: &str) -> bool {
    std::path::Path::new(target.trim_matches('"'))
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
}

fn workspace_target_remainder<'a>(input: &'a str, workspace: &str) -> Option<&'a str> {
    if input.len() <= workspace.len()
        || !input
            .get(..workspace.len())?
            .eq_ignore_ascii_case(workspace)
        || !input.as_bytes().get(workspace.len())?.is_ascii_whitespace()
    {
        return None;
    }
    Some(input[workspace.len()..].trim())
}

fn simple_action(label: impl Into<String>, desc: impl Into<String>, action: &str) -> Action {
    Action {
        label: label.into(),
        desc: desc.into(),
        action: action.into(),
        args: None,
    }
}

fn query_action(label: impl Into<String>, query: impl Into<String>) -> Action {
    Action {
        label: label.into(),
        desc: "Virtual Desktop".into(),
        action: format!("query:{}", query.into()),
        args: None,
    }
}

fn json_action<T: Serialize>(
    label: impl Into<String>,
    desc: String,
    action: &str,
    payload: &T,
) -> Action {
    Action {
        label: label.into(),
        desc,
        action: action.into(),
        args: serde_json::to_string(payload).ok(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::VirtualDesktopCommand;
    use crate::virtual_desktop::rules::{RuleRuntimeFactory, RuleRuntimeHandle};
    use crate::virtual_desktop::{
        VirtualDesktopCapabilities, VirtualDesktopId, VirtualDesktopInfo, VirtualDesktopSelector,
    };
    use crate::window_catalog::WindowDescriptor;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct FakeDesktops {
        snapshot: VirtualDesktopSnapshot,
        snapshots: AtomicUsize,
    }

    struct FakeWorkspaces(Vec<crate::multi_manager::workspace_catalog::WorkspaceDescriptor>);
    struct FakeWindowDetails {
        windows: std::collections::HashMap<usize, WindowDescriptor>,
        calls: AtomicUsize,
    }
    impl WindowDetailsSource for FakeWindowDetails {
        fn describe(&self, hwnd: usize) -> Option<WindowDescriptor> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            self.windows.get(&hwnd).cloned()
        }
    }

    #[derive(Default)]
    struct RuleRuntimeCounts {
        starts: AtomicUsize,
        shutdowns: AtomicUsize,
    }

    struct FakeRuleRuntimeFactory(Arc<RuleRuntimeCounts>);
    struct FakeRuleRuntime(Arc<RuleRuntimeCounts>);

    impl RuleRuntimeFactory for FakeRuleRuntimeFactory {
        fn start(&self, _: Arc<Vec<VirtualDesktopRule>>) -> Option<Box<dyn RuleRuntimeHandle>> {
            self.0.starts.fetch_add(1, Ordering::SeqCst);
            Some(Box::new(FakeRuleRuntime(Arc::clone(&self.0))))
        }
    }

    impl RuleRuntimeHandle for FakeRuleRuntime {
        fn shutdown(self: Box<Self>) {
            self.0.shutdowns.fetch_add(1, Ordering::SeqCst);
        }
    }
    impl WorkspaceSource for FakeWorkspaces {
        fn snapshot(&self) -> Vec<crate::multi_manager::workspace_catalog::WorkspaceDescriptor> {
            self.0.clone()
        }
    }

    impl DesktopSource for FakeDesktops {
        fn snapshot(&self) -> Result<VirtualDesktopSnapshot, String> {
            self.snapshots.fetch_add(1, Ordering::Relaxed);
            Ok(self.snapshot.clone())
        }
    }

    fn id(number: u32) -> VirtualDesktopId {
        VirtualDesktopId::parse(&format!("{number:08x}-0000-0000-0000-000000000000")).unwrap()
    }

    fn source() -> Arc<FakeDesktops> {
        Arc::new(FakeDesktops {
            snapshot: VirtualDesktopSnapshot {
                desktops: ["Personal", "Coding", "Gaming"]
                    .into_iter()
                    .enumerate()
                    .map(|(index, name)| VirtualDesktopInfo {
                        id: id(index as u32 + 1),
                        index: index as u32 + 1,
                        name: Some(name.into()),
                        is_current: index == 1,
                    })
                    .collect(),
                capabilities: VirtualDesktopCapabilities {
                    enumeration: true,
                    direct_switching: true,
                    creation: true,
                    closing: true,
                    renaming: true,
                    window_membership: true,
                    window_movement: true,
                },
            },
            snapshots: AtomicUsize::new(0),
        })
    }

    fn plugin(source: Arc<FakeDesktops>) -> VirtualDesktopPlugin {
        plugin_with_workspaces(
            source,
            vec![
                ("workspace-42", "Coding"),
                ("workspace-gaming", "Gaming Space"),
            ],
        )
    }

    fn plugin_with_workspaces(
        source: Arc<FakeDesktops>,
        workspaces: Vec<(&str, &str)>,
    ) -> VirtualDesktopPlugin {
        VirtualDesktopPlugin::with_source(
            source,
            Arc::new(FakeWorkspaces(
                workspaces
                    .into_iter()
                    .map(|(id, name)| {
                        crate::multi_manager::workspace_catalog::WorkspaceDescriptor {
                            id: id.into(),
                            name: name.into(),
                        }
                    })
                    .collect(),
            )),
            WindowCatalog::from_enriched_snapshot(
                vec![WindowDescriptor {
                    title: "Code: project | alpha".into(),
                    hwnd: 42,
                    pid: 7,
                    executable: Some("code.exe".into()),
                    process_path: Some("C:\\Tools\\code.exe".into()),
                    class_name: Some("Chrome_WidgetWin_1".into()),
                }],
                std::collections::HashMap::from([(42, Some(id(2)))]),
            ),
            Arc::new(vec![Action {
                label: "Text Editor".into(),
                desc: "Apps".into(),
                action: "C:\\Tools\\editor.exe".into(),
                args: Some("--name 'a | b'".into()),
            }]),
        )
    }

    #[test]
    fn prefix_is_case_insensitive_token_bounded_and_non_vd_does_no_work() {
        let source = source();
        let plugin = plugin(Arc::clone(&source));
        assert!(plugin.search("files").is_empty());
        assert!(plugin.search("vds").is_empty());
        assert!(plugin.search("🔥vd").is_empty());
        assert!(plugin.search("é").is_empty());
        assert_eq!(source.snapshots.load(Ordering::Relaxed), 0);
        assert!(!plugin.search("VD").is_empty());
        assert_eq!(source.snapshots.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn strip_command_is_utf8_safe_and_token_bounded() {
        assert_eq!(strip_command("VD 🔥", "vd"), Some("🔥"));
        assert_eq!(strip_command("rename 工作", "rename"), Some("工作"));
        assert_eq!(strip_command("🔥vd", "vd"), None);
        assert_eq!(strip_command("vdesktop", "vd"), None);
    }

    #[test]
    fn overview_and_filters_expose_current_switch_move_create_and_settings() {
        let plugin = plugin(source());
        let overview = plugin.search("vd");
        for expected in [
            "Current Desktop: Coding",
            "Create Virtual Desktop",
            "Switch Previous",
            "Switch Next",
            "Move Active Window to Gaming",
            "Virtual Desktop Settings",
        ] {
            assert!(
                overview.iter().any(|row| row.label.contains(expected)),
                "{expected}"
            );
        }
        let switch = plugin.search("vd switch cod");
        assert_eq!(switch.len(), 1);
        assert!(
            matches!(crate::commands::parse_action(&switch[0]).unwrap(), Command::VirtualDesktop(VirtualDesktopCommand::Switch { target }) if target == id(2).to_string())
        );
        assert_eq!(plugin.search("vd switch 3").len(), 1);
        assert_eq!(plugin.search(&format!("vd switch {}", id(1))).len(), 1);
        let moving = plugin.search("vd move active gam");
        assert_eq!(moving.len(), 2);
        assert!(moving.iter().any(|row| matches!(
            crate::commands::parse_action(row).unwrap(),
            Command::VirtualDesktop(VirtualDesktopCommand::MoveActiveWindow { follow: false, .. })
        )));
        assert!(moving.iter().any(|row| matches!(
            crate::commands::parse_action(row).unwrap(),
            Command::VirtualDesktop(VirtualDesktopCommand::MoveActiveWindow { follow: true, .. })
        )));
    }

    #[test]
    fn window_results_label_desktop_and_keep_activate_move_follow_distinct() {
        let plugin = plugin(source());
        let rows = plugin.search("vd windows code");
        assert!(rows.iter().any(|row| row.desc.contains("Coding")));
        assert!(rows.iter().any(|row| matches!(
            crate::commands::parse_action(row).unwrap(),
            Command::VirtualDesktop(VirtualDesktopCommand::ActivateWindow { hwnd: 42 })
        )));
        assert!(
            rows.len() <= 2,
            "window search stays bounded before move expansion"
        );
        let moves = plugin.search("vd windows move 42");
        assert!(moves.iter().any(|row| matches!(
            crate::commands::parse_action(row).unwrap(),
            Command::VirtualDesktop(VirtualDesktopCommand::MoveWindow { follow: false, .. })
        )));
        assert!(moves.iter().any(|row| matches!(
            crate::commands::parse_action(row).unwrap(),
            Command::VirtualDesktop(VirtualDesktopCommand::MoveWindow { follow: true, .. })
        )));
    }

    #[test]
    fn executable_filter_is_explicitly_staged_cached_and_keeps_baseline_minimal() {
        let mut plugin = plugin(source());
        plugin.catalog = WindowCatalog::from_enriched_snapshot(
            vec![WindowDescriptor {
                title: "Unrelated title".into(),
                hwnd: 77,
                pid: 7,
                executable: None,
                process_path: None,
                class_name: None,
            }],
            std::collections::HashMap::from([(77, Some(id(2)))]),
        );
        let details = Arc::new(FakeWindowDetails {
            windows: std::collections::HashMap::from([(
                77,
                WindowDescriptor {
                    title: "Unrelated title".into(),
                    hwnd: 77,
                    pid: 7,
                    executable: Some("editor.exe".into()),
                    process_path: Some("C:\\Tools\\editor.exe".into()),
                    class_name: Some("EditorWindow".into()),
                },
            )]),
            calls: AtomicUsize::new(0),
        });
        plugin.window_details = details.clone();

        let baseline = plugin.search("vd windows editor");
        assert_eq!(details.calls.load(Ordering::Relaxed), 0);
        assert_eq!(baseline.len(), 1);
        assert_eq!(baseline[0].action, "query:vd windows details editor");

        let detailed = plugin.search("vd windows details editor");
        assert_eq!(detailed.len(), 2);
        assert_eq!(details.calls.load(Ordering::Relaxed), 1);
        let repeated = plugin.search("vd windows details editor");
        assert_eq!(repeated.len(), 2);
        assert_eq!(details.calls.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn rename_and_launch_preserve_arbitrary_text_in_json_payloads() {
        let plugin = plugin(source());
        let rename = plugin.search("vd rename 2 Work | Focus");
        assert!(
            matches!(crate::commands::parse_action(&rename[0]).unwrap(), Command::VirtualDesktop(VirtualDesktopCommand::Rename { target, name }) if target == "2" && name == "Work | Focus")
        );
        let launch = plugin.search(&format!("vd launch {} editor", id(3)));
        assert_eq!(launch.len(), 2);
        assert!(launch.iter().any(|row|
            matches!(crate::commands::parse_action(row).unwrap(), Command::VirtualDesktop(VirtualDesktopCommand::Launch(payload)) if payload.args.as_deref() == Some("--name 'a | b'") && payload.target == id(3).to_string() && !payload.follow)
        ));
        assert!(launch.iter().any(|row|
            matches!(crate::commands::parse_action(row).unwrap(), Command::VirtualDesktop(VirtualDesktopCommand::Launch(payload)) if payload.follow)
        ));
    }

    #[test]
    fn launch_discovery_excludes_shortcuts_documents_urls_and_namespaced_actions() {
        let mut plugin = plugin(source());
        plugin.actions = Arc::new(vec![
            Action {
                label: "Executable".into(),
                desc: String::new(),
                action: "C:\\Tools\\app.exe".into(),
                args: None,
            },
            Action {
                label: "Shortcut".into(),
                desc: String::new(),
                action: "C:\\Links\\app.lnk".into(),
                args: None,
            },
            Action {
                label: "Document".into(),
                desc: String::new(),
                action: "C:\\Docs\\notes.txt".into(),
                args: None,
            },
            Action {
                label: "URL".into(),
                desc: String::new(),
                action: "https://example.com".into(),
                args: None,
            },
            Action {
                label: "Namespaced".into(),
                desc: String::new(),
                action: "shell:C:\\Tools\\app.exe".into(),
                args: None,
            },
        ]);
        let rows = plugin.search("vd launch 2");
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|row| row.label.contains("Executable")));
    }

    #[test]
    fn snapshot_failures_surface_an_actionable_diagnostic_row() {
        struct ErrorSource;
        impl DesktopSource for ErrorSource {
            fn snapshot(&self) -> Result<VirtualDesktopSnapshot, String> {
                Err("unsupported Windows build 99999".into())
            }
        }
        let plugin = VirtualDesktopPlugin::with_source(
            Arc::new(ErrorSource),
            Arc::new(FakeWorkspaces(Vec::new())),
            WindowCatalog::from_snapshot(Vec::new()),
            Arc::new(Vec::new()),
        );
        let rows = plugin.search("vd");
        assert_eq!(rows.len(), 1);
        assert!(rows[0].label.contains("unavailable"));
        assert!(rows[0].desc.contains("unsupported Windows build 99999"));
        assert_eq!(rows[0].action, "query:vd ");
    }

    #[test]
    fn unsupported_capability_surfaces_diagnostic_instead_of_dead_action() {
        let source = source();
        let mut unavailable = source.snapshot.clone();
        unavailable.capabilities.window_movement = false;
        let plugin = plugin(Arc::new(FakeDesktops {
            snapshot: unavailable,
            snapshots: AtomicUsize::new(0),
        }));
        let rows = plugin.search("vd move active 2");
        assert_eq!(rows.len(), 1);
        assert!(rows[0].label.contains("Unavailable"));
        assert!(rows[0].desc.contains("verified ABI"));
    }

    #[test]
    fn command_discovery_and_default_settings_are_registered() {
        let plugin = plugin(source());
        let commands = plugin.commands();
        for expected in [
            "vd",
            "vd list",
            "vd current",
            "vd switch",
            "vd windows",
            "vd move active",
            "vd launch",
            "vd bind workspace",
            "vd unbind workspace",
            "vd rules",
            "vd settings",
        ] {
            assert!(
                commands.iter().any(|action| action.label == expected),
                "{expected}"
            );
        }
        let settings: VirtualDesktopPluginSettings =
            serde_json::from_value(plugin.default_settings().unwrap()).unwrap();
        assert_eq!(settings.launch_timeout_ms, 10_000);
        assert!(settings.auto_switch_rules.is_empty());
        assert_eq!(plugin.search("vd rules")[0].action, "vd:settings");
    }

    #[test]
    fn legacy_settings_default_to_no_rule_runtime_and_rules_round_trip_bindings() {
        let legacy: VirtualDesktopPluginSettings =
            serde_json::from_value(serde_json::json!({ "launch_timeout_ms": 5000 })).unwrap();
        assert_eq!(legacy.launch_timeout_ms, 5000);
        assert!(legacy.auto_switch_rules.is_empty());

        let settings = VirtualDesktopPluginSettings {
            auto_switch_rules: vec![VirtualDesktopRule {
                id: "editor-work".into(),
                enabled: true,
                executable: "editor.exe".into(),
                target: Some(crate::virtual_desktop::VirtualDesktopBinding {
                    id: id(1),
                    cached_name: Some("Personal".into()),
                }),
                ..VirtualDesktopRule::default()
            }],
            ..VirtualDesktopPluginSettings::default()
        };
        let round_trip: VirtualDesktopPluginSettings =
            serde_json::from_value(serde_json::to_value(&settings).unwrap()).unwrap();
        assert_eq!(round_trip, settings);
        assert!(round_trip.auto_switch_rules[0].enabled);
    }

    #[test]
    fn rule_runtime_requires_plugin_enablement_and_stops_on_disable_and_drop() {
        let counts = Arc::new(RuleRuntimeCounts::default());
        {
            let mut plugin = plugin(source());
            plugin.rule_runtime =
                RuleRuntimeController::new(Arc::new(FakeRuleRuntimeFactory(Arc::clone(&counts))));
            let settings = VirtualDesktopPluginSettings {
                auto_switch_rules: vec![VirtualDesktopRule {
                    id: "editor".into(),
                    enabled: true,
                    executable: "editor.exe".into(),
                    target: Some(crate::virtual_desktop::VirtualDesktopBinding {
                        id: id(1),
                        cached_name: Some("Personal".into()),
                    }),
                    ..VirtualDesktopRule::default()
                }],
                ..VirtualDesktopPluginSettings::default()
            };
            plugin.apply_settings(&serde_json::to_value(settings).unwrap());
            assert_eq!(counts.starts.load(Ordering::SeqCst), 0);

            plugin.set_enabled(true);
            assert_eq!(counts.starts.load(Ordering::SeqCst), 1);
            let committed = plugin.settings.clone();
            let mut unsaved = committed.clone();
            unsaved.auto_switch_rules[0].executable = "unsaved.exe".into();
            let mut draft_value = serde_json::to_value(unsaved).unwrap();
            let context = egui::Context::default();
            let _ = context.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    plugin.settings_ui(ui, &mut draft_value);
                });
            });
            assert_eq!(plugin.settings, committed);
            assert_eq!(counts.starts.load(Ordering::SeqCst), 1);
            assert_eq!(counts.shutdowns.load(Ordering::SeqCst), 0);

            plugin.set_enabled(false);
            assert_eq!(counts.shutdowns.load(Ordering::SeqCst), 1);
            plugin.set_enabled(true);
            assert_eq!(counts.starts.load(Ordering::SeqCst), 2);
        }
        assert_eq!(counts.shutdowns.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn settings_desktop_dropdown_is_on_demand_not_polled_each_frame() {
        let source = source();
        let mut plugin = plugin(Arc::clone(&source));
        let mut value = plugin.default_settings().unwrap();
        let context = egui::Context::default();
        for _ in 0..2 {
            let _ = context.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    plugin.settings_ui(ui, &mut value);
                });
            });
        }
        assert_eq!(source.snapshots.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn workspace_binding_actions_use_stable_workspace_id_and_resolved_desktop_guid() {
        let plugin = plugin(source());
        let discovery = plugin.search("vd bind workspace cod");
        assert_eq!(discovery.len(), 1);
        assert_eq!(discovery[0].action, "query:vd bind workspace Coding ");
        let bind = plugin.search("vd bind workspace Coding Coding");
        assert_eq!(bind.len(), 1);
        assert!(matches!(
            crate::commands::parse_action(&bind[0]).unwrap(),
            Command::VirtualDesktop(VirtualDesktopCommand::BindWorkspace { workspace_id, target, .. })
                if workspace_id == "workspace-42" && target == id(2).to_string()
        ));
        let unbind = plugin.search("vd unbind workspace Coding");
        assert_eq!(unbind.len(), 1);
        assert!(matches!(
            crate::commands::parse_action(&unbind[0]).unwrap(),
            Command::VirtualDesktop(VirtualDesktopCommand::UnbindWorkspace { workspace_id })
                if workspace_id == "workspace-42"
        ));
    }

    #[test]
    fn production_workspace_sources_keep_app_instances_isolated() {
        fn workspace(
            id: &str,
        ) -> Arc<std::sync::Mutex<Vec<crate::multi_manager::model::MmWorkspace>>> {
            Arc::new(std::sync::Mutex::new(vec![
                crate::multi_manager::model::MmWorkspace {
                    id: id.into(),
                    name: "Coding".into(),
                    ..crate::multi_manager::model::MmWorkspace::default()
                },
            ]))
        }

        fn plugin_for_catalog(
            catalog: Arc<crate::multi_manager::workspace_catalog::WorkspaceCatalog>,
        ) -> VirtualDesktopPlugin {
            VirtualDesktopPlugin::with_source(
                source(),
                Arc::new(ProductionWorkspaceSource { catalog }),
                WindowCatalog::from_enriched_snapshot(Vec::new(), Default::default()),
                Arc::new(Vec::new()),
            )
        }

        let first_workspaces = workspace("workspace-first");
        let second_workspaces = workspace("workspace-second");
        let first_catalog =
            Arc::new(crate::multi_manager::workspace_catalog::WorkspaceCatalog::default());
        let second_catalog =
            Arc::new(crate::multi_manager::workspace_catalog::WorkspaceCatalog::default());
        first_catalog.attach(&first_workspaces);
        second_catalog.attach(&second_workspaces);
        let first = plugin_for_catalog(first_catalog);
        let second = plugin_for_catalog(second_catalog);

        let first_bind = first.search("vd bind workspace Coding Coding");
        let second_bind = second.search("vd bind workspace Coding Coding");
        assert!(matches!(
            crate::commands::parse_action(&first_bind[0]).unwrap(),
            Command::VirtualDesktop(VirtualDesktopCommand::BindWorkspace { workspace_id, .. })
                if workspace_id == "workspace-first"
        ));
        assert!(matches!(
            crate::commands::parse_action(&second_bind[0]).unwrap(),
            Command::VirtualDesktop(VirtualDesktopCommand::BindWorkspace { workspace_id, .. })
                if workspace_id == "workspace-second"
        ));
    }

    #[test]
    fn duplicate_workspace_names_do_not_bind_ambiguously() {
        let plugin = plugin_with_workspaces(
            source(),
            vec![("workspace-a", "Coding"), ("workspace-b", "Coding")],
        );
        assert!(plugin.search("vd bind workspace Coding Coding").is_empty());
        let by_id = plugin.search("vd bind workspace workspace-a Coding");
        assert!(matches!(
            crate::commands::parse_action(&by_id[0]).unwrap(),
            Command::VirtualDesktop(VirtualDesktopCommand::BindWorkspace { workspace_id, .. })
                if workspace_id == "workspace-a"
        ));
    }
}
