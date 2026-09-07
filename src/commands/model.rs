use crate::actions::Action;
use crate::clipboard_modify::actions::{
    ClipboardModifyActionPayload, ClipboardModifySectionPayload,
};
use crate::common::entity_ref::EntityRef;
use crate::diff::query::DiffOpenPayload;
use crate::file_search::actions::{FileSearchModePayload, FileSearchStartPayload};
use crate::mouse_gestures::selection::{GestureFocusArgs, GestureToggleArgs};
use crate::persistence::{PersistentStoreId, RecoveryTarget};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivationSource {
    Enter,
    Click,
    Dashboard,
    Gesture,
    Macro,
}

impl ActivationSource {
    pub fn label(self) -> &'static str {
        match self {
            Self::Enter => "enter",
            Self::Click => "click",
            Self::Dashboard => "dashboard",
            Self::Gesture => "gesture",
            Self::Macro => "macro",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CommandInvocation {
    pub command: Command,
    pub original_action: Action,
    pub query_override: Option<String>,
    pub source: ActivationSource,
}

impl CommandInvocation {
    pub fn domain(&self) -> &'static str {
        self.command.domain()
    }
    pub fn kind_name(&self) -> &'static str {
        self.command.kind_name()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Launcher(LauncherCommand),
    Query(QueryCommand),
    Dialog(DialogCommand),
    Calendar(CalendarCommand),
    Note(NoteCommand),
    Link(LinkCommand),
    Todo(TodoCommand),
    MouseGesture(MouseGestureCommand),
    MultiManager(MultiManagerCommand),
    FileSearch(FileSearchCommand),
    Diff(DiffCommand),
    ClipboardModify(ClipboardModifyCommand),
    Screenshot(ScreenshotCommand),
    Shell(ShellCommand),
    Clipboard(ClipboardCommand),
    Calculator(CalculatorCommand),
    Storage(StorageCommand),
    Timer(TimerCommand),
    System(SystemCommand),
    BrowserTab(BrowserTabCommand),
    Media(MediaCommand),
    Layout(LayoutCommand),
    Macro(MacroCommand),
    Crop(CropCommand),
    Data(DataCommand),
    External(ExternalCommand),
}

impl Command {
    pub fn domain(&self) -> &'static str {
        match self {
            Self::Launcher(_) => "launcher",
            Self::Query(_) => "query",
            Self::Dialog(_) => "dialog",
            Self::Calendar(_) => "calendar",
            Self::Note(_) => "note",
            Self::Link(_) => "link",
            Self::Todo(_) => "todo",
            Self::MouseGesture(_) => "mouse_gesture",
            Self::MultiManager(_) => "multi_manager",
            Self::FileSearch(_) => "file_search",
            Self::Diff(_) => "diff",
            Self::ClipboardModify(_) => "clipboard_modify",
            Self::Screenshot(_) => "screenshot",
            Self::Shell(_) => "shell",
            Self::Clipboard(_) => "clipboard",
            Self::Calculator(_) => "calculator",
            Self::Storage(_) => "storage",
            Self::Timer(_) => "timer",
            Self::System(_) => "system",
            Self::BrowserTab(_) => "browser_tab",
            Self::Media(_) => "media",
            Self::Layout(_) => "layout",
            Self::Macro(_) => "macro",
            Self::Crop(_) => "crop",
            Self::Data(_) => "data",
            Self::External(_) => "external",
        }
    }
    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::Launcher(v) => v.kind_name(),
            Self::Query(v) => v.kind_name(),
            Self::Dialog(v) => v.kind_name(),
            Self::Calendar(v) => v.kind_name(),
            Self::Note(v) => v.kind_name(),
            Self::Link(v) => v.kind_name(),
            Self::Todo(v) => v.kind_name(),
            Self::MouseGesture(v) => v.kind_name(),
            Self::MultiManager(v) => v.kind_name(),
            Self::FileSearch(v) => v.kind_name(),
            Self::Diff(v) => v.kind_name(),
            Self::ClipboardModify(v) => v.kind_name(),
            Self::Screenshot(v) => v.kind_name(),
            Self::Shell(v) => v.kind_name(),
            Self::Clipboard(v) => v.kind_name(),
            Self::Calculator(v) => v.kind_name(),
            Self::Storage(v) => v.kind_name(),
            Self::Timer(v) => v.kind_name(),
            Self::System(v) => v.kind_name(),
            Self::BrowserTab(v) => v.kind_name(),
            Self::Media(v) => v.kind_name(),
            Self::Layout(v) => v.kind_name(),
            Self::Macro(v) => v.kind_name(),
            Self::Crop(v) => v.kind_name(),
            Self::Data(v) => v.kind_name(),
            Self::External(v) => v.kind_name(),
        }
    }
}

macro_rules! kinds { ($t:ty, $($pat:pat => $name:literal),+ $(,)?) => { impl $t { pub fn kind_name(&self) -> &'static str { match self { $($pat => $name,)+ } } } }; }

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LauncherCommand {
    Toggle,
    Show { query: Option<String> },
    Hide,
    Focus,
    Restore,
}
kinds!(LauncherCommand, Self::Toggle => "toggle", Self::Show { .. } => "show", Self::Hide => "hide", Self::Focus => "focus", Self::Restore => "restore");
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QueryCommand {
    Set {
        query: String,
        argument: Option<String>,
    },
    ExecuteFirst {
        query: String,
    },
}
kinds!(QueryCommand, Self::Set { .. } => "set", Self::ExecuteFirst { .. } => "execute_first");
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DialogCommand {
    Help,
    Convert,
    Settings,
    DashboardSettings,
    Theme,
}
kinds!(DialogCommand, Self::Help => "help", Self::Convert => "convert", Self::Settings => "settings", Self::DashboardSettings => "dashboard_settings", Self::Theme => "theme");
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CalendarCommand {
    Open { view: String },
    Jump { reference: String },
    Add { input: String },
    Search { input: String },
    Upcoming,
    Snooze { input: String },
}
kinds!(CalendarCommand, Self::Open { .. } => "open", Self::Jump { .. } => "jump", Self::Add { .. } => "add", Self::Search { .. } => "search", Self::Upcoming => "upcoming", Self::Snooze { .. } => "snooze");
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NoteCommand {
    Dialog,
    GraphDialog {
        args: Option<String>,
    },
    UnusedAssets,
    Open {
        slug: String,
    },
    New {
        slug: String,
        template: Option<String>,
    },
    MalformedNew {
        raw: String,
        args: Option<String>,
        error: String,
    },
    TemplatesDisabled,
    Tags,
    OpenLink {
        link: String,
    },
    WrapLinks {
        slug: String,
    },
    Remove {
        slug: String,
    },
    Reload,
}
kinds!(NoteCommand, Self::Dialog => "dialog", Self::GraphDialog { .. } => "graph_dialog", Self::UnusedAssets => "unused_assets", Self::Open { .. } => "open", Self::New { .. } => "new", Self::MalformedNew { .. } => "malformed_new", Self::TemplatesDisabled => "templates_disabled", Self::Tags => "tags", Self::OpenLink { .. } => "open_link", Self::WrapLinks { .. } => "wrap_links", Self::Remove { .. } => "remove", Self::Reload => "reload");
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkCommand {
    Open { id: String },
}
kinds!(LinkCommand, Self::Open { .. } => "open");
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TodoCommand {
    Dialog,
    View,
    Add {
        text: String,
        priority: u8,
        tags: Vec<String>,
        refs: Vec<EntityRef>,
        toast_text: String,
    },
    SetPriority {
        index: usize,
        priority: u8,
    },
    SetTags {
        index: usize,
        tags: Vec<String>,
    },
    Remove {
        index: usize,
    },
    Done {
        index: usize,
    },
    Edit {
        index: usize,
    },
    Compatibility {
        kind: TodoCompatibilityKind,
    },
    Clear,
    Export,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TodoCompatibilityKind {
    Add { toast_text: String },
    SetPriority,
    SetTags,
    Remove,
    Done,
    Edit,
}
kinds!(TodoCommand, Self::Dialog => "dialog", Self::View => "view", Self::Add { .. } => "add", Self::SetPriority { .. } => "set_priority", Self::SetTags { .. } => "set_tags", Self::Remove { .. } => "remove", Self::Done { .. } => "done", Self::Edit { .. } => "edit", Self::Compatibility { .. } => "compatibility", Self::Clear => "clear", Self::Export => "export");
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MouseGestureCommand {
    Dialog,
    Add,
    Binding,
    Focus { args: Option<GestureFocusArgs> },
    Settings,
    Toggle { args: Option<GestureToggleArgs> },
}
kinds!(MouseGestureCommand, Self::Dialog => "dialog", Self::Add => "add", Self::Binding => "binding", Self::Focus { .. } => "focus", Self::Settings => "settings", Self::Toggle { .. } => "toggle");
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MultiManagerCommand {
    Open,
    Settings,
    Save,
    Reload,
    SendAllHome,
    Reconnect,
    SaveBindings,
    RestoreBindings,
    Import,
    RecaptureAll,
    Toggle(String),
    Home(String),
    Target(String),
    Capture(String),
    Disable(String),
    Enable(String),
}
kinds!(MultiManagerCommand, Self::Open => "open", Self::Settings => "settings", Self::Save => "save", Self::Reload => "reload", Self::SendAllHome => "send_all_home", Self::Reconnect => "reconnect", Self::SaveBindings => "save_bindings", Self::RestoreBindings => "restore_bindings", Self::Import => "import", Self::RecaptureAll => "recapture_all", Self::Toggle(_) => "toggle", Self::Home(_) => "home", Self::Target(_) => "target", Self::Capture(_) => "capture", Self::Disable(_) => "disable", Self::Enable(_) => "enable");
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileSearchCommand {
    Open,
    Cancel,
    SetMode(FileSearchModePayload),
    Start(FileSearchStartPayload),
    Invalid { raw: String, error: String },
}
kinds!(FileSearchCommand, Self::Open => "open", Self::Cancel => "cancel", Self::SetMode(_) => "set_mode", Self::Start(_) => "start", Self::Invalid { .. } => "invalid");
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiffCommand {
    Open(DiffOpenPayload),
    Invalid { raw: String, error: String },
}
kinds!(DiffCommand, Self::Open(_) => "open", Self::Invalid { .. } => "invalid");
#[derive(Clone, Debug, PartialEq)]
pub enum ClipboardModifyCommand {
    Open {
        section: ClipboardModifySectionPayload,
    },
    Execute {
        payload: Option<ClipboardModifyActionPayload>,
        raw_argument: Option<String>,
        payload_error: Option<String>,
    },
    Undo {
        raw_argument: Option<String>,
    },
    Error {
        message: String,
    },
}
kinds!(ClipboardModifyCommand, Self::Open { .. } => "open", Self::Execute { .. } => "execute", Self::Undo { .. } => "undo", Self::Error { .. } => "error");
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenshotMode {
    Window,
    Region,
    Desktop,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenshotDestination {
    Editor,
    Clipboard,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenshotMarkup {
    Rectangle,
    Pen,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenshotCompatibility {
    Shared,
    GuiOnly,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScreenshotCommand {
    Capture {
        mode: ScreenshotMode,
        destination: ScreenshotDestination,
        markup: ScreenshotMarkup,
        compatibility: ScreenshotCompatibility,
    },
    UnknownMode {
        raw: String,
    },
}
kinds!(ScreenshotCommand, Self::Capture { .. } => "capture", Self::UnknownMode { .. } => "unknown_mode");
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShellCommand {
    Dialog,
    Run { command: String, keep_open: bool },
    Add { name: String, args: String },
    Remove { name: String },
}
kinds!(ShellCommand, Self::Dialog => "dialog", Self::Run { .. } => "run", Self::Add { .. } => "add", Self::Remove { .. } => "remove");
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClipboardCommand {
    Dialog,
    Clear,
    Copy { index: usize },
    SetText { text: String },
}
kinds!(ClipboardCommand, Self::Dialog => "dialog", Self::Clear => "clear", Self::Copy { .. } => "copy", Self::SetText { .. } => "set_text");
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CalculatorCommand {
    CopyResult {
        result: String,
        expression: Option<String>,
    },
    CopyHistory {
        index: usize,
    },
}
kinds!(CalculatorCommand, Self::CopyResult { .. } => "copy_result", Self::CopyHistory { .. } => "copy_history");
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StorageCommand {
    BookmarkDialog,
    BookmarkAdd(String),
    BookmarkRemove(String),
    FolderAdd(String),
    FolderRemove(String),
    HistoryClear,
    HistoryLaunch(usize),
    SnippetAdd {
        alias: String,
        text: String,
    },
    SnippetEdit(String),
    SnippetRemove(String),
    SnippetDialog,
    FavoriteAdd {
        label: String,
        command: String,
        args: Option<String>,
    },
    FavoriteRemove(String),
    FavoriteDialog(String),
    TempfileNew(Option<String>),
    TempfileDialog,
    TempfileOpen,
    TempfileOpenFile(String),
    TempfileClear,
    TempfileRemove(String),
    TempfileAlias {
        path: String,
        alias: String,
    },
    InvalidTempfileAlias,
    RecycleClean,
}
kinds!(StorageCommand, Self::BookmarkDialog => "bookmark_dialog", Self::BookmarkAdd(_) => "bookmark_add", Self::BookmarkRemove(_) => "bookmark_remove", Self::FolderAdd(_) => "folder_add", Self::FolderRemove(_) => "folder_remove", Self::HistoryClear => "history_clear", Self::HistoryLaunch(_) => "history_launch", Self::SnippetAdd { .. } => "snippet_add", Self::SnippetEdit(_) => "snippet_edit", Self::SnippetRemove(_) => "snippet_remove", Self::SnippetDialog => "snippet_dialog", Self::FavoriteAdd { .. } => "favorite_add", Self::FavoriteRemove(_) => "favorite_remove", Self::FavoriteDialog(_) => "favorite_dialog", Self::TempfileNew(_) => "tempfile_new", Self::TempfileDialog => "tempfile_dialog", Self::TempfileOpen => "tempfile_open", Self::TempfileOpenFile(_) => "tempfile_open_file", Self::TempfileClear => "tempfile_clear", Self::TempfileRemove(_) => "tempfile_remove", Self::TempfileAlias { .. } => "tempfile_alias", Self::InvalidTempfileAlias => "invalid_tempfile_alias", Self::RecycleClean => "recycle_clean");
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TimerCommand {
    TimerDialog,
    AlarmDialog,
    Cancel(u64),
    Pause(u64),
    Resume(u64),
    InvalidCancel,
    InvalidPause,
    InvalidResume,
    Start { duration: String, name: String },
    AlarmSet { time: String, name: String },
    StopwatchPause(u64),
    StopwatchResume(u64),
    StopwatchStop(u64),
    StopwatchStart(String),
    StopwatchShow(u64),
}
kinds!(TimerCommand, Self::TimerDialog => "timer_dialog", Self::AlarmDialog => "alarm_dialog", Self::Cancel(_) => "cancel", Self::Pause(_) => "pause", Self::Resume(_) => "resume", Self::InvalidCancel => "invalid_cancel", Self::InvalidPause => "invalid_pause", Self::InvalidResume => "invalid_resume", Self::Start { .. } => "start", Self::AlarmSet { .. } => "alarm_set", Self::StopwatchPause(_) => "stopwatch_pause", Self::StopwatchResume(_) => "stopwatch_resume", Self::StopwatchStop(_) => "stopwatch_stop", Self::StopwatchStart(_) => "stopwatch_start", Self::StopwatchShow(_) => "stopwatch_show");
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SystemCommand {
    BrightnessDialog,
    VolumeDialog,
    CpuList(usize),
    InvalidCpuList,
    Shutdown,
    Reboot,
    Lock,
    Logoff,
    Unknown(String),
    ProcessKill(u32),
    ProcessSwitch(u32),
    WindowSwitch(isize),
    WindowClose(isize),
    Brightness(u32),
    Volume(u32),
    ProcessVolume { pid: u32, level: u32 },
    ProcessToggleMute(u32),
    MuteActive,
    ToggleMute,
    PowerPlan(String),
    Keys(String),
}
kinds!(SystemCommand, Self::BrightnessDialog => "brightness_dialog", Self::VolumeDialog => "volume_dialog", Self::CpuList(_) => "cpu_list", Self::InvalidCpuList => "invalid_cpu_list", Self::Shutdown => "shutdown", Self::Reboot => "reboot", Self::Lock => "lock", Self::Logoff => "logoff", Self::Unknown(_) => "unknown", Self::ProcessKill(_) => "process_kill", Self::ProcessSwitch(_) => "process_switch", Self::WindowSwitch(_) => "window_switch", Self::WindowClose(_) => "window_close", Self::Brightness(_) => "brightness", Self::Volume(_) => "volume", Self::ProcessVolume { .. } => "process_volume", Self::ProcessToggleMute(_) => "process_toggle_mute", Self::MuteActive => "mute_active", Self::ToggleMute => "toggle_mute", Self::PowerPlan(_) => "power_plan", Self::Keys(_) => "keys");
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BrowserTabCommand {
    Switch(Vec<i32>),
    InvalidSwitch,
    Cache,
    Clear,
}
kinds!(BrowserTabCommand, Self::Switch(_) => "switch", Self::InvalidSwitch => "invalid_switch", Self::Cache => "cache", Self::Clear => "clear");
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaCommand {
    Play,
    Pause,
    Next,
    Previous,
}
kinds!(MediaCommand, Self::Play => "play", Self::Pause => "pause", Self::Next => "next", Self::Previous => "previous");
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LayoutCommand {
    Save { name: String, flags: Option<String> },
    Load { name: String, flags: Option<String> },
    Show { name: String, flags: Option<String> },
    Remove { name: String, flags: Option<String> },
    List { flags: Option<String> },
    Edit,
}
kinds!(LayoutCommand, Self::Save { .. } => "save", Self::Load { .. } => "load", Self::Show { .. } => "show", Self::Remove { .. } => "remove", Self::List { .. } => "list", Self::Edit => "edit");
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MacroCommand {
    LegacyDialog,
    MkDialog,
    RunLegacy(String),
    MkRun(u64),
    MkPause,
    MkResume,
    MkStop,
    MkRecord,
    MkRecordStop,
    Invalid { raw: String },
}
kinds!(MacroCommand, Self::LegacyDialog => "legacy_dialog", Self::MkDialog => "mk_dialog", Self::RunLegacy(_) => "run_legacy", Self::MkRun(_) => "mk_run", Self::MkPause => "mk_pause", Self::MkResume => "mk_resume", Self::MkStop => "mk_stop", Self::MkRecord => "mk_record", Self::MkRecordStop => "mk_record_stop", Self::Invalid { .. } => "invalid");
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CropCommand {
    Image,
    Screenshot,
}
kinds!(CropCommand, Self::Image => "image", Self::Screenshot => "screenshot");

/// User-facing data maintenance commands.
///
/// Recovery is intentionally nested under this domain instead of extending
/// [`StorageCommand`], whose established meaning is launcher catalog data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DataCommand {
    Dialog,
    Health,
    Backup,
    OpenFolder,
    Recovery(DataRecoveryCommand),
    Invalid { raw: String, error: String },
}

impl DataCommand {
    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::Dialog => "dialog",
            Self::Health => "health",
            Self::Backup => "backup",
            Self::OpenFolder => "folder",
            Self::Recovery(command) => command.kind_name(),
            Self::Invalid { .. } => "invalid",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataDialogFocus {
    Overview,
    Health,
}

/// Proof that a destructive recovery request came from an explicit UI
/// confirmation step. Raw action parsing cannot produce this token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DataRecoveryConfirmation(());

impl DataRecoveryConfirmation {
    /// Minted only by the crate-owned confirmation UI after the user accepts
    /// the destructive recovery prompt.
    pub(crate) fn from_explicit_user_confirmation() -> Self {
        Self(())
    }
}

/// Recovery requests are constructed by the Data & Recovery UI after the user
/// has selected a typed catalog store and explicitly confirmed the operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DataRecoveryCommand {
    Restore {
        target: RecoveryTarget,
        snapshot_id: String,
        confirmation: DataRecoveryConfirmation,
    },
    Reset {
        store_id: PersistentStoreId,
        confirmation: DataRecoveryConfirmation,
    },
}

impl DataRecoveryCommand {
    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::Restore { .. } => "restore",
            Self::Reset { .. } => "reset",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExternalNamespace {
    Favorite,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExternalCommand {
    pub target: String,
    pub args: Option<String>,
    pub namespace: Option<ExternalNamespace>,
}
kinds!(ExternalCommand, Self { .. } => "launch");
