use crate::actions::Action;
use crate::plugin::Plugin;
use crate::plugins::layouts_storage::{
    get_layout, layouts_config_path, load_layouts, LayoutMatch,
};

#[derive(Default, Debug, Clone)]
struct LayoutFlags {
    dry_run: bool,
    no_launch: bool,
    only_active_monitor: bool,
    include_minimized: bool,
    exclude_minimized: bool,
    filter: Option<String>,
    only_groups: Vec<String>,
}

impl LayoutFlags {
    fn is_empty(&self) -> bool {
        !self.dry_run
            && !self.no_launch
            && !self.only_active_monitor
            && !self.include_minimized
            && !self.exclude_minimized
            && self.filter.is_none()
            && self.only_groups.is_empty()
    }

    fn serialize(&self) -> String {
        let mut parts = Vec::new();
        if self.dry_run {
            parts.push("dry_run".to_string());
        }
        if self.no_launch {
            parts.push("no_launch".to_string());
        }
        if self.only_active_monitor {
            parts.push("only_active_monitor".to_string());
        }
        if self.include_minimized {
            parts.push("include_minimized".to_string());
        }
        if self.exclude_minimized {
            parts.push("exclude_minimized".to_string());
        }
        if let Some(filter) = &self.filter {
            parts.push(format!("filter={filter}"));
        }
        for group in &self.only_groups {
            parts.push(format!("only={group}"));
        }
        parts.join(",")
    }
}

fn parse_name_and_flags(input: &str) -> (String, LayoutFlags) {
    let mut name_parts = Vec::new();
    let mut flags = LayoutFlags::default();
    let mut iter = input.split_whitespace().peekable();
    while let Some(token) = iter.next() {
        if token == "--dry-run" {
            flags.dry_run = true;
            continue;
        }
        if token == "--no-launch" {
            flags.no_launch = true;
            continue;
        }
        if token == "--only-active-monitor" {
            flags.only_active_monitor = true;
            continue;
        }
        if token == "--include-minimized" {
            flags.include_minimized = true;
            continue;
        }
        if token == "--exclude-minimized" {
            flags.exclude_minimized = true;
            continue;
        }
        if token == "--filter" {
            if let Some(value) = iter.next() {
                flags.filter = Some(value.to_string());
            }
            continue;
        }
        if let Some(value) = token.strip_prefix("--filter=") {
            flags.filter = Some(value.to_string());
            continue;
        }
        if token == "--only" {
            if let Some(value) = iter.next() {
                for entry in value.split(',').map(str::trim).filter(|v| !v.is_empty()) {
                    flags.only_groups.push(entry.to_string());
                }
            }
            continue;
        }
        if let Some(value) = token.strip_prefix("--only=") {
            for entry in value.split(',').map(str::trim).filter(|v| !v.is_empty()) {
                flags.only_groups.push(entry.to_string());
            }
            continue;
        }
        name_parts.push(token.to_string());
    }
    (name_parts.join(" "), flags)
}

fn build_action(action: String, flags: &LayoutFlags) -> String {
    if flags.is_empty() {
        action
    } else {
        format!("{action}|{}", flags.serialize())
    }
}

fn format_match(matcher: &LayoutMatch) -> String {
    let mut parts = Vec::new();
    if let Some(app_id) = &matcher.app_id {
        parts.push(format!("app_id={app_id}"));
    }
    if let Some(title) = &matcher.title {
        parts.push(format!("title={title}"));
    }
    if let Some(class) = &matcher.class {
        parts.push(format!("class={class}"));
    }
    if let Some(process) = &matcher.process {
        parts.push(format!("process={process}"));
    }
    if parts.is_empty() {
        "any window".to_string()
    } else {
        parts.join(", ")
    }
}

fn matches_group_filter(window_group: Option<&str>, only_groups: &[String]) -> bool {
    if only_groups.is_empty() {
        return true;
    }
    let Some(window_group) = window_group else {
        return false;
    };
    let window_group = window_group.to_lowercase();
    only_groups
        .iter()
        .any(|group| window_group == group.to_lowercase())
}

pub struct LayoutPlugin;

impl Plugin for LayoutPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let config_path = layouts_config_path();
        let config_exists = config_path.exists();
        let config_label = if config_exists {
            "Edit layouts.json"
        } else {
            "Create layouts.json"
        };
        let config_desc = config_path.to_string_lossy();
        let trimmed = query.trim();
        let rest = match crate::common::strip_prefix_ci(trimmed, "layout") {
            Some(rest) => rest.trim(),
            None => return Vec::new(),
        };

        if rest.is_empty() {
            return vec![
                Action {
                    label: "layout list".into(),
                    desc: "Layout".into(),
                    action: "query:layout list".into(),
                    args: None,
                },
                Action {
                    label: "layout save <name>".into(),
                    desc: "Layout".into(),
                    action: "query:layout save ".into(),
                    args: None,
                },
                Action {
                    label: "layout load <name>".into(),
                    desc: "Layout".into(),
                    action: "query:layout load ".into(),
                    args: None,
                },
                Action {
                    label: "layout show <name>".into(),
                    desc: "Layout".into(),
                    action: "query:layout show ".into(),
                    args: None,
                },
                Action {
                    label: format!("{config_label} ({config_desc})"),
                    desc: "Layout config".into(),
                    action: "layout:edit".into(),
                    args: None,
                },
                Action {
                    label: "layout rm <name>".into(),
                    desc: "Layout".into(),
                    action: "query:layout rm ".into(),
                    args: None,
                },
            ];
        }

        if let Some(rest) = crate::common::strip_prefix_ci(rest, "list") {
            let filter = rest.trim().to_lowercase();
            if let Ok(store) = load_layouts(layouts_config_path()) {
                return store
                    .layouts
                    .iter()
                    .filter(|layout| {
                        filter.is_empty() || layout.name.to_lowercase().contains(&filter)
                    })
                    .map(|layout| Action {
                        label: format!("Load layout {}", layout.name),
                        desc: "Layout".into(),
                        action: build_action(
                            format!("layout:load:{}", layout.name),
                            &LayoutFlags::default(),
                        ),
                        args: None,
                    })
                    .collect();
            }
            return Vec::new();
        }

        if let Some(rest) = crate::common::strip_prefix_ci(rest, "save") {
            let (name, flags) = parse_name_and_flags(rest.trim());
            if name.is_empty() {
                return Vec::new();
            }
            return vec![Action {
                label: format!("Save layout {name}"),
                desc: "Layout".into(),
                action: build_action(format!("layout:save:{name}"), &flags),
                args: None,
            }];
        }

        if let Some(rest) = crate::common::strip_prefix_ci(rest, "load") {
            let (name, flags) = parse_name_and_flags(rest.trim());
            if name.is_empty() {
                return Vec::new();
            }
            return vec![Action {
                label: format!("Load layout {name}"),
                desc: "Layout".into(),
                action: build_action(format!("layout:load:{name}"), &flags),
                args: None,
            }];
        }

        if let Some(rest) = crate::common::strip_prefix_ci(rest, "show") {
            let (name, flags) = parse_name_and_flags(rest.trim());
            if let Ok(store) = load_layouts(layouts_config_path()) {
                if !name.is_empty() {
                    if let Some(layout) = get_layout(&store, &name) {
                        let action = build_action(format!("layout:show:{}", layout.name), &flags);
                        let layout_name = layout.name.as_str();
                        return layout
                            .windows
                            .iter()
                            .filter(|window| {
                                matches_group_filter(window.group.as_deref(), &flags.only_groups)
                            })
                            .enumerate()
                            .map(|(idx, window)| {
                                let rect = window.placement.rect;
                                let monitor = window
                                    .placement
                                    .monitor
                                    .as_deref()
                                    .unwrap_or("any monitor");
                                let desktop = window.desktop.as_deref().unwrap_or("any desktop");
                                let match_label = format_match(&window.matcher);
                                let group_label = window.group.as_deref().unwrap_or("ungrouped");
                                Action {
                                    label: format!(
                                        "Window {}: {} @ {} [{}] [{:.2}, {:.2}, {:.2}, {:.2}] ({})",
                                        idx + 1,
                                        match_label,
                                        monitor,
                                        desktop,
                                        rect[0],
                                        rect[1],
                                        rect[2],
                                        rect[3],
                                        group_label
                                    ),
                                    desc: format!("Layout preview: {layout_name}"),
                                    action: action.clone(),
                                    args: None,
                                }
                            })
                            .collect();
                    }
                }

                let filter = name.to_lowercase();
                return store
                    .layouts
                    .iter()
                    .filter(|layout| {
                        filter.is_empty() || layout.name.to_lowercase().contains(&filter)
                    })
                    .map(|layout| Action {
                        label: format!("Show layout {}", layout.name),
                        desc: "Layout".into(),
                        action: format!("query:layout show {}", layout.name),
                        args: None,
                    })
                    .collect();
            }
            return Vec::new();
        }

        if let Some(rest) = crate::common::strip_prefix_ci(rest, "edit") {
            if rest.trim().is_empty() {
                return vec![Action {
                    label: format!("{config_label} ({config_desc})"),
                    desc: "Layout config".into(),
                    action: "layout:edit".into(),
                    args: None,
                }];
            }
        }

        if let Some(rest) = crate::common::strip_prefix_ci(rest, "rm") {
            let (name, flags) = parse_name_and_flags(rest.trim());
            if name.is_empty() {
                return Vec::new();
            }
            return vec![Action {
                label: format!("Remove layout {name}"),
                desc: "Layout".into(),
                action: build_action(format!("layout:rm:{name}"), &flags),
                args: None,
            }];
        }

        Vec::new()
    }

    fn name(&self) -> &str {
        "layout"
    }

    fn description(&self) -> &str {
        "Manage saved window layouts (prefix: `layout`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![Action {
            label: "layout".into(),
            desc: "Layout".into(),
            action: "query:layout ".into(),
            args: None,
        }]
    }
}
