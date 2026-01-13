//! Layout action helpers.
//!
//! Action format (flags are optional and must use a single delimiter):
//! - `layout:save:<name>|<flags>`
//! - `layout:load:<name>|<flags>`
//! - `layout:show:<name>|<flags>`
//! - `layout:rm:<name>|<flags>`
//! - `layout:list|<flags>`
//!
//! Flags are comma-separated (`,`) and values use `key=value`, for example:
//! `layout:load:Work|dry_run,only_active_monitor,filter=chrome`.
use crate::plugins::layouts_storage::{
    self, list_layouts as list_saved_layouts, remove_layout as remove_saved_layout, Layout,
    LayoutOptions, LAYOUTS_FILE,
};
use crate::windows_layout::{collect_layout_windows, LayoutWindowOptions};

#[derive(Default)]
struct LayoutFlags {
    dry_run: bool,
    no_launch: bool,
    only_active_monitor: bool,
    include_minimized: bool,
    exclude_minimized: bool,
    filter: Option<String>,
}

fn parse_flags(raw: Option<&str>) -> LayoutFlags {
    let mut flags = LayoutFlags::default();
    let Some(raw) = raw else {
        return flags;
    };
    for flag in raw.split(',').map(str::trim).filter(|f| !f.is_empty()) {
        match flag {
            "dry_run" => flags.dry_run = true,
            "no_launch" => flags.no_launch = true,
            "only_active_monitor" => flags.only_active_monitor = true,
            "include_minimized" => flags.include_minimized = true,
            "exclude_minimized" => flags.exclude_minimized = true,
            _ => {
                if let Some(value) = flag.strip_prefix("filter=") {
                    flags.filter = Some(value.to_string());
                }
            }
        }
    }
    flags
}

fn sanitize_alias(name: &str) -> String {
    #[cfg(windows)]
    let invalid = ['\\', '/', ':', '*', '?', '"', '<', '>', '|'];
    #[cfg(not(windows))]
    let invalid = ['\\', '/'];
    let mut alias = String::with_capacity(name.len());
    for c in name.chars() {
        if invalid.contains(&c) {
            alias.push('_');
        } else {
            alias.push(c);
        }
    }
    if alias.is_empty() {
        "layout".to_string()
    } else {
        alias
    }
}

fn ensure_layout_name(name: &str) -> anyhow::Result<()> {
    if name.trim().is_empty() {
        anyhow::bail!("layout name cannot be empty");
    }
    Ok(())
}

pub fn save_layout(name: &str, flags: Option<&str>) -> anyhow::Result<()> {
    ensure_layout_name(name)?;
    let flags = parse_flags(flags);
    if flags.dry_run {
        return Ok(());
    }

    let mut store = layouts_storage::load_layouts(LAYOUTS_FILE)?;
    let windows = collect_layout_windows(LayoutWindowOptions {
        only_active_monitor: flags.only_active_monitor,
        include_minimized: flags.include_minimized,
        exclude_minimized: flags.exclude_minimized,
    })?;
    let layout = Layout {
        name: name.to_string(),
        windows,
        launches: Vec::new(),
        created_at: Some(chrono::Utc::now().to_rfc3339()),
        notes: String::new(),
        options: LayoutOptions::default(),
        ignore: Vec::new(),
    };
    layouts_storage::upsert_layout(&mut store, layout);
    layouts_storage::save_layouts(LAYOUTS_FILE, &store)?;
    Ok(())
}

pub fn load_layout(name: &str, flags: Option<&str>) -> anyhow::Result<()> {
    ensure_layout_name(name)?;
    let flags = parse_flags(flags);
    let store = layouts_storage::load_layouts(LAYOUTS_FILE)?;
    let layout = layouts_storage::get_layout(&store, name)
        .ok_or_else(|| anyhow::anyhow!("layout '{name}' not found"))?;
    if flags.dry_run {
        return Ok(());
    }
    if flags.no_launch {
        return Ok(());
    }
    if layout.launches.is_empty() {
        return Ok(());
    }
    for launch in &layout.launches {
        let mut cmd = std::process::Command::new(&launch.command);
        if let Some(cwd) = &launch.cwd {
            cmd.current_dir(cwd);
        }
        if !launch.args.is_empty() {
            cmd.args(&launch.args);
        }
        cmd.spawn()?;
    }
    Ok(())
}

pub fn show_layout(name: &str, flags: Option<&str>) -> anyhow::Result<()> {
    ensure_layout_name(name)?;
    let flags = parse_flags(flags);
    let store = layouts_storage::load_layouts(LAYOUTS_FILE)?;
    let layout = layouts_storage::get_layout(&store, name)
        .ok_or_else(|| anyhow::anyhow!("layout '{name}' not found"))?;
    if flags.dry_run {
        return Ok(());
    }
    let contents = serde_json::to_string_pretty(layout)?;
    let alias = sanitize_alias(&format!("layout_{name}"));
    let path = crate::plugins::tempfile::create_named_file(&alias, &contents)?;
    open::that(&path)?;
    Ok(())
}

pub fn remove_layout(name: &str, flags: Option<&str>) -> anyhow::Result<()> {
    ensure_layout_name(name)?;
    let flags = parse_flags(flags);
    let mut store = layouts_storage::load_layouts(LAYOUTS_FILE)?;
    if layouts_storage::get_layout(&store, name).is_none() {
        anyhow::bail!("layout '{name}' not found");
    }
    if flags.dry_run {
        return Ok(());
    }
    let _ = remove_saved_layout(&mut store, name);
    layouts_storage::save_layouts(LAYOUTS_FILE, &store)?;
    Ok(())
}

pub fn list_layouts(flags: Option<&str>) -> anyhow::Result<()> {
    use std::fmt::Write as _;

    let flags = parse_flags(flags);
    let store = layouts_storage::load_layouts(LAYOUTS_FILE)?;
    let mut list = list_saved_layouts(&store);
    if let Some(filter) = flags.filter {
        let needle = filter.to_lowercase();
        list.retain(|name| name.to_lowercase().contains(&needle));
    }
    let mut contents = String::new();
    if list.is_empty() {
        contents.push_str("No layouts found.\n");
    } else {
        for name in list {
            writeln!(&mut contents, "- {name}")?;
        }
    }
    if flags.dry_run {
        return Ok(());
    }
    let path = crate::plugins::tempfile::create_named_file("layouts_list", &contents)?;
    open::that(&path)?;
    Ok(())
}
