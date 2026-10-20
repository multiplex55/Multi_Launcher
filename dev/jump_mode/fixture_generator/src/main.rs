use multi_launcher::actions::{Action, load_actions_typed};
use multi_launcher::common::persistence::LoadState;
use multi_launcher::dashboard::config::{DashboardConfig, GridConfig, OverflowMode, SlotConfig};
use multi_launcher::dashboard::widgets::{ClickBehavior, PinnedQueryResultsConfig};
use multi_launcher::hotkey::validation::validate_hotkey_routes;
use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::note::{NoteExternalOpen, NotePlugin, NotePluginSettings, load_notes};
use multi_launcher::radial::model::RadialDocument;
use multi_launcher::settings::{LogFile, Settings};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const FIXTURE_ID: &str = "jump-mode-m01-v2";
const NOTE_COUNT: usize = 32;
const SAFE_PLUGIN: &str = "notes";
const LAUNCHER_HOTKEY: &str = "Ctrl+Alt+F10";
const NO_RESULTS_QUERY: &str = "zzzznomatchprobe";

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct FixtureManifest {
    schema_version: u32,
    fixture_id: String,
    scenario: String,
    experimental_jump_mode_enabled: bool,
    launcher_hotkey: String,
    enabled_plugins: Vec<String>,
    dashboard_slot_ids: Vec<String>,
    note_ids: Vec<String>,
    action_count: usize,
    external_note_open: String,
    no_results_query: String,
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
    reduce_dashboard_work_when_unfocused: bool,
    experimental_jump_mode_enabled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FixtureScenario {
    Baseline,
    QueryProbe,
    Phase1Enabled,
    Phase1Disabled,
}

impl FixtureScenario {
    fn parse(value: Option<String>) -> anyhow::Result<Self> {
        match value.as_deref().unwrap_or("baseline") {
            "baseline" => Ok(Self::Baseline),
            "query-probe" => Ok(Self::QueryProbe),
            "phase1-enabled" => Ok(Self::Phase1Enabled),
            "phase1-disabled" => Ok(Self::Phase1Disabled),
            value => anyhow::bail!(
                "unknown fixture scenario {value:?}; expected baseline, query-probe, phase1-enabled, or phase1-disabled"
            ),
        }
    }

    fn jump_mode_enabled(self) -> bool {
        matches!(self, Self::QueryProbe | Self::Phase1Enabled)
    }

    fn phase1(self) -> bool {
        matches!(self, Self::Phase1Enabled | Self::Phase1Disabled)
    }

    fn label(self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::QueryProbe => "query-probe",
            Self::Phase1Enabled => "phase1-enabled",
            Self::Phase1Disabled => "phase1-disabled",
        }
    }
}

#[derive(Debug, Serialize)]
struct NoteQueryObservation {
    slot_id: String,
    query: String,
    action_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
struct TagObservation {
    tag: String,
    count: usize,
}

#[derive(Debug, Serialize)]
struct ProductionSemanticsObservation {
    canonical_notes_dir: String,
    loaded_note_count: usize,
    note_queries: Vec<NoteQueryObservation>,
    no_results_query: String,
    no_results_count: usize,
    observed_tags: Vec<TagObservation>,
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
    let scenario = args
        .next()
        .map(|value| {
            value
                .into_string()
                .map_err(|_| anyhow::anyhow!("fixture scenario argument must be valid Unicode"))
        })
        .transpose()?;
    let scenario = FixtureScenario::parse(scenario)?;
    anyhow::ensure!(args.next().is_none(), "unexpected extra argument");

    match mode.as_str() {
        "generate" => generate(&profile, scenario)?,
        "verify" => verify(&profile, scenario)?,
        _ => anyhow::bail!("unknown mode {mode:?}; expected generate or verify"),
    }
    println!(
        "fixture {mode} verified at {} for {}",
        profile.display(),
        scenario.label()
    );
    Ok(())
}

fn generate(profile: &Path, scenario: FixtureScenario) -> anyhow::Result<()> {
    anyhow::ensure!(
        !profile.exists(),
        "refusing to overwrite an existing fixture profile: {}",
        profile.display()
    );
    fs::create_dir_all(profile)?;
    let notes_dir = profile.join("notes");
    fs::create_dir(&notes_dir)?;
    let templates_dir = profile.join("templates");
    fs::create_dir(&templates_dir)?;
    set_isolated_fixture_dirs(profile)?;
    let screenshot_dir = profile.join("screenshot-output");
    fs::create_dir(&screenshot_dir)?;

    let mut settings = Settings::default();
    settings.hotkey = Some(LAUNCHER_HOTKEY.into());
    settings.help_hotkey = None;
    settings.quit_hotkey = None;
    settings.paste_sequence.next_hotkey = None;
    settings.paste_sequence.cancel_hotkey = None;
    settings.window_size = if scenario.phase1() {
        Some((1800, 1000))
    } else {
        Some((900, 560))
    };
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
    settings.reduce_dashboard_work_when_unfocused = false;
    settings.experimental_jump_mode_enabled = scenario.jump_mode_enabled();
    // Keep normal and error notifications visible in every native scenario.
    // Hiding errors would make the visible test less informative.
    settings.enable_toasts = true;
    settings.note_save_on_close = false;
    if scenario.phase1() {
        settings.query_results_layout.enabled = true;
        settings.query_results_layout.rows = 4;
        settings.query_results_layout.cols = 8;
        settings.query_results_layout.respect_plugin_capability = false;
        settings.query_results_layout.plugin_opt_out.clear();
    }
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

    let dashboard = dashboard_fixture(scenario);
    write_json(&profile.join("dashboard.json"), &dashboard)?;

    let note_ids = note_ids();
    for (index, note_id) in note_ids.iter().enumerate() {
        let ordinal = index + 1;
        let title = fixture_note_title(scenario, ordinal);
        let note_text = format!(
            "# {title}\n\nFixture record {ordinal:03} for the isolated Jump Mode runner.\n\nStable fixture ID: `{note_id}`\nTag: #jump_qa_bucket_{:02}\n",
            ((index % 4) + 1)
        );
        fs::write(notes_dir.join(format!("{note_id}.md")), note_text)?;
    }

    let manifest = fixture_manifest(note_ids, scenario);
    write_json(&profile.join("fixture_manifest.json"), &manifest)?;
    verify(profile, scenario)
}

fn verify(profile: &Path, scenario: FixtureScenario) -> anyhow::Result<()> {
    anyhow::ensure!(profile.is_dir(), "fixture profile is not a directory");
    let profile = fs::canonicalize(profile)?;
    let (notes_dir, _templates_dir) = set_isolated_fixture_dirs(&profile)?;
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
        !settings.reduce_dashboard_work_when_unfocused,
        "fixture must allow dashboard query refreshes while the launcher is initially unfocused"
    );
    anyhow::ensure!(
        settings.enable_toasts && settings.show_error_toasts,
        "fixture must preserve normal and error notifications"
    );
    if scenario.phase1() {
        anyhow::ensure!(
            settings.window_size == Some((1800, 1000)),
            "Phase 1 fixture must use its bounded larger viewport"
        );
        anyhow::ensure!(
            settings.query_results_layout.enabled
                && settings.query_results_layout.rows == 4
                && settings.query_results_layout.cols == 8
                && !settings.query_results_layout.respect_plugin_capability
                && settings.query_results_layout.plugin_opt_out.is_empty(),
            "Phase 1 fixture must explicitly enable the 4x8 result grid for the notes plugin"
        );
        anyhow::ensure!(
            !settings.note_save_on_close,
            "Phase 1 fixture must not save or mutate notes when safely closing a panel"
        );
    }
    anyhow::ensure!(
        settings.experimental_jump_mode_enabled == scenario.jump_mode_enabled(),
        "fixture Jump Mode setting does not match the requested scenario"
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
    anyhow::ensure!(
        settings.paste_sequence.next_hotkey.is_none()
            && settings.paste_sequence.cancel_hotkey.is_none(),
        "fixture Next and Cancel Paste Sequence shortcuts must remain disabled"
    );
    validate_hotkey_routes(&settings, &RadialDocument::starter(), None).map_err(|issues| {
        anyhow::anyhow!(
            "fixture hotkey routes are invalid: {}",
            issues
                .iter()
                .map(|issue| issue.message.as_str())
                .collect::<Vec<_>>()
                .join("; ")
        )
    })?;
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
    let expected_slot_ids = dashboard_slot_ids(scenario);
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

    let expected_queries = dashboard_queries(scenario);
    let mut typed_configs = std::collections::HashMap::new();
    for slot in &dashboard.slots {
        let slot_id = slot
            .id
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("dashboard fixture slot is missing its stable ID"))?;
        let config: PinnedQueryResultsConfig = serde_json::from_value(slot.settings.clone())?;
        let expected_query = expected_queries
            .iter()
            .find(|(expected_id, _)| *expected_id == slot_id)
            .map(|(_, query)| *query)
            .ok_or_else(|| anyhow::anyhow!("unexpected dashboard slot {slot_id:?}"))?;
        anyhow::ensure!(
            config.engine == SAFE_PLUGIN
                && config.query == expected_query
                && config.limit == NOTE_COUNT
                && config.click_behavior == ClickBehavior::FillQuery,
            "dashboard slot {slot_id:?} does not match its typed production config"
        );
        anyhow::ensure!(
            typed_configs.insert(slot_id.to_owned(), config).is_none(),
            "dashboard fixture contains duplicate slot ID {slot_id:?}"
        );
    }

    let loaded_notes = load_notes()?;
    let mut loaded_note_ids: Vec<String> =
        loaded_notes.iter().map(|note| note.slug.clone()).collect();
    loaded_note_ids.sort();
    let mut expected_note_ids = note_ids();
    expected_note_ids.sort();
    anyhow::ensure!(
        loaded_note_ids == expected_note_ids,
        "production note loader did not return the stable fixture records"
    );
    let mut tag_counts = std::collections::BTreeMap::new();
    for note in &loaded_notes {
        for tag in &note.tags {
            *tag_counts.entry(tag.clone()).or_insert(0usize) += 1;
        }
    }
    let expected_tag_counts: std::collections::BTreeMap<String, usize> = (1..=4)
        .map(|bucket| (format!("jump_qa_bucket_{bucket:02}"), NOTE_COUNT / 4))
        .collect();
    anyhow::ensure!(
        tag_counts == expected_tag_counts,
        "production note loader did not extract the expected four underscore tags"
    );

    // ML_NOTES_DIR was set before settings/plugin construction and before this
    // production plugin starts its watcher/coordinator threads.
    let note_plugin = NotePlugin::new();
    let expected_open_actions: Vec<String> = expected_note_ids
        .iter()
        .map(|slug| format!("note:open:{slug}"))
        .collect();
    let mut query_observations = Vec::new();
    let note_list_slot_ids = if scenario.phase1() {
        vec![
            "jump-qa-phase1-list-a",
            "jump-qa-phase1-list-b",
            "jump-qa-phase1-list-c",
        ]
    } else {
        vec!["jump-qa-note-list", "jump-qa-note-search"]
    };
    for slot_id in note_list_slot_ids {
        let config = typed_configs
            .get(slot_id)
            .ok_or_else(|| anyhow::anyhow!("missing typed config for {slot_id:?}"))?;
        let expected_query = if scenario.phase1() {
            "note list"
        } else if slot_id == "jump-qa-note-list" {
            "note list"
        } else {
            "note search"
        };
        let mut action_ids: Vec<String> = note_plugin
            .search(&config.query)
            .into_iter()
            .map(|action| action.action)
            .collect();
        action_ids.sort();
        anyhow::ensure!(
            action_ids == expected_open_actions,
            "production NotePlugin query {:?} did not return all stable fixture note IDs",
            config.query
        );
        anyhow::ensure!(
            config.query == expected_query,
            "dashboard slot {slot_id:?} did not use the expected production query"
        );
        query_observations.push(NoteQueryObservation {
            slot_id: slot_id.to_owned(),
            query: config.query.clone(),
            action_ids,
        });
    }
    if scenario.phase1() {
        let command_actions = note_plugin.search("note");
        anyhow::ensure!(
            command_actions.iter().any(|action| {
                action.label == "note list" && action.action == "query:note list"
            }),
            "production NotePlugin command query did not expose the safe `note list` FillQuery target"
        );
        let mut duplicate_actions: Vec<_> = note_plugin
            .search("note open Jump QA Shared Note")
            .into_iter()
            .map(|action| action.action)
            .collect();
        duplicate_actions.sort();
        anyhow::ensure!(
            duplicate_actions
                == [
                    "note:open:jump-qa-note-001".to_owned(),
                    "note:open:jump-qa-note-002".to_owned(),
                ],
            "duplicate-title fixture records must retain their distinct note slugs"
        );
    }
    let tag_query = if scenario.phase1() {
        "note tags"
    } else {
        typed_configs
            .get("jump-qa-note-tags")
            .ok_or_else(|| anyhow::anyhow!("missing typed config for note tags"))?
            .query
            .as_str()
    };
    let tag_actions = note_plugin.search(tag_query);
    let mut observed_tag_counts = std::collections::BTreeMap::new();
    for action in &tag_actions {
        let tag = action
            .label
            .strip_prefix('#')
            .and_then(|label| label.split_once(" ("))
            .and_then(|(tag, count)| count.strip_suffix(')').map(|count| (tag, count)))
            .ok_or_else(|| {
                anyhow::anyhow!("unexpected production tag action label {:?}", action.label)
            })?;
        let count: usize = tag.1.parse()?;
        observed_tag_counts.insert(tag.0.to_owned(), count);
    }
    anyhow::ensure!(
        observed_tag_counts == expected_tag_counts,
        "production NotePlugin tag query did not return the four expected underscore tags"
    );
    let no_results = note_plugin.search(NO_RESULTS_QUERY);
    anyhow::ensure!(
        no_results.is_empty(),
        "production NotePlugin no-results query unexpectedly returned actions"
    );
    let production_semantics = ProductionSemanticsObservation {
        canonical_notes_dir: notes_dir.to_string_lossy().into_owned(),
        loaded_note_count: loaded_notes.len(),
        note_queries: query_observations,
        no_results_query: NO_RESULTS_QUERY.to_owned(),
        no_results_count: no_results.len(),
        observed_tags: observed_tag_counts
            .iter()
            .map(|(tag, count)| TagObservation {
                tag: tag.clone(),
                count: *count,
            })
            .collect(),
    };
    println!(
        "production_semantics={}",
        serde_json::to_string(&production_semantics)?
    );

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
        manifest == fixture_manifest(note_ids(), scenario),
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
    anyhow::ensure!(manifest.safety.reduce_dashboard_work_when_unfocused == false);
    anyhow::ensure!(
        manifest.experimental_jump_mode_enabled == scenario.jump_mode_enabled()
            && manifest.safety.experimental_jump_mode_enabled == scenario.jump_mode_enabled(),
        "fixture manifest Jump Mode gate does not match the requested scenario"
    );

    let resolved_notes_dir = fs::canonicalize(notes_dir)?;
    anyhow::ensure!(
        resolved_notes_dir.starts_with(&profile),
        "fixture notes escaped the isolated profile"
    );
    Ok(())
}

fn dashboard_fixture(scenario: FixtureScenario) -> DashboardConfig {
    if scenario.phase1() {
        return DashboardConfig {
            version: 1,
            grid: GridConfig { rows: 2, cols: 2 },
            slots: vec![
                dashboard_slot("jump-qa-phase1-list-a", 0, 0, "note list"),
                dashboard_slot("jump-qa-phase1-list-b", 0, 1, "note list"),
                dashboard_slot("jump-qa-phase1-list-c", 1, 0, "note list"),
                dashboard_slot("jump-qa-phase1-commands", 1, 1, "note"),
            ],
        };
    }
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

fn dashboard_slot_ids(scenario: FixtureScenario) -> Vec<String> {
    if scenario.phase1() {
        return vec![
            "jump-qa-phase1-list-a".into(),
            "jump-qa-phase1-list-b".into(),
            "jump-qa-phase1-list-c".into(),
            "jump-qa-phase1-commands".into(),
        ];
    }
    vec![
        "jump-qa-note-list".into(),
        "jump-qa-note-tags".into(),
        "jump-qa-note-search".into(),
    ]
}

fn dashboard_queries(scenario: FixtureScenario) -> Vec<(&'static str, &'static str)> {
    if scenario.phase1() {
        vec![
            ("jump-qa-phase1-list-a", "note list"),
            ("jump-qa-phase1-list-b", "note list"),
            ("jump-qa-phase1-list-c", "note list"),
            ("jump-qa-phase1-commands", "note"),
        ]
    } else {
        vec![
            ("jump-qa-note-list", "note list"),
            ("jump-qa-note-tags", "note tags"),
            ("jump-qa-note-search", "note search"),
        ]
    }
}

fn note_ids() -> Vec<String> {
    (1..=NOTE_COUNT)
        .map(|index| format!("jump-qa-note-{index:03}"))
        .collect()
}

fn fixture_note_title(scenario: FixtureScenario, ordinal: usize) -> String {
    if scenario.phase1() && ordinal <= 2 {
        "Jump QA Shared Note".to_owned()
    } else {
        format!("Jump QA Note {ordinal:03}")
    }
}

fn fixture_manifest(note_ids: Vec<String>, scenario: FixtureScenario) -> FixtureManifest {
    let jump_mode_enabled = scenario.jump_mode_enabled();
    FixtureManifest {
        schema_version: 2,
        fixture_id: match scenario {
            FixtureScenario::QueryProbe => "jump-mode-m05-query-probe-v1".to_owned(),
            FixtureScenario::Phase1Enabled => "jump-mode-m08-phase1-enabled-v1".to_owned(),
            FixtureScenario::Phase1Disabled => "jump-mode-m08-phase1-disabled-v1".to_owned(),
            FixtureScenario::Baseline => FIXTURE_ID.to_owned(),
        },
        scenario: scenario.label().to_owned(),
        experimental_jump_mode_enabled: jump_mode_enabled,
        launcher_hotkey: LAUNCHER_HOTKEY.to_owned(),
        enabled_plugins: vec![SAFE_PLUGIN.to_owned()],
        dashboard_slot_ids: dashboard_slot_ids(scenario),
        note_ids,
        action_count: 0,
        external_note_open: "Neither".to_owned(),
        no_results_query: NO_RESULTS_QUERY.to_owned(),
        safety: SafetyManifest {
            profile_isolated_by_working_directory: true,
            notes_path_supplied_by_ml_notes_dir: true,
            clipboard_modify_plugin_enabled: false,
            macros_enabled: false,
            network_plugin_enabled: false,
            custom_actions_present: false,
            dashboard_click_behavior: "fill_query".to_owned(),
            reduce_dashboard_work_when_unfocused: false,
            experimental_jump_mode_enabled: jump_mode_enabled,
        },
    }
}

fn set_isolated_fixture_dirs(profile: &Path) -> anyhow::Result<(PathBuf, PathBuf)> {
    let profile = fs::canonicalize(profile)?;
    let notes_dir = fs::canonicalize(profile.join("notes"))?;
    anyhow::ensure!(
        notes_dir != profile && notes_dir.starts_with(&profile),
        "fixture notes path must resolve below the isolated profile"
    );
    let templates_dir = fs::canonicalize(profile.join("templates"))?;
    anyhow::ensure!(
        templates_dir != profile && templates_dir.starts_with(&profile),
        "fixture templates path must resolve below the isolated profile"
    );
    // SAFETY: this binary is a single-threaded fixture CLI. The generator and
    // verifier set both isolated data paths before constructing NotePlugin or
    // starting any production worker/watcher threads; neither path spawns
    // threads earlier. Canonical containment is checked before environment
    // mutation and before any production loader/plugin reads those paths.
    unsafe { env::set_var("ML_NOTES_DIR", &notes_dir) };
    unsafe { env::set_var("ML_NOTE_TEMPLATES_DIR", &templates_dir) };
    Ok((notes_dir, templates_dir))
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
        let fixture = fixture_manifest(ids, FixtureScenario::Baseline);
        assert_eq!(fixture.enabled_plugins, ["notes"]);
        assert_eq!(fixture.action_count, 0);
        assert_eq!(LAUNCHER_HOTKEY, "Ctrl+Alt+F10");
        assert!(!fixture.safety.clipboard_modify_plugin_enabled);
        assert!(!fixture.safety.macros_enabled);
        assert!(!fixture.safety.network_plugin_enabled);
        assert!(!fixture.experimental_jump_mode_enabled);
        assert!(!fixture.safety.experimental_jump_mode_enabled);
    }

    #[test]
    fn query_probe_fixture_explicitly_enables_the_experimental_gate() {
        let fixture = fixture_manifest(note_ids(), FixtureScenario::QueryProbe);
        assert_eq!(fixture.scenario, "query-probe");
        assert_eq!(fixture.fixture_id, "jump-mode-m05-query-probe-v1");
        assert!(fixture.experimental_jump_mode_enabled);
        assert!(fixture.safety.experimental_jump_mode_enabled);
    }

    #[test]
    fn phase1_fixtures_use_safe_dense_fill_query_slots_and_distinct_note_slugs() {
        let enabled = fixture_manifest(note_ids(), FixtureScenario::Phase1Enabled);
        let disabled = fixture_manifest(note_ids(), FixtureScenario::Phase1Disabled);
        assert!(enabled.experimental_jump_mode_enabled);
        assert!(!disabled.experimental_jump_mode_enabled);
        assert_eq!(enabled.dashboard_slot_ids.len(), 4);
        assert_eq!(enabled.dashboard_slot_ids, disabled.dashboard_slot_ids);

        let dashboard = dashboard_fixture(FixtureScenario::Phase1Enabled);
        assert_eq!(dashboard.slots.len(), 4);
        assert_eq!(dashboard.slots[0].settings["query"], "note list");
        assert_eq!(dashboard.slots[1].settings["query"], "note list");
        assert_eq!(dashboard.slots[2].settings["query"], "note list");
        assert_eq!(dashboard.slots[3].settings["query"], "note");
        assert!(dashboard.slots.iter().all(|slot| {
            slot.widget == "pinned_query_results" && slot.settings["click_behavior"] == "fill_query"
        }));

        let slugs = note_ids();
        assert_eq!(
            fixture_note_title(FixtureScenario::Phase1Enabled, 1),
            "Jump QA Shared Note"
        );
        assert_eq!(
            fixture_note_title(FixtureScenario::Phase1Enabled, 2),
            "Jump QA Shared Note"
        );
        assert_ne!(slugs[0], slugs[1]);
        assert_ne!(
            fixture_note_title(FixtureScenario::Phase1Enabled, 3),
            fixture_note_title(FixtureScenario::Phase1Enabled, 1)
        );
    }

    #[test]
    fn dashboard_fixture_only_fills_safe_note_queries() {
        let dashboard = dashboard_fixture(FixtureScenario::Baseline);
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

    #[test]
    fn fixture_hotkey_passes_production_route_validation_and_f12_conflicts() {
        let mut settings = Settings::default();
        settings.hotkey = Some(LAUNCHER_HOTKEY.to_owned());
        settings.quit_hotkey = None;
        settings.help_hotkey = None;
        assert!(validate_hotkey_routes(&settings, &RadialDocument::starter(), None).is_ok());

        settings.hotkey = Some("Ctrl+Alt+F12".to_owned());
        let issues =
            validate_hotkey_routes(&settings, &RadialDocument::starter(), None).unwrap_err();
        assert!(issues.iter().any(|issue| {
            issue
                .message
                .contains("launcher hotkey can co-fire with Screen Draw emergency hotkey")
        }));
    }
}
