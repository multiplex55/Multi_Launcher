use multi_launcher::actions::{Action, load_actions_typed};
use multi_launcher::common::persistence::LoadState;
use multi_launcher::dashboard::config::{DashboardConfig, GridConfig, OverflowMode, SlotConfig};
use multi_launcher::plugins::note::{NoteExternalOpen, NotePluginSettings};
use multi_launcher::settings::{LogFile, Settings};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const FIXTURE_ID: &str = "jump-mode-m01-v1";
const NOTE_COUNT: usize = 32;
const SAFE_PLUGIN: &str = "notes";
const LAUNCHER_HOTKEY: &str = "Ctrl+Alt+F12";

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct FixtureManifest {
    schema_version: u32,
    fixture_id: String,
    enabled_plugins: Vec<String>,
    dashboard_slot_ids: Vec<String>,
    note_ids: Vec<String>,
    action_count: usize,
    external_note_open: String,
    safety: SafetyManifest,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct SafetyManifest {
    profile_isolated_by_working_directory: bool,
    notes_path_supplied_by_ml_notes_dir: bool,
    clipboard_modify_plugin_enabled: bool,
    macros_enabled: bool,
    network_plugin_enabled: bool,
    custom_actions_present: bool,
    dashboard_click_behavior: String,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("jump_mode_fixture: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> anyhow::Result<()> {
    let mut args = env::args_os().skip(1);
    let mode = args
        .next()
        .and_then(|value| value.into_string().ok())
        .ok_or_else(|| anyhow::anyhow!("usage: jump_mode_fixture <generate|verify> <profile>"))?;
    let profile = args
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("profile path is required"))?;
    anyhow::ensure!(args.next().is_none(), "unexpected extra argument");

    match mode.as_str() {
        "generate" => generate(&profile)?,
        "verify" => verify(&profile)?,
        _ => anyhow::bail!("unknown mode {mode:?}; expected generate or verify"),
    }
    println!("fixture {mode} verified at {}", profile.display());
    Ok(())
}

fn generate(profile: &Path) -> anyhow::Result<()> {
    anyhow::ensure!(
        !profile.exists(),
        "refusing to overwrite an existing fixture profile: {}",
        profile.display()
    );
    fs::create_dir_all(profile)?;
    let notes_dir = profile.join("notes");
    fs::create_dir(&notes_dir)?;
    let screenshot_dir = profile.join("screenshot-output");
    fs::create_dir(&screenshot_dir)?;

    let mut settings = Settings::default();
    settings.hotkey = Some(LAUNCHER_HOTKEY.into());
    settings.help_hotkey = None;
    settings.quit_hotkey = None;
    settings.window_size = Some((900, 560));
    settings.offscreen_pos = None;
    settings.follow_mouse = false;
    settings.static_location_enabled = false;
    settings.index_paths = Some(Vec::new());
    settings.max_indexed_items = Some(0);
    settings.plugin_dirs = Some(Vec::new());
    settings.enabled_plugins = Some(HashSet::from([SAFE_PLUGIN.to_owned()]));
    settings.debug_logging = true;
    settings.log_file = Some(LogFile::Path(
        profile.join("launcher.log").to_string_lossy().into_owned(),
    ));
    settings.screenshot_dir = Some(screenshot_dir.to_string_lossy().into_owned());
    settings.screenshot_save_file = false;
    settings.screenshot_auto_save = false;
    settings.screenshot_use_editor = false;
    settings.note.backlinks_enabled = false;
    settings.note.aliases_enabled = false;
    settings.note.templates_enabled = false;
    settings
        .plugin_settings
        .insert("clipboard_modify".into(), json!({}));
    let note_settings = NotePluginSettings {
        external_open: NoteExternalOpen::Neither,
        backlinks_enabled: false,
        aliases_enabled: false,
        templates_enabled: false,
    };
    let serialized_note_settings = serde_json::to_value(note_settings)?;
    settings
        .plugin_settings
        .insert("note".into(), serialized_note_settings.clone());
    settings
        .plugin_settings
        .insert("notes".into(), serialized_note_settings);

    write_json(&profile.join("settings.json"), &settings)?;
    write_json(&profile.join("actions.json"), &Vec::<Action>::new())?;

    let dashboard = dashboard_fixture();
    write_json(&profile.join("dashboard.json"), &dashboard)?;

    let note_ids = note_ids();
    for (index, note_id) in note_ids.iter().enumerate() {
        let ordinal = index + 1;
        let note_text = format!(
            "# Jump QA Note {ordinal:03}\n\nFixture record {ordinal:03} for the isolated Jump Mode runner.\n\nStable fixture ID: `{note_id}`\nTag: #jump-qa-bucket-{:02}\n",
            ((index % 4) + 1)
        );
        fs::write(notes_dir.join(format!("{note_id}.md")), note_text)?;
    }

    let manifest = fixture_manifest(note_ids);
    write_json(&profile.join("fixture_manifest.json"), &manifest)?;
    verify(profile)
}

fn verify(profile: &Path) -> anyhow::Result<()> {
    anyhow::ensure!(profile.is_dir(), "fixture profile is not a directory");
    let profile = fs::canonicalize(profile)?;
    let settings_path = profile.join("settings.json");
    let settings = match Settings::load_typed(&settings_path)? {
        LoadState::Loaded(settings) => settings,
        LoadState::Missing | LoadState::Empty => {
            anyhow::bail!("fixture settings were not loaded as a valid persisted document")
        }
    };
    let enabled_plugins = settings
        .enabled_plugins
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("fixture must use an explicit plugin allowlist"))?;
    anyhow::ensure!(
        enabled_plugins.len() == 1 && enabled_plugins.contains(SAFE_PLUGIN),
        "fixture plugin allowlist must contain only {SAFE_PLUGIN:?}"
    );
    anyhow::ensure!(
        settings.plugin_dirs.as_deref() == Some(&[]),
        "external plugin dirs must be disabled"
    );
    anyhow::ensure!(
        settings.index_paths.as_deref() == Some(&[]),
        "filesystem indexing must be disabled"
    );
    anyhow::ensure!(
        settings.plugin_settings.contains_key("clipboard_modify"),
        "clipboard migration guard is missing"
    );
    anyhow::ensure!(
        !enabled_plugins.contains("clipboard_modify")
            && !enabled_plugins.contains("clipboard")
            && !enabled_plugins.contains("macros")
            && !enabled_plugins.contains("mkmacro")
            && !enabled_plugins.contains("network")
            && !enabled_plugins.contains("shell")
            && !enabled_plugins.contains("system"),
        "fixture allowlist includes a side-effecting plugin"
    );
    anyhow::ensure!(
        settings.hotkey.as_deref() == Some(LAUNCHER_HOTKEY),
        "fixture must use its isolated launcher chord"
    );
    let configured_screenshot_dir = PathBuf::from(
        settings
            .screenshot_dir
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("fixture screenshot output directory is missing"))?,
    );
    anyhow::ensure!(
        configured_screenshot_dir.is_absolute(),
        "fixture screenshot output directory must be absolute"
    );
    let expected_screenshot_dir = fs::canonicalize(profile.join("screenshot-output"))?;
    anyhow::ensure!(
        fs::canonicalize(configured_screenshot_dir)? == expected_screenshot_dir,
        "fixture screenshot output must remain within the isolated profile"
    );
    anyhow::ensure!(
        !settings.screenshot_auto_save
            && !settings.screenshot_save_file
            && !settings.screenshot_use_editor,
        "fixture must not save or open user screenshots"
    );

    let actions_path = profile.join("actions.json");
    let actions = match load_actions_typed(&actions_path)? {
        LoadState::Loaded(actions) => actions,
        LoadState::Missing | LoadState::Empty => anyhow::bail!("actions.json is not a typed array"),
    };
    anyhow::ensure!(
        actions.is_empty(),
        "fixture must not configure executable actions"
    );

    let dashboard_path = profile.join("dashboard.json");
    let dashboard = match DashboardConfig::load_typed(&dashboard_path)? {
        LoadState::Loaded(dashboard) => dashboard,
        LoadState::Missing | LoadState::Empty => {
            anyhow::bail!("dashboard.json is not a typed document")
        }
    };
    let expected_slot_ids = dashboard_slot_ids();
    let actual_slot_ids: Vec<String> = dashboard
        .slots
        .iter()
        .filter_map(|slot| slot.id.clone())
        .collect();
    anyhow::ensure!(
        actual_slot_ids == expected_slot_ids,
        "dashboard fixture slots changed"
    );
    anyhow::ensure!(
        dashboard.slots.iter().all(|slot| {
            slot.widget == "pinned_query_results"
                && slot.settings.get("engine").and_then(Value::as_str) == Some(SAFE_PLUGIN)
                && slot.settings.get("click_behavior").and_then(Value::as_str) == Some("fill_query")
        }),
        "dashboard contains an unexpected widget or click behavior"
    );

    let notes_dir = profile.join("notes");
    anyhow::ensure!(notes_dir.is_dir(), "isolated notes directory is missing");
    let mut actual_notes = fs::read_dir(&notes_dir)?
        .map(|entry| {
            entry.map(|entry| {
                entry
                    .path()
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or_default()
                    .to_owned()
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    actual_notes.sort();
    let mut expected_notes = note_ids();
    expected_notes.sort();
    anyhow::ensure!(
        actual_notes == expected_notes,
        "fixture notes do not match stable IDs"
    );

    let manifest: FixtureManifest =
        serde_json::from_slice(&fs::read(profile.join("fixture_manifest.json"))?)?;
    anyhow::ensure!(
        manifest == fixture_manifest(note_ids()),
        "fixture manifest is stale"
    );
    anyhow::ensure!(
        manifest.action_count == 0,
        "fixture manifest claims executable actions"
    );
    anyhow::ensure!(manifest.safety.clipboard_modify_plugin_enabled == false);
    anyhow::ensure!(manifest.safety.macros_enabled == false);
    anyhow::ensure!(manifest.safety.network_plugin_enabled == false);
    anyhow::ensure!(manifest.safety.custom_actions_present == false);

    let resolved_notes_dir = fs::canonicalize(notes_dir)?;
    anyhow::ensure!(
        resolved_notes_dir.starts_with(&profile),
        "fixture notes escaped the isolated profile"
    );
    Ok(())
}

fn dashboard_fixture() -> DashboardConfig {
    DashboardConfig {
        version: 1,
        grid: GridConfig { rows: 2, cols: 2 },
        slots: vec![
            dashboard_slot("jump-qa-note-list", 0, 0, "note list"),
            dashboard_slot("jump-qa-note-tags", 0, 1, "note tags"),
            dashboard_slot("jump-qa-note-search", 1, 0, "note search"),
        ],
    }
}

fn dashboard_slot(id: &str, row: i32, col: i32, query: &str) -> SlotConfig {
    SlotConfig {
        id: Some(id.to_owned()),
        widget: "pinned_query_results".to_owned(),
        row,
        col,
        row_span: 1,
        col_span: 1,
        settings: json!({
            "engine": SAFE_PLUGIN,
            "query": query,
            "limit": 32,
            "click_behavior": "fill_query"
        }),
        overflow: OverflowMode::Scroll,
    }
}

fn dashboard_slot_ids() -> Vec<String> {
    vec![
        "jump-qa-note-list".into(),
        "jump-qa-note-tags".into(),
        "jump-qa-note-search".into(),
    ]
}

fn note_ids() -> Vec<String> {
    (1..=NOTE_COUNT)
        .map(|index| format!("jump-qa-note-{index:03}"))
        .collect()
}

fn fixture_manifest(note_ids: Vec<String>) -> FixtureManifest {
    FixtureManifest {
        schema_version: 1,
        fixture_id: FIXTURE_ID.to_owned(),
        enabled_plugins: vec![SAFE_PLUGIN.to_owned()],
        dashboard_slot_ids: dashboard_slot_ids(),
        note_ids,
        action_count: 0,
        external_note_open: "Neither".to_owned(),
        safety: SafetyManifest {
            profile_isolated_by_working_directory: true,
            notes_path_supplied_by_ml_notes_dir: true,
            clipboard_modify_plugin_enabled: false,
            macros_enabled: false,
            network_plugin_enabled: false,
            custom_actions_present: false,
            dashboard_click_behavior: "fill_query".to_owned(),
        },
    }
}

fn write_json(path: &Path, value: &impl Serialize) -> anyhow::Result<()> {
    let bytes = serde_json::to_vec_pretty(value)?;
    fs::write(path, bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_contains_stable_dense_safe_note_ids() {
        let ids = note_ids();
        let unique: HashSet<_> = ids.iter().collect();
        assert_eq!(ids.len(), 32);
        assert_eq!(unique.len(), ids.len());
        assert_eq!(ids.first().map(String::as_str), Some("jump-qa-note-001"));
        assert_eq!(ids.last().map(String::as_str), Some("jump-qa-note-032"));
        let fixture = fixture_manifest(ids);
        assert_eq!(fixture.enabled_plugins, ["notes"]);
        assert_eq!(fixture.action_count, 0);
        assert_eq!(LAUNCHER_HOTKEY, "Ctrl+Alt+F12");
        assert!(!fixture.safety.clipboard_modify_plugin_enabled);
        assert!(!fixture.safety.macros_enabled);
        assert!(!fixture.safety.network_plugin_enabled);
    }

    #[test]
    fn dashboard_fixture_only_fills_safe_note_queries() {
        let dashboard = dashboard_fixture();
        assert_eq!(dashboard.slots.len(), 3);
        for slot in dashboard.slots {
            assert_eq!(slot.widget, "pinned_query_results");
            assert_eq!(slot.settings["engine"], "notes");
            assert_eq!(slot.settings["click_behavior"], "fill_query");
            assert!(
                slot.settings["query"]
                    .as_str()
                    .unwrap()
                    .starts_with("note ")
            );
        }
    }
}
