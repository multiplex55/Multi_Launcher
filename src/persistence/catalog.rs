use crate::actions::Action;
use crate::common::persistence::{LoadState, PersistenceError, read_bytes};
use crate::dashboard::config::DashboardConfig;
use crate::history::{HistoryEntry, HistoryPin};
use crate::multi_manager::model::MmWorkspace;
use crate::note_ui_state::NoteUiState;
use crate::platform::app_data::AppDataRoot;
use crate::plugins::bookmarks::BookmarkEntry;
use crate::plugins::calendar::CalendarEvent;
use crate::plugins::fav::FavEntry;
use crate::plugins::folders::FolderEntry;
use crate::plugins::layouts_storage::LayoutStore;
use crate::plugins::macros::MacroEntry;
use crate::plugins::shell::ShellCmdEntry;
use crate::plugins::snippets::SnippetEntry;
use crate::plugins::todo::TodoEntry;
use crate::settings::Settings;
use serde::de::DeserializeOwned;
use std::path::{Component, Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum PersistentStoreId {
    Settings,
    Actions,
    Bookmarks,
    Folders,
    Snippets,
    Favorites,
    Todos,
    ShellCommands,
    LegacyMacros,
    HistoryPins,
    CalendarEvents,
    Layouts,
    DashboardConfig,
    MouseGestureDefinitions,
    MkMacroDocument,
    MkMacroAssets,
    ClipboardModifiers,
    MultiManagerWorkspaces,
    Notes,
    NotesAssets,
    Scratchpad,
    QueryHistory,
    ClipboardHistory,
    CalculatorHistory,
    Usage,
    CalendarState,
    MouseGestureUsage,
    MouseGestureState,
    NoteUiState,
    MultiManagerBindings,
    Alarms,
    LauncherLog,
    ToastLog,
}

impl PersistentStoreId {
    pub const ALL: [Self; 33] = [
        Self::Settings,
        Self::Actions,
        Self::Bookmarks,
        Self::Folders,
        Self::Snippets,
        Self::Favorites,
        Self::Todos,
        Self::ShellCommands,
        Self::LegacyMacros,
        Self::HistoryPins,
        Self::CalendarEvents,
        Self::Layouts,
        Self::DashboardConfig,
        Self::MouseGestureDefinitions,
        Self::MkMacroDocument,
        Self::MkMacroAssets,
        Self::ClipboardModifiers,
        Self::MultiManagerWorkspaces,
        Self::Notes,
        Self::NotesAssets,
        Self::Scratchpad,
        Self::QueryHistory,
        Self::ClipboardHistory,
        Self::CalculatorHistory,
        Self::Usage,
        Self::CalendarState,
        Self::MouseGestureUsage,
        Self::MouseGestureState,
        Self::NoteUiState,
        Self::MultiManagerBindings,
        Self::Alarms,
        Self::LauncherLog,
        Self::ToastLog,
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreKind {
    File,
    Directory,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreCriticality {
    Critical,
    Replaceable,
    Runtime,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreOwnership {
    ApplicationOwned,
    External,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackupPolicy {
    Include,
    ExcludeExternal,
    ExcludeReplaceable,
    ExcludeRuntime,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StorePrivacy {
    Ordinary,
    UserContent,
    Sensitive,
    SystemMetadata,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteFrequency {
    Low,
    Moderate,
    High,
    Session,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StoreHealth {
    Healthy,
    Missing,
    Empty,
    Malformed { message: String },
    Unreadable { message: String },
    UnsupportedSchema { version: String },
}

#[derive(Clone, Copy)]
enum ProbeKind {
    Json(fn(&Path, &[u8]) -> ProbeResult),
    OpaqueFile,
    NotesDirectory,
    AssetsDirectory,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ProbeResult {
    Healthy,
    Malformed,
    UnsupportedSchema(String),
}

#[derive(Clone)]
pub struct StoreDescriptor {
    pub id: PersistentStoreId,
    pub label: &'static str,
    pub path: PathBuf,
    pub kind: StoreKind,
    pub criticality: StoreCriticality,
    pub ownership: StoreOwnership,
    pub backup_policy: BackupPolicy,
    pub restore_eligible: bool,
    pub reset_eligible: bool,
    pub privacy: StorePrivacy,
    pub frequency: WriteFrequency,
    pub externally_configured: bool,
    probe: ProbeKind,
}

impl std::fmt::Debug for StoreDescriptor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoreDescriptor")
            .field("id", &self.id)
            .field("path", &self.path)
            .field("ownership", &self.ownership)
            .finish_non_exhaustive()
    }
}

impl StoreDescriptor {
    /// Inspect this store on demand without invoking migrations or changing bytes.
    pub fn probe(&self) -> StoreHealth {
        match self.probe {
            ProbeKind::Json(validate) => probe_file(&self.path, Some(validate)),
            ProbeKind::OpaqueFile => probe_file(&self.path, None),
            ProbeKind::NotesDirectory => probe_directory(&self.path, Some("md")),
            ProbeKind::AssetsDirectory => probe_directory(&self.path, None),
        }
    }
}

#[derive(Clone, Debug)]
pub struct PersistenceCatalog {
    stores: Vec<StoreDescriptor>,
}

impl PersistenceCatalog {
    /// Build path metadata only. Health inspection remains explicitly on demand.
    pub fn new(root: &AppDataRoot, settings: &Settings) -> Self {
        let current_dir = std::env::current_dir().unwrap_or_else(|_| root.path().to_path_buf());
        let dashboard_setting = settings
            .dashboard
            .config_path
            .as_deref()
            .filter(|path| !path.trim().is_empty());
        let dashboard_path = absolute_from(
            &current_dir,
            &DashboardConfig::path_for(dashboard_setting.unwrap_or("dashboard.json")),
        );
        let scratchpad_setting = discover_scratchpad_path(&dashboard_path);
        let notes_setting = std::env::var_os("ML_NOTES_DIR").map(PathBuf::from);
        let notes_source = notes_setting
            .clone()
            .unwrap_or_else(crate::plugins::note::notes_dir);
        let notes_path = absolute_from(&current_dir, &notes_source);

        let stores = PersistentStoreId::ALL
            .into_iter()
            .map(|id| {
                descriptor_for(
                    id,
                    root,
                    settings,
                    &current_dir,
                    &dashboard_path,
                    dashboard_setting.is_some(),
                    &notes_path,
                    notes_setting.is_some(),
                    scratchpad_setting.as_deref(),
                )
            })
            .collect();
        Self { stores }
    }

    pub fn stores(&self) -> &[StoreDescriptor] {
        &self.stores
    }

    pub fn get(&self, id: PersistentStoreId) -> &StoreDescriptor {
        self.stores
            .iter()
            .find(|store| store.id == id)
            .expect("every persistent store ID has one canonical descriptor")
    }
}

#[derive(Clone, Copy)]
struct StoreSpec {
    label: &'static str,
    kind: StoreKind,
    criticality: StoreCriticality,
    privacy: StorePrivacy,
    frequency: WriteFrequency,
    reset: bool,
    probe: ProbeKind,
}

fn spec(id: PersistentStoreId) -> StoreSpec {
    use PersistentStoreId as Id;
    use StoreCriticality::{Critical, Replaceable, Runtime};
    use StoreKind::{Directory, File};
    use StorePrivacy::{Ordinary, Sensitive, SystemMetadata, UserContent};
    use WriteFrequency::{High, Low, Moderate, Session};
    let json_value = ProbeKind::Json(probe_json::<serde_json::Value>);
    match id {
        Id::Settings => s(
            "Settings",
            File,
            Critical,
            Ordinary,
            Low,
            true,
            ProbeKind::Json(probe_json::<Settings>),
        ),
        Id::Actions => s(
            "Actions",
            File,
            Critical,
            Ordinary,
            Low,
            true,
            ProbeKind::Json(probe_json::<Vec<Action>>),
        ),
        Id::Bookmarks => s(
            "Bookmarks",
            File,
            Critical,
            Ordinary,
            Low,
            true,
            ProbeKind::Json(probe_bookmarks),
        ),
        Id::Folders => s(
            "Folders",
            File,
            Critical,
            Ordinary,
            Low,
            true,
            ProbeKind::Json(probe_json::<Vec<FolderEntry>>),
        ),
        Id::Snippets => s(
            "Snippets",
            File,
            Critical,
            Sensitive,
            Low,
            true,
            ProbeKind::Json(probe_json::<Vec<SnippetEntry>>),
        ),
        Id::Favorites => s(
            "Favorites",
            File,
            Critical,
            Ordinary,
            Low,
            true,
            ProbeKind::Json(probe_json::<Vec<FavEntry>>),
        ),
        Id::Todos => s(
            "Todos",
            File,
            Critical,
            Sensitive,
            Moderate,
            true,
            ProbeKind::Json(probe_json::<Vec<TodoEntry>>),
        ),
        Id::ShellCommands => s(
            "Shell commands",
            File,
            Critical,
            Ordinary,
            Low,
            true,
            ProbeKind::Json(probe_json::<Vec<ShellCmdEntry>>),
        ),
        Id::LegacyMacros => s(
            "Legacy macros",
            File,
            Critical,
            Ordinary,
            Low,
            true,
            ProbeKind::Json(probe_json::<Vec<MacroEntry>>),
        ),
        Id::HistoryPins => s(
            "History pins",
            File,
            Critical,
            UserContent,
            Low,
            true,
            ProbeKind::Json(probe_json::<Vec<HistoryPin>>),
        ),
        Id::CalendarEvents => s(
            "Calendar events",
            File,
            Critical,
            Sensitive,
            Moderate,
            true,
            ProbeKind::Json(probe_json::<Vec<CalendarEvent>>),
        ),
        Id::Layouts => s(
            "Layouts",
            File,
            Critical,
            Ordinary,
            Low,
            true,
            ProbeKind::Json(probe_json::<LayoutStore>),
        ),
        Id::DashboardConfig => s(
            "Dashboard configuration",
            File,
            Critical,
            Ordinary,
            Low,
            true,
            ProbeKind::Json(probe_json::<DashboardConfig>),
        ),
        Id::MouseGestureDefinitions => s(
            "Mouse gesture definitions",
            File,
            Critical,
            Ordinary,
            Low,
            true,
            ProbeKind::Json(probe_gestures),
        ),
        Id::MkMacroDocument => s(
            "MkMacro document",
            File,
            Critical,
            Sensitive,
            Moderate,
            true,
            ProbeKind::Json(probe_mkmacro),
        ),
        Id::MkMacroAssets => s(
            "MkMacro assets",
            Directory,
            Critical,
            Sensitive,
            Moderate,
            false,
            ProbeKind::AssetsDirectory,
        ),
        Id::ClipboardModifiers => s(
            "Clipboard Modify configuration",
            File,
            Critical,
            Sensitive,
            Low,
            true,
            ProbeKind::Json(probe_clipboard_modifiers),
        ),
        Id::MultiManagerWorkspaces => s(
            "MultiManager workspaces",
            File,
            Critical,
            Sensitive,
            Moderate,
            true,
            ProbeKind::Json(probe_json::<Vec<MmWorkspace>>),
        ),
        Id::Notes => s(
            "Notes",
            Directory,
            Critical,
            Sensitive,
            Moderate,
            true,
            ProbeKind::NotesDirectory,
        ),
        Id::NotesAssets => s(
            "Note assets",
            Directory,
            Critical,
            Sensitive,
            Moderate,
            false,
            ProbeKind::AssetsDirectory,
        ),
        Id::Scratchpad => s(
            "Scratchpad",
            File,
            Critical,
            Sensitive,
            Moderate,
            true,
            ProbeKind::Json(probe_scratchpad),
        ),
        Id::QueryHistory => s(
            "Query history",
            File,
            Replaceable,
            Sensitive,
            High,
            true,
            ProbeKind::Json(probe_json::<Vec<HistoryEntry>>),
        ),
        Id::ClipboardHistory => s(
            "Clipboard history",
            File,
            Replaceable,
            Sensitive,
            High,
            true,
            ProbeKind::Json(probe_json::<Vec<String>>),
        ),
        Id::CalculatorHistory => s(
            "Calculator history",
            File,
            Replaceable,
            UserContent,
            Moderate,
            true,
            json_value,
        ),
        Id::Usage => s(
            "Usage counters",
            File,
            Replaceable,
            SystemMetadata,
            High,
            true,
            json_value,
        ),
        Id::CalendarState => s(
            "Calendar view state",
            File,
            Replaceable,
            UserContent,
            Session,
            true,
            json_value,
        ),
        Id::MouseGestureUsage => s(
            "Mouse gesture usage",
            File,
            Replaceable,
            SystemMetadata,
            High,
            true,
            json_value,
        ),
        Id::MouseGestureState => s(
            "Mouse gesture runtime state",
            File,
            Replaceable,
            SystemMetadata,
            Session,
            true,
            json_value,
        ),
        Id::NoteUiState => s(
            "Note UI state",
            File,
            Replaceable,
            UserContent,
            Session,
            true,
            ProbeKind::Json(probe_json::<NoteUiState>),
        ),
        Id::MultiManagerBindings => s(
            "MultiManager bindings",
            File,
            Replaceable,
            Sensitive,
            Moderate,
            true,
            json_value,
        ),
        Id::Alarms => s(
            "Timer alarms",
            File,
            Runtime,
            UserContent,
            Session,
            true,
            json_value,
        ),
        Id::LauncherLog => s(
            "Launcher log",
            File,
            Runtime,
            Sensitive,
            High,
            false,
            ProbeKind::OpaqueFile,
        ),
        Id::ToastLog => s(
            "Toast log",
            File,
            Runtime,
            Sensitive,
            High,
            false,
            ProbeKind::OpaqueFile,
        ),
    }
}

const fn s(
    label: &'static str,
    kind: StoreKind,
    criticality: StoreCriticality,
    privacy: StorePrivacy,
    frequency: WriteFrequency,
    reset: bool,
    probe: ProbeKind,
) -> StoreSpec {
    StoreSpec {
        label,
        kind,
        criticality,
        privacy,
        frequency,
        reset,
        probe,
    }
}

#[allow(clippy::too_many_arguments)]
fn descriptor_for(
    id: PersistentStoreId,
    root: &AppDataRoot,
    settings: &Settings,
    current_dir: &Path,
    dashboard_path: &Path,
    dashboard_configured: bool,
    notes_path: &Path,
    notes_configured: bool,
    scratchpad_setting: Option<&Path>,
) -> StoreDescriptor {
    use PersistentStoreId as Id;
    let (path, externally_configured) = match id {
        Id::Settings => (root.path().join("settings.json"), false),
        Id::Layouts => (
            root.path()
                .join(crate::plugins::layouts_storage::LAYOUTS_FILE),
            false,
        ),
        Id::ClipboardModifiers => (
            root.path()
                .join(crate::clipboard_modify::config::DEFAULT_RELATIVE_PATH),
            false,
        ),
        Id::NoteUiState => (root.path().join("note_ui_state.json"), false),
        Id::DashboardConfig => (dashboard_path.to_path_buf(), dashboard_configured),
        Id::MultiManagerWorkspaces => (
            absolute_from(
                root.path(),
                Path::new(&settings.multi_manager.workspaces_path),
            ),
            settings.multi_manager.workspaces_path
                != crate::settings::default_multi_manager_workspaces_path(),
        ),
        Id::MultiManagerBindings => (
            absolute_from(
                root.path(),
                Path::new(&settings.multi_manager.bindings_path),
            ),
            settings.multi_manager.bindings_path
                != crate::settings::default_multi_manager_bindings_path(),
        ),
        Id::Notes => (notes_path.to_path_buf(), notes_configured),
        Id::NotesAssets => (notes_path.join("assets"), notes_configured),
        Id::Scratchpad => (
            absolute_from(
                current_dir,
                scratchpad_setting.unwrap_or_else(|| Path::new("scratchpad.json")),
            ),
            scratchpad_setting.is_some(),
        ),
        Id::Actions => cwd(current_dir, "actions.json"),
        Id::Bookmarks => cwd(current_dir, crate::plugins::bookmarks::BOOKMARKS_FILE),
        Id::Folders => cwd(current_dir, crate::plugins::folders::FOLDERS_FILE),
        Id::Snippets => cwd(current_dir, crate::plugins::snippets::SNIPPETS_FILE),
        Id::Favorites => cwd(current_dir, crate::plugins::fav::FAV_FILE),
        Id::Todos => cwd(current_dir, crate::plugins::todo::TODO_FILE),
        Id::ShellCommands => cwd(current_dir, crate::plugins::shell::SHELL_CMDS_FILE),
        Id::LegacyMacros => cwd(current_dir, crate::plugins::macros::MACROS_FILE),
        Id::HistoryPins => cwd(current_dir, crate::history::HISTORY_PINS_FILE),
        Id::CalendarEvents => cwd(current_dir, crate::plugins::calendar::CALENDAR_EVENTS_FILE),
        Id::MouseGestureDefinitions => cwd(current_dir, crate::mouse_gestures::db::GESTURES_FILE),
        Id::MkMacroDocument => cwd(current_dir, crate::mkmacro::store::MKMACROS_FILE),
        Id::MkMacroAssets => cwd(current_dir, crate::mkmacro::store::ASSET_DIRECTORY),
        Id::QueryHistory => cwd(current_dir, "history.json"),
        Id::ClipboardHistory => cwd(current_dir, crate::plugins::clipboard::CLIPBOARD_FILE),
        Id::CalculatorHistory => cwd(current_dir, crate::plugins::calc_history::CALC_HISTORY_FILE),
        Id::Usage => cwd(current_dir, crate::usage::USAGE_FILE),
        Id::CalendarState => cwd(current_dir, crate::plugins::calendar::CALENDAR_STATE_FILE),
        Id::MouseGestureUsage => cwd(
            current_dir,
            crate::mouse_gestures::usage::GESTURES_USAGE_FILE,
        ),
        Id::MouseGestureState => cwd(current_dir, "mouse_gestures_state.json"),
        Id::Alarms => cwd(current_dir, crate::plugins::timer::ALARMS_FILE),
        Id::LauncherLog => {
            let configured = matches!(
                settings.log_file.as_ref(),
                Some(crate::settings::LogFile::Path(_))
            );
            let path = settings
                .log_file_path()
                .unwrap_or_else(crate::settings::default_log_path);
            (absolute_from(current_dir, &path), configured)
        }
        Id::ToastLog => cwd(current_dir, crate::toast_log::TOAST_LOG_FILE),
    };
    let path = lexical_absolute(path, current_dir);
    let ownership = if path_is_within(root.path(), &path) {
        StoreOwnership::ApplicationOwned
    } else {
        StoreOwnership::External
    };
    let spec = spec(id);
    let backup_policy = match (spec.criticality, ownership) {
        (StoreCriticality::Critical, StoreOwnership::ApplicationOwned) => BackupPolicy::Include,
        (StoreCriticality::Critical, StoreOwnership::External) => BackupPolicy::ExcludeExternal,
        (StoreCriticality::Replaceable, _) => BackupPolicy::ExcludeReplaceable,
        (StoreCriticality::Runtime, _) => BackupPolicy::ExcludeRuntime,
    };
    let restore_eligible = backup_policy == BackupPolicy::Include;
    StoreDescriptor {
        id,
        label: spec.label,
        path,
        kind: spec.kind,
        criticality: spec.criticality,
        ownership,
        backup_policy,
        restore_eligible,
        reset_eligible: spec.reset && ownership == StoreOwnership::ApplicationOwned,
        privacy: spec.privacy,
        frequency: spec.frequency,
        externally_configured,
        probe: spec.probe,
    }
}

fn cwd(base: &Path, relative: &str) -> (PathBuf, bool) {
    (base.join(relative), false)
}

fn absolute_from(base: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    }
}

fn lexical_absolute(path: PathBuf, current_dir: &Path) -> PathBuf {
    let absolute = absolute_from(current_dir, &path);
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if normalized.file_name().is_some() {
                    normalized.pop();
                }
            }
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                normalized.push(component.as_os_str())
            }
        }
    }
    normalized
}

fn path_is_within(root: &Path, candidate: &Path) -> bool {
    let root = lexical_absolute(root.to_path_buf(), root);
    let candidate = lexical_absolute(candidate.to_path_buf(), root.as_path());
    #[cfg(windows)]
    {
        let root = root.to_string_lossy().replace('/', "\\").to_lowercase();
        let candidate = candidate
            .to_string_lossy()
            .replace('/', "\\")
            .to_lowercase();
        candidate == root
            || candidate
                .strip_prefix(&root)
                .is_some_and(|suffix| suffix.starts_with('\\'))
    }
    #[cfg(not(windows))]
    candidate.starts_with(root)
}

fn probe_file(path: &Path, validate: Option<fn(&Path, &[u8]) -> ProbeResult>) -> StoreHealth {
    match read_bytes(path) {
        Ok(LoadState::Missing) => StoreHealth::Missing,
        Ok(LoadState::Empty) => StoreHealth::Empty,
        Ok(LoadState::Loaded(bytes)) => match validate
            .map(|f| f(path, &bytes))
            .unwrap_or(ProbeResult::Healthy)
        {
            ProbeResult::Healthy => StoreHealth::Healthy,
            ProbeResult::Malformed => StoreHealth::Malformed {
                message: "stored data does not match the expected format".into(),
            },
            ProbeResult::UnsupportedSchema(version) => StoreHealth::UnsupportedSchema { version },
        },
        Err(PersistenceError::Read { source, .. }) => StoreHealth::Unreadable {
            message: source.to_string(),
        },
        Err(_) => StoreHealth::Unreadable {
            message: "unable to inspect stored data".into(),
        },
    }
}

fn probe_directory(path: &Path, extension: Option<&str>) -> StoreHealth {
    let entries = match std::fs::read_dir(path) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return StoreHealth::Missing,
        Err(error) => {
            return StoreHealth::Unreadable {
                message: error.to_string(),
            };
        }
    };
    let mut found = false;
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                return StoreHealth::Unreadable {
                    message: error.to_string(),
                };
            }
        };
        if extension
            .is_some_and(|ext| entry.path().extension().and_then(|x| x.to_str()) != Some(ext))
        {
            continue;
        }
        found = true;
        if extension.is_some()
            && let Err(error) = std::fs::read(entry.path())
        {
            return StoreHealth::Unreadable {
                message: error.to_string(),
            };
        }
    }
    if found {
        StoreHealth::Healthy
    } else {
        StoreHealth::Empty
    }
}

fn probe_json<T: DeserializeOwned>(_: &Path, bytes: &[u8]) -> ProbeResult {
    serde_json::from_slice::<T>(bytes)
        .map(|_| ProbeResult::Healthy)
        .unwrap_or(ProbeResult::Malformed)
}

fn probe_bookmarks(_: &Path, bytes: &[u8]) -> ProbeResult {
    if serde_json::from_slice::<Vec<BookmarkEntry>>(bytes).is_ok()
        || serde_json::from_slice::<Vec<String>>(bytes).is_ok()
    {
        ProbeResult::Healthy
    } else {
        ProbeResult::Malformed
    }
}

fn probe_gestures(path: &Path, bytes: &[u8]) -> ProbeResult {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(bytes) else {
        return ProbeResult::Malformed;
    };
    let version = value
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(1);
    if version > crate::mouse_gestures::db::SCHEMA_VERSION as u64 {
        return ProbeResult::UnsupportedSchema(version.to_string());
    }
    crate::mouse_gestures::db::decode_gestures(path, bytes)
        .map(|_| ProbeResult::Healthy)
        .unwrap_or(ProbeResult::Malformed)
}

fn probe_mkmacro(_: &Path, bytes: &[u8]) -> ProbeResult {
    match crate::mkmacro::store::probe_document(bytes) {
        Ok(crate::mkmacro::store::DocumentProbe::Supported) => ProbeResult::Healthy,
        Ok(crate::mkmacro::store::DocumentProbe::Unsupported(version)) => {
            ProbeResult::UnsupportedSchema(version.to_string())
        }
        Err(_) => ProbeResult::Malformed,
    }
}

fn probe_clipboard_modifiers(_: &Path, bytes: &[u8]) -> ProbeResult {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(bytes) else {
        return ProbeResult::Malformed;
    };
    let Some(version) = value
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
    else {
        return ProbeResult::Malformed;
    };
    if version > crate::clipboard_modify::config::CURRENT_SCHEMA_VERSION as u64 {
        return ProbeResult::UnsupportedSchema(version.to_string());
    }
    if version < crate::clipboard_modify::config::CURRENT_SCHEMA_VERSION as u64 {
        return crate::clipboard_modify::migrate::decode_migration(version as u32, bytes)
            .map(|_| ProbeResult::Healthy)
            .unwrap_or(ProbeResult::Malformed);
    }
    let Ok(model) = serde_json::from_value::<
        crate::clipboard_modify::config::VersionedClipboardModifiersFile,
    >(value) else {
        return ProbeResult::Malformed;
    };
    crate::clipboard_modify::config::validate_model(&model)
        .map(|_| ProbeResult::Healthy)
        .unwrap_or(ProbeResult::Malformed)
}

fn probe_scratchpad(_: &Path, bytes: &[u8]) -> ProbeResult {
    #[derive(serde::Deserialize)]
    struct Scratchpad {
        content: String,
    }
    serde_json::from_slice::<Scratchpad>(bytes)
        .map(|value| {
            let _ = value.content;
            ProbeResult::Healthy
        })
        .unwrap_or(ProbeResult::Malformed)
}

fn discover_scratchpad_path(dashboard_path: &Path) -> Option<PathBuf> {
    let bytes = match read_bytes(dashboard_path).ok()? {
        LoadState::Loaded(bytes) => bytes,
        _ => return None,
    };
    let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    value.get("slots")?.as_array()?.iter().find_map(|slot| {
        (slot.get("widget")?.as_str()? == "scratchpad")
            .then(|| slot.get("settings")?.get("storage_path")?.as_str())
            .flatten()
            .filter(|path| !path.trim().is_empty())
            .map(PathBuf::from)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn root(path: &Path) -> AppDataRoot {
        AppDataRoot::from_settings_path(path.join("settings.json")).unwrap()
    }

    fn test_catalog(root_path: &Path, settings: &Settings) -> PersistenceCatalog {
        let root = root(root_path);
        let current = root_path;
        let dashboard = root_path.join("dashboard.json");
        let notes = root_path.join("notes");
        PersistenceCatalog {
            stores: PersistentStoreId::ALL
                .into_iter()
                .map(|id| {
                    descriptor_for(
                        id, &root, settings, current, &dashboard, false, &notes, false, None,
                    )
                })
                .collect(),
        }
    }

    fn json_descriptor(path: &Path, validate: fn(&Path, &[u8]) -> ProbeResult) -> StoreDescriptor {
        StoreDescriptor {
            id: PersistentStoreId::Settings,
            label: "test",
            path: path.to_path_buf(),
            kind: StoreKind::File,
            criticality: StoreCriticality::Critical,
            ownership: StoreOwnership::ApplicationOwned,
            backup_policy: BackupPolicy::Include,
            restore_eligible: true,
            reset_eligible: true,
            privacy: StorePrivacy::Ordinary,
            frequency: WriteFrequency::Low,
            externally_configured: false,
            probe: ProbeKind::Json(validate),
        }
    }

    #[test]
    fn catalog_has_exactly_one_descriptor_for_every_typed_id() {
        let directory = tempfile::tempdir().unwrap();
        let catalog = test_catalog(directory.path(), &Settings::default());
        assert_eq!(catalog.stores().len(), PersistentStoreId::ALL.len());
        let ids = catalog
            .stores()
            .iter()
            .map(|store| store.id)
            .collect::<BTreeSet<_>>();
        assert_eq!(ids.len(), PersistentStoreId::ALL.len());
        assert!(
            PersistentStoreId::ALL
                .into_iter()
                .all(|id| ids.contains(&id))
        );
        let paths = catalog
            .stores()
            .iter()
            .map(|store| store.path.clone())
            .collect::<BTreeSet<_>>();
        assert_eq!(paths.len(), catalog.stores().len());
    }

    #[test]
    fn configured_paths_preserve_internal_external_and_escaping_semantics() {
        let directory = tempfile::tempdir().unwrap();
        let root = root(directory.path());
        let current = directory.path();
        let mut settings = Settings::default();
        settings.multi_manager.workspaces_path = "nested/workspaces.json".into();
        let internal = descriptor_for(
            PersistentStoreId::MultiManagerWorkspaces,
            &root,
            &settings,
            current,
            &directory.path().join("dashboard.json"),
            false,
            &directory.path().join("notes"),
            false,
            None,
        );
        assert_eq!(
            internal.path,
            directory.path().join("nested/workspaces.json")
        );
        assert_eq!(internal.ownership, StoreOwnership::ApplicationOwned);
        assert!(internal.externally_configured);

        settings.multi_manager.workspaces_path = "../outside/workspaces.json".into();
        let escaped = descriptor_for(
            PersistentStoreId::MultiManagerWorkspaces,
            &root,
            &settings,
            current,
            &directory.path().join("dashboard.json"),
            false,
            &directory.path().join("notes"),
            false,
            None,
        );
        assert_eq!(escaped.ownership, StoreOwnership::External);
        assert_eq!(escaped.backup_policy, BackupPolicy::ExcludeExternal);
        assert!(!escaped.restore_eligible);
        assert!(!escaped.reset_eligible);

        let external_root = tempfile::tempdir().unwrap();
        settings.multi_manager.workspaces_path = external_root
            .path()
            .join("absolute.json")
            .to_string_lossy()
            .into_owned();
        let absolute = descriptor_for(
            PersistentStoreId::MultiManagerWorkspaces,
            &root,
            &settings,
            current,
            &directory.path().join("dashboard.json"),
            false,
            &directory.path().join("notes"),
            false,
            None,
        );
        assert_eq!(absolute.ownership, StoreOwnership::External);

        let dashboard = descriptor_for(
            PersistentStoreId::DashboardConfig,
            &root,
            &settings,
            current,
            &directory.path().join("custom/dashboard.json"),
            true,
            &directory.path().join("notes"),
            false,
            None,
        );
        assert_eq!(dashboard.ownership, StoreOwnership::ApplicationOwned);
        assert!(dashboard.externally_configured);

        let notes = descriptor_for(
            PersistentStoreId::Notes,
            &root,
            &settings,
            current,
            &directory.path().join("dashboard.json"),
            false,
            &external_root.path().join("notes"),
            true,
            Some(&external_root.path().join("scratchpad.json")),
        );
        assert_eq!(notes.ownership, StoreOwnership::External);
        assert!(notes.externally_configured);
    }

    #[test]
    fn dashboard_scratchpad_path_discovery_is_read_only_and_classified() {
        let directory = tempfile::tempdir().unwrap();
        let dashboard = directory.path().join("dashboard.json");
        let bytes = br#"{"slots":[{"widget":"scratchpad","settings":{"storage_path":"../private/scratch.json"}}]}"#;
        std::fs::write(&dashboard, bytes).unwrap();
        assert_eq!(
            discover_scratchpad_path(&dashboard),
            Some(PathBuf::from("../private/scratch.json"))
        );
        assert_eq!(std::fs::read(&dashboard).unwrap(), bytes);
    }

    #[test]
    fn public_catalog_resolves_configured_dashboard_and_scratchpad_paths() {
        let root_directory = tempfile::tempdir().unwrap();
        let external_directory = tempfile::tempdir().unwrap();
        let dashboard_path = external_directory.path().join("dashboard.json");
        let scratchpad_path = external_directory.path().join("scratchpad.json");
        let bytes = format!(
            r#"{{"slots":[{{"widget":"scratchpad","settings":{{"storage_path":{}}}}}]}}"#,
            serde_json::to_string(&scratchpad_path.to_string_lossy()).unwrap()
        );
        std::fs::write(&dashboard_path, &bytes).unwrap();

        let mut settings = Settings::default();
        settings.dashboard.config_path = Some(dashboard_path.to_string_lossy().into_owned());
        let catalog = PersistenceCatalog::new(&root(root_directory.path()), &settings);

        let dashboard = catalog.get(PersistentStoreId::DashboardConfig);
        assert_eq!(dashboard.path, dashboard_path);
        assert_eq!(dashboard.ownership, StoreOwnership::External);
        assert!(dashboard.externally_configured);
        assert_eq!(dashboard.backup_policy, BackupPolicy::ExcludeExternal);

        let scratchpad = catalog.get(PersistentStoreId::Scratchpad);
        assert_eq!(scratchpad.path, scratchpad_path);
        assert_eq!(scratchpad.ownership, StoreOwnership::External);
        assert!(scratchpad.externally_configured);
        assert_eq!(scratchpad.backup_policy, BackupPolicy::ExcludeExternal);
        assert_eq!(std::fs::read_to_string(&dashboard_path).unwrap(), bytes);
        assert!(!scratchpad_path.exists());
    }

    #[test]
    fn public_catalog_resolves_configured_launcher_log_path() {
        let root_directory = tempfile::tempdir().unwrap();
        let external_directory = tempfile::tempdir().unwrap();
        let log_path = external_directory.path().join("custom.log");
        let mut settings = Settings::default();
        settings.log_file = Some(crate::settings::LogFile::Path(
            log_path.to_string_lossy().into_owned(),
        ));

        let catalog = PersistenceCatalog::new(&root(root_directory.path()), &settings);
        let log = catalog.get(PersistentStoreId::LauncherLog);

        assert_eq!(log.path, log_path);
        assert_eq!(log.ownership, StoreOwnership::External);
        assert!(log.externally_configured);
        assert_eq!(log.backup_policy, BackupPolicy::ExcludeRuntime);
    }

    #[test]
    fn health_probe_distinguishes_every_supported_status_without_writing() {
        let directory = tempfile::tempdir().unwrap();
        let missing = json_descriptor(
            &directory.path().join("missing.json"),
            probe_json::<Vec<String>>,
        );
        assert_eq!(missing.probe(), StoreHealth::Missing);

        let path = directory.path().join("state.json");
        let descriptor = json_descriptor(&path, probe_json::<Vec<String>>);
        std::fs::write(&path, " \r\n").unwrap();
        assert_eq!(descriptor.probe(), StoreHealth::Empty);
        std::fs::write(&path, r#"["ok"]"#).unwrap();
        assert_eq!(descriptor.probe(), StoreHealth::Healthy);
        std::fs::write(&path, "not json").unwrap();
        assert!(matches!(descriptor.probe(), StoreHealth::Malformed { .. }));

        let unreadable = json_descriptor(directory.path(), probe_json::<Vec<String>>);
        assert!(matches!(unreadable.probe(), StoreHealth::Unreadable { .. }));

        std::fs::write(
            &path,
            format!(
                r#"{{"schema_version":{},"gestures":[]}}"#,
                crate::mouse_gestures::db::SCHEMA_VERSION + 1
            ),
        )
        .unwrap();
        let future = json_descriptor(&path, probe_gestures);
        assert_eq!(
            future.probe(),
            StoreHealth::UnsupportedSchema {
                version: (crate::mouse_gestures::db::SCHEMA_VERSION + 1).to_string()
            }
        );
    }

    #[test]
    fn legacy_and_repairable_documents_are_probed_byte_for_byte_read_only() {
        let directory = tempfile::tempdir().unwrap();
        let cases: Vec<(&str, &[u8], fn(&Path, &[u8]) -> ProbeResult)> = vec![
            (
                "todo.json",
                br#"[{"text":"legacy","done":false}]"#,
                probe_json::<Vec<TodoEntry>>,
            ),
            (
                "layouts.json",
                br#"{"version":0,"layouts":[]}"#,
                probe_json::<LayoutStore>,
            ),
            (
                "mouse_gestures.json",
                br#"{"schema_version":1,"gestures":[]}"#,
                probe_gestures,
            ),
            (
                "multi_manager_workspaces.json",
                br#"[{"name":"Legacy","home_rect":[1,2,3,4]}]"#,
                probe_json::<Vec<MmWorkspace>>,
            ),
            (
                "mkmacros.json",
                br#"{"schema_version":9,"macros":[]}"#,
                probe_mkmacro,
            ),
            (
                "clipboard_modifiers.json",
                br#"{"schema_version":0}"#,
                probe_clipboard_modifiers,
            ),
        ];
        for (name, bytes, probe) in cases {
            let path = directory.path().join(name);
            std::fs::write(&path, bytes).unwrap();
            let descriptor = json_descriptor(&path, probe);
            assert_eq!(descriptor.probe(), StoreHealth::Healthy, "{name}");
            assert_eq!(std::fs::read(&path).unwrap(), bytes, "{name}");
        }

        let invalid_clipboard = directory.path().join("invalid-clipboard.json");
        let invalid_bytes = br#"{"schema_version":0,"unexpected":true}"#;
        std::fs::write(&invalid_clipboard, invalid_bytes).unwrap();
        assert_eq!(
            json_descriptor(&invalid_clipboard, probe_clipboard_modifiers).probe(),
            StoreHealth::Malformed {
                message: "stored data does not match the expected format".into()
            }
        );
        assert_eq!(std::fs::read(&invalid_clipboard).unwrap(), invalid_bytes);

        let invalid_mkmacro = directory.path().join("invalid-mkmacros.json");
        let invalid_bytes = br#"{"schema_version":9,"macros":["bad"]}"#;
        std::fs::write(&invalid_mkmacro, invalid_bytes).unwrap();
        assert!(matches!(
            json_descriptor(&invalid_mkmacro, probe_mkmacro).probe(),
            StoreHealth::Malformed { .. }
        ));
        assert_eq!(std::fs::read(&invalid_mkmacro).unwrap(), invalid_bytes);
    }

    #[test]
    fn backup_privacy_and_restore_matrix_is_canonical() {
        let directory = tempfile::tempdir().unwrap();
        let mut settings = Settings::default();
        settings.multi_manager.workspaces_path = "owned/workspaces.json".into();
        let catalog = test_catalog(directory.path(), &settings);
        for store in catalog.stores() {
            match store.criticality {
                StoreCriticality::Critical => {
                    let expected = if store.ownership == StoreOwnership::ApplicationOwned {
                        BackupPolicy::Include
                    } else {
                        BackupPolicy::ExcludeExternal
                    };
                    assert_eq!(store.backup_policy, expected, "{:?}", store.id);
                    assert_eq!(store.restore_eligible, expected == BackupPolicy::Include);
                }
                StoreCriticality::Replaceable => {
                    assert_eq!(store.backup_policy, BackupPolicy::ExcludeReplaceable);
                    assert!(!store.restore_eligible);
                }
                StoreCriticality::Runtime => {
                    assert_eq!(store.backup_policy, BackupPolicy::ExcludeRuntime);
                    assert!(!store.restore_eligible);
                }
            }
        }
        assert_eq!(
            catalog.get(PersistentStoreId::Snippets).privacy,
            StorePrivacy::Sensitive
        );
        assert_eq!(
            catalog
                .get(PersistentStoreId::ClipboardHistory)
                .backup_policy,
            BackupPolicy::ExcludeReplaceable
        );
    }

    #[test]
    fn notes_directory_probe_does_not_create_missing_paths() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("missing-notes");
        assert_eq!(probe_directory(&path, Some("md")), StoreHealth::Missing);
        assert!(!path.exists());
    }
}
