use std::pin::Pin;
#[cfg(windows)]
use std::sync::mpsc::{Receiver, Sender, channel};
mod appearance;

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    ffi::{OsStr, OsString},
    future::Future,
    path::PathBuf,
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
    },
    task::{Context as FutureContext, Poll, Waker},
    time::Duration,
};

use gpui_kit::StatefulInteractiveElement;
use gpui_kit::base::Disableable;
use gpui_kit::component::IndexPath;
use gpui_kit::component::input::{InputEvent, MoveDown, MoveUp};
use gpui_kit::component::{
    WindowExt as _,
    button::Button,
    input::{Input, InputState, Textarea, TextareaState},
    label::Label,
    searchable_list::SearchableListItem,
    select::{Select, SelectEvent, SelectState},
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use fuzzy_matcher::skim::SkimMatcherV2;

#[cfg(windows)]
use std::ffi::c_void;

pub(crate) use crate::gpui_restart::Frontend;
use crate::gpui_restart::{
    EffectPermit, HashWork, RestartAdmission, RestartBlocker, RestartContext, RestartPlan,
};
use multi_launcher::SettingsSaveWorkerRegistry;
use multi_launcher::actions::Action;
use multi_launcher::actions::search::{CachedSearchEntry, ReadOnlySearchInput, compose_read_only_results};
use multi_launcher::common::query::ActionFilterMetadata;
use multi_launcher::clipboard_modify::clipboard::{
    ArboardClipboardBackend, ClipboardBackend, ClipboardError,
};
use multi_launcher::commands::handlers::handle_hash_generator;
use multi_launcher::commands::{
    ActivationSource, Command, CommandInvocation, CommandOutcome, HashGeneratorCommand,
    parse_action, parse_command,
};
use multi_launcher::coordinate_tool::{
    CoordinateToolController, CoordinateToolPreferences, NativeCoordinateRuntimeFactory,
};
use multi_launcher::hash_generator::{
    HashAlgorithm, HashClipboardCopy, HashComparison, HashInputMode, HashSession,
};
use multi_launcher::hotkey::HotkeyListener;
#[cfg(windows)]
use multi_launcher::hotkey::runtime::HotkeyTrigger;
use multi_launcher::launcher::query_history::{QueryHistoryDirection, QueryHistoryNavigator};
use multi_launcher::settings::{
    AppliedWindowSize, NativeWindowGeometry, PreferredWindowSize, RememberedPresentation,
    UtilityDialogSizes, UtilityPresentations, UtilityToolKey,
};
use multi_launcher::utility_geometry_binding::{
    CheckedNativeWindow, NativeGeometryBinding, NativeGeometrySlot, NativeGeometryStamp,
    NativeWindowMetrics, NativeWindowRect, native_window_metrics, new_native_geometry_slot,
    restore_native_window_geometry, take_native_geometry,
};
use multi_launcher::utility_presentation::{UtilityPresentationStore, UtilityPresentationUpdate};
use multi_launcher::utility_size::{
    PhysicalWindowRect, UtilityNativeGeometrySample, UtilitySizeStore,
};

pub(crate) struct LauncherSearchStartup {
    pub actions: Vec<Action>,
    pub match_exact: bool,
    pub fuzzy_weight: f32,
    pub usage_weight: f32,
    pub usage: HashMap<String, u32>,
    pub folder_aliases_lc: HashMap<String, Option<String>>,
    pub bookmark_aliases_lc: HashMap<String, Option<String>>,
}

struct LauncherSearchCatalog {
    actions: Vec<Action>,
    cached_entries: Vec<CachedSearchEntry>,
    filter_metadata: Vec<ActionFilterMetadata>,
    matcher: SkimMatcherV2,
    match_exact: bool,
    fuzzy_weight: f32,
    usage_weight: f32,
    usage: HashMap<String, u32>,
    folder_aliases_lc: HashMap<String, Option<String>>,
    bookmark_aliases_lc: HashMap<String, Option<String>>,
}

impl LauncherSearchCatalog {
    fn new(startup: LauncherSearchStartup) -> Self {
        // The GPUI slice currently has one installed typed result route. Keep
        // only persisted actions that the real activation boundary can parse.
        let actions = supported_hash_search_actions(startup.actions);
        let cached_entries = actions.iter().map(CachedSearchEntry::from_action).collect();
        let filter_metadata = actions
            .iter()
            .map(ActionFilterMetadata::from_action)
            .collect();
        Self {
            actions,
            cached_entries,
            filter_metadata,
            matcher: SkimMatcherV2::default(),
            match_exact: startup.match_exact,
            fuzzy_weight: startup.fuzzy_weight,
            usage_weight: startup.usage_weight,
            usage: startup.usage,
            folder_aliases_lc: startup.folder_aliases_lc,
            bookmark_aliases_lc: startup.bookmark_aliases_lc,
        }
    }

    fn compose(
        &self,
        raw_query: &str,
        provider_actions: Vec<Action>,
    ) -> Vec<Action> {
        // GPUI currently has no ordinary empty-query catalog or command cache.
        // Clearing Search therefore restores its existing blank result state.
        if raw_query.trim().is_empty() {
            return Vec::new();
        }
        compose_read_only_results(
            ReadOnlySearchInput {
                raw_query,
                command_cache: &[],
                local_actions: &self.actions,
                local_cached_entries: &self.cached_entries,
                local_filter_metadata: &self.filter_metadata,
                folder_aliases_lc: &self.folder_aliases_lc,
                bookmark_aliases_lc: &self.bookmark_aliases_lc,
                matcher: &self.matcher,
                match_exact: self.match_exact,
                fuzzy_weight: self.fuzzy_weight,
                collect_search_stats: false,
                collect_candidate_count: false,
                usage: &self.usage,
                usage_weight: self.usage_weight,
                provider_results: provider_actions
                    .into_iter()
                    .map(|action| (action, 0.0))
                    .collect(),
            },
            |_| {},
            None,
        )
        .actions
    }
}

fn supported_hash_search_actions(actions: Vec<Action>) -> Vec<Action> {
    actions
        .into_iter()
        .filter(|action| {
            matches!(
                parse_action(action),
                Ok(Command::HashGenerator(HashGeneratorCommand::Open))
            )
        })
        .collect()
}

fn single_line_history_queries<'a>(queries: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    queries
        .into_iter()
        .filter(|query| !query.contains(['\n', '\r']))
        .map(str::to_owned)
        .collect()
}

gpui_kit::actions!([
    ToggleSecondaryWindow,
    ToggleCoordinateHudProbe,
    OpenHashGenerator,
    SearchHistoryOlder,
    SearchHistoryNewer,
    FocusDockedHashResize,
    CalculateHash,
    DockHashGenerator,
    PopoutHashGenerator,
    CloseHashGenerator,
    RestartWithEgui
]);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FrontendSelection {
    DefaultEgui,
    Explicit(Frontend),
}

impl FrontendSelection {
    pub(crate) const fn frontend(self) -> Frontend {
        match self {
            Self::DefaultEgui | Self::Explicit(Frontend::Egui) => Frontend::Egui,
            Self::Explicit(Frontend::Gpui) => Frontend::Gpui,
        }
    }

    const fn requested_frontend(self) -> Option<Frontend> {
        match self {
            Self::DefaultEgui => None,
            Self::Explicit(frontend) => Some(frontend),
        }
    }
}

fn selector_choice_message() -> String {
    format!(
        "Choose {} or {}.",
        Frontend::Egui.selector_argument(),
        Frontend::Gpui.selector_argument()
    )
}

pub(crate) fn selection_from_args(
    args: impl IntoIterator<Item = OsString>,
) -> Result<FrontendSelection, String> {
    let mut selection = FrontendSelection::DefaultEgui;

    for arg in args {
        match arg.as_os_str() {
            value if value == OsStr::new(Frontend::Egui.selector_argument()) => {
                if matches!(selection, FrontendSelection::Explicit(_)) {
                    return Err("Specify only one --ui selector.".to_owned());
                }
                selection = FrontendSelection::Explicit(Frontend::Egui);
            }
            value if value == OsStr::new(Frontend::Gpui.selector_argument()) => {
                if matches!(selection, FrontendSelection::Explicit(_)) {
                    return Err("Specify only one --ui selector.".to_owned());
                }
                selection = FrontendSelection::Explicit(Frontend::Gpui);
            }
            value if value == OsStr::new("--ui") => {
                return Err(selector_choice_message());
            }
            value if value.to_string_lossy().starts_with("--ui=") => {
                return Err(selector_choice_message());
            }
            _ => {}
        }
    }

    Ok(selection)
}

pub(crate) fn duplicate_instance_message(selection: FrontendSelection) -> Option<String> {
    let selector = selection.requested_frontend()?.selector_argument();
    Some(format!(
        "The data directory is already owned by another Multi Launcher instance. Close that instance, then rerun with {selector} from the same working directory."
    ))
}

pub(crate) fn show_startup_error(message: &str) {
    #[cfg(windows)]
    {
        use windows::{
            Win32::{
                Foundation::HWND,
                UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW},
            },
            core::PCWSTR,
        };

        let message: Vec<u16> = message.encode_utf16().chain(Some(0)).collect();
        let title: Vec<u16> = "Multi Launcher".encode_utf16().chain(Some(0)).collect();

        // SAFETY: both strings remain alive and NUL-terminated throughout the call.
        unsafe {
            MessageBoxW(
                HWND::default(),
                PCWSTR(message.as_ptr()),
                PCWSTR(title.as_ptr()),
                MB_OK | MB_ICONERROR,
            );
        }
    }

    #[cfg(not(windows))]
    eprintln!("Multi Launcher: {message}");
}

fn toggle_coordinate_hud_probe(
    controller: &mut CoordinateToolController,
) -> Result<String, String> {
    let enabled = !controller.runtime_state().hud_enabled();
    let disabled_sample = if enabled {
        None
    } else {
        controller.sample_for_copy().ok()
    };
    controller.set_hud_enabled(enabled)?;
    if !enabled {
        let dpi = disabled_sample
            .as_ref()
            .and_then(|sample| sample.monitor.as_ref())
            .and_then(|monitor| monitor.effective_dpi);
        return Ok(match dpi {
            Some((horizontal, vertical)) => format!(
                "Coordinate HUD disabled. Last sample monitor DPI: {horizontal}×{vertical}."
            ),
            None => "Coordinate HUD disabled.".to_owned(),
        });
    }

    match controller.sample_for_copy() {
        Ok(sample) => {
            let point = sample.desktop_point;
            let mut status = format!("Coordinate HUD enabled at ({}, {}).", point.x, point.y);
            if let Some((horizontal, vertical)) = sample
                .monitor
                .as_ref()
                .and_then(|monitor| monitor.effective_dpi)
            {
                status.push_str(&format!(" Monitor DPI: {horizontal}×{vertical}."));
            }
            Ok(status)
        }
        Err(error) => Ok(format!(
            "Coordinate HUD enabled; current sample unavailable: {error}"
        )),
    }
}

struct RootWindow {
    root_window: Option<AnyWindowHandle>,
    listener_slot: Arc<Mutex<Option<HotkeyListener>>>,
    root_visibility: RootVisibility,
    query: Entity<InputState>,
    enabled_plugins: Option<HashSet<String>>,
    enabled_capabilities: Option<HashMap<String, Vec<String>>>,
    search_catalog: LauncherSearchCatalog,
    launcher_results: LauncherSearchResults,
    query_history: QueryHistoryNavigator,
    fresh_search_enter: Option<FreshSearchEnter>,
    native_window: Option<NativeWindow>,
    focus_handle: FocusHandle,
    resize_focus_handle: FocusHandle,
    root_scroll: ScrollHandle,
    utility_presentations: UtilityPresentationStore,
    utility_size_store: Arc<Mutex<UtilitySizeStore>>,
    utility_size_polling: Arc<AtomicBool>,
    hash_docked_preferred_size: PreferredWindowSize,
    hash_docked_resize: Arc<Mutex<Option<DockedHashResize>>>,
    hash_geometry_sequence: Arc<AtomicU64>,
    hash_tool_key: UtilityToolKey,
    hash_session: Arc<Mutex<HashSession>>,
    hash_wake_signal: Arc<HashWakeSignal>,
    hash_file_picker_pending: Arc<AtomicBool>,
    hash_clipboard_pending: Arc<HashClipboardPendingState>,
    hash_clipboard_operation: Option<HashClipboardOperation>,
    hash_clipboard_operation_counter: u64,
    hash_window: HashWindowState,
    hash_window_revision: Arc<AtomicU64>,
    hash_mount_generation: Arc<AtomicU64>,
    hash_mount_counter: u64,
    hash_rehost_pending: Arc<AtomicBool>,
    hash_rehost_revision: Arc<AtomicU64>,
    hash_window_close_subscriptions: Vec<(AnyWindowHandle, Subscription)>,
    hash_retiring_windows: Vec<RetiringHashWindow>,
    hash_presentation_feedback: Arc<Mutex<Option<String>>>,
    hash_open_error: Option<String>,
    hash_presentation_error: Option<String>,
    hash_native_geometry_error: Option<String>,
    hash_size_write_error: Option<String>,
    secondary: SecondaryWindowState,
    secondary_revision: Arc<AtomicU64>,
    secondary_close_subscription: Option<Subscription>,
    secondary_error: Option<String>,
    coordinate_hud_probe_controller: Arc<Mutex<CoordinateToolController>>,
    coordinate_hud_probe_closing: Arc<AtomicBool>,
    coordinate_hud_probe_pending: bool,
    coordinate_hud_probe_status: String,
    coordinate_shutdown_complete: Arc<AtomicBool>,
    restart_context: Option<RestartContext>,
    restart_admission: Arc<RestartAdmission>,
    restart_retiring: Arc<AtomicBool>,
    restart_status: Option<String>,
    restart_exit: Rc<RefCell<Option<(RestartContext, Vec<NativeWindow>)>>>,
    restart_failure: Rc<RefCell<Option<String>>>,
}

#[derive(Default)]
struct LauncherSearchResults {
    generation: u64,
    query: String,
    actions: Vec<multi_launcher::actions::Action>,
    selected: Option<usize>,
}

impl LauncherSearchResults {
    fn replace(&mut self, query: String, actions: Vec<multi_launcher::actions::Action>) {
        self.generation = self.generation.wrapping_add(1).max(1);
        self.query = query;
        self.actions = actions;
        self.selected = None;
    }

    fn select_down(&mut self) -> bool {
        if self.actions.is_empty() {
            return false;
        }
        let next = match self.selected {
            Some(index) if index < self.actions.len() => {
                Some((index + 1).min(self.actions.len() - 1))
            }
            _ => Some(0),
        };
        let changed = self.selected != next;
        self.selected = next;
        changed
    }

    fn select_up(&mut self) -> bool {
        let next = match self.selected {
            Some(index) if index > 0 && index < self.actions.len() => Some(index - 1),
            _ => None,
        };
        let changed = self.selected != next;
        self.selected = next;
        changed
    }

    fn enter_target(&self, current_query: &str) -> Option<(u64, usize)> {
        if self.query != current_query {
            return None;
        }
        let index = self
            .selected
            .filter(|index| *index < self.actions.len())
            .or_else(|| (self.actions.len() == 1).then_some(0))?;
        Some((self.generation, index))
    }

    fn resolve_action(
        &self,
        generation: u64,
        index: usize,
        current_query: &str,
    ) -> Option<multi_launcher::actions::Action> {
        if generation != self.generation || self.query != current_query {
            return None;
        }
        self.actions.get(index).cloned()
    }
}

fn launcher_result_row_id(generation: u64, absolute_index: usize) -> String {
    format!("launcher-search-result-row-{generation}-{absolute_index}")
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FreshSearchEnter {
    generation: u64,
    query: String,
}

fn is_fresh_plain_search_enter(
    focused: bool,
    key: &str,
    modifiers: Modifiers,
    is_held: bool,
) -> bool {
    focused && key == "enter" && modifiers == Modifiers::default() && !is_held
}

fn fresh_search_enter_marker(
    focused: bool,
    key: &str,
    modifiers: Modifiers,
    is_held: bool,
    generation: u64,
    query: String,
) -> Option<FreshSearchEnter> {
    is_fresh_plain_search_enter(focused, key, modifiers, is_held)
        .then_some(FreshSearchEnter { generation, query })
}

fn parse_hash_search_invocation(
    action: multi_launcher::actions::Action,
    source: ActivationSource,
) -> Result<CommandInvocation, String> {
    let invocation = parse_command(action, None, source).map_err(|error| error.to_string())?;
    if matches!(
        &invocation.command,
        Command::HashGenerator(HashGeneratorCommand::Open)
    ) {
        Ok(invocation)
    } else {
        Err("The selected launcher action is not an installed Hash Generator route.".to_owned())
    }
}

fn dispatch_hash_search_invocation(
    invocation: CommandInvocation,
    open: impl FnOnce() -> Result<(), String>,
) -> Result<CommandOutcome, String> {
    match invocation.command {
        Command::HashGenerator(command) => handle_hash_generator(&command, open),
        _ => Err("The selected launcher action is not an installed Hash Generator route.".into()),
    }
}

fn result_click_source(is_keyboard: bool) -> ActivationSource {
    if is_keyboard {
        ActivationSource::Enter
    } else {
        ActivationSource::Click
    }
}

enum HashWindowState {
    Closed,
    Docked {
        mount_generation: u64,
        view: Entity<HashGeneratorWindow>,
    },
    Popout {
        mount_generation: u64,
        window: AnyWindowHandle,
        view: Entity<HashGeneratorWindow>,
    },
}

enum BuiltHashMount {
    Docked {
        view: Entity<HashGeneratorWindow>,
    },
    Popout {
        window: AnyWindowHandle,
        view: Entity<HashGeneratorWindow>,
        native_window: Option<NativeWindow>,
        close_subscription: Subscription,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HashPresentationMode {
    Docked,
    Popout,
}

#[derive(Clone, Copy)]
enum DockedHashAction {
    Calculate,
    Dock,
    Popout,
    Close,
}

const HASH_DOCK_FACTORY_SIZE: PreferredWindowSize = PreferredWindowSize {
    width: 620.0,
    height: 440.0,
};
const HASH_DOCK_MIN_WIDTH: f32 = 420.0;
const HASH_DOCK_MIN_HEIGHT: f32 = 320.0;
const HASH_DOCK_MAX_DIMENSION: f32 = 8192.0;
const HASH_DOCK_RESIZE_HANDLE_SIZE: f32 = 26.0;
const HASH_DOCK_KEYBOARD_RESIZE_STEP: f32 = 16.0;
const HASH_ROOT_CONTROLS_WIDTH: f32 = 520.0;

#[derive(Clone, Copy, Debug)]
enum DockedHashResizeOrigin {
    Pointer {
        position: Point<Pixels>,
        visible_size: AppliedWindowSize,
    },
    Keyboard,
}

#[derive(Clone, Copy, Debug)]
struct DockedHashResize {
    mount_generation: u64,
    session_epoch: u64,
    start_size: PreferredWindowSize,
    available_at_start: AppliedWindowSize,
    preview_size: PreferredWindowSize,
    changed_width: bool,
    changed_height: bool,
    drag_started: bool,
    origin: DockedHashResizeOrigin,
}

#[derive(Clone)]
struct DockedHashResizePayload {
    mount_generation: u64,
    session_epoch: u64,
    start_size: PreferredWindowSize,
    preview_extent: Arc<Mutex<Option<DockedHashResize>>>,
}

impl DockedHashResizePayload {
    fn mark_drag_started(&self) {
        let mut preview = self
            .preview_extent
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(resize) = preview.as_mut().filter(|resize| {
            resize.mount_generation == self.mount_generation
                && resize.session_epoch == self.session_epoch
                && resize.start_size == self.start_size
                && matches!(resize.origin, DockedHashResizeOrigin::Pointer { .. })
        }) {
            resize.drag_started = true;
        }
    }
}

fn docked_hash_available_size(
    viewport_width: f32,
    viewport_height: f32,
    window_padding: f32,
) -> AppliedWindowSize {
    let content_width = (viewport_width - window_padding * 2.0).max(1.0);
    let height = (viewport_height - window_padding * 2.0).max(1.0);
    AppliedWindowSize {
        width: content_width,
        height,
    }
}

fn docked_hash_fit(
    preferred: PreferredWindowSize,
    available: AppliedWindowSize,
) -> AppliedWindowSize {
    preferred.apply_within(available.width, available.height)
}

fn docked_hash_resize_accessibility_label(
    preferred: PreferredWindowSize,
    current_visible: AppliedWindowSize,
    preview: PreferredWindowSize,
    preview_visible: AppliedWindowSize,
    preview_active: bool,
) -> String {
    let current_preferred = format!("{:.0} by {:.0}", preferred.width, preferred.height);
    let current_visible = format!(
        "{:.0} by {:.0}",
        current_visible.width, current_visible.height
    );
    if preview_active {
        format!(
            "Resize docked Hash Generator. Current preferred size: {current_preferred} logical points, displayed {current_visible}. Preview size: {:.0} by {:.0} logical points, displayed {:.0} by {:.0}. Use arrow keys to preview, Enter to save, Escape to discard.",
            preview.width, preview.height, preview_visible.width, preview_visible.height,
        )
    } else {
        format!(
            "Resize docked Hash Generator. Current preferred size: {current_preferred} logical points, displayed {current_visible}. Use arrow keys to preview, Enter to save, Escape to discard."
        )
    }
}

fn docked_hash_resize_identity_matches(
    active_mount_generation: u64,
    mount_generation: u64,
    active_session_epoch: u64,
    session_epoch: u64,
    docked_mount_is_current: bool,
) -> bool {
    mount_generation != 0
        && active_mount_generation == mount_generation
        && active_session_epoch == session_epoch
        && docked_mount_is_current
}

fn pointer_docked_resize_preview(
    start: PreferredWindowSize,
    visible_at_start: AppliedWindowSize,
    delta_x: f32,
    delta_y: f32,
) -> (PreferredWindowSize, bool, bool) {
    let changed_width = delta_x.abs() > 1.0;
    let changed_height = delta_y.abs() > 1.0;
    let width = if changed_width {
        (visible_at_start.width + delta_x).clamp(HASH_DOCK_MIN_WIDTH, HASH_DOCK_MAX_DIMENSION)
    } else {
        start.width
    };
    let height = if changed_height {
        (visible_at_start.height + delta_y).clamp(HASH_DOCK_MIN_HEIGHT, HASH_DOCK_MAX_DIMENSION)
    } else {
        start.height
    };
    (
        PreferredWindowSize::new(width, height).unwrap_or(start),
        changed_width,
        changed_height,
    )
}

fn keyboard_docked_resize_preview(
    current: PreferredWindowSize,
    delta_x: f32,
    delta_y: f32,
) -> PreferredWindowSize {
    PreferredWindowSize::new(
        if delta_x == 0.0 {
            current.width
        } else {
            (current.width + delta_x).clamp(HASH_DOCK_MIN_WIDTH, HASH_DOCK_MAX_DIMENSION)
        },
        if delta_y == 0.0 {
            current.height
        } else {
            (current.height + delta_y).clamp(HASH_DOCK_MIN_HEIGHT, HASH_DOCK_MAX_DIMENSION)
        },
    )
    .unwrap_or(current)
}

fn docked_resize_commit_size(resize: DockedHashResize) -> Option<PreferredWindowSize> {
    if matches!(resize.origin, DockedHashResizeOrigin::Pointer { .. }) && !resize.drag_started {
        return None;
    }
    let applied_preview = docked_hash_fit(resize.preview_size, resize.available_at_start);
    let visible_at_start = match resize.origin {
        DockedHashResizeOrigin::Pointer { visible_size, .. } => visible_size,
        DockedHashResizeOrigin::Keyboard => {
            docked_hash_fit(resize.start_size, resize.available_at_start)
        }
    };
    let width = if resize.changed_width
        && (applied_preview.width - visible_at_start.width).abs() > 1.0
        && (resize.preview_size.width - resize.start_size.width).abs() > 1.0
    {
        resize.preview_size.width
    } else {
        resize.start_size.width
    };
    let height = if resize.changed_height
        && (applied_preview.height - visible_at_start.height).abs() > 1.0
        && (resize.preview_size.height - resize.start_size.height).abs() > 1.0
    {
        resize.preview_size.height
    } else {
        resize.start_size.height
    };
    if (width - resize.start_size.width).abs() <= 1.0
        && (height - resize.start_size.height).abs() <= 1.0
    {
        return None;
    }
    PreferredWindowSize::new(width, height).ok()
}

impl HashPresentationMode {
    fn from_saved(preference: Option<RememberedPresentation>) -> Self {
        match preference {
            Some(RememberedPresentation::Popout) => Self::Popout,
            Some(RememberedPresentation::Docked) | None => Self::Docked,
        }
    }

    fn remembered(self) -> RememberedPresentation {
        match self {
            Self::Docked => RememberedPresentation::Docked,
            Self::Popout => RememberedPresentation::Popout,
        }
    }
}

fn hash_mount_identity_matches(
    active_mount_generation: u64,
    mount_generation: u64,
    active_session_epoch: u64,
    session_epoch: u64,
) -> bool {
    mount_generation != 0
        && active_mount_generation == mount_generation
        && active_session_epoch == session_epoch
}

fn docked_hash_focus_is_current(
    requested_mount_generation: Option<u64>,
    active_mount_generation: u64,
    docked_mount_generation: Option<u64>,
) -> bool {
    requested_mount_generation.is_some_and(|generation| {
        generation != 0
            && generation == active_mount_generation
            && docked_mount_generation == Some(generation)
    })
}

fn root_search_focus_is_current(
    current_mode: Option<HashPresentationMode>,
    current_mount_generation: Option<u64>,
    active_mount_generation: u64,
    requested_mode: Option<HashPresentationMode>,
    requested_mount_generation: u64,
    restart_fenced: bool,
) -> bool {
    !restart_fenced
        && current_mode == requested_mode
        && current_mount_generation
            == (requested_mount_generation != 0).then_some(requested_mount_generation)
        && active_mount_generation == requested_mount_generation
}

fn search_input_is_current(
    root_visible: bool,
    lifecycle_active: bool,
    root_window_matches: bool,
    search_focused: bool,
    window_active: bool,
) -> bool {
    root_visible && lifecycle_active && root_window_matches && search_focused && window_active
}

impl HashWindowState {
    fn mode(&self) -> Option<HashPresentationMode> {
        match self {
            Self::Closed => None,
            Self::Docked { .. } => Some(HashPresentationMode::Docked),
            Self::Popout { .. } => Some(HashPresentationMode::Popout),
        }
    }

    fn mount_generation(&self) -> Option<u64> {
        match self {
            Self::Closed => None,
            Self::Docked {
                mount_generation, ..
            }
            | Self::Popout {
                mount_generation, ..
            } => Some(*mount_generation),
        }
    }

    fn view(&self) -> Option<&Entity<HashGeneratorWindow>> {
        match self {
            Self::Closed => None,
            Self::Docked { view, .. } | Self::Popout { view, .. } => Some(view),
        }
    }
}

#[derive(Clone, Copy)]
struct RetiringHashWindow {
    window: AnyWindowHandle,
    native_window: Option<NativeWindow>,
}

struct HashMountConfig {
    session: Arc<Mutex<HashSession>>,
    wake_signal: Arc<HashWakeSignal>,
    file_picker_pending: Arc<AtomicBool>,
    clipboard_pending: Arc<HashClipboardPendingState>,
    restart_admission: Arc<RestartAdmission>,
    mount_generation: u64,
    session_epoch: u64,
    active_mount_generation: Arc<AtomicU64>,
    rehost_pending: Arc<AtomicBool>,
    rehost_revision: Arc<AtomicU64>,
    presentation_feedback: Arc<Mutex<Option<String>>>,
    native_geometry_sequence: Arc<AtomicU64>,
    hash_tool_key: UtilityToolKey,
    size_write_error: Option<String>,
    root: WeakEntity<RootWindow>,
    mode: HashPresentationMode,
}

#[derive(Clone, Copy, Debug)]
struct HashPickerRequest {
    mount_generation: u64,
    session_epoch: u64,
    source_generation: u64,
}

struct HashPickerTarget {
    mode: HashPresentationMode,
    window: AnyWindowHandle,
    view: Entity<HashGeneratorWindow>,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HashClipboardPendingKind {
    Idle = 0,
    Load = 1,
    Copy = 2,
}

#[derive(Default)]
struct HashClipboardPendingState(AtomicU8);

impl HashClipboardPendingState {
    fn begin(&self, kind: HashClipboardPendingKind) -> bool {
        debug_assert_ne!(kind, HashClipboardPendingKind::Idle);
        self.0
            .compare_exchange(
                HashClipboardPendingKind::Idle as u8,
                kind as u8,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
    }

    fn finish(&self) {
        self.0
            .store(HashClipboardPendingKind::Idle as u8, Ordering::Release);
    }

    fn kind(&self) -> HashClipboardPendingKind {
        match self.0.load(Ordering::Acquire) {
            1 => HashClipboardPendingKind::Load,
            2 => HashClipboardPendingKind::Copy,
            _ => HashClipboardPendingKind::Idle,
        }
    }

    fn is_pending(&self) -> bool {
        self.kind() != HashClipboardPendingKind::Idle
    }

    fn is_copying(&self) -> bool {
        self.kind() == HashClipboardPendingKind::Copy
    }
}

fn hash_clipboard_pending_feedback(kind: HashClipboardPendingKind) -> Option<&'static str> {
    match kind {
        HashClipboardPendingKind::Idle => None,
        HashClipboardPendingKind::Load => Some(
            "Loading Clipboard… Wait for it to finish before closing. A source change discards this load.",
        ),
        HashClipboardPendingKind::Copy => Some(
            "Copying digest… Editing, Calculate, rehosting, and closing are paused until the clipboard write finishes.",
        ),
    }
}

fn hash_clipboard_close_message(kind: HashClipboardPendingKind) -> Option<&'static str> {
    match kind {
        HashClipboardPendingKind::Idle => None,
        HashClipboardPendingKind::Load => {
            Some("Wait for the Clipboard load to finish before closing Multi Launcher.")
        }
        HashClipboardPendingKind::Copy => {
            Some("Wait for the digest clipboard write to finish before closing Multi Launcher.")
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct HashClipboardRequest {
    mount_generation: u64,
    session_epoch: u64,
    source_generation: u64,
}

#[derive(Clone)]
enum HashClipboardOperationKind {
    Load,
    Copy(HashClipboardCopy),
}

struct HashClipboardOperation {
    id: u64,
    request: HashClipboardRequest,
    kind: HashClipboardOperationKind,
}

enum HashClipboardOutcome {
    Load(Result<String, ClipboardError>),
    Copy(Result<(), ClipboardError>),
}

struct HashClipboardTarget {
    window: AnyWindowHandle,
    mount_generation: u64,
    session_epoch: u64,
    view: Entity<HashGeneratorWindow>,
}

fn hash_clipboard_copy_admissible(
    pending: HashClipboardPendingKind,
    file_picker_pending: bool,
) -> bool {
    pending == HashClipboardPendingKind::Idle && !file_picker_pending
}

fn hash_file_picker_admissible(
    pending: HashClipboardPendingKind,
    file_picker_pending: bool,
) -> bool {
    !file_picker_pending && pending != HashClipboardPendingKind::Copy
}

enum HashPickerOutcome {
    Selected(PathBuf),
    Cancelled,
    Failed(String),
}

fn classify_hash_picker_result<E>(
    result: Result<anyhow::Result<Option<Vec<PathBuf>>>, E>,
) -> HashPickerOutcome {
    match result {
        Ok(Ok(Some(mut paths))) => paths.drain(..).next().map_or_else(
            || HashPickerOutcome::Failed("The file picker returned no file".to_owned()),
            HashPickerOutcome::Selected,
        ),
        Ok(Ok(None)) => HashPickerOutcome::Cancelled,
        Ok(Err(error)) => HashPickerOutcome::Failed(error.to_string()),
        Err(_) => HashPickerOutcome::Failed(
            "The file picker did not return a selection result".to_owned(),
        ),
    }
}

struct HashPickerPendingGuard {
    pending: Arc<AtomicBool>,
    cleared: bool,
}

impl HashPickerPendingGuard {
    fn new(pending: Arc<AtomicBool>) -> Self {
        Self {
            pending,
            cleared: false,
        }
    }

    fn clear_and_notify(&mut self, root: &WeakEntity<RootWindow>, cx: &mut AsyncApp) {
        self.pending.store(false, Ordering::Release);
        self.cleared = true;
        let _ = root.update(cx, |root, cx| {
            if let Some(view) = root.hash_window.view().cloned() {
                view.update(cx, |_, cx| cx.notify());
            }
            cx.notify();
        });
    }
}

impl Drop for HashPickerPendingGuard {
    fn drop(&mut self) {
        if !self.cleared {
            self.pending.store(false, Ordering::Release);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct HashDraft {
    text: String,
    expected: String,
    mode: HashInputMode,
    algorithm: HashAlgorithm,
    file_path: Option<PathBuf>,
    clipboard_snapshot_revision: Option<u64>,
}

impl HashDraft {
    fn from_session(session: &HashSession) -> Self {
        Self {
            text: session.text().to_owned(),
            expected: session.expected().to_owned(),
            mode: session.mode(),
            algorithm: session.algorithm(),
            file_path: session.file_path().map(PathBuf::from),
            clipboard_snapshot_revision: session.clipboard_snapshot_revision(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HashSourceChoice {
    Text,
    Clipboard,
    File,
}

impl HashSourceChoice {
    const fn mode(self) -> HashInputMode {
        match self {
            Self::Text => HashInputMode::Text,
            Self::Clipboard => HashInputMode::Clipboard,
            Self::File => HashInputMode::File,
        }
    }

    const fn from_mode(mode: HashInputMode) -> Option<Self> {
        match mode {
            HashInputMode::Text => Some(Self::Text),
            HashInputMode::Clipboard => Some(Self::Clipboard),
            HashInputMode::File => Some(Self::File),
        }
    }
}

impl SearchableListItem for HashSourceChoice {
    type Value = Self;

    fn title(&self) -> SharedString {
        match self {
            Self::Text => "Text",
            Self::Clipboard => "Clipboard",
            Self::File => "File",
        }
        .into()
    }

    fn render(&self, _: &mut Window, _: &mut App) -> impl IntoElement {
        Label::new(self.title())
    }

    fn value(&self) -> &Self::Value {
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct HashAlgorithmChoice(HashAlgorithm);

impl SearchableListItem for HashAlgorithmChoice {
    type Value = Self;

    fn title(&self) -> SharedString {
        self.0.name().into()
    }

    fn render(&self, _: &mut Window, _: &mut App) -> impl IntoElement {
        Label::new(self.title())
    }

    fn value(&self) -> &Self::Value {
        self
    }
}

fn hash_picker_result_is_current(
    active_mount_generation: u64,
    mount_generation: u64,
    active_session_epoch: u64,
    session_epoch: u64,
    active_source_generation: u64,
    source_generation: u64,
) -> bool {
    hash_mount_identity_matches(
        active_mount_generation,
        mount_generation,
        active_session_epoch,
        session_epoch,
    ) && active_source_generation == source_generation
}

fn hash_clipboard_request_is_current(
    active_mount_generation: u64,
    active_session_epoch: u64,
    active_source_generation: u64,
    request: HashClipboardRequest,
    rehost_pending: bool,
) -> bool {
    !rehost_pending
        && hash_picker_result_is_current(
            active_mount_generation,
            request.mount_generation,
            active_session_epoch,
            request.session_epoch,
            active_source_generation,
            request.source_generation,
        )
}

fn create_hash_generator_view(
    window: &mut Window,
    cx: &mut App,
    initial_draft: HashDraft,
    native_window: Option<NativeWindow>,
    config: HashMountConfig,
) -> Entity<HashGeneratorWindow> {
    let HashMountConfig {
        session,
        wake_signal,
        file_picker_pending,
        clipboard_pending,
        restart_admission,
        mount_generation,
        session_epoch,
        active_mount_generation,
        rehost_pending,
        rehost_revision,
        presentation_feedback,
        native_geometry_sequence,
        hash_tool_key,
        size_write_error,
        root,
        mode,
    } = config;
    if mode == HashPresentationMode::Popout {
        let pending = Arc::clone(&clipboard_pending);
        let root_for_close = root.clone();
        window.on_window_should_close(cx, move |_, app| {
            let Some(message) = hash_clipboard_close_message(pending.kind()) else {
                return true;
            };
            let _ = root_for_close.update(app, |root, cx| {
                root.restart_status = Some(message.to_owned());
                cx.notify();
            });
            false
        });
    }
    let text = cx.new(|cx| {
        TextareaState::new(window, cx)
            .default_value(initial_draft.text.clone())
            .auto_grow(4, 10)
            .placeholder("Enter text to hash")
    });
    let expected = cx.new(|cx| {
        TextareaState::new(window, cx)
            .default_value(initial_draft.expected.clone())
            .auto_grow(1, 2)
            .placeholder("Optional expected digest")
    });

    let source_index = HashSourceChoice::from_mode(initial_draft.mode).map(|source| match source {
        HashSourceChoice::Text => 0,
        HashSourceChoice::Clipboard => 1,
        HashSourceChoice::File => 2,
    });
    let source = cx.new(|cx| {
        SelectState::new(
            vec![
                HashSourceChoice::Text,
                HashSourceChoice::Clipboard,
                HashSourceChoice::File,
            ],
            source_index.map(|index| IndexPath::new(index)),
            window,
            cx,
        )
    });
    let algorithm_index = match initial_draft.algorithm {
        HashAlgorithm::Sha256 => 0,
        HashAlgorithm::Sha512 => 1,
        HashAlgorithm::Sha1 => 2,
    };
    let algorithm = cx.new(|cx| {
        SelectState::new(
            vec![
                HashAlgorithmChoice(HashAlgorithm::Sha256),
                HashAlgorithmChoice(HashAlgorithm::Sha512),
                HashAlgorithmChoice(HashAlgorithm::Sha1),
            ],
            Some(IndexPath::new(algorithm_index)),
            window,
            cx,
        )
    });

    if initial_draft.mode == HashInputMode::Text {
        text.focus_handle(cx).focus(window, cx);
    } else {
        source.update(cx, |state, cx| state.focus(window, cx));
    }

    let native_geometry_slot = new_native_geometry_slot();
    let native_geometry_stamp = NativeGeometryStamp {
        session_epoch,
        mount_generation,
    };
    let mut native_geometry_error = None;
    let factory_outer_size = native_window
        .and_then(|window| checked_geometry_target(window))
        .and_then(|target| match native_window_metrics(target, 1.0) {
            Ok(Some(metrics)) => outer_size_in_points(metrics),
            Ok(None) => None,
            Err(error) => {
                native_geometry_error = Some(format!(
                    "Could not inspect the Hash Generator window geometry: {error}"
                ));
                None
            }
        });
    let text_for_subscription = text.clone();
    let expected_for_subscription = expected.clone();
    let source_for_subscription = source.clone();
    let algorithm_for_subscription = algorithm.clone();
    cx.new(move |cx| {
        let text_subscription = cx.subscribe(
            &text_for_subscription,
            |this: &mut HashGeneratorWindow,
             text: Entity<TextareaState>,
             event: &InputEvent,
             cx: &mut Context<HashGeneratorWindow>| {
                if !matches!(event, InputEvent::Change)
                    || this.rehost_pending.load(Ordering::Acquire)
                    || this.clipboard_pending.is_copying()
                    || !this.is_current_mount()
                {
                    return;
                }
                let Some(_operation) = this.restart_admission.admit() else {
                    return;
                };
                let text = text.read(cx).value().to_string();
                let mut session = this
                    .session
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if session.mode() != HashInputMode::Text {
                    return;
                }
                if !hash_mount_identity_matches(
                    this.active_mount_generation.load(Ordering::Acquire),
                    this.mount_generation,
                    session.epoch(),
                    this.session_epoch,
                ) {
                    return;
                }
                session.set_text(text);
                cx.notify();
            },
        );

        let expected_subscription = cx.subscribe(
            &expected_for_subscription,
            |this: &mut HashGeneratorWindow,
             expected: Entity<TextareaState>,
             event: &InputEvent,
             cx: &mut Context<HashGeneratorWindow>| {
                if !matches!(event, InputEvent::Change)
                    || this.rehost_pending.load(Ordering::Acquire)
                    || this.clipboard_pending.is_copying()
                    || !this.is_current_mount()
                {
                    return;
                }
                let Some(_operation) = this.restart_admission.admit() else {
                    return;
                };
                let mut session = this
                    .session
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if session.epoch() != this.session_epoch {
                    return;
                }
                session.set_expected(expected.read(cx).value().to_string());
                cx.notify();
            },
        );

        let source_subscription = cx.subscribe(
            &source_for_subscription,
            |this: &mut HashGeneratorWindow,
             source: Entity<SelectState<Vec<HashSourceChoice>>>,
             event: &SelectEvent<Vec<HashSourceChoice>>,
             cx: &mut Context<HashGeneratorWindow>| {
                if this.clipboard_pending.is_copying() || !this.is_current_mount() {
                    return;
                }
                let SelectEvent::Confirm(Some(choice)) = event else {
                    return;
                };
                let Some(_operation) = this.restart_admission.admit() else {
                    return;
                };
                let selected = source.read(cx).selected_value().copied();
                if selected != Some(*choice) {
                    return;
                }
                let mut session = this
                    .session
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if session.epoch() != this.session_epoch {
                    return;
                }
                session.set_mode(choice.mode());
                this.file_picker_feedback = None;
                cx.notify();
            },
        );

        let algorithm_subscription = cx.subscribe(
            &algorithm_for_subscription,
            |this: &mut HashGeneratorWindow,
             algorithm: Entity<SelectState<Vec<HashAlgorithmChoice>>>,
             event: &SelectEvent<Vec<HashAlgorithmChoice>>,
             cx: &mut Context<HashGeneratorWindow>| {
                if this.clipboard_pending.is_copying() || !this.is_current_mount() {
                    return;
                }
                let SelectEvent::Confirm(Some(choice)) = event else {
                    return;
                };
                let Some(_operation) = this.restart_admission.admit() else {
                    return;
                };
                let selected = algorithm.read(cx).selected_value().copied();
                if selected != Some(*choice) {
                    return;
                }
                let mut session = this
                    .session
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if session.epoch() != this.session_epoch {
                    return;
                }
                session.set_algorithm(choice.0);
                cx.notify();
            },
        );

        HashGeneratorWindow {
            text,
            expected,
            source,
            algorithm,
            focus_handle: cx.focus_handle(),
            session,
            wake_signal,
            file_picker_pending,
            clipboard_pending,
            native_window,
            native_geometry_slot,
            native_geometry_stamp,
            hash_tool_key,
            native_geometry_sequence,
            native_geometry_binding: None,
            native_geometry_error,
            size_write_error,
            factory_outer_size,
            restart_admission,
            mount_generation,
            session_epoch,
            active_mount_generation,
            rehost_pending,
            rehost_revision,
            presentation_feedback,
            root,
            mode,
            file_picker_feedback: None,
            _subscriptions: vec![
                text_subscription,
                expected_subscription,
                source_subscription,
                algorithm_subscription,
            ],
        }
    })
}

fn hash_popout_window_options(cx: &App) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::centered(size(px(620.), px(440.)), cx)),
        window_min_size: Some(size(px(420.), px(320.))),
        titlebar: Some(TitlebarOptions {
            title: Some("Multi Launcher GPUI Hash Generator".into()),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Coalescing wake signal for the one retained HashSession job.
///
/// The receiver awaits this future instead of blocking a GPUI executor worker.
/// The signal is closed from app/root teardown so the stored task waker cannot
/// retain an abandoned receiver task.
#[derive(Default)]
struct HashWakeSignal {
    state: Mutex<HashWakeState>,
}

#[derive(Default)]
struct HashWakeState {
    pending: bool,
    closed: bool,
    waker: Option<Waker>,
}

impl HashWakeSignal {
    fn notify(&self) {
        let waker = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if state.closed {
                return;
            }
            state.pending = true;
            state.waker.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }

    fn close(&self) {
        let waker = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.closed = true;
            state.waker.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }

    fn wait(self: &Arc<Self>) -> HashWakeFuture {
        HashWakeFuture {
            signal: Arc::clone(self),
        }
    }
}

struct HashWakeFuture {
    signal: Arc<HashWakeSignal>,
}

impl Future for HashWakeFuture {
    type Output = bool;

    fn poll(self: Pin<&mut Self>, cx: &mut FutureContext<'_>) -> Poll<Self::Output> {
        let mut state = self
            .get_mut()
            .signal
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.closed {
            Poll::Ready(false)
        } else if state.pending {
            state.pending = false;
            Poll::Ready(true)
        } else {
            state.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

impl Drop for HashWakeFuture {
    fn drop(&mut self) {
        self.signal
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .waker
            .take();
    }
}

enum SecondaryWindowState {
    Closed,
    OpenPending,
    Live {
        window: AnyWindowHandle,
        view: Entity<SecondaryWindow>,
    },
}

fn update_kit_window_content<T: 'static, C: AppContext, R>(
    window: AnyWindowHandle,
    content: &Entity<T>,
    cx: &mut C,
    update: impl FnOnce(&mut T, &mut Window, &mut Context<T>) -> R,
) -> anyhow::Result<R> {
    window.update(cx, |_, window, cx| {
        content.update(cx, |content, content_cx| {
            update(content, window, content_cx)
        })
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RestartDrafts {
    search: String,
    utility: Option<String>,
    hash: HashDraft,
    hash_window_open: bool,
}

struct RestartSnapshot {
    drafts: RestartDrafts,
    hash_work: HashWork,
    child_windows: Vec<AnyWindowHandle>,
    native_windows: Vec<NativeWindow>,
}

struct AcceptedRestart {
    context: RestartContext,
    root_window: AnyWindowHandle,
    child_windows: Vec<AnyWindowHandle>,
    native_windows: Vec<NativeWindow>,
    listener_slot: Arc<Mutex<Option<HotkeyListener>>>,
    coordinate_controller: Arc<Mutex<CoordinateToolController>>,
    coordinate_shutdown_complete: Arc<AtomicBool>,
    restart_retiring: Arc<AtomicBool>,
    restart_exit: Rc<RefCell<Option<(RestartContext, Vec<NativeWindow>)>>>,
    restart_failure: Rc<RefCell<Option<String>>>,
}

async fn wait_for_native_retirement(cx: &mut AsyncApp, windows: &[NativeWindow]) -> bool {
    for _ in 0..100 {
        if windows
            .iter()
            .all(|window| !native_window_is_valid(*window))
        {
            return true;
        }
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
    windows
        .iter()
        .all(|window| !native_window_is_valid(*window))
}

async fn wait_for_hash_window_retirement(
    cx: &mut AsyncApp,
    native_window: Option<NativeWindow>,
) -> bool {
    if let Some(native_window) = native_window {
        return wait_for_native_retirement(cx, &[native_window]).await;
    }
    #[cfg(windows)]
    {
        // A Windows handle must be checked before it can be retired safely.
        return false;
    }
    #[cfg(not(windows))]
    {
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
        true
    }
}

async fn remove_and_wait_for_hash_window(cx: &mut AsyncApp, retiring: RetiringHashWindow) -> bool {
    if let Err(error) = cx.update(|app| {
        retiring
            .window
            .update(app, |_, window, _| window.remove_window())
    }) {
        tracing::debug!(%error, "GPUI Hash window was already unavailable during retirement");
    }
    wait_for_hash_window_retirement(cx, retiring.native_window).await
}

async fn restore_root_search_focus_after_hash_close(
    root: WeakEntity<RootWindow>,
    cx: &mut AsyncApp,
) {
    let target = root.update(cx, |root, cx| {
        let may_restore_search = root.root_visibility.visible.load(Ordering::Acquire)
            && root_search_focus_is_current(
                root.hash_window.mode(),
                root.hash_window.mount_generation(),
                root.hash_mount_generation.load(Ordering::Acquire),
                None,
                0,
                root.restart_admission.is_fenced(),
            )
            && (!cfg!(windows) || root.native_window.is_some_and(native_window_is_valid));
        let target = may_restore_search.then(|| {
            root.root_window
                .map(|root_window| (root_window, root.native_window))
        });
        if target.is_some() {
            cx.notify();
        }
        target.flatten()
    });
    let Ok(Some((root_window, native_root))) = target else {
        return;
    };
    let Some(root_view) = root.upgrade() else {
        return;
    };

    let _ = update_kit_window_content(root_window, &root_view, cx, |root, window, cx| {
        let root_handle_matches = root
            .root_window
            .is_some_and(|handle| handle.window_id() == root_window.window_id());
        let still_current = root_handle_matches
            && root.root_visibility.visible.load(Ordering::Acquire)
            && root_search_focus_is_current(
                root.hash_window.mode(),
                root.hash_window.mount_generation(),
                root.hash_mount_generation.load(Ordering::Acquire),
                None,
                0,
                root.restart_admission.is_fenced(),
            )
            && root.native_window == native_root
            && (!cfg!(windows) || native_root.is_some_and(native_window_is_valid));
        if still_current {
            // This updates the root window's logical focus without activating it.
            root.query.focus_handle(cx).focus(window, cx);
        }
    });
}

fn describe_restart_drafts(drafts: &RestartDrafts) -> String {
    let mut names = Vec::new();
    if !drafts.search.is_empty() {
        names.push("Search query");
    }
    if drafts.utility.as_ref().is_some_and(|text| !text.is_empty()) {
        names.push("retained utility editor");
    }
    if !drafts.hash.text.is_empty() {
        names.push("Hash Generator text");
    }
    if !drafts.hash.expected.is_empty() {
        names.push("Expected digest");
    }
    if drafts.hash.file_path.is_some() {
        names.push("selected Hash Generator file");
    }
    if drafts.hash.clipboard_snapshot_revision.is_some() {
        names.push("loaded Clipboard snapshot");
    }
    if drafts.hash.mode != HashInputMode::Text {
        names.push("Hash Generator source selection");
    }
    if drafts.hash.algorithm != HashAlgorithm::Sha256 {
        names.push("non-default Hash Generator algorithm");
    }
    if names.is_empty() {
        return "Restart Multi Launcher in the egui fallback? The GPUI windows will close."
            .to_owned();
    }
    format!(
        "Restart Multi Launcher in the egui fallback? This will discard the current {} and close the GPUI windows.",
        names.join(", ")
    )
}

fn restart_blocker_message(blocker: RestartBlocker) -> String {
    match blocker {
        RestartBlocker::HashWork(HashWork::Active) => {
            "Hash Generator is still calculating. Cancel it or wait for it to finish, then retry the egui restart.".to_owned()
        }
        RestartBlocker::HashWork(HashWork::Retiring) => {
            "Hash Generator is still stopping its previous calculation. Wait for it to retire, then retry the egui restart.".to_owned()
        }
        RestartBlocker::HashWork(HashWork::Idle) => {
            "The restart confirmation is no longer active. Start it again to switch to egui.".to_owned()
        }
        RestartBlocker::PendingEffects(count) => format!(
            "Wait for the {count} pending GPUI operation(s) to finish, then retry the egui restart."
        ),
        RestartBlocker::ConfirmationInactive => {
            "The egui restart confirmation is no longer active. Start it again to switch frontends."
                .to_owned()
        }
    }
}

impl RootWindow {
    fn focus_docked_hash_resize_handle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.hash_window.mode() != Some(HashPresentationMode::Docked)
            || self.hash_rehost_pending.load(Ordering::Acquire)
            || self.restart_admission.is_fenced()
        {
            return;
        }
        self.root_scroll.scroll_to_bottom();
        self.resize_focus_handle.focus(window, cx);
        cx.notify();
    }

    fn forward_docked_hash_action(
        &mut self,
        action: DockedHashAction,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.resize_focus_handle.is_focused(window)
            || self.hash_window.mode() != Some(HashPresentationMode::Docked)
            || self.hash_rehost_pending.load(Ordering::Acquire)
            || self.restart_admission.is_fenced()
        {
            return false;
        }
        let HashWindowState::Docked {
            mount_generation,
            view,
        } = &self.hash_window
        else {
            return false;
        };
        let mount_generation = *mount_generation;
        let session_epoch = self
            .hash_session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .epoch();
        if self.hash_mount_generation.load(Ordering::Acquire) != mount_generation {
            return false;
        }

        view.update(cx, |tool, cx| {
            if tool.mount_generation != mount_generation
                || tool.session_epoch != session_epoch
                || !tool.is_current_mount()
            {
                return false;
            }
            match action {
                DockedHashAction::Calculate => tool.calculate_hash(cx),
                DockedHashAction::Dock => tool.request_rehost(HashPresentationMode::Docked, cx),
                DockedHashAction::Popout => tool.request_rehost(HashPresentationMode::Popout, cx),
                DockedHashAction::Close => tool.request_close(cx),
            }
            true
        })
    }

    fn docked_hash_resize_is_current(&self, payload: &DockedHashResizePayload) -> bool {
        let docked_mount_is_current = self.hash_window.mode() == Some(HashPresentationMode::Docked)
            && self.hash_window.mount_generation() == Some(payload.mount_generation);
        let active_session_epoch = self
            .hash_session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .epoch();
        Arc::ptr_eq(&self.hash_docked_resize, &payload.preview_extent)
            && docked_hash_resize_identity_matches(
                self.hash_mount_generation.load(Ordering::Acquire),
                payload.mount_generation,
                active_session_epoch,
                payload.session_epoch,
                docked_mount_is_current,
            )
            && self.hash_docked_preferred_size == payload.start_size
            && !self.hash_rehost_pending.load(Ordering::Acquire)
            && !self.restart_admission.is_fenced()
    }

    fn begin_docked_hash_pointer_resize(
        &mut self,
        payload: &DockedHashResizePayload,
        position: Point<Pixels>,
        window: &Window,
        window_padding: f32,
        cx: &mut Context<Self>,
    ) {
        if !self.docked_hash_resize_is_current(payload) {
            return;
        }
        let available = docked_hash_available_size(
            window.bounds().size.width.as_f32(),
            window.bounds().size.height.as_f32(),
            window_padding,
        );
        let visible_size = docked_hash_fit(payload.start_size, available);
        let mut preview = payload
            .preview_extent
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *preview = Some(DockedHashResize {
            mount_generation: payload.mount_generation,
            session_epoch: payload.session_epoch,
            start_size: payload.start_size,
            available_at_start: available,
            preview_size: payload.start_size,
            changed_width: false,
            changed_height: false,
            drag_started: false,
            origin: DockedHashResizeOrigin::Pointer {
                position,
                visible_size,
            },
        });
        drop(preview);
        cx.notify();
    }

    fn update_docked_hash_pointer_resize(
        &mut self,
        payload: &DockedHashResizePayload,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        if !self.docked_hash_resize_is_current(payload) {
            return;
        }
        let mut preview = payload
            .preview_extent
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(resize) = preview.as_mut().filter(|resize| {
            resize.mount_generation == payload.mount_generation
                && resize.session_epoch == payload.session_epoch
                && resize.start_size == payload.start_size
                && resize.drag_started
        }) else {
            return;
        };
        let DockedHashResizeOrigin::Pointer {
            position: origin,
            visible_size,
        } = resize.origin
        else {
            return;
        };
        let delta_x = position.x.as_f32() - origin.x.as_f32();
        let delta_y = position.y.as_f32() - origin.y.as_f32();
        let (preview_size, changed_width, changed_height) =
            pointer_docked_resize_preview(resize.start_size, visible_size, delta_x, delta_y);
        if resize.preview_size != preview_size
            || resize.changed_width != changed_width
            || resize.changed_height != changed_height
        {
            resize.preview_size = preview_size;
            resize.changed_width = changed_width;
            resize.changed_height = changed_height;
            drop(preview);
            cx.notify();
        }
    }

    fn preview_docked_hash_keyboard_resize(
        &mut self,
        payload: &DockedHashResizePayload,
        delta_x: f32,
        delta_y: f32,
        available: AppliedWindowSize,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.docked_hash_resize_is_current(payload) {
            return false;
        }
        let mut preview = payload
            .preview_extent
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match preview.as_mut() {
            Some(resize)
                if resize.mount_generation == payload.mount_generation
                    && resize.session_epoch == payload.session_epoch
                    && resize.start_size == payload.start_size
                    && matches!(resize.origin, DockedHashResizeOrigin::Keyboard) =>
            {
                resize.preview_size =
                    keyboard_docked_resize_preview(resize.preview_size, delta_x, delta_y);
                resize.changed_width |= delta_x != 0.0;
                resize.changed_height |= delta_y != 0.0;
            }
            Some(_) => return false,
            None => {
                let preview_size =
                    keyboard_docked_resize_preview(payload.start_size, delta_x, delta_y);
                *preview = Some(DockedHashResize {
                    mount_generation: payload.mount_generation,
                    session_epoch: payload.session_epoch,
                    start_size: payload.start_size,
                    available_at_start: available,
                    preview_size,
                    changed_width: delta_x != 0.0,
                    changed_height: delta_y != 0.0,
                    drag_started: false,
                    origin: DockedHashResizeOrigin::Keyboard,
                });
            }
        }
        drop(preview);
        cx.notify();
        true
    }

    fn cancel_docked_hash_resize(&mut self, payload: &DockedHashResizePayload) -> bool {
        if !Arc::ptr_eq(&self.hash_docked_resize, &payload.preview_extent) {
            return false;
        }
        let mut preview = payload
            .preview_extent
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let matches = preview.as_ref().is_some_and(|resize| {
            resize.mount_generation == payload.mount_generation
                && resize.session_epoch == payload.session_epoch
                && resize.start_size == payload.start_size
        });
        if matches {
            preview.take();
        }
        matches
    }

    fn cancel_docked_hash_preview(&mut self) -> bool {
        let mut preview = self
            .hash_docked_resize
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        preview.take().is_some()
    }

    fn cancel_docked_hash_resize_for_mount(&mut self, mount_generation: u64, session_epoch: u64) {
        let mut preview = self
            .hash_docked_resize
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if preview.as_ref().is_some_and(|resize| {
            resize.mount_generation == mount_generation && resize.session_epoch == session_epoch
        }) {
            preview.take();
        }
    }

    fn commit_docked_hash_resize(
        &mut self,
        payload: &DockedHashResizePayload,
        keyboard: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.docked_hash_resize_is_current(payload) {
            return false;
        }
        let resize = {
            let mut preview = payload
                .preview_extent
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let matches = preview.as_ref().is_some_and(|resize| {
                resize.mount_generation == payload.mount_generation
                    && resize.session_epoch == payload.session_epoch
                    && resize.start_size == payload.start_size
                    && matches!(
                        (keyboard, resize.origin),
                        (true, DockedHashResizeOrigin::Keyboard)
                            | (false, DockedHashResizeOrigin::Pointer { .. })
                    )
            });
            matches.then(|| preview.take()).flatten()
        };
        let Some(resize) = resize else {
            return false;
        };
        let Some(size) = docked_resize_commit_size(resize) else {
            cx.notify();
            return true;
        };
        let accepted = self
            .utility_size_store
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .set_preferred_size(self.hash_tool_key, size);
        match accepted {
            Ok(_revision) => {
                self.hash_docked_preferred_size = size;
                self.start_utility_size_ack_poll(cx);
            }
            Err(error) => {
                self.set_hash_size_write_error(
                    Some(format!(
                        "Could not save the Hash Generator docked size. Resize it again to retry. {error}"
                    )),
                    cx,
                );
            }
        }
        cx.notify();
        true
    }

    fn handle_docked_hash_resize_key(
        &mut self,
        payload: &DockedHashResizePayload,
        key: &str,
        available: AppliedWindowSize,
        cx: &mut Context<Self>,
    ) -> bool {
        match key {
            "left" => self.preview_docked_hash_keyboard_resize(
                payload,
                -HASH_DOCK_KEYBOARD_RESIZE_STEP,
                0.0,
                available,
                cx,
            ),
            "right" => self.preview_docked_hash_keyboard_resize(
                payload,
                HASH_DOCK_KEYBOARD_RESIZE_STEP,
                0.0,
                available,
                cx,
            ),
            "up" => self.preview_docked_hash_keyboard_resize(
                payload,
                0.0,
                -HASH_DOCK_KEYBOARD_RESIZE_STEP,
                available,
                cx,
            ),
            "down" => self.preview_docked_hash_keyboard_resize(
                payload,
                0.0,
                HASH_DOCK_KEYBOARD_RESIZE_STEP,
                available,
                cx,
            ),
            "enter" | "return" => self.commit_docked_hash_resize(payload, true, cx),
            "escape" => {
                let cancelled = self.cancel_docked_hash_resize(payload);
                if cancelled {
                    cx.notify();
                }
                cancelled
            }
            _ => false,
        }
    }

    fn docked_hash_resize_preview(
        &self,
        mount_generation: u64,
        session_epoch: u64,
    ) -> Option<PreferredWindowSize> {
        self.hash_docked_resize
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .filter(|resize| {
                resize.mount_generation == mount_generation
                    && resize.session_epoch == session_epoch
                    && self.hash_docked_preferred_size == resize.start_size
            })
            .map(|resize| resize.preview_size)
    }

    fn drain_hash_native_geometry(&mut self, cx: &mut Context<Self>, retire_binding: bool) {
        let Some(view) = self.hash_window.view().cloned() else {
            return;
        };
        let sample = view.update(cx, |tool, _| {
            let sample = tool.take_native_geometry();
            if retire_binding {
                tool.detach_native_geometry();
            }
            sample
        });
        let Some((sample, factory_size)) = sample else {
            return;
        };
        let key = self.hash_tool_key;
        let mut store = self
            .utility_size_store
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let factory_size = store
            .effective_native_geometry(key)
            .is_none()
            .then_some(factory_size)
            .flatten();
        match store.observe_native_geometry(key, sample, factory_size) {
            Ok(Some(_revision)) => {
                drop(store);
                self.start_utility_size_ack_poll(cx);
            }
            Ok(None) => {}
            Err(error) => {
                drop(store);
                self.set_hash_size_write_error(
                    Some(format!(
                        "Could not save Hash Generator window geometry. Move or resize it again to retry. {error}"
                    )),
                    cx,
                );
            }
        }
    }

    fn poll_utility_size_writer(&mut self, cx: &mut Context<Self>) -> bool {
        let (pending, poll_error, write_failure, acknowledged) = {
            let mut store = self
                .utility_size_store
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let poll = store.poll();
            let poll_error = poll.as_ref().err().cloned();
            let acknowledged = poll.as_ref().is_ok_and(|poll| !poll.accepted.is_empty());
            let write_failure = store
                .take_new_write_failures()
                .into_iter()
                .find(|failure| failure.key == self.hash_tool_key)
                .map(|failure| failure.message);
            (
                store.has_pending_acknowledgement(),
                poll_error,
                write_failure,
                acknowledged,
            )
        };
        if let Some(error) = poll_error {
            self.set_hash_size_write_error(
                Some(format!(
                    "Could not publish the Hash Generator geometry save: {error}"
                )),
                cx,
            );
        } else if let Some(error) = write_failure {
            self.set_hash_size_write_error(
                Some(format!(
                    "Could not save Hash Generator window geometry. Move or resize it again to retry. {error}"
                )),
                cx,
            );
        } else if acknowledged {
            self.set_hash_size_write_error(None, cx);
        }
        if !pending {
            self.utility_size_polling.store(false, Ordering::Release);
        }
        pending
    }

    fn set_hash_size_write_error(&mut self, message: Option<String>, cx: &mut Context<Self>) {
        self.hash_size_write_error = message.clone();
        if let Some(view) = self.hash_window.view().cloned() {
            view.update(cx, |tool, cx| {
                tool.size_write_error = message;
                cx.notify();
            });
        }
        cx.notify();
    }

    fn start_utility_size_ack_poll(&self, cx: &mut Context<Self>) {
        if !self
            .utility_size_polling
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            return;
        }
        let root = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(35))
                    .await;
                let pending = root.update(cx, |root, cx| root.poll_utility_size_writer(cx));
                match pending {
                    Ok(true) => {}
                    _ => break,
                }
            }
        })
        .detach();
    }

    fn restored_hash_geometry(&self) -> Option<NativeWindowGeometry> {
        self.utility_size_store
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .effective_native_geometry(self.hash_tool_key)
    }

    fn schedule_hash_popout_setup(
        &self,
        native_window: NativeWindow,
        view: Entity<HashGeneratorWindow>,
        mount_generation: u64,
        session_epoch: u64,
        operation: EffectPermit,
        cx: &mut Context<Self>,
    ) {
        let target = checked_geometry_target(native_window);
        let geometry = self.restored_hash_geometry();
        let active_mount_generation = Arc::clone(&self.hash_mount_generation);
        let session = Arc::clone(&self.hash_session);
        let restart_admission = Arc::clone(&self.restart_admission);
        let root = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let _operation = operation;
            let current = || {
                active_mount_generation.load(Ordering::Acquire) == mount_generation
                    && session
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .epoch()
                        == session_epoch
                    && !restart_admission.is_fenced()
            };
            if !current() {
                return;
            }

            // This foreground task runs on GPUI's owner thread. Do not wrap
            // SetWindowPos in AsyncApp::update: WM_DPICHANGED may synchronously
            // call back into GPUI while the App RefCell is borrowed.
            let placement_error = target
                .ok_or_else(|| "the Hash Generator HWND could not be checked".to_owned())
                .and_then(|target| {
                    restore_native_window_geometry(target, geometry, (420, 320), 1.0).map(|_| ())
                })
                .err()
                .map(|error| {
                    format!("Could not restore the Hash Generator popout position: {error}")
                });
            if !current() {
                return;
            }

            let view_error = view.update(cx, |tool, cx| {
                if !tool.is_current_mount() || !current() {
                    return None;
                }
                let attach_error = tool
                    .attach_native_geometry()
                    .err()
                    .map(|error| format!("Native move/resize persistence is unavailable: {error}"));
                let mut errors = Vec::new();
                if let Some(error) = tool.native_geometry_error.take() {
                    errors.push(error);
                }
                if let Some(error) = placement_error.clone() {
                    errors.push(error);
                }
                if let Some(error) = attach_error {
                    errors.push(error);
                }
                let message = (!errors.is_empty()).then(|| errors.join(" "));
                tool.native_geometry_error = message.clone();
                cx.notify();
                message
            });
            if let Some(error) = view_error {
                let _ = root.update(cx, |root, cx| {
                    if root.hash_window.mount_generation() == Some(mount_generation)
                        && root
                            .hash_session
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .epoch()
                            == session_epoch
                    {
                        root.hash_native_geometry_error = Some(error);
                        cx.notify();
                    }
                });
            }
        })
        .detach();
    }

    fn capture_restart_snapshot(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<RestartSnapshot, String> {
        if self
            .hash_docked_resize
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .is_some_and(|resize| {
                self.hash_window.mode() == Some(HashPresentationMode::Docked)
                    && self.hash_window.mount_generation() == Some(resize.mount_generation)
                    && self.hash_mount_generation.load(Ordering::Acquire) == resize.mount_generation
                    && self
                        .hash_session
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .epoch()
                        == resize.session_epoch
            })
        {
            return Err(
                "A docked Hash Generator resize is being previewed. Press Enter to save the size or Escape to discard it, then retry the egui restart."
                    .to_owned(),
            );
        }
        self.drain_hash_native_geometry(cx, false);
        let _ = self.poll_utility_size_writer(cx);
        let size_status = self
            .utility_size_store
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .status(self.hash_tool_key);
        if size_status.pending_revision.is_some() {
            self.start_utility_size_ack_poll(cx);
            return Err(
                "The Hash Generator window geometry is still being saved. Wait for the settings write to finish, then retry the egui restart."
                    .to_owned(),
            );
        }
        if let Some((_, error)) = size_status.failure {
            return Err(format!(
                "The Hash Generator window geometry could not be saved: {error}. Move or resize the popout to retry the save before restarting."
            ));
        }
        if let Some(message) = hash_clipboard_close_message(self.hash_clipboard_pending.kind()) {
            return Err(format!("{message} Then retry the egui restart."));
        }
        self.hash_retiring_windows
            .retain(|retiring| retiring.native_window.is_none_or(native_window_is_valid));
        if self.hash_rehost_pending.load(Ordering::Acquire) {
            return Err(
                "Hash Generator is changing presentation. Wait for docking or popout to finish, then retry the egui restart."
                    .to_owned(),
            );
        }
        if self.utility_presentations.has_pending() {
            return Err(
                "The Hash Generator presentation preference is still being saved. Wait for the save to finish, then retry the egui restart."
                    .to_owned(),
            );
        }
        if !self.hash_retiring_windows.is_empty() {
            return Err(
                "A previous Hash Generator popout has not retired yet. Wait for its window to close, then retry the egui restart."
                    .to_owned(),
            );
        }
        if self.coordinate_hud_probe_pending {
            return Err(
                "Coordinate HUD is still changing state. Wait for it to finish, then retry the egui restart."
                    .to_owned(),
            );
        }
        if self.hash_file_picker_pending.load(Ordering::Acquire) {
            return Err(
                "The Hash Generator file picker is still open. Finish or cancel it, then retry the egui restart."
                    .to_owned(),
            );
        }

        let mut child_windows = Vec::new();
        let mut native_windows = self.native_window.into_iter().collect::<Vec<_>>();
        let utility = match &self.secondary {
            SecondaryWindowState::Closed => None,
            SecondaryWindowState::OpenPending => {
                return Err(
                    "The retained utility window is still opening. Wait for it to finish, then retry the egui restart."
                        .to_owned(),
                );
            }
            SecondaryWindowState::Live { window, view } => {
                let handle = *window;
                let (text, native) =
                    update_kit_window_content(handle, view, cx, |utility, _, cx| {
                        (
                            utility.query.read(cx).value().to_string(),
                            utility.native_window,
                        )
                    })
                    .map_err(|error| {
                        format!("Could not inspect the retained utility draft: {error}")
                    })?;
                child_windows.push(handle);
                if let Some(native) = native {
                    native_windows.push(native);
                } else if cfg!(windows) {
                    return Err(
                        "The retained utility window has no checked Windows handle; close it and retry the egui restart."
                            .to_owned(),
                    );
                }
                Some(text)
            }
        };

        let (mut hash_draft, hash_work) = {
            let session = self
                .hash_session
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let work = if session.is_running() {
                HashWork::Active
            } else if session.has_job() {
                HashWork::Retiring
            } else {
                HashWork::Idle
            };
            (HashDraft::from_session(&session), work)
        };
        let hash_window_open = self.hash_window.mode().is_some();
        match &self.hash_window {
            HashWindowState::Closed => {}
            HashWindowState::Docked { view, .. } => {
                hash_draft = view.update(cx, |tool, cx| tool.draft(cx));
            }
            HashWindowState::Popout { window, view, .. } => {
                let handle = *window;
                let (draft, native) = update_kit_window_content(handle, view, cx, |tool, _, cx| {
                    (tool.draft(cx), tool.native_window)
                })
                .map_err(|error| format!("Could not inspect the Hash Generator window: {error}"))?;
                hash_draft = draft;
                child_windows.push(handle);
                if let Some(native) = native {
                    native_windows.push(native);
                } else if cfg!(windows) {
                    return Err(
                        "The Hash Generator window has no checked Windows handle; close it and retry the egui restart."
                            .to_owned(),
                    );
                }
            }
        }

        if cfg!(windows) {
            if self.native_window.is_none() {
                return Err(
                    "The GPUI root has no checked Windows handle; restart is unavailable until the window is restored."
                        .to_owned(),
                );
            }
            if native_windows
                .iter()
                .any(|window| !native_window_is_valid(*window))
            {
                return Err(
                    "A GPUI window handle became stale. Close any utility windows and retry the egui restart."
                        .to_owned(),
                );
            }
        }

        Ok(RestartSnapshot {
            drafts: RestartDrafts {
                search: self.query.read(cx).value().to_string(),
                utility,
                hash: hash_draft,
                hash_window_open,
            },
            hash_work,
            child_windows,
            native_windows,
        })
    }

    fn open_restart_confirmation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(context) = self.restart_context.clone() else {
            self.restart_status = Some(format!(
                "Could not capture the executable path and working directory before restart. Close and relaunch manually with {}.",
                Frontend::Egui.selector_argument()
            ));
            cx.notify();
            return;
        };
        if !self.restart_admission.begin_confirmation() {
            self.restart_status = Some(
                "An egui restart has already been accepted or is awaiting cancellation.".to_owned(),
            );
            cx.notify();
            return;
        }

        let snapshot = match self.capture_restart_snapshot(cx) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                self.restart_admission.cancel_confirmation();
                self.restart_status = Some(error);
                cx.notify();
                return;
            }
        };
        if snapshot.hash_work != HashWork::Idle {
            self.restart_admission.cancel_confirmation();
            self.restart_status = Some(restart_blocker_message(RestartBlocker::HashWork(
                snapshot.hash_work,
            )));
            cx.notify();
            return;
        }
        let pending = self.restart_admission.pending_effects();
        if pending != 0 {
            self.restart_admission.cancel_confirmation();
            self.restart_status = Some(restart_blocker_message(RestartBlocker::PendingEffects(
                pending,
            )));
            cx.notify();
            return;
        }

        let description = describe_restart_drafts(&snapshot.drafts);
        let expected_drafts = snapshot.drafts;
        let root = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let root_for_cancel = root.clone();
            let root_for_close = root.clone();
            let root_for_accept = root.clone();
            let context_for_accept = context.clone();
            let expected_drafts_for_accept = expected_drafts.clone();
            alert
                .title(Label::new("Restart in egui?"))
                .description(Label::new(description.clone()))
                .confirm()
                .ok_text("Restart in egui")
                .cancel_text("Cancel")
                .on_cancel(move |_, _, app| {
                    let _ = root_for_cancel.update(app, |root, cx| {
                        root.restart_admission.cancel_confirmation();
                        root.restart_status = None;
                        cx.notify();
                    });
                    true
                })
                .on_close(move |_, _, app| {
                    let _ = root_for_close.update(app, |root, cx| {
                        root.restart_admission.cancel_confirmation();
                        cx.notify();
                    });
                })
                .on_ok(move |_, _, app| {
                    let root_for_task = root_for_accept.clone();
                    let accepted = root_for_accept.update(app, |root, cx| {
                        root.accept_restart(
                            context_for_accept.clone(),
                            &expected_drafts_for_accept,
                            cx,
                        )
                    });
                    match accepted {
                        Ok(Ok(accepted)) => {
                            app.spawn(async move |cx| {
                                Self::complete_restart(root_for_task, accepted, cx).await;
                            })
                            .detach();
                        }
                        Ok(Err(error)) => {
                            let _ = root_for_accept.update(app, |root, cx| {
                                root.restart_admission.cancel_confirmation();
                                root.restart_status = Some(error);
                                cx.notify();
                            });
                        }
                        Err(error) => {
                            tracing::error!(%error, "could not validate the GPUI restart request");
                        }
                    }
                    true
                })
        });
    }

    fn accept_restart(
        &mut self,
        context: RestartContext,
        expected_drafts: &RestartDrafts,
        cx: &mut Context<Self>,
    ) -> Result<AcceptedRestart, String> {
        let snapshot = self.capture_restart_snapshot(cx)?;
        if &snapshot.drafts != expected_drafts {
            return Err(
                "A GPUI editor changed after the confirmation opened. Review its current draft and confirm the egui restart again."
                    .to_owned(),
            );
        }
        let root_window = self.root_window.ok_or_else(|| {
            "The GPUI root window handle is unavailable; no egui process was started.".to_owned()
        })?;
        self.restart_admission
            .try_accept(snapshot.hash_work)
            .map_err(restart_blocker_message)?;
        self.drain_hash_native_geometry(cx, true);

        self.query
            .update(cx, |input, cx| input.set_readonly(true, cx));
        if let SecondaryWindowState::Live { view, .. } = &self.secondary {
            let view = view.clone();
            view.update(cx, |utility, cx| {
                utility
                    .query
                    .update(cx, |input, cx| input.set_readonly(true, cx));
            });
        }
        if let Some(view) = self.hash_window.view() {
            let view = view.clone();
            view.update(cx, |tool, cx| tool.set_editors_readonly(true, cx));
        }

        {
            let mut session = self
                .hash_session
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if session.file_path() != snapshot.drafts.hash.file_path.as_deref() {
                session.set_file_path(snapshot.drafts.hash.file_path.clone());
            }
            session.set_text(snapshot.drafts.hash.text.clone());
            session.set_expected(snapshot.drafts.hash.expected.clone());
            session.set_mode(snapshot.drafts.hash.mode);
            session.set_algorithm(snapshot.drafts.hash.algorithm);
        }
        self.root_visibility.active.store(false, Ordering::Release);
        self.root_visibility
            .request_revision
            .fetch_add(1, Ordering::AcqRel);
        self.coordinate_hud_probe_closing
            .store(true, Ordering::Release);
        self.coordinate_hud_probe_status = "Closing GPUI services for the egui restart…".to_owned();
        self.hash_wake_signal.close();
        self.hash_session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .shutdown();
        self.hash_window_revision.fetch_add(1, Ordering::AcqRel);
        self.secondary_revision.fetch_add(1, Ordering::AcqRel);
        self.restart_status = None;
        cx.notify();

        Ok(AcceptedRestart {
            context,
            root_window,
            child_windows: snapshot.child_windows,
            native_windows: snapshot.native_windows,
            listener_slot: Arc::clone(&self.listener_slot),
            coordinate_controller: Arc::clone(&self.coordinate_hud_probe_controller),
            coordinate_shutdown_complete: Arc::clone(&self.coordinate_shutdown_complete),
            restart_retiring: Arc::clone(&self.restart_retiring),
            restart_exit: Rc::clone(&self.restart_exit),
            restart_failure: Rc::clone(&self.restart_failure),
        })
    }

    async fn complete_restart(
        root: WeakEntity<Self>,
        accepted: AcceptedRestart,
        cx: &mut AsyncApp,
    ) {
        let listener_slot = Arc::clone(&accepted.listener_slot);
        let coordinate_controller = Arc::clone(&accepted.coordinate_controller);
        let coordinate_shutdown_complete = Arc::clone(&accepted.coordinate_shutdown_complete);
        let teardown = cx
            .background_executor()
            .spawn(async move {
                let listener_stop = match listener_slot.lock() {
                    Ok(mut slot) => slot
                        .take()
                        .map_or(Ok(()), |mut listener| listener.stop_checked()),
                    Err(_) => Err("the hotkey listener owner lock was poisoned".to_owned()),
                };
                listener_stop?;
                coordinate_controller
                    .lock()
                    .map_err(|_| "the Coordinate HUD controller lock was poisoned".to_owned())?
                    .shutdown()?;
                coordinate_shutdown_complete.store(true, Ordering::Release);
                Ok::<(), String>(())
            })
            .await;

        if let Err(error) = teardown {
            let _ = root.update(cx, |root, cx| {
                root.restart_status = Some(format!(
                    "Egui restart stopped because GPUI teardown failed: {error}. No new process was started. GPUI remains open; copy any unsaved text, then close and rerun with {}.",
                    Frontend::Egui.selector_argument()
                ));
                cx.notify();
            });
            return;
        }

        let child_windows = accepted.child_windows;
        let child_native_windows = accepted
            .native_windows
            .iter()
            .skip(1)
            .copied()
            .collect::<Vec<_>>();
        let close_children_error = RefCell::new(None::<String>);
        cx.update(|app| {
            for window in child_windows {
                if let Err(error) = window.update(app, |_, window, _| window.remove_window()) {
                    close_children_error
                        .borrow_mut()
                        .replace(format!("could not retire a GPUI utility window: {error}"));
                    break;
                }
            }
        });
        let close_children = close_children_error.into_inner().map_or(Ok(()), Err);
        if let Err(error) = close_children {
            let _ = root.update(cx, |root, cx| {
                root.restart_status = Some(format!(
                    "Egui restart stopped because a GPUI utility window could not be closed: {error}. No new process was started. GPUI remains open; copy any unsaved text, then close and rerun with {}.",
                    Frontend::Egui.selector_argument()
                ));
                cx.notify();
            });
            return;
        }
        if !wait_for_native_retirement(cx, &child_native_windows).await {
            let _ = root.update(cx, |root, cx| {
                root.restart_status = Some(format!(
                    "Egui restart stopped because a utility window did not retire. No new process was started. GPUI remains open; copy any unsaved text, then close and rerun with {}.",
                    Frontend::Egui.selector_argument()
                ));
                cx.notify();
            });
            return;
        }

        let root_window = accepted.root_window;
        let close_root_error = RefCell::new(None::<String>);
        cx.update(|app| {
            app.set_quit_mode(QuitMode::Explicit);
            accepted.restart_retiring.store(true, Ordering::Release);
            if let Err(error) = root_window.update(app, |_, window, _| window.remove_window()) {
                close_root_error
                    .borrow_mut()
                    .replace(format!("could not retire the GPUI root window: {error}"));
            }
        });
        let close_root = close_root_error.into_inner().map_or(Ok(()), Err);
        if let Err(error) = close_root {
            accepted.restart_failure.borrow_mut().replace(format!(
                "The GPUI root window could not be closed during restart: {error}. No egui process was started; manually close Multi Launcher and rerun with {}.",
                Frontend::Egui.selector_argument()
            ));
            let _ = cx.update(|app| app.quit());
            return;
        }

        let root_native_window = accepted.native_windows.first().copied();
        let root_retired = match root_native_window {
            Some(window) => wait_for_native_retirement(cx, &[window]).await,
            None => true,
        };
        if !root_retired {
            accepted.restart_failure.borrow_mut().replace(format!(
                "The GPUI root window did not retire before restart. No egui process was started; manually close Multi Launcher and rerun with {}.",
                Frontend::Egui.selector_argument()
            ));
            let _ = cx.update(|app| app.quit());
            return;
        }

        accepted
            .restart_exit
            .borrow_mut()
            .replace((accepted.context, accepted.native_windows));
        let _ = cx.update(|app| app.quit());
    }

    fn run_hash_generator_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(operation) = self.restart_admission.admit() else {
            self.restart_status =
                Some("GPUI is closing; new utility operations are disabled.".to_owned());
            cx.notify();
            return;
        };
        let outcome = handle_hash_generator(&HashGeneratorCommand::Open, || {
            self.open_hash_generator_window(operation, window, cx)
        });
        self.hash_open_error = outcome
            .err()
            .map(|error| format!("Could not open Hash Generator: {error}"));
        cx.notify();
    }

    fn refresh_launcher_results(&mut self, query: String) {
        let hash_generator = multi_launcher::plugins::hash_generator::HashGeneratorPlugin;
        let provider_actions = multi_launcher::plugin::search_filtered_plugins(
            [&hash_generator as &dyn multi_launcher::plugin::Plugin],
            &query,
            self.enabled_plugins.as_ref(),
            self.enabled_capabilities.as_ref(),
        );
        let actions = self.search_catalog.compose(&query, provider_actions);
        self.launcher_results.replace(query, actions);
        self.fresh_search_enter = None;
        // Query replacement can change the list height while the root is
        // scrolled near the end. Keep the Search field at the leading edge
        // whenever a tall result set overflows the shared root scroller.
        self.root_scroll.scroll_to_top_of_item(0);
    }

    fn record_search_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        self.fresh_search_enter = fresh_search_enter_marker(
            self.search_input_is_live(window, cx),
            event.keystroke.key.as_str(),
            event.keystroke.modifiers,
            event.is_held,
            self.launcher_results.generation,
            self.query.read(cx).value().to_string(),
        );
    }

    fn record_search_key_up(&mut self, event: &KeyUpEvent) {
        if event.keystroke.key == "enter" {
            self.fresh_search_enter = None;
        }
    }

    fn search_input_operation(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<EffectPermit> {
        if !self.search_input_is_live(window, cx) {
            return None;
        }
        self.restart_admission.admit()
    }

    fn search_input_is_live(&self, window: &Window, cx: &mut Context<Self>) -> bool {
        let root_window_matches = self
            .root_window
            .is_some_and(|root| root.window_id() == window.window_handle().window_id());
        search_input_is_current(
            self.root_visibility.visible.load(Ordering::Acquire),
            self.root_visibility.active.load(Ordering::Acquire),
            root_window_matches,
            self.query.focus_handle(cx).is_focused(window),
            window.is_window_active(),
        )
    }

    fn navigate_search_history(
        &mut self,
        direction: QueryHistoryDirection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(_operation) = self.search_input_operation(window, cx) else {
            return false;
        };
        let current_query = self.query.read(cx).value().to_string();
        self.query_history.synchronize(&current_query);
        let next_query = match direction {
            QueryHistoryDirection::Older => self.query_history.older(&current_query, || {
                multi_launcher::history::with_history(|entries| {
                    single_line_history_queries(entries.iter().map(|entry| entry.query.as_str()))
                })
                .unwrap_or_default()
            }),
            QueryHistoryDirection::Newer => self.query_history.newer(&current_query),
        };
        if let Some(next_query) = next_query {
            self.query.update(cx, |state, cx| {
                state.replace_all(next_query, window, cx);
            });
        }
        true
    }

    fn move_search_selection_down(&mut self, window: &Window, cx: &mut Context<Self>) -> bool {
        let Some(_operation) = self.search_input_operation(window, cx) else {
            return false;
        };
        self.launcher_results.select_down();
        cx.notify();
        true
    }

    fn move_search_selection_up(&mut self, window: &Window, cx: &mut Context<Self>) -> bool {
        let Some(_operation) = self.search_input_operation(window, cx) else {
            return false;
        };
        self.launcher_results.select_up();
        cx.notify();
        true
    }

    fn activate_launcher_result(
        &mut self,
        generation: u64,
        index: usize,
        source: ActivationSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let query = self.query.read(cx).value().to_string();
        let Some(action) = self
            .launcher_results
            .resolve_action(generation, index, &query)
        else {
            return;
        };
        let invocation = match parse_hash_search_invocation(action, source) {
            Ok(invocation) => invocation,
            Err(error) => {
                self.hash_open_error = Some(format!("Could not open Hash Generator: {error}"));
                cx.notify();
                return;
            }
        };
        let Some(operation) = self.restart_admission.admit() else {
            self.hash_open_error =
                Some("GPUI is closing; new utility operations are disabled.".to_owned());
            cx.notify();
            return;
        };
        let outcome = dispatch_hash_search_invocation(invocation, || {
            self.open_hash_generator_window(operation, window, cx)
        });
        if outcome.is_ok() {
            self.query_history.reset();
        }
        self.hash_open_error = outcome
            .err()
            .map(|error| format!("Could not open Hash Generator: {error}"));
        // Hash Open preserves the launcher query and results. A successful
        // activation retires transient traversal without writing query history.
        cx.notify();
    }

    fn next_hash_mount_generation(&mut self) -> u64 {
        self.hash_mount_counter = self.hash_mount_counter.wrapping_add(1).max(1);
        self.hash_mount_counter
    }

    fn notify_hash_mount(&self, cx: &mut Context<Self>) {
        if let Some(view) = self.hash_window.view().cloned() {
            view.update(cx, |_, cx| cx.notify());
        }
        cx.notify();
    }

    fn observe_hash_popout_close(
        &mut self,
        window: AnyWindowHandle,
        native_window: Option<NativeWindow>,
        mount_generation: u64,
        session_epoch: u64,
        cx: &mut Context<Self>,
    ) -> Subscription {
        let window_id = window.window_id();
        let root = cx.entity().downgrade();
        let active_mount_generation = Arc::clone(&self.hash_mount_generation);
        let session = Arc::clone(&self.hash_session);
        let restart_admission = Arc::clone(&self.restart_admission);
        let hash_window_revision = Arc::clone(&self.hash_window_revision);
        cx.on_window_closed(move |cx, closed| {
            if closed != window_id {
                return;
            }
            let Some(operation) = restart_admission.admit() else {
                return;
            };
            let active_mount_generation = Arc::clone(&active_mount_generation);
            let session = Arc::clone(&session);
            let hash_window_revision = Arc::clone(&hash_window_revision);
            let root_for_update = root.clone();
            let root_for_retirement = root.clone();
            let _ = root_for_update.update(cx, move |root, cx| {
                root.hash_window_close_subscriptions
                    .retain(|(handle, _)| handle.window_id() != closed);
                let is_active_mount = matches!(
                    &root.hash_window,
                    HashWindowState::Popout {
                        mount_generation: active_generation,
                        window,
                        ..
                    } if *active_generation == mount_generation
                        && window.window_id() == closed
                        && active_mount_generation.load(Ordering::Acquire) == mount_generation
                );
                if is_active_mount {
                    root.drain_hash_native_geometry(cx, true);
                }
                let mut session = session
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if is_active_mount && session.epoch() == session_epoch {
                    root.hash_retiring_windows.push(RetiringHashWindow {
                        window,
                        native_window,
                    });
                    session.end();
                    active_mount_generation.store(0, Ordering::Release);
                    root.hash_window = HashWindowState::Closed;
                    root.hash_rehost_pending.store(false, Ordering::Release);
                    root.hash_rehost_revision.fetch_add(1, Ordering::AcqRel);
                    hash_window_revision.fetch_add(1, Ordering::AcqRel);
                    let root_for_retirement = root_for_retirement.clone();
                    cx.spawn(async move |_, cx| {
                        let _operation = operation;
                        let retired =
                            wait_for_hash_window_retirement(cx, native_window).await;
                        let _ = root_for_retirement.update(cx, |root, cx| {
                            if retired {
                                root.hash_retiring_windows
                                    .retain(|retiring| retiring.window.window_id() != window_id);
                            } else {
                                let message = "The Hash Generator window closed, but Windows has not retired its native handle yet.".to_owned();
                                root.hash_presentation_error = Some(message.clone());
                                *root.hash_presentation_feedback
                                    .lock()
                                    .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(message);
                            }
                            cx.notify();
                        });
                        if retired {
                            restore_root_search_focus_after_hash_close(
                                root_for_retirement.clone(),
                                cx,
                            )
                            .await;
                        }
                    })
                    .detach();
                }
                cx.notify();
            });
        })
    }

    fn open_hash_generator_window(
        &mut self,
        operation: EffectPermit,
        root_window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        match &self.hash_window {
            HashWindowState::Docked { view, .. } => {
                let view = view.clone();
                view.update(cx, |tool, cx| tool.focus_editable(root_window, cx));
                return Ok(());
            }
            HashWindowState::Popout { window, view, .. } => {
                let window = *window;
                let view = view.clone();
                let expected_foreground = current_foreground_identity();
                let revision = self
                    .hash_window_revision
                    .fetch_add(1, Ordering::AcqRel)
                    .wrapping_add(1)
                    .max(1);
                match update_kit_window_content(window, &view, cx, |tool, window, cx| {
                    tool.focus_editable(window, cx);
                    tool.native_window
                }) {
                    Ok(Some(native_window)) => {
                        let revision_guard = Arc::clone(&self.hash_window_revision);
                        cx.spawn(async move |_, cx| {
                            let _operation = operation;
                            cx.background_executor()
                                .spawn(async move {
                                    activate_for_secondary_window(
                                        &revision_guard,
                                        revision,
                                        native_window,
                                        expected_foreground,
                                    );
                                })
                                .await;
                        })
                        .detach();
                        return Ok(());
                    }
                    Ok(None) if !cfg!(windows) => return Ok(()),
                    Ok(None) => {
                        return Err(
                            "The Hash Generator window has no checked Windows handle to activate."
                                .to_owned(),
                        );
                    }
                    Err(error) => {
                        return Err(format!("Could not focus the existing Hash window: {error}"));
                    }
                }
            }
            HashWindowState::Closed => {}
        }

        self.hash_session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .begin(None);
        self.hash_native_geometry_error = None;
        let session_epoch = self
            .hash_session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .epoch();
        let initial_draft = {
            let session = self
                .hash_session
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            HashDraft::from_session(&session)
        };
        let mount_generation = self.next_hash_mount_generation();
        let mode = HashPresentationMode::from_saved(
            self.utility_presentations.preference(self.hash_tool_key),
        );
        let root = cx.entity().downgrade();
        let mount_config = HashMountConfig {
            session: Arc::clone(&self.hash_session),
            wake_signal: Arc::clone(&self.hash_wake_signal),
            file_picker_pending: Arc::clone(&self.hash_file_picker_pending),
            clipboard_pending: Arc::clone(&self.hash_clipboard_pending),
            restart_admission: Arc::clone(&self.restart_admission),
            mount_generation,
            session_epoch,
            active_mount_generation: Arc::clone(&self.hash_mount_generation),
            rehost_pending: Arc::clone(&self.hash_rehost_pending),
            rehost_revision: Arc::clone(&self.hash_rehost_revision),
            presentation_feedback: Arc::clone(&self.hash_presentation_feedback),
            native_geometry_sequence: Arc::clone(&self.hash_geometry_sequence),
            hash_tool_key: self.hash_tool_key,
            size_write_error: self.hash_size_write_error.clone(),
            root,
            mode,
        };
        let mut popout_setup = None;
        match mode {
            HashPresentationMode::Docked => {
                self.hash_mount_generation
                    .store(mount_generation, Ordering::Release);
                let view =
                    create_hash_generator_view(root_window, cx, initial_draft, None, mount_config);
                self.hash_window = HashWindowState::Docked {
                    mount_generation,
                    view,
                };
            }
            HashPresentationMode::Popout => {
                let options = hash_popout_window_options(cx);
                let (window, view) = match gpui_kit::open_window(options, cx, move |window, cx| {
                    let native_window = checked_native_window(window).ok().flatten();
                    create_hash_generator_view(
                        window,
                        cx,
                        initial_draft,
                        native_window,
                        mount_config,
                    )
                }) {
                    Ok(opened) => opened,
                    Err(error) => {
                        self.hash_session
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .end();
                        return Err(format!(
                            "Kit could not create the Hash Generator window: {error}"
                        ));
                    }
                };
                let native_window = view.read(cx).native_window;
                if cfg!(windows) && native_window.is_none() {
                    self.hash_retiring_windows.push(RetiringHashWindow {
                        window,
                        native_window,
                    });
                    let _ = window.update(cx, |_, window, _| window.remove_window());
                    self.hash_session
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .end();
                    return Err(
                        "The Hash Generator popout has no checked Windows handle. It was asked to close, but restart is blocked because Windows retirement cannot be confirmed."
                            .to_owned(),
                    );
                }
                let geometry_error = view.read(cx).native_geometry_error.clone();
                self.hash_native_geometry_error = geometry_error;
                self.hash_mount_generation
                    .store(mount_generation, Ordering::Release);
                let close_subscription = self.observe_hash_popout_close(
                    window,
                    native_window,
                    mount_generation,
                    session_epoch,
                    cx,
                );
                let view_for_setup = view.clone();
                self.hash_window = HashWindowState::Popout {
                    mount_generation,
                    window,
                    view,
                };
                self.hash_window_close_subscriptions
                    .push((window, close_subscription));
                popout_setup = native_window.map(|native_window| (native_window, view_for_setup));
            }
        }
        self.hash_wake_signal.notify();
        self.hash_open_error = None;
        if let Some((native_window, view)) = popout_setup {
            self.schedule_hash_popout_setup(
                native_window,
                view,
                mount_generation,
                session_epoch,
                operation,
                cx,
            );
        } else {
            drop(operation);
        }
        Ok(())
    }

    fn restore_hash_transition(
        &mut self,
        mount_generation: u64,
        session_epoch: u64,
        revision: u64,
        message: String,
        cx: &mut Context<Self>,
    ) {
        if self.hash_rehost_revision.load(Ordering::Acquire) != revision {
            return;
        }
        self.hash_rehost_pending.store(false, Ordering::Release);
        let still_current = self.hash_mount_generation.load(Ordering::Acquire) == mount_generation
            && self
                .hash_session
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .epoch()
                == session_epoch;
        if still_current && let Some(view) = self.hash_window.view().cloned() {
            view.update(cx, |tool, cx| {
                tool.set_editors_readonly(false, cx);
                cx.notify();
            });
        }
        self.hash_open_error = Some(message.clone());
        *self
            .hash_presentation_feedback
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(message);
        cx.notify();
    }

    fn start_hash_rehost(
        &mut self,
        target: HashPresentationMode,
        source_mount_generation: u64,
        session_epoch: u64,
        revision: u64,
        operation: EffectPermit,
        cx: &mut Context<Self>,
    ) {
        self.drain_hash_native_geometry(cx, false);
        if self.hash_rehost_revision.load(Ordering::Acquire) != revision {
            return;
        }
        if !self.hash_rehost_pending.load(Ordering::Acquire)
            || self.hash_mount_generation.load(Ordering::Acquire) != source_mount_generation
            || self.hash_window.mount_generation() != Some(source_mount_generation)
        {
            self.restore_hash_transition(
                source_mount_generation,
                session_epoch,
                revision,
                "Hash Generator changed before the requested presentation could start.".to_owned(),
                cx,
            );
            return;
        }
        self.cancel_docked_hash_resize_for_mount(source_mount_generation, session_epoch);
        let source_mode = self.hash_window.mode();
        if source_mode == Some(target) {
            self.hash_rehost_pending.store(false, Ordering::Release);
            if let Some(view) = self.hash_window.view().cloned() {
                view.update(cx, |tool, cx| {
                    tool.set_editors_readonly(false, cx);
                    cx.notify();
                });
            }
            return;
        }
        let session_is_current = self
            .hash_session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .epoch()
            == session_epoch;
        if !session_is_current {
            self.restore_hash_transition(
                source_mount_generation,
                session_epoch,
                revision,
                "Hash Generator session changed before rehosting. Reopen it and try again."
                    .to_owned(),
                cx,
            );
            return;
        }

        let Some(root_window) = self.root_window else {
            self.restore_hash_transition(
                source_mount_generation,
                session_epoch,
                revision,
                "The GPUI root window is unavailable; Hash Generator could not be docked or popped out."
                    .to_owned(),
                cx,
            );
            return;
        };
        let initial_draft = {
            let session = self
                .hash_session
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            HashDraft::from_session(&session)
        };
        let destination_mount_generation = self.next_hash_mount_generation();
        let destination_session_epoch = session_epoch;
        let root = cx.entity().downgrade();
        let session = Arc::clone(&self.hash_session);
        let wake_signal = Arc::clone(&self.hash_wake_signal);
        let file_picker_pending = Arc::clone(&self.hash_file_picker_pending);
        let clipboard_pending = Arc::clone(&self.hash_clipboard_pending);
        let restart_admission = Arc::clone(&self.restart_admission);
        let active_mount_generation = Arc::clone(&self.hash_mount_generation);
        let rehost_pending = Arc::clone(&self.hash_rehost_pending);
        let rehost_revision = Arc::clone(&self.hash_rehost_revision);
        let presentation_feedback = Arc::clone(&self.hash_presentation_feedback);
        let native_geometry_sequence = Arc::clone(&self.hash_geometry_sequence);
        let hash_tool_key = self.hash_tool_key;
        let restore_geometry = self.restored_hash_geometry();
        let size_write_error = self.hash_size_write_error.clone();
        let utility_presentations = self.utility_presentations.clone();
        let root_visibility = self.root_visibility.clone();
        let setup_session = Arc::clone(&session);
        let setup_active_mount_generation = Arc::clone(&active_mount_generation);
        let setup_restart_admission = Arc::clone(&restart_admission);
        cx.notify();

        cx.spawn(async move |_, cx| {
            let _operation = operation;
            let mount_config = HashMountConfig {
                session,
                wake_signal,
                file_picker_pending,
                clipboard_pending,
                restart_admission,
                mount_generation: destination_mount_generation,
                session_epoch: destination_session_epoch,
                active_mount_generation,
                rehost_pending: Arc::clone(&rehost_pending),
                rehost_revision,
                presentation_feedback: Arc::clone(&presentation_feedback),
                native_geometry_sequence,
                hash_tool_key,
                size_write_error,
                root: root.clone(),
                mode: target,
            };
            let built = match target {
                HashPresentationMode::Docked => {
                    let config = mount_config;
                    match cx.update(|app| {
                        root_window.update(app, |_, window, app| {
                            create_hash_generator_view(
                                window,
                                app,
                                initial_draft,
                                None,
                                config,
                            )
                        })
                    }) {
                        Ok(view) => Ok(BuiltHashMount::Docked { view }),
                        Err(error) => Err(format!(
                            "Could not build the docked Hash Generator view: {error}"
                        )),
                    }
                }
                HashPresentationMode::Popout => {
                    let options = cx.update(|app| hash_popout_window_options(app));
                    let config = mount_config;
                    match cx.update(|app| {
                        gpui_kit::open_window(options, app, move |window, app| {
                            let native_window = checked_native_window(window).ok().flatten();
                            create_hash_generator_view(
                                window,
                                app,
                                initial_draft,
                                native_window,
                                config,
                            )
                        })
                    }) {
                        Ok((window, view)) => {
                            let native_window = cx.update(|app| view.read(app).native_window);
                            if cfg!(windows) && native_window.is_none() {
                                let _ = view.update(cx, |tool, _| {
                                    let _ = tool.take_native_geometry();
                                    tool.detach_native_geometry();
                                });
                                let candidate = RetiringHashWindow {
                                    window,
                                    native_window,
                                };
                                if !remove_and_wait_for_hash_window(cx, candidate).await {
                                    let _ = root.update(cx, |root, cx| {
                                        root.hash_retiring_windows.push(candidate);
                                        cx.notify();
                                    });
                                }
                                Err("The Hash Generator popout did not provide a checked Windows handle.".to_owned())
                            } else {
                                match root.update(cx, |root, cx| {
                                    root.observe_hash_popout_close(
                                        window,
                                        native_window,
                                        destination_mount_generation,
                                        destination_session_epoch,
                                        cx,
                                    )
                                }) {
                                    Ok(close_subscription) => Ok(BuiltHashMount::Popout {
                                        window,
                                        view,
                                        native_window,
                                        close_subscription,
                                    }),
                                    Err(error) => {
                                        let _ = view.update(cx, |tool, _| {
                                            let _ = tool.take_native_geometry();
                                            tool.detach_native_geometry();
                                        });
                                        let candidate = RetiringHashWindow {
                                            window,
                                            native_window,
                                        };
                                        if !remove_and_wait_for_hash_window(cx, candidate).await {
                                            let _ = root.update(cx, |root, cx| {
                                                root.hash_retiring_windows.push(candidate);
                                                cx.notify();
                                            });
                                        }
                                        Err(format!(
                                            "Could not install the Hash Generator close owner: {error}"
                                        ))
                                    }
                                }
                            }
                        }
                        Err(error) => Err(format!(
                            "Could not open the Hash Generator popout: {error}"
                        )),
                    }
                }
            };

            let built = match built {
                Ok(built) => built,
                Err(error) => {
                    let _ = root.update(cx, |root, cx| {
                        root.restore_hash_transition(
                            source_mount_generation,
                            session_epoch,
                            revision,
                            error,
                            cx,
                        );
                    });
                    return;
                }
            };

            let new_view = match &built {
                BuiltHashMount::Docked { view } | BuiltHashMount::Popout { view, .. } => {
                    view.clone()
                }
            };
            let candidate_popout = match &built {
                BuiltHashMount::Docked { .. } => None,
                BuiltHashMount::Popout {
                    window,
                    native_window,
                    ..
                } => Some(RetiringHashWindow {
                    window: *window,
                    native_window: *native_window,
                }),
            };
            let candidate_native_window = candidate_popout.and_then(|candidate| candidate.native_window);
            let (new_state, new_popout_subscription) = match built {
                BuiltHashMount::Docked { view } => (
                    HashWindowState::Docked {
                        mount_generation: destination_mount_generation,
                        view,
                    },
                    None,
                ),
                BuiltHashMount::Popout {
                    window,
                    view,
                    close_subscription,
                    ..
                } => (
                    HashWindowState::Popout {
                        mount_generation: destination_mount_generation,
                        window,
                        view,
                    },
                    Some((window, close_subscription)),
                ),
            };
            let commit = root.update(cx, |root, cx| {
                let still_current = root.hash_rehost_pending.load(Ordering::Acquire)
                    && root.hash_rehost_revision.load(Ordering::Acquire) == revision
                    && root.hash_mount_generation.load(Ordering::Acquire)
                        == source_mount_generation
                    && root.hash_window.mount_generation() == Some(source_mount_generation)
                    && root
                        .hash_session
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .epoch()
                        == session_epoch;
                if !still_current {
                    return Err("The active Hash Generator mount changed before rehosting completed.".to_owned());
                }

                root.drain_hash_native_geometry(cx, true);
                let old_state = std::mem::replace(&mut root.hash_window, new_state);
                let new_geometry_error = new_view.read(cx).native_geometry_error.clone();
                root.hash_native_geometry_error = new_geometry_error;
                let retiring = match old_state {
                    HashWindowState::Popout { window, view, .. } => {
                        let retiring = RetiringHashWindow {
                            window,
                            native_window: view.read(cx).native_window,
                        };
                        root.hash_retiring_windows.push(retiring);
                        Some(retiring)
                    }
                    _ => None,
                };
                root.hash_mount_generation
                    .store(destination_mount_generation, Ordering::Release);
                root.hash_rehost_pending.store(false, Ordering::Release);
                if let Some((window, subscription)) = new_popout_subscription {
                    root.hash_window_close_subscriptions
                        .push((window, subscription));
                }
                new_view.update(cx, |tool, cx| {
                    tool.set_editors_readonly(false, cx);
                    cx.notify();
                });
                root.hash_open_error = None;
                root.hash_presentation_error = None;
                *root.hash_presentation_feedback
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
                let preference_update = root
                    .utility_presentations
                    .remember(hash_tool_key, target.remembered());
                match preference_update {
                    UtilityPresentationUpdate::Unchanged => {}
                    UtilityPresentationUpdate::Pending => {
                        *root.hash_presentation_feedback
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner) =
                            Some("Saving Hash Generator presentation preference…".to_owned());
                    }
                    UtilityPresentationUpdate::ImmediateFailure => {
                        for (_, error) in root.utility_presentations.poll() {
                            root.hash_presentation_error = Some(error.clone());
                            *root.hash_presentation_feedback
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner) =
                                Some(error);
                        }
                    }
                }
                let show_root = target == HashPresentationMode::Docked
                    && !root.root_visibility.visible.load(Ordering::Acquire);
                root.notify_hash_mount(cx);
                root.hash_wake_signal.notify();
                Ok((retiring, preference_update, show_root))
            });

            let (retiring, preference_update, show_root) = match commit {
                Ok(Ok(committed)) => committed,
                Ok(Err(error)) => {
                    let _ = new_view.update(cx, |tool, _| {
                        let _ = tool.take_native_geometry();
                        tool.detach_native_geometry();
                    });
                    if let Some(candidate) = candidate_popout {
                        let retired = remove_and_wait_for_hash_window(cx, candidate).await;
                        if !retired {
                            let _ = root.update(cx, |root, cx| {
                                root.hash_retiring_windows.push(candidate);
                                cx.notify();
                            });
                        }
                    }
                    let _ = root.update(cx, |root, cx| {
                        root.restore_hash_transition(
                            source_mount_generation,
                            session_epoch,
                            revision,
                            error,
                            cx,
                        );
                    });
                    return;
                }
                Err(error) => {
                    let _ = root.update(cx, |root, cx| {
                        root.restore_hash_transition(
                            source_mount_generation,
                            session_epoch,
                            revision,
                            format!("Could not commit the Hash Generator presentation: {error}"),
                            cx,
                        );
                    });
                    return;
                }
            };

            if let Some(native_window) = candidate_native_window {
                let still_current = || {
                    setup_active_mount_generation.load(Ordering::Acquire)
                        == destination_mount_generation
                        && setup_session
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .epoch()
                            == destination_session_epoch
                        && !setup_restart_admission.is_fenced()
                };
                if still_current() {
                    // Native placement can synchronously deliver WM_DPICHANGED,
                    // so it must run after the root commit and outside every
                    // App, Window, and entity update closure.
                    let placement_error = checked_geometry_target(native_window)
                        .ok_or_else(|| {
                            "the Hash Generator HWND could not be checked".to_owned()
                        })
                        .and_then(|target| {
                            restore_native_window_geometry(
                                target,
                                restore_geometry,
                                (420, 320),
                                1.0,
                            )
                            .map(|_| ())
                        })
                        .err()
                        .map(|error| {
                            format!("Could not restore the Hash Generator popout position: {error}")
                        });
                    if still_current() {
                        let setup_error = new_view.update(cx, |tool, cx| {
                            if !tool.is_current_mount() || !still_current() {
                                return None;
                            }
                            let mut errors = Vec::new();
                            if let Some(error) = tool.native_geometry_error.take() {
                                errors.push(error);
                            }
                            if let Some(error) = placement_error.clone() {
                                errors.push(error);
                            }
                            if let Err(error) = tool.attach_native_geometry() {
                                errors.push(format!(
                                    "Native move/resize persistence is unavailable: {error}"
                                ));
                            }
                            let message = (!errors.is_empty()).then(|| errors.join(" "));
                            tool.native_geometry_error = message.clone();
                            cx.notify();
                            message
                        });
                        if let Some(error) = setup_error {
                            let _ = root.update(cx, |root, cx| {
                                if root.hash_window.mount_generation()
                                    == Some(destination_mount_generation)
                                {
                                    root.hash_native_geometry_error = Some(error);
                                    cx.notify();
                                }
                            });
                        }
                    }
                }
            }

            if show_root && !root_visibility.request_hash_visible(destination_mount_generation) {
                let _ = root.update(cx, |root, cx| {
                    let message = "Hash Generator is docked, but the root window could not be shown by its visibility owner.".to_owned();
                    root.hash_open_error = Some(message.clone());
                    *root.hash_presentation_feedback
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(message);
                    cx.notify();
                });
            }

            if source_mode == Some(HashPresentationMode::Docked)
                && target == HashPresentationMode::Popout
                && let Some(root_view) = root.upgrade()
            {
                let focus_result = update_kit_window_content(
                    root_window,
                    &root_view,
                    cx,
                    |root, window, cx| {
                        let root_handle_matches = root.root_window.is_some_and(|handle| {
                            handle.window_id() == root_window.window_id()
                        });
                        let mount_matches = root_search_focus_is_current(
                            root.hash_window.mode(),
                            root.hash_window.mount_generation(),
                            root.hash_mount_generation.load(Ordering::Acquire),
                            Some(HashPresentationMode::Popout),
                            destination_mount_generation,
                            root.restart_admission.is_fenced(),
                        );
                        let native_root_is_current = !cfg!(windows)
                            || root.native_window.is_some_and(native_window_is_valid);
                        if root_handle_matches
                            && mount_matches
                            && native_root_is_current
                        {
                            root.query.focus_handle(cx).focus(window, cx);
                        }
                    },
                );
                if let Err(error) = focus_result {
                    tracing::debug!(%error, "could not restore GPUI root focus after popping out Hash Generator");
                }
            }

            if let Some(retiring) = retiring {
                let retired = remove_and_wait_for_hash_window(cx, retiring).await;
                if retired {
                    let _ = root.update(cx, |root, cx| {
                        root.hash_retiring_windows.retain(|window| {
                            window.window.window_id() != retiring.window.window_id()
                        });
                        cx.notify();
                    });
                } else {
                    let _ = root.update(cx, |root, cx| {
                        let message = "The previous Hash Generator popout has not retired. Close it before restarting in egui.".to_owned();
                        root.hash_open_error = Some(message.clone());
                        *root.hash_presentation_feedback
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(message);
                        cx.notify();
                    });
                }
            }

            if preference_update == UtilityPresentationUpdate::Pending {
                while utility_presentations.has_pending() {
                    cx.background_executor()
                        .timer(Duration::from_millis(16))
                        .await;
                    let failures = utility_presentations.poll();
                    if !failures.is_empty() {
                        let _ = root.update(cx, |root, cx| {
                            for (_, error) in failures {
                                root.hash_presentation_error = Some(error.clone());
                                *root.hash_presentation_feedback
                                    .lock()
                                    .unwrap_or_else(std::sync::PoisonError::into_inner) =
                                    Some(error);
                            }
                        root.notify_hash_mount(cx);
                        });
                    }
                }
                let _ = root.update(cx, |root, cx| {
                    if root.hash_presentation_error.is_none() {
                        *root.hash_presentation_feedback
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
                    }
                    root.notify_hash_mount(cx);
                });
            }
        })
        .detach();
    }

    fn start_hash_close(
        &mut self,
        mount_generation: u64,
        session_epoch: u64,
        revision: u64,
        operation: EffectPermit,
        cx: &mut Context<Self>,
    ) {
        if self.hash_rehost_revision.load(Ordering::Acquire) != revision
            || !self.hash_rehost_pending.load(Ordering::Acquire)
            || self.hash_mount_generation.load(Ordering::Acquire) != mount_generation
            || self.hash_window.mount_generation() != Some(mount_generation)
        {
            self.restore_hash_transition(
                mount_generation,
                session_epoch,
                revision,
                "Hash Generator changed before it could close.".to_owned(),
                cx,
            );
            return;
        }
        let epoch_matches = self
            .hash_session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .epoch()
            == session_epoch;
        if !epoch_matches {
            self.restore_hash_transition(
                mount_generation,
                session_epoch,
                revision,
                "Hash Generator session changed before it could close.".to_owned(),
                cx,
            );
            return;
        }
        self.cancel_docked_hash_resize_for_mount(mount_generation, session_epoch);
        self.drain_hash_native_geometry(cx, true);
        let retiring = match &self.hash_window {
            HashWindowState::Popout { window, view, .. } => Some(RetiringHashWindow {
                window: *window,
                native_window: view.read(cx).native_window,
            }),
            _ => None,
        };
        if let Some(retiring) = retiring {
            self.hash_retiring_windows.push(retiring);
        }
        self.hash_mount_generation.store(0, Ordering::Release);
        self.hash_rehost_pending.store(false, Ordering::Release);
        self.hash_window = HashWindowState::Closed;
        self.hash_session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .end();
        self.hash_window_revision.fetch_add(1, Ordering::AcqRel);
        self.hash_open_error = None;
        cx.notify();

        let root = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let _operation = operation;
            let retired = if let Some(retiring) = retiring {
                let retired = remove_and_wait_for_hash_window(cx, retiring).await;
                let _ = root.update(cx, |root, cx| {
                    if retired {
                        root.hash_retiring_windows.retain(|window| {
                            window.window.window_id() != retiring.window.window_id()
                        });
                    } else {
                        let message =
                            "Hash Generator closed, but its popout window did not retire."
                                .to_owned();
                        root.hash_open_error = Some(message.clone());
                        *root
                            .hash_presentation_feedback
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(message);
                    }
                    cx.notify();
                });
                retired
            } else {
                true
            };
            if retired {
                restore_root_search_focus_after_hash_close(root.clone(), cx).await;
            }
        })
        .detach();
    }

    fn toggle_coordinate_hud_probe(&mut self, cx: &mut Context<Self>) {
        if self.coordinate_hud_probe_pending
            || self.coordinate_hud_probe_closing.load(Ordering::Acquire)
        {
            return;
        }
        let Some(operation) = self.restart_admission.admit() else {
            self.coordinate_hud_probe_status =
                "GPUI is closing; Coordinate HUD changes are disabled.".to_owned();
            cx.notify();
            return;
        };

        self.coordinate_hud_probe_pending = true;
        self.coordinate_hud_probe_status = "Updating Coordinate HUD…".to_owned();
        cx.notify();

        let controller = Arc::clone(&self.coordinate_hud_probe_controller);
        let closing = Arc::clone(&self.coordinate_hud_probe_closing);
        cx.spawn(async move |this, cx| {
            let _operation = operation;
            let result = cx
                .background_executor()
                .spawn(async move {
                    if closing.load(Ordering::Acquire) {
                        return Err("launcher is closing".to_owned());
                    }
                    let mut controller = controller
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    if closing.load(Ordering::Acquire) {
                        return Err("launcher is closing".to_owned());
                    }
                    toggle_coordinate_hud_probe(&mut controller)
                })
                .await;
            let status = match result {
                Ok(status) => status,
                Err(error) => format!("Coordinate HUD error: {error}"),
            };
            let _ = this.update(cx, |this, cx| {
                this.coordinate_hud_probe_pending = false;
                this.coordinate_hud_probe_status = status;
                cx.notify();
            });
        })
        .detach();
    }

    fn open_or_toggle_secondary(&mut self, cx: &mut Context<Self>) {
        let Some(operation) = self.restart_admission.admit() else {
            self.restart_status =
                Some("GPUI is closing; utility window changes are disabled.".to_owned());
            cx.notify();
            return;
        };
        let foreground_before_request = current_foreground_identity();
        let (handle, view) = match &self.secondary {
            SecondaryWindowState::Closed => {
                self.open_secondary(operation, cx);
                return;
            }
            SecondaryWindowState::OpenPending => return,
            SecondaryWindowState::Live { window, view } => (*window, view.clone()),
        };

        {
            let state = update_kit_window_content(handle, &view, cx, |secondary, window, cx| {
                let is_visible = secondary
                    .native_window
                    .and_then(|native| native_window_is_visible(native).ok())
                    .unwrap_or(true);
                if !is_visible {
                    secondary.query.focus_handle(cx).focus(window, cx);
                }
                (secondary.native_window, is_visible)
            });

            if let Ok((Some(native_window), was_visible)) = state {
                let show = !was_visible;
                let secondary_revision = Arc::clone(&self.secondary_revision);
                let revision = secondary_revision.fetch_add(1, Ordering::AcqRel) + 1;
                cx.spawn(async move |_, cx| {
                    let _operation = operation;
                    if secondary_revision.load(Ordering::Acquire) != revision {
                        return;
                    }
                    if let Err(error) = set_native_window_visible(native_window, show) {
                        tracing::warn!(%error, "could not change the retained GPUI window visibility");
                        return;
                    }
                    if show {
                        let expected_after = current_foreground_identity()
                            .filter(|foreground| Some(*foreground) == foreground_before_request);
                        let secondary_revision = Arc::clone(&secondary_revision);
                        cx.background_executor()
                            .spawn(async move {
                                activate_for_secondary_window(
                                    &secondary_revision,
                                    revision,
                                    native_window,
                                    expected_after,
                                );
                            })
                            .await;
                    }
                })
                .detach();
                return;
            }

            match state {
                Ok(_) => {
                    self.secondary_error = Some(
                        "The retained window has no usable native handle on this platform."
                            .to_owned(),
                    );
                    cx.notify();
                    return;
                }
                Err(_) => {
                    self.secondary = SecondaryWindowState::Closed;
                    self.secondary_revision.fetch_add(1, Ordering::AcqRel);
                    self.secondary_close_subscription = None;
                }
            }
        }

        self.open_secondary(operation, cx);
    }

    fn open_secondary(&mut self, operation: EffectPermit, cx: &mut Context<Self>) {
        if !matches!(self.secondary, SecondaryWindowState::Closed) {
            return;
        }
        self.secondary = SecondaryWindowState::OpenPending;
        let options = WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some("Multi Launcher GPUI Retained Window".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let secondary_revision = Arc::clone(&self.secondary_revision);
        let restart_admission = Arc::clone(&self.restart_admission);
        cx.spawn(async move |this, cx| {
            let _operation = operation;
            let opened = cx.update(|cx| {
                gpui_kit::open_window(options, cx, |window, cx| {
                    let query = cx.new(|cx| {
                        InputState::new(window, cx).placeholder("Keep text while this window is open")
                    });
                    query.focus_handle(cx).focus(window, cx);
                    let native_window = match checked_native_window(window) {
                        Ok(native_window) => native_window,
                        Err(error) => {
                            tracing::error!(%error, "could not capture retained GPUI window handle");
                            None
                        }
                    };
                    cx.new(|_| SecondaryWindow {
                        query,
                        native_window,
                        secondary_revision: Arc::clone(&secondary_revision),
                        restart_admission: Arc::clone(&restart_admission),
                    })
                })
            });
            match opened {
                Ok((window, view)) => {
                    let window_id = window.window_id();
                    let root = this.clone();
                    let close_subscription = cx.update(|cx| {
                        cx.on_window_closed(move |cx, closed| {
                            if closed != window_id {
                                return;
                            }
                            let _ = root.update(cx, |root, cx| {
                                if matches!(
                                    &root.secondary,
                                    SecondaryWindowState::Live { window, .. }
                                        if window.window_id() == window_id
                                ) {
                                    root.secondary = SecondaryWindowState::Closed;
                                    root.secondary_revision.fetch_add(1, Ordering::AcqRel);
                                    root.secondary_close_subscription = None;
                                    cx.notify();
                                }
                            });
                        })
                    });
                    let _ = this.update(cx, |this, cx| {
                        if matches!(this.secondary, SecondaryWindowState::OpenPending) {
                            this.secondary = SecondaryWindowState::Live { window, view };
                            this.secondary_close_subscription = Some(close_subscription);
                            this.secondary_error = None;
                            cx.notify();
                        }
                    });
                }
                Err(error) => {
                    let _ = this.update(cx, |this, cx| {
                        if matches!(this.secondary, SecondaryWindowState::OpenPending) {
                            this.secondary = SecondaryWindowState::Closed;
                            this.secondary_error = Some(format!(
                                "Could not open the retained GPUI window: {error}"
                            ));
                            cx.notify();
                        }
                    });
                }
            }
        })
        .detach();
    }

    fn hash_clipboard_request_is_current(&self, request: HashClipboardRequest) -> bool {
        if self.restart_admission.is_fenced()
            || self.hash_window.mount_generation() != Some(request.mount_generation)
        {
            return false;
        }
        let session = self
            .hash_session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        hash_clipboard_request_is_current(
            self.hash_mount_generation.load(Ordering::Acquire),
            session.epoch(),
            session.generation(),
            request,
            self.hash_rehost_pending.load(Ordering::Acquire),
        )
    }

    fn current_hash_clipboard_target(&self) -> Option<HashClipboardTarget> {
        let session_epoch = self
            .hash_session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .epoch();
        match &self.hash_window {
            HashWindowState::Closed => None,
            HashWindowState::Docked {
                mount_generation,
                view,
            } => Some(HashClipboardTarget {
                window: self.root_window?,
                mount_generation: *mount_generation,
                session_epoch,
                view: view.clone(),
            }),
            HashWindowState::Popout {
                mount_generation,
                window,
                view,
            } => Some(HashClipboardTarget {
                window: *window,
                mount_generation: *mount_generation,
                session_epoch,
                view: view.clone(),
            }),
        }
    }

    fn start_hash_clipboard_operation(
        &mut self,
        request: HashClipboardRequest,
        kind: HashClipboardOperationKind,
        operation: EffectPermit,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if self.hash_clipboard_operation.is_some() || self.hash_clipboard_pending.is_pending() {
            return Err(
                "Another Hash Generator clipboard operation is already in progress.".into(),
            );
        }
        if !self.hash_clipboard_request_is_current(request) {
            return Err(
                "Hash Generator changed before the clipboard operation could start. Try again."
                    .into(),
            );
        }
        if let HashClipboardOperationKind::Copy(copy) = &kind {
            if !hash_clipboard_copy_admissible(
                self.hash_clipboard_pending.kind(),
                self.hash_file_picker_pending.load(Ordering::Acquire),
            ) {
                return Err(
                    "Finish the file picker or other Clipboard operation before copying the digest."
                        .into(),
                );
            }
            let session = self
                .hash_session
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if !session.is_clipboard_copy_current(copy) {
                return Err(
                    "The current digest changed before it could be copied. Calculate again and retry."
                        .into(),
                );
            }
        }

        let pending_kind = match &kind {
            HashClipboardOperationKind::Load => HashClipboardPendingKind::Load,
            HashClipboardOperationKind::Copy(_) => HashClipboardPendingKind::Copy,
        };
        if !self.hash_clipboard_pending.begin(pending_kind) {
            return Err(
                "Another Hash Generator clipboard operation is already in progress.".into(),
            );
        }
        self.hash_clipboard_operation_counter =
            self.hash_clipboard_operation_counter.wrapping_add(1).max(1);
        let operation_id = self.hash_clipboard_operation_counter;
        let worker_kind = kind.clone();
        self.hash_clipboard_operation = Some(HashClipboardOperation {
            id: operation_id,
            request,
            kind,
        });
        let pending = Arc::clone(&self.hash_clipboard_pending);
        cx.spawn(async move |this, cx| {
            let _operation = operation;
            let outcome = match worker_kind {
                HashClipboardOperationKind::Load => {
                    let result = cx
                        .background_executor()
                        .spawn(async { ArboardClipboardBackend.read_text() })
                        .await;
                    HashClipboardOutcome::Load(result)
                }
                HashClipboardOperationKind::Copy(copy) => {
                    let digest = copy.digest().to_owned();
                    let result = cx
                        .background_executor()
                        .spawn(async move { ArboardClipboardBackend.write_text(&digest) })
                        .await;
                    HashClipboardOutcome::Copy(result)
                }
            };
            if this
                .update(cx, |root, cx| {
                    root.finish_hash_clipboard_operation(operation_id, outcome, cx);
                })
                .is_err()
            {
                pending.finish();
            }
        })
        .detach();
        Ok(())
    }

    fn finish_hash_clipboard_operation(
        &mut self,
        operation_id: u64,
        outcome: HashClipboardOutcome,
        cx: &mut Context<Self>,
    ) {
        if !self
            .hash_clipboard_operation
            .as_ref()
            .is_some_and(|operation| operation.id == operation_id)
        {
            return;
        }
        let Some(operation) = self.hash_clipboard_operation.take() else {
            return;
        };
        let request_is_current = self.hash_clipboard_request_is_current(operation.request);
        let is_copy = matches!(&operation.kind, HashClipboardOperationKind::Copy(_));
        let restore_copy_editors = is_copy
            && self.hash_window.mount_generation() == Some(operation.request.mount_generation)
            && self
                .hash_session
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .epoch()
                == operation.request.session_epoch;
        let load_was_applied = request_is_current
            && matches!(&operation.kind, HashClipboardOperationKind::Load)
            && matches!(&outcome, HashClipboardOutcome::Load(Ok(_)));

        if request_is_current {
            let mut session = self
                .hash_session
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            match (operation.kind, outcome) {
                (HashClipboardOperationKind::Load, HashClipboardOutcome::Load(result)) => {
                    session.apply_clipboard_load(result);
                }
                (HashClipboardOperationKind::Copy(request), HashClipboardOutcome::Copy(result))
                    if session.is_clipboard_copy_current(&request) =>
                {
                    session.apply_clipboard_copy(&request, result);
                }
                _ => {}
            }
        }

        self.hash_clipboard_pending.finish();
        if let Some(target) = self.current_hash_clipboard_target() {
            let same_mount = target.mount_generation == operation.request.mount_generation
                && target.session_epoch == operation.request.session_epoch;
            if let Err(error) =
                update_kit_window_content(target.window, &target.view, cx, |tool, window, cx| {
                    if restore_copy_editors && same_mount {
                        tool.set_editors_readonly(false, cx);
                    }
                    if load_was_applied && same_mount {
                        tool.source.update(cx, |source, cx| {
                            source.set_selected_value(&HashSourceChoice::Clipboard, window, cx)
                        });
                    }
                    cx.notify();
                })
            {
                tracing::debug!(%error, "could not notify the current Hash view after clipboard work");
            }
        }
        cx.notify();
    }

    fn hash_picker_target(&self, request: HashPickerRequest) -> Option<HashPickerTarget> {
        if self.hash_rehost_pending.load(Ordering::Acquire)
            || self.restart_admission.is_fenced()
            || self.hash_mount_generation.load(Ordering::Acquire) != request.mount_generation
        {
            return None;
        }
        let session = self
            .hash_session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !hash_picker_result_is_current(
            self.hash_mount_generation.load(Ordering::Acquire),
            request.mount_generation,
            session.epoch(),
            request.session_epoch,
            session.generation(),
            request.source_generation,
        ) {
            return None;
        }
        drop(session);

        match &self.hash_window {
            HashWindowState::Docked {
                mount_generation,
                view,
            } if *mount_generation == request.mount_generation => Some(HashPickerTarget {
                mode: HashPresentationMode::Docked,
                window: self.root_window?,
                view: view.clone(),
            }),
            HashWindowState::Popout {
                mount_generation,
                window,
                view,
            } if *mount_generation == request.mount_generation => Some(HashPickerTarget {
                mode: HashPresentationMode::Popout,
                window: *window,
                view: view.clone(),
            }),
            _ => None,
        }
    }

    fn start_hash_file_picker<F, E>(
        &mut self,
        request: HashPickerRequest,
        picker: F,
        operation: EffectPermit,
        cx: &mut Context<Self>,
    ) where
        F: Future<Output = Result<anyhow::Result<Option<Vec<PathBuf>>>, E>> + 'static,
        E: 'static,
    {
        let picker_pending = Arc::clone(&self.hash_file_picker_pending);
        cx.spawn(async move |this, cx| {
            let _operation = operation;
            let mut picker_pending = HashPickerPendingGuard::new(picker_pending);
            let outcome = classify_hash_picker_result(picker.await);
            let target = this
                .update(cx, |root, _| root.hash_picker_target(request))
                .ok()
                .flatten();
            let Some(target) = target else {
                picker_pending.clear_and_notify(&this, cx);
                return;
            };

            let update_result = match target.mode {
                HashPresentationMode::Docked => {
                    let Some(root_view) = this.upgrade() else {
                        picker_pending.clear_and_notify(&this, cx);
                        return;
                    };
                    cx.update(|app| {
                        update_kit_window_content(
                            target.window,
                            &root_view,
                            app,
                            |root, window, cx| {
                                if root.hash_picker_target(request).is_none() {
                                    return;
                                }
                                if let Some(view) = root.hash_window.view().cloned() {
                                    view.update(cx, |tool, cx| {
                                        tool.resolve_file_picker(request, outcome, window, cx);
                                    });
                                }
                            },
                        )
                    })
                }
                HashPresentationMode::Popout => cx.update(|app| {
                    update_kit_window_content(
                        target.window,
                        &target.view,
                        app,
                        |tool, window, cx| {
                            tool.resolve_file_picker(request, outcome, window, cx);
                        },
                    )
                }),
            };
            picker_pending.clear_and_notify(&this, cx);
            if let Err(error) = update_result {
                tracing::debug!(%error, "could not apply Hash Generator file picker result");
            }
        })
        .detach();
    }

    fn request_quit(&mut self, cx: &mut Context<Self>) {
        if let Some(message) = hash_clipboard_close_message(self.hash_clipboard_pending.kind()) {
            self.restart_status = Some(message.to_owned());
            cx.notify();
            return;
        }
        if self.hash_file_picker_pending.load(Ordering::Acquire) {
            self.restart_status = Some(
                "Finish or cancel the Hash Generator file picker before quitting Multi Launcher."
                    .to_owned(),
            );
            cx.notify();
            return;
        }
        self.hash_docked_resize
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        self.drain_hash_native_geometry(cx, true);
        cx.quit();
    }
}

impl Render for RootWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let resolved = appearance::current(cx);
        let metrics = resolved.metrics;
        let palette = resolved.palette;
        let docked_hash = match &self.hash_window {
            HashWindowState::Docked {
                mount_generation,
                view,
            } => Some((view.clone(), *mount_generation)),
            _ => None,
        };
        let root_controls_width = if docked_hash.is_some() {
            docked_hash_available_size(
                window.bounds().size.width.as_f32(),
                window.bounds().size.height.as_f32(),
                metrics.window_padding,
            )
            .width
            .min(HASH_ROOT_CONTROLS_WIDTH)
        } else {
            HASH_ROOT_CONTROLS_WIDTH
        };
        let multirow_results = self.launcher_results.actions.len() > 1;
        let docked_hash_frame = docked_hash.as_ref().map(|(view, mount_generation)| {
            let session_epoch = self
                .hash_session
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .epoch();
            let payload = DockedHashResizePayload {
                mount_generation: *mount_generation,
                session_epoch,
                start_size: self.hash_docked_preferred_size,
                preview_extent: Arc::clone(&self.hash_docked_resize),
            };
            let resize_preview = self.docked_hash_resize_preview(*mount_generation, session_epoch);
            let requested_size = resize_preview.unwrap_or(self.hash_docked_preferred_size);
            let available_size = docked_hash_available_size(
                window.bounds().size.width.as_f32(),
                window.bounds().size.height.as_f32(),
                metrics.window_padding,
            );
            let current_visible_size =
                docked_hash_fit(self.hash_docked_preferred_size, available_size);
            let displayed_size = docked_hash_fit(requested_size, available_size);
            let resize_accessibility_label = docked_hash_resize_accessibility_label(
                self.hash_docked_preferred_size,
                current_visible_size,
                requested_size,
                displayed_size,
                resize_preview.is_some(),
            );
            let handle_size = HASH_DOCK_RESIZE_HANDLE_SIZE
                .min(displayed_size.width)
                .min(displayed_size.height)
                .max(1.0);

            let pointer_payload = payload.clone();
            let key_payload = payload.clone();
            let drag_started_payload = payload.clone();
            let key_available_size = available_size;
            let resize_handle = div()
                .id("hash-generator-docked-resize-handle")
                .absolute()
                .right_0()
                .bottom_0()
                .size(px(handle_size))
                .cursor(CursorStyle::ResizeUpLeftDownRight)
                .role(Role::Button)
                .aria_label(resize_accessibility_label)
                .key_context("HashGeneratorWindow")
                .track_focus(&self.resize_focus_handle)
                .tab_index(0)
                .focus_visible(|style| {
                    style
                        .border_1()
                        .border_color(appearance::to_hsla(palette.focus))
                })
                .bg(appearance::to_hsla(palette.panel_surface))
                .text_color(appearance::to_hsla(palette.accent))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |root, event: &MouseDownEvent, window, cx| {
                        root.resize_focus_handle.focus(window, cx);
                        cx.stop_propagation();
                        root.begin_docked_hash_pointer_resize(
                            &pointer_payload,
                            event.position,
                            window,
                            metrics.window_padding,
                            cx,
                        );
                    }),
                )
                .on_drag(payload.clone(), move |_, _, _, cx| {
                    drag_started_payload.mark_drag_started();
                    cx.new(|_| Empty)
                })
                .on_key_down(cx.listener(move |root, event: &KeyDownEvent, _, cx| {
                    if event.keystroke.modifiers != Modifiers::default() {
                        return;
                    }
                    if root.handle_docked_hash_resize_key(
                        &key_payload,
                        event.keystroke.key.as_str(),
                        key_available_size,
                        cx,
                    ) {
                        cx.stop_propagation();
                    }
                }))
                .child(Label::new("↘").text_size(px(metrics.heading_text_size)));

            let release_payload = payload.clone();
            let outside_release_payload = payload.clone();
            div()
                .id("hash-generator-docked-frame")
                .relative()
                .flex_shrink_0()
                .w(px(displayed_size.width))
                .h(px(displayed_size.height))
                .on_drag_move(cx.listener(
                    move |root, event: &DragMoveEvent<DockedHashResizePayload>, _, cx| {
                        let drag_payload = event.drag(cx).clone();
                        root.update_docked_hash_pointer_resize(
                            &drag_payload,
                            event.event.position,
                            cx,
                        );
                    },
                ))
                .capture_any_mouse_up(cx.listener(move |root, event: &MouseUpEvent, _, cx| {
                    if event.button == MouseButton::Left {
                        root.update_docked_hash_pointer_resize(
                            &release_payload,
                            event.position,
                            cx,
                        );
                        root.commit_docked_hash_resize(&release_payload, false, cx);
                    }
                }))
                .on_mouse_up_out(
                    MouseButton::Left,
                    cx.listener(move |root, event: &MouseUpEvent, _, cx| {
                        root.update_docked_hash_pointer_resize(
                            &outside_release_payload,
                            event.position,
                            cx,
                        );
                        root.commit_docked_hash_resize(&outside_release_payload, false, cx);
                    }),
                )
                .child(
                    div()
                        .id("hash-generator-docked-scroll-viewport")
                        .size_full()
                        .pr(px(handle_size))
                        .pb(px(handle_size))
                        .child(view.clone()),
                )
                .child(resize_handle)
        });
        let root_controls = div()
            .flex()
            .flex_col()
            .gap_y(px(metrics.item_spacing_y))
            .w(px(root_controls_width))
            .p(px(metrics.panel_padding))
            .bg(appearance::to_hsla(palette.panel_surface))
            .rounded(px(metrics.corner_radius))
            .text_color(appearance::to_hsla(palette.primary_text))
            .text_size(px(metrics.body_text_size))
            .line_height(px(metrics.line_height))
            .child(
                Label::new("Search")
                    .text_size(px(metrics.heading_text_size))
                    .line_height(px(metrics.line_height)),
            )
            .child(
                div()
                    .id("launcher-search-input")
                    .key_context("LauncherSearch")
                    .capture_action(cx.listener(|root, _: &MoveDown, window, cx| {
                        if root.move_search_selection_down(window, cx) {
                            cx.stop_propagation();
                        }
                    }))
                    .capture_action(cx.listener(|root, _: &MoveUp, window, cx| {
                        if root.move_search_selection_up(window, cx) {
                            cx.stop_propagation();
                        }
                    }))
                    .capture_action(cx.listener(|root, _: &SearchHistoryOlder, window, cx| {
                        if root.navigate_search_history(
                            QueryHistoryDirection::Older,
                            window,
                            cx,
                        ) {
                            cx.stop_propagation();
                        }
                    }))
                    .capture_action(cx.listener(|root, _: &SearchHistoryNewer, window, cx| {
                        if root.navigate_search_history(
                            QueryHistoryDirection::Newer,
                            window,
                            cx,
                        ) {
                            cx.stop_propagation();
                        }
                    }))
                    .capture_key_down(cx.listener(|root, event, window, cx| {
                        root.record_search_key_down(event, window, cx);
                    }))
                    .on_key_up(cx.listener(|root, event, _, _| {
                        root.record_search_key_up(event);
                    }))
                    .child(
                        Input::new(&self.query)
                            .aria_label("Search")
                            .readonly(self.restart_admission.is_fenced()),
                    ),
            )
            .children(self.launcher_results.actions.iter().enumerate().map(
                |(index, action)| {
                    let generation = self.launcher_results.generation;
                    let selected = self.launcher_results.selected == Some(index);
                    div()
                        .id(launcher_result_row_id(generation, index))
                        .flex()
                        .flex_col()
                        .gap_y(px(metrics.item_spacing_y))
                        .w_full()
                        .p(px(metrics.panel_padding))
                        .rounded(px(metrics.corner_radius))
                        .bg(appearance::to_hsla(if selected {
                            palette.selection_fill
                        } else {
                            palette.panel_surface
                        }))
                        .text_color(appearance::to_hsla(palette.primary_text))
                        .role(Role::Button)
                        .aria_label(format!("{}: {}", action.label, action.desc))
                        .tab_stop(false)
                        .on_click(cx.listener(move |root, event: &ClickEvent, window, cx| {
                            root.activate_launcher_result(
                                generation,
                                index,
                                result_click_source(event.is_keyboard()),
                                window,
                                cx,
                            );
                        }))
                        .child(
                            Label::new(action.label.clone())
                                .id("launcher-search-result-label")
                                .text_size(px(metrics.body_text_size))
                                .line_height(px(metrics.line_height)),
                        )
                        .child(
                            Label::new(action.desc.clone())
                                .id("launcher-search-result-description")
                                .text_size(px(metrics.small_text_size))
                                .line_height(px(metrics.line_height)),
                        )
                },
            ))
            .child(
                Label::new("Ctrl+Enter: open or dismiss the retained window")
                    .text_size(px(metrics.small_text_size))
                    .line_height(px(metrics.line_height)),
            )
            .child(
                Button::new("retained-window")
                    .label("Open or dismiss retained window")
                    .tab_stop(true)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.open_or_toggle_secondary(cx);
                    })),
            )
            .child(
                Button::new("coordinate-hud-probe")
                    .label("Toggle passive Coordinate HUD (F6)")
                    .tab_stop(true)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.toggle_coordinate_hud_probe(cx);
                    })),
            )
            .child(
                Button::new("open-hash-generator")
                    .label("Open Hash Generator (Ctrl+G)")
                    .tab_stop(true)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.run_hash_generator_open(window, cx);
                    })),
            )
            .children(docked_hash.is_some().then(|| {
                Label::new(
                    "Ctrl+Shift+R: focus docked resize grip · arrows preview · Enter saves · Escape discards",
                )
                .text_size(px(metrics.small_text_size))
                .line_height(px(metrics.line_height))
            }))
            .child(
                Button::new("restart-egui")
                    .label("Restart in egui (Ctrl+Shift+E)")
                    .tab_stop(true)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_restart_confirmation(window, cx);
                    })),
            )
            .child(
                Label::new(self.coordinate_hud_probe_status.clone())
                    .text_size(px(metrics.small_text_size))
                    .line_height(px(metrics.line_height)),
            )
            .children(self.restart_status.as_ref().map(|status| {
                Label::new(status.clone())
                    .text_color(appearance::to_hsla(palette.warning))
                    .text_size(px(metrics.small_text_size))
                    .line_height(px(metrics.line_height))
            }))
            .child(
                Button::new("quit-gpui")
                    .label("Quit Multi Launcher")
                    .tab_stop(true)
                    .on_click(cx.listener(|this, _, _, cx| this.request_quit(cx))),
            )
            .children(self.secondary_error.as_ref().map(|error| {
                Label::new(error.clone())
                    .text_color(appearance::to_hsla(palette.error))
                    .text_size(px(metrics.small_text_size))
                    .line_height(px(metrics.line_height))
            }))
            .children(self.hash_open_error.as_ref().map(|error| {
                Label::new(error.clone())
                    .text_color(appearance::to_hsla(palette.error))
                    .text_size(px(metrics.small_text_size))
                    .line_height(px(metrics.line_height))
            }))
            .children(self.hash_presentation_error.as_ref().map(|error| {
                Label::new(error.clone())
                    .text_color(appearance::to_hsla(palette.error))
                    .text_size(px(metrics.small_text_size))
                    .line_height(px(metrics.line_height))
            }))
            .children(self.hash_native_geometry_error.as_ref().map(|error| {
                Label::new(error.clone())
                    .id("hash-native-geometry-error")
                    .text_color(appearance::to_hsla(palette.warning))
                    .text_size(px(metrics.small_text_size))
                    .line_height(px(metrics.line_height))
            }))
            .children(self.hash_size_write_error.as_ref().map(|error| {
                Label::new(error.clone())
                    .id("hash-size-write-error")
                    .text_color(appearance::to_hsla(palette.error))
                    .text_size(px(metrics.small_text_size))
                    .line_height(px(metrics.line_height))
            }));
        let mut body = div()
            .flex()
            .flex_col()
            .gap_y(px(metrics.section_spacing))
            .w(px(root_controls_width))
            .child(root_controls);
        if let Some(docked_hash_frame) = docked_hash_frame {
            body = body.child(docked_hash_frame);
        }

        let mut root_scroll = div()
            .id("multi-launcher-root-scroll")
            .flex()
            .size_full()
            .p(px(metrics.window_padding))
            .overflow_y_scroll()
            .items_center()
            .justify_center()
            .track_scroll(&self.root_scroll)
            .when(multirow_results, |this| this.items_start())
            .when(docked_hash.is_some(), |this| {
                this.justify_start().items_start()
            })
            .track_focus(&self.focus_handle)
            .key_context("RootWindow")
            .on_action(cx.listener(|this, _: &FocusDockedHashResize, window, cx| {
                this.focus_docked_hash_resize_handle(window, cx);
            }))
            .on_action(cx.listener(|this, _: &CalculateHash, window, cx| {
                if this.forward_docked_hash_action(DockedHashAction::Calculate, window, cx) {
                    cx.stop_propagation();
                }
            }))
            .on_action(cx.listener(|this, _: &DockHashGenerator, window, cx| {
                if this.forward_docked_hash_action(DockedHashAction::Dock, window, cx) {
                    cx.stop_propagation();
                }
            }))
            .on_action(cx.listener(|this, _: &PopoutHashGenerator, window, cx| {
                if this.forward_docked_hash_action(DockedHashAction::Popout, window, cx) {
                    cx.stop_propagation();
                }
            }))
            .on_action(cx.listener(|this, _: &CloseHashGenerator, window, cx| {
                if this.forward_docked_hash_action(DockedHashAction::Close, window, cx) {
                    cx.stop_propagation();
                }
            }))
            .on_action(cx.listener(|this, _: &ToggleSecondaryWindow, _, cx| {
                this.open_or_toggle_secondary(cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleCoordinateHudProbe, _, cx| {
                this.toggle_coordinate_hud_probe(cx);
            }))
            .on_action(cx.listener(|this, _: &OpenHashGenerator, window, cx| {
                this.run_hash_generator_open(window, cx);
            }))
            .on_action(cx.listener(|this, _: &RestartWithEgui, window, cx| {
                this.open_restart_confirmation(window, cx);
            }))
            .child(body);
        if docked_hash.is_some() {
            root_scroll = root_scroll.capture_any_mouse_down(cx.listener(|this, _, _, cx| {
                if this.cancel_docked_hash_preview() {
                    cx.notify();
                }
            }));
        }
        root_scroll
    }
}

struct HashGeneratorWindow {
    text: Entity<TextareaState>,
    expected: Entity<TextareaState>,
    source: Entity<SelectState<Vec<HashSourceChoice>>>,
    algorithm: Entity<SelectState<Vec<HashAlgorithmChoice>>>,
    focus_handle: FocusHandle,
    session: Arc<Mutex<HashSession>>,
    wake_signal: Arc<HashWakeSignal>,
    file_picker_pending: Arc<AtomicBool>,
    clipboard_pending: Arc<HashClipboardPendingState>,
    native_window: Option<NativeWindow>,
    native_geometry_slot: NativeGeometrySlot,
    native_geometry_stamp: NativeGeometryStamp,
    hash_tool_key: UtilityToolKey,
    native_geometry_sequence: Arc<AtomicU64>,
    native_geometry_binding: Option<NativeGeometryBinding>,
    native_geometry_error: Option<String>,
    size_write_error: Option<String>,
    factory_outer_size: Option<PreferredWindowSize>,
    restart_admission: Arc<RestartAdmission>,
    mount_generation: u64,
    session_epoch: u64,
    active_mount_generation: Arc<AtomicU64>,
    rehost_pending: Arc<AtomicBool>,
    rehost_revision: Arc<AtomicU64>,
    presentation_feedback: Arc<Mutex<Option<String>>>,
    root: WeakEntity<RootWindow>,
    mode: HashPresentationMode,
    file_picker_feedback: Option<String>,
    _subscriptions: Vec<Subscription>,
}

impl HashGeneratorWindow {
    fn attach_native_geometry(&mut self) -> Result<(), String> {
        if self.native_geometry_binding.is_some() || self.mode != HashPresentationMode::Popout {
            return Ok(());
        }
        if !self.is_current_mount() {
            return Err("the Hash Generator mount is no longer active".to_owned());
        }
        let Some(native_window) = self.native_window else {
            return if cfg!(windows) {
                Err("the Hash Generator popout has no checked HWND".to_owned())
            } else {
                Ok(())
            };
        };
        let target = checked_geometry_target(native_window)
            .ok_or_else(|| "the Hash Generator HWND could not be checked".to_owned())?;
        let wake_signal = Arc::clone(&self.wake_signal);
        let binding = NativeGeometryBinding::attach_checked(
            target,
            self.hash_tool_key,
            self.native_geometry_stamp,
            Arc::clone(&self.native_geometry_slot),
            1.0,
            Arc::clone(&self.native_geometry_sequence),
            Arc::new(move |_| wake_signal.notify()),
        )?;
        self.native_geometry_binding = Some(binding);
        Ok(())
    }

    fn take_native_geometry(
        &self,
    ) -> Option<(UtilityNativeGeometrySample, Option<PreferredWindowSize>)> {
        let sample = take_native_geometry(
            &self.native_geometry_slot,
            self.hash_tool_key,
            self.native_geometry_stamp,
        )?;
        let convert = |rect: NativeWindowRect| PhysicalWindowRect {
            left: rect.left,
            top: rect.top,
            right: rect.right,
            bottom: rect.bottom,
        };
        Some((
            UtilityNativeGeometrySample {
                position: convert(sample.position),
                position_scale: sample.position_scale,
                position_sequence: sample.position_sequence,
                size: sample.size.map(convert),
                size_sequence: sample.size_sequence,
            },
            self.factory_outer_size,
        ))
    }

    fn detach_native_geometry(&mut self) {
        self.native_geometry_binding.take();
    }

    fn is_current_mount(&self) -> bool {
        if self.active_mount_generation.load(Ordering::Acquire) != self.mount_generation
            || self.rehost_pending.load(Ordering::Acquire)
            || self.restart_admission.is_fenced()
        {
            return false;
        }
        let session = self
            .session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        hash_mount_identity_matches(
            self.active_mount_generation.load(Ordering::Acquire),
            self.mount_generation,
            session.epoch(),
            self.session_epoch,
        )
    }

    fn draft(&self, cx: &App) -> HashDraft {
        let text = self.text.read(cx).value().to_string();
        let expected = self.expected.read(cx).value().to_string();
        let mode = self
            .source
            .read(cx)
            .selected_value()
            .copied()
            .map(HashSourceChoice::mode);
        let algorithm = self
            .algorithm
            .read(cx)
            .selected_value()
            .map(|choice| choice.0);
        let session = self
            .session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        HashDraft {
            text,
            expected,
            mode: mode.unwrap_or_else(|| session.mode()),
            algorithm: algorithm.unwrap_or_else(|| session.algorithm()),
            file_path: session.file_path().map(PathBuf::from),
            clipboard_snapshot_revision: session.clipboard_snapshot_revision(),
        }
    }

    fn sync_session_draft(&self, cx: &App) -> Option<HashDraft> {
        let draft = self.draft(cx);
        let mut session = self
            .session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if session.epoch() != self.session_epoch {
            return None;
        }
        if session.file_path() != draft.file_path.as_deref() {
            session.set_file_path(draft.file_path.clone());
        }
        session.set_text(draft.text.clone());
        session.set_expected(draft.expected.clone());
        session.set_mode(draft.mode);
        session.set_algorithm(draft.algorithm);
        Some(draft)
    }

    fn set_editors_readonly(&mut self, readonly: bool, cx: &mut Context<Self>) {
        self.text
            .update(cx, |state, cx| state.set_readonly(readonly, cx));
        self.expected
            .update(cx, |state, cx| state.set_readonly(readonly, cx));
    }

    fn focus_editable(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mode = self
            .session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .mode();
        if mode == HashInputMode::Text {
            self.text.focus_handle(cx).focus(window, cx);
        } else {
            self.source.update(cx, |state, cx| state.focus(window, cx));
        }
    }

    fn picker_request_is_current(&self, request: HashPickerRequest) -> bool {
        if !self.is_current_mount()
            || request.mount_generation != self.mount_generation
            || request.session_epoch != self.session_epoch
        {
            return false;
        }
        let session = self
            .session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        hash_picker_result_is_current(
            self.active_mount_generation.load(Ordering::Acquire),
            request.mount_generation,
            session.epoch(),
            request.session_epoch,
            session.generation(),
            request.source_generation,
        )
    }

    fn resolve_file_picker(
        &mut self,
        request: HashPickerRequest,
        outcome: HashPickerOutcome,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.picker_request_is_current(request) {
            return;
        }
        match outcome {
            HashPickerOutcome::Selected(path) => {
                self.session
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .set_file_path(Some(path));
                self.source.update(cx, |state, cx| {
                    state.set_selected_value(&HashSourceChoice::File, window, cx)
                });
                self.file_picker_feedback = None;
            }
            HashPickerOutcome::Cancelled => {
                self.file_picker_feedback =
                    Some("File selection cancelled; the current source is unchanged.".to_owned());
            }
            HashPickerOutcome::Failed(error) => {
                let message = format!("Could not choose a file: {error}");
                self.session
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .set_error(message.clone());
                self.file_picker_feedback = Some(message);
            }
        }
        cx.notify();
    }

    fn calculate_hash(&mut self, cx: &mut Context<Self>) {
        if !self.is_current_mount() || self.clipboard_pending.is_copying() {
            return;
        }
        let Some(_operation) = self.restart_admission.admit() else {
            return;
        };
        if self.sync_session_draft(cx).is_none() {
            return;
        }
        let mut session = self
            .session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if session.has_job() {
            return;
        }
        let wake_signal = Arc::clone(&self.wake_signal);
        if let Err(error) = session.calculate(Arc::new(move || wake_signal.notify())) {
            session.set_error(error);
        }
        cx.notify();
    }

    fn cancel_hash(&mut self, cx: &mut Context<Self>) {
        if !self.is_current_mount() || self.clipboard_pending.is_copying() {
            return;
        }
        let Some(_operation) = self.restart_admission.admit() else {
            return;
        };
        let mut session = self
            .session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if session.epoch() == self.session_epoch {
            session.cancel();
        }
        cx.notify();
    }

    fn clipboard_request(&self) -> Option<HashClipboardRequest> {
        if !self.is_current_mount() {
            return None;
        }
        let session = self
            .session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        (session.epoch() == self.session_epoch).then_some(HashClipboardRequest {
            mount_generation: self.mount_generation,
            session_epoch: self.session_epoch,
            source_generation: session.generation(),
        })
    }

    fn load_clipboard(&mut self, cx: &mut Context<Self>) {
        if !self.is_current_mount() || self.clipboard_pending.is_pending() {
            return;
        }
        let Some(operation) = self.restart_admission.admit() else {
            return;
        };
        if self.sync_session_draft(cx).is_none() {
            return;
        }
        let Some(request) = self.clipboard_request() else {
            return;
        };
        match self.root.update(cx, |root, cx| {
            root.start_hash_clipboard_operation(
                request,
                HashClipboardOperationKind::Load,
                operation,
                cx,
            )
        }) {
            Ok(Ok(())) => self.file_picker_feedback = None,
            Ok(Err(error)) => self
                .set_clipboard_start_error(format!("Could not start Clipboard Load: {error}"), cx),
            Err(error) => self
                .set_clipboard_start_error(format!("Could not start Clipboard Load: {error}"), cx),
        }
        cx.notify();
    }

    fn copy_digest(&mut self, cx: &mut Context<Self>) {
        if !self.is_current_mount()
            || !hash_clipboard_copy_admissible(
                self.clipboard_pending.kind(),
                self.file_picker_pending.load(Ordering::Acquire),
            )
        {
            return;
        }
        let Some(operation) = self.restart_admission.admit() else {
            return;
        };
        if self.sync_session_draft(cx).is_none() {
            return;
        }
        let (request, copy) = {
            let session = self
                .session
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if session.epoch() != self.session_epoch {
                return;
            }
            let Some(copy) = session.clipboard_copy_request() else {
                return;
            };
            (
                HashClipboardRequest {
                    mount_generation: self.mount_generation,
                    session_epoch: self.session_epoch,
                    source_generation: session.generation(),
                },
                copy,
            )
        };

        self.set_editors_readonly(true, cx);
        match self.root.update(cx, |root, cx| {
            root.start_hash_clipboard_operation(
                request,
                HashClipboardOperationKind::Copy(copy),
                operation,
                cx,
            )
        }) {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                self.set_editors_readonly(false, cx);
                self.set_clipboard_start_error(format!("Could not start Copy Digest: {error}"), cx);
            }
            Err(error) => {
                self.set_editors_readonly(false, cx);
                self.set_clipboard_start_error(format!("Could not start Copy Digest: {error}"), cx);
            }
        }
        cx.notify();
    }

    fn set_clipboard_start_error(&mut self, message: String, cx: &mut Context<Self>) {
        self.session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .set_error(message);
        cx.notify();
    }

    fn choose_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.is_current_mount()
            || !hash_file_picker_admissible(
                self.clipboard_pending.kind(),
                self.file_picker_pending.load(Ordering::Acquire),
            )
        {
            return;
        }
        if let Err(error) = checked_active_dialog_owner(window) {
            self.file_picker_feedback = Some(format!("Could not open the file picker: {error}"));
            cx.notify();
            return;
        }
        if self
            .file_picker_pending
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            self.file_picker_feedback = Some("A file picker is already open.".to_owned());
            cx.notify();
            return;
        }
        let Some(operation) = self.restart_admission.admit() else {
            self.file_picker_pending.store(false, Ordering::Release);
            return;
        };
        let request = {
            let session = self
                .session
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if session.epoch() != self.session_epoch {
                self.file_picker_pending.store(false, Ordering::Release);
                return;
            }
            HashPickerRequest {
                mount_generation: self.mount_generation,
                session_epoch: self.session_epoch,
                source_generation: session.generation(),
            }
        };
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose a file to hash".into()),
        });
        let pending = Arc::clone(&self.file_picker_pending);
        let root = self.root.clone();
        match root.update(cx, |root, cx| {
            root.start_hash_file_picker(request, picker, operation, cx);
        }) {
            Ok(()) => self.file_picker_feedback = None,
            Err(error) => {
                pending.store(false, Ordering::Release);
                self.file_picker_feedback =
                    Some(format!("Could not start the file picker: {error}"));
            }
        }
        cx.notify();
    }

    fn request_rehost(&mut self, target: HashPresentationMode, cx: &mut Context<Self>) {
        if target == self.mode
            || !self.is_current_mount()
            || self.clipboard_pending.is_copying()
            || self.file_picker_pending.load(Ordering::Acquire)
        {
            return;
        }
        if self
            .rehost_pending
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        let Some(operation) = self.restart_admission.admit() else {
            self.rehost_pending.store(false, Ordering::Release);
            return;
        };
        let Some(_) = self.sync_session_draft(cx) else {
            self.rehost_pending.store(false, Ordering::Release);
            return;
        };
        self.set_editors_readonly(true, cx);
        let revision = self
            .rehost_revision
            .fetch_add(1, Ordering::AcqRel)
            .wrapping_add(1)
            .max(1);
        let root = self.root.clone();
        let mount_generation = self.mount_generation;
        let session_epoch = self.session_epoch;
        cx.spawn(async move |_, cx| {
            let _ = root.update(cx, |root, cx| {
                root.start_hash_rehost(
                    target,
                    mount_generation,
                    session_epoch,
                    revision,
                    operation,
                    cx,
                );
            });
        })
        .detach();
        cx.notify();
    }

    fn request_close(&mut self, cx: &mut Context<Self>) {
        if !self.is_current_mount() || self.clipboard_pending.is_pending() {
            return;
        }
        if self
            .rehost_pending
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        let Some(operation) = self.restart_admission.admit() else {
            self.rehost_pending.store(false, Ordering::Release);
            return;
        };
        self.set_editors_readonly(true, cx);
        let revision = self
            .rehost_revision
            .fetch_add(1, Ordering::AcqRel)
            .wrapping_add(1)
            .max(1);
        let root = self.root.clone();
        let mount_generation = self.mount_generation;
        let session_epoch = self.session_epoch;
        cx.spawn(async move |_, cx| {
            let _ = root.update(cx, |root, cx| {
                root.start_hash_close(mount_generation, session_epoch, revision, operation, cx);
            });
        })
        .detach();
        cx.notify();
    }
}

impl Render for HashGeneratorWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let resolved = appearance::current(cx);
        let metrics = resolved.metrics;
        let palette = resolved.palette;
        let (
            has_job,
            is_running,
            is_stopping,
            progress,
            digest,
            error,
            comparison,
            mode,
            algorithm,
            path,
            clipboard_byte_len,
            clipboard_feedback,
            can_copy_digest,
        ) = {
            let session = self
                .session
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            (
                session.has_job(),
                session.is_running(),
                session.is_stopping_stale_job(),
                session.progress(),
                session
                    .result()
                    .map(|result| (result.algorithm, result.digest.clone())),
                session.error().map(str::to_owned),
                session.comparison(),
                session.mode(),
                session.algorithm(),
                session.file_path().map(|path| path.display().to_string()),
                session.clipboard_byte_len(),
                session.clipboard_feedback().map(str::to_owned),
                session.can_copy_digest(),
            )
        };
        let can_interact = self.is_current_mount();
        let file_picker_pending = self.file_picker_pending.load(Ordering::Acquire);
        let clipboard_pending_kind = self.clipboard_pending.kind();
        let clipboard_copy_pending = clipboard_pending_kind == HashClipboardPendingKind::Copy;
        let clipboard_pending = clipboard_pending_kind != HashClipboardPendingKind::Idle;
        let clipboard_pending_status = hash_clipboard_pending_feedback(clipboard_pending_kind);
        let picker_feedback = if file_picker_pending {
            Some("Choose a file to hash…".to_owned())
        } else {
            self.file_picker_feedback.clone()
        };
        let presentation_feedback = self
            .presentation_feedback
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let progress_label = progress.map(|progress| match progress.total_bytes {
            Some(total) => {
                let percent = if total == 0 {
                    0
                } else {
                    progress
                        .bytes_processed
                        .saturating_mul(100)
                        .saturating_div(total)
                        .min(100)
                };
                format!(
                    "Hashing · {} / {} bytes ({percent}%)",
                    progress.bytes_processed, total
                )
            }
            None => format!(
                "Hashing · {} bytes processed (total unknown)",
                progress.bytes_processed
            ),
        });
        let comparison_label = comparison.map(|comparison| match comparison {
            Ok(HashComparison::NotRequested) => (
                "Enter an expected digest to compare".to_owned(),
                palette.primary_text,
            ),
            Ok(HashComparison::Match) => ("Expected digest matches".to_owned(), palette.success),
            Ok(HashComparison::Mismatch) => {
                ("Expected digest does not match".to_owned(), palette.warning)
            }
            Err(error) => (format!("Expected digest: {error}"), palette.error),
        });
        let source_name = match mode {
            HashInputMode::Text => "Text",
            HashInputMode::File => "File",
            HashInputMode::Clipboard => "Clipboard",
        };
        let algorithm_name = algorithm.name();

        div()
            .id("hash-generator-tool-scroll")
            .flex()
            .size_full()
            .p(px(metrics.window_padding))
            .overflow_y_scroll()
            .track_focus(&self.focus_handle)
            .key_context("HashGeneratorWindow")
            .on_action(cx.listener(|this, _: &CalculateHash, _, cx| {
                this.calculate_hash(cx);
            }))
            .on_action(cx.listener(|this, _: &DockHashGenerator, _, cx| {
                this.request_rehost(HashPresentationMode::Docked, cx);
            }))
            .on_action(cx.listener(|this, _: &PopoutHashGenerator, _, cx| {
                this.request_rehost(HashPresentationMode::Popout, cx);
            }))
            .on_action(cx.listener(|this, _: &CloseHashGenerator, _, cx| {
                this.request_close(cx);
            }))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_y(px(metrics.item_spacing_y))
                    .w_full()
                    .p(px(metrics.panel_padding))
                    .bg(appearance::to_hsla(palette.panel_surface))
                    .rounded(px(metrics.corner_radius))
                    .text_color(appearance::to_hsla(palette.primary_text))
                    .text_size(px(metrics.body_text_size))
                    .line_height(px(metrics.line_height))
                    .child(
                        Label::new(format!("{source_name} · {algorithm_name}"))
                            .text_size(px(metrics.heading_text_size))
                            .line_height(px(metrics.line_height)),
                    )
                    .child(
                        Select::new(&self.source)
                            .accessibility_label("Hash input source")
                            .placeholder("Choose source")
                            .disabled(!can_interact || file_picker_pending || clipboard_copy_pending),
                    )
                    .child(
                        Button::new("load-hash-clipboard")
                            .label("Load Clipboard")
                            .tab_stop(true)
                            .disabled(!can_interact || clipboard_pending)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.load_clipboard(cx);
                            })),
                    )
                    .when(mode == HashInputMode::Text, |this| {
                        this.child(
                            Textarea::new(&self.text)
                                .aria_label("Text to hash")
                                .tab_index(0)
                                .readonly(!can_interact || clipboard_copy_pending),
                        )
                    })
                    .when(mode == HashInputMode::Clipboard, |this| {
                        let snapshot_status = clipboard_byte_len.map_or_else(
                            || "No Clipboard snapshot loaded".to_owned(),
                            |bytes| format!("Loaded Clipboard snapshot · {bytes} bytes"),
                        );
                        this.child(
                            Label::new(snapshot_status)
                                .id("hash-clipboard-snapshot-status")
                                .text_size(px(metrics.small_text_size))
                                .line_height(px(metrics.line_height)),
                        )
                    })
                    .when(mode == HashInputMode::File, |this| {
                        let selected_path = path
                            .clone()
                            .unwrap_or_else(|| "No file selected".to_owned());
                        this.child(
                            Label::new(format!("File: {selected_path}"))
                                .text_size(px(metrics.small_text_size))
                                .line_height(px(metrics.line_height)),
                        )
                            .child(
                                Button::new("choose-hash-file")
                                    .label("Choose File")
                                    .tab_stop(true)
                                    .disabled(!can_interact || file_picker_pending || clipboard_copy_pending)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.choose_file(window, cx);
                                    })),
                            )
                    })
                    .child(
                        Textarea::new(&self.expected)
                            .aria_label("Expected digest")
                            .tab_index(0)
                            .readonly(!can_interact || clipboard_copy_pending),
                    )
                    .children(
                        comparison_label.map(|(label, color)| {
                            Label::new(label)
                                .id("hash-comparison-status")
                                .text_color(appearance::to_hsla(color))
                                .text_size(px(metrics.small_text_size))
                                .line_height(px(metrics.line_height))
                        }),
                    )
                    .child(
                        Select::new(&self.algorithm)
                            .accessibility_label("Hash algorithm")
                            .disabled(!can_interact || clipboard_copy_pending),
                    )
                    .children(
                        algorithm.security_notice().map(|notice| {
                            Label::new(notice)
                                .id("hash-algorithm-warning")
                                .text_color(appearance::to_hsla(palette.warning))
                                .text_size(px(metrics.small_text_size))
                                .line_height(px(metrics.line_height))
                        }),
                    )
                    .child(
                        Label::new("Tab moves through controls · Space or Enter opens a selector")
                            .text_size(px(metrics.small_text_size))
                            .line_height(px(metrics.line_height)),
                    )
                    .child(
                        Label::new("Ctrl+Enter: calculate · Ctrl+Shift+D: dock · Ctrl+Shift+P: pop out · Ctrl+W: close")
                            .text_size(px(metrics.small_text_size))
                            .line_height(px(metrics.line_height)),
                    )
                    .child(
                        Button::new("calculate-hash")
                            .label("Calculate")
                            .tab_stop(true)
                            .disabled(has_job || !can_interact || clipboard_copy_pending)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.calculate_hash(cx);
                            })),
                    )
                    .child(
                        Button::new("dock-hash-generator")
                            .label("Dock (Ctrl+Shift+D)")
                            .tab_stop(true)
                            .disabled(!can_interact || clipboard_copy_pending || self.mode == HashPresentationMode::Docked)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.request_rehost(HashPresentationMode::Docked, cx);
                            })),
                    )
                    .child(
                        Button::new("popout-hash-generator")
                            .label("Pop Out (Ctrl+Shift+P)")
                            .tab_stop(true)
                            .disabled(!can_interact || clipboard_copy_pending || self.mode == HashPresentationMode::Popout)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.request_rehost(HashPresentationMode::Popout, cx);
                            })),
                    )
                    .child(
                        Button::new("close-hash-generator")
                            .label("Close Hash Generator")
                            .tab_stop(true)
                            .disabled(!can_interact || clipboard_pending)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.request_close(cx);
                            })),
                    )
                    .children(progress_label.map(|label| {
                        Label::new(label)
                            .id("hash-progress-status")
                            .text_size(px(metrics.small_text_size))
                            .line_height(px(metrics.line_height))
                    }))
                    .children(is_stopping.then(|| {
                        Label::new("Stopping the previous calculation…")
                            .id("hash-stopping-status")
                            .text_color(appearance::to_hsla(palette.warning))
                            .text_size(px(metrics.small_text_size))
                            .line_height(px(metrics.line_height))
                    }))
                    .children(digest.clone().map(|(algorithm, digest)| {
                        Label::new(format!("{}: {digest}", algorithm.name()))
                            .id("hash-digest-result")
                            .text_color(appearance::to_hsla(palette.success))
                            .text_size(px(metrics.body_text_size))
                            .line_height(px(metrics.line_height))
                    }))
                    .when(can_copy_digest && digest.is_some(), |this| {
                        this.child(
                            Button::new("copy-hash-digest")
                                .label("Copy Digest")
                                .tab_stop(true)
                                .disabled(!can_interact || clipboard_pending || file_picker_pending)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.copy_digest(cx);
                                })),
                        )
                    })
                    .children(error.map(|error| {
                        Label::new(error)
                            .id("hash-error-status")
                            .text_color(appearance::to_hsla(palette.error))
                            .text_size(px(metrics.small_text_size))
                            .line_height(px(metrics.line_height))
                    }))
                    .children(picker_feedback.map(|feedback| {
                        Label::new(feedback)
                            .id("hash-picker-status")
                            .text_color(appearance::to_hsla(palette.warning))
                            .text_size(px(metrics.small_text_size))
                            .line_height(px(metrics.line_height))
                    }))
                    .children(clipboard_pending_status.map(|feedback| {
                        Label::new(feedback)
                            .id("hash-clipboard-operation-status")
                            .text_color(appearance::to_hsla(palette.warning))
                            .text_size(px(metrics.small_text_size))
                            .line_height(px(metrics.line_height))
                    }))
                    .children(clipboard_feedback.map(|feedback| {
                        Label::new(feedback)
                            .id("hash-clipboard-feedback-status")
                            .text_size(px(metrics.small_text_size))
                            .line_height(px(metrics.line_height))
                    }))
                    .children(
                        presentation_feedback.map(|feedback| {
                            Label::new(feedback).id("hash-presentation-status")
                                .text_size(px(metrics.small_text_size))
                                .line_height(px(metrics.line_height))
                        }),
                    )
                    .children(self.native_geometry_error.as_ref().map(|error| {
                        Label::new(error.clone())
                            .id("hash-native-geometry-status")
                            .text_color(appearance::to_hsla(palette.warning))
                            .text_size(px(metrics.small_text_size))
                            .line_height(px(metrics.line_height))
                    }))
                    .children(self.size_write_error.as_ref().map(|error| {
                        Label::new(error.clone())
                            .id("hash-size-write-status")
                            .text_color(appearance::to_hsla(palette.error))
                            .text_size(px(metrics.small_text_size))
                            .line_height(px(metrics.line_height))
                    }))
                    .children(is_running.then(|| {
                        Button::new("cancel-hash")
                            .label("Cancel")
                            .tab_stop(true)
                            .disabled(!can_interact || clipboard_copy_pending)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.cancel_hash(cx);
                            }))
                    })),
            )
    }
}

struct SecondaryWindow {
    query: Entity<InputState>,
    native_window: Option<NativeWindow>,
    secondary_revision: Arc<AtomicU64>,
    restart_admission: Arc<RestartAdmission>,
}

impl Render for SecondaryWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .size_full()
            .items_center()
            .justify_center()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .w(px(420.))
                    .child(Label::new("Retained window input"))
                    .child(
                        Input::new(&self.query)
                            .aria_label("Retained window input")
                            .readonly(self.restart_admission.is_fenced()),
                    )
                    .child(
                        Button::new("dismiss-retained-window")
                            .label("Dismiss window")
                            .on_click(cx.listener(|this, _, _, cx| {
                                let Some(operation) = this.restart_admission.admit() else {
                                    return;
                                };
                                if let Some(native_window) = this.native_window {
                                    let secondary_revision = Arc::clone(&this.secondary_revision);
                                    let revision =
                                        secondary_revision.fetch_add(1, Ordering::AcqRel) + 1;
                                    cx.spawn(async move |_, _cx| {
                                        let _operation = operation;
                                        if secondary_revision.load(Ordering::Acquire) != revision {
                                            return;
                                        }
                                        if let Err(error) =
                                            set_native_window_visible(native_window, false)
                                        {
                                            tracing::warn!(%error, "could not dismiss retained GPUI window");
                                        }
                                    })
                                    .detach();
                                }
                            })),
                    ),
            )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ForegroundIdentity {
    hwnd: usize,
    process_id: u32,
    thread_id: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct NativeWindow {
    identity: ForegroundIdentity,
}

fn checked_geometry_target(window: NativeWindow) -> Option<CheckedNativeWindow> {
    Some(CheckedNativeWindow {
        hwnd: window.identity.hwnd,
        process_id: window.identity.process_id,
        thread_id: window.identity.thread_id,
    })
}

fn outer_size_in_points(metrics: NativeWindowMetrics) -> Option<PreferredWindowSize> {
    if !metrics.scale_factor.is_finite() || metrics.scale_factor <= 0.0 {
        return None;
    }
    PreferredWindowSize::new(
        metrics.outer.width() as f32 / metrics.scale_factor,
        metrics.outer.height() as f32 / metrics.scale_factor,
    )
    .ok()
}

#[cfg(windows)]
fn checked_native_window(window: &Window) -> Result<Option<NativeWindow>, String> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows::Win32::{
        Foundation::HWND,
        UI::WindowsAndMessaging::{GetWindowThreadProcessId, IsWindow},
    };

    let handle = HasWindowHandle::window_handle(window)
        .map_err(|error| format!("GPUI did not expose a native window handle: {error}"))?;
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return Err("GPUI returned a non-Windows native window handle".to_owned());
    };
    let hwnd = HWND(handle.hwnd.get() as *mut c_void);
    if hwnd.0.is_null() || !unsafe { IsWindow(hwnd) }.as_bool() {
        return Err("GPUI returned an invalid Windows window handle".to_owned());
    }

    let mut process_id = 0;
    let thread_id = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
    if process_id != std::process::id() || thread_id == 0 {
        return Err("GPUI window handle is not owned by this process".to_owned());
    }

    Ok(Some(NativeWindow {
        identity: ForegroundIdentity {
            hwnd: hwnd.0 as usize,
            process_id,
            thread_id,
        },
    }))
}

#[cfg(not(windows))]
fn checked_native_window(_: &Window) -> Result<Option<NativeWindow>, String> {
    Ok(None)
}

fn checked_active_dialog_owner(window: &Window) -> Result<Option<NativeWindow>, String> {
    let native_window = checked_native_window(window)?;
    #[cfg(windows)]
    {
        use windows::Win32::UI::Input::KeyboardAndMouse::GetActiveWindow;

        let Some(native_window) = native_window else {
            return Err("The Hash Generator window has no checked Windows owner".to_owned());
        };
        let active = native_identity(unsafe { GetActiveWindow() });
        if active != Some(native_window.identity) {
            return Err(
                "Activate this Hash Generator window before opening its file picker".to_owned(),
            );
        }
    }
    Ok(native_window)
}

#[cfg(windows)]
fn current_foreground_identity() -> Option<ForegroundIdentity> {
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
    let hwnd = unsafe { GetForegroundWindow() };
    native_identity(hwnd)
}

#[cfg(not(windows))]
fn current_foreground_identity() -> Option<ForegroundIdentity> {
    None
}

#[cfg(windows)]
fn native_identity(hwnd: windows::Win32::Foundation::HWND) -> Option<ForegroundIdentity> {
    use windows::Win32::UI::WindowsAndMessaging::{GetWindowThreadProcessId, IsWindow};
    if hwnd.0.is_null() || !unsafe { IsWindow(hwnd) }.as_bool() {
        return None;
    }
    let mut process_id = 0;
    let thread_id = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
    (process_id != 0 && thread_id != 0).then_some(ForegroundIdentity {
        hwnd: hwnd.0 as usize,
        process_id,
        thread_id,
    })
}

#[cfg(windows)]
fn capture_foreground_identity() -> Option<ForegroundIdentity> {
    let foreground = current_foreground_identity()?;
    if foreground.process_id == std::process::id() {
        return Some(foreground);
    }

    multi_launcher::active_window::capture_external_foreground_window()?;
    (current_foreground_identity() == Some(foreground)).then_some(foreground)
}

#[cfg(not(windows))]
fn capture_foreground_identity() -> Option<ForegroundIdentity> {
    None
}

#[cfg(windows)]
fn native_window_is_valid(window: NativeWindow) -> bool {
    use windows::Win32::{
        Foundation::HWND,
        UI::WindowsAndMessaging::{GetWindowThreadProcessId, IsWindow},
    };
    let hwnd = HWND(window.identity.hwnd as *mut c_void);
    if hwnd.0.is_null() || !unsafe { IsWindow(hwnd) }.as_bool() {
        return false;
    }
    let mut process_id = 0;
    let thread_id = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
    process_id == window.identity.process_id && thread_id == window.identity.thread_id
}

#[cfg(not(windows))]
fn native_window_is_valid(_: NativeWindow) -> bool {
    false
}

#[cfg(windows)]
fn native_window_is_visible(window: NativeWindow) -> Result<bool, String> {
    use windows::Win32::{Foundation::HWND, UI::WindowsAndMessaging::IsWindowVisible};
    if !native_window_is_valid(window) {
        return Err("the GPUI window handle is stale".to_owned());
    }
    Ok(unsafe { IsWindowVisible(HWND(window.identity.hwnd as *mut c_void)) }.as_bool())
}

#[cfg(not(windows))]
fn native_window_is_visible(_: NativeWindow) -> Result<bool, String> {
    Err("native GPUI window visibility is only available on Windows".to_owned())
}

#[cfg(windows)]
fn set_native_window_visible(window: NativeWindow, visible: bool) -> Result<(), String> {
    use windows::Win32::{
        Foundation::HWND,
        UI::WindowsAndMessaging::{
            IsWindowVisible, SHOW_WINDOW_CMD, SW_HIDE, SW_SHOWNOACTIVATE, ShowWindow,
        },
    };
    if !native_window_is_valid(window) {
        return Err("the GPUI window handle is stale".to_owned());
    }
    let hwnd = HWND(window.identity.hwnd as *mut c_void);
    let command: SHOW_WINDOW_CMD = if visible { SW_SHOWNOACTIVATE } else { SW_HIDE };
    // SAFETY: the HWND was captured from GPUI and revalidated against this process and thread.
    unsafe {
        let _ = ShowWindow(hwnd, command);
    }
    if !native_window_is_valid(window) || unsafe { IsWindowVisible(hwnd) }.as_bool() != visible {
        return Err("Windows did not apply the requested GPUI visibility".to_owned());
    }
    Ok(())
}

#[cfg(not(windows))]
fn set_native_window_visible(_: NativeWindow, _: bool) -> Result<(), String> {
    Err("native GPUI window visibility is only available on Windows".to_owned())
}

#[cfg(windows)]
fn activate_native_window(window: NativeWindow) -> Result<(), String> {
    use windows::Win32::{Foundation::HWND, UI::WindowsAndMessaging::SetForegroundWindow};
    if !native_window_is_valid(window) {
        return Err("the GPUI window handle is stale".to_owned());
    }
    // One guarded USER32 attempt avoids an asynchronous retry that could steal
    // focus after the foreground has changed.
    if unsafe { SetForegroundWindow(HWND(window.identity.hwnd as *mut c_void)) }.as_bool() {
        Ok(())
    } else {
        Err("Windows refused to activate the GPUI window".to_owned())
    }
}

#[cfg(not(windows))]
fn activate_native_window(_: NativeWindow) -> Result<(), String> {
    Err("native GPUI window activation is only available on Windows".to_owned())
}

#[derive(Clone)]
struct RootVisibility {
    visible: Arc<AtomicBool>,
    request_revision: Arc<AtomicU64>,
    active: Arc<AtomicBool>,
    #[cfg(windows)]
    request_sender: Arc<Mutex<Option<Sender<()>>>>,
    #[cfg(windows)]
    requested_visible: Arc<Mutex<Option<bool>>>,
    #[cfg(windows)]
    requested_hash_focus: Arc<AtomicU64>,
}

impl RootVisibility {
    fn new() -> Self {
        Self {
            visible: Arc::new(AtomicBool::new(true)),
            request_revision: Arc::new(AtomicU64::new(0)),
            active: Arc::new(AtomicBool::new(false)),
            #[cfg(windows)]
            request_sender: Arc::new(Mutex::new(None)),
            #[cfg(windows)]
            requested_visible: Arc::new(Mutex::new(None)),
            #[cfg(windows)]
            requested_hash_focus: Arc::new(AtomicU64::new(0)),
        }
    }

    #[cfg(windows)]
    fn bind_request_sender(&self, sender: Sender<()>) {
        if let Ok(mut slot) = self.request_sender.lock() {
            *slot = Some(sender);
        }
    }

    fn request_visible(&self, visible: bool) -> bool {
        #[cfg(windows)]
        {
            if !self.active.load(Ordering::Acquire) {
                return false;
            }
            if let Ok(mut pending) = self.requested_visible.lock() {
                *pending = Some(visible);
            } else {
                return false;
            }
            if let Ok(sender) = self.request_sender.lock()
                && sender
                    .as_ref()
                    .is_some_and(|sender| sender.send(()).is_ok())
            {
                return true;
            }
            if let Ok(mut pending) = self.requested_visible.lock() {
                *pending = None;
            }
            false
        }
        #[cfg(not(windows))]
        {
            let _ = visible;
            false
        }
    }

    fn request_hash_visible(&self, mount_generation: u64) -> bool {
        #[cfg(windows)]
        {
            self.requested_hash_focus
                .store(mount_generation, Ordering::Release);
            if self.request_visible(true) {
                return true;
            }
            let _ = self.requested_hash_focus.compare_exchange(
                mount_generation,
                0,
                Ordering::AcqRel,
                Ordering::Acquire,
            );
            false
        }
        #[cfg(not(windows))]
        {
            let _ = mount_generation;
            false
        }
    }

    #[cfg(windows)]
    fn take_requested_visible(&self) -> Option<bool> {
        self.requested_visible
            .lock()
            .ok()
            .and_then(|mut pending| pending.take())
    }

    #[cfg(windows)]
    fn take_requested_hash_focus(&self) -> Option<u64> {
        match self.requested_hash_focus.swap(0, Ordering::AcqRel) {
            0 => None,
            generation => Some(generation),
        }
    }

    fn set_visible(&self, visible: bool) -> (u64, bool) {
        let revision = self
            .request_revision
            .fetch_add(1, Ordering::AcqRel)
            .wrapping_add(1)
            .max(1);
        self.visible.store(visible, Ordering::Release);
        (revision, visible)
    }

    fn next_toggle(&self) -> (u64, bool) {
        let visible = !self.visible.load(Ordering::Acquire);
        self.set_visible(visible)
    }

    fn is_current(&self, revision: u64, visible: bool) -> bool {
        self.active.load(Ordering::Acquire)
            && self.request_revision.load(Ordering::Acquire) == revision
            && self.visible.load(Ordering::Acquire) == visible
    }

    fn revert_if_current(&self, revision: u64, visible: bool) {
        if self.request_revision.load(Ordering::Acquire) == revision
            && self.visible.load(Ordering::Acquire) == visible
        {
            self.visible.store(!visible, Ordering::Release);
        }
    }
}

#[cfg(windows)]
fn activate_for_root_request(
    visibility: &RootVisibility,
    revision: u64,
    desired_visible: bool,
    target: NativeWindow,
    expected_foreground: Option<ForegroundIdentity>,
) {
    if !visibility.is_current(revision, desired_visible) || !native_window_is_valid(target) {
        return;
    }
    let foreground = current_foreground_identity();
    if foreground == Some(target.identity) {
        return;
    }
    let Some(expected_foreground) = expected_foreground else {
        return;
    };
    if foreground != Some(expected_foreground) {
        return;
    }
    if !visibility.is_current(revision, desired_visible) {
        return;
    }
    if let Err(error) = activate_native_window(target) {
        tracing::debug!(%error, "GPUI foreground activation was refused");
    }
}

#[cfg(windows)]
fn activate_for_secondary_window(
    secondary_revision: &AtomicU64,
    revision: u64,
    window: NativeWindow,
    expected_foreground: Option<ForegroundIdentity>,
) {
    if secondary_revision.load(Ordering::Acquire) != revision || !native_window_is_valid(window) {
        return;
    }
    let foreground = current_foreground_identity();
    if foreground == Some(window.identity) {
        return;
    }
    let Some(expected_foreground) = expected_foreground else {
        return;
    };
    if foreground != Some(expected_foreground)
        || secondary_revision.load(Ordering::Acquire) != revision
    {
        return;
    }
    if let Err(error) = activate_native_window(window) {
        tracing::debug!(%error, "retained GPUI window activation was refused");
    }
}

#[cfg(not(windows))]
fn activate_for_secondary_window(
    _: &AtomicU64,
    _: u64,
    _: NativeWindow,
    _: Option<ForegroundIdentity>,
) {
}

pub(crate) fn run_foundation_window(
    hotkey: multi_launcher::hotkey::Hotkey,
    enabled_plugins: Option<HashSet<String>>,
    enabled_capabilities: Option<HashMap<String, Vec<String>>>,
    search_startup: LauncherSearchStartup,
    coordinate_tool_preferences: CoordinateToolPreferences,
    theme_settings: multi_launcher::settings::ThemeSettings,
    settings_path: String,
    saved_utility_presentations: UtilityPresentations,
    saved_utility_sizes: UtilityDialogSizes,
    settings_save_workers: SettingsSaveWorkerRegistry,
) -> Result<Option<RestartPlan>, String> {
    let launcher_search_catalog = LauncherSearchCatalog::new(search_startup);
    let hash_tool_key = UtilityToolKey::parse("hash-generator")
        .map_err(|error| format!("Hash Generator has no stable presentation key: {error}"))?;
    let utility_presentations = UtilityPresentationStore::new(
        settings_path.clone(),
        settings_save_workers.clone(),
        saved_utility_presentations,
    );
    let mut utility_sizes = UtilitySizeStore::new(
        settings_path,
        settings_save_workers.clone(),
        saved_utility_sizes,
    );
    utility_sizes.track_key(hash_tool_key);
    let hash_docked_preferred_size = match utility_sizes.preference(hash_tool_key) {
        multi_launcher::settings::UtilitySizePreference::Preferred(size) => size,
        _ => HASH_DOCK_FACTORY_SIZE,
    };
    let hash_docked_preferred_size_for_root = hash_docked_preferred_size;
    let utility_size_store = Arc::new(Mutex::new(utility_sizes));
    let utility_size_store_after_run = Arc::clone(&utility_size_store);
    let utility_size_polling = Arc::new(AtomicBool::new(false));
    let utility_size_polling_for_root = Arc::clone(&utility_size_polling);
    let hash_geometry_sequence = Arc::new(AtomicU64::new(0));
    let hash_geometry_sequence_for_root = Arc::clone(&hash_geometry_sequence);
    let restart_context = RestartContext::capture().ok();
    let restart_exit = Rc::new(RefCell::new(None));
    let restart_exit_after_run = Rc::clone(&restart_exit);
    let startup_error = Rc::new(RefCell::new(None));
    let startup_error_sink = Rc::clone(&startup_error);
    let subscriptions = Rc::new(RefCell::new(Vec::new()));
    let subscription_sink = Rc::clone(&subscriptions);
    let root_for_quit = Rc::new(RefCell::new(None::<WeakEntity<RootWindow>>));
    let root_for_quit_callback = Rc::clone(&root_for_quit);
    let listener_slot = Arc::new(Mutex::new(None::<HotkeyListener>));
    let listener_slot_for_run = Arc::clone(&listener_slot);
    let listener_slot_for_quit = Arc::clone(&listener_slot);
    let listener_slot_for_root = Arc::clone(&listener_slot);
    let restart_admission = Arc::new(RestartAdmission::default());
    let restart_admission_for_quit = Arc::clone(&restart_admission);
    let restart_admission_for_root = Arc::clone(&restart_admission);
    let restart_admission_for_hotkey = Arc::clone(&restart_admission);
    let visibility = RootVisibility::new();
    let visibility_for_quit = visibility.clone();
    let visibility_for_root = visibility.clone();
    let restart_retiring = Arc::new(AtomicBool::new(false));
    let restart_retiring_for_root = Arc::clone(&restart_retiring);
    let restart_retiring_for_root_close = Arc::clone(&restart_retiring);
    let restart_failure = Rc::new(RefCell::new(None));
    let restart_failure_after_run = Rc::clone(&restart_failure);
    let initial_return_target = capture_foreground_identity();
    let hash_session = Arc::new(Mutex::new(HashSession::new()));
    let hash_session_for_quit = Arc::clone(&hash_session);
    let hash_session_for_root = Arc::clone(&hash_session);
    let hash_window_revision = Arc::new(AtomicU64::new(0));
    let hash_window_revision_for_app = Arc::clone(&hash_window_revision);
    let hash_mount_generation = Arc::new(AtomicU64::new(0));
    let hash_mount_generation_for_quit = Arc::clone(&hash_mount_generation);
    let hash_mount_generation_for_root = Arc::clone(&hash_mount_generation);
    let hash_rehost_pending = Arc::new(AtomicBool::new(false));
    let hash_rehost_pending_for_quit = Arc::clone(&hash_rehost_pending);
    let hash_rehost_pending_for_root = Arc::clone(&hash_rehost_pending);
    let hash_rehost_revision = Arc::new(AtomicU64::new(0));
    let hash_rehost_revision_for_root = Arc::clone(&hash_rehost_revision);
    let hash_presentation_feedback = Arc::new(Mutex::new(None));
    let hash_presentation_feedback_for_root = Arc::clone(&hash_presentation_feedback);
    let hash_file_picker_pending = Arc::new(AtomicBool::new(false));
    let hash_file_picker_pending_for_quit = Arc::clone(&hash_file_picker_pending);
    let hash_file_picker_pending_for_root = Arc::clone(&hash_file_picker_pending);
    let hash_clipboard_pending = Arc::new(HashClipboardPendingState::default());
    let hash_clipboard_pending_for_root = Arc::clone(&hash_clipboard_pending);
    let hash_wake_signal = Arc::new(HashWakeSignal::default());
    let hash_wake_signal_for_quit = Arc::clone(&hash_wake_signal);
    let hash_wake_signal_for_root = Arc::clone(&hash_wake_signal);
    let hash_wake_signal_for_task = Arc::clone(&hash_wake_signal);
    let hash_wake_signal_for_root_close = Arc::clone(&hash_wake_signal);
    let coordinate_hud_probe_controller =
        Arc::new(Mutex::new(CoordinateToolController::new_with_preferences(
            Arc::new(NativeCoordinateRuntimeFactory),
            coordinate_tool_preferences,
        )));
    let coordinate_hud_probe_closing = Arc::new(AtomicBool::new(false));
    let coordinate_shutdown_complete = Arc::new(AtomicBool::new(false));
    let coordinate_shutdown_complete_for_root = Arc::clone(&coordinate_shutdown_complete);
    let controller_for_app = Arc::clone(&coordinate_hud_probe_controller);
    let closing_for_app = Arc::clone(&coordinate_hud_probe_closing);
    let utility_presentations_after_run = utility_presentations.clone();

    let appearance_settings_for_run = theme_settings.clone();
    application().with_assets(assets::Assets).run(move |cx| {
        init(cx);
        let initial_appearance = appearance::system_appearance(cx.window_appearance());
        appearance::apply_saved_theme(
            &appearance_settings_for_run,
            initial_appearance,
            cx,
        );
        cx.bind_keys([
            KeyBinding::new("ctrl-enter", ToggleSecondaryWindow, Some("RootWindow")),
            KeyBinding::new("f6", ToggleCoordinateHudProbe, Some("RootWindow")),
            KeyBinding::new("ctrl-g", OpenHashGenerator, Some("RootWindow")),
            KeyBinding::new("ctrl-shift-r", FocusDockedHashResize, Some("RootWindow")),
            KeyBinding::new("ctrl-shift-e", RestartWithEgui, Some("RootWindow")),
            KeyBinding::new("ctrl-up", SearchHistoryOlder, Some("LauncherSearch")),
            KeyBinding::new("ctrl-down", SearchHistoryNewer, Some("LauncherSearch")),
            KeyBinding::new("ctrl-enter", CalculateHash, Some("HashGeneratorWindow")),
            KeyBinding::new(
                "ctrl-shift-d",
                DockHashGenerator,
                Some("HashGeneratorWindow"),
            ),
            KeyBinding::new(
                "ctrl-shift-p",
                PopoutHashGenerator,
                Some("HashGeneratorWindow"),
            ),
            KeyBinding::new("ctrl-w", CloseHashGenerator, Some("HashGeneratorWindow")),
        ]);
        let coordinate_hud_probe_closing_for_quit = Arc::clone(&closing_for_app);
        let hash_session_for_quit = Arc::clone(&hash_session_for_quit);
        let hash_wake_signal_for_quit = Arc::clone(&hash_wake_signal_for_quit);
        let hash_window_revision_for_quit = Arc::clone(&hash_window_revision_for_app);
        let restart_admission_for_quit = Arc::clone(&restart_admission_for_quit);
        let quit_subscription = cx.on_app_quit(move |app| {
            if let Some(root) = root_for_quit_callback.borrow().clone() {
                let _ = root.update(app, |root, cx| {
                    root.drain_hash_native_geometry(cx, true);
                });
            }
            visibility_for_quit.active.store(false, Ordering::Release);
            restart_admission_for_quit.fence();
            coordinate_hud_probe_closing_for_quit.store(true, Ordering::Release);
            hash_wake_signal_for_quit.close();
            hash_window_revision_for_quit.fetch_add(1, Ordering::AcqRel);
            hash_session_for_quit
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .shutdown();
            hash_mount_generation_for_quit.store(0, Ordering::Release);
            hash_rehost_pending_for_quit.store(false, Ordering::Release);
            hash_file_picker_pending_for_quit.store(false, Ordering::Release);
            if let Ok(mut listener) = listener_slot_for_quit.lock()
                && let Some(mut listener) = listener.take()
            {
                listener.stop();
            }
            async {}
        });
        subscription_sink.borrow_mut().push(quit_subscription);

        let options = WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some("Multi Launcher GPUI Foundation".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let controller_for_root = Arc::clone(&controller_for_app);
        let closing_for_root = Arc::clone(&closing_for_app);
        let utility_sizes_for_root = Arc::clone(&utility_size_store);
        let utility_size_polling_for_root = Arc::clone(&utility_size_polling_for_root);
        let hash_geometry_sequence_for_root = Arc::clone(&hash_geometry_sequence_for_root);
        let session_for_root = Arc::clone(&hash_session_for_root);
        let wake_signal_for_root = Arc::clone(&hash_wake_signal_for_root);
        let picker_pending_for_root_close = Arc::clone(&hash_file_picker_pending_for_root);
        let clipboard_pending_for_root_close = Arc::clone(&hash_clipboard_pending_for_root);
        let window_revision_for_root = Arc::clone(&hash_window_revision_for_app);
        let restart_exit_for_root = Rc::clone(&restart_exit);
        let restart_failure_for_root = Rc::clone(&restart_failure);
        let restart_context_for_root = restart_context.clone();
        let theme_settings_for_root = theme_settings.clone();
        let appearance_subscription_sink = Rc::clone(&subscription_sink);
        let (root_window, root_view) = match open_window(options, cx, move |window, cx| {
            if appearance::should_observe_system(&theme_settings_for_root) {
                let settings = theme_settings_for_root.clone();
                let appearance_subscription = window.observe_window_appearance(move |window, cx| {
                    appearance::apply_saved_theme(
                        &settings,
                        appearance::system_appearance(window.appearance()),
                        cx,
                    );
                });
                appearance_subscription_sink
                    .borrow_mut()
                    .push(appearance_subscription);
            }
            let query = cx.new(|cx| InputState::new(window, cx).placeholder("Type a query"));
            query.focus_handle(cx).focus(window, cx);
            let focus_handle = cx.focus_handle();
            focus_handle.focus(window, cx);
            let resize_focus_handle = cx.focus_handle();
            let search_catalog_for_root = launcher_search_catalog;
            let root = cx.new(move |_| RootWindow {
                root_window: None,
                listener_slot: Arc::clone(&listener_slot_for_root),
                root_visibility: visibility_for_root.clone(),
                query,
                enabled_plugins: enabled_plugins.clone(),
                enabled_capabilities: enabled_capabilities.clone(),
                search_catalog: search_catalog_for_root,
                launcher_results: LauncherSearchResults::default(),
                query_history:
                    QueryHistoryNavigator::default(),
                fresh_search_enter: None,
                native_window: None,
                focus_handle,
                resize_focus_handle,
                root_scroll: ScrollHandle::new(),
                utility_presentations: utility_presentations.clone(),
                utility_size_store: Arc::clone(&utility_sizes_for_root),
                utility_size_polling: Arc::clone(&utility_size_polling_for_root),
                hash_docked_preferred_size: hash_docked_preferred_size_for_root,
                hash_docked_resize: Arc::new(Mutex::new(None)),
                hash_geometry_sequence: Arc::clone(&hash_geometry_sequence_for_root),
                hash_tool_key,
                hash_session: session_for_root,
                hash_wake_signal: wake_signal_for_root,
                hash_file_picker_pending: hash_file_picker_pending_for_root,
                hash_clipboard_pending: hash_clipboard_pending_for_root,
                hash_clipboard_operation: None,
                hash_clipboard_operation_counter: 0,
                hash_window: HashWindowState::Closed,
                hash_window_revision: window_revision_for_root,
                hash_mount_generation: hash_mount_generation_for_root,
                hash_mount_counter: 0,
                hash_rehost_pending: hash_rehost_pending_for_root,
                hash_rehost_revision: hash_rehost_revision_for_root,
                hash_window_close_subscriptions: Vec::new(),
                hash_retiring_windows: Vec::new(),
                hash_presentation_feedback: hash_presentation_feedback_for_root,
                hash_open_error: None,
                hash_presentation_error: None,
                hash_native_geometry_error: None,
                hash_size_write_error: None,
                secondary: SecondaryWindowState::Closed,
                secondary_revision: Arc::new(AtomicU64::new(0)),
                secondary_close_subscription: None,
                secondary_error: None,
                coordinate_hud_probe_controller: controller_for_root,
                coordinate_hud_probe_closing: closing_for_root,
                coordinate_hud_probe_pending: false,
                coordinate_hud_probe_status: "Coordinate HUD probe is idle.".to_owned(),
                coordinate_shutdown_complete: Arc::clone(&coordinate_shutdown_complete_for_root),
                restart_context: restart_context_for_root,
                restart_admission: Arc::clone(&restart_admission_for_root),
                restart_retiring: Arc::clone(&restart_retiring_for_root),
                restart_status: None,
                restart_exit: restart_exit_for_root,
                restart_failure: restart_failure_for_root,
            });
            let search_subscription = root.update(cx, |root, cx| {
                let query = root.query.clone();
                cx.subscribe(
                    &query,
                    |root: &mut RootWindow,
                     query: Entity<InputState>,
                     event: &InputEvent,
                     cx: &mut Context<RootWindow>| {
                        match event {
                            InputEvent::Change => {
                                let query_text = query.read(cx).value().to_string();
                                root.query_history.synchronize(&query_text);
                                root.refresh_launcher_results(query_text);
                                cx.notify();
                            }
                            InputEvent::PressEnter { secondary, shift } => {
                                let marker = root.fresh_search_enter.take();
                                if *secondary || *shift {
                                    return;
                                }
                                let query_text = query.read(cx).value().to_string();
                                let Some(marker) = marker.filter(|marker| {
                                    marker.generation == root.launcher_results.generation
                                        && marker.query == query_text
                                }) else {
                                    return;
                                };
                                let Some((generation, index)) =
                                    root.launcher_results.enter_target(&query_text)
                                else {
                                    return;
                                };
                                if generation != marker.generation {
                                    return;
                                }
                                let Some(root_window) = root.root_window else {
                                    return;
                                };
                                let root_view = cx.entity();
                                cx.spawn(async move |_, cx| {
                                    let _ = cx.update(|app| {
                                        let _ = update_kit_window_content(
                                            root_window,
                                            &root_view,
                                            app,
                                            |root, window, cx| {
                                                let Some(_search_operation) =
                                                    root.search_input_operation(window, cx)
                                                else {
                                                    return;
                                                };
                                                let query_is_current = root
                                                    .query
                                                    .read(cx)
                                                    .value()
                                                    .as_ref()
                                                    == query_text;
                                                if query_is_current
                                                    && root.launcher_results.generation
                                                        == generation
                                                {
                                                    root.activate_launcher_result(
                                                        generation,
                                                        index,
                                                        ActivationSource::Enter,
                                                        window,
                                                        cx,
                                                    );
                                                }
                                            },
                                        );
                                    });
                                })
                                .detach();
                            }
                            InputEvent::Blur => root.fresh_search_enter = None,
                            _ => {}
                        }
                    },
                )
            });
            appearance_subscription_sink
                .borrow_mut()
                .push(search_subscription);
            let subscription = root.update(cx, |root, cx| {
                let resize_focus_handle = root.resize_focus_handle.clone();
                cx.on_blur(&resize_focus_handle, window, |root, _, cx| {
                    if root.cancel_docked_hash_preview() {
                        cx.notify();
                    }
                })
            });
            appearance_subscription_sink
                .borrow_mut()
                .push(subscription);
            let root_for_window_close = root.downgrade();
            window.on_window_should_close(cx, move |_, app| {
                let clipboard_message =
                    hash_clipboard_close_message(clipboard_pending_for_root_close.kind());
                let picker_pending = picker_pending_for_root_close.load(Ordering::Acquire);
                if clipboard_message.is_none() && !picker_pending {
                    return true;
                }
                let _ = root_for_window_close.update(app, |root, cx| {
                    root.restart_status = Some(clipboard_message.map_or_else(
                        || "Finish or cancel the Hash Generator file picker before closing Multi Launcher.".to_owned(),
                        str::to_owned,
                    ));
                    cx.notify();
                });
                false
            });
            root
        }) {
            Ok((window, view)) => (window, view),
            Err(error) => {
                *startup_error_sink.borrow_mut() = Some(error.to_string());
                cx.quit();
                return;
            }
        };
        *root_for_quit.borrow_mut() = Some(root_view.downgrade());

        let native_window =
            match update_kit_window_content(root_window, &root_view, cx, |root, window, cx| {
                let native_window = checked_native_window(window)?;
                root.native_window = native_window;
                root.root_window = Some(root_window);
                root.query.focus_handle(cx).focus(window, cx);
                Ok::<_, String>(native_window)
            }) {
                Ok(Ok(native_window)) if !cfg!(windows) || native_window.is_some() => native_window,
                Ok(Ok(_)) => {
                    *startup_error_sink.borrow_mut() = Some(
                        "GPUI did not expose a checked Windows window handle for the launcher."
                            .to_owned(),
                    );
                    cx.quit();
                    return;
                }
                Ok(Err(error)) => {
                    *startup_error_sink.borrow_mut() = Some(error);
                    cx.quit();
                    return;
                }
                Err(error) => {
                    *startup_error_sink.borrow_mut() = Some(format!(
                        "GPUI could not initialize its root window: {error}"
                    ));
                    cx.quit();
                    return;
                }
            };

        let root_id = root_window.window_id();
        let root_for_window_close = root_view.downgrade();
        let hash_window_revision_for_root_close = Arc::clone(&hash_window_revision_for_app);
        let hash_mount_generation_for_root_close = Arc::clone(&hash_mount_generation);
        let hash_rehost_pending_for_root_close = Arc::clone(&hash_rehost_pending);
        let hash_rehost_revision_for_root_close = Arc::clone(&hash_rehost_revision);
        subscription_sink
            .borrow_mut()
            .push(cx.on_window_closed(move |cx, closed| {
                if closed == root_id {
                    let _ = root_for_window_close.update(cx, |root, cx| {
                        root.drain_hash_native_geometry(cx, true);
                    });
                    hash_wake_signal_for_root_close.close();
                    hash_window_revision_for_root_close.fetch_add(1, Ordering::AcqRel);
                    hash_mount_generation_for_root_close.store(0, Ordering::Release);
                    hash_rehost_pending_for_root_close.store(false, Ordering::Release);
                    hash_rehost_revision_for_root_close.fetch_add(1, Ordering::AcqRel);
                    if !restart_retiring_for_root_close.load(Ordering::Acquire) {
                        cx.quit();
                    }
                }
            }));

        let hash_root_view = root_view.downgrade();
        cx.spawn(async move |cx| {
            while hash_wake_signal_for_task.wait().await {
                let _ = cx.update(|cx| {
                    let live_window = hash_root_view.update(cx, |root, cx| {
                        root.drain_hash_native_geometry(cx, false);
                        root.hash_session
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .poll();
                        match &root.hash_window {
                            HashWindowState::Closed => None,
                            HashWindowState::Docked { view, .. } => Some((None, view.clone())),
                            HashWindowState::Popout { window, view, .. } => {
                                Some((Some(*window), view.clone()))
                            }
                        }
                    });
                    if let Ok(Some((window, view))) = live_window {
                        if let Some(window) = window {
                            let _ = update_kit_window_content(window, &view, cx, |_, _, cx| {
                                cx.notify();
                            });
                        } else {
                            view.update(cx, |_, cx| cx.notify());
                        }
                    }
                });
            }
        })
        .detach();

        #[cfg(windows)]
        {
            let Some(native_window) = native_window else {
                *startup_error_sink.borrow_mut() = Some(
                    "GPUI did not provide a Windows window handle for the launcher.".to_owned(),
                );
                cx.quit();
                return;
            };
            let (event_tx, event_rx) = channel();
            visibility.bind_request_sender(event_tx.clone());
            let trigger = Arc::new(HotkeyTrigger::new(hotkey));
            let listener = match HotkeyTrigger::prepare_listener(
                vec![Arc::clone(&trigger)],
                "gpui-frontend",
                event_tx,
            ) {
                Ok(listener) => listener,
                Err(error) => {
                    *startup_error_sink.borrow_mut() = Some(format!(
                        "Could not start the configured launcher hotkey: {error}"
                    ));
                    cx.quit();
                    return;
                }
            };
            if let Ok(mut slot) = listener_slot_for_run.lock() {
                *slot = Some(listener);
            } else {
                *startup_error_sink.borrow_mut() =
                    Some("Could not initialize the GPUI hotkey owner.".to_owned());
                cx.quit();
                return;
            }
            if let Ok(slot) = listener_slot_for_run.lock()
                && let Some(listener) = slot.as_ref()
            {
                listener.activate();
            }
            visibility.active.store(true, Ordering::Release);

            let root_window = root_window;
            let root_view = root_view;
            let visibility = visibility.clone();
            cx.spawn(async move |cx| {
                let mut event_rx: Receiver<()> = event_rx;
                let mut return_target = initial_return_target;
                loop {
                    let (receiver, event) = cx
                        .background_executor()
                        .spawn(async move {
                            let event = event_rx.recv();
                            (event_rx, event)
                        })
                        .await;
                    event_rx = receiver;
                    if event.is_err() || !visibility.active.load(Ordering::Acquire) {
                        break;
                    }
                    let Some(_operation) = restart_admission_for_hotkey.admit() else {
                        break;
                    };

                    let (revision, desired_visible, hash_focus_generation) =
                        if let Some(requested_visible) = visibility.take_requested_visible() {
                            let hash_focus_generation = visibility.take_requested_hash_focus();
                            let (revision, visible) = visibility.set_visible(requested_visible);
                            (revision, visible, hash_focus_generation)
                        } else if trigger.take() {
                            let (revision, visible) = visibility.next_toggle();
                            (revision, visible, None)
                        } else {
                            continue;
                        };
                    let foreground = if desired_visible {
                        cx.background_executor()
                            .spawn(async { capture_foreground_identity() })
                            .await
                    } else {
                        None
                    };
                    if !visibility.is_current(revision, desired_visible) {
                        continue;
                    }
                    let query_window = update_kit_window_content(
                        root_window,
                        &root_view,
                        &mut *cx,
                        |root, window, cx| {
                            if desired_visible {
                                let docked_mount_generation = match &root.hash_window {
                                    HashWindowState::Docked {
                                        mount_generation, ..
                                    } => Some(*mount_generation),
                                    _ => None,
                                };
                                let hash_view = if docked_hash_focus_is_current(
                                    hash_focus_generation,
                                    root.hash_mount_generation.load(Ordering::Acquire),
                                    docked_mount_generation,
                                ) {
                                    root.hash_window.view().cloned()
                                } else {
                                    None
                                };
                                if let Some(view) = hash_view {
                                    view.update(cx, |tool, cx| {
                                        tool.focus_editable(window, cx);
                                    });
                                } else {
                                    root.query.focus_handle(cx).focus(window, cx);
                                }
                            } else {
                                root.cancel_docked_hash_preview();
                            }
                            root.native_window
                        },
                    );
                    if !matches!(query_window, Ok(Some(current)) if current == native_window) {
                        tracing::warn!("GPUI root window closed before hotkey presentation");
                        cx.update(|cx| cx.quit());
                        break;
                    }

                    let foreground_before_transition = current_foreground_identity();
                    let should_restore = !desired_visible
                        && foreground_before_transition == Some(native_window.identity);
                    let show_request_is_current = desired_visible
                        && foreground
                            .is_some_and(|captured| Some(captured) == foreground_before_transition);
                    if desired_visible {
                        return_target = foreground
                            .filter(|captured| Some(*captured) == foreground_before_transition)
                            .filter(|captured| captured.hwnd != native_window.identity.hwnd);
                    } else if !should_restore {
                        return_target = None;
                    }

                    if let Err(error) = set_native_window_visible(native_window, desired_visible) {
                        visibility.revert_if_current(revision, desired_visible);
                        tracing::error!(%error, "could not change GPUI root window visibility");
                        continue;
                    }
                    let foreground_after_transition = current_foreground_identity();
                    let show_request_is_current = show_request_is_current
                        && foreground_after_transition == foreground_before_transition;

                    let target = if desired_visible {
                        show_request_is_current.then_some(native_window)
                    } else if should_restore
                        && foreground_after_transition == Some(native_window.identity)
                    {
                        return_target.map(|identity| NativeWindow { identity })
                    } else {
                        None
                    };
                    if let Some(target) = target {
                        let expected_foreground = foreground_after_transition;
                        let visibility = visibility.clone();
                        cx.background_executor()
                            .spawn(async move {
                                activate_for_root_request(
                                    &visibility,
                                    revision,
                                    desired_visible,
                                    target,
                                    expected_foreground,
                                );
                            })
                            .await;
                    }
                }
            })
            .detach();
        }

        #[cfg(not(windows))]
        {
            let _ = (native_window, hotkey, initial_return_target);
        }
    });

    let restart_exit = restart_exit_after_run.borrow_mut().take();

    hash_wake_signal.close();
    hash_window_revision.fetch_add(1, Ordering::AcqRel);
    hash_session
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .shutdown();

    coordinate_hud_probe_closing.store(true, Ordering::Release);
    let coordinate_shutdown_result = if coordinate_shutdown_complete.load(Ordering::Acquire) {
        Ok(())
    } else {
        coordinate_hud_probe_controller
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .shutdown()
    };
    if let Err(error) = &coordinate_shutdown_result {
        if restart_exit.is_none() {
            tracing::warn!(%error, "could not fully shut down the GPUI Coordinate HUD probe");
        }
    }

    if let Ok(mut listener) = listener_slot.lock()
        && let Some(mut listener) = listener.take()
    {
        listener.stop();
    }

    let settings_save_receipt = settings_save_workers.join_all_checked();
    let (utility_size_failures, utility_size_pending) = {
        let mut store = utility_size_store_after_run
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let _ = store.poll();
        (
            store.take_new_write_failures(),
            store.has_pending_acknowledgement(),
        )
    };
    for failure in utility_size_failures {
        tracing::error!(tool = failure.key.as_str(), error = %failure.message, "could not save utility window geometry during GPUI shutdown");
    }
    for (key, error) in utility_presentations_after_run.poll() {
        tracing::error!(tool = key.as_str(), %error, "could not save utility presentation preference during GPUI shutdown");
    }

    if let Some(error) = startup_error.borrow_mut().take() {
        return Err(format!(
            "GPUI could not open its window: {error}. Restart with {} to use the fallback frontend.",
            Frontend::Egui.selector_argument()
        ));
    }
    if let Some(error) = restart_failure_after_run.borrow_mut().take() {
        return Err(error);
    }

    if restart_exit.is_some() {
        let status = utility_size_store_after_run
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .status(hash_tool_key);
        if utility_size_pending || status.pending_revision.is_some() {
            return Err(
                "The Hash Generator geometry save did not settle before restart. No egui process was started; reopen GPUI and retry after the save finishes."
                    .to_owned(),
            );
        }
        if let Some((_, error)) = status.failure {
            return Err(format!(
                "The Hash Generator geometry save failed before restart. No egui process was started: {error}"
            ));
        }
        if let Err(error) = settings_save_receipt.ensure_clean() {
            return Err(format!(
                "The final Settings drain did not complete: {error}. No egui process was started. Resolve the reported persistence problem and rerun Multi Launcher from the same working directory."
            ));
        }
    }

    let Some((context, native_windows)) = restart_exit else {
        return Ok(None);
    };
    let coordinate_shutdown_result = coordinate_shutdown_result.map_err(|error| {
        format!(
            "The Coordinate HUD did not shut down cleanly; no egui process was started: {error}"
        )
    });
    #[cfg(windows)]
    let windows_retired = !native_windows
        .iter()
        .any(|window| native_window_is_valid(*window));
    #[cfg(not(windows))]
    let windows_retired = {
        let _ = native_windows;
        true
    };

    crate::gpui_restart::plan_after_teardown(
        context,
        Frontend::Egui,
        coordinate_shutdown_result,
        windows_retired,
    )
    .map(Some)
}

#[cfg(test)]
mod tests {
    use super::{
        ActivationSource, AppliedWindowSize, CommandOutcome, DockedHashResize,
        DockedHashResizeOrigin, Frontend, FrontendSelection, FutureContext,
        HashClipboardPendingKind, HashClipboardPendingState, HashClipboardRequest, HashDraft,
        HashPickerOutcome, HashPresentationMode, HashWakeSignal, LauncherSearchCatalog,
        LauncherSearchResults, LauncherSearchStartup, Modifiers, Poll, PreferredWindowSize,
        classify_hash_picker_result, dispatch_hash_search_invocation,
        docked_hash_available_size, docked_hash_fit, docked_hash_focus_is_current,
        docked_hash_resize_accessibility_label, docked_hash_resize_identity_matches,
        docked_resize_commit_size, duplicate_instance_message, fresh_search_enter_marker,
        hash_clipboard_copy_admissible, hash_clipboard_request_is_current,
        hash_file_picker_admissible, hash_mount_identity_matches, hash_picker_result_is_current,
        is_fresh_plain_search_enter, keyboard_docked_resize_preview, launcher_result_row_id,
        parse_hash_search_invocation, pointer_docked_resize_preview, result_click_source,
        root_search_focus_is_current, search_input_is_current, selection_from_args,
        single_line_history_queries, supported_hash_search_actions,
    };
    use gpui_kit::{point, px};
    use multi_launcher::{
        hash_generator::{HashAlgorithm, parse_expected_digest},
        settings::RememberedPresentation,
    };
    use std::collections::HashMap;
    use std::ffi::OsString;
    use std::future::Future;
    use std::path::PathBuf;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use std::task::{Wake, Waker};

    #[derive(Default)]
    struct WakeCounter(AtomicUsize);

    impl Wake for WakeCounter {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn args(args: &[&str]) -> Vec<OsString> {
        args.iter().map(|arg| OsString::from(*arg)).collect()
    }

    fn hash_action() -> multi_launcher::actions::Action {
        let hash_generator = multi_launcher::plugins::hash_generator::HashGeneratorPlugin;
        multi_launcher::plugin::search_filtered_plugins(
            [&hash_generator as &dyn multi_launcher::plugin::Plugin],
            "hash",
            None,
            None,
        )
        .into_iter()
        .next()
        .expect("the built-in Hash Generator route is installed")
    }

    #[test]
    fn launcher_results_follow_egui_enter_selection_and_reject_stale_rows() {
        let first = hash_action();
        let second = multi_launcher::actions::Action {
            label: "Second installed route".into(),
            desc: "Fixture row".into(),
            action: "hash:open".into(),
            args: Some("beta argument".into()),
        };
        let mut results = LauncherSearchResults::default();
        results.replace("hash".into(), vec![first.clone()]);
        let sole_generation = results.generation;
        assert_eq!(results.enter_target("hash"), Some((sole_generation, 0)));

        results.replace("hash".into(), vec![first.clone(), second.clone()]);
        let rendered_generation = results.generation;
        assert_eq!(results.enter_target("hash"), None);
        assert!(results.select_down());
        assert!(results.select_down());
        assert_eq!(results.enter_target("hash"), Some((rendered_generation, 1)));
        assert_eq!(
            results.resolve_action(rendered_generation, 1, "hash"),
            Some(second.clone())
        );
        let selected = results
            .resolve_action(rendered_generation, 1, "hash")
            .expect("the selected absolute row resolves in its current generation");
        let invocation = parse_hash_search_invocation(selected, ActivationSource::Enter)
            .expect("the persisted row resolves through the installed typed route");
        assert_eq!(invocation.original_action, second);
        assert_eq!(invocation.source, ActivationSource::Enter);

        results.replace("hash changed".into(), vec![first]);
        assert_eq!(results.selected, None);
        assert_eq!(
            results.resolve_action(rendered_generation, 0, "hash changed"),
            None,
            "a row rendered for an older result generation must not activate"
        );
        assert_eq!(results.enter_target("hash"), None);

        let stale_generation = results.generation;
        results.replace("hash changed".into(), vec![hash_action()]);
        assert_eq!(
            results.resolve_action(stale_generation, 0, "hash changed"),
            None,
            "replacement at the same query and index still rejects the old row generation"
        );
    }

    #[test]
    fn launcher_search_arrows_navigate_and_invalidate_equal_query_rows() {
        let first = hash_action();
        let second = multi_launcher::actions::Action {
            label: "Second installed route".into(),
            desc: "Fixture row".into(),
            action: "hash:open".into(),
            args: Some("second argument".into()),
        };
        let actions = vec![first, second];
        let mut results = LauncherSearchResults::default();
        results.replace("app QMarker".into(), actions.clone());

        assert!(!results.select_up());
        assert!(results.select_down());
        assert_eq!(results.selected, Some(0));
        assert!(results.select_down());
        assert_eq!(results.selected, Some(1));
        assert!(!results.select_down());
        assert_eq!(results.selected, Some(1));
        assert!(results.select_up());
        assert_eq!(results.selected, Some(0));
        assert!(results.select_up());
        assert_eq!(results.selected, None);
        assert_eq!(results.enter_target("app QMarker"), None);

        let previous_generation = results.generation;
        assert!(results.select_down());
        assert_eq!(results.selected, Some(0));
        results.replace("app QMarker".into(), actions);
        assert_eq!(results.generation, previous_generation + 1);
        assert_eq!(results.selected, None);
        assert_eq!(results.enter_target("app QMarker"), None);
    }

    #[test]
    fn gpui_history_snapshot_preserves_single_line_unicode_and_filters_newlines() {
        assert_eq!(
            single_line_history_queries([
                " app QMarker 東京 ",
                "query\nwith line break",
                "query\rwith carriage return",
                "hash",
            ]),
            vec![" app QMarker 東京 ", "hash"]
        );
    }

    #[test]
    fn search_history_actions_require_the_visible_live_focused_root() {
        assert!(search_input_is_current(true, true, true, true, true));
        assert!(!search_input_is_current(false, true, true, true, true));
        assert!(!search_input_is_current(true, false, true, true, true));
        assert!(!search_input_is_current(true, true, false, true, true));
        assert!(!search_input_is_current(true, true, true, false, true));
        assert!(!search_input_is_current(true, true, true, true, false));
    }

    #[test]
    fn gpui_app_catalog_keeps_distinct_hash_arguments_and_omits_unsupported_rows() {
        let alpha = multi_launcher::actions::Action {
            label: "Alpha QMarker".into(),
            desc: "Hash generator".into(),
            action: "hash:open".into(),
            args: Some("alpha argument".into()),
        };
        let beta = multi_launcher::actions::Action {
            label: "Beta QMarker".into(),
            desc: "Hash generator".into(),
            action: "hash:open".into(),
            args: Some("beta argument".into()),
        };
        let unsupported = multi_launcher::actions::Action {
            label: "Unsupported QMarker".into(),
            desc: "Other feature".into(),
            action: "query:settings".into(),
            args: None,
        };
        let catalog = LauncherSearchCatalog::new(LauncherSearchStartup {
            actions: vec![alpha.clone(), beta.clone(), unsupported.clone()],
            match_exact: true,
            fuzzy_weight: 0.75,
            usage_weight: 0.0,
            usage: HashMap::new(),
            folder_aliases_lc: HashMap::new(),
            bookmark_aliases_lc: HashMap::new(),
        });

        assert_eq!(catalog.actions, vec![alpha.clone(), beta.clone()]);
        let composed = catalog.compose("app QMarker", Vec::new());
        assert_eq!(
            composed,
            vec![alpha.clone(), beta.clone()],
            "same command strings do not collapse actions with distinct arguments"
        );
        let mut results = LauncherSearchResults::default();
        results.replace("app QMarker".into(), composed);
        assert_eq!(results.enter_target("app QMarker"), None);
        assert!(results.select_down());
        assert!(results.select_down());
        let (generation, index) = results
            .enter_target("app QMarker")
            .expect("Down selects the second persisted row for fresh Enter");
        assert_eq!(index, 1);
        let action = results
            .resolve_action(generation, index, "app QMarker")
            .expect("the current query, generation and absolute position resolve Beta");
        let invocation = parse_hash_search_invocation(action, ActivationSource::Enter)
            .expect("Beta resolves through the installed typed Hash Open route");
        assert_eq!(invocation.original_action, beta);
        assert_eq!(invocation.source, ActivationSource::Enter);
        assert!(
            catalog.compose("", Vec::new()).is_empty(),
            "clearing GPUI Search keeps the existing blank result scope"
        );
        assert_eq!(supported_hash_search_actions(vec![unsupported]), Vec::new());
    }

    #[test]
    fn launcher_row_ids_include_generation_and_absolute_position() {
        assert_ne!(launcher_result_row_id(9, 0), launcher_result_row_id(9, 1));
        assert_ne!(launcher_result_row_id(9, 1), launcher_result_row_id(10, 1));
    }

    #[test]
    fn search_enter_accepts_only_fresh_plain_keydown() {
        assert!(is_fresh_plain_search_enter(
            true,
            "enter",
            Modifiers::default(),
            false,
        ));
        assert!(!is_fresh_plain_search_enter(
            true,
            "enter",
            Modifiers::default(),
            true,
        ));
        assert!(!is_fresh_plain_search_enter(
            true,
            "enter",
            Modifiers::control(),
            false,
        ));
        assert!(!is_fresh_plain_search_enter(
            false,
            "enter",
            Modifiers::default(),
            false,
        ));
        assert!(!is_fresh_plain_search_enter(
            true,
            "down",
            Modifiers::default(),
            false,
        ));

        let marker =
            fresh_search_enter_marker(true, "enter", Modifiers::default(), false, 7, "hash".into())
                .unwrap();
        assert_eq!(marker.generation, 7);
        assert_eq!(marker.query, "hash");
    }

    #[test]
    fn search_activation_preserves_source_and_dispatches_hash_open_once() {
        let mut action = hash_action();
        action.args = Some("retained action arguments".into());
        assert_eq!(result_click_source(true), ActivationSource::Enter);
        assert_eq!(result_click_source(false), ActivationSource::Click);
        for source in [ActivationSource::Enter, ActivationSource::Click] {
            let invocation = parse_hash_search_invocation(action.clone(), source).unwrap();
            assert_eq!(invocation.original_action, action);
            assert_eq!(invocation.query_override, None);
            assert_eq!(invocation.source, source);

            let mut opened = 0;
            let outcome = dispatch_hash_search_invocation(invocation, || {
                opened += 1;
                Ok(())
            })
            .unwrap();
            assert_eq!(opened, 1);
            assert_eq!(outcome, CommandOutcome::default());
        }

        let invalid = multi_launcher::actions::Action {
            label: "Not Hash".into(),
            desc: "Other route".into(),
            action: "query:settings".into(),
            args: None,
        };
        assert!(parse_hash_search_invocation(invalid, ActivationSource::Click).is_err());
    }

    #[test]
    fn defaults_to_egui_and_preserves_other_arguments() {
        assert_eq!(
            selection_from_args(args(&["--open", "settings"])),
            Ok(FrontendSelection::DefaultEgui)
        );
    }

    #[test]
    fn selects_each_explicit_frontend() {
        assert_eq!(
            selection_from_args(args(&["--ui=egui"])),
            Ok(FrontendSelection::Explicit(Frontend::Egui))
        );
        assert_eq!(
            selection_from_args(args(&["--ui=gpui"])),
            Ok(FrontendSelection::Explicit(Frontend::Gpui))
        );
    }

    #[test]
    fn rejects_invalid_and_repeated_frontend_selectors() {
        assert!(selection_from_args(args(&["--ui=unknown"])).is_err());
        assert!(selection_from_args(args(&["--ui"])).is_err());
        assert!(selection_from_args(args(&["--ui=egui", "--ui=gpui"])).is_err());
        assert!(selection_from_args(args(&["--ui=egui", "--ui=egui"])).is_err());
    }

    #[test]
    fn duplicate_launch_diagnostic_is_silent_only_for_default_selection() {
        assert_eq!(
            duplicate_instance_message(FrontendSelection::DefaultEgui),
            None
        );
        assert_eq!(
            duplicate_instance_message(FrontendSelection::Explicit(Frontend::Egui)).as_deref(),
            Some(
                "The data directory is already owned by another Multi Launcher instance. Close that instance, then rerun with --ui=egui from the same working directory."
            )
        );
        assert_eq!(
            duplicate_instance_message(FrontendSelection::Explicit(Frontend::Gpui)).as_deref(),
            Some(
                "The data directory is already owned by another Multi Launcher instance. Close that instance, then rerun with --ui=gpui from the same working directory."
            )
        );
    }

    #[test]
    fn hash_mount_defaults_docked_and_restores_saved_popout() {
        assert_eq!(
            HashPresentationMode::from_saved(None),
            HashPresentationMode::Docked
        );
        assert_eq!(
            HashPresentationMode::from_saved(Some(RememberedPresentation::Popout)),
            HashPresentationMode::Popout
        );
        assert_eq!(
            HashPresentationMode::from_saved(Some(RememberedPresentation::Docked)),
            HashPresentationMode::Docked
        );
    }

    #[test]
    fn docked_hash_fit_is_transient_and_handles_viewports_below_settings_limits() {
        let preferred = PreferredWindowSize::new(620.0, 440.0).unwrap();
        let available = docked_hash_available_size(40.0, 30.0, 12.0);
        assert_eq!(
            available,
            AppliedWindowSize {
                width: 16.0,
                height: 6.0
            }
        );
        assert_eq!(
            docked_hash_fit(preferred, available),
            AppliedWindowSize {
                width: 16.0,
                height: 6.0
            }
        );
        assert_eq!(preferred.width, 620.0);
        assert_eq!(preferred.height, 440.0);
    }

    #[test]
    fn docked_hash_resize_callbacks_require_the_active_docked_mount_and_session() {
        assert!(docked_hash_resize_identity_matches(7, 7, 3, 3, true));
        assert!(!docked_hash_resize_identity_matches(8, 7, 3, 3, true));
        assert!(!docked_hash_resize_identity_matches(7, 7, 4, 3, true));
        assert!(!docked_hash_resize_identity_matches(7, 7, 3, 3, false));
        assert!(!docked_hash_resize_identity_matches(0, 0, 3, 3, true));
    }

    #[test]
    fn docked_pointer_resize_commits_one_changed_axis_and_respects_applied_fit() {
        let preferred = PreferredWindowSize::new(620.0, 440.0).unwrap();
        let visible = AppliedWindowSize {
            width: 430.0,
            height: 300.0,
        };
        let (preview, changed_width, changed_height) =
            pointer_docked_resize_preview(preferred, visible, 2.0, 0.0);
        assert_eq!(preview, PreferredWindowSize::new(432.0, 440.0).unwrap());
        assert!(changed_width);
        assert!(!changed_height);

        let constrained = DockedHashResize {
            mount_generation: 7,
            session_epoch: 3,
            start_size: preferred,
            available_at_start: visible,
            preview_size: preview,
            changed_width,
            changed_height,
            drag_started: true,
            origin: DockedHashResizeOrigin::Pointer {
                position: point(px(0.0), px(0.0)),
                visible_size: visible,
            },
        };
        assert_eq!(docked_resize_commit_size(constrained), None);

        let click_without_drag = DockedHashResize {
            mount_generation: 7,
            session_epoch: 3,
            start_size: preferred,
            available_at_start: visible,
            preview_size: preferred,
            changed_width: false,
            changed_height: false,
            drag_started: false,
            origin: DockedHashResizeOrigin::Pointer {
                position: point(px(0.0), px(0.0)),
                visible_size: visible,
            },
        };
        assert_eq!(docked_resize_commit_size(click_without_drag), None);

        let room = AppliedWindowSize {
            width: 900.0,
            height: 700.0,
        };
        let visible = docked_hash_fit(preferred, room);
        let (preview, changed_width, changed_height) =
            pointer_docked_resize_preview(preferred, visible, 32.0, 0.0);
        let committed = docked_resize_commit_size(DockedHashResize {
            mount_generation: 7,
            session_epoch: 3,
            start_size: preferred,
            available_at_start: room,
            preview_size: preview,
            changed_width,
            changed_height,
            drag_started: true,
            origin: DockedHashResizeOrigin::Pointer {
                position: point(px(0.0), px(0.0)),
                visible_size: visible,
            },
        });
        assert_eq!(
            committed,
            Some(PreferredWindowSize::new(652.0, 440.0).unwrap())
        );
    }

    #[test]
    fn docked_keyboard_resize_keeps_the_untouched_legacy_axis() {
        let preferred = PreferredWindowSize::new(400.0, 440.0).unwrap();
        let preview = keyboard_docked_resize_preview(preferred, 0.0, 16.0);
        assert_eq!(preview, PreferredWindowSize::new(400.0, 456.0).unwrap());
        let available = AppliedWindowSize {
            width: 8192.0,
            height: 8192.0,
        };
        assert_eq!(
            docked_resize_commit_size(DockedHashResize {
                mount_generation: 9,
                session_epoch: 2,
                start_size: preferred,
                available_at_start: available,
                preview_size: preview,
                changed_width: false,
                changed_height: true,
                drag_started: false,
                origin: DockedHashResizeOrigin::Keyboard,
            }),
            Some(PreferredWindowSize::new(400.0, 456.0).unwrap())
        );

        let clipped = AppliedWindowSize {
            width: 430.0,
            height: 300.0,
        };
        let clipped_preference = PreferredWindowSize::new(620.0, 440.0).unwrap();
        assert_eq!(
            docked_resize_commit_size(DockedHashResize {
                mount_generation: 9,
                session_epoch: 2,
                start_size: clipped_preference,
                available_at_start: clipped,
                preview_size: keyboard_docked_resize_preview(clipped_preference, 16.0, 0.0),
                changed_width: true,
                changed_height: false,
                drag_started: false,
                origin: DockedHashResizeOrigin::Keyboard,
            }),
            None,
            "a keyboard preview that remains clamped to the same root-fit width must not persist"
        );
    }

    #[test]
    fn docked_resize_accessibility_label_reports_current_and_preview_dimensions() {
        let label = docked_hash_resize_accessibility_label(
            PreferredWindowSize::new(620.0, 440.0).unwrap(),
            AppliedWindowSize {
                width: 430.0,
                height: 300.0,
            },
            PreferredWindowSize::new(636.0, 456.0).unwrap(),
            AppliedWindowSize {
                width: 430.0,
                height: 300.0,
            },
            true,
        );
        assert!(label.contains("Current preferred size: 620 by 440 logical points"));
        assert!(label.contains("displayed 430 by 300"));
        assert!(label.contains("Preview size: 636 by 456 logical points"));
        assert!(label.contains("Enter to save, Escape to discard"));
    }

    #[test]
    fn hash_mount_callbacks_require_current_mount_and_session() {
        assert!(hash_mount_identity_matches(4, 4, 7, 7));
        assert!(!hash_mount_identity_matches(5, 4, 7, 7));
        assert!(!hash_mount_identity_matches(4, 4, 8, 7));
        assert!(!hash_mount_identity_matches(0, 0, 7, 7));
    }

    #[test]
    fn hash_file_picker_completion_requires_mount_session_and_source_generation() {
        assert!(hash_picker_result_is_current(4, 4, 7, 7, 11, 11));
        assert!(!hash_picker_result_is_current(5, 4, 7, 7, 11, 11));
        assert!(!hash_picker_result_is_current(4, 4, 8, 7, 11, 11));
        assert!(!hash_picker_result_is_current(4, 4, 7, 7, 12, 11));
    }

    #[test]
    fn hash_clipboard_completion_requires_mount_session_and_source_generation() {
        let request = HashClipboardRequest {
            mount_generation: 4,
            session_epoch: 7,
            source_generation: 11,
        };
        assert!(hash_clipboard_request_is_current(4, 7, 11, request, false));
        assert!(!hash_clipboard_request_is_current(5, 7, 11, request, false));
        assert!(!hash_clipboard_request_is_current(4, 8, 11, request, false));
        assert!(!hash_clipboard_request_is_current(4, 7, 12, request, false));
        assert!(!hash_clipboard_request_is_current(4, 7, 11, request, true));
    }

    #[test]
    fn hash_clipboard_slot_coalesces_and_copy_excludes_picker_in_both_orders() {
        let pending = HashClipboardPendingState::default();
        assert!(pending.begin(HashClipboardPendingKind::Load));
        assert!(!pending.begin(HashClipboardPendingKind::Copy));
        pending.finish();
        assert!(pending.begin(HashClipboardPendingKind::Copy));
        assert!(!pending.begin(HashClipboardPendingKind::Load));
        pending.finish();

        assert!(hash_clipboard_copy_admissible(
            HashClipboardPendingKind::Idle,
            false,
        ));
        assert!(!hash_clipboard_copy_admissible(
            HashClipboardPendingKind::Idle,
            true,
        ));
        assert!(!hash_clipboard_copy_admissible(
            HashClipboardPendingKind::Load,
            false,
        ));
        assert!(!hash_clipboard_copy_admissible(
            HashClipboardPendingKind::Copy,
            false,
        ));
        assert!(hash_file_picker_admissible(
            HashClipboardPendingKind::Idle,
            false,
        ));
        assert!(!hash_file_picker_admissible(
            HashClipboardPendingKind::Copy,
            false,
        ));
        assert!(!hash_file_picker_admissible(
            HashClipboardPendingKind::Idle,
            true,
        ));
    }

    #[test]
    fn restart_draft_keeps_only_opaque_clipboard_snapshot_identity() {
        let mut session = multi_launcher::hash_generator::HashSession::new();
        session.apply_clipboard_load(Ok("secret".to_owned()));
        let first = HashDraft::from_session(&session);
        session.apply_clipboard_load(Ok("public".to_owned()));
        let replacement = HashDraft::from_session(&session);

        assert_ne!(first, replacement);
        assert_ne!(
            first.clipboard_snapshot_revision,
            replacement.clipboard_snapshot_revision
        );
        let serialized_for_diagnostics = format!("{first:?}");
        assert!(!serialized_for_diagnostics.contains("secret"));
        assert!(!serialized_for_diagnostics.contains("public"));
    }

    #[test]
    fn hash_file_picker_completion_classifies_selection_cancel_and_failure() {
        let selected_path = PathBuf::from("sample.txt");
        assert!(matches!(
            classify_hash_picker_result::<std::io::Error>(Ok(Ok(Some(vec![
                selected_path.clone()
            ])))),
            HashPickerOutcome::Selected(path) if path == selected_path
        ));
        assert!(matches!(
            classify_hash_picker_result::<std::io::Error>(Ok(Ok(None))),
            HashPickerOutcome::Cancelled
        ));
        assert!(matches!(
            classify_hash_picker_result::<std::io::Error>(Ok(Ok(Some(Vec::new())))),
            HashPickerOutcome::Failed(_)
        ));
        assert!(matches!(
            classify_hash_picker_result::<std::io::Error>(Ok(Err(anyhow::anyhow!(
                "native dialog failed"
            )))),
            HashPickerOutcome::Failed(message) if message.contains("native dialog failed")
        ));
        assert!(matches!(
            classify_hash_picker_result::<std::io::Error>(Err(std::io::Error::other(
                "dialog task failed"
            ))),
            HashPickerOutcome::Failed(message) if message.contains("did not return")
        ));
    }

    #[test]
    fn expected_digest_newlines_remain_invalid_internal_separators() {
        let digest = "a".repeat(64);
        let split = format!("{}\n{}", &digest[..32], &digest[32..]);
        assert!(parse_expected_digest(HashAlgorithm::Sha256, &split).is_err());
        assert!(
            parse_expected_digest(HashAlgorithm::Sha256, &format!("\r\n{digest}\t"))
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn dock_reveal_focus_is_fenced_to_the_current_mount() {
        assert!(docked_hash_focus_is_current(Some(8), 8, Some(8)));
        assert!(!docked_hash_focus_is_current(Some(8), 9, Some(8)));
        assert!(!docked_hash_focus_is_current(Some(8), 8, Some(7)));
        assert!(!docked_hash_focus_is_current(Some(8), 8, None));
        assert!(!docked_hash_focus_is_current(None, 8, Some(8)));
    }

    #[test]
    fn root_search_focus_restoration_respects_mount_and_restart_fences() {
        assert!(root_search_focus_is_current(
            Some(HashPresentationMode::Popout),
            Some(9),
            9,
            Some(HashPresentationMode::Popout),
            9,
            false,
        ));
        assert!(!root_search_focus_is_current(
            Some(HashPresentationMode::Popout),
            Some(10),
            10,
            Some(HashPresentationMode::Popout),
            9,
            false,
        ));
        assert!(root_search_focus_is_current(None, None, 0, None, 0, false));
        assert!(!root_search_focus_is_current(None, None, 0, None, 0, true,));
        assert!(!root_search_focus_is_current(
            Some(HashPresentationMode::Docked),
            Some(9),
            9,
            None,
            0,
            false,
        ));
    }

    #[test]
    fn hash_wake_coalesces_notifications_and_closes_waiter() {
        let signal = Arc::new(HashWakeSignal::default());
        let wake_counter = Arc::new(WakeCounter::default());
        let waker = Waker::from(Arc::clone(&wake_counter));
        let mut wait = Box::pin(signal.wait());
        let mut cx = FutureContext::from_waker(&waker);

        assert!(matches!(wait.as_mut().poll(&mut cx), Poll::Pending));
        signal.notify();
        signal.notify();
        assert_eq!(wake_counter.0.load(Ordering::Relaxed), 1);
        assert!(matches!(wait.as_mut().poll(&mut cx), Poll::Ready(true)));
        assert!(matches!(wait.as_mut().poll(&mut cx), Poll::Pending));
        signal.close();
        assert_eq!(wake_counter.0.load(Ordering::Relaxed), 2);
        assert!(matches!(wait.as_mut().poll(&mut cx), Poll::Ready(false)));
    }
}
