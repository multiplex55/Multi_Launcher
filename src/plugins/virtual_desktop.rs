use crate::actions::Action;
use crate::commands::{
    Command, ExternalCommand, VirtualDesktopLaunchPayload, VirtualDesktopMoveActivePayload,
    VirtualDesktopMoveWindowPayload, VirtualDesktopRenamePayload, VirtualDesktopTargetPayload,
    VirtualDesktopWindowPayload,
};
use crate::plugin::Plugin;
use crate::virtual_desktop::{VirtualDesktopService, VirtualDesktopSnapshot};
use crate::window_catalog::WindowCatalog;
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct VirtualDesktopPluginSettings {
    pub launch_timeout_ms: u64,
}

impl Default for VirtualDesktopPluginSettings {
    fn default() -> Self {
        Self {
            launch_timeout_ms: 10_000,
        }
    }
}

trait DesktopSource: Send + Sync {
    fn snapshot(&self) -> Result<VirtualDesktopSnapshot, String>;
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
    catalog: Arc<WindowCatalog>,
    actions: Arc<Vec<Action>>,
    settings: VirtualDesktopPluginSettings,
}

impl VirtualDesktopPlugin {
    pub(crate) fn new(catalog: Arc<WindowCatalog>, actions: Arc<Vec<Action>>) -> Self {
        Self {
            desktops: Arc::new(ProductionDesktopSource),
            catalog,
            actions,
            settings: VirtualDesktopPluginSettings::default(),
        }
    }

    #[cfg(test)]
    fn with_source(
        desktops: Arc<dyn DesktopSource>,
        catalog: Arc<WindowCatalog>,
        actions: Arc<Vec<Action>>,
    ) -> Self {
        Self {
            desktops,
            catalog,
            actions,
            settings: VirtualDesktopPluginSettings::default(),
        }
    }

    fn overview(&self, snapshot: &VirtualDesktopSnapshot) -> Vec<Action> {
        let mut rows = Vec::new();
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
        rows.push(simple_action(
            "Create Virtual Desktop",
            "Virtual Desktop",
            "vd:create",
        ));
        rows.push(simple_action(
            "Switch Previous",
            "Virtual Desktop",
            "vd:previous",
        ));
        rows.push(simple_action("Switch Next", "Virtual Desktop", "vd:next"));
        rows.extend(self.desktop_actions(snapshot, "", false));
        rows.push(query_action(
            "Search Windows Across Desktops",
            "vd windows ",
        ));
        rows.push(simple_action(
            "Virtual Desktop Settings",
            "Virtual Desktop",
            "vd:settings",
        ));
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
            if !move_only {
                rows.push(json_action(
                    format!("Switch to {}", desktop.display_name()),
                    format!("Desktop {}", desktop.index),
                    "vd:switch",
                    &VirtualDesktopTargetPayload {
                        target: target.clone(),
                    },
                ));
            }
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
}

impl Plugin for VirtualDesktopPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let trimmed = query.trim();
        if trimmed.len() < 2
            || !trimmed[..2].eq_ignore_ascii_case("vd")
            || trimmed
                .as_bytes()
                .get(2)
                .is_some_and(|byte| !byte.is_ascii_whitespace())
        {
            return Vec::new();
        }
        let Ok(snapshot) = self.desktops.snapshot() else {
            return Vec::new();
        };
        let rest = trimmed[2..].trim();
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
        if let Some(input) = strip_command(rest, "windows move") {
            return self.window_move_actions(&snapshot, input);
        }
        if let Some(filter) = strip_command(rest, "windows") {
            return self.window_actions(&snapshot, filter);
        }
        if let Some(input) = strip_command(rest, "launch") {
            return self.launch_actions(&snapshot, input);
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
        if let Ok(settings) = serde_json::from_value(value.clone()) {
            self.settings = settings;
        }
    }
    fn settings_ui(&mut self, ui: &mut egui::Ui, value: &mut serde_json::Value) {
        let mut settings: VirtualDesktopPluginSettings =
            serde_json::from_value(value.clone()).unwrap_or_default();
        ui.label("Launch window discovery timeout");
        ui.add(egui::Slider::new(&mut settings.launch_timeout_ms, 1_000..=30_000).suffix(" ms"));
        *value = serde_json::to_value(&settings).unwrap_or_default();
        self.settings = settings;
    }
}

fn strip_command<'a>(input: &'a str, command: &str) -> Option<&'a str> {
    if input.eq_ignore_ascii_case(command) {
        Some("")
    } else if input.len() > command.len()
        && input[..command.len()].eq_ignore_ascii_case(command)
        && input.as_bytes()[command.len()].is_ascii_whitespace()
    {
        Some(input[command.len()..].trim())
    } else {
        None
    }
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
    use crate::virtual_desktop::{
        VirtualDesktopCapabilities, VirtualDesktopId, VirtualDesktopInfo, VirtualDesktopSelector,
    };
    use crate::window_catalog::WindowDescriptor;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct FakeDesktops {
        snapshot: VirtualDesktopSnapshot,
        snapshots: AtomicUsize,
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
                capabilities: VirtualDesktopCapabilities::default(),
            },
            snapshots: AtomicUsize::new(0),
        })
    }

    fn plugin(source: Arc<FakeDesktops>) -> VirtualDesktopPlugin {
        VirtualDesktopPlugin::with_source(
            source,
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
        assert_eq!(source.snapshots.load(Ordering::Relaxed), 0);
        assert!(!plugin.search("VD").is_empty());
        assert_eq!(source.snapshots.load(Ordering::Relaxed), 1);
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
    }
}
