use multi_launcher::actions::{Action, load_actions_typed};
use multi_launcher::common::persistence::LoadState;
use multi_launcher::dashboard::config::{DashboardConfig, GridConfig, OverflowMode, SlotConfig};
use multi_launcher::dashboard::widgets::{ClickBehavior, PinnedQueryResultsConfig};
use multi_launcher::file_search::settings::FileSearchSettings;
use multi_launcher::history::{HISTORY_PINS_FILE, HistoryPin};
use multi_launcher::hotkey::validation::validate_hotkey_routes;
use multi_launcher::mkmacro::hotkeys::{HotkeyDiagnosticSeverity, validate_hotkeys};
use multi_launcher::mkmacro::store::ASSET_DIRECTORY;
use multi_launcher::mkmacro::store::LoadDisposition;
use multi_launcher::mkmacro::templates::{MKMACRO_TEMPLATES_FILE, MkMacroTemplateCatalog};
use multi_launcher::mkmacro::{
    DiagnosticSeverity, MkAction, MkDelayMode, MkDelayPayload, MkErrorPolicy, MkHotkey, MkKey,
    MkMacro, MkMacroDocument, MkMacroSettings, MkMacroStore, MkStep, MkStepMetadata,
    validate_document,
};
use multi_launcher::paste_sequence::{
    PASTE_SEQUENCES_FILE, PlaybackMode, SequenceDraft, SequenceStore, Step as SequenceStep,
};
use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::bookmarks::BOOKMARKS_FILE;
use multi_launcher::plugins::clipboard::CLIPBOARD_FILE;
use multi_launcher::plugins::fav::{FAV_FILE, FavEntry};
use multi_launcher::plugins::folders::{FOLDERS_FILE, FolderEntry};
use multi_launcher::plugins::note::{NoteExternalOpen, NotePlugin, NotePluginSettings, load_notes};
use multi_launcher::plugins::snippets::SNIPPETS_FILE;
use multi_launcher::plugins::timer::ALARMS_FILE;
use multi_launcher::plugins::todo::{TODO_FILE, TodoEntry, load_todos_typed};
use multi_launcher::radial::model::RadialDocument;
use multi_launcher::settings::{LogFile, NoteViewMode, PasteSequencePreferences, Settings};
use multi_launcher::usage::{USAGE_FILE, UsageEntry};
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
const FILE_SEARCH_ROOT_DIRECTORY: &str = "file-search-root";
const FILE_SEARCH_FIXTURE_FILE: &str = "jump-mode-safe-fixture.txt";
const FILE_SEARCH_FIXTURE_CONTENT: &str = "Jump Mode isolated File Search fixture.\n";
const TARGET_REPAIR_ACTION_LABEL: &str = "Jump QA Repairable App";
const TARGET_REPAIR_ACTION_DESCRIPTION: &str =
    "Inert saved target for the isolated repair scenario.";
const TARGET_REPAIR_ACTION_ARGS: &str = "--fixture-argument keep-exactly";
const TARGET_REPAIR_UNRELATED_ACTION_LABEL: &str = "Jump QA Unrelated Inert App";
const TARGET_REPAIR_UNRELATED_ACTION_DESCRIPTION: &str =
    "Unrelated inert saved action retained during target repair.";
const TARGET_REPAIR_FOLDER_LABEL: &str = "Jump QA Repairable Folder";
const TARGET_REPAIR_FOLDER_ALIAS: &str = "Jump QA Repairable Folder Alias";
const TARGET_REPAIR_MISSING_ACTION: &str = "target-repair/missing-source/old application Ω.exe";
const TARGET_REPAIR_MISSING_FOLDER: &str = "target-repair/missing-source/old folder Ω";
const TARGET_REPAIR_REPLACEMENT_FILE: &str = "target-repair/replacements/Selected document Ω.txt";
const TARGET_REPAIR_REPLACEMENT_FOLDER: &str = "target-repair/replacements/Selected folder Ω";
const TARGET_REPAIR_ACTION_FAVORITE_LABEL: &str = "QA favorite for repairable app";
const TARGET_REPAIR_FOLDER_FAVORITE_LABEL: &str = "QA favorite for repairable folder";
const TARGET_REPAIR_UNRELATED_FAVORITE_LABEL: &str = "QA unrelated favorite retained";
const TARGET_REPAIR_UNRELATED_PATH: &str = "target-repair/unrelated/untouched.exe";
const TARGET_REPAIR_ACTION_PIN_QUERY: &str = "QA pinned action query";
const TARGET_REPAIR_FOLDER_PIN_QUERY: &str = "QA pinned folder query";
const TARGET_REPAIR_UNRELATED_PIN_QUERY: &str = "QA unrelated pin query retained";
const TARGET_REPAIR_ACTION_USAGE: u32 = 7;
const TARGET_REPAIR_FOLDER_USAGE: u32 = 5;
const TARGET_REPAIR_UNRELATED_USAGE: u32 = 11;

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct FixtureManifest {
    schema_version: u32,
    fixture_id: String,
    scenario: String,
    phase2_surface: Option<String>,
    qa_empty_paste_invocation_context: bool,
    experimental_jump_mode_enabled: bool,
    launcher_hotkey: String,
    enabled_plugins: Vec<String>,
    dashboard_slot_ids: Vec<String>,
    note_ids: Vec<String>,
    action_count: usize,
    #[serde(default)]
    target_repair: Option<TargetRepairManifest>,
    external_note_open: String,
    no_results_query: String,
    safety: SafetyManifest,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct TargetRepairManifest {
    outcome: TargetRepairOutcome,
    action_label: String,
    action_description: String,
    action_args: Option<String>,
    unrelated_action_label: String,
    unrelated_action_description: String,
    missing_action_path: String,
    folder_label: String,
    folder_alias: Option<String>,
    missing_folder_path: String,
    replacement_file: String,
    replacement_folder: String,
    associations: TargetRepairAssociationsManifest,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum TargetRepairOutcome {
    FileCancel,
    FileConfirm,
    FolderConfirm,
}

impl TargetRepairOutcome {
    fn parse(value: Option<String>) -> anyhow::Result<Self> {
        match value.as_deref() {
            Some("file-cancel") => Ok(Self::FileCancel),
            Some("file-confirm") => Ok(Self::FileConfirm),
            Some("folder-confirm") => Ok(Self::FolderConfirm),
            Some(value) => anyhow::bail!("unknown TargetRepair outcome {value:?}"),
            None => anyhow::bail!(
                "TargetRepair requires one outcome: file-cancel, file-confirm, or folder-confirm"
            ),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::FileCancel => "file-cancel",
            Self::FileConfirm => "file-confirm",
            Self::FolderConfirm => "folder-confirm",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct TargetRepairAssociationsManifest {
    action_favorite_label: String,
    folder_favorite_label: String,
    unrelated_favorite_label: String,
    unrelated_path: String,
    action_pin_query: String,
    folder_pin_query: String,
    unrelated_pin_query: String,
    action_usage_count: u32,
    folder_usage_count: u32,
    unrelated_usage_count: u32,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct SafetyManifest {
    profile_isolated_by_working_directory: bool,
    notes_path_supplied_by_ml_notes_dir: bool,
    clipboard_plugin_enabled: bool,
    clipboard_modify_plugin_enabled: bool,
    macros_enabled: bool,
    network_plugin_enabled: bool,
    custom_actions_present: bool,
    dashboard_click_behavior: String,
    reduce_dashboard_work_when_unfocused: bool,
    experimental_jump_mode_enabled: bool,
    mkmacro_plugin_enabled: bool,
    file_search_root_isolated: bool,
    file_search_fixture_file_count: usize,
    file_search_everything_enabled: bool,
    qa_empty_paste_invocation_context: bool,
    paste_next_hotkey_enabled: bool,
    paste_cancel_hotkey_enabled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FixtureScenario {
    Baseline,
    QueryProbe,
    Phase1Enabled,
    Phase1Disabled,
    Phase2(Phase2Surface),
    TargetRepair,
    NoteReplaceEnabled,
    NoteReplaceDisabled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase2Surface {
    Settings,
    Notes,
    Todo,
    Timer,
    Paste,
    MkMacro,
    FileSearch,
}

impl Phase2Surface {
    fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "settings" => Self::Settings,
            "notes" => Self::Notes,
            "todo" => Self::Todo,
            "timer" => Self::Timer,
            "paste" => Self::Paste,
            "mkmacro" => Self::MkMacro,
            "file-search" => Self::FileSearch,
            _ => return None,
        })
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Settings => "settings",
            Self::Notes => "notes",
            Self::Todo => "todo",
            Self::Timer => "timer",
            Self::Paste => "paste",
            Self::MkMacro => "mkmacro",
            Self::FileSearch => "file-search",
        }
    }

    fn enabled_plugins(self) -> &'static [&'static str] {
        match self {
            Self::Settings => &["notes", "settings"],
            Self::Notes => &["notes"],
            Self::Todo => &["notes", "todo"],
            Self::Timer => &["notes", "timer"],
            Self::Paste => &["notes", "paste_sequence"],
            Self::MkMacro => &["notes", "mkmacro"],
            Self::FileSearch => &["notes", "file_search"],
        }
    }
}

impl FixtureScenario {
    fn parse(value: Option<String>) -> anyhow::Result<Self> {
        match value.as_deref().unwrap_or("baseline") {
            "baseline" => Ok(Self::Baseline),
            "query-probe" => Ok(Self::QueryProbe),
            "phase1-enabled" => Ok(Self::Phase1Enabled),
            "phase1-disabled" => Ok(Self::Phase1Disabled),
            "target-repair" => Ok(Self::TargetRepair),
            "note-replace-enabled" => Ok(Self::NoteReplaceEnabled),
            "note-replace-disabled" => Ok(Self::NoteReplaceDisabled),
            value if value.starts_with("phase2-") => {
                let surface = &value["phase2-".len()..];
                Phase2Surface::parse(surface)
                    .map(Self::Phase2)
                    .ok_or_else(|| anyhow::anyhow!("unknown Phase 2 surface {surface:?}"))
            }
            value => anyhow::bail!(
                "unknown fixture scenario {value:?}; expected baseline, query-probe, phase1-enabled, phase1-disabled, target-repair, note-replace-enabled, note-replace-disabled, or phase2-settings/notes/todo/timer/paste/mkmacro/file-search"
            ),
        }
    }

    fn jump_mode_enabled(self) -> bool {
        matches!(
            self,
            Self::QueryProbe
                | Self::Phase1Enabled
                | Self::Phase2(_)
                | Self::TargetRepair
                | Self::NoteReplaceEnabled
        )
    }

    fn phase1(self) -> bool {
        matches!(self, Self::Phase1Enabled | Self::Phase1Disabled)
    }

    fn phase2_surface(self) -> Option<Phase2Surface> {
        match self {
            Self::Phase2(surface) => Some(surface),
            Self::TargetRepair
            | Self::NoteReplaceEnabled
            | Self::NoteReplaceDisabled
            | Self::Baseline
            | Self::QueryProbe
            | Self::Phase1Enabled
            | Self::Phase1Disabled => None,
        }
    }

    fn enabled_plugins(self) -> Vec<String> {
        if self == Self::TargetRepair {
            return vec![
                "folders".to_owned(),
                "missing".to_owned(),
                SAFE_PLUGIN.to_owned(),
            ];
        }
        self.phase2_surface().map_or_else(
            || vec![SAFE_PLUGIN.to_owned()],
            |surface| {
                surface
                    .enabled_plugins()
                    .iter()
                    .map(|plugin| (*plugin).to_owned())
                    .collect()
            },
        )
    }

    fn label(self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::QueryProbe => "query-probe",
            Self::Phase1Enabled => "phase1-enabled",
            Self::Phase1Disabled => "phase1-disabled",
            Self::Phase2(surface) => match surface {
                Phase2Surface::Settings => "phase2-settings",
                Phase2Surface::Notes => "phase2-notes",
                Phase2Surface::Todo => "phase2-todo",
                Phase2Surface::Timer => "phase2-timer",
                Phase2Surface::Paste => "phase2-paste",
                Phase2Surface::MkMacro => "phase2-mkmacro",
                Phase2Surface::FileSearch => "phase2-file-search",
            },
            Self::TargetRepair => "target-repair",
            Self::NoteReplaceEnabled => "note-replace-enabled",
            Self::NoteReplaceDisabled => "note-replace-disabled",
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
        .ok_or_else(|| anyhow::anyhow!("usage: jump_mode_fixture <generate|verify> <profile> [scenario] [target-repair-outcome]"))?;
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
    let target_repair_outcome = if scenario == FixtureScenario::TargetRepair {
        let outcome = args
            .next()
            .map(|value| {
                value.into_string().map_err(|_| {
                    anyhow::anyhow!("TargetRepair outcome argument must be valid Unicode")
                })
            })
            .transpose()?;
        Some(TargetRepairOutcome::parse(outcome)?)
    } else {
        None
    };
    anyhow::ensure!(args.next().is_none(), "unexpected extra argument");

    match mode.as_str() {
        "generate" => generate(&profile, scenario, target_repair_outcome)?,
        "verify" => verify(&profile, scenario, target_repair_outcome)?,
        _ => anyhow::bail!("unknown mode {mode:?}; expected generate or verify"),
    }
    println!(
        "fixture {mode} verified at {} for {}",
        profile.display(),
        scenario.label()
    );
    Ok(())
}

fn generate(
    profile: &Path,
    scenario: FixtureScenario,
    target_repair_outcome: Option<TargetRepairOutcome>,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        (scenario == FixtureScenario::TargetRepair) == target_repair_outcome.is_some(),
        "TargetRepair fixture generation requires exactly one explicit outcome"
    );
    anyhow::ensure!(
        !profile.exists(),
        "refusing to overwrite an existing fixture profile: {}",
        profile.display()
    );
    fs::create_dir_all(profile)?;
    let target_repair_profile = (scenario == FixtureScenario::TargetRepair)
        .then(|| fs::canonicalize(profile))
        .transpose()?;
    let notes_dir = profile.join("notes");
    fs::create_dir(&notes_dir)?;
    let templates_dir = profile.join("templates");
    fs::create_dir(&templates_dir)?;
    set_isolated_fixture_dirs(profile)?;
    let screenshot_dir = profile.join("screenshot-output");
    fs::create_dir(&screenshot_dir)?;

    let mut settings = Settings {
        hotkey: Some(LAUNCHER_HOTKEY.into()),
        help_hotkey: None,
        quit_hotkey: None,
        paste_sequence: PasteSequencePreferences {
            next_hotkey: None,
            cancel_hotkey: None,
            ..Default::default()
        },
        ..Default::default()
    };
    settings.window_size = if scenario.phase1()
        || scenario.phase2_surface().is_some()
        || scenario == FixtureScenario::TargetRepair
        || matches!(
            scenario,
            FixtureScenario::NoteReplaceEnabled | FixtureScenario::NoteReplaceDisabled
        ) {
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
    settings.enabled_plugins = Some(scenario.enabled_plugins().into_iter().collect());
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
    if matches!(
        scenario,
        FixtureScenario::NoteReplaceEnabled | FixtureScenario::NoteReplaceDisabled
    ) {
        settings.note.default_view_mode = NoteViewMode::Edit;
    }
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
    if scenario.phase2_surface() == Some(Phase2Surface::FileSearch) {
        let file_search_settings = FileSearchSettings {
            global_search_roots: vec![fs::canonicalize(profile)?.join(FILE_SEARCH_ROOT_DIRECTORY)],
            everything_enabled: false,
            ripgrep_executable_path: fs::canonicalize(profile)?.join("fixture-rg-unavailable.exe"),
            ..Default::default()
        };
        settings.plugin_settings.insert(
            "file_search".into(),
            serde_json::to_value(file_search_settings)?,
        );
    }

    write_json(&profile.join("settings.json"), &settings)?;
    let actions = if scenario == FixtureScenario::TargetRepair {
        target_repair_actions(
            target_repair_profile
                .as_deref()
                .expect("Target Repair profile was canonicalized"),
        )
    } else {
        Vec::new()
    };
    write_json(&profile.join("actions.json"), &actions)?;

    let dashboard = dashboard_fixture(scenario);
    write_json(&profile.join("dashboard.json"), &dashboard)?;

    let note_ids = note_ids();
    for (index, note_id) in note_ids.iter().enumerate() {
        let ordinal = index + 1;
        let title = fixture_note_title(scenario, ordinal);
        let note_text = fixture_note_text(scenario, ordinal, note_id, &title);
        fs::write(notes_dir.join(format!("{note_id}.md")), note_text)?;
    }

    // Constructors and background pollers can run before plugin allowlists
    // are applied. Every profile therefore owns safe CWD stores up front.
    write_phase2_stores(profile, scenario.phase2_surface())?;
    if let Some(target_repair_profile) = target_repair_profile.as_deref() {
        write_target_repair_stores(target_repair_profile)?;
    }

    let manifest = fixture_manifest(note_ids, scenario, target_repair_outcome);
    write_json(&profile.join("fixture_manifest.json"), &manifest)?;
    verify(profile, scenario, target_repair_outcome)
}

fn verify(
    profile: &Path,
    scenario: FixtureScenario,
    target_repair_outcome: Option<TargetRepairOutcome>,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        (scenario == FixtureScenario::TargetRepair) == target_repair_outcome.is_some(),
        "TargetRepair fixture verification requires exactly one explicit outcome"
    );
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
        enabled_plugins.iter().cloned().collect::<HashSet<_>>()
            == scenario.enabled_plugins().into_iter().collect(),
        "fixture plugin allowlist does not match the safe scenario-specific allowlist"
    );
    if scenario.phase2_surface() == Some(Phase2Surface::FileSearch) {
        verify_file_search_settings(&profile, &settings)?;
    } else {
        anyhow::ensure!(
            !settings.plugin_settings.contains_key("file_search"),
            "non-File Search fixtures must not configure File Search roots"
        );
    }
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
    anyhow::ensure!(
        settings.paste_sequence.next_hotkey.is_none()
            && settings.paste_sequence.cancel_hotkey.is_none(),
        "all fixture profiles must keep Paste Sequence Next and Cancel shortcuts disabled"
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
    if matches!(
        scenario,
        FixtureScenario::NoteReplaceEnabled | FixtureScenario::NoteReplaceDisabled
    ) {
        anyhow::ensure!(
            settings.note.default_view_mode == NoteViewMode::Edit,
            "NoteReplace fixture must open the generated note in Edit without changing production defaults"
        );
    }
    anyhow::ensure!(
        settings.plugin_settings.contains_key("clipboard_modify"),
        "clipboard migration guard is missing"
    );
    anyhow::ensure!(
        !enabled_plugins.contains("clipboard_modify")
            && !enabled_plugins.contains("clipboard")
            && !enabled_plugins.contains("macros")
            && (scenario.phase2_surface() == Some(Phase2Surface::MkMacro)
                || !enabled_plugins.contains("mkmacro"))
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
    if scenario == FixtureScenario::TargetRepair {
        let expected = target_repair_actions(&profile);
        anyhow::ensure!(
            actions == expected,
            "Target Repair fixture must contain its exact repair action followed by one inert unrelated action"
        );
        verify_target_repair_stores(&profile)?;
    } else {
        anyhow::ensure!(
            actions.is_empty(),
            "fixture must not configure executable actions"
        );
        anyhow::ensure!(
            !profile.join(FOLDERS_FILE).exists(),
            "non-repair fixtures must not create saved folder entries"
        );
    }

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
    if matches!(
        scenario,
        FixtureScenario::NoteReplaceEnabled | FixtureScenario::NoteReplaceDisabled
    ) {
        let first = fs::read_to_string(notes_dir.join("jump-qa-note-001.md"))?;
        let second = fs::read_to_string(notes_dir.join("jump-qa-note-002.md"))?;
        anyhow::ensure!(
            first
                == fixture_note_text(
                    scenario,
                    1,
                    "jump-qa-note-001",
                    &fixture_note_title(scenario, 1),
                ),
            "Note Replace fixture source is not the exact CRLF/Unicode/literal input"
        );
        anyhow::ensure!(
            second
                == fixture_note_text(
                    scenario,
                    2,
                    "jump-qa-note-002",
                    &fixture_note_title(scenario, 2),
                ),
            "Note Replace fixture's unrelated second note changed"
        );
    }
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
    if matches!(
        scenario,
        FixtureScenario::NoteReplaceEnabled | FixtureScenario::NoteReplaceDisabled
    ) {
        let expected_title = fixture_note_title(scenario, 1);
        let loaded_target = loaded_notes
            .iter()
            .find(|note| note.slug == "jump-qa-note-001")
            .ok_or_else(|| anyhow::anyhow!("Note Replace target note was not loaded"))?;
        anyhow::ensure!(
            loaded_target.title == expected_title,
            "production note loader title {:?} does not match fixture title {:?}",
            loaded_target.title,
            expected_title
        );
        let title_query = format!("note open {expected_title}");
        let title_actions = note_plugin.search(&title_query);
        anyhow::ensure!(
            title_actions.len() == 1 && title_actions[0].action == "note:open:jump-qa-note-001",
            "production NotePlugin query {title_query:?} must uniquely resolve the generated target note"
        );
    }
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
        let expected_query = if scenario.phase1() || slot_id == "jump-qa-note-list" {
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
        manifest == fixture_manifest(note_ids(), scenario, target_repair_outcome),
        "fixture manifest is stale"
    );
    anyhow::ensure!(
        manifest.action_count
            == if scenario == FixtureScenario::TargetRepair {
                2
            } else {
                0
            },
        "fixture action count does not match its isolated scenario"
    );
    anyhow::ensure!(manifest.safety.clipboard_modify_plugin_enabled == false);
    anyhow::ensure!(manifest.safety.clipboard_plugin_enabled == false);
    anyhow::ensure!(manifest.safety.macros_enabled == false);
    anyhow::ensure!(manifest.safety.network_plugin_enabled == false);
    anyhow::ensure!(
        manifest.safety.custom_actions_present == (scenario == FixtureScenario::TargetRepair),
        "fixture custom-action claim does not match the isolated scenario"
    );
    anyhow::ensure!(manifest.safety.reduce_dashboard_work_when_unfocused == false);
    anyhow::ensure!(
        manifest.experimental_jump_mode_enabled == scenario.jump_mode_enabled()
            && manifest.safety.experimental_jump_mode_enabled == scenario.jump_mode_enabled(),
        "fixture manifest Jump Mode gate does not match the requested scenario"
    );
    anyhow::ensure!(
        manifest.phase2_surface.as_deref() == scenario.phase2_surface().map(Phase2Surface::as_str),
        "fixture manifest Phase 2 surface does not match the requested scenario"
    );
    let empty_paste_context = scenario.phase2_surface() == Some(Phase2Surface::Paste);
    anyhow::ensure!(
        manifest.qa_empty_paste_invocation_context == empty_paste_context
            && manifest.safety.qa_empty_paste_invocation_context == empty_paste_context,
        "fixture empty-context capability must be enabled only for the Phase 2 Paste profile"
    );
    anyhow::ensure!(
        !manifest.safety.paste_next_hotkey_enabled && !manifest.safety.paste_cancel_hotkey_enabled,
        "fixture manifest must record disabled Paste Sequence shortcuts"
    );
    anyhow::ensure!(
        manifest.safety.mkmacro_plugin_enabled
            == (scenario.phase2_surface() == Some(Phase2Surface::MkMacro)),
        "fixture manifest MkMacro allowlist claim does not match the selected surface"
    );
    anyhow::ensure!(
        manifest.safety.file_search_root_isolated
            == (scenario.phase2_surface() == Some(Phase2Surface::FileSearch))
            && manifest.safety.file_search_fixture_file_count
                == if scenario.phase2_surface() == Some(Phase2Surface::FileSearch) {
                    1
                } else {
                    0
                }
            && !manifest.safety.file_search_everything_enabled,
        "fixture manifest File Search safety claims do not match the selected surface"
    );

    verify_phase2_stores(&profile, scenario.phase2_surface())?;

    let resolved_notes_dir = fs::canonicalize(notes_dir)?;
    anyhow::ensure!(
        resolved_notes_dir.starts_with(&profile),
        "fixture notes escaped the isolated profile"
    );
    Ok(())
}

fn verify_file_search_settings(profile: &Path, settings: &Settings) -> anyhow::Result<()> {
    let profile = fs::canonicalize(profile)?;
    let expected_root = profile.join(FILE_SEARCH_ROOT_DIRECTORY);
    let value = settings
        .plugin_settings
        .get("file_search")
        .ok_or_else(|| anyhow::anyhow!("File Search fixture settings are missing"))?;
    let file_search: FileSearchSettings = serde_json::from_value(value.clone())?;
    anyhow::ensure!(
        file_search.global_search_roots.len() == 1
            && file_search.global_search_roots[0] == expected_root
            && expected_root.starts_with(&profile)
            && fs::canonicalize(&expected_root)? == expected_root,
        "File Search must configure only its canonical isolated fixture root"
    );
    anyhow::ensure!(
        !file_search.everything_enabled,
        "Everything global-search integration must be disabled in the fixture"
    );
    anyhow::ensure!(
        file_search.ripgrep_executable_path == profile.join("fixture-rg-unavailable.exe")
            && !file_search.ripgrep_executable_path.exists()
            && file_search.preferred_editor_command.is_empty()
            && file_search.preferred_editor_args.is_empty()
            && file_search.preferred_terminal_command.is_empty()
            && file_search.preferred_terminal_args.is_empty(),
        "File Search fixture must not configure external tools or commands"
    );
    Ok(())
}

fn verify_file_search_fixture_root(
    profile: &Path,
    surface: Option<Phase2Surface>,
) -> anyhow::Result<()> {
    let root = profile.join(FILE_SEARCH_ROOT_DIRECTORY);
    if surface != Some(Phase2Surface::FileSearch) {
        anyhow::ensure!(
            !root.exists(),
            "non-File Search fixture contains an unexpected search root"
        );
        return Ok(());
    }

    let profile = fs::canonicalize(profile)?;
    let root = fs::canonicalize(root)?;
    anyhow::ensure!(
        root == profile.join(FILE_SEARCH_ROOT_DIRECTORY)
            && root != profile
            && root.starts_with(&profile),
        "File Search fixture root must remain inside its isolated profile"
    );
    let entries = fs::read_dir(&root)?.collect::<Result<Vec<_>, _>>()?;
    anyhow::ensure!(
        entries.len() == 1 && entries[0].file_name() == FILE_SEARCH_FIXTURE_FILE,
        "File Search fixture root must contain exactly one deterministic harmless file"
    );
    anyhow::ensure!(
        entries[0].file_type()?.is_file()
            && fs::read(entries[0].path())? == FILE_SEARCH_FIXTURE_CONTENT.as_bytes(),
        "File Search fixture file must contain only its fixed harmless text"
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

fn phase2_recorder_hotkey() -> MkHotkey {
    MkHotkey {
        key: MkKey::Function(24),
        modifiers: vec![MkKey::Control, MkKey::Alt, MkKey::Shift],
    }
}

fn phase2_macro_document(surface: Option<Phase2Surface>) -> MkMacroDocument {
    let mut document = MkMacroDocument {
        settings: MkMacroSettings {
            record_toggle_hotkey: phase2_recorder_hotkey(),
            recorder: Default::default(),
        },
        ..Default::default()
    };
    if surface == Some(Phase2Surface::MkMacro) {
        document.macros.push(MkMacro {
            id: 1,
            name: "Jump QA Inert Macro".into(),
            description: "Disabled fixture macro with one inert delay".into(),
            enabled: false,
            hotkey: None,
            hotkey_scope: Default::default(),
            folder_id: None,
            playback: Default::default(),
            signature: Default::default(),
            steps: vec![MkStep {
                id: 1,
                enabled: false,
                breakpoint: false,
                repeat: 1,
                delay_after_ms: 0,
                on_error: MkErrorPolicy::Stop,
                metadata: MkStepMetadata {
                    label: "Inert zero delay".into(),
                    comment: String::new(),
                    ..Default::default()
                },
                action: MkAction::Delay(MkDelayPayload {
                    mode: MkDelayMode::Fixed,
                    fixed_ms: 0,
                    minimum_ms: 0,
                    maximum_ms: 0,
                }),
            }],
        });
    }
    document
}

fn write_phase2_stores(profile: &Path, surface: Option<Phase2Surface>) -> anyhow::Result<()> {
    let todo = if surface == Some(Phase2Surface::Todo) {
        vec![TodoEntry {
            id: "jump-qa-todo-001".into(),
            text: "Jump QA isolated todo".into(),
            done: false,
            priority: 0,
            tags: Vec::new(),
            entity_refs: Vec::new(),
        }]
    } else {
        Vec::new()
    };
    write_json(&profile.join(TODO_FILE), &todo)?;
    write_json(&profile.join(ALARMS_FILE), &Vec::<Value>::new())?;
    write_json(&profile.join(CLIPBOARD_FILE), &Vec::<Value>::new())?;
    write_json(&profile.join(SNIPPETS_FILE), &Vec::<Value>::new())?;
    write_json(&profile.join("macros.json"), &Vec::<Value>::new())?;
    write_json(
        &profile.join(MKMACRO_TEMPLATES_FILE),
        &MkMacroTemplateCatalog::default(),
    )?;
    fs::create_dir_all(profile.join(ASSET_DIRECTORY))?;
    if surface == Some(Phase2Surface::FileSearch) {
        let root = profile.join(FILE_SEARCH_ROOT_DIRECTORY);
        fs::create_dir(&root)?;
        fs::write(
            root.join(FILE_SEARCH_FIXTURE_FILE),
            FILE_SEARCH_FIXTURE_CONTENT,
        )?;
    }

    let todo_state = load_todos_typed(profile.join(TODO_FILE))?;
    match (surface, todo_state) {
        (Some(Phase2Surface::Todo), LoadState::Loaded(entries)) => anyhow::ensure!(
            entries == todo,
            "typed Todo loader did not preserve the isolated plain fixture entry"
        ),
        (_, LoadState::Loaded(entries)) => anyhow::ensure!(
            entries.is_empty(),
            "typed Todo loader found unexpected entries outside the Todo surface"
        ),
        _ => anyhow::bail!("Todo fixture must use a typed loaded array"),
    }
    let clipboard_state =
        multi_launcher::plugins::clipboard::load_history_typed(profile.join(CLIPBOARD_FILE))?;
    anyhow::ensure!(
        matches!(clipboard_state, LoadState::Loaded(entries) if entries.is_empty()),
        "typed clipboard history store must be an empty array"
    );
    let snippets_state =
        multi_launcher::plugins::snippets::load_snippets_typed(profile.join(SNIPPETS_FILE))?;
    anyhow::ensure!(
        matches!(snippets_state, LoadState::Loaded(entries) if entries.is_empty()),
        "typed snippets store must be an empty array"
    );

    let store = SequenceStore::new(profile.join(PASTE_SEQUENCES_FILE));
    let mut snapshot = store.load()?;
    let mut sequence = SequenceDraft::new(
        "Jump QA Literal Sequence",
        vec![SequenceStep::LiteralText {
            text: "isolated fixture text".into(),
        }],
    );
    sequence.mode = PlaybackMode::ManualAdvance;
    sequence.delay_ms = 0;
    sequence.hotkey = None;
    snapshot.upsert(sequence)?;
    store.commit(&mut snapshot)?;

    let document = phase2_macro_document(surface);
    anyhow::ensure!(
        validate_hotkeys(&document, &[]).is_empty(),
        "Phase 2 recorder chord or macro hotkeys failed production validation"
    );
    let diagnostics = validate_document(&document, None);
    anyhow::ensure!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.severity != DiagnosticSeverity::Fatal),
        "isolated macro document must pass production semantic validation"
    );
    {
        let (store, _) = MkMacroStore::open(profile)?;
        store.save(document)?;
    }
    Ok(())
}

fn verify_phase2_stores(profile: &Path, surface: Option<Phase2Surface>) -> anyhow::Result<()> {
    verify_file_search_fixture_root(profile, surface)?;
    let todo = match load_todos_typed(profile.join(TODO_FILE))? {
        LoadState::Loaded(entries) => entries,
        LoadState::Missing | LoadState::Empty => {
            anyhow::bail!("Todo fixture must use a typed loaded array")
        }
    };
    if surface == Some(Phase2Surface::Todo) {
        anyhow::ensure!(
            todo.len() == 1
                && todo[0].id == "jump-qa-todo-001"
                && !todo[0].done
                && todo[0].priority == 0
                && todo[0].tags.is_empty()
                && todo[0].entity_refs.is_empty(),
            "Todo fixture must be one unfinished plain item without entity references"
        );
    } else {
        anyhow::ensure!(
            todo.is_empty(),
            "unexpected Todo data in this Phase 2 profile"
        );
    }
    for path in [ALARMS_FILE, "macros.json"] {
        let value: Value = serde_json::from_slice(&fs::read(profile.join(path))?)?;
        anyhow::ensure!(
            value.as_array().is_some_and(Vec::is_empty),
            "isolated store {path} must be an empty typed array"
        );
    }
    anyhow::ensure!(
        matches!(
            multi_launcher::plugins::clipboard::load_history_typed(
                profile.join(CLIPBOARD_FILE)
            )?,
            LoadState::Loaded(entries) if entries.is_empty()
        ) && matches!(
            multi_launcher::plugins::snippets::load_snippets_typed(
                profile.join(SNIPPETS_FILE)
            )?,
            LoadState::Loaded(entries) if entries.is_empty()
        ),
        "typed clipboard and snippets stores must be empty"
    );

    let sequence_store = SequenceStore::new(profile.join(PASTE_SEQUENCES_FILE));
    let sequences = sequence_store.load()?;
    anyhow::ensure!(
        sequences.sequences().len() == 1,
        "expected one isolated sequence"
    );
    let sequence = &sequences.sequences()[0];
    anyhow::ensure!(
        sequence.name == "Jump QA Literal Sequence"
            && sequence.hotkey.is_none()
            && sequence.mode == PlaybackMode::ManualAdvance
            && sequence.delay_ms == 0
            && sequence.steps
                == [SequenceStep::LiteralText {
                    text: "isolated fixture text".into(),
                }],
        "Paste fixture must be a manual, unbound, literal-only sequence"
    );

    let (macro_store, disposition) = MkMacroStore::open(profile)?;
    anyhow::ensure!(
        matches!(disposition, LoadDisposition::Loaded),
        "typed MkMacro store must reload the saved isolated document"
    );
    let macro_document = macro_store.snapshot();
    anyhow::ensure!(
        validate_document(&macro_document, None)
            .iter()
            .all(|diagnostic| diagnostic.severity != DiagnosticSeverity::Fatal),
        "reloaded MkMacro document must pass production semantic validation"
    );
    anyhow::ensure!(
        macro_document.settings.record_toggle_hotkey.key == MkKey::Function(24)
            && macro_document.settings.record_toggle_hotkey.modifiers
                == [MkKey::Control, MkKey::Alt, MkKey::Shift]
            && macro_document
                .settings
                .recorder
                .pause_resume_hotkey
                .is_none()
            && macro_document.settings.recorder.marker_hotkey.is_none()
            && validate_hotkeys(&macro_document, &[]).is_empty(),
        "Phase 2 recorder chord must be an UNSENT production-valid Ctrl+Alt+Shift+F24 chord"
    );
    if surface == Some(Phase2Surface::MkMacro) {
        anyhow::ensure!(
            macro_document.macros.len() == 1,
            "MkMacro fixture needs one macro"
        );
        let m = &macro_document.macros[0];
        anyhow::ensure!(
            m.id == 1
                && m.name == "Jump QA Inert Macro"
                && !m.enabled
                && m.hotkey.is_none()
                && m.steps.len() == 1
                && m.steps[0].id == 1
                && !m.steps[0].enabled
                && !m.steps[0].breakpoint
                && m.steps[0].repeat == 1
                && m.steps[0].delay_after_ms == 0
                && m.steps[0].on_error == MkErrorPolicy::Stop
                && m.steps[0].metadata.label == "Inert zero delay"
                && m.steps[0].metadata.comment.is_empty()
                && matches!(
                    m.steps[0].action,
                    MkAction::Delay(MkDelayPayload {
                        mode: MkDelayMode::Fixed,
                        fixed_ms: 0,
                        minimum_ms: 0,
                        maximum_ms: 0,
                    })
                ),
            "MkMacro fixture must remain disabled, unbound, and inert"
        );
        let mut recorder_conflict_probe = (*macro_document).clone();
        let macro_entry = &mut recorder_conflict_probe.macros[0];
        macro_entry.enabled = true;
        macro_entry.hotkey = Some(phase2_recorder_hotkey());
        let conflict_diagnostics = validate_hotkeys(&recorder_conflict_probe, &[]);
        anyhow::ensure!(
            conflict_diagnostics.len() == 1
                && conflict_diagnostics[0].severity == HotkeyDiagnosticSeverity::Error
                && conflict_diagnostics[0].message.contains("recording toggle"),
            "production hotkey compiler must detect the in-memory recorder chord conflict"
        );
    } else {
        anyhow::ensure!(
            macro_document.macros.is_empty(),
            "unexpected macro data in this Phase 2 profile"
        );
    }
    let macro_templates: MkMacroTemplateCatalog =
        serde_json::from_slice(&fs::read(profile.join(MKMACRO_TEMPLATES_FILE))?)?;
    anyhow::ensure!(
        macro_templates == MkMacroTemplateCatalog::default(),
        "isolated MkMacro templates must remain empty"
    );
    let assets = fs::read_dir(profile.join(ASSET_DIRECTORY))?;
    anyhow::ensure!(assets.count() == 0, "Phase 2 macro assets must be empty");
    Ok(())
}

fn note_ids() -> Vec<String> {
    (1..=NOTE_COUNT)
        .map(|index| format!("jump-qa-note-{index:03}"))
        .collect()
}

fn fixture_note_title(scenario: FixtureScenario, ordinal: usize) -> String {
    if scenario.phase1() && ordinal <= 2 {
        "Jump QA Shared Note".to_owned()
    } else if matches!(
        scenario,
        FixtureScenario::NoteReplaceEnabled | FixtureScenario::NoteReplaceDisabled
    ) && ordinal == 1
    {
        "Jump QA Replace Note".to_owned()
    } else {
        format!("Jump QA Note {ordinal:03}")
    }
}

fn fixture_note_text(
    scenario: FixtureScenario,
    ordinal: usize,
    note_id: &str,
    title: &str,
) -> String {
    if matches!(
        scenario,
        FixtureScenario::NoteReplaceEnabled | FixtureScenario::NoteReplaceDisabled
    ) && ordinal == 1
    {
        format!(
            "# {title}\r\n\r\nTag: #jump_qa_bucket_01\r\n\r\n[shown Ω](<literal Ω $1\\path>)\r\n[shown Ω](<literal Ω $1\\path>)\r\nkeep this line\r\n"
        )
    } else {
        format!(
            "# {title}\n\nFixture record {ordinal:03} for the isolated Jump Mode runner.\n\nStable fixture ID: `{note_id}`\nTag: #jump_qa_bucket_{:02}\n",
            ((ordinal - 1) % 4) + 1
        )
    }
}

fn fixture_manifest(
    note_ids: Vec<String>,
    scenario: FixtureScenario,
    target_repair_outcome: Option<TargetRepairOutcome>,
) -> FixtureManifest {
    let jump_mode_enabled = scenario.jump_mode_enabled();
    let phase2_surface = scenario.phase2_surface();
    let enabled_plugins = scenario.enabled_plugins();
    let empty_paste_context = phase2_surface == Some(Phase2Surface::Paste);
    FixtureManifest {
        schema_version: 4,
        fixture_id: match scenario {
            FixtureScenario::QueryProbe => "jump-mode-m05-query-probe-v1".to_owned(),
            FixtureScenario::Phase1Enabled => "jump-mode-m08-phase1-enabled-v1".to_owned(),
            FixtureScenario::Phase1Disabled => "jump-mode-m08-phase1-disabled-v1".to_owned(),
            FixtureScenario::Baseline => FIXTURE_ID.to_owned(),
            FixtureScenario::TargetRepair => format!(
                "jump-mode-m3-target-repair-{}-v1",
                target_repair_outcome
                    .expect("TargetRepair fixtures carry an explicit outcome")
                    .as_str()
            ),
            FixtureScenario::NoteReplaceEnabled => "jump-mode-m9b-note-replace-on-v1".to_owned(),
            FixtureScenario::NoteReplaceDisabled => "jump-mode-m9b-note-replace-off-v1".to_owned(),
            FixtureScenario::Phase2(surface) => {
                format!("jump-mode-m13-{}-v1", surface.as_str())
            }
        },
        scenario: scenario.label().to_owned(),
        phase2_surface: phase2_surface.map(|surface| surface.as_str().to_owned()),
        qa_empty_paste_invocation_context: empty_paste_context,
        experimental_jump_mode_enabled: jump_mode_enabled,
        launcher_hotkey: LAUNCHER_HOTKEY.to_owned(),
        enabled_plugins,
        dashboard_slot_ids: dashboard_slot_ids(scenario),
        note_ids,
        action_count: if scenario == FixtureScenario::TargetRepair {
            2
        } else {
            0
        },
        target_repair: (scenario == FixtureScenario::TargetRepair)
            .then(|| target_repair_manifest(target_repair_outcome.expect("TargetRepair outcome"))),
        external_note_open: "Neither".to_owned(),
        no_results_query: NO_RESULTS_QUERY.to_owned(),
        safety: SafetyManifest {
            profile_isolated_by_working_directory: true,
            notes_path_supplied_by_ml_notes_dir: true,
            clipboard_plugin_enabled: false,
            clipboard_modify_plugin_enabled: false,
            macros_enabled: false,
            network_plugin_enabled: false,
            custom_actions_present: scenario == FixtureScenario::TargetRepair,
            dashboard_click_behavior: "fill_query".to_owned(),
            reduce_dashboard_work_when_unfocused: false,
            experimental_jump_mode_enabled: jump_mode_enabled,
            mkmacro_plugin_enabled: phase2_surface == Some(Phase2Surface::MkMacro),
            file_search_root_isolated: phase2_surface == Some(Phase2Surface::FileSearch),
            file_search_fixture_file_count: if phase2_surface == Some(Phase2Surface::FileSearch) {
                1
            } else {
                0
            },
            file_search_everything_enabled: false,
            qa_empty_paste_invocation_context: empty_paste_context,
            paste_next_hotkey_enabled: false,
            paste_cancel_hotkey_enabled: false,
        },
    }
}

fn target_repair_manifest(outcome: TargetRepairOutcome) -> TargetRepairManifest {
    TargetRepairManifest {
        outcome,
        action_label: TARGET_REPAIR_ACTION_LABEL.to_owned(),
        action_description: TARGET_REPAIR_ACTION_DESCRIPTION.to_owned(),
        action_args: Some(TARGET_REPAIR_ACTION_ARGS.to_owned()),
        unrelated_action_label: TARGET_REPAIR_UNRELATED_ACTION_LABEL.to_owned(),
        unrelated_action_description: TARGET_REPAIR_UNRELATED_ACTION_DESCRIPTION.to_owned(),
        missing_action_path: TARGET_REPAIR_MISSING_ACTION.to_owned(),
        folder_label: TARGET_REPAIR_FOLDER_LABEL.to_owned(),
        folder_alias: Some(TARGET_REPAIR_FOLDER_ALIAS.to_owned()),
        missing_folder_path: TARGET_REPAIR_MISSING_FOLDER.to_owned(),
        replacement_file: TARGET_REPAIR_REPLACEMENT_FILE.to_owned(),
        replacement_folder: TARGET_REPAIR_REPLACEMENT_FOLDER.to_owned(),
        associations: TargetRepairAssociationsManifest {
            action_favorite_label: TARGET_REPAIR_ACTION_FAVORITE_LABEL.to_owned(),
            folder_favorite_label: TARGET_REPAIR_FOLDER_FAVORITE_LABEL.to_owned(),
            unrelated_favorite_label: TARGET_REPAIR_UNRELATED_FAVORITE_LABEL.to_owned(),
            unrelated_path: TARGET_REPAIR_UNRELATED_PATH.to_owned(),
            action_pin_query: TARGET_REPAIR_ACTION_PIN_QUERY.to_owned(),
            folder_pin_query: TARGET_REPAIR_FOLDER_PIN_QUERY.to_owned(),
            unrelated_pin_query: TARGET_REPAIR_UNRELATED_PIN_QUERY.to_owned(),
            action_usage_count: TARGET_REPAIR_ACTION_USAGE,
            folder_usage_count: TARGET_REPAIR_FOLDER_USAGE,
            unrelated_usage_count: TARGET_REPAIR_UNRELATED_USAGE,
        },
    }
}

fn target_repair_action(profile: &Path) -> Action {
    Action {
        label: TARGET_REPAIR_ACTION_LABEL.to_owned(),
        desc: TARGET_REPAIR_ACTION_DESCRIPTION.to_owned(),
        action: target_repair_fixture_path(profile, TARGET_REPAIR_MISSING_ACTION)
            .to_string_lossy()
            .into_owned(),
        args: Some(TARGET_REPAIR_ACTION_ARGS.to_owned()),
    }
}

fn target_repair_unrelated_action(profile: &Path) -> Action {
    Action {
        label: TARGET_REPAIR_UNRELATED_ACTION_LABEL.to_owned(),
        desc: TARGET_REPAIR_UNRELATED_ACTION_DESCRIPTION.to_owned(),
        action: target_repair_fixture_path(profile, TARGET_REPAIR_UNRELATED_PATH)
            .to_string_lossy()
            .into_owned(),
        args: Some("--leave-unrelated".to_owned()),
    }
}

fn target_repair_actions(profile: &Path) -> Vec<Action> {
    vec![
        target_repair_action(profile),
        target_repair_unrelated_action(profile),
    ]
}

/// Build the fixture's persisted paths from normalized components and keep
/// them in the ordinary absolute form shared with the PowerShell verifier.
fn target_repair_fixture_path(profile: &Path, relative: &str) -> PathBuf {
    let mut path = profile.to_path_buf();
    for component in relative.split(['/', '\\']).filter(|part| !part.is_empty()) {
        path.push(component);
    }

    #[cfg(windows)]
    {
        let rendered = path.to_string_lossy();
        if let Some(unc_path) = rendered.strip_prefix(r"\\?\UNC\") {
            return PathBuf::from(format!(r"\\{unc_path}"));
        }
        if let Some(ordinary_path) = rendered.strip_prefix(r"\\?\") {
            return PathBuf::from(ordinary_path);
        }
    }

    path
}

fn write_target_repair_stores(profile: &Path) -> anyhow::Result<()> {
    let missing_parent = target_repair_fixture_path(profile, "target-repair/missing-source");
    fs::create_dir_all(&missing_parent)?;
    let replacements = target_repair_fixture_path(profile, "target-repair/replacements");
    fs::create_dir_all(&replacements)?;
    fs::write(
        target_repair_fixture_path(profile, TARGET_REPAIR_REPLACEMENT_FILE),
        b"Inert selected file for the isolated target repair scenario.\n",
    )?;
    let replacement_folder = target_repair_fixture_path(profile, TARGET_REPAIR_REPLACEMENT_FOLDER);
    fs::create_dir(&replacement_folder)?;
    fs::write(
        replacement_folder.join("fixture.txt"),
        b"Inert folder contents.\n",
    )?;
    write_json(
        &profile.join(FOLDERS_FILE),
        &[FolderEntry {
            label: TARGET_REPAIR_FOLDER_LABEL.to_owned(),
            path: target_repair_fixture_path(profile, TARGET_REPAIR_MISSING_FOLDER)
                .to_string_lossy()
                .into_owned(),
            alias: Some(TARGET_REPAIR_FOLDER_ALIAS.to_owned()),
        }],
    )?;
    let action = target_repair_action(profile);
    let folder_path = target_repair_fixture_path(profile, TARGET_REPAIR_MISSING_FOLDER)
        .to_string_lossy()
        .into_owned();
    let unrelated_path = target_repair_fixture_path(profile, TARGET_REPAIR_UNRELATED_PATH)
        .to_string_lossy()
        .into_owned();
    write_json(
        &profile.join(FAV_FILE),
        &[
            FavEntry {
                label: TARGET_REPAIR_ACTION_FAVORITE_LABEL.to_owned(),
                action: action.action.clone(),
                args: action.args.clone(),
            },
            FavEntry {
                label: TARGET_REPAIR_FOLDER_FAVORITE_LABEL.to_owned(),
                action: folder_path.clone(),
                args: None,
            },
            FavEntry {
                label: TARGET_REPAIR_UNRELATED_FAVORITE_LABEL.to_owned(),
                action: unrelated_path.clone(),
                args: Some("--leave-unrelated".to_owned()),
            },
        ],
    )?;
    write_json(
        &profile.join(BOOKMARKS_FILE),
        &Vec::<multi_launcher::plugins::bookmarks::BookmarkEntry>::new(),
    )?;
    write_json(
        &profile.join(HISTORY_PINS_FILE),
        &[
            target_repair_pin(
                &action.action,
                TARGET_REPAIR_ACTION_FAVORITE_LABEL,
                action.args.clone(),
                TARGET_REPAIR_ACTION_PIN_QUERY,
                31,
            ),
            target_repair_pin(
                &folder_path,
                TARGET_REPAIR_FOLDER_FAVORITE_LABEL,
                None,
                TARGET_REPAIR_FOLDER_PIN_QUERY,
                32,
            ),
            target_repair_pin(
                &unrelated_path,
                TARGET_REPAIR_UNRELATED_FAVORITE_LABEL,
                Some("--leave-unrelated".to_owned()),
                TARGET_REPAIR_UNRELATED_PIN_QUERY,
                33,
            ),
        ],
    )?;
    write_json(
        &profile.join(USAGE_FILE),
        &[
            UsageEntry {
                action: action.action,
                count: TARGET_REPAIR_ACTION_USAGE,
            },
            UsageEntry {
                action: folder_path,
                count: TARGET_REPAIR_FOLDER_USAGE,
            },
            UsageEntry {
                action: unrelated_path,
                count: TARGET_REPAIR_UNRELATED_USAGE,
            },
        ],
    )?;
    Ok(())
}

fn target_repair_pin(
    action_id: &str,
    label: &str,
    args: Option<String>,
    query: &str,
    timestamp: i64,
) -> HistoryPin {
    HistoryPin {
        action_id: action_id.to_owned(),
        label: label.to_owned(),
        desc: "Inert saved Target Repair association.".to_owned(),
        args,
        query: query.to_owned(),
        timestamp,
    }
}

fn verify_target_repair_stores(profile: &Path) -> anyhow::Result<()> {
    let folders =
        match multi_launcher::plugins::folders::load_folders_typed(profile.join(FOLDERS_FILE))? {
            LoadState::Loaded(folders) => folders,
            LoadState::Missing | LoadState::Empty => {
                anyhow::bail!("Target Repair fixture folders.json must be a loaded array")
            }
        };
    let expected_folder = FolderEntry {
        label: TARGET_REPAIR_FOLDER_LABEL.to_owned(),
        path: target_repair_fixture_path(profile, TARGET_REPAIR_MISSING_FOLDER)
            .to_string_lossy()
            .into_owned(),
        alias: Some(TARGET_REPAIR_FOLDER_ALIAS.to_owned()),
    };
    anyhow::ensure!(
        folders == [expected_folder.clone()],
        "Target Repair fixture must contain only its exact saved folder entry"
    );
    anyhow::ensure!(
        !target_repair_fixture_path(profile, TARGET_REPAIR_MISSING_ACTION).exists()
            && !target_repair_fixture_path(profile, TARGET_REPAIR_MISSING_FOLDER).exists(),
        "Target Repair source targets must be absent before the UI scenario"
    );
    anyhow::ensure!(
        target_repair_fixture_path(profile, TARGET_REPAIR_REPLACEMENT_FILE).is_file()
            && target_repair_fixture_path(profile, TARGET_REPAIR_REPLACEMENT_FOLDER).is_dir(),
        "Target Repair replacement choices must exist as inert file and folder fixtures"
    );
    let action_path = target_repair_action(profile).action;
    let folder_path = expected_folder.path.clone();
    let unrelated_path = target_repair_fixture_path(profile, TARGET_REPAIR_UNRELATED_PATH)
        .to_string_lossy()
        .into_owned();
    let favorites = match multi_launcher::plugins::fav::load_favs_typed(profile.join(FAV_FILE))? {
        LoadState::Loaded(entries) => entries,
        LoadState::Missing | LoadState::Empty => {
            anyhow::bail!("Target Repair fixture favorite store must be a loaded array")
        }
    };
    anyhow::ensure!(
        favorites
            == [
                FavEntry {
                    label: TARGET_REPAIR_ACTION_FAVORITE_LABEL.to_owned(),
                    action: action_path.clone(),
                    args: Some(TARGET_REPAIR_ACTION_ARGS.to_owned()),
                },
                FavEntry {
                    label: TARGET_REPAIR_FOLDER_FAVORITE_LABEL.to_owned(),
                    action: folder_path.clone(),
                    args: None,
                },
                FavEntry {
                    label: TARGET_REPAIR_UNRELATED_FAVORITE_LABEL.to_owned(),
                    action: unrelated_path.clone(),
                    args: Some("--leave-unrelated".to_owned()),
                },
            ],
        "Target Repair fixture favorites must bind both exact records and retain one unrelated row"
    );
    let pins = match multi_launcher::history::load_pins_typed(profile.join(HISTORY_PINS_FILE))? {
        LoadState::Loaded(entries) => entries,
        LoadState::Missing | LoadState::Empty => {
            anyhow::bail!("Target Repair fixture pin store must be a loaded array")
        }
    };
    anyhow::ensure!(
        pins == [
            target_repair_pin(
                &action_path,
                TARGET_REPAIR_ACTION_FAVORITE_LABEL,
                Some(TARGET_REPAIR_ACTION_ARGS.to_owned()),
                TARGET_REPAIR_ACTION_PIN_QUERY,
                31,
            ),
            target_repair_pin(
                &folder_path,
                TARGET_REPAIR_FOLDER_FAVORITE_LABEL,
                None,
                TARGET_REPAIR_FOLDER_PIN_QUERY,
                32,
            ),
            target_repair_pin(
                &unrelated_path,
                TARGET_REPAIR_UNRELATED_FAVORITE_LABEL,
                Some("--leave-unrelated".to_owned()),
                TARGET_REPAIR_UNRELATED_PIN_QUERY,
                33,
            ),
        ],
        "Target Repair fixture pins must retain exact query/args associations and one unrelated row"
    );
    let usage = match multi_launcher::usage::load_usage_typed(profile.join(USAGE_FILE))? {
        LoadState::Loaded(entries) => entries,
        LoadState::Missing | LoadState::Empty => {
            anyhow::bail!("Target Repair fixture usage store must be a loaded array")
        }
    };
    anyhow::ensure!(
        usage
            == [
                UsageEntry {
                    action: action_path,
                    count: TARGET_REPAIR_ACTION_USAGE,
                },
                UsageEntry {
                    action: folder_path,
                    count: TARGET_REPAIR_FOLDER_USAGE,
                },
                UsageEntry {
                    action: unrelated_path,
                    count: TARGET_REPAIR_UNRELATED_USAGE,
                },
            ],
        "Target Repair fixture usage must include exact source counts and an unrelated retained count"
    );
    anyhow::ensure!(
        matches!(
            multi_launcher::plugins::bookmarks::load_bookmarks_typed(
                profile.join(BOOKMARKS_FILE)
            )?,
            LoadState::Loaded(entries) if entries.is_empty()
        ),
        "Target Repair fixture bookmark store must remain empty"
    );
    Ok(())
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
        let fixture = fixture_manifest(ids, FixtureScenario::Baseline, None);
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
    fn target_repair_fixture_binds_only_generated_inert_records() {
        let profile = tempfile::tempdir().unwrap();
        let scenario = FixtureScenario::TargetRepair;
        let outcomes = [
            TargetRepairOutcome::FileCancel,
            TargetRepairOutcome::FileConfirm,
            TargetRepairOutcome::FolderConfirm,
        ];
        let outcome = outcomes[0];
        let profile_path = fs::canonicalize(profile.path()).unwrap();
        let actions = target_repair_actions(&profile_path);
        write_json(&profile_path.join("actions.json"), &actions).unwrap();
        write_target_repair_stores(&profile_path).unwrap();

        let manifest = fixture_manifest(note_ids(), scenario, Some(outcome));
        assert_eq!(manifest.scenario, "target-repair");
        assert!(manifest.experimental_jump_mode_enabled);
        assert_eq!(manifest.enabled_plugins, ["folders", "missing", "notes"]);
        assert_eq!(manifest.action_count, 2);
        assert!(manifest.safety.custom_actions_present);
        assert_eq!(
            manifest.target_repair,
            Some(target_repair_manifest(outcome))
        );
        assert_eq!(actions[0].label, TARGET_REPAIR_ACTION_LABEL);
        assert_eq!(actions[0].desc, TARGET_REPAIR_ACTION_DESCRIPTION);
        assert_eq!(actions[0].args.as_deref(), Some(TARGET_REPAIR_ACTION_ARGS));
        assert!(Path::new(&actions[0].action).is_absolute());
        assert_eq!(actions[1], target_repair_unrelated_action(&profile_path));
        let fixture_ids = outcomes.map(|outcome| {
            let manifest = fixture_manifest(note_ids(), scenario, Some(outcome));
            assert_eq!(
                manifest.target_repair.as_ref().map(|repair| repair.outcome),
                Some(outcome)
            );
            manifest.fixture_id
        });
        assert_eq!(fixture_ids.len(), 3);
        assert_ne!(fixture_ids[0], fixture_ids[1]);
        assert_ne!(fixture_ids[0], fixture_ids[2]);
        assert_ne!(fixture_ids[1], fixture_ids[2]);
        #[cfg(windows)]
        let normalized_folder =
            target_repair_fixture_path(&profile_path, TARGET_REPAIR_MISSING_FOLDER);
        #[cfg(windows)]
        let normalized_unrelated =
            target_repair_fixture_path(&profile_path, TARGET_REPAIR_UNRELATED_PATH);
        #[cfg(windows)]
        for path in [
            actions[0].action.as_str(),
            normalized_folder.to_str().unwrap(),
            normalized_unrelated.to_str().unwrap(),
        ] {
            assert!(!path.contains('/'));
            assert!(!path.starts_with(r"\\?\"));
        }
        verify_target_repair_stores(&profile_path).unwrap();
    }

    #[test]
    fn query_probe_fixture_explicitly_enables_the_experimental_gate() {
        let fixture = fixture_manifest(note_ids(), FixtureScenario::QueryProbe, None);
        assert_eq!(fixture.scenario, "query-probe");
        assert_eq!(fixture.fixture_id, "jump-mode-m05-query-probe-v1");
        assert!(fixture.experimental_jump_mode_enabled);
        assert!(fixture.safety.experimental_jump_mode_enabled);
    }

    #[test]
    fn note_replace_fixture_variants_share_exact_sources_and_only_change_jump_gate() {
        let enabled = FixtureScenario::NoteReplaceEnabled;
        let disabled = FixtureScenario::NoteReplaceDisabled;
        let enabled_manifest = fixture_manifest(note_ids(), enabled, None);
        let disabled_manifest = fixture_manifest(note_ids(), disabled, None);
        assert_eq!(enabled.label(), "note-replace-enabled");
        assert_eq!(disabled.label(), "note-replace-disabled");
        assert!(enabled.jump_mode_enabled());
        assert!(!disabled.jump_mode_enabled());
        assert_eq!(enabled_manifest.enabled_plugins, ["notes"]);
        assert_eq!(disabled_manifest.enabled_plugins, ["notes"]);
        assert_eq!(enabled_manifest.note_ids, disabled_manifest.note_ids);
        assert_eq!(enabled_manifest.action_count, 0);
        assert_eq!(disabled_manifest.action_count, 0);
        assert_eq!(
            fixture_note_text(enabled, 1, "jump-qa-note-001", "Jump QA Replace Note"),
            "# Jump QA Replace Note\r\n\r\nTag: #jump_qa_bucket_01\r\n\r\n[shown Ω](<literal Ω $1\\path>)\r\n[shown Ω](<literal Ω $1\\path>)\r\nkeep this line\r\n"
        );
        assert_eq!(
            fixture_note_text(enabled, 2, "jump-qa-note-002", "Jump QA Note 002"),
            fixture_note_text(disabled, 2, "jump-qa-note-002", "Jump QA Note 002")
        );
    }

    #[test]
    fn phase1_fixtures_use_safe_dense_fill_query_slots_and_distinct_note_slugs() {
        let enabled = fixture_manifest(note_ids(), FixtureScenario::Phase1Enabled, None);
        let disabled = fixture_manifest(note_ids(), FixtureScenario::Phase1Disabled, None);
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
    fn all_scenario_profiles_seed_isolated_inert_stores_before_app_construction() {
        for surface in [
            None,
            Some(Phase2Surface::Settings),
            Some(Phase2Surface::Notes),
            Some(Phase2Surface::Todo),
            Some(Phase2Surface::Timer),
            Some(Phase2Surface::Paste),
            Some(Phase2Surface::MkMacro),
            Some(Phase2Surface::FileSearch),
        ] {
            let profile = tempfile::tempdir().unwrap();
            write_phase2_stores(profile.path(), surface).unwrap();
            verify_phase2_stores(profile.path(), surface).unwrap();
        }
    }

    #[test]
    fn fixture_and_safety_manifests_reject_unknown_fields() {
        let mut fixture = serde_json::to_value(fixture_manifest(
            note_ids(),
            FixtureScenario::Phase2(Phase2Surface::Paste),
            None,
        ))
        .unwrap();
        fixture["unexpected"] = json!(true);
        assert!(serde_json::from_value::<FixtureManifest>(fixture).is_err());

        let mut safety = serde_json::to_value(
            fixture_manifest(
                note_ids(),
                FixtureScenario::Phase2(Phase2Surface::Paste),
                None,
            )
            .safety,
        )
        .unwrap();
        safety["unexpected"] = json!(true);
        assert!(serde_json::from_value::<SafetyManifest>(safety).is_err());
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
        let mut settings = Settings {
            hotkey: Some(LAUNCHER_HOTKEY.to_owned()),
            quit_hotkey: None,
            help_hotkey: None,
            ..Default::default()
        };
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
