mod action_sheet;
mod actions;
mod add_action_dialog;
mod add_bookmark_dialog;
mod alias_dialog;
mod batch_rename_dialog;
mod bookmark_alias_dialog;
mod brightness_dialog;
mod calendar_event_details;
mod calendar_event_editor;
mod calendar_popover;
mod clipboard_dialog;
mod clipboard_modify_dialog;
mod color_pick;
mod command_host;
mod confirmation_modal;
mod convert_panel;
pub(crate) mod coordinate_tool;
mod cpu_list_dialog;
pub mod crop_dialog;
mod dashboard_editor_dialog;
mod data_recovery_dialog;
mod diff_dialog;
mod event_channel;
mod fav_dialog;
mod file_search_dialog;
pub mod file_search_preview_dialog;
mod font_revision;
mod hash_generator_dialog;
pub(crate) mod hotfolder_dialog;
pub(crate) mod hotfolder_dispatch;
pub(crate) mod hotfolder_runtime;
mod hotkey_directory_dialog;
#[cfg(feature = "perf-harness")]
pub use hotfolder_runtime::HotFolderIdleFixture;
mod image_compare_dialog;
#[cfg(feature = "dev-smoke-binaries")]
pub mod image_compare_smoke;
pub mod image_crop_editor;
mod image_panel;
mod indexing;
mod json_utility_dialog;
pub(crate) mod jump_mode;
pub use jump_mode::ui::JumpModeScope;
mod macro_dialog;
pub mod mkmacro_dialog;
mod mouse_gesture_settings_dialog;
mod mouse_gestures_dialog;
mod mouse_settings_dialog;
mod multi_manager_actions;
mod note_clipboard;
mod note_find;
mod note_find_preview;
mod note_graph_dialog;
pub(crate) mod note_mutation;
mod note_panel;
mod notes_dialog;
mod numpad_navigation;
mod ocr;
mod ocr_view;
mod paste_sequence_dialog;
pub(crate) mod pinboard;
mod pinboard_windows;
#[cfg(all(windows, feature = "dev-smoke-binaries"))]
pub use pinboard_windows::run_pin_opacity_smoke;
#[cfg(feature = "qa-framebuffer-capture")]
mod qa_framebuffer;
mod qr_dialog;
mod query_history;
mod query_observation;
mod radial_actions;
mod radial_editor;
mod regex_tester_dialog;
mod render;
mod screen_draw_restore;
mod screen_draw_toolbar;
pub(crate) mod screenshot_capture;
pub(crate) mod screenshot_countdown;
mod screenshot_editor;
mod search;
mod search_replace_dialog;
mod shell_cmd_dialog;
mod snippet_dialog;
mod snippet_prompt_dialog;
mod state;
mod table_utility_dialog;
mod tempfile_alias_dialog;
mod tempfile_dialog;
pub mod theme;
mod theme_settings_dialog;
mod timer_dialog;
mod toast_log_dialog;
mod todo_dialog;
mod todo_view_dialog;
#[cfg(test)]
mod track_c_action_observer;
mod track_c_query_observer;
pub(crate) mod universal_action_catalog;
mod universal_action_executor;
mod unused_assets_dialog;
pub(crate) mod volume_data;
mod volume_dialog;
mod watch;
mod xml_toolkit_dialog;
pub(crate) mod xml_toolkit_schema;
pub(crate) mod xml_toolkit_tree;
pub(crate) mod xml_toolkit_xpath;

pub use crate::settings_save::SettingsSaveWorkerRegistry;

pub(crate) use radial_editor::{
    AuthoringBindingEditorIdentity, BindingEditorScope, BindingEditorSlot, BindingEditorSurface,
};
pub(crate) use render::persist_launcher_close_preferences;
pub use state::AuthoringProviderSearchRequest;
pub(crate) use state::{AuthoringProviderSearchPurpose, AuthoringProviderTestPayload};

pub(crate) const RADIAL_DESIGNER_WINDOW_TITLE: &str = "Radial Designer";

pub use crate::brightness_service::BRIGHTNESS_QUERIES;
pub use add_action_dialog::AddActionDialog;
#[cfg(feature = "qa-framebuffer-capture")]
pub(crate) use add_action_dialog::{TargetRepairControlPaint, TargetRepairDialogPaint};
pub use add_bookmark_dialog::AddBookmarkDialog;
pub use alias_dialog::AliasDialog;
pub(crate) use batch_rename_dialog::{BatchRenameDialogState, BatchRenameUiCommand};
pub use bookmark_alias_dialog::BookmarkAliasDialog;
pub use brightness_dialog::BrightnessDialog;
pub use calendar_event_details::CalendarEventDetails;
pub use calendar_event_editor::CalendarEventEditor;
pub use calendar_popover::CalendarPopover;
pub use clipboard_dialog::ClipboardDialog;
pub(crate) use search_replace_dialog::{
    ReplacementPreviewWorker, SearchReplaceDialogState, SearchReplaceUiCommand,
};

pub use clipboard_modify_dialog::{ClipboardModifyDialogSection, ClipboardModifyDialogState};
pub use convert_panel::ConvertPanel;
pub use cpu_list_dialog::CpuListDialog;
pub(crate) use data_recovery_dialog::{
    DataRecoveryDialog, DataRecoveryUiAction, PendingRecoveryIntent, SafeStartupDiagnostic,
};
pub use diff_dialog::DiffDialogState;
pub use fav_dialog::FavDialog;
pub use file_search_dialog::{
    FileSearchDialogState, FileSearchMode, FileSearchScopeMode, FileSearchUiCommand,
};
pub use file_search_preview_dialog::FileSearchPreviewDialogState;
pub use hash_generator_dialog::HashGeneratorDialogState;
pub use image_compare_dialog::ImageCompareDialogState;
pub use image_panel::ImagePanel;
pub use json_utility_dialog::JsonUtilityDialogState;
pub use macro_dialog::MacroDialog;
pub use mkmacro_dialog::MkMacroDialog;
pub use mouse_gesture_settings_dialog::MouseGestureSettingsDialog;
pub use mouse_gestures_dialog::{GestureRecorder, MgGesturesDialog, RecorderConfig};
pub(crate) use mouse_settings_dialog::MouseSettingsDialog;
pub use note_graph_dialog::NoteGraphDialog;
pub use note_panel::{
    NotePanel, build_nvim_command, build_wezterm_command, extract_links, show_wiki_link,
    spawn_external,
};
pub use notes_dialog::NotesDialog;
pub(crate) use paste_sequence_dialog::PasteSequenceDialog;
#[cfg(feature = "qa-framebuffer-capture")]
pub use qa_framebuffer::QaFramebufferRequest;
pub use qr_dialog::QrDialogState;
pub use regex_tester_dialog::RegexTesterDialogState;
pub use screenshot_editor::{
    MarkupArrow, MarkupHistory, MarkupLayer, MarkupRect, MarkupStroke, MarkupText, MarkupTool,
    ScreenshotEditor, render_markup_layers,
};
pub use shell_cmd_dialog::ShellCmdDialog;
pub use snippet_dialog::SnippetDialog;
pub use table_utility_dialog::TableUtilityDialogState;
pub use tempfile_alias_dialog::TempfileAliasDialog;
pub use tempfile_dialog::TempfileDialog;
pub use theme_settings_dialog::ThemeSettingsDialogState;
pub use timer_dialog::{TimerCompletionDialog, TimerDialog};
pub use toast_log_dialog::ToastLogDialog;
pub use todo_dialog::TodoDialog;
pub use todo_view_dialog::{TodoViewDialog, todo_view_layout_sizes, todo_view_window_constraints};
pub use unused_assets_dialog::UnusedAssetsDialog;
pub use volume_dialog::VolumeDialog;
pub use xml_toolkit_dialog::XmlToolkitDialogState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ScreenDrawRegionOperation {
    generation: crate::screen_draw::ScreenDrawGeneration,
    operation_id: mkmacro_dialog::visual_overlay::OperationId,
}

struct ScreenshotCaptureParking {
    request_id: screenshot_capture::ScreenshotRequestId,
    transaction: crate::launcher_parking::LauncherParkingTransaction<u64>,
    visibility_revision: u64,
    was_visible: bool,
    #[cfg(feature = "qa-framebuffer-capture")]
    root_capture_safe_ack_at_unix_ms: Option<u64>,
    #[cfg(feature = "qa-framebuffer-capture")]
    parking_verified_at_unix_ms: Option<u64>,
    #[cfg(feature = "qa-framebuffer-capture")]
    presentation_granted_at_unix_ms: Option<u64>,
}

use crate::actions::folders;
use crate::actions::{Action, ActionIdLookup, load_actions_typed};
use crate::actions_editor::ActionsEditor;
use crate::clipboard_modify::coordinator::{
    ImmediateCompletionEvent, ImmediateExecutionCoordinator,
};
use crate::clipboard_modify::runtime::{ClipboardModifyRuntime, clipboard_service};
use crate::common::query::{ActionFilterMetadata, action_matches_filters, split_action_filters};
use crate::dashboard::config::DashboardConfig;
use crate::dashboard::widgets::{WidgetRegistry, WidgetSettingsContext};
use crate::dashboard::{
    Dashboard, DashboardContext, DashboardDataCache, DashboardEvent, DashboardRefreshRequest,
    DashboardRuntime, WidgetActivation,
};
use crate::file_search::coordinator::SearchCoordinator;
use crate::help_window::HelpWindow;
use crate::history::{self, HISTORY_PINS_FILE, HistoryEntry, HistoryPin};
use crate::indexer;

use crate::multi_manager::state::MultiManagerState;
use crate::multi_manager::ui::{MultiManagerDialog, MultiManagerSettingsDialog};
use crate::performance::track_c::EventOrigin;
use crate::plugin::{CAP_FORCE_LIST_RESULTS, CAP_GRID_RESULTS_COMPATIBLE, PluginManager};
use crate::plugin_editor::PluginEditor;
use crate::plugins::clipboard_modify::ClipboardModifyPluginSettings;
use crate::plugins::note::{NoteExternalOpen, NotePluginSettings};
use crate::plugins::snippets::{SNIPPETS_FILE, remove_snippet};
use crate::settings::{MultiManagerSettings, NoteSettings, QueryResultsLayoutSettings, Settings};
use crate::settings_editor::SettingsEditor;
use crate::toast_log::{TOAST_LOG_FILE, append_toast_log};
use crate::usage::{self, USAGE_FILE};
use crate::visibility::{
    RootFocusIntent, RootViewportCtx, RootWindowBridge, ViewportCtx, ViewportWake,
    VisiblePlacementPolicy, apply_visibility, apply_visibility_with_focus_intent,
};
use action_sheet::ActionSheetState;
use chrono::NaiveDate;
use confirmation_modal::{ConfirmationModal, ConfirmationResult, DestructiveAction};
use dashboard_editor_dialog::DashboardEditorDialog;
use eframe::egui;
use egui_toast::{Toast, ToastKind, ToastOptions, Toasts};
pub use event_channel::EventReceiver;
use event_channel::EventSender;
use fst::Map;
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use numpad_navigation::{
    LauncherNumpadNavigation, NativeNumpadKeyStateProbe, consume_physical_numpad_navigation,
};
use once_cell::sync::Lazy;
use query_history::{QueryHistoryDirection, QueryHistoryNavigator};
#[cfg(test)]
use search::{COMPLETION_REBUILD_DEBOUNCE, NOTE_SEARCH_DEBOUNCE};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt::Display;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
};
use std::time::{Duration, Instant};
use url::Url;
use watch::watch_file;

pub use crate::commands::ActivationSource;
pub use crate::radial::authoring::RadialAuthoringSession;
pub(crate) use state::{
    AuthoringActionRevalidation, PendingConfirmCommand, PendingUniversalActionInvocation,
    UiErrorEvent,
};
pub use state::{
    ClipboardModifyGuiEvent, RadialPlacementFailureNotice, TestWatchEvent,
    VirtualDesktopGuiCompletion, WatchEvent,
};

const SUBCOMMANDS: &[&str] = &[
    "add", "rm", "list", "clear", "open", "new", "alias", "set", "pause", "resume", "cancel",
    "edit", "ma",
];

pub(super) fn radial_placement_viewport_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("radial-submenu-placement-recovery")
}

/// Prefix used to search user saved applications.
pub const APP_PREFIX: &str = "app";
fn scale_ui<R>(ui: &mut egui::Ui, scale: f32, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.scope(|ui| {
        if (scale - 1.0).abs() > f32::EPSILON {
            let mut style: egui::Style = (**ui.style()).clone();
            style.spacing.item_spacing *= scale;
            style.spacing.interact_size *= scale;
            for font in style.text_styles.values_mut() {
                font.size *= scale;
            }
            ui.set_style(style);
        }
        add_contents(ui)
    })
    .inner
}

fn push_toast(toasts: &mut Toasts, toast: Toast) {
    append_toast_log(toast.text.text());
    toasts.add(toast);
}

fn normalize_static_window_config(
    follow_mouse: bool,
    static_location_enabled: bool,
    static_pos: Option<(i32, i32)>,
    static_size: Option<(i32, i32)>,
) -> (bool, Option<(i32, i32)>, Option<(i32, i32)>) {
    if follow_mouse {
        (false, None, None)
    } else {
        (static_location_enabled, static_pos, static_size)
    }
}

struct AppEventSink {
    id: u64,
    sender: AppEventSender,
    wake: Option<ViewportWake>,
    queued: Arc<AtomicUsize>,
}

enum AppEventSender {
    Raw(Sender<WatchEvent>),
    Observed(EventSender),
}

impl AppEventSender {
    fn send(&self, event: WatchEvent) -> Result<(), ()> {
        match self {
            Self::Raw(sender) => sender.send(event).map_err(|_| ()),
            Self::Observed(sender) => sender.send_as(event, EventOrigin::Registry).map_err(|_| ()),
        }
    }
}

const APP_EVENT_PENDING_CAPACITY: usize = 256;

#[cfg(test)]
pub(crate) static TEST_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

struct AppEventRegistry {
    sinks: Vec<AppEventSink>,
    /// Events emitted before the first GUI owner registers. Once an owner has
    /// existed, a disposed owner must not cause stale work to leak into a
    /// future viewport.
    pending_before_owner: VecDeque<WatchEvent>,
    owner_registered: bool,
}

static APP_EVENT_REGISTRY: Lazy<Mutex<AppEventRegistry>> = Lazy::new(|| {
    Mutex::new(AppEventRegistry {
        sinks: Vec::new(),
        pending_before_owner: VecDeque::new(),
        owner_registered: false,
    })
});
static NEXT_APP_EVENT_SINK_ID: AtomicU64 = AtomicU64::new(1);
static RADIAL_AUTHORING_CLIENT: Lazy<Mutex<Option<crate::radial::authoring::AuthoringClient>>> =
    Lazy::new(|| Mutex::new(None));
static RADIAL_CONTROL_CLIENT: Lazy<Mutex<Option<crate::radial::control::RadialControlClient>>> =
    Lazy::new(|| Mutex::new(None));
static RADIAL_PUBLISHED_DOCUMENT: Lazy<Mutex<Arc<crate::radial::model::RadialDocument>>> =
    Lazy::new(|| Mutex::new(Arc::new(crate::radial::model::RadialDocument::starter())));

/// Installs the GUI side of the main-owned radial authoring service. Editors
/// clone this narrow client; they never construct or write a `RadialStore`.
pub fn install_radial_authoring_client(client: crate::radial::authoring::AuthoringClient) {
    if let Ok(mut slot) = RADIAL_AUTHORING_CLIENT.lock() {
        *slot = Some(client);
    }
}

pub fn radial_authoring_client() -> Option<crate::radial::authoring::AuthoringClient> {
    RADIAL_AUTHORING_CLIENT.lock().ok()?.clone()
}

/// Installs the GUI side of the main-owned radial runtime control service.
pub fn install_radial_control_client(client: crate::radial::control::RadialControlClient) {
    if let Ok(mut slot) = RADIAL_CONTROL_CLIENT.lock() {
        *slot = Some(client);
    }
}

fn radial_control_client() -> Option<crate::radial::control::RadialControlClient> {
    RADIAL_CONTROL_CLIENT.lock().ok()?.clone()
}

/// Publishes the immutable settings/diagnostic view; process main remains the
/// sole store and runtime owner.
pub fn install_radial_published_document(document: Arc<crate::radial::model::RadialDocument>) {
    if let Ok(mut slot) = RADIAL_PUBLISHED_DOCUMENT.lock() {
        *slot = document;
    }
}

pub(crate) fn radial_published_document() -> Arc<crate::radial::model::RadialDocument> {
    RADIAL_PUBLISHED_DOCUMENT
        .lock()
        .map(|document| Arc::clone(&document))
        .unwrap_or_else(|_| Arc::new(crate::radial::model::RadialDocument::starter()))
}

/// Owns one GUI event sink registration. Dropping it removes the sink from
/// the process-wide fan-out, so a closed launcher viewport cannot remain in
/// the registry indefinitely.
pub struct EventSinkRegistration {
    id: u64,
    queued: Arc<AtomicUsize>,
}

impl EventSinkRegistration {
    pub(crate) fn owns_live_queue(&self) -> bool {
        APP_EVENT_REGISTRY.lock().is_ok_and(|registry| {
            registry
                .sinks
                .iter()
                .any(|sink| sink.id == self.id && sink.wake.is_some())
        })
    }
    pub(crate) fn owner_id(&self) -> u64 {
        self.id
    }
    /// Attach the viewport wake after registration. If events arrived before
    /// attachment, wake once after releasing the registry lock so the queued
    /// work is not stranded.
    pub fn attach_wake(&self, wake: ViewportWake) {
        let wake_now = APP_EVENT_REGISTRY.lock().ok().and_then(|mut registry| {
            let sink = registry.sinks.iter_mut().find(|sink| sink.id == self.id)?;
            sink.wake = Some(wake.clone());
            (sink.queued.load(Ordering::Acquire) > 0).then_some(wake)
        });
        if let Some(wake) = wake_now {
            wake.wake();
        }
    }

    pub(crate) fn event_consumed(&self) {
        let _ = self
            .queued
            .try_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                Some(count.saturating_sub(1))
            });
    }
}

impl Drop for EventSinkRegistration {
    fn drop(&mut self) {
        crate::paste_sequence::admission::physical_paste_bridge().invalidate_owner(self.id);
        if let Ok(mut registry) = APP_EVENT_REGISTRY.lock() {
            registry.sinks.retain(|sink| sink.id != self.id);
        }
    }
}

fn register_event_sink(
    sender: AppEventSender,
    wake: Option<ViewportWake>,
) -> EventSinkRegistration {
    let registration = EventSinkRegistration {
        id: NEXT_APP_EVENT_SINK_ID.fetch_add(1, Ordering::Relaxed),
        queued: Arc::new(AtomicUsize::new(0)),
    };
    let mut wake_now = None;
    if let Ok(mut registry) = APP_EVENT_REGISTRY.lock() {
        let first_owner = !registry.owner_registered;
        registry.owner_registered = true;
        let sink = AppEventSink {
            id: registration.id,
            sender,
            wake,
            queued: Arc::clone(&registration.queued),
        };
        if first_owner {
            let pending = registry.pending_before_owner.drain(..).collect::<Vec<_>>();
            for event in pending {
                sink.queued.fetch_add(1, Ordering::AcqRel);
                if sink.sender.send(event).is_err() {
                    let _ = sink
                        .queued
                        .try_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                            Some(n.saturating_sub(1))
                        });
                    return registration;
                }
            }
        }
        if sink.queued.load(Ordering::Acquire) > 0 {
            wake_now = sink.wake.clone();
        }
        registry.sinks.push(sink);
    }
    if let Some(wake) = wake_now {
        wake.wake();
    }
    registration
}

/// Register a GUI event sink with the viewport that owns its work queue.
pub fn register_event_sender_with_wake(
    tx: Sender<WatchEvent>,
    wake: ViewportWake,
) -> EventSinkRegistration {
    register_event_sink(AppEventSender::Raw(tx), Some(wake))
}

pub fn register_event_sender(tx: Sender<WatchEvent>) -> EventSinkRegistration {
    register_event_sink(AppEventSender::Raw(tx), None)
}

fn register_observed_event_sender(tx: EventSender, wake: ViewportWake) -> EventSinkRegistration {
    register_event_sink(AppEventSender::Observed(tx), Some(wake))
}

pub fn send_event(ev: WatchEvent) {
    let wakes = APP_EVENT_REGISTRY
        .lock()
        .map(|mut registry| {
            if registry.sinks.is_empty() {
                if !registry.owner_registered {
                    if registry.pending_before_owner.len() == APP_EVENT_PENDING_CAPACITY {
                        registry.pending_before_owner.pop_front();
                    }
                    registry.pending_before_owner.push_back(ev);
                }
                return Vec::new();
            }
            let mut wakes = Vec::new();
            registry.sinks.retain_mut(|sink| {
                sink.queued.fetch_add(1, Ordering::AcqRel);
                if sink.sender.send(ev.clone()).is_err() {
                    let _ = sink
                        .queued
                        .try_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                            Some(n.saturating_sub(1))
                        });
                    return false;
                }
                if let Some(wake) = &sink.wake {
                    wakes.push(wake.clone());
                }
                true
            });
            wakes
        })
        .unwrap_or_default();
    for wake in wakes {
        wake.wake();
    }
}

/// Physical gestures belong to one started playback owner. They must never
/// fan out or enter the pre-owner buffer used by ordinary refresh events.
pub fn send_physical_paste_event(
    owner: u64,
    notice: crate::paste_sequence::admission::PhysicalPasteNotice,
) -> Result<(), &'static str> {
    let wake = {
        let mut registry = APP_EVENT_REGISTRY
            .lock()
            .map_err(|_| "event registry unavailable")?;
        let sink = registry
            .sinks
            .iter_mut()
            .find(|sink| sink.id == owner)
            .ok_or("playback event owner unavailable")?;
        let wake = sink
            .wake
            .clone()
            .ok_or("playback viewport wake unavailable")?;
        sink.queued.fetch_add(1, Ordering::AcqRel);
        if sink.sender.send(WatchEvent::PhysicalPaste(notice)).is_err() {
            sink.queued.fetch_sub(1, Ordering::AcqRel);
            return Err("playback event queue closed");
        }
        wake
    };
    wake.wake();
    Ok(())
}

#[cfg(not(test))]
fn open_link(url: &str) -> std::io::Result<()> {
    open::that(url)
}

#[cfg(test)]
fn open_link(_url: &str) -> std::io::Result<()> {
    OPEN_LINK_COUNT.fetch_add(1, Ordering::SeqCst);
    Ok(())
}

#[cfg(test)]
pub static OPEN_LINK_COUNT: AtomicUsize = AtomicUsize::new(0);

#[allow(dead_code)]
pub static EXECUTE_ACTION_COUNT: AtomicUsize = AtomicUsize::new(0);

/// Host-owned boundaries needed by visual rectangle authoring.  Keeping this
/// bundle at the launcher seam lets tests replace native Windows services while
/// exercising the exact same installation as production.
pub(crate) struct VisualCaptureDependencies {
    overlay: Box<dyn mkmacro_dialog::visual_capture_workflow::RectangleOverlay>,
    capture: Box<dyn mkmacro_dialog::visual_capture_workflow::CaptureAdapter>,
    assets: Box<dyn mkmacro_dialog::visual_capture_workflow::AssetStoreAdapter>,
}

fn install_visual_capture(dialog: &mut MkMacroDialog, dependencies: VisualCaptureDependencies) {
    dialog.action_editor.visual_capture = Some(
        mkmacro_dialog::visual_capture_workflow::VisualCaptureWorkflow::new(
            dependencies.overlay,
            dependencies.capture,
            dependencies.assets,
        ),
    );
}

fn production_visual_capture_dependencies(
    dialog: &MkMacroDialog,
    store: Arc<crate::mkmacro::MkMacroStore>,
    screen_backend: Arc<dyn crate::mkmacro::ScreenCaptureBackend>,
) -> VisualCaptureDependencies {
    VisualCaptureDependencies {
        overlay: Box::new(
            mkmacro_dialog::visual_capture_workflow::VisualOverlayRectangleAdapter::new(
                dialog.visual_overlay.clone(),
                Arc::clone(&screen_backend),
            ),
        ),
        capture: Box::new(
            mkmacro_dialog::visual_capture_workflow::ScreenCaptureAdapter(screen_backend),
        ),
        assets: Box::new(mkmacro_dialog::visual_capture_workflow::MkMacroAssetStoreAdapter(store)),
    }
}

static EXECUTE_ACTION_HOOK: Lazy<
    Mutex<Option<Box<dyn Fn(&Action) -> anyhow::Result<()> + Send + Sync>>>,
> = Lazy::new(|| Mutex::new(None));

pub type ActivationHook = Box<dyn Fn(&Action, ActivationSource) + Send + Sync>;
static ACTIVATION_HOOK: Lazy<Mutex<Option<ActivationHook>>> = Lazy::new(|| Mutex::new(None));

pub fn set_execute_action_hook(
    hook: Option<Box<dyn Fn(&Action) -> anyhow::Result<()> + Send + Sync>>,
) {
    if let Ok(mut guard) = EXECUTE_ACTION_HOOK.lock() {
        *guard = hook;
    }
}

/// Install an observer for actions entering the normal launcher activation path.
///
/// This is primarily an integration-test seam: unlike the execution hook it
/// does not replace activation, and therefore can verify the dispatch source
/// while the real action handler continues to run.
pub fn set_activation_hook(hook: Option<ActivationHook>) {
    if let Ok(mut guard) = ACTIVATION_HOOK.lock() {
        *guard = hook;
    }
}

pub(crate) fn execute_parsed_action(
    command: &crate::commands::Command,
    action: &Action,
) -> anyhow::Result<()> {
    if let Ok(guard) = EXECUTE_ACTION_HOOK.lock()
        && let Some(ref hook) = *guard
    {
        return hook(action);
    }
    crate::commands::headless::execute(command.clone(), action)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Panel {
    AliasDialog,
    BookmarkAliasDialog,
    TempfileAliasDialog,
    TempfileDialog,
    AddBookmarkDialog,
    HelpOverlay,
    HelpWindow,
    TimerDialog,
    CompletionDialog,
    ShellCmdDialog,
    SnippetDialog,
    PasteSequenceDialog,
    HashGeneratorDialog,
    SnippetPromptDialog,
    MacroDialog,
    MkMacroDialog,
    MouseGesturesDialog,
    MouseGestureSettingsDialog,
    MouseSettingsDialog,
    ThemeSettingsDialog,
    HotkeyDirectoryDialog,
    FavDialog,
    FileSearchDialog,
    NotesDialog,
    NoteGraphDialog,
    UnusedAssetsDialog,
    NotePanel,
    ImagePanel,
    ScreenshotEditor,
    CropDialog,
    TodoDialog,
    TodoViewDialog,
    ClipboardDialog,
    ClipboardModifyDialog,
    JsonUtilityDialog,
    RegexTesterDialog,
    BatchRenameDialog,
    SearchReplaceDialog,
    XmlToolkitDialog,
    QrDialog,
    ConvertPanel,
    VolumeDialog,
    BrightnessDialog,
    CpuListDialog,
    DataRecoveryDialog,
    ToastLogDialog,
    CalendarPopover,
    CalendarEventEditor,
    CalendarEventDetails,
    Editor,
    Settings,
    Plugins,
    MultiManagerDialog,
    MultiManagerSettingsDialog,
    RadialEditor,
    TableUtilityDialog,
    ImageCompareDialog,
}

impl Panel {
    pub const fn display_label(self) -> &'static str {
        match self {
            Self::AliasDialog => "Alias",
            Self::BookmarkAliasDialog => "Bookmark Alias",
            Self::TempfileAliasDialog => "Temporary File Alias",
            Self::TempfileDialog => "Temporary File",
            Self::AddBookmarkDialog => "Add Bookmark",
            Self::HelpOverlay => "Quick Help",
            Self::HelpWindow => "Command Help",
            Self::TimerDialog => "Timer",
            Self::CompletionDialog => "Completion",
            Self::ShellCmdDialog => "Shell Commands",
            Self::SnippetDialog => "Snippets",
            Self::PasteSequenceDialog => "Paste Sequences",
            Self::HashGeneratorDialog => "Hash Generator",
            Self::SnippetPromptDialog => "Snippet Prompt",
            Self::MacroDialog => "Macros",
            Self::MkMacroDialog => "MKMacro",
            Self::MouseGesturesDialog => "Mouse Gestures",
            Self::MouseGestureSettingsDialog => "Mouse Gesture Settings",
            Self::MouseSettingsDialog => "Mouse Settings",
            Self::ThemeSettingsDialog => "Theme Settings",
            Self::HotkeyDirectoryDialog => "Hotkey Directory",
            Self::FavDialog => "Favorites",
            Self::FileSearchDialog => "File Search",
            Self::NotesDialog => "Quick Notes",
            Self::NoteGraphDialog => "Note Graph",
            Self::UnusedAssetsDialog => "Unused Note Assets",
            Self::NotePanel => "Note",
            Self::ImagePanel => "Image Viewer",
            Self::ScreenshotEditor => "Screenshot Editor",
            Self::CropDialog => "Image Crop",
            Self::TodoDialog => "To-do",
            Self::TodoViewDialog => "To-do View",
            Self::ClipboardDialog => "Clipboard History",
            Self::ClipboardModifyDialog => "Clipboard Modify",
            Self::JsonUtilityDialog => "JSON Utility",
            Self::RegexTesterDialog => "Regex Tester",
            Self::BatchRenameDialog => "Batch Renamer",
            Self::SearchReplaceDialog => "Search and Replace",
            Self::XmlToolkitDialog => "XML Toolkit",
            Self::TableUtilityDialog => "Table Utility",
            Self::ImageCompareDialog => "Image Comparison",
            Self::QrDialog => "QR Generator",
            Self::ConvertPanel => "Converter",
            Self::VolumeDialog => "Volume",
            Self::BrightnessDialog => "Brightness",
            Self::CpuListDialog => "CPU Usage",
            Self::DataRecoveryDialog => "Data & Recovery",
            Self::ToastLogDialog => "Toast Log",
            Self::CalendarPopover => "Calendar",
            Self::CalendarEventEditor => "Calendar Event",
            Self::CalendarEventDetails => "Calendar Event Details",
            Self::Editor => "App Editor",
            Self::Settings => "Settings",
            Self::Plugins => "Plugins",
            Self::MultiManagerDialog => "Multi Manager",
            Self::MultiManagerSettingsDialog => "Multi Manager Settings",
            Self::RadialEditor => "Radial Editor",
        }
    }
}

#[derive(Default)]
struct PanelStates {
    alias_dialog: bool,
    bookmark_alias_dialog: bool,
    tempfile_alias_dialog: bool,
    tempfile_dialog: bool,
    add_bookmark_dialog: bool,
    help_overlay: bool,
    help_window: bool,
    timer_dialog: bool,
    completion_dialog: bool,
    shell_cmd_dialog: bool,
    snippet_dialog: bool,
    paste_sequence_dialog: bool,
    hash_generator_dialog: bool,
    snippet_prompt_dialog: bool,
    macro_dialog: bool,
    mkmacro_dialog: bool,
    mouse_gestures_dialog: bool,
    mouse_gesture_settings_dialog: bool,
    mouse_settings_dialog: bool,
    theme_settings_dialog: bool,
    hotkey_directory_dialog: bool,
    fav_dialog: bool,
    file_search_dialog: bool,
    notes_dialog: bool,
    note_graph_dialog: bool,
    unused_assets_dialog: bool,
    note_panel: bool,
    image_panel: bool,
    screenshot_editor: bool,
    crop_dialog: bool,
    todo_dialog: bool,
    todo_view_dialog: bool,
    clipboard_dialog: bool,
    clipboard_modify_dialog: bool,
    json_utility_dialog: bool,
    regex_tester_dialog: bool,
    batch_rename_dialog: bool,
    search_replace_dialog: bool,
    xml_toolkit_dialog: bool,
    table_utility_dialog: bool,
    image_compare_dialog: bool,
    qr_dialog: bool,
    convert_panel: bool,
    volume_dialog: bool,
    brightness_dialog: bool,
    cpu_list_dialog: bool,
    data_recovery_dialog: bool,
    toast_log_dialog: bool,
    calendar_popover: bool,
    calendar_event_editor: bool,
    calendar_event_details: bool,
    editor: bool,
    settings: bool,
    plugins: bool,
    multi_manager_dialog: bool,
    multi_manager_settings_dialog: bool,
    radial_editor: bool,
}

/// Primary GUI state for Multi Launcher.
///
/// The application may create multiple windows or helper threads. To keep the
/// available actions consistent across those components, `LauncherApp` holds
/// them in an [`Arc<Vec<Action>>`]. Cloning the `Arc` only replicates the
/// pointer, allowing cheap, thread-safe sharing without duplicating the vector
/// itself.
#[derive(Default)]
pub(crate) struct MacroPromptUi {
    pub pending: Option<crate::mkmacro::PendingPrompt>,
    pub text: String,
    pub first_frame: bool,
    pub bootstrap_pending: bool,
}

#[derive(Clone)]
pub(crate) struct PasteSequenceDocumentRevisionNotifier {
    revision: Arc<AtomicU64>,
    main_wake: Option<Sender<()>>,
}

impl PasteSequenceDocumentRevisionNotifier {
    pub(crate) fn new(revision: Arc<AtomicU64>, main_wake: Option<Sender<()>>) -> Self {
        Self {
            revision,
            main_wake,
        }
    }

    pub(crate) fn note_revision(&self, revision: u64) {
        let previous = self.revision.fetch_max(revision, Ordering::AcqRel);
        if revision > previous
            && let Some(main_wake) = self.main_wake.as_ref()
        {
            let _ = main_wake.send(());
        }
    }
}

pub struct LauncherApp {
    /// Shared list of all actions available to the launcher.
    ///
    /// The list is wrapped in an [`Arc`] so windows or background tasks can
    /// access it without cloning the underlying `Vec`. Cloning this field only
    /// duplicates the pointer, keeping the action data itself shared. When
    /// actions are edited the entire `Arc` is replaced with a new one.
    pub actions: Arc<Vec<Action>>,
    command_bus: Arc<crate::commands::CommandBus>,
    hotfolder_action_adapter: Arc<hotfolder_dispatch::LauncherHotFolderActionAdapter>,
    hotfolder_action_pump: hotfolder_dispatch::LauncherActionCompletionPump,
    pub(crate) hotfolder_runtime: hotfolder_runtime::HotFolderRuntimeDriver,
    pub(crate) hotfolder_dialog: hotfolder_dialog::HotFolderDialog,
    action_cache: Vec<CachedSearchEntry>,
    action_filter_metadata: Vec<ActionFilterMetadata>,
    actions_by_id: ActionIdLookup,
    command_cache: Vec<Action>,
    command_search_cache: Vec<CachedSearchEntry>,
    completion_index: Option<Map<Vec<u8>>>,
    action_completion_dirty: bool,
    command_completion_dirty: bool,
    completion_rebuild_after: Option<Instant>,
    suggestions: Vec<String>,
    autocomplete_index: usize,
    pub query: String,
    query_history: QueryHistoryNavigator,
    pub results: Vec<Action>,
    root_list_geometry: render::RootListGeometryCache,
    root_frame_logic: Option<render::RootFrameLogic>,
    pending_ui_frame_receipt: Option<render::UiFrameReceiptCandidate>,
    #[cfg(feature = "qa-framebuffer-capture")]
    qa_framebuffer: Option<qa_framebuffer::FramebufferProbe>,
    indexing: indexing::IndexingOwner,
    pub matcher: SkimMatcherV2,
    pub error: Option<String>,
    error_time: Option<Instant>,
    pub plugins: PluginManager,
    /// Main-thread orchestration for Screen Draw. Native session resources are
    /// intentionally owned outside `LauncherApp` by the later worker layer.
    pub screen_draw_controller: crate::screen_draw::ScreenDrawController,
    pub(crate) coordinate_tool: coordinate_tool::CoordinateToolGui,
    coordinate_capture_parking: Option<coordinate_tool::CoordinatePickParking>,
    coordinate_capture_generation: u64,
    screen_draw_recovery_bridge: Arc<crate::screen_draw::ScreenDrawRecoveryBridge>,
    color_pick: color_pick::ColorPickLifecycle,
    ocr: ocr::OcrLifecycle,
    pinboard_capture: pinboard::PinboardCaptureLifecycle,
    pin_list_open: bool,
    screen_draw_launcher_parking: Option<
        crate::launcher_parking::LauncherParkingTransaction<
            crate::screen_draw::ScreenDrawGeneration,
        >,
    >,
    screen_draw_toolbar: screen_draw_toolbar::ScreenDrawToolbarUi,
    screen_draw_restore_publication: screen_draw_restore::ScreenDrawRestorePublication,
    pub selected: Option<usize>,
    action_sheet: ActionSheetState,
    /// Test seam for verifying that command dispatch used normal activation,
    /// including the activation source, without launching an external process.
    #[cfg(test)]
    pub(crate) test_last_activation: Option<(Action, ActivationSource)>,
    /// Complete activation trace used by dispatch tests (including recursive
    /// query activations).  This deliberately lives at the activation seam,
    /// rather than in the macro broker.
    #[cfg(test)]
    pub(crate) test_activation_trace: Vec<(Action, ActivationSource)>,
    #[cfg(test)]
    pub(crate) test_sequence_context_capture_forbidden: bool,
    #[cfg(test)]
    pub(crate) test_sequence_context_capture_calls: usize,
    pub usage: HashMap<String, u32>,
    pub(crate) target_repair_usage_generation: Arc<AtomicU64>,
    pub(crate) target_repair_usage_barrier: Arc<Mutex<()>>,
    pub(crate) target_repair_deferred_usage: Arc<Mutex<HashMap<String, u32>>>,
    pub(crate) target_repair_usage_handoff: crate::target_repair::UsageHandoff,
    pub(crate) target_repair_client: Option<crate::target_repair::TargetRepairClient>,
    /// IDs registered through the legacy public `global_hotkey::Hotkey` API.
    /// The launcher itself routes configured hotkeys through the main runtime.
    pub registered_hotkeys: Mutex<HashMap<String, usize>>,
    pub show_editor: bool,
    pub show_settings: bool,
    pub show_plugins: bool,
    pub actions_path: String,
    pub editor: ActionsEditor,
    pub settings_editor: SettingsEditor,
    pub plugin_editor: PluginEditor,
    pub settings_path: String,
    pub(crate) settings_save_workers: crate::settings_save::SettingsSaveWorkerRegistry,
    pub(crate) pending_settings_save: Option<crate::settings_save::PendingSettingsSave>,
    pub(crate) settings_save_reconciliation_required: bool,
    /// Persistence failure captured before GUI startup. A later recovery UI can
    /// consume this without re-reading or replacing the damaged settings file.
    pub startup_settings_diagnostic: Option<crate::startup::SettingsStartupDiagnostic>,
    pub actions_persistence_diagnostic: Option<crate::common::persistence::PersistenceError>,
    pub startup_recovery: crate::persistence::RecoveryStartupResult,
    pub multi_manager: MultiManagerState,
    pub multi_manager_settings: MultiManagerSettings,
    pub launcher_hwnd: Option<usize>,
    pub multi_manager_dialog: MultiManagerDialog,
    pub multi_manager_settings_dialog: MultiManagerSettingsDialog,
    radial_editor: Arc<Mutex<radial_editor::RadialEditorState>>,
    /// Hold watchers so the `RecommendedWatcher` instances remain active.
    #[allow(dead_code)] // required to keep watchers alive
    watchers: Vec<RecommendedWatcher>,
    pub dashboard: Dashboard,
    dashboard_runtime: DashboardRuntime,
    dashboard_data_cache: DashboardDataCache,
    dashboard_initial_refresh_queued: bool,
    pub dashboard_enabled: bool,
    pub dashboard_show_when_empty: bool,
    pub dashboard_path: String,
    pub dashboard_default_location: Option<String>,
    pub reduce_dashboard_work_when_unfocused: bool,
    pub show_dashboard_diagnostics: bool,
    pub dashboard_editor: DashboardEditorDialog,
    pub show_dashboard_editor: bool,
    rx: EventReceiver,
    event_sink: EventSinkRegistration,
    event_tx: EventSender,
    /// Bounds deferred radial provider work to one in-flight invocation. A
    /// cancelled provider call keeps this slot until the call actually exits.
    pub(crate) radial_provider_search_capacity:
        crate::radial::handoff::DeferredProviderSearchCapacity,
    /// Temporarily disables provider lookup while a failed deferred query is
    /// handed back to the ordinary query UI. This is scoped to that one
    /// synchronous command outcome so it cannot re-enter the provider that
    /// just timed out, even if its worker releases capacity at that boundary.
    pub(crate) radial_suppressed_provider_query: Option<String>,
    pub(crate) radial_queryexec_depth: Option<u8>,
    pub(crate) radial_query_observation: crate::gui::query_observation::QueryObservationMailbox,
    egui_ctx: egui::Context,
    virtual_desktop_interaction_token: u64,
    command_root_policy: crate::universal_actions::RootLauncherPolicy,
    folder_aliases: HashMap<String, Option<String>>,
    folder_aliases_lc: HashMap<String, Option<String>>,
    bookmark_aliases: HashMap<String, Option<String>>,
    bookmark_aliases_lc: HashMap<String, Option<String>>,
    plugin_dirs: Option<Vec<String>>,
    enabled_plugins: Option<HashSet<String>>,
    enabled_capabilities: Option<std::collections::HashMap<String, Vec<String>>>,
    root_window_bridge: RootWindowBridge,
    snippet_origin_window: Option<crate::active_window::ExternalForegroundWindow>,
    pub(crate) sequence_playback: crate::paste_sequence::SequencePlaybackController,
    sequence_exit_requested: bool,
    pub(crate) sequence_routes: Option<crate::paste_sequence::SequenceRoutePresentation>,
    /// Latest immutable hotkey directory assembled from owner publications.
    pub(crate) hotkey_directory: Option<Arc<crate::hotkey::directory::Snapshot>>,
    pub(crate) hotkey_directory_dialog: hotkey_directory_dialog::HotkeyDirectoryDialog,
    paste_sequence_document_revision: Arc<AtomicU64>,
    hotkey_directory_main_wake: Option<Sender<()>>,
    visible_flag: Arc<AtomicBool>,
    restore_flag: Arc<AtomicBool>,
    visibility_revision: crate::visibility::VisibilityRevision,
    authoring_catalog_cache: Mutex<universal_action_catalog::AuthoringCatalogCache>,
    #[cfg(test)]
    authoring_catalog_build_count: AtomicUsize,
    #[cfg(test)]
    test_unpublish_root_identity_before_visibility_reconcile: bool,
    last_visible: bool,
    offscreen_pos: (f32, f32),
    pub window_size: (i32, i32),
    pub window_pos: (i32, i32),
    focus_query: bool,
    pending_show_query_focus: Option<PendingShowQueryFocus>,
    query_focus_session: Option<QueryFocusSession>,
    last_query_focus_geometry: Option<QueryFocusResponseGeometry>,
    move_cursor_end: bool,
    toasts: egui_toast::Toasts,
    #[cfg(test)]
    pub test_toast_messages: Vec<String>,
    #[cfg(test)]
    pub(crate) test_recorded_history_queries: Vec<String>,
    #[cfg(test)]
    pub(crate) test_skip_history_persistence: bool,
    #[cfg(test)]
    pub(crate) test_screenshot_delivery: Option<
        Box<
            dyn FnOnce(
                    image::RgbaImage,
                    bool,
                    MarkupTool,
                )
                    -> Result<crate::plugins::screenshot::ScreenshotLaunchOutcome, String>
                + Send,
        >,
    >,
    #[cfg(test)]
    pub(crate) test_root_rendered_rows: Vec<(usize, String)>,
    #[cfg(test)]
    pub(crate) test_root_rendered_row_ids: Vec<(usize, egui::Id)>,
    #[cfg(test)]
    pub(crate) test_root_rendered_row_rects: Vec<(usize, egui::Rect)>,
    #[cfg(test)]
    pub(crate) test_root_scroll_viewport_rect: Option<egui::Rect>,
    #[cfg(test)]
    pub(crate) test_root_scroll_area_extent: Option<(egui::Rect, egui::Vec2, egui::Vec2)>,
    #[cfg(test)]
    pub(crate) test_root_scroll_area_id: Option<egui::Id>,
    #[cfg(test)]
    pub(crate) test_defer_virtual_desktop_completion: bool,
    pub enable_toasts: bool,
    pub show_inline_errors: bool,
    pub show_error_toasts: bool,
    pub toast_duration: f32,
    alias_dialog: AliasDialog,
    bookmark_alias_dialog: BookmarkAliasDialog,
    tempfile_alias_dialog: TempfileAliasDialog,
    tempfile_dialog: TempfileDialog,
    add_bookmark_dialog: AddBookmarkDialog,
    help_window: crate::help_window::HelpWindow,
    timer_dialog: TimerDialog,
    completion_dialog: TimerCompletionDialog,
    shell_cmd_dialog: ShellCmdDialog,
    snippet_dialog: SnippetDialog,
    paste_sequence_dialog: PasteSequenceDialog,
    pub hash_generator_dialog: HashGeneratorDialogState,
    snippet_prompt_dialog: snippet_prompt_dialog::SnippetPromptDialog,
    macro_dialog: MacroDialog,
    pub mkmacro_dialog: MkMacroDialog,
    pub(crate) macro_prompt: MacroPromptUi,
    macro_prompt_broker: Arc<crate::mkmacro::PromptBroker>,
    pub(crate) macro_parameter_prompt: mkmacro_dialog::parameter_prompt::ParameterPromptUi,
    mouse_gestures_dialog: MgGesturesDialog,
    mouse_gesture_settings_dialog: MouseGestureSettingsDialog,
    mouse_settings_dialog: MouseSettingsDialog,
    theme_settings_dialog_open: bool,
    theme_settings_dialog: ThemeSettingsDialogState,
    fav_dialog: FavDialog,
    file_search_dialog: FileSearchDialogState,
    pub diff_dialog: DiffDialogState,
    file_search_coordinator: SearchCoordinator,
    batch_rename_dialog: BatchRenameDialogState,
    batch_rename_coordinator: Option<crate::file_operations::RenameScanCoordinator>,
    search_replace_dialog: SearchReplaceDialogState,
    replace_search_coordinator: Option<crate::search_replace::SearchCoordinator>,
    replace_preview_worker: Option<ReplacementPreviewWorker>,
    replace_executor_worker: Option<crate::replace_executor::ReplaceExecutorWorker>,
    replace_operation_pending: bool,
    replace_worker_snapshot: crate::replace_executor::ReplaceWorkerSnapshot,
    notes_dialog: NotesDialog,
    note_graph_dialog: NoteGraphDialog,
    unused_assets_dialog: UnusedAssetsDialog,
    note_panels: Vec<NotePanel>,
    #[cfg(test)]
    pub(crate) note_mutation_refresh_count: usize,
    #[cfg(test)]
    pub(crate) note_mutation_cache_refresh_count: usize,
    #[cfg(test)]
    pub(crate) note_mutation_quick_notes_refresh_count: usize,
    #[cfg(test)]
    pub(crate) note_mutation_panel_invalidation_count: usize,
    #[cfg(test)]
    pub(crate) note_mutation_search_count: usize,
    image_panels: Vec<ImagePanel>,
    screenshot_editors: Vec<ScreenshotEditor>,
    pub(crate) screenshot_capture: screenshot_capture::ScreenshotCaptureOwner,
    pub(crate) screenshot_countdown: screenshot_countdown::ScreenshotCountdownPresentation,
    screenshot_capture_parking: Option<ScreenshotCaptureParking>,
    #[cfg(test)]
    test_screenshot_parking: Option<crate::launcher_parking::LauncherParkingTransaction<u64>>,
    pending_screenshot_completion: Option<screenshot_capture::ScreenshotCaptureCompletion>,
    pub(crate) crop_dialog: crop_dialog::CropDialogState,
    crop_screenshot_operation: Option<mkmacro_dialog::visual_overlay::OperationId>,
    crop_screenshot_capture: Option<Arc<dyn crate::mkmacro::ScreenCaptureBackend>>,
    screen_draw_region_operation: Option<ScreenDrawRegionOperation>,
    todo_dialog: TodoDialog,
    todo_view_dialog: TodoViewDialog,
    clipboard_dialog: ClipboardDialog,
    pub clipboard_modify_runtime: ClipboardModifyRuntime,
    pub clipboard_modify_dialog: ClipboardModifyDialogState,
    pub json_utility_dialog: JsonUtilityDialogState,
    pub regex_tester_dialog: RegexTesterDialogState,
    pub xml_toolkit_dialog: XmlToolkitDialogState,
    pub table_utility_dialog: TableUtilityDialogState,
    pub image_compare_dialog: ImageCompareDialogState,
    pub qr_dialog: QrDialogState,
    pub clipboard_modify_config_diagnostic: Option<String>,
    clipboard_modify_watcher: Option<crate::clipboard_modify::watch::ClipboardModifyWatcher>,
    pub(crate) clipboard_modify_hide_launcher_after_apply: bool,
    clipboard_modify_immediate: ImmediateExecutionCoordinator<
        crate::clipboard_modify::clipboard::ProductionClipboardService,
    >,
    clipboard_modify_events: Vec<ImmediateCompletionEvent>,
    convert_panel: ConvertPanel,
    volume_dialog: VolumeDialog,
    brightness_dialog: BrightnessDialog,
    cpu_list_dialog: CpuListDialog,
    data_recovery_dialog: DataRecoveryDialog,
    toast_log_dialog: ToastLogDialog,
    calendar_popover: CalendarPopover,
    calendar_event_editor: CalendarEventEditor,
    calendar_event_details: CalendarEventDetails,
    panel_stack: Vec<Panel>,
    panel_states: PanelStates,
    pinned_panels: Vec<Panel>,
    pub help_flag: Arc<AtomicBool>,
    pub hotkey_str: Option<String>,
    pub quit_hotkey_str: Option<String>,
    pub help_hotkey_str: Option<String>,
    pub query_scale: f32,
    pub list_scale: f32,
    pub(crate) show_launcher_icon: bool,
    pub(crate) show_launcher_title: bool,
    jump_mode_ui: jump_mode::ui::JumpModeUi,
    /// Number of user defined commands at the start of `actions`.
    pub custom_len: usize,
    pub history_limit: usize,
    pub clipboard_limit: usize,
    pub fuzzy_weight: f32,
    pub usage_weight: f32,
    pub match_exact: bool,
    pub page_jump: usize,
    pub query_results_layout: QueryResultsLayoutSettings,
    resolved_grid_layout: bool,
    pub note_settings: NoteSettings,
    pub note_panel_default_size: (f32, f32),
    pub note_save_on_close: bool,
    pub note_confirm_discard_unsaved_changes: bool,
    pub note_always_overwrite: bool,
    pub note_images_as_links: bool,
    pub note_show_details: bool,
    pub note_external_open: NoteExternalOpen,
    pub note_font_size: f32,
    pub note_more_limit: usize,
    pub follow_mouse: bool,
    pub static_location_enabled: bool,
    pub static_pos: Option<(i32, i32)>,
    pub static_size: Option<(i32, i32)>,
    pub hide_after_run: bool,
    pub always_on_top: bool,
    pub timer_refresh: f32,
    pub disable_timer_updates: bool,
    pub preserve_command: bool,
    pub clear_query_after_run: bool,
    pub require_confirm_destructive: bool,
    pub(crate) radial_feature_settings: crate::radial::model::RadialFeatureSettings,
    pub(crate) radial_expected_diagnostics: VecDeque<crate::radial::diagnostics::RadialDiagnostic>,
    pub(crate) radial_migration_receipt:
        Option<crate::settings::SubmenuPresentationMigrationReceipt>,
    radial_placement_viewport: state::RadialPlacementViewportState,
    pub query_autocomplete: bool,
    pub net_refresh: f32,
    pub net_unit: crate::settings::NetUnit,
    pub screenshot_dir: Option<String>,
    pub screenshot_save_file: bool,
    pub screenshot_auto_save: bool,
    pub screenshot_use_editor: bool,
    pub calendar_popover_open: bool,
    pub calendar_editor_open: bool,
    pub calendar_details_open: bool,
    pub calendar_selected_date: Option<NaiveDate>,
    pub calendar_selected_event: Option<String>,
    last_timer_update: Instant,
    last_net_update: Instant,
    last_stopwatch_update: Instant,
    last_search_query: String,
    last_results_valid: bool,
    last_search_pending: bool,
    last_search_provider_revision: u64,
    last_search_result_catalog_versions:
        Option<crate::radial::dynamic::MutableResultCatalogVersions>,
    last_search_result_catalog_versions_stable: bool,
    #[cfg(test)]
    track_c_test_force_enabled: bool,
    #[cfg(test)]
    track_c_test_provider_change_at_frame_end: bool,
    #[cfg(test)]
    track_c_test_catalog_version_change_at_frame_end: bool,
    track_c_query_observer: track_c_query_observer::Observer,
    background_query_refresh_pending: bool,
    last_search_provider_deferral: search::ProviderSearchDeferral,
    last_plugin_search_generation: u64,
    last_timer_query: bool,
    last_stopwatch_query: bool,
    last_note_search_change: Option<Instant>,
    pending_query: Option<String>,
    confirm_modal: ConfirmationModal,
    pending_confirm: Option<PendingConfirmCommand>,
    pending_universal_confirm: Option<PendingUniversalActionInvocation>,
    radial_preparations: HashMap<
        crate::radial::model::InvocationId,
        (
            crate::radial::bindings::PreparationGeneration,
            crate::radial::model::ConfigRevision,
        ),
    >,
    radial_consumed_dispatches: VecDeque<crate::radial::handoff::RadialDispatchIdentity>,
    radial_current_preparation: Option<(
        crate::radial::model::InvocationId,
        crate::radial::bindings::PreparationGeneration,
        crate::radial::model::ConfigRevision,
    )>,
    pending_data_recovery: Option<PendingRecoveryIntent>,
    pub vim_mode: bool,
    pub file_search_window_open: bool,
    pub file_search_selected_kind: crate::file_search::model::SearchKind,
    pub file_search_root: Option<std::path::PathBuf>,
    pub file_search_text: String,
    pub file_search_active: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct PendingShowQueryFocus {
    visibility_revision: u64,
    root_identity: Option<(usize, u64)>,
    query_cursor_range: Option<egui::text::CCursorRange>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct QueryFocusNotesContext {
    notes_dialog_open: bool,
    note_panel_layers: std::collections::HashSet<egui::LayerId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct QueryFocusSession {
    origin: QueryFocusOrigin,
    visibility_revision: u64,
    root_identity: (usize, u64),
    notes_context: QueryFocusNotesContext,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct QueryFocusResponseGeometry {
    response_id: egui::Id,
    viewport: egui::ViewportId,
    rect: egui::Rect,
    ui_clip_rect: egui::Rect,
    layer_id: egui::LayerId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum QueryFocusOrigin {
    Show,
    Mouse,
    Hint {
        notes_instance: u64,
        notes_generation: jump_mode::TargetGeneration,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct CachedSearchEntry {
    label_lc: String,
    desc_lc: String,
    action_lc: String,
}

impl CachedSearchEntry {
    fn from_action(action: &Action) -> Self {
        Self {
            label_lc: action.label.to_lowercase(),
            desc_lc: action.desc.to_lowercase(),
            action_lc: action.action.to_lowercase(),
        }
    }
}

impl LauncherApp {
    /// Installs the owned repair worker using this app's actual Hot Folder
    /// store and root repaint handle. Startup code supplies the shared Radial
    /// owner and data root without reaching through private GUI state.
    pub fn initialize_target_repair_service(
        &mut self,
        root: PathBuf,
        radial: Arc<crate::radial::store::RadialStore>,
    ) {
        let repaint_context = self.egui_ctx.clone();
        match crate::target_repair::TargetRepairClient::new(
            root,
            radial,
            self.hotfolder_runtime.repair_store_handle(),
            self.target_repair_usage_handoff.clone(),
            Arc::clone(&self.target_repair_usage_barrier),
            move || repaint_context.request_repaint(),
        ) {
            Ok(client) => self.target_repair_client = Some(client),
            Err(error) => self.report_error_message("target.repair.startup", error.message),
        }
    }

    pub(crate) fn submit_target_repair(
        &self,
        selected: crate::target_repair::SelectedTarget,
        replacement: PathBuf,
        expected_kind: crate::target_repair::FilesystemTargetKind,
        presentation_token: u64,
    ) -> Result<(), crate::target_repair::RepairFailure> {
        let client = self.target_repair_client.as_ref().ok_or_else(|| {
            crate::target_repair::RepairFailure {
                kind: crate::target_repair::RepairFailureKind::WorkerUnavailable,
                message: "Target repair is unavailable.",
            }
        })?;
        let usage = crate::target_repair::LiveUsageSnapshot::capture(
            &self.usage,
            Arc::clone(&self.target_repair_usage_generation),
            Arc::clone(&self.target_repair_usage_barrier),
            Arc::clone(&self.target_repair_deferred_usage),
            self.target_repair_usage_handoff.clone(),
        )?;
        client.submit(
            crate::target_repair::RepairRequest::new(selected, replacement)
                .with_expected_replacement_kind(expected_kind)
                .with_presentation_token(presentation_token)
                .with_live_usage(usage),
        )
    }

    fn process_target_repair_completions(&mut self) {
        let completions = self
            .target_repair_client
            .as_ref()
            .map(crate::target_repair::TargetRepairClient::poll_completions)
            .unwrap_or_default();
        for completion in completions {
            self.editor.complete_target_repair(&completion);
            match completion.result {
                Ok(commit) => self
                    .publish_target_repair_actions(commit.custom_actions, commit.actions_version),
                Err(error) => self.report_error_message("target.repair", error.message),
            }
        }
        let handoff = {
            let usage_barrier = match self.target_repair_usage_barrier.try_lock() {
                Ok(guard) => Some(guard),
                Err(std::sync::TryLockError::Poisoned(error)) => Some(error.into_inner()),
                Err(std::sync::TryLockError::WouldBlock) => None,
            };
            if let Some(_usage_barrier) = usage_barrier
                && let Some(client) = self.target_repair_client.as_ref()
            {
                let applied =
                    client.apply_usage_handoff(&mut self.usage, &self.target_repair_deferred_usage);
                if applied.changed {
                    self.target_repair_usage_generation
                        .fetch_add(1, Ordering::AcqRel);
                }
                applied
            } else {
                crate::target_repair::UsageHandoffApply::default()
            }
        };
        if handoff.conflict {
            self.report_error_message(
                "target.repair.usage",
                "Usage could not be reconciled safely after target repair; no usage values were merged.",
            );
        }
    }

    fn publish_target_repair_actions(
        &mut self,
        custom_actions: Option<Vec<Action>>,
        committed_version: Option<u64>,
    ) {
        match (custom_actions, committed_version) {
            (Some(custom_actions), Some(committed_version))
                if crate::actions::actions_version() == committed_version =>
            {
                self.publish_custom_actions_with_indexed_tail(custom_actions);
            }
            (Some(_), _) | (_, Some(_)) => {
                // A newer action owner revision has already won. Let the
                // normal watcher path load that authoritative store instead
                // of replacing it with this older repair snapshot. An
                // incomplete receipt is handled the same safe way.
                let _ = self
                    .event_tx
                    .send_as(WatchEvent::Actions, EventOrigin::FileWatcher);
            }
            (None, None) => {}
        }
    }

    pub fn install_hotkey_directory_document_revision(
        &mut self,
        revision: Arc<AtomicU64>,
        main_wake: Sender<()>,
    ) {
        self.paste_sequence_document_revision = revision;
        self.hotkey_directory_main_wake = Some(main_wake);
    }

    pub(crate) fn request_hotkey_directory_open(&mut self) {
        self.hotkey_directory_dialog.request_open();
    }

    pub(crate) fn paste_sequence_document_revision_notifier(
        &self,
    ) -> PasteSequenceDocumentRevisionNotifier {
        PasteSequenceDocumentRevisionNotifier::new(
            Arc::clone(&self.paste_sequence_document_revision),
            self.hotkey_directory_main_wake.clone(),
        )
    }

    pub(crate) fn settings_save_close_preferences(
        &self,
    ) -> crate::settings_save::LauncherClosePreferences {
        let file_search_ui_preferences = self
            .file_search_dialog
            .ui_preferences_dirty
            .then(|| self.file_search_dialog.ui_preferences.clone());
        crate::settings_save::LauncherClosePreferences {
            window_size: self.window_size,
            pinned_panels: self.pinned_panels.clone(),
            clipboard_modify_dialog_size: (
                self.clipboard_modify_dialog.persisted_window_size.x,
                self.clipboard_modify_dialog.persisted_window_size.y,
            ),
            file_search_ui_preferences,
        }
    }

    pub fn set_startup_persistence_context(
        &mut self,
        recovery: crate::persistence::RecoveryStartupResult,
    ) {
        let mut diagnostics = Vec::new();
        if let Some(diagnostic) = self.startup_settings_diagnostic.as_ref() {
            let error = diagnostic.error();
            diagnostics.push(SafeStartupDiagnostic {
                label: "Settings startup".into(),
                path: Some(error.path().to_path_buf()),
                summary: format!("{} failed: {error}", error.operation()),
            });
        }
        if let Some(error) = self.actions_persistence_diagnostic.as_ref() {
            diagnostics.push(SafeStartupDiagnostic {
                label: "Actions startup".into(),
                path: Some(error.path().to_path_buf()),
                summary: format!("{} failed: {error}", error.operation()),
            });
        }
        if let Some(diagnostic) = recovery.diagnostic.as_ref() {
            diagnostics.push(SafeStartupDiagnostic {
                label: "Startup recovery".into(),
                path: None,
                summary: diagnostic.to_string(),
            });
        } else if let Some(action) = recovery.applied.as_ref() {
            diagnostics.push(SafeStartupDiagnostic {
                label: "Startup recovery applied".into(),
                path: None,
                summary: format!("Applied {action:?} before persistent stores loaded"),
            });
        }
        self.startup_recovery = recovery;
        self.data_recovery_dialog
            .set_startup_diagnostics(diagnostics);
    }

    pub(crate) fn update_custom_actions(
        &mut self,
        mutate: impl FnOnce(&mut Vec<Action>) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        let committed = crate::actions::update_actions(&self.actions_path, mutate)?;
        self.publish_custom_actions_with_indexed_tail(committed);
        Ok(())
    }

    /// Publish a new custom prefix while retaining the already-indexed tail.
    ///
    /// Local persistence and external watcher reloads share this boundary so
    /// neither path reconstructs indexed actions from the filesystem. Version
    /// changes remain owned by the persistence/watcher caller.
    fn publish_custom_actions_with_indexed_tail(&mut self, custom: Vec<Action>) {
        let mut timer = crate::performance::track_c::Timer::start(
            crate::performance::track_c::Phase::ActionPublishPrepare,
        );
        let indexed = {
            #[cfg(test)]
            let _track_c_tail_copy =
                track_c_action_observer::scope(track_c_action_observer::Phase::TailCopy);
            self.actions
                .iter()
                .skip(self.custom_len.min(self.actions.len()))
                .cloned()
                .collect::<Vec<_>>()
        };
        #[cfg(test)]
        track_c_action_observer::add(
            track_c_action_observer::Counter::TailActionsCloned,
            indexed.len(),
        );
        timer.set_work_units(indexed.len());
        timer.finish(crate::performance::track_c::Outcome::Completed);
        self.publish_actions(custom, indexed);
    }

    fn publish_actions(
        &mut self,
        custom: Vec<Action>,
        additional: impl IntoIterator<Item = Action>,
    ) {
        let mut timer = crate::performance::track_c::Timer::start(
            crate::performance::track_c::Phase::ActionPublishCommit,
        );
        self.custom_len = custom.len();
        let catalog_len = {
            #[cfg(test)]
            let _track_c_catalog_assembly =
                track_c_action_observer::scope(track_c_action_observer::Phase::CatalogAssemblyArc);
            let mut actions = custom;
            actions.extend(additional);
            let catalog_len = actions.len();
            self.actions = Arc::new(actions);
            catalog_len
        };
        self.plugins
            .set_missing_custom_actions(Arc::new(self.actions[..self.custom_len].to_vec()));
        #[cfg(test)]
        track_c_action_observer::add(
            track_c_action_observer::Counter::CatalogAssemblyActions,
            catalog_len,
        );
        timer.set_work_units(catalog_len);
        self.update_action_cache();
        {
            #[cfg(test)]
            let _track_c_same_query_refresh =
                track_c_action_observer::scope(track_c_action_observer::Phase::SameQueryRefresh);
            self.request_background_query_refresh();
        }
        timer.finish(crate::performance::track_c::Outcome::Completed);
    }

    pub fn plugin_enabled(&self, name: &str) -> bool {
        match &self.enabled_plugins {
            Some(set) => set.contains(name),
            None => true,
        }
    }

    pub fn enabled_plugins_list(&self) -> Option<Vec<String>> {
        self.enabled_plugins
            .as_ref()
            .map(|set| set.iter().cloned().collect())
    }
    pub fn add_toast(&mut self, toast: Toast) {
        #[cfg(test)]
        self.test_toast_messages.push(toast.text.text().to_string());
        push_toast(&mut self.toasts, toast);
    }

    pub fn add_error_toast(&mut self, msg: impl Into<String>) {
        if self.enable_toasts && self.show_error_toasts {
            self.add_toast(Toast {
                text: msg.into().into(),
                kind: ToastKind::Error,
                options: ToastOptions::default().duration_in_seconds(self.toast_duration as f64),

                style: Default::default(),
            });
        }
    }

    pub(crate) fn request_radial_submenu_migration_restore(&mut self) {
        let Some(client) = radial_control_client() else {
            self.report_error_message(
                "radial.migration.restore",
                "Radial migration service is unavailable.",
            );
            return;
        };
        if let Err(error) = client
            .send(crate::radial::control::RadialControlRequest::RestoreSubmenuPresentationMigration)
        {
            self.report_error_message("radial.migration.restore", error.to_string());
        }
    }

    fn show_radial_placement_failure(&mut self, ctx: &egui::Context, handle_ui_actions: bool) {
        if !self.radial_placement_viewport.present_requested() {
            return;
        }
        let notice = self.radial_placement_viewport.notice().cloned();
        let close_because_resolved = notice.is_none();
        let focus_requested = self.radial_placement_viewport.take_focus_request();
        #[derive(Clone, Copy)]
        enum Choice {
            Designer,
            Cascade,
            Dismiss,
        }
        let mut choice = None;
        let mut native_close_requested = false;
        let viewport_id = radial_placement_viewport_id();
        let viewport_builder = egui::ViewportBuilder::default()
            .with_title("Radial submenu placement")
            .with_inner_size([440.0, 170.0])
            .with_min_inner_size([360.0, 130.0])
            .with_resizable(false)
            .with_visible(true)
            .with_always_on_top();
        ctx.show_viewport_immediate(viewport_id, viewport_builder, |child, class| {
            let independent_viewport = class == egui::ViewportClass::Immediate;
            if independent_viewport && focus_requested {
                child.send_viewport_cmd(egui::ViewportCommand::Focus);
            }
            if close_because_resolved {
                if independent_viewport {
                    child.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                return;
            }
            if handle_ui_actions
                && independent_viewport
                && child.input(|input| input.viewport().close_requested())
            {
                native_close_requested = true;
                child.send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }

            let Some(notice) = notice.as_ref() else {
                return;
            };
            egui::CentralPanel::default().show(child, |ui| {
                ui.heading("Radial submenu could not fit");
                ui.label(format!(
                    "{} could not be opened from {}.",
                    notice.child_menu_id, notice.parent_menu_id
                ));
                ui.label(&notice.message);
                ui.small(
                    "You can edit the menu in Designer or deliberately switch this parent to Cascade.",
                );
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Open Designer").clicked() && handle_ui_actions {
                        choice = Some(Choice::Designer);
                    }
                    if notice.can_switch_parent_to_cascade()
                        && ui.button("Switch this parent to Cascade").clicked()
                        && handle_ui_actions
                    {
                        choice = Some(Choice::Cascade);
                    }
                    if ui.button("Dismiss").clicked() && handle_ui_actions {
                        choice = Some(Choice::Dismiss);
                    }
                });
            });
            if independent_viewport
                && matches!(choice, Some(Choice::Designer | Choice::Dismiss))
            {
                child.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
        self.radial_placement_viewport.mark_viewport_registered();
        if close_because_resolved || native_close_requested {
            self.radial_placement_viewport.mark_viewport_closed();
        }
        match choice {
            Some(Choice::Designer) => {
                if let Some(notice) = notice.as_ref() {
                    self.open_radial_designer_from_placement(notice, ctx);
                }
            }
            Some(Choice::Cascade) => {
                let Some(notice) = notice else {
                    return;
                };
                let Some(client) = radial_control_client() else {
                    self.radial_placement_viewport.update_message(
                        "Could not switch this parent to Cascade: radial control service is unavailable.",
                    );
                    self.report_error_message(
                        "radial.placement",
                        "Radial control service is unavailable.",
                    );
                    return;
                };
                match self.request_radial_placement_cascade(&notice, &client) {
                    Ok(true) => {}
                    Ok(false) => return,
                    Err(error) => {
                        self.radial_placement_viewport.update_message(format!(
                            "Could not switch {} to Cascade: {error}",
                            notice.parent_menu_id
                        ));
                        self.report_error_message("radial.placement", error.to_string());
                    }
                }
                ctx.request_repaint();
            }
            Some(Choice::Dismiss) => {
                if let Some(notice) = notice.as_ref() {
                    self.dismiss_radial_placement_notice(notice);
                }
                ctx.request_repaint();
            }
            None => {}
        }
    }

    fn radial_placement_notice_is_current(
        &self,
        notice: &state::RadialPlacementFailureNotice,
    ) -> bool {
        self.radial_placement_viewport
            .notice()
            .is_some_and(|active| {
                active.session_id == notice.session_id
                    && active.parent_frame_id == notice.parent_frame_id
                    && active.parent_menu_id == notice.parent_menu_id
                    && active.child_menu_id == notice.child_menu_id
            })
    }

    fn open_radial_designer_from_placement(
        &mut self,
        notice: &state::RadialPlacementFailureNotice,
        ctx: &egui::Context,
    ) -> bool {
        if !self.radial_placement_notice_is_current(notice) {
            return false;
        }
        self.radial_placement_viewport.mark_viewport_closed();
        // The Designer is an independent deferred viewport. Opening it from
        // placement recovery must not show or move the launcher's root grid.
        self.focus_panel(Panel::RadialEditor);
        ctx.request_repaint();
        true
    }

    fn dismiss_radial_placement_notice(
        &mut self,
        notice: &state::RadialPlacementFailureNotice,
    ) -> bool {
        if !self.radial_placement_notice_is_current(notice) {
            return false;
        }
        self.radial_placement_viewport.dismiss_notice();
        self.radial_placement_viewport.mark_viewport_closed();
        true
    }

    fn request_radial_placement_cascade(
        &self,
        notice: &state::RadialPlacementFailureNotice,
        client: &crate::radial::control::RadialControlClient,
    ) -> Result<bool, crate::radial::control::RadialControlError> {
        if !notice.can_switch_parent_to_cascade()
            || !self.radial_placement_notice_is_current(notice)
        {
            return Ok(false);
        }
        client
            .send(
                crate::radial::control::RadialControlRequest::SetActiveParentSubmenuCascade {
                    session_id: notice.session_id.clone(),
                    parent_frame_id: notice.parent_frame_id,
                    parent_menu_id: notice.parent_menu_id.clone(),
                },
            )
            .map(|()| true)
    }

    fn set_inline_error(&mut self, msg: String) {
        self.error = Some(msg);
        self.error_time = Some(Instant::now());
    }

    pub fn report_error(&mut self, context: &'static str, err: impl Display) {
        self.report_error_message(context, err.to_string());
    }

    pub fn report_ui_error(&mut self, err: UiErrorEvent) {
        self.report_error_message(err.context, err.message);
    }

    pub fn report_error_message(&mut self, context: &'static str, msg: impl Into<String>) {
        let msg = msg.into();
        tracing::error!(context, error = %msg);
        append_toast_log(&format!("[error:{context}] {msg}"));
        if self.show_inline_errors {
            self.set_inline_error(msg.clone());
        }
        if self.enable_toasts && self.show_error_toasts {
            self.add_toast(Toast {
                text: msg.into(),
                kind: ToastKind::Error,
                options: ToastOptions::default().duration_in_seconds(self.toast_duration as f64),

                style: Default::default(),
            });
        }
    }

    fn should_render_inline_error(&self) -> bool {
        self.show_inline_errors && self.error.is_some()
    }

    fn open_settings_dialog(&mut self) {
        if !self.show_settings && !self.settings_save_pending() {
            match Settings::load(&self.settings_path) {
                Ok(settings) => {
                    if let Err(error) = self
                        .coordinate_tool
                        .apply_loaded_preferences(settings.coordinate_tool.clone())
                    {
                        self.report_error_message("coordinate_tool.settings_reload", error);
                    }
                    self.settings_editor = SettingsEditor::new_with_plugins(&settings);
                }
                Err(e) => {
                    let msg = format!("Failed to load settings: {e}");
                    self.report_error_message("settings_dialog.load", msg);
                }
            }
        }
        self.show_settings = true;
    }

    /// Opens the main settings at playback controls, preserving any open draft.
    pub(crate) fn open_paste_sequence_settings_dialog(&mut self) {
        let settings_was_open = self.show_settings;
        self.open_settings_dialog();
        self.settings_editor.request_control_navigation(
            "paste.next.enabled",
            settings_was_open.then(|| self.jump_mode_ui.settings_instance()),
            &self.plugins,
        );
    }

    pub(crate) fn paste_sequence_shortcut_summary(&self) -> String {
        self.sequence_routes.as_ref().map_or_else(
            || "Playback shortcut status unavailable; waiting for runtime acknowledgement.".into(),
            crate::paste_sequence::SequenceRoutePresentation::summary,
        )
    }

    pub(crate) fn should_keep_paste_sequence_dialog_visible_during_playback(&self) -> bool {
        self.sequence_routes.as_ref().map_or(
            self.settings_editor
                .baseline_settings
                .paste_sequence
                .keep_dialog_visible_during_playback,
            |routes| routes.keep_dialog_visible_during_playback,
        )
    }

    /// Central runtime gate for Jump Mode. The value changes only after the
    /// Settings save/apply path commits the editor's persisted settings.
    pub(crate) fn jump_mode_enabled(&self) -> bool {
        self.jump_mode_ui.is_enabled()
    }

    pub(crate) fn settings_navigation_owner_instance(&self) -> u64 {
        self.jump_mode_ui.settings_instance()
    }

    pub(crate) fn jump_mode_session_active(&self) -> bool {
        self.jump_mode_ui.active_session().is_some()
    }

    pub(crate) fn set_jump_mode_enabled(&mut self, enabled: bool) {
        self.jump_mode_ui.set_enabled(enabled);
    }

    /// Opens the standalone Mouse Gesture Settings dialog.
    ///
    /// This is exposed so non-`gui` modules (e.g. `settings_editor`) can
    /// trigger it without reaching into private `LauncherApp` fields.
    pub fn open_mouse_gesture_settings_dialog(&mut self) {
        self.mouse_gesture_settings_dialog.open();
    }

    pub(crate) fn open_mouse_settings_dialog(&mut self) {
        if !self.mouse_settings_dialog.open {
            let preferences = self.coordinate_tool.preferences().clone();
            self.mouse_settings_dialog.open(&preferences);
        }
    }

    pub(crate) fn is_mouse_settings_dialog_open(&self) -> bool {
        self.mouse_settings_dialog.open
    }

    pub(crate) fn is_mouse_gesture_settings_dialog_open(&self) -> bool {
        self.mouse_gesture_settings_dialog.open
    }

    pub(crate) fn is_brightness_dialog_open(&self) -> bool {
        self.brightness_dialog.open
    }

    pub(crate) fn brightness_settings_control_available(&self) -> Option<bool> {
        self.brightness_dialog.settings_control_available()
    }

    pub(crate) fn open_brightness_settings_dialog(&mut self) {
        self.brightness_dialog.open();
        self.focus_panel(Panel::BrightnessDialog);
        self.egui_ctx.request_repaint_of(egui::ViewportId::ROOT);
    }

    fn retire_pending_brightness_navigation(&mut self) {
        let is_current_brightness_request = self
            .settings_editor
            .pending_settings_navigation
            .as_ref()
            .is_some_and(|request| {
                request.control_id == "brightness.level"
                    && request.owner
                        == crate::settings_editor::catalog::SettingsOwner::BrightnessDialog
                    && request.section
                        == crate::settings_editor::catalog::SettingsSection::Brightness
            });
        if is_current_brightness_request {
            self.settings_editor.pending_settings_navigation = None;
        }
    }

    pub fn open_clipboard_modify_dialog(&mut self) {
        self.clipboard_modify_dialog.open_section(
            ClipboardModifyDialogSection::Modify,
            &crate::clipboard_modify::runtime::clipboard_service(),
        );
    }

    pub fn open_clipboard_modify_config_file(&mut self) {
        let path = self.clipboard_modify_runtime.store.path.clone();
        if !path.exists() {
            let model = crate::clipboard_modify::config::default_model();
            if let Err(err) = crate::clipboard_modify::config::save_model_atomic(&path, &model) {
                self.report_error_message(
                    "clipboard_modify.config.create",
                    format!("Failed to create Clipboard Modify config: {err}"),
                );
                return;
            }
        }
        if let Err(err) = open::that(&path) {
            self.report_error_message(
                "clipboard_modify.config.open",
                format!("Failed to open {}: {err}", path.display()),
            );
        }
    }

    pub fn reload_clipboard_modify_config(&mut self) {
        match self.clipboard_modify_runtime.reload_now() {
            Ok(catalog) => {
                self.refresh_clipboard_modify_catalog(catalog, true);
                self.handle_clipboard_modify_gui_event(
                    ClipboardModifyGuiEvent::ConfigurationReloadSuccess,
                );
            }
            Err(err) => {
                self.handle_clipboard_modify_gui_event(
                    ClipboardModifyGuiEvent::ConfigurationReloadFailure(err.to_string()),
                );
            }
        }
    }

    pub fn reset_clipboard_modify_config_to_factory_defaults(&mut self) {
        match self.clipboard_modify_runtime.reset_to_factory_defaults() {
            Ok(catalog) => self.refresh_clipboard_modify_catalog(catalog, true),
            Err(err) => self.report_error_message("clipboard_modify.config.reset", err.to_string()),
        }
    }

    pub fn apply_file_search_settings(
        &mut self,
        mut settings: crate::file_search::settings::FileSearchSettings,
    ) {
        if self.file_search_dialog.ui_preferences_dirty {
            settings.ui_preferences = self.file_search_dialog.ui_preferences.clone();
        }
        for diagnostic in settings.validate() {
            tracing::warn!(%diagnostic, "file_search settings warning during runtime apply");
        }
        self.file_search_dialog
            .cancel_search(&mut self.file_search_coordinator);
        self.file_search_coordinator
            .reconfigure_from_settings(settings.clone());
        self.file_search_dialog.case_sensitive = settings.case_sensitive;
        self.file_search_dialog.include_hidden = settings.include_hidden_files;
        self.file_search_dialog
            .set_ui_preferences(settings.ui_preferences.clone());
        self.file_search_dialog.settings = settings;
    }

    pub(crate) fn handle_batch_rename_ui_command(&mut self, command: BatchRenameUiCommand) {
        match command {
            BatchRenameUiCommand::SelectRoot(root) => self.start_batch_rename_scan(root),
            BatchRenameUiCommand::Rescan | BatchRenameUiCommand::RecursiveChanged => {
                if let Some(root) = self.batch_rename_dialog.root_path().map(Path::to_path_buf) {
                    self.start_batch_rename_scan(root);
                }
            }
            BatchRenameUiCommand::ApplyConfirmed => {
                let plan = self.batch_rename_dialog.take_pending_intent();
                let Some(plan) = plan else {
                    self.batch_rename_dialog.set_status_message(
                        "The confirmed preview is no longer available. Refresh and review it again."
                            .to_owned(),
                    );
                    return;
                };
                let Some(coordinator) = self.batch_rename_coordinator.as_ref() else {
                    self.batch_rename_dialog.set_status_message(
                        "The rename worker is unavailable. Refresh the preview before trying again."
                            .to_owned(),
                    );
                    return;
                };
                match coordinator.submit_apply(plan) {
                    Ok(()) => self.batch_rename_dialog.mark_operation_submitted(),
                    Err(error) => self.batch_rename_dialog.set_status_message(format!(
                        "The confirmed batch was not started: {error}"
                    )),
                }
            }
            BatchRenameUiCommand::UndoConfirmed(transaction_id) => {
                let Some(coordinator) = self.batch_rename_coordinator.as_ref() else {
                    self.batch_rename_dialog.set_status_message(
                        "The rename worker is unavailable; recovery data remains saved.".to_owned(),
                    );
                    return;
                };
                match coordinator.submit_undo(transaction_id) {
                    Ok(()) => self.batch_rename_dialog.mark_operation_submitted(),
                    Err(error) => self
                        .batch_rename_dialog
                        .set_status_message(format!("Undo was not started: {error}")),
                }
            }
            BatchRenameUiCommand::CancelOperation => {
                if let Some(coordinator) = self.batch_rename_coordinator.as_ref() {
                    coordinator.cancel_operation();
                }
            }
            BatchRenameUiCommand::Closed => {
                self.end_batch_rename_session();
                self.panel_states.batch_rename_dialog = false;
                self.panel_stack
                    .retain(|panel| *panel != Panel::BatchRenameDialog);
            }
        }
    }

    fn start_batch_rename_scan(&mut self, root: PathBuf) {
        if self.batch_rename_coordinator.is_none() {
            let journal_path =
                crate::platform::app_data::AppDataRoot::from_settings_path(&self.settings_path)
                    .map(|root| {
                        root.path()
                            .join(crate::batch_rename_transaction::RECOVERY_JOURNAL_FILE)
                    })
                    .map_err(|error| std::io::Error::other(error.to_string()));
            let coordinator = journal_path
                .and_then(crate::file_operations::RenameScanCoordinator::new_with_journal);
            match coordinator {
                Ok(coordinator) => self.batch_rename_coordinator = Some(coordinator),
                Err(error) => {
                    self.batch_rename_dialog.fail_to_start_scan(
                        root,
                        format!("Could not start the directory scan: {error}"),
                    );
                    return;
                }
            }
        }
        if let Some(coordinator) = self.batch_rename_coordinator.as_ref() {
            self.batch_rename_dialog.start_scan(root, coordinator);
        }
    }

    pub(crate) fn poll_batch_rename_runtime(&mut self, ctx: &egui::Context) {
        if self.batch_rename_dialog.poll_scan() {
            ctx.request_repaint();
        }
        let runtime = self.batch_rename_runtime_snapshot();
        self.batch_rename_dialog.observe_runtime(&runtime);
        if self.batch_rename_dialog.has_pending_scan() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        if runtime.operation_active {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }

    fn batch_rename_runtime_snapshot(&self) -> crate::file_operations::RenameRuntimeSnapshot {
        self.batch_rename_coordinator
            .as_ref()
            .map(crate::file_operations::RenameScanCoordinator::runtime_snapshot)
            .unwrap_or_default()
    }

    fn end_batch_rename_session(&mut self) {
        if let Some(coordinator) = self.batch_rename_coordinator.as_ref() {
            coordinator.cancel_operation();
        }
        self.batch_rename_dialog.end_session();
    }

    pub(crate) fn shutdown_batch_rename_scanner(&mut self) {
        self.end_batch_rename_session();
        if let Some(coordinator) = self.batch_rename_coordinator.take() {
            if let Err(error) = coordinator.shutdown() {
                tracing::warn!(%error, "batch rename scanner did not shut down cleanly");
            }
            self.batch_rename_dialog.discard_finished_ticket();
        }
    }

    fn ensure_replace_executor(&mut self) -> Result<PathBuf, String> {
        let recovery_directory =
            crate::platform::app_data::AppDataRoot::from_settings_path(&self.settings_path)
                .map_err(|error| {
                    format!("could not locate application data for replacement recovery: {error}")
                })?
                .path()
                .join(crate::replace_executor::REPLACE_RECOVERY_DIRECTORY);
        self.search_replace_dialog
            .set_recovery_directory(recovery_directory.clone());
        if self.replace_executor_worker.is_none() {
            let executor = crate::replace_executor::ReplaceExecutor::new(
                recovery_directory.clone(),
            )
            .map_err(|error| format!("could not initialize replacement recovery: {error}"))?;
            self.replace_executor_worker = Some(
                crate::replace_executor::ReplaceExecutorWorker::new(executor).map_err(|error| {
                    format!("could not start the guarded replace worker: {error}")
                })?,
            );
        }
        self.search_replace_dialog.start_receipt_load()?;
        Ok(recovery_directory)
    }

    fn ensure_search_replace_runtime(&mut self) -> Result<(), String> {
        let _ = self.ensure_replace_executor()?;
        if self.replace_search_coordinator.is_none() {
            self.replace_search_coordinator = Some(
                crate::search_replace::SearchCoordinator::new()
                    .map_err(|error| format!("could not start the search worker: {error}"))?,
            );
        }
        if self.replace_preview_worker.is_none() {
            self.replace_preview_worker = Some(
                ReplacementPreviewWorker::new()
                    .map_err(|error| format!("could not start the preview worker: {error}"))?,
            );
        }
        Ok(())
    }

    pub(crate) fn handle_search_replace_ui_command(&mut self, command: SearchReplaceUiCommand) {
        match command {
            SearchReplaceUiCommand::SelectRoot(root) => self.search_replace_dialog.set_root(root),
            SearchReplaceUiCommand::Search => self.start_search_replace_search(),
            SearchReplaceUiCommand::CancelSearch => self.search_replace_dialog.cancel_search(),
            SearchReplaceUiCommand::ReviewApply => {}
            SearchReplaceUiCommand::ConfirmApply => {
                let setup = self.ensure_search_replace_runtime();
                if let Err(error) = setup {
                    self.search_replace_dialog.confirm_request_failed(error);
                    return;
                }
                let Some(job) = self.search_replace_dialog.take_confirm_job() else {
                    self.search_replace_dialog.set_status_message(
                        "The reviewed candidate is no longer current. Refresh and inspect it again.",
                    );
                    return;
                };
                let Some(worker) = self.replace_preview_worker.as_ref() else {
                    self.search_replace_dialog
                        .confirm_request_failed("The preview worker is unavailable.".into());
                    return;
                };
                if let Err(error) = worker.submit(job) {
                    self.search_replace_dialog.confirm_request_failed(error);
                }
            }
            SearchReplaceUiCommand::CancelApply | SearchReplaceUiCommand::CancelRecoveryAction => {
                self.search_replace_dialog.cancel_confirmation();
            }
            SearchReplaceUiCommand::ReviewRestore(transaction_id) => {
                self.search_replace_dialog.review_recovery(
                    transaction_id,
                    search_replace_dialog::RecoveryAction::Restore,
                )
            }
            SearchReplaceUiCommand::ReviewRecovery(transaction_id) => {
                self.search_replace_dialog.review_recovery(
                    transaction_id,
                    search_replace_dialog::RecoveryAction::Recover,
                )
            }
            SearchReplaceUiCommand::ConfirmRestore => {
                self.submit_search_replace_recovery(search_replace_dialog::RecoveryAction::Restore)
            }
            SearchReplaceUiCommand::ConfirmRecovery => {
                self.submit_search_replace_recovery(search_replace_dialog::RecoveryAction::Recover)
            }
            SearchReplaceUiCommand::CancelOperation => {
                if let Some(worker) = self.replace_executor_worker.as_ref() {
                    worker.cancel_active();
                }
            }
            SearchReplaceUiCommand::Closed => {
                self.end_search_replace_session();
                self.panel_states.search_replace_dialog = false;
                self.panel_stack
                    .retain(|panel| *panel != Panel::SearchReplaceDialog);
            }
        }
    }

    fn start_search_replace_search(&mut self) {
        if let Err(error) = self.ensure_search_replace_runtime() {
            self.search_replace_dialog.fail_search(error);
            return;
        }
        let request = match self.search_replace_dialog.build_search_request() {
            Ok(request) => request,
            Err(error) => {
                self.search_replace_dialog.fail_search(error);
                return;
            }
        };
        let Some(coordinator) = self.replace_search_coordinator.as_ref() else {
            self.search_replace_dialog
                .fail_search("The search worker is unavailable.".into());
            return;
        };
        match coordinator.submit(request) {
            Ok(ticket) => self.search_replace_dialog.record_search_ticket(ticket),
            Err(error) => self
                .search_replace_dialog
                .fail_search(format!("Could not start search: {error}")),
        }
    }

    fn submit_ready_search_replace_confirmation(&mut self) {
        let Some(request) = self.search_replace_dialog.take_confirmed_request() else {
            return;
        };
        let Some(worker) = self.replace_executor_worker.as_ref() else {
            self.search_replace_dialog
                .confirm_request_failed("The guarded replace worker is unavailable.".into());
            return;
        };
        let snapshot = worker.snapshot();
        if snapshot.active || snapshot.result.is_some() || snapshot.shutdown_requested {
            self.search_replace_dialog.confirm_request_failed(
                "The guarded replace worker is busy. The reviewed candidate was not submitted; refresh and review it again.".into(),
            );
            return;
        }
        match worker.submit_apply(request) {
            Ok(()) => {
                self.replace_operation_pending = true;
                self.search_replace_dialog.confirm_request_submitted();
            }
            Err(error) => self.search_replace_dialog.confirm_request_failed(format!(
                "The guarded replace worker refused the reviewed request: {error}"
            )),
        }
    }

    fn submit_search_replace_recovery(
        &mut self,
        expected_action: search_replace_dialog::RecoveryAction,
    ) {
        let Some((transaction_id, action)) = self.search_replace_dialog.take_recovery_intent()
        else {
            return;
        };
        if action != expected_action {
            self.search_replace_dialog.set_status_message(
                "The recovery review changed. Reopen and confirm the intended receipt action.",
            );
            return;
        }
        if let Err(error) = self.ensure_replace_executor() {
            self.search_replace_dialog.set_status_message(error);
            return;
        }
        let Some(worker) = self.replace_executor_worker.as_ref() else {
            self.search_replace_dialog.set_status_message(
                "The guarded replace worker is unavailable; the receipt remains saved.",
            );
            return;
        };
        let result = match action {
            search_replace_dialog::RecoveryAction::Restore => worker.submit_restore(transaction_id),
            search_replace_dialog::RecoveryAction::Recover => {
                worker.submit_recovery(transaction_id)
            }
        };
        match result {
            Ok(()) => {
                self.replace_operation_pending = true;
                self.search_replace_dialog.complete_recovery_submission();
            }
            Err(error) => self.search_replace_dialog.set_status_message(format!(
                "The requested recovery operation was not started: {error}"
            )),
        }
    }

    pub(crate) fn poll_search_replace_runtime(&mut self, ctx: &egui::Context) {
        let replace_snapshot = self
            .replace_operation_pending
            .then(|| {
                self.replace_executor_worker
                    .as_ref()
                    .map(|worker| worker.snapshot())
            })
            .flatten();
        let changed = self.search_replace_dialog.poll_background(
            self.replace_search_coordinator.as_ref(),
            self.replace_preview_worker.as_ref(),
            replace_snapshot.as_ref(),
            self.replace_executor_worker.as_ref(),
        );
        if let Some(snapshot) = replace_snapshot {
            if snapshot.result.is_some() || snapshot.worker_exited {
                self.replace_operation_pending = false;
                self.replace_worker_snapshot =
                    crate::replace_executor::ReplaceWorkerSnapshot::default();
                if snapshot.worker_exited && snapshot.result.is_none() {
                    self.search_replace_dialog.set_status_message(
                        "The guarded replace worker exited before returning a receipt. Existing recovery data remains on disk.",
                    );
                }
            } else {
                self.replace_worker_snapshot = snapshot;
            }
        } else {
            self.replace_worker_snapshot =
                crate::replace_executor::ReplaceWorkerSnapshot::default();
        }
        if changed {
            ctx.request_repaint();
        }
        if self.search_replace_dialog.has_background_work(
            self.replace_preview_worker.as_ref(),
            self.replace_operation_pending,
        ) {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }

    fn end_search_replace_session(&mut self) {
        self.search_replace_dialog.end_session(
            self.replace_search_coordinator.as_ref(),
            self.replace_executor_worker.as_ref(),
        );
    }

    pub(crate) fn shutdown_search_replace_runtime(&mut self) {
        self.end_search_replace_session();
        if let Some(coordinator) = self.replace_search_coordinator.take()
            && let Err(error) = coordinator.shutdown()
        {
            tracing::warn!(%error, "search and replace scanner did not shut down cleanly");
        }
        if let Some(worker) = self.replace_preview_worker.take()
            && let Err(error) = worker.shutdown()
        {
            tracing::warn!(%error, "replacement preview worker did not shut down cleanly");
        }
        if let Some(worker) = self.replace_executor_worker.take()
            && let Err(error) = worker.shutdown()
        {
            tracing::warn!(%error, "guarded replace worker did not shut down cleanly");
        }
        if let Some(loader) = self.search_replace_dialog.take_receipt_loader()
            && let Err(error) = loader.shutdown()
        {
            tracing::warn!(%error, "replacement receipt reader did not shut down cleanly");
        }
        self.replace_operation_pending = false;
    }

    pub(crate) fn save_file_search_ui_preferences_if_dirty(&mut self) {
        if self.file_search_dialog.ui_preferences_dirty {
            self.handle_file_search_ui_command(FileSearchUiCommand::PersistPreferences(
                self.file_search_dialog.ui_preferences.clone(),
            ));
        }
    }

    pub(crate) fn handle_file_search_ui_command(&mut self, command: FileSearchUiCommand) {
        match command {
            FileSearchUiCommand::OpenHash(path) => self.open_hash_generator_for_file(path),
            FileSearchUiCommand::PersistPreferences(preferences) => {
                match crate::settings::Settings::update(&self.settings_path, |settings| {
                    let mut cfg = settings
                        .plugin_settings
                        .get("file_search")
                        .and_then(|value| serde_json::from_value(value.clone()).ok())
                        .unwrap_or_else(crate::file_search::settings::FileSearchSettings::default);
                    cfg.ui_preferences = preferences;
                    let value = serde_json::to_value(&cfg)?;
                    settings
                        .plugin_settings
                        .insert("file_search".to_owned(), value);
                    Ok(())
                }) {
                    Ok(settings) => {
                        if let Some(value) = settings.plugin_settings.get("file_search").cloned() {
                            match serde_json::from_value(value.clone()) {
                                Ok(committed) => {
                                    self.file_search_dialog.ui_preferences_dirty = false;
                                    self.apply_file_search_settings(committed);
                                    self.settings_editor
                                        .set_plugin_setting_value("file_search", value);
                                }
                                Err(error) => self.report_error_message(
                                    "file_search.preferences.publish",
                                    format!(
                                        "Failed to publish saved file-search preferences: {error}"
                                    ),
                                ),
                            }
                        }
                    }
                    Err(error) => {
                        let persisted = self.file_search_dialog.settings.ui_preferences.clone();
                        self.file_search_dialog.set_ui_preferences(persisted);
                        self.report_error_message(
                            "file_search.preferences.save",
                            format!("Failed to save file-search preferences: {error}"),
                        );
                    }
                }
            }
            FileSearchUiCommand::ConfigureRipgrep(path) => {
                match crate::settings::Settings::update(&self.settings_path, |settings| {
                    let mut cfg = settings
                        .plugin_settings
                        .get("file_search")
                        .and_then(|value| serde_json::from_value(value.clone()).ok())
                        .unwrap_or_else(crate::file_search::settings::FileSearchSettings::default);
                    cfg.ripgrep_executable_path = path;
                    let value = serde_json::to_value(&cfg)?;
                    settings
                        .plugin_settings
                        .insert("file_search".to_owned(), value);
                    Ok(())
                }) {
                    Ok(settings) => {
                        if let Some(value) = settings.plugin_settings.get("file_search").cloned() {
                            self.settings_editor
                                .set_plugin_setting_value("file_search", value.clone());
                            if let Ok(cfg) = serde_json::from_value(value) {
                                self.apply_file_search_settings(cfg);
                                self.file_search_dialog.warning_error_message =
                                    Some("ripgrep path saved for future searches.".to_owned());
                            }
                        }
                    }
                    Err(error) => self.report_error_message(
                        "file_search.ripgrep.save",
                        format!("Failed to save ripgrep path: {error}"),
                    ),
                }
            }
        }
    }

    pub(crate) fn open_hash_generator_for_file(&mut self, path: PathBuf) {
        self.hash_generator_dialog.open_with_file(path);
        self.focus_panel(Panel::HashGeneratorDialog);
    }

    pub(crate) fn merge_file_search_ui_preferences_into_settings(
        &mut self,
        settings: &mut crate::settings::Settings,
    ) {
        self.file_search_dialog.save_dirty_ui_preferences();
        if let Ok(value) = serde_json::to_value(&self.file_search_dialog.settings) {
            settings
                .plugin_settings
                .insert("file_search".to_owned(), value);
        }
    }

    pub fn open_theme_settings_dialog(&mut self) {
        if self.theme_settings_dialog_open {
            return;
        }
        self.theme_settings_dialog_open = true;
        let ctx = self.egui_ctx.clone();
        let settings_path = self.settings_path.clone();
        self.theme_settings_dialog
            .begin_session(&ctx, &settings_path);
    }

    pub fn is_theme_settings_dialog_open(&self) -> bool {
        self.theme_settings_dialog_open
    }

    pub(crate) fn theme_accent_override_is_set(&self) -> bool {
        self.theme_settings_dialog
            .draft
            .preferences
            .accent
            .is_some()
    }

    fn retire_settings_navigation_for_panel(&mut self, panel: Panel) {
        use crate::settings_editor::catalog::SettingsSection;

        match panel {
            Panel::Settings => self.settings_editor.retire_all_settings_navigation(),
            Panel::Plugins => self.settings_editor.retire_plugin_settings_navigation(),
            Panel::ThemeSettingsDialog => self
                .settings_editor
                .retire_settings_navigation_for_section(&SettingsSection::Theme),
            Panel::MouseSettingsDialog => self
                .settings_editor
                .retire_settings_navigation_for_section(&SettingsSection::Mouse),
            Panel::MouseGestureSettingsDialog => self
                .settings_editor
                .retire_settings_navigation_for_section(&SettingsSection::MouseGestures),
            Panel::BrightnessDialog => self
                .settings_editor
                .retire_settings_navigation_for_section(&SettingsSection::Brightness),
            _ => {}
        }
    }

    pub fn close_theme_settings_dialog(&mut self) {
        if self.pinned_panels.contains(&Panel::ThemeSettingsDialog) {
            self.theme_settings_dialog_open = true;
            self.panel_states.theme_settings_dialog = true;
            if !self.panel_stack.contains(&Panel::ThemeSettingsDialog) {
                self.panel_stack.push(Panel::ThemeSettingsDialog);
            }
            return;
        }
        self.retire_settings_navigation_for_panel(Panel::ThemeSettingsDialog);
        self.theme_settings_dialog_open = false;
        self.theme_settings_dialog.end_session(&self.egui_ctx);
        self.panel_states.theme_settings_dialog = false;
        self.panel_stack
            .retain(|panel| *panel != Panel::ThemeSettingsDialog);
        self.update_panel_stack();
    }

    pub(crate) fn finish_theme_settings_dialog_ui(
        &mut self,
        open: bool,
        state: ThemeSettingsDialogState,
    ) {
        self.theme_settings_dialog = state;
        let pinned_close = !open && self.pinned_panels.contains(&Panel::ThemeSettingsDialog);
        self.theme_settings_dialog_open = open || pinned_close;
        if pinned_close {
            self.update_panel_stack();
        } else if !open {
            self.close_theme_settings_dialog();
        }
    }
    fn apply_enabled_plugin_selection(&mut self, enabled_plugins: Option<HashSet<String>>) {
        let hotfolder_was_enabled = self.plugin_enabled("hotfolder");
        let hotfolder_will_be_enabled = enabled_plugins
            .as_ref()
            .is_none_or(|plugins| plugins.contains("hotfolder"));
        let paste_sequence_was_enabled = self
            .enabled_plugins
            .as_ref()
            .is_none_or(|plugins| plugins.contains("paste_sequence"));
        let paste_sequence_will_be_enabled = enabled_plugins
            .as_ref()
            .is_none_or(|plugins| plugins.contains("paste_sequence"));
        self.enabled_plugins = enabled_plugins;
        if paste_sequence_was_enabled && !paste_sequence_will_be_enabled {
            self.cancel_active_paste_sequence();
            self.close_paste_sequence_dialog();
        }

        // Keep the mouse-gesture runtime and all indexed actions in lockstep
        // with the plugin selection committed by Settings.
        crate::plugins::mouse_gestures::sync_enabled_plugins(self.enabled_plugins.as_ref());
        self.plugins
            .sync_enabled_plugins(self.enabled_plugins.as_ref());
        self.update_command_cache();

        if hotfolder_was_enabled && !hotfolder_will_be_enabled {
            // Retire GUI replies before asking the worker to cancel and stop.
            self.hotfolder_action_adapter.set_active(false);
            self.hotfolder_action_pump.set_active(false);
            if let Err(error) = self.hotfolder_runtime.stop() {
                tracing::warn!(reason = error, "Hot Folder stop request was not queued");
            }
        } else if !hotfolder_was_enabled && hotfolder_will_be_enabled {
            self.hotfolder_action_adapter.set_active(true);
            self.hotfolder_action_pump.set_active(true);
            if let Err(error) = self
                .hotfolder_runtime
                .start(self.hotfolder_capability_inputs())
            {
                tracing::warn!(reason = error, "Hot Folder start request was not queued");
            }
        }
    }

    pub fn update_paths(
        &mut self,
        plugin_dirs: Option<Vec<String>>,
        index_config: crate::indexer::coordinator::IndexConfig,
        enabled_plugins: Option<HashSet<String>>,
        enabled_capabilities: Option<std::collections::HashMap<String, Vec<String>>>,
        offscreen_pos: Option<(i32, i32)>,
        enable_toasts: Option<bool>,
        show_inline_errors: Option<bool>,
        show_error_toasts: Option<bool>,
        toast_duration: Option<f32>,
        fuzzy_weight: Option<f32>,
        usage_weight: Option<f32>,
        match_exact: Option<bool>,
        follow_mouse: Option<bool>,
        static_enabled: Option<bool>,
        static_pos: Option<(i32, i32)>,
        static_size: Option<(i32, i32)>,
        hide_after_run: Option<bool>,
        clear_query_after_run: Option<bool>,
        require_confirm_destructive: Option<bool>,
        timer_refresh: Option<f32>,
        disable_timer_updates: Option<bool>,
        preserve_command: Option<bool>,
        query_autocomplete: Option<bool>,
        net_refresh: Option<f32>,
        net_unit: Option<crate::settings::NetUnit>,
        screenshot_dir: Option<String>,
        screenshot_save_file: Option<bool>,
        screenshot_use_editor: Option<bool>,
        screenshot_auto_save: Option<bool>,
        always_on_top: Option<bool>,
        page_jump: Option<usize>,
        note_settings: Option<NoteSettings>,
        note_panel_default_size: Option<(f32, f32)>,
        note_save_on_close: Option<bool>,
        note_confirm_discard_unsaved_changes: Option<bool>,
        note_always_overwrite: Option<bool>,
        note_images_as_links: Option<bool>,
        note_show_details: Option<bool>,
        note_more_limit: Option<usize>,
        show_dashboard_diagnostics: Option<bool>,
    ) {
        self.plugin_dirs = plugin_dirs;
        self.apply_enabled_plugin_selection(enabled_plugins);
        self.enabled_capabilities = enabled_capabilities;
        if let Some((x, y)) = offscreen_pos {
            self.offscreen_pos = (x as f32, y as f32);
        }
        if let Some(v) = enable_toasts {
            self.enable_toasts = v;
        }
        if let Some(v) = show_inline_errors {
            self.show_inline_errors = v;
        }
        if let Some(v) = show_error_toasts {
            self.show_error_toasts = v;
        }
        if let Some(v) = toast_duration {
            self.toast_duration = v;
        }
        if let Some(v) = fuzzy_weight {
            self.fuzzy_weight = v;
        }
        if let Some(v) = usage_weight {
            self.usage_weight = v;
        }
        if let Some(v) = match_exact {
            self.match_exact = v;
        }
        if let Some(v) = follow_mouse {
            self.follow_mouse = v;
        }
        let requested_static_enabled = static_enabled.unwrap_or(self.static_location_enabled);
        let requested_static_pos = static_pos.or(self.static_pos);
        let requested_static_size = static_size.or(self.static_size);
        let (normalized_static_enabled, normalized_static_pos, normalized_static_size) =
            normalize_static_window_config(
                self.follow_mouse,
                requested_static_enabled,
                requested_static_pos,
                requested_static_size,
            );
        self.static_location_enabled = normalized_static_enabled;
        self.static_pos = normalized_static_pos;
        self.static_size = normalized_static_size;
        if let Some(v) = hide_after_run {
            self.hide_after_run = v;
        }
        if let Some(v) = clear_query_after_run {
            self.clear_query_after_run = v;
        }
        if let Some(v) = require_confirm_destructive {
            self.require_confirm_destructive = v;
        }
        if let Some(v) = timer_refresh {
            self.timer_refresh = v;
        }
        if let Some(v) = disable_timer_updates {
            self.disable_timer_updates = v;
        }
        if let Some(v) = preserve_command {
            self.preserve_command = v;
        }
        if let Some(v) = query_autocomplete {
            self.query_autocomplete = v;
            if !v {
                self.suggestions.clear();
            } else {
                self.update_suggestions();
            }
        }
        if let Some(v) = net_refresh {
            self.net_refresh = v;
        }
        if let Some(v) = net_unit {
            self.net_unit = v;
        }
        if screenshot_dir.is_some() {
            self.screenshot_dir = screenshot_dir;
        }
        if let Some(v) = screenshot_save_file {
            self.screenshot_save_file = v;
        }
        if let Some(v) = screenshot_use_editor {
            self.screenshot_use_editor = v;
        }
        if let Some(v) = screenshot_auto_save {
            self.screenshot_auto_save = v;
        }
        if let Some(v) = always_on_top {
            self.always_on_top = v;
        }
        if let Some(v) = page_jump {
            self.page_jump = v;
        }
        if let Some(v) = note_settings {
            self.note_settings = v;
        }
        if let Some(v) = note_panel_default_size {
            self.note_panel_default_size = v;
        }
        if let Some(v) = note_save_on_close {
            self.note_save_on_close = v;
        }
        if let Some(v) = note_confirm_discard_unsaved_changes {
            self.note_confirm_discard_unsaved_changes = v;
        }
        if let Some(v) = note_always_overwrite {
            self.note_always_overwrite = v;
        }
        if let Some(v) = note_images_as_links {
            self.note_images_as_links = v;
        }
        if let Some(v) = note_show_details {
            self.note_show_details = v;
        }
        if let Some(v) = note_more_limit {
            self.note_more_limit = v;
        }
        if let Some(v) = show_dashboard_diagnostics {
            self.show_dashboard_diagnostics = v;
        }
        self.request_index_config(index_config);
        self.recompute_query_results_layout();
        crate::plugins::mouse_gestures::sync_enabled_plugins(self.enabled_plugins.as_ref());
    }

    pub fn new(
        ctx: &egui::Context,
        actions: Arc<Vec<Action>>,
        custom_len: usize,
        plugins: PluginManager,
        actions_path: String,
        settings_path: String,
        settings: Settings,
        plugin_dirs: Option<Vec<String>>,
        index_paths: Option<Vec<String>>,
        enabled_plugins: Option<HashSet<String>>,
        enabled_capabilities: Option<std::collections::HashMap<String, Vec<String>>>,
        visible_flag: Arc<AtomicBool>,
        restore_flag: Arc<AtomicBool>,
        help_flag: Arc<AtomicBool>,
    ) -> Self {
        // egui snapshots context plugins when a UI run begins, so install the
        // font-revision owner before the first eframe pass can begin.
        crate::gui::font_revision::register(ctx);
        crate::gui::theme::initialize_runtime(ctx, &settings.theme);
        crate::plugins::macros::configure_search_runtime(&settings, &actions_path);
        let desired_index_config = crate::indexer::coordinator::IndexConfig::new(
            index_paths.unwrap_or_default(),
            settings.max_indexed_items,
        );
        let (tx, rx) = event_channel::channel();
        let event_sink = register_observed_event_sender(tx.clone(), ViewportWake::root(ctx));
        let mut watchers = Vec::new();
        let mut toasts = Toasts::new().anchor(egui::Align2::RIGHT_TOP, [-10.0, 10.0]);
        let enable_toasts = settings.enable_toasts;
        let show_inline_errors = settings.show_inline_errors;
        let show_error_toasts = settings.show_error_toasts;
        let toast_duration = settings.toast_duration;
        let data_root = crate::platform::app_data::AppDataRoot::from_settings_path(&settings_path)
            .expect("LauncherApp settings path must resolve an application data root");
        let regex_tester_dialog = RegexTesterDialogState::with_data_root(&data_root);
        let data_recovery_dialog = DataRecoveryDialog::new(data_root.clone(), settings.clone(), {
            let ctx = ctx.clone();
            move || ctx.request_repaint()
        });
        use std::path::Path;

        let dashboard_timer = crate::performance::Timer::start();
        let dashboard_path = DashboardConfig::path_for(
            settings
                .dashboard
                .config_path
                .as_deref()
                .unwrap_or("dashboard.json"),
        );
        let dashboard_registry = WidgetRegistry::with_defaults();
        let dashboard_event_cb = std::sync::Arc::new({
            let tx = tx.with_origin(EventOrigin::Dashboard);
            let ctx = ctx.clone();
            move |ev: DashboardEvent| {
                if tx.send(WatchEvent::Dashboard(ev)).is_ok() {
                    ctx.request_repaint();
                }
            }
        });
        let mut dashboard = Dashboard::new(
            &dashboard_path,
            dashboard_registry.clone(),
            Some(dashboard_event_cb),
        );
        dashboard.attach_watcher();
        dashboard_timer.finish("startup.dashboard_construction");
        let watcher_timer = crate::performance::Timer::start();

        let (folder_aliases, folder_aliases_lc) =
            Self::try_folder_alias_maps().unwrap_or_else(|error| {
                tracing::error!(%error, "folder aliases startup retained invalid persisted file");
                Default::default()
            });
        let (bookmark_aliases, bookmark_aliases_lc) = Self::try_bookmark_alias_maps()
            .unwrap_or_else(|error| {
                tracing::error!(%error, "bookmark aliases startup retained invalid persisted file");
                Default::default()
            });
        let clipboard_modify_catalog = plugins.clipboard_modifier_catalog();
        let (clipboard_modify_runtime, loaded_clipboard_modify) =
            ClipboardModifyRuntime::new(Path::new(&settings_path), clipboard_modify_catalog);
        let clipboard_modify_config_diagnostic = loaded_clipboard_modify.state.startup_diagnostic();
        let clipboard_modify_watcher =
            crate::clipboard_modify::watch::ClipboardModifyWatcher::start(
                clipboard_modify_runtime.store.clone(),
                Duration::from_millis(150),
                {
                    let ctx = ctx.clone();
                    move || ctx.request_repaint()
                },
            )
            .map_err(
                |error| tracing::warn!(%error, "failed to watch Clipboard Modify configuration"),
            )
            .ok();

        #[cfg(not(test))]
        match watch_file(
            Path::new(&actions_path),
            tx.clone(),
            WatchEvent::Actions,
            ctx.clone(),
        ) {
            Ok(w) => watchers.push(w),
            Err(e) => {
                tracing::error!("watch error: {:?}", e);
                if enable_toasts && show_error_toasts {
                    push_toast(
                        &mut toasts,
                        Toast {
                            text: format!("Failed to watch {}", actions_path).into(),
                            kind: ToastKind::Error,
                            options: ToastOptions::default()
                                .duration_in_seconds(toast_duration as f64),

                            style: Default::default(),
                        },
                    );
                }
            }
        }

        #[cfg(test)]
        {
            if Path::new(&actions_path).exists() {
                if let Ok(w) = watch_file(
                    Path::new(&actions_path),
                    tx.clone(),
                    WatchEvent::Actions,
                    ctx.clone(),
                ) {
                    watchers.push(w);
                }
            } else if enable_toasts && show_error_toasts {
                push_toast(
                    &mut toasts,
                    Toast {
                        text: format!("Failed to watch {}", actions_path).into(),
                        kind: ToastKind::Error,
                        options: ToastOptions::default().duration_in_seconds(toast_duration as f64),

                        style: Default::default(),
                    },
                );
            }
        }

        #[cfg(not(test))]
        match watch_file(
            Path::new(crate::plugins::folders::FOLDERS_FILE),
            tx.clone(),
            WatchEvent::Folders,
            ctx.clone(),
        ) {
            Ok(w) => watchers.push(w),
            Err(e) => {
                tracing::error!("watch error: {:?}", e);
                if enable_toasts && show_error_toasts {
                    push_toast(
                        &mut toasts,
                        Toast {
                            text: "Failed to watch folders.json".into(),
                            kind: ToastKind::Error,
                            options: ToastOptions::default()
                                .duration_in_seconds(toast_duration as f64),

                            style: Default::default(),
                        },
                    );
                }
            }
        }

        #[cfg(test)]
        {
            let path = Path::new(crate::plugins::folders::FOLDERS_FILE);
            if path.exists() {
                if let Ok(w) = watch_file(path, tx.clone(), WatchEvent::Folders, ctx.clone()) {
                    watchers.push(w);
                }
            } else if enable_toasts && show_error_toasts {
                push_toast(
                    &mut toasts,
                    Toast {
                        text: "Failed to watch folders.json".into(),
                        kind: ToastKind::Error,
                        options: ToastOptions::default().duration_in_seconds(toast_duration as f64),

                        style: Default::default(),
                    },
                );
            }
        }

        #[cfg(not(test))]
        match watch_file(
            Path::new(crate::plugins::bookmarks::BOOKMARKS_FILE),
            tx.clone(),
            WatchEvent::Bookmarks,
            ctx.clone(),
        ) {
            Ok(w) => watchers.push(w),
            Err(e) => {
                tracing::error!("watch error: {:?}", e);
                if enable_toasts && show_error_toasts {
                    push_toast(
                        &mut toasts,
                        Toast {
                            text: "Failed to watch bookmarks.json".into(),
                            kind: ToastKind::Error,
                            options: ToastOptions::default()
                                .duration_in_seconds(toast_duration as f64),

                            style: Default::default(),
                        },
                    );
                }
            }
        }

        #[cfg(test)]
        {
            let path = Path::new(crate::plugins::bookmarks::BOOKMARKS_FILE);
            if path.exists() {
                if let Ok(w) = watch_file(path, tx.clone(), WatchEvent::Bookmarks, ctx.clone()) {
                    watchers.push(w);
                }
            } else if enable_toasts && show_error_toasts {
                push_toast(
                    &mut toasts,
                    Toast {
                        text: "Failed to watch bookmarks.json".into(),
                        kind: ToastKind::Error,
                        options: ToastOptions::default().duration_in_seconds(toast_duration as f64),

                        style: Default::default(),
                    },
                );
            }
        }

        let notes_dir = crate::plugins::note::notes_dir();
        let _ = std::fs::create_dir_all(&notes_dir);

        #[cfg(not(test))]
        for (path, event) in [
            (
                Path::new(crate::plugins::clipboard::CLIPBOARD_FILE),
                WatchEvent::Clipboard,
            ),
            (
                Path::new(crate::plugins::snippets::SNIPPETS_FILE),
                WatchEvent::Snippets,
            ),
            (
                Path::new(crate::plugins::todo::TODO_FILE),
                WatchEvent::Todos,
            ),
            (
                Path::new(crate::plugins::fav::FAV_FILE),
                WatchEvent::Favorites,
            ),
            (notes_dir.as_path(), WatchEvent::Notes),
            (
                Path::new(crate::mouse_gestures::db::GESTURES_FILE),
                WatchEvent::Gestures,
            ),
            (
                Path::new(crate::mouse_gestures::usage::GESTURES_USAGE_FILE),
                WatchEvent::Gestures,
            ),
        ] {
            match watch_file(path, tx.clone(), event, ctx.clone()) {
                Ok(w) => watchers.push(w),
                Err(e) => tracing::error!("watch error: {:?}", e),
            }
        }

        #[cfg(test)]
        for (path, event) in [
            (
                Path::new(crate::plugins::clipboard::CLIPBOARD_FILE),
                WatchEvent::Clipboard,
            ),
            (
                Path::new(crate::plugins::snippets::SNIPPETS_FILE),
                WatchEvent::Snippets,
            ),
            (
                Path::new(crate::plugins::todo::TODO_FILE),
                WatchEvent::Todos,
            ),
            (
                Path::new(crate::plugins::fav::FAV_FILE),
                WatchEvent::Favorites,
            ),
            (notes_dir.as_path(), WatchEvent::Notes),
            (
                Path::new(crate::mouse_gestures::db::GESTURES_FILE),
                WatchEvent::Gestures,
            ),
            (
                Path::new(crate::mouse_gestures::usage::GESTURES_USAGE_FILE),
                WatchEvent::Gestures,
            ),
        ] {
            if path.exists()
                && let Ok(w) = watch_file(path, tx.clone(), event, ctx.clone())
            {
                watchers.push(w);
            }
        }

        let initial_visible = visible_flag.load(Ordering::SeqCst);

        let offscreen_pos = {
            let (x, y) = settings.offscreen_pos.unwrap_or((2000, 2000));
            (x as f32, y as f32)
        };
        let win_size = settings.window_size.unwrap_or((400, 220));
        let query_scale = settings.query_scale.unwrap_or(1.0).min(5.0);
        let list_scale = settings.list_scale.unwrap_or(1.0).min(5.0);
        let static_pos = settings.static_pos;
        let static_size = settings.static_size;
        let follow_mouse = settings.follow_mouse;
        let (static_enabled, static_pos, static_size) = normalize_static_window_config(
            follow_mouse,
            settings.static_location_enabled,
            static_pos,
            static_size,
        );

        let note_external_open = settings
            .plugin_settings
            .get("note")
            .and_then(|v| serde_json::from_value::<NotePluginSettings>(v.clone()).ok())
            .map(|s| s.external_open)
            .unwrap_or(NoteExternalOpen::Wezterm);

        let file_search_settings = settings
            .plugin_settings
            .get("file_search")
            .and_then(|v| {
                serde_json::from_value::<crate::file_search::settings::FileSearchSettings>(
                    v.clone(),
                )
                .ok()
            })
            .unwrap_or_default();

        let clipboard_modify_settings = settings
            .plugin_settings
            .get("clipboard_modify")
            .and_then(|v| serde_json::from_value::<ClipboardModifyPluginSettings>(v.clone()).ok())
            .unwrap_or_default();

        watcher_timer.finish("startup.watcher_initialization");
        let settings_editor = SettingsEditor::new_with_plugins(&settings);
        let multi_manager =
            MultiManagerState::load_or_default(&settings.multi_manager, &settings_path);
        plugins
            .internal_services()
            .workspace_catalog
            .attach(&multi_manager.workspaces);
        #[cfg(test)]
        let plugin_editor = if plugins.is_inert_for_test() {
            PluginEditor::new_with_manager_for_test(&settings, &plugins)
        } else {
            PluginEditor::new(&settings)
        };
        #[cfg(not(test))]
        let plugin_editor = PluginEditor::new(&settings);
        let actions_by_id = ActionIdLookup::new(Arc::clone(&actions));
        let dashboard_runtime = DashboardRuntime::start({
            let ctx = ctx.clone();
            move || ctx.request_repaint()
        });
        let dashboard_data_cache = dashboard_runtime.cache();
        let mkmacro_timer = crate::performance::Timer::start();
        let mut mkmacro_dialog = MkMacroDialog::new_with_authoring_context(
            Arc::clone(&plugins.internal_services().mkmacro_store),
            mkmacro_dialog::MkMacroAuthoringContext {
                launcher_actions: Arc::clone(&actions),
            },
        );
        mkmacro_dialog.set_macro_properties_preferences(settings.mkmacro_properties.clone());
        let pinboard_capture_backend: Arc<dyn crate::mkmacro::ScreenCaptureBackend> =
            Arc::new(crate::mkmacro::WindowsScreenCaptureBackend::system());
        let visual_capture_dependencies = production_visual_capture_dependencies(
            &mkmacro_dialog,
            Arc::clone(&plugins.internal_services().mkmacro_store),
            Arc::clone(&pinboard_capture_backend),
        );
        install_visual_capture(&mut mkmacro_dialog, visual_capture_dependencies);
        mkmacro_timer.finish("startup.mkmacro");
        let screen_draw_recovery_bridge =
            Arc::new(crate::screen_draw::ScreenDrawRecoveryBridge::default());
        let (hotfolder_action_adapter, hotfolder_action_pump) =
            hotfolder_dispatch::LauncherHotFolderActionAdapter::new(
                ctx.clone(),
                Arc::clone(&plugins.internal_services().mkmacro_store),
            );
        let hotfolder_runtime = hotfolder_runtime::HotFolderRuntimeDriver::new(
            data_root,
            hotfolder_action_adapter.clone(),
        );
        let mut app = Self {
            actions: Arc::clone(&actions),
            command_bus: Arc::new(crate::commands::CommandBus),
            hotfolder_action_adapter,
            hotfolder_action_pump,
            hotfolder_runtime,
            hotfolder_dialog: hotfolder_dialog::HotFolderDialog::default(),
            query: String::new(),
            query_history: QueryHistoryNavigator::default(),
            results: (*actions).clone(),
            root_list_geometry: render::RootListGeometryCache::default(),
            root_frame_logic: None,
            pending_ui_frame_receipt: None,
            #[cfg(feature = "qa-framebuffer-capture")]
            qa_framebuffer: None,
            indexing: indexing::IndexingOwner::new(desired_index_config),
            matcher: SkimMatcherV2::default(),
            error: None,
            error_time: None,
            plugins,
            screen_draw_controller: {
                let mut controller = crate::screen_draw::ScreenDrawController::default();
                let mut screen_draw_settings: crate::screen_draw::ScreenDrawSettings = settings
                    .plugin_settings
                    .get("screen_draw")
                    .cloned()
                    .and_then(|value| serde_json::from_value(value).ok())
                    .unwrap_or_default();
                screen_draw_settings.normalize();
                controller.update_settings(screen_draw_settings);
                controller.set_recovery_bridge(Arc::clone(&screen_draw_recovery_bridge));
                controller
            },
            coordinate_tool: coordinate_tool::CoordinateToolGui::new(
                settings_path.clone(),
                settings.coordinate_tool.clone(),
            ),
            coordinate_capture_parking: None,
            coordinate_capture_generation: 0,
            screen_draw_recovery_bridge,
            color_pick: color_pick::ColorPickLifecycle::default(),
            ocr: ocr::OcrLifecycle::default(),
            pinboard_capture: pinboard::PinboardCaptureLifecycle::new(
                mkmacro_dialog.visual_overlay_controller(),
                pinboard_capture_backend,
            ),
            pin_list_open: false,
            screen_draw_launcher_parking: None,
            screen_draw_restore_publication:
                screen_draw_restore::ScreenDrawRestorePublication::default(),
            screen_draw_toolbar: screen_draw_toolbar::ScreenDrawToolbarUi::default(),
            selected: None,
            action_sheet: ActionSheetState::default(),
            #[cfg(test)]
            test_last_activation: None,
            #[cfg(test)]
            test_activation_trace: Vec::new(),
            #[cfg(test)]
            test_sequence_context_capture_forbidden: false,
            #[cfg(test)]
            test_sequence_context_capture_calls: 0,
            usage: usage::load_usage(USAGE_FILE).unwrap_or_default(),
            target_repair_usage_generation: Arc::new(AtomicU64::new(0)),
            target_repair_usage_barrier: Arc::new(Mutex::new(())),
            target_repair_deferred_usage: Arc::new(Mutex::new(HashMap::new())),
            target_repair_usage_handoff: crate::target_repair::UsageHandoff::default(),
            target_repair_client: None,
            registered_hotkeys: Mutex::new(HashMap::new()),
            show_editor: false,
            show_settings: false,
            show_plugins: false,
            actions_path,
            editor: ActionsEditor::default(),
            settings_editor,
            plugin_editor,
            settings_path,
            settings_save_workers: crate::settings_save::SettingsSaveWorkerRegistry::default(),
            pending_settings_save: None,
            settings_save_reconciliation_required: false,
            startup_settings_diagnostic: None,
            actions_persistence_diagnostic: None,
            startup_recovery: Default::default(),
            multi_manager,
            multi_manager_settings: settings.multi_manager.clone(),
            launcher_hwnd: None,
            multi_manager_dialog: MultiManagerDialog::default(),
            multi_manager_settings_dialog: MultiManagerSettingsDialog::default(),
            radial_editor: {
                let mut editor = radial_editor::RadialEditorState::default();
                editor.set_preferences(settings.radial_designer.clone());
                Arc::new(Mutex::new(editor))
            },
            watchers,
            dashboard,
            dashboard_runtime,
            dashboard_data_cache,
            dashboard_initial_refresh_queued: false,
            dashboard_enabled: settings.dashboard.enabled,
            dashboard_show_when_empty: settings.dashboard.show_when_query_empty,
            dashboard_path: dashboard_path.to_string_lossy().to_string(),
            dashboard_default_location: settings.dashboard.default_location.clone(),
            reduce_dashboard_work_when_unfocused: settings.reduce_dashboard_work_when_unfocused,
            show_dashboard_diagnostics: settings.show_dashboard_diagnostics,
            dashboard_editor: DashboardEditorDialog::default(),
            show_dashboard_editor: false,
            rx,
            event_sink,
            event_tx: tx,
            radial_provider_search_capacity: Default::default(),
            radial_suppressed_provider_query: None,
            radial_queryexec_depth: None,
            radial_query_observation:
                crate::gui::query_observation::QueryObservationMailbox::from_environment(
                    crate::radial::acceptance_trace::enabled(),
                ),
            egui_ctx: ctx.clone(),
            virtual_desktop_interaction_token: 0,
            command_root_policy: crate::universal_actions::RootLauncherPolicy::Legacy,
            folder_aliases,
            folder_aliases_lc,
            bookmark_aliases,
            bookmark_aliases_lc,
            plugin_dirs,
            enabled_plugins,
            enabled_capabilities,
            root_window_bridge: RootWindowBridge::default(),
            snippet_origin_window: None,
            sequence_playback: crate::paste_sequence::SequencePlaybackController::default(),
            sequence_exit_requested: false,
            sequence_routes: None,
            hotkey_directory: None,
            hotkey_directory_dialog: hotkey_directory_dialog::HotkeyDirectoryDialog::default(),
            paste_sequence_document_revision: Arc::new(AtomicU64::new(0)),
            hotkey_directory_main_wake: None,
            visible_flag: visible_flag.clone(),
            restore_flag: restore_flag.clone(),
            visibility_revision: crate::visibility::VisibilityRevision::default(),
            authoring_catalog_cache: Mutex::new(None),
            #[cfg(test)]
            authoring_catalog_build_count: AtomicUsize::new(0),
            #[cfg(test)]
            test_unpublish_root_identity_before_visibility_reconcile: false,
            last_visible: initial_visible,
            offscreen_pos,
            window_size: win_size,
            window_pos: (0, 0),
            focus_query: false,
            pending_show_query_focus: None,
            query_focus_session: None,
            last_query_focus_geometry: None,
            move_cursor_end: false,
            toasts,
            #[cfg(test)]
            test_toast_messages: Vec::new(),
            #[cfg(test)]
            test_recorded_history_queries: Vec::new(),
            #[cfg(test)]
            test_skip_history_persistence: false,
            #[cfg(test)]
            test_screenshot_delivery: None,
            #[cfg(test)]
            test_root_rendered_rows: Vec::new(),
            #[cfg(test)]
            test_root_rendered_row_ids: Vec::new(),
            #[cfg(test)]
            test_root_rendered_row_rects: Vec::new(),
            #[cfg(test)]
            test_root_scroll_viewport_rect: None,
            #[cfg(test)]
            test_root_scroll_area_extent: None,
            #[cfg(test)]
            test_root_scroll_area_id: None,
            #[cfg(test)]
            test_defer_virtual_desktop_completion: false,
            enable_toasts,
            show_inline_errors,
            show_error_toasts,
            toast_duration,
            alias_dialog: AliasDialog::default(),
            bookmark_alias_dialog: BookmarkAliasDialog::default(),
            tempfile_alias_dialog: TempfileAliasDialog::default(),
            tempfile_dialog: TempfileDialog::default(),
            add_bookmark_dialog: AddBookmarkDialog::default(),
            help_window: {
                let mut help_window = HelpWindow::default();
                help_window.show_examples = settings.show_examples;
                help_window
            },
            timer_dialog: TimerDialog::default(),
            completion_dialog: TimerCompletionDialog::default(),
            shell_cmd_dialog: ShellCmdDialog::default(),
            snippet_dialog: SnippetDialog::default(),
            paste_sequence_dialog: PasteSequenceDialog::default(),
            hash_generator_dialog: HashGeneratorDialogState::default(),
            snippet_prompt_dialog: snippet_prompt_dialog::SnippetPromptDialog::default(),
            macro_dialog: MacroDialog::default(),
            mkmacro_dialog,
            macro_prompt: MacroPromptUi::default(),
            macro_prompt_broker: crate::mkmacro::production_prompt_broker(),
            macro_parameter_prompt: mkmacro_dialog::parameter_prompt::ParameterPromptUi::new(ctx),
            mouse_gestures_dialog: MgGesturesDialog::default(),
            mouse_gesture_settings_dialog: MouseGestureSettingsDialog::default(),
            mouse_settings_dialog: MouseSettingsDialog::default(),
            theme_settings_dialog_open: false,
            theme_settings_dialog: ThemeSettingsDialogState::default(),
            fav_dialog: FavDialog::default(),
            file_search_dialog: FileSearchDialogState::from_settings(file_search_settings.clone()),
            file_search_coordinator: SearchCoordinator::from_settings(file_search_settings),
            batch_rename_dialog: BatchRenameDialogState::default(),
            batch_rename_coordinator: None,
            search_replace_dialog: SearchReplaceDialogState::default(),
            replace_search_coordinator: None,
            replace_preview_worker: None,
            replace_executor_worker: None,
            replace_operation_pending: false,
            replace_worker_snapshot: crate::replace_executor::ReplaceWorkerSnapshot::default(),
            diff_dialog: DiffDialogState::default(),
            notes_dialog: NotesDialog::default(),
            note_graph_dialog: NoteGraphDialog::default(),
            unused_assets_dialog: UnusedAssetsDialog::default(),
            note_panels: Vec::new(),
            #[cfg(test)]
            note_mutation_refresh_count: 0,
            #[cfg(test)]
            note_mutation_cache_refresh_count: 0,
            #[cfg(test)]
            note_mutation_quick_notes_refresh_count: 0,
            #[cfg(test)]
            note_mutation_panel_invalidation_count: 0,
            #[cfg(test)]
            note_mutation_search_count: 0,
            image_panels: Vec::new(),
            screenshot_editors: Vec::new(),
            screenshot_capture: screenshot_capture::ScreenshotCaptureOwner::new(),
            screenshot_countdown: screenshot_countdown::ScreenshotCountdownPresentation::default(),
            screenshot_capture_parking: None,
            #[cfg(test)]
            test_screenshot_parking: None,
            pending_screenshot_completion: None,
            crop_dialog: crop_dialog::CropDialogState::default(),
            crop_screenshot_operation: None,
            crop_screenshot_capture: None,
            screen_draw_region_operation: None,
            todo_dialog: TodoDialog::default(),
            todo_view_dialog: TodoViewDialog::default(),
            clipboard_dialog: ClipboardDialog::default(),
            clipboard_modify_runtime,
            clipboard_modify_dialog: ClipboardModifyDialogState::with_initial_size(
                clipboard_modify_settings.dialog_width,
                clipboard_modify_settings.dialog_height,
            ),
            json_utility_dialog: JsonUtilityDialogState::default(),
            regex_tester_dialog,
            xml_toolkit_dialog: XmlToolkitDialogState::default(),
            table_utility_dialog: TableUtilityDialogState::default(),
            image_compare_dialog: ImageCompareDialogState::new(ctx),
            qr_dialog: QrDialogState::default(),
            clipboard_modify_config_diagnostic,
            clipboard_modify_watcher,
            clipboard_modify_hide_launcher_after_apply: clipboard_modify_settings
                .hide_launcher_after_apply,
            clipboard_modify_immediate: ImmediateExecutionCoordinator::new(clipboard_service()),
            clipboard_modify_events: Vec::new(),
            convert_panel: ConvertPanel::default(),
            volume_dialog: VolumeDialog::default(),
            brightness_dialog: BrightnessDialog::default(),
            cpu_list_dialog: CpuListDialog::default(),
            data_recovery_dialog,
            toast_log_dialog: ToastLogDialog::default(),
            calendar_popover: CalendarPopover::default(),
            calendar_event_editor: CalendarEventEditor::default(),
            calendar_event_details: CalendarEventDetails::default(),
            panel_stack: Vec::new(),
            panel_states: PanelStates::default(),
            pinned_panels: settings.pinned_panels.clone(),
            help_flag: help_flag.clone(),
            hotkey_str: settings.hotkey.clone(),
            quit_hotkey_str: settings.quit_hotkey.clone(),
            help_hotkey_str: settings.help_hotkey.clone(),
            query_scale,
            list_scale,
            show_launcher_icon: settings.show_launcher_icon,
            show_launcher_title: settings.show_launcher_title,
            jump_mode_ui: jump_mode::ui::JumpModeUi::new(settings.experimental_jump_mode_enabled),
            custom_len,
            history_limit: settings.history_limit,
            clipboard_limit: settings.clipboard_limit,
            fuzzy_weight: settings.fuzzy_weight,
            usage_weight: settings.usage_weight,
            match_exact: settings.match_exact,
            page_jump: settings.page_jump,
            query_results_layout: settings.query_results_layout.clone(),
            resolved_grid_layout: false,
            note_settings: settings.note.clone(),
            note_panel_default_size: settings.note_panel_default_size,
            note_save_on_close: settings.note_save_on_close,
            note_confirm_discard_unsaved_changes: settings.note_confirm_discard_unsaved_changes,
            note_always_overwrite: settings.note_always_overwrite,
            note_images_as_links: settings.note_images_as_links,
            note_show_details: settings.note_show_details,
            note_external_open,
            note_font_size: 16.0,
            note_more_limit: settings.note_more_limit,
            follow_mouse,
            static_location_enabled: static_enabled,
            static_pos,
            static_size,
            hide_after_run: settings.hide_after_run,
            always_on_top: settings.always_on_top,
            timer_refresh: settings.timer_refresh,
            disable_timer_updates: settings.disable_timer_updates,
            preserve_command: settings.preserve_command,
            clear_query_after_run: settings.clear_query_after_run,
            require_confirm_destructive: settings.require_confirm_destructive,
            radial_feature_settings: settings.radial.clone(),
            radial_expected_diagnostics: VecDeque::new(),
            radial_migration_receipt: settings.radial_submenu_migration.clone(),
            radial_placement_viewport: state::RadialPlacementViewportState::default(),
            query_autocomplete: settings.query_autocomplete,
            net_refresh: settings.net_refresh,
            net_unit: settings.net_unit,
            screenshot_dir: settings.screenshot_dir.clone(),
            screenshot_save_file: settings.screenshot_save_file,
            screenshot_auto_save: settings.screenshot_auto_save,
            screenshot_use_editor: settings.screenshot_use_editor,
            calendar_popover_open: false,
            calendar_editor_open: false,
            calendar_details_open: false,
            calendar_selected_date: None,
            calendar_selected_event: None,
            last_timer_update: Instant::now(),
            last_net_update: Instant::now(),
            last_stopwatch_update: Instant::now(),
            last_search_query: String::new(),
            last_results_valid: false,
            last_search_pending: false,
            last_search_provider_revision: 0,
            last_search_result_catalog_versions: None,
            last_search_result_catalog_versions_stable: false,
            #[cfg(test)]
            track_c_test_force_enabled: false,
            #[cfg(test)]
            track_c_test_provider_change_at_frame_end: false,
            #[cfg(test)]
            track_c_test_catalog_version_change_at_frame_end: false,
            track_c_query_observer: track_c_query_observer::Observer::new(
                crate::performance::enabled(),
            ),
            background_query_refresh_pending: false,
            last_search_provider_deferral: Default::default(),
            last_plugin_search_generation: 0,
            last_timer_query: false,
            last_stopwatch_query: false,
            last_note_search_change: None,
            pending_query: None,
            confirm_modal: ConfirmationModal::default(),
            pending_confirm: None,
            pending_universal_confirm: None,
            radial_preparations: HashMap::new(),
            radial_consumed_dispatches: VecDeque::new(),
            radial_current_preparation: None,
            pending_data_recovery: None,
            action_cache: Vec::new(),
            action_filter_metadata: Vec::new(),
            actions_by_id,
            command_cache: Vec::new(),
            command_search_cache: Vec::new(),
            completion_index: None,
            action_completion_dirty: false,
            command_completion_dirty: false,
            completion_rebuild_after: None,
            suggestions: Vec::new(),
            autocomplete_index: 0,
            vim_mode: false,
            file_search_window_open: false,
            file_search_selected_kind: crate::file_search::model::SearchKind::Filename,
            file_search_root: None,
            file_search_text: String::new(),
            file_search_active: false,
        };

        tracing::debug!("initial viewport visible: {}", initial_visible);
        apply_visibility(
            initial_visible,
            VisiblePlacementPolicy::ApplyConfiguredPlacement,
            &RootViewportCtx::with_window_bridge(ctx, app.root_window_bridge.clone()),
            offscreen_pos,
            follow_mouse,
            static_enabled,
            static_pos.map(|(x, y)| (x as f32, y as f32)),
            static_size.map(|(w, h)| (w as f32, h as f32)),
            (win_size.0 as f32, win_size.1 as f32),
        );
        // Creation precedes the first native frame. Preserve its activation
        // through the existing current visibility/restore request owner.
        if initial_visible {
            restore_flag.store(true, Ordering::SeqCst);
        }

        app.enforce_pinned();
        app.update_panel_stack();

        let action_cache_timer = crate::performance::Timer::start();
        app.update_action_cache();
        action_cache_timer.finish("startup.action_filter_metadata");
        let command_cache_timer = crate::performance::Timer::start();
        app.update_command_cache();
        command_cache_timer.finish("startup.command_search_cache");
        let completion_timer = crate::performance::Timer::start();
        app.rebuild_completion_index_now();
        completion_timer.finish("startup.completion_index");
        app.search();
        let repaint_context = ctx.clone();
        app.clipboard_modify_immediate
            .set_repaint_callback(Arc::new({
                let repaint_context = repaint_context.clone();
                move || repaint_context.request_repaint()
            }));
        app.plugins
            .set_search_repaint_callback(Arc::new(move || repaint_context.request_repaint()));
        crate::plugins::mouse_gestures::sync_enabled_plugins(app.enabled_plugins.as_ref());
        app.plugins
            .sync_enabled_plugins(app.enabled_plugins.as_ref());
        let hotfolder_enabled = app.plugin_enabled("hotfolder");
        app.hotfolder_action_adapter.set_active(hotfolder_enabled);
        app.hotfolder_action_pump.set_active(hotfolder_enabled);
        if hotfolder_enabled
            && let Err(error) = app
                .hotfolder_runtime
                .start(app.hotfolder_capability_inputs())
        {
            tracing::warn!(reason = error, "Hot Folder runtime did not start");
        }
        app.recompute_query_results_layout();
        app
    }

    /// Transfer the foreground snapshot captured immediately before initial ROOT creation.
    pub fn set_initial_snippet_origin_window(
        &mut self,
        origin: Option<crate::active_window::ExternalForegroundWindow>,
    ) {
        self.snippet_origin_window = origin;
    }

    pub fn maybe_refresh_timer_list(&mut self) {
        let trimmed = self.query.trim();
        if (trimmed.starts_with("timer list")
            || trimmed.starts_with("alarm list")
            || self.last_timer_query)
            && !self.disable_timer_updates
            && self.last_timer_update.elapsed().as_secs_f32() >= self.timer_refresh
        {
            self.last_results_valid = false;
            self.search();
            self.last_timer_update = Instant::now();
        }
    }

    #[cfg_attr(test, allow(dead_code))]
    pub fn maybe_refresh_stopwatch_list(&mut self) {
        let trimmed = self.query.trim();
        if (trimmed.starts_with("sw list") || self.last_stopwatch_query)
            && !crate::plugins::stopwatch::running_stopwatches().is_empty()
            && self.last_stopwatch_update.elapsed().as_secs_f32()
                >= crate::plugins::stopwatch::refresh_rate()
        {
            self.last_results_valid = false;
            self.search();
            self.last_stopwatch_update = Instant::now();
        }
    }

    pub(crate) fn accept_suggestion(&mut self, tab: bool) -> bool {
        if !self.query_autocomplete || self.suggestions.is_empty() {
            return false;
        }
        let suggestion = if tab {
            self.suggestions.get(self.autocomplete_index).cloned()
        } else {
            self.suggestions.first().cloned()
        };
        if let Some(s) = suggestion
            && s != self.query.to_lowercase()
        {
            let old_suggestions = self.suggestions.clone();
            let old_index = self.autocomplete_index;
            self.track_c_query_observer.begin_from(
                self.track_c_query_observer.tentative_start(),
                track_c_query_observer::Origin::Suggestion,
                &s,
            );
            self.query = s;
            self.move_cursor_end = true;
            self.search();
            if tab && !old_suggestions.is_empty() {
                self.suggestions = old_suggestions;
                self.autocomplete_index = (old_index + 1) % self.suggestions.len();
            }
            return true;
        }
        false
    }

    fn should_use_grid_layout(&self) -> bool {
        if !self.query_results_layout.enabled {
            return false;
        }

        // Global grid mode when capability gating is disabled.
        if !self.query_results_layout.respect_plugin_capability {
            return true;
        }

        let trimmed = self.query.trim();
        if trimmed.is_empty() {
            return false;
        }

        let (filtered_query, _) = split_action_filters(trimmed);
        let query_head = filtered_query
            .split_whitespace()
            .next()
            .map(str::to_ascii_lowercase);

        let mut prefixed_matches = Vec::new();
        for plugin in self.plugins.iter() {
            if let Some(enabled) = self.enabled_plugins.as_ref()
                && !enabled.contains(plugin.name())
            {
                continue;
            }

            let prefixes = plugin.query_prefixes();
            if prefixes.is_empty() {
                continue;
            }
            let Some(head) = query_head.as_deref() else {
                continue;
            };
            if prefixes
                .iter()
                .any(|prefix| prefix.eq_ignore_ascii_case(head))
            {
                prefixed_matches.push(plugin);
            }
        }

        // No plugin-prefixed context: use the configured grid layout.
        if prefixed_matches.is_empty() {
            return true;
        }

        // Ambiguous/mixed plugin context: safely fall back to list mode.
        if prefixed_matches.len() != 1 {
            return false;
        }

        let plugin = &**prefixed_matches[0];
        if self
            .query_results_layout
            .plugin_opt_out
            .iter()
            .any(|name| name.eq_ignore_ascii_case(plugin.name()))
        {
            return false;
        }

        let capabilities = plugin.capabilities();
        if capabilities.contains(&CAP_FORCE_LIST_RESULTS) {
            return false;
        }
        capabilities.contains(&CAP_GRID_RESULTS_COMPATIBLE)
    }

    pub fn recompute_query_results_layout(&mut self) {
        self.resolved_grid_layout = self.should_use_grid_layout();
    }

    /// Handle a keyboard navigation key. Returns the index of a selected
    /// action when `Enter` is pressed and a selection is available.
    pub fn handle_key(&mut self, key: egui::Key) -> Option<usize> {
        let cols = self.query_results_layout.cols.max(1);
        let move_to = |current: usize, delta: isize, max: usize| -> usize {
            current.saturating_add_signed(delta).min(max)
        };

        match key {
            egui::Key::ArrowDown | egui::Key::Num2 => {
                if !self.results.is_empty() {
                    let max = self.results.len() - 1;
                    self.selected = match self.selected {
                        Some(i) if self.resolved_grid_layout => {
                            Some(move_to(i, cols as isize, max))
                        }
                        Some(i) if i < max => Some(i + 1),
                        Some(i) => Some(i),
                        None => Some(0),
                    };
                }
                None
            }
            egui::Key::ArrowUp | egui::Key::Num8 => {
                if !self.results.is_empty() {
                    let max = self.results.len() - 1;
                    self.selected = match self.selected {
                        Some(i) if self.resolved_grid_layout => {
                            Some(move_to(i, -(cols as isize), max))
                        }
                        Some(i) if i > 0 => Some(i - 1),
                        Some(i) => Some(i.min(max)),
                        None => Some(0),
                    };
                }
                None
            }
            egui::Key::ArrowRight | egui::Key::Num6 => {
                if self.resolved_grid_layout && !self.results.is_empty() {
                    let max = self.results.len() - 1;
                    self.selected = match self.selected {
                        Some(i) if i < max => Some(i + 1),
                        Some(i) => Some(i),
                        None => Some(0),
                    };
                }
                None
            }
            egui::Key::ArrowLeft | egui::Key::Num4 => {
                if self.resolved_grid_layout && !self.results.is_empty() {
                    self.selected = match self.selected {
                        Some(i) if i > 0 => Some(i - 1),
                        Some(i) => Some(i),
                        None => Some(0),
                    };
                }
                None
            }
            egui::Key::PageDown => {
                if !self.results.is_empty() {
                    let max = self.results.len() - 1;
                    self.selected = match self.selected {
                        Some(i) => Some(i.saturating_add(self.page_jump).min(max)),
                        None => Some(0),
                    };
                }
                None
            }
            egui::Key::PageUp => {
                if !self.results.is_empty() {
                    self.selected = match self.selected {
                        Some(i) => Some(i.saturating_sub(self.page_jump)),
                        None => Some(0),
                    };
                }
                None
            }
            egui::Key::Enter => self.current_actionable_result_index(),
            _ => None,
        }
    }

    /// Shared target rule for normal Enter and the keyboard Action Sheet.
    pub(crate) fn current_actionable_result_index(&self) -> Option<usize> {
        self.selected
            .or_else(|| (self.results.len() == 1).then_some(0))
    }

    pub fn focus_input(&mut self) {
        self.focus_query = true;
    }

    fn resolve_pin_action(&self, pin: &HistoryPin) -> Option<Action> {
        let snapshot = self.dashboard_data_cache.snapshot();
        if pin.action_id.starts_with("snippet:run:") {
            return crate::plugins::snippets::resolve_snippet_run_action_from_entries(
                &pin.action_id,
                pin.args.as_deref(),
                &snapshot.snippets,
            );
        }

        if let Some(action) = self.actions_by_id.get(&pin.action_id) {
            return Some(action.clone());
        }

        let commands = self
            .plugins
            .commands_filtered(self.enabled_plugins.as_ref());
        if let Some(action) = commands.into_iter().find(|action| {
            action.action == pin.action_id && action.args.as_deref() == pin.args.as_deref()
        }) {
            return Some(action);
        }

        if let Some(action) = snapshot.processes.iter().find(|action| {
            action.action == pin.action_id && action.args.as_deref() == pin.args.as_deref()
        }) {
            return Some(action.clone());
        }

        if let Some(fav) = snapshot
            .favorites
            .iter()
            .find(|fav| fav.action == pin.action_id && fav.args.as_deref() == pin.args.as_deref())
        {
            return Some(Action {
                label: fav.label.clone(),
                desc: "Fav".into(),
                action: fav.action.clone(),
                args: fav.args.clone(),
            });
        }

        if let Some(slug) = pin.action_id.strip_prefix("note:open:")
            && let Some(note) = snapshot.notes.iter().find(|note| note.slug == slug)
        {
            return Some(Action {
                label: note.alias.as_ref().unwrap_or(&note.title).clone(),
                desc: "Note".into(),
                action: pin.action_id.clone(),
                args: None,
            });
        }

        if let Some(idx) = pin
            .action_id
            .strip_prefix("clipboard:copy:")
            .and_then(|s| s.parse::<usize>().ok())
            && let Some(entry) = snapshot.clipboard_history.get(idx)
        {
            return Some(Action {
                label: entry.clone(),
                desc: "Clipboard".into(),
                action: pin.action_id.clone(),
                args: None,
            });
        }

        if let Some(idx) = pin
            .action_id
            .strip_prefix("todo:done:")
            .and_then(|s| s.parse::<usize>().ok())
            && let Some(todo) = snapshot.todos.get(idx)
        {
            return Some(Action {
                label: format!("{} {}", if todo.done { "[x]" } else { "[ ]" }, todo.text),
                desc: "Todo".into(),
                action: pin.action_id.clone(),
                args: None,
            });
        }

        if let Some(idx) = pin
            .action_id
            .strip_prefix("todo:edit:")
            .and_then(|s| s.parse::<usize>().ok())
            && let Some(todo) = snapshot.todos.get(idx)
        {
            return Some(Action {
                label: format!("{} {}", if todo.done { "[x]" } else { "[ ]" }, todo.text),
                desc: "Todo".into(),
                action: pin.action_id.clone(),
                args: None,
            });
        }

        if let Some(idx) = pin
            .action_id
            .strip_prefix("todo:remove:")
            .and_then(|s| s.parse::<usize>().ok())
            && let Some(todo) = snapshot.todos.get(idx)
        {
            return Some(Action {
                label: format!("Remove todo {}", todo.text),
                desc: "Todo".into(),
                action: pin.action_id.clone(),
                args: None,
            });
        }

        if let Some(alias) = pin.action_id.strip_prefix("snippet:edit:")
            && snapshot.snippets.iter().any(|s| s.alias == alias)
        {
            return Some(Action {
                label: format!("Edit snippet {alias}"),
                desc: "Snippet".into(),
                action: pin.action_id.clone(),
                args: None,
            });
        }

        if let Some(alias) = pin.action_id.strip_prefix("snippet:remove:")
            && snapshot.snippets.iter().any(|s| s.alias == alias)
        {
            return Some(Action {
                label: format!("Remove snippet {alias}"),
                desc: "Snippet".into(),
                action: pin.action_id.clone(),
                args: None,
            });
        }

        // Pre-alias snippet favorites/pins stored literal clipboard commands.
        // Keep their exact saved action and presentation; indexed history copies
        // still require a current matching clipboard entry above.
        if pin.action_id.starts_with("clipboard:") && !pin.action_id.starts_with("clipboard:copy:")
        {
            return Some(Action {
                label: pin.label.clone(),
                desc: pin.desc.clone(),
                action: pin.action_id.clone(),
                args: pin.args.clone(),
            });
        }

        None
    }

    pub fn set_last_search_query(&mut self, s: String) {
        self.last_search_query = s;
    }

    pub fn set_last_timer_update(&mut self, t: Instant) {
        self.last_timer_update = t;
    }

    pub fn set_last_stopwatch_update(&mut self, t: Instant) {
        self.last_stopwatch_update = t;
    }

    #[cfg_attr(test, allow(dead_code))]
    pub fn last_timer_update(&self) -> Instant {
        self.last_timer_update
    }

    #[cfg_attr(test, allow(dead_code))]
    pub fn last_stopwatch_update(&self) -> Instant {
        self.last_stopwatch_update
    }

    pub fn get_last_search_query(&self) -> &str {
        &self.last_search_query
    }

    pub fn last_timer_query_flag(&self) -> bool {
        self.last_timer_query
    }

    pub fn last_stopwatch_query_flag(&self) -> bool {
        self.last_stopwatch_query
    }

    pub fn visible_flag_state(&self) -> bool {
        self.visible_flag.load(Ordering::SeqCst)
    }

    pub fn restore_flag_state(&self) -> bool {
        self.restore_flag.load(Ordering::SeqCst)
    }

    #[cfg(test)]
    pub(crate) fn mark_visible_root_for_test(&mut self) {
        self.visible_flag.store(true, Ordering::SeqCst);
        self.restore_flag.store(false, Ordering::SeqCst);
        self.last_visible = true;
    }

    pub fn install_visibility_revision(&mut self, revision: crate::visibility::VisibilityRevision) {
        self.visibility_revision = revision;
    }

    pub(crate) fn request_launcher_visibility(&self, visible: bool) -> u64 {
        self.request_launcher_state(Some(visible), None)
    }

    pub(crate) fn request_launcher_state(
        &self,
        visible: Option<bool>,
        restore: Option<bool>,
    ) -> u64 {
        let revision = self
            .visibility_revision
            .request(|| {
                if let Some(visible) = visible {
                    self.visible_flag.store(visible, Ordering::SeqCst);
                }
                if let Some(restore) = restore {
                    self.restore_flag.store(restore, Ordering::SeqCst);
                } else if visible == Some(true) {
                    self.restore_flag.store(true, Ordering::SeqCst);
                }
            })
            .0;
        self.egui_ctx.request_repaint_of(egui::ViewportId::ROOT);
        revision
    }

    pub(crate) fn toggle_launcher_visibility(&self) -> bool {
        self.visibility_revision
            .request(|| {
                let next = !self.visible_flag.load(Ordering::SeqCst);
                self.visible_flag.store(next, Ordering::SeqCst);
                next
            })
            .1
    }

    pub fn should_show_dashboard(&self, trimmed: &str) -> bool {
        self.dashboard_enabled && self.dashboard_show_when_empty && trimmed.trim().is_empty()
    }

    pub fn move_cursor_end_flag(&self) -> bool {
        self.move_cursor_end
    }

    const TRACKED_PANELS: [Panel; 57] = [
        Panel::AliasDialog,
        Panel::BookmarkAliasDialog,
        Panel::TempfileAliasDialog,
        Panel::TempfileDialog,
        Panel::AddBookmarkDialog,
        Panel::HelpOverlay,
        Panel::HelpWindow,
        Panel::TimerDialog,
        Panel::CompletionDialog,
        Panel::ShellCmdDialog,
        Panel::SnippetDialog,
        Panel::PasteSequenceDialog,
        Panel::HashGeneratorDialog,
        Panel::SnippetPromptDialog,
        Panel::MacroDialog,
        Panel::MkMacroDialog,
        Panel::MouseGesturesDialog,
        Panel::MouseGestureSettingsDialog,
        Panel::MouseSettingsDialog,
        Panel::ThemeSettingsDialog,
        Panel::HotkeyDirectoryDialog,
        Panel::FavDialog,
        Panel::FileSearchDialog,
        Panel::NotesDialog,
        Panel::NoteGraphDialog,
        Panel::UnusedAssetsDialog,
        Panel::NotePanel,
        Panel::ImagePanel,
        Panel::ScreenshotEditor,
        Panel::CropDialog,
        Panel::TodoDialog,
        Panel::TodoViewDialog,
        Panel::ClipboardDialog,
        Panel::ClipboardModifyDialog,
        Panel::JsonUtilityDialog,
        Panel::RegexTesterDialog,
        Panel::BatchRenameDialog,
        Panel::SearchReplaceDialog,
        Panel::XmlToolkitDialog,
        Panel::TableUtilityDialog,
        Panel::QrDialog,
        Panel::ConvertPanel,
        Panel::VolumeDialog,
        Panel::BrightnessDialog,
        Panel::CpuListDialog,
        Panel::DataRecoveryDialog,
        Panel::ToastLogDialog,
        Panel::CalendarPopover,
        Panel::CalendarEventEditor,
        Panel::CalendarEventDetails,
        Panel::Editor,
        Panel::Settings,
        Panel::Plugins,
        Panel::MultiManagerDialog,
        Panel::MultiManagerSettingsDialog,
        Panel::RadialEditor,
        Panel::ImageCompareDialog,
    ];

    fn is_panel_open(&self, panel: Panel) -> bool {
        match panel {
            Panel::AliasDialog => self.alias_dialog.open,
            Panel::BookmarkAliasDialog => self.bookmark_alias_dialog.open,
            Panel::TempfileAliasDialog => self.tempfile_alias_dialog.open,
            Panel::TempfileDialog => self.tempfile_dialog.open,
            Panel::AddBookmarkDialog => self.add_bookmark_dialog.open,
            Panel::HelpOverlay => self.help_window.overlay_open,
            Panel::HelpWindow => self.help_window.open,
            Panel::TimerDialog => self.timer_dialog.open,
            Panel::CompletionDialog => self.completion_dialog.open,
            Panel::ShellCmdDialog => self.shell_cmd_dialog.open,
            Panel::SnippetDialog => self.snippet_dialog.open,
            Panel::PasteSequenceDialog => self.paste_sequence_dialog.open,
            Panel::HashGeneratorDialog => self.hash_generator_dialog.open,
            Panel::SnippetPromptDialog => self.snippet_prompt_dialog.is_open(),
            Panel::MacroDialog => self.macro_dialog.open,
            Panel::MkMacroDialog => self.mkmacro_dialog.open,
            Panel::MouseGesturesDialog => self.mouse_gestures_dialog.open,
            Panel::MouseGestureSettingsDialog => self.mouse_gesture_settings_dialog.open,
            Panel::MouseSettingsDialog => self.mouse_settings_dialog.open,
            Panel::ThemeSettingsDialog => self.theme_settings_dialog_open,
            Panel::HotkeyDirectoryDialog => self.hotkey_directory_dialog.open,
            Panel::FavDialog => self.fav_dialog.open,
            Panel::FileSearchDialog => self.file_search_dialog.open,
            Panel::NotesDialog => self.notes_dialog.open,
            Panel::NoteGraphDialog => self.note_graph_dialog.open,
            Panel::UnusedAssetsDialog => self.unused_assets_dialog.open,
            Panel::NotePanel => !self.note_panels.is_empty(),
            Panel::ImagePanel => !self.image_panels.is_empty(),
            Panel::ScreenshotEditor => !self.screenshot_editors.is_empty(),
            Panel::CropDialog => self.crop_dialog.is_open(),
            Panel::TodoDialog => self.todo_dialog.open,
            Panel::TodoViewDialog => self.todo_view_dialog.open,
            Panel::ClipboardDialog => self.clipboard_dialog.open,
            Panel::ClipboardModifyDialog => self.clipboard_modify_dialog.open,
            Panel::JsonUtilityDialog => self.json_utility_dialog.open,
            Panel::RegexTesterDialog => self.regex_tester_dialog.open,
            Panel::BatchRenameDialog => self.batch_rename_dialog.open,
            Panel::SearchReplaceDialog => self.search_replace_dialog.open,
            Panel::XmlToolkitDialog => self.xml_toolkit_dialog.open,
            Panel::TableUtilityDialog => self.table_utility_dialog.open,
            Panel::ImageCompareDialog => self.image_compare_dialog.open,
            Panel::QrDialog => self.qr_dialog.open,
            Panel::ConvertPanel => self.convert_panel.open,
            Panel::VolumeDialog => self.volume_dialog.open,
            Panel::BrightnessDialog => self.brightness_dialog.open,
            Panel::CpuListDialog => self.cpu_list_dialog.open,
            Panel::DataRecoveryDialog => self.data_recovery_dialog.open,
            Panel::ToastLogDialog => self.toast_log_dialog.open,
            Panel::CalendarPopover => self.calendar_popover_open,
            Panel::CalendarEventEditor => self.calendar_editor_open,
            Panel::CalendarEventDetails => self.calendar_details_open,
            Panel::Editor => self.show_editor,
            Panel::Settings => self.show_settings,
            Panel::Plugins => self.show_plugins,
            Panel::MultiManagerDialog => self.multi_manager_dialog.open,
            Panel::MultiManagerSettingsDialog => self.multi_manager_settings_dialog.open,
            Panel::RadialEditor => self
                .radial_editor
                .lock()
                .map(|editor| editor.open)
                .unwrap_or(false),
        }
    }

    fn any_panel_open(&self) -> bool {
        self.pin_list_open
            || self.ocr_surface_visible()
            || Self::TRACKED_PANELS
                .iter()
                .copied()
                .any(|panel| self.is_panel_open(panel))
    }

    pub fn unregister_all_hotkeys(&self) {
        use windows::Win32::UI::Input::KeyboardAndMouse::UnregisterHotKey;
        if let Ok(mut registered_hotkeys) = self.registered_hotkeys.lock() {
            for id in registered_hotkeys.values() {
                unsafe {
                    let _ = UnregisterHotKey(None, *id as i32);
                }
            }
            registered_hotkeys.clear();
        } else {
            tracing::error!("failed to lock registered_hotkeys");
        }
    }

    /// Return the currently configured screenshot directory, if any.
    pub fn get_screenshot_dir(&self) -> Option<&str> {
        self.screenshot_dir.as_deref()
    }

    /// Whether screenshots copied to the clipboard are also saved to disk.
    pub fn get_screenshot_save_file(&self) -> bool {
        self.screenshot_save_file
    }

    /// Whether screenshots are saved automatically after editing.
    pub fn get_screenshot_auto_save(&self) -> bool {
        self.screenshot_auto_save
    }

    pub fn get_screenshot_use_editor(&self) -> bool {
        self.screenshot_use_editor
    }

    /// Close the top-most open dialog if any is visible.
    /// Returns `true` when a dialog was closed.
    pub fn close_front_dialog(&mut self) -> bool {
        if self.pin_list_open {
            self.pin_list_open = false;
            return true;
        }
        if self.ocr_surface_visible() {
            self.close_ocr_surface();
            return true;
        }
        if self.panel_stack.last() == Some(&Panel::Editor) && self.editor.repair_dialog_is_open() {
            self.editor.retire_repair_dialog();
            return true;
        }
        let panel = match self.panel_stack.pop() {
            Some(p) => p,
            None => return false,
        };
        if self.pinned_panels.contains(&panel) {
            self.panel_stack.push(panel);
            return false;
        }
        self.retire_settings_navigation_for_panel(panel);
        match panel {
            Panel::AliasDialog => {
                self.alias_dialog.open = false;
                self.panel_states.alias_dialog = false;
            }
            Panel::BookmarkAliasDialog => {
                self.bookmark_alias_dialog.open = false;
                self.panel_states.bookmark_alias_dialog = false;
            }
            Panel::TempfileAliasDialog => {
                self.tempfile_alias_dialog.open = false;
                self.panel_states.tempfile_alias_dialog = false;
            }
            Panel::TempfileDialog => {
                self.tempfile_dialog.open = false;
                self.panel_states.tempfile_dialog = false;
            }
            Panel::AddBookmarkDialog => {
                self.add_bookmark_dialog.open = false;
                self.panel_states.add_bookmark_dialog = false;
            }
            Panel::HelpOverlay => {
                self.help_window.overlay_open = false;
                self.panel_states.help_overlay = false;
            }
            Panel::HelpWindow => {
                self.help_window.open = false;
                self.panel_states.help_window = false;
            }
            Panel::TimerDialog => {
                self.timer_dialog.open = false;
                self.panel_states.timer_dialog = false;
            }
            Panel::CompletionDialog => {
                self.completion_dialog.open = false;
                self.panel_states.completion_dialog = false;
            }
            Panel::ShellCmdDialog => {
                self.shell_cmd_dialog.open = false;
                self.panel_states.shell_cmd_dialog = false;
            }
            Panel::SnippetDialog => {
                self.snippet_dialog.end_session();
                self.panel_states.snippet_dialog = false;
            }
            Panel::PasteSequenceDialog => {
                self.paste_sequence_dialog.end_session();
                self.panel_states.paste_sequence_dialog = false;
            }
            Panel::HashGeneratorDialog => {
                self.hash_generator_dialog.end_session();
                self.panel_states.hash_generator_dialog = false;
            }
            Panel::SnippetPromptDialog => {
                let _ = self.cancel_snippet_prompt();
            }
            Panel::MacroDialog => {
                self.macro_dialog.open = false;
                self.panel_states.macro_dialog = false;
            }
            Panel::MkMacroDialog => {
                self.mkmacro_dialog.action_editor.cancel();
                self.mkmacro_dialog.open = false;
                self.panel_states.mkmacro_dialog = false;
            }
            Panel::MouseGesturesDialog => {
                self.mouse_gestures_dialog.open = false;
                self.panel_states.mouse_gestures_dialog = false;
            }
            Panel::MouseGestureSettingsDialog => {
                self.mouse_gesture_settings_dialog.open = false;
                self.panel_states.mouse_gesture_settings_dialog = false;
            }
            Panel::MouseSettingsDialog => {
                self.mouse_settings_dialog.open = false;
                self.panel_states.mouse_settings_dialog = false;
            }
            Panel::ThemeSettingsDialog => {
                self.close_theme_settings_dialog();
            }
            Panel::HotkeyDirectoryDialog => {
                self.hotkey_directory_dialog.open = false;
                self.hotkey_directory_dialog.clear_message();
                self.panel_states.hotkey_directory_dialog = false;
            }
            Panel::FavDialog => {
                self.fav_dialog.open = false;
                self.panel_states.fav_dialog = false;
            }
            Panel::FileSearchDialog => {
                self.file_search_dialog.open = false;
                self.panel_states.file_search_dialog = false;
            }
            Panel::BatchRenameDialog => {
                self.end_batch_rename_session();
                self.panel_states.batch_rename_dialog = false;
            }
            Panel::SearchReplaceDialog => {
                self.end_search_replace_session();
                self.panel_states.search_replace_dialog = false;
            }
            Panel::NotesDialog => {
                self.notes_dialog.open = false;
                self.panel_states.notes_dialog = false;
            }
            Panel::NoteGraphDialog => {
                self.note_graph_dialog.open = false;
                self.panel_states.note_graph_dialog = false;
            }
            Panel::UnusedAssetsDialog => {
                self.unused_assets_dialog.open = false;
                self.panel_states.unused_assets_dialog = false;
            }
            Panel::NotePanel => {
                if let Some(mut note_panel) = self.note_panels.pop() {
                    note_panel.request_close(self);
                    if note_panel.open {
                        self.note_panels.push(note_panel);
                        self.panel_stack.push(Panel::NotePanel);
                    }
                }
                self.panel_states.note_panel = !self.note_panels.is_empty();
                if self.panel_states.note_panel && !self.panel_stack.contains(&Panel::NotePanel) {
                    self.panel_stack.push(Panel::NotePanel);
                }
            }
            Panel::ImagePanel => {
                let _ = self.image_panels.pop();
                self.panel_states.image_panel = false;
            }
            Panel::ScreenshotEditor => {
                let _ = self.screenshot_editors.pop();
                self.panel_states.screenshot_editor = false;
            }
            Panel::CropDialog => {
                self.crop_dialog.cancel();
                self.panel_states.crop_dialog = false;
            }
            Panel::TodoDialog => {
                self.todo_dialog.open = false;
                self.panel_states.todo_dialog = false;
            }
            Panel::TodoViewDialog => {
                self.todo_view_dialog.open = false;
                self.panel_states.todo_view_dialog = false;
            }
            Panel::ClipboardDialog => {
                self.clipboard_dialog.open = false;
                self.panel_states.clipboard_dialog = false;
            }
            Panel::ClipboardModifyDialog => {
                self.clipboard_modify_dialog.open = false;
                self.clipboard_modify_dialog.cleanup_after_close();
                self.panel_states.clipboard_modify_dialog = false;
            }
            Panel::QrDialog => {
                self.qr_dialog.close();
                self.panel_states.qr_dialog = false;
            }
            Panel::RegexTesterDialog => {
                self.regex_tester_dialog.open = false;
                self.panel_states.regex_tester_dialog = false;
            }
            Panel::XmlToolkitDialog => {
                self.xml_toolkit_dialog.end_session();
                self.panel_states.xml_toolkit_dialog = false;
            }
            Panel::TableUtilityDialog => {
                self.table_utility_dialog.close();
                self.panel_states.table_utility_dialog = false;
            }
            Panel::ImageCompareDialog => {
                self.image_compare_dialog.close();
                self.panel_states.image_compare_dialog = false;
            }
            Panel::JsonUtilityDialog => {
                self.json_utility_dialog.close();
                self.panel_states.json_utility_dialog = false;
            }
            Panel::ConvertPanel => {
                self.convert_panel.open = false;
                self.panel_states.convert_panel = false;
            }
            Panel::VolumeDialog => {
                self.volume_dialog.open = false;
                self.panel_states.volume_dialog = false;
            }
            Panel::BrightnessDialog => {
                self.retire_pending_brightness_navigation();
                self.brightness_dialog.close();
                self.panel_states.brightness_dialog = false;
            }
            Panel::CpuListDialog => {
                self.cpu_list_dialog.open = false;
                self.panel_states.cpu_list_dialog = false;
            }
            Panel::DataRecoveryDialog => {
                self.data_recovery_dialog.open = false;
                self.panel_states.data_recovery_dialog = false;
            }
            Panel::ToastLogDialog => {
                self.toast_log_dialog.open = false;
                self.panel_states.toast_log_dialog = false;
            }
            Panel::CalendarPopover => {
                self.calendar_popover_open = false;
                self.panel_states.calendar_popover = false;
            }
            Panel::CalendarEventEditor => {
                self.calendar_editor_open = false;
                self.panel_states.calendar_event_editor = false;
            }
            Panel::CalendarEventDetails => {
                self.calendar_details_open = false;
                self.panel_states.calendar_event_details = false;
            }
            Panel::Editor => {
                self.editor.retire_repair_dialog();
                self.show_editor = false;
                self.panel_states.editor = false;
            }
            Panel::Settings => {
                self.show_settings = false;
                self.panel_states.settings = false;
            }
            Panel::Plugins => {
                self.show_plugins = false;
                self.panel_states.plugins = false;
            }
            Panel::MultiManagerDialog => {
                self.multi_manager_dialog.open = false;
                self.panel_states.multi_manager_dialog = false;
            }
            Panel::MultiManagerSettingsDialog => {
                self.multi_manager_settings_dialog.open = false;
                self.panel_states.multi_manager_settings_dialog = false;
            }
            Panel::RadialEditor => {
                if let Ok(mut editor) = self.radial_editor.lock() {
                    editor.request_close();
                }
                self.panel_states.radial_editor = self
                    .radial_editor
                    .lock()
                    .map(|editor| editor.open)
                    .unwrap_or(false);
                if self.panel_states.radial_editor {
                    self.panel_stack.push(Panel::RadialEditor);
                }
            }
        }
        true
    }

    pub(crate) fn close_paste_sequence_dialog(&mut self) {
        self.paste_sequence_dialog.end_session();
        self.panel_states.paste_sequence_dialog = false;
        self.panel_stack
            .retain(|panel| *panel != Panel::PasteSequenceDialog);
    }

    fn force_close_panel(&mut self, panel: Panel) {
        self.retire_settings_navigation_for_panel(panel);
        match panel {
            Panel::AliasDialog => {
                self.alias_dialog.open = false;
                self.panel_states.alias_dialog = false;
            }
            Panel::BookmarkAliasDialog => {
                self.bookmark_alias_dialog.open = false;
                self.panel_states.bookmark_alias_dialog = false;
            }
            Panel::TempfileAliasDialog => {
                self.tempfile_alias_dialog.open = false;
                self.panel_states.tempfile_alias_dialog = false;
            }
            Panel::TempfileDialog => {
                self.tempfile_dialog.open = false;
                self.panel_states.tempfile_dialog = false;
            }
            Panel::AddBookmarkDialog => {
                self.add_bookmark_dialog.open = false;
                self.panel_states.add_bookmark_dialog = false;
            }
            Panel::HelpOverlay => {
                self.help_window.overlay_open = false;
                self.panel_states.help_overlay = false;
            }
            Panel::HelpWindow => {
                self.help_window.open = false;
                self.panel_states.help_window = false;
            }
            Panel::TimerDialog => {
                self.timer_dialog.open = false;
                self.panel_states.timer_dialog = false;
            }
            Panel::CompletionDialog => {
                self.completion_dialog.open = false;
                self.panel_states.completion_dialog = false;
            }
            Panel::ShellCmdDialog => {
                self.shell_cmd_dialog.open = false;
                self.panel_states.shell_cmd_dialog = false;
            }
            Panel::SnippetDialog => {
                self.snippet_dialog.end_session();
                self.panel_states.snippet_dialog = false;
            }
            Panel::PasteSequenceDialog => {
                self.paste_sequence_dialog.end_session();
                self.panel_states.paste_sequence_dialog = false;
            }
            Panel::HashGeneratorDialog => {
                self.hash_generator_dialog.end_session();
                self.panel_states.hash_generator_dialog = false;
            }
            Panel::SnippetPromptDialog => {
                let _ = self.cancel_snippet_prompt();
            }
            Panel::MacroDialog => {
                self.macro_dialog.open = false;
                self.panel_states.macro_dialog = false;
            }
            Panel::MkMacroDialog => {
                self.mkmacro_dialog.action_editor.cancel();
                self.mkmacro_dialog.open = false;
                self.panel_states.mkmacro_dialog = false;
            }
            Panel::MouseGesturesDialog => {
                self.mouse_gestures_dialog.open = false;
                self.panel_states.mouse_gestures_dialog = false;
            }
            Panel::MouseGestureSettingsDialog => {
                self.mouse_gesture_settings_dialog.open = false;
                self.panel_states.mouse_gesture_settings_dialog = false;
            }
            Panel::MouseSettingsDialog => {
                self.mouse_settings_dialog.open = false;
                self.panel_states.mouse_settings_dialog = false;
            }
            Panel::HotkeyDirectoryDialog => {
                self.hotkey_directory_dialog.open = false;
                self.hotkey_directory_dialog.clear_message();
                self.panel_states.hotkey_directory_dialog = false;
            }
            Panel::ThemeSettingsDialog => {
                self.close_theme_settings_dialog();
            }
            Panel::FavDialog => {
                self.fav_dialog.open = false;
                self.panel_states.fav_dialog = false;
            }
            Panel::FileSearchDialog => {
                self.file_search_dialog.open = false;
                self.panel_states.file_search_dialog = false;
            }
            Panel::BatchRenameDialog => {
                self.end_batch_rename_session();
                self.panel_states.batch_rename_dialog = false;
            }
            Panel::SearchReplaceDialog => {
                self.end_search_replace_session();
                self.panel_states.search_replace_dialog = false;
            }
            Panel::NotesDialog => {
                self.notes_dialog.open = false;
                self.panel_states.notes_dialog = false;
            }
            Panel::NoteGraphDialog => {
                self.note_graph_dialog.open = false;
                self.panel_states.note_graph_dialog = false;
            }
            Panel::UnusedAssetsDialog => {
                self.unused_assets_dialog.open = false;
                self.panel_states.unused_assets_dialog = false;
            }
            Panel::NotePanel => {
                if let Some(mut note_panel) = self.note_panels.pop() {
                    note_panel.request_close(self);
                    if note_panel.open {
                        self.note_panels.push(note_panel);
                        self.panel_stack.push(Panel::NotePanel);
                    }
                }
                self.panel_states.note_panel = !self.note_panels.is_empty();
                if self.panel_states.note_panel && !self.panel_stack.contains(&Panel::NotePanel) {
                    self.panel_stack.push(Panel::NotePanel);
                }
            }
            Panel::ImagePanel => {
                let _ = self.image_panels.pop();
                self.panel_states.image_panel = false;
            }
            Panel::ScreenshotEditor => {
                let _ = self.screenshot_editors.pop();
                self.panel_states.screenshot_editor = false;
            }
            Panel::CropDialog => {
                self.crop_dialog.cancel();
                self.panel_states.crop_dialog = false;
            }
            Panel::TodoDialog => {
                self.todo_dialog.open = false;
                self.panel_states.todo_dialog = false;
            }
            Panel::TodoViewDialog => {
                self.todo_view_dialog.open = false;
                self.panel_states.todo_view_dialog = false;
            }
            Panel::ClipboardDialog => {
                self.clipboard_dialog.open = false;
                self.panel_states.clipboard_dialog = false;
            }
            Panel::ClipboardModifyDialog => {
                self.clipboard_modify_dialog.open = false;
                self.clipboard_modify_dialog.cleanup_after_close();
                self.panel_states.clipboard_modify_dialog = false;
            }
            Panel::QrDialog => {
                self.qr_dialog.close();
                self.panel_states.qr_dialog = false;
            }
            Panel::RegexTesterDialog => {
                self.regex_tester_dialog.open = false;
                self.panel_states.regex_tester_dialog = false;
            }
            Panel::XmlToolkitDialog => {
                self.xml_toolkit_dialog.end_session();
                self.panel_states.xml_toolkit_dialog = false;
            }
            Panel::TableUtilityDialog => {
                self.table_utility_dialog.close();
                self.panel_states.table_utility_dialog = false;
            }
            Panel::ImageCompareDialog => {
                self.image_compare_dialog.close();
                self.panel_states.image_compare_dialog = false;
            }
            Panel::JsonUtilityDialog => {
                self.json_utility_dialog.close();
                self.panel_states.json_utility_dialog = false;
            }
            Panel::ConvertPanel => {
                self.convert_panel.open = false;
                self.panel_states.convert_panel = false;
            }
            Panel::VolumeDialog => {
                self.volume_dialog.open = false;
                self.panel_states.volume_dialog = false;
            }
            Panel::BrightnessDialog => {
                self.retire_pending_brightness_navigation();
                self.brightness_dialog.close();
                self.panel_states.brightness_dialog = false;
            }
            Panel::CpuListDialog => {
                self.cpu_list_dialog.open = false;
                self.panel_states.cpu_list_dialog = false;
            }
            Panel::DataRecoveryDialog => {
                self.data_recovery_dialog.open = false;
                self.panel_states.data_recovery_dialog = false;
            }
            Panel::ToastLogDialog => {
                self.toast_log_dialog.open = false;
                self.panel_states.toast_log_dialog = false;
            }
            Panel::CalendarPopover => {
                self.calendar_popover_open = false;
                self.panel_states.calendar_popover = false;
            }
            Panel::CalendarEventEditor => {
                self.calendar_editor_open = false;
                self.panel_states.calendar_event_editor = false;
            }
            Panel::CalendarEventDetails => {
                self.calendar_details_open = false;
                self.panel_states.calendar_event_details = false;
            }
            Panel::Editor => {
                self.editor.retire_repair_dialog();
                self.show_editor = false;
                self.panel_states.editor = false;
            }
            Panel::Settings => {
                self.show_settings = false;
                self.panel_states.settings = false;
            }
            Panel::Plugins => {
                self.show_plugins = false;
                self.panel_states.plugins = false;
            }
            Panel::MultiManagerDialog => {
                self.multi_manager_dialog.open = false;
                self.panel_states.multi_manager_dialog = false;
            }
            Panel::MultiManagerSettingsDialog => {
                self.multi_manager_settings_dialog.open = false;
                self.panel_states.multi_manager_settings_dialog = false;
            }
            Panel::RadialEditor => {
                if let Ok(mut editor) = self.radial_editor.lock() {
                    editor.force_close();
                }
                self.panel_states.radial_editor = false;
            }
        }
        self.panel_stack.retain(|p| *p != panel);
    }

    fn ensure_open(&mut self, panel: Panel) {
        match panel {
            Panel::AliasDialog => self.alias_dialog.open = true,
            Panel::BookmarkAliasDialog => self.bookmark_alias_dialog.open = true,
            Panel::TempfileAliasDialog => self.tempfile_alias_dialog.open = true,
            Panel::TempfileDialog => self.tempfile_dialog.open = true,
            Panel::AddBookmarkDialog => self.add_bookmark_dialog.open = true,
            Panel::HelpOverlay => self.help_window.overlay_open = true,
            Panel::HelpWindow => self.help_window.open = true,
            Panel::TimerDialog => self.timer_dialog.open = true,
            Panel::CompletionDialog => self.completion_dialog.open = true,
            Panel::ShellCmdDialog => self.shell_cmd_dialog.ensure_open(),
            Panel::SnippetDialog => self.snippet_dialog.ensure_open(),
            Panel::PasteSequenceDialog => {
                let context = self.paste_sequence_invocation_context_for_open();
                self.paste_sequence_dialog.open_with_context(context);
            }
            Panel::HashGeneratorDialog => self.hash_generator_dialog.ensure_open(),
            Panel::SnippetPromptDialog => self.snippet_prompt_dialog.ensure_open(),
            Panel::MacroDialog => self.macro_dialog.ensure_open(),
            Panel::MkMacroDialog => self.mkmacro_dialog.open(),
            Panel::MouseGesturesDialog => self.mouse_gestures_dialog.open = true,
            Panel::MouseGestureSettingsDialog => self.mouse_gesture_settings_dialog.open(),
            Panel::MouseSettingsDialog => self.open_mouse_settings_dialog(),
            Panel::HotkeyDirectoryDialog => self.hotkey_directory_dialog.open = true,
            Panel::ThemeSettingsDialog => self.open_theme_settings_dialog(),
            Panel::FavDialog => self.fav_dialog.ensure_open(),
            Panel::FileSearchDialog => self.file_search_dialog.open(),
            Panel::BatchRenameDialog => self.batch_rename_dialog.open(),
            Panel::SearchReplaceDialog => {
                if let Err(error) = self.ensure_search_replace_runtime() {
                    self.search_replace_dialog.set_status_message(error);
                }
                self.search_replace_dialog.open();
            }
            Panel::NotesDialog => self.notes_dialog.open = true,
            Panel::NoteGraphDialog => self.note_graph_dialog.open = true,
            Panel::UnusedAssetsDialog => self.unused_assets_dialog.ensure_open(),
            Panel::NotePanel => {}
            Panel::ImagePanel => {}
            Panel::ScreenshotEditor => {}
            Panel::CropDialog => {}
            Panel::TodoDialog => self.todo_dialog.ensure_open(),
            Panel::TodoViewDialog => self.todo_view_dialog.ensure_open(),
            Panel::ClipboardDialog => self.clipboard_dialog.ensure_open(),
            Panel::ClipboardModifyDialog => self
                .clipboard_modify_dialog
                .open_section(ClipboardModifyDialogSection::Modify, &clipboard_service()),
            Panel::JsonUtilityDialog => self.json_utility_dialog.reopen_existing(),
            Panel::QrDialog => self.qr_dialog.open = true,
            Panel::RegexTesterDialog => {
                if !self.regex_tester_dialog.open {
                    self.regex_tester_dialog.open();
                }
            }
            Panel::XmlToolkitDialog => self.xml_toolkit_dialog.open(),
            Panel::TableUtilityDialog => self.table_utility_dialog.open(),
            Panel::ImageCompareDialog => self.image_compare_dialog.open(),
            Panel::ConvertPanel => self.convert_panel.open = true,
            Panel::VolumeDialog => self.volume_dialog.ensure_open(),
            Panel::BrightnessDialog => {
                self.brightness_dialog.open();
                self.egui_ctx.request_repaint_of(egui::ViewportId::ROOT);
            }
            Panel::CpuListDialog => self.cpu_list_dialog.open = true,
            Panel::DataRecoveryDialog => {
                let _ = self
                    .data_recovery_dialog
                    .open(crate::commands::DataDialogFocus::Overview);
            }
            Panel::ToastLogDialog => self.toast_log_dialog.open = true,
            Panel::CalendarPopover => self.calendar_popover_open = true,
            Panel::CalendarEventEditor => self.calendar_editor_open = true,
            Panel::CalendarEventDetails => self.calendar_details_open = true,
            Panel::Editor => self.show_editor = true,
            Panel::Settings => self.open_settings_dialog(),
            Panel::Plugins => self.show_plugins = true,
            Panel::MultiManagerDialog => self.multi_manager_dialog.open = true,
            Panel::MultiManagerSettingsDialog => self.multi_manager_settings_dialog.open = true,
            Panel::RadialEditor => {
                if let Ok(mut editor) = self.radial_editor.lock() {
                    editor.open();
                }
            }
        }
        if !self.panel_stack.contains(&panel) {
            self.panel_stack.push(panel);
        }
    }

    pub(crate) fn focus_panel(&mut self, panel: Panel) {
        self.panel_stack.retain(|p| *p != panel);
        self.panel_stack.push(panel);
        self.ensure_open(panel);
    }

    fn enforce_pinned(&mut self) {
        let pinned = self.pinned_panels.clone();
        for panel in pinned {
            if panel == Panel::RadialEditor {
                if let Ok(mut editor) = self.radial_editor.lock() {
                    editor.ensure_open();
                }
                if !self.panel_stack.contains(&panel) {
                    self.panel_stack.push(panel);
                }
            } else {
                self.ensure_open(panel);
            }
        }
    }

    fn toggle_pin(&mut self, panel: Panel) {
        if self.pinned_panels.contains(&panel) {
            self.pinned_panels.retain(|p| *p != panel);
            if panel == Panel::RadialEditor {
                // Unpinning is a user close request, not authority to discard
                // an authoring draft. Dirty state must pass through the
                // editor's explicit save/discard/keep-editing prompt.
                if let Ok(mut editor) = self.radial_editor.lock() {
                    editor.request_close();
                }
                self.panel_states.radial_editor = self
                    .radial_editor
                    .lock()
                    .map(|editor| editor.open)
                    .unwrap_or(false);
                if self.panel_states.radial_editor && !self.panel_stack.contains(&panel) {
                    self.panel_stack.push(panel);
                }
            } else {
                self.force_close_panel(panel);
            }
        } else {
            self.pinned_panels.push(panel);
            self.focus_panel(panel);
        }
        self.save_pinned_panels();
    }

    fn save_pinned_panels(&mut self) {
        let pinned_panels = self.pinned_panels.clone();
        if let Err(e) = Settings::update(&self.settings_path, |settings| {
            settings.pinned_panels = pinned_panels;
            Ok(())
        }) {
            self.report_error_message("launcher", format!("Failed to save: {e}"));
        }
    }

    fn update_panel_stack(&mut self) {
        macro_rules! check {
            ($field:ident, $kind:expr) => {{
                let is_open = self.is_panel_open($kind);
                if is_open && !self.panel_states.$field {
                    self.panel_stack.retain(|p| *p != $kind);
                    self.panel_stack.push($kind);
                    self.panel_states.$field = true;
                } else if !is_open && self.panel_states.$field {
                    self.panel_stack.retain(|p| *p != $kind);
                    self.panel_states.$field = false;
                }
            }};
        }

        check!(alias_dialog, Panel::AliasDialog);
        check!(bookmark_alias_dialog, Panel::BookmarkAliasDialog);
        check!(tempfile_alias_dialog, Panel::TempfileAliasDialog);
        check!(tempfile_dialog, Panel::TempfileDialog);
        check!(add_bookmark_dialog, Panel::AddBookmarkDialog);
        check!(help_overlay, Panel::HelpOverlay);
        check!(help_window, Panel::HelpWindow);
        check!(timer_dialog, Panel::TimerDialog);
        check!(completion_dialog, Panel::CompletionDialog);
        check!(shell_cmd_dialog, Panel::ShellCmdDialog);
        check!(snippet_dialog, Panel::SnippetDialog);
        check!(paste_sequence_dialog, Panel::PasteSequenceDialog);
        check!(hash_generator_dialog, Panel::HashGeneratorDialog);
        check!(snippet_prompt_dialog, Panel::SnippetPromptDialog);
        check!(macro_dialog, Panel::MacroDialog);
        check!(mkmacro_dialog, Panel::MkMacroDialog);
        check!(mouse_gestures_dialog, Panel::MouseGesturesDialog);
        check!(
            mouse_gesture_settings_dialog,
            Panel::MouseGestureSettingsDialog
        );
        check!(mouse_settings_dialog, Panel::MouseSettingsDialog);
        check!(theme_settings_dialog, Panel::ThemeSettingsDialog);
        check!(hotkey_directory_dialog, Panel::HotkeyDirectoryDialog);
        check!(fav_dialog, Panel::FavDialog);
        check!(file_search_dialog, Panel::FileSearchDialog);
        check!(notes_dialog, Panel::NotesDialog);
        check!(note_graph_dialog, Panel::NoteGraphDialog);
        check!(unused_assets_dialog, Panel::UnusedAssetsDialog);
        check!(note_panel, Panel::NotePanel);
        check!(image_panel, Panel::ImagePanel);
        check!(screenshot_editor, Panel::ScreenshotEditor);
        check!(crop_dialog, Panel::CropDialog);
        check!(todo_dialog, Panel::TodoDialog);
        check!(todo_view_dialog, Panel::TodoViewDialog);
        check!(clipboard_dialog, Panel::ClipboardDialog);
        check!(clipboard_modify_dialog, Panel::ClipboardModifyDialog);
        check!(json_utility_dialog, Panel::JsonUtilityDialog);
        check!(regex_tester_dialog, Panel::RegexTesterDialog);
        check!(batch_rename_dialog, Panel::BatchRenameDialog);
        check!(search_replace_dialog, Panel::SearchReplaceDialog);
        check!(xml_toolkit_dialog, Panel::XmlToolkitDialog);
        check!(table_utility_dialog, Panel::TableUtilityDialog);
        check!(image_compare_dialog, Panel::ImageCompareDialog);
        check!(qr_dialog, Panel::QrDialog);
        check!(convert_panel, Panel::ConvertPanel);
        check!(volume_dialog, Panel::VolumeDialog);
        check!(brightness_dialog, Panel::BrightnessDialog);
        check!(cpu_list_dialog, Panel::CpuListDialog);
        check!(data_recovery_dialog, Panel::DataRecoveryDialog);
        check!(toast_log_dialog, Panel::ToastLogDialog);
        check!(calendar_popover, Panel::CalendarPopover);
        check!(calendar_event_editor, Panel::CalendarEventEditor);
        check!(calendar_event_details, Panel::CalendarEventDetails);
        check!(editor, Panel::Editor);
        check!(settings, Panel::Settings);
        check!(plugins, Panel::Plugins);
        check!(multi_manager_dialog, Panel::MultiManagerDialog);
        check!(
            multi_manager_settings_dialog,
            Panel::MultiManagerSettingsDialog
        );
        check!(radial_editor, Panel::RadialEditor);
    }
}

impl LauncherApp {
    /// Publish a query for the next root frame, invalidating any one-shot
    /// command-to-pending association before the independent request is stored.
    pub(super) fn publish_pending_query(&mut self, query: String) {
        self.track_c_query_observer.pending_query_published();
        self.pending_query = Some(query);
    }

    pub fn install_root_window_bridge(&mut self, bridge: RootWindowBridge) {
        self.root_window_bridge = bridge;
    }

    pub fn install_screen_draw_recovery_bridge(
        &mut self,
        bridge: Arc<crate::screen_draw::ScreenDrawRecoveryBridge>,
    ) {
        self.screen_draw_controller
            .set_recovery_bridge(Arc::clone(&bridge));
        self.screen_draw_recovery_bridge = bridge;
    }

    /// Returns the app-owned observed event receiver. `try_recv` and
    /// `recv_timeout` expose plain `WatchEvent` values while internal queue
    /// age/depth diagnostics remain behind the private envelope.
    pub fn watch_receiver(&self) -> &EventReceiver {
        &self.rx
    }

    /// Open a note panel for the given slug, optionally using a template for new notes.
    pub fn open_note_panel(&mut self, slug: &str, template: Option<&str>) {
        use crate::plugins::note::{
            Note, expand_template_variables, extract_aliases, get_template, load_notes,
        };
        let note = load_notes()
            .unwrap_or_default()
            .into_iter()
            .find(|n| {
                n.slug == slug
                    || n.alias
                        .as_ref()
                        .map(|a| a.eq_ignore_ascii_case(slug))
                        .unwrap_or(false)
            })
            .unwrap_or_else(|| {
                let title = slug.replace('-', " ");
                let content = if let Some(tpl_name) = template {
                    if let Some(tpl) = get_template(tpl_name) {
                        let filled =
                            expand_template_variables(&tpl, &title, slug, chrono::Local::now());
                        if filled.starts_with("# ") {
                            filled
                        } else {
                            format!("# {}\n\n{}", title, filled)
                        }
                    } else {
                        format!("# {}\n\n", title)
                    }
                } else {
                    format!("# {}\n\n", title)
                };
                let aliases = extract_aliases(&content);
                let alias = aliases.first().cloned();
                Note {
                    title,
                    path: std::path::PathBuf::new(),
                    content,
                    tags: Vec::new(),
                    links: Vec::new(),
                    slug: String::new(),
                    alias,
                    aliases,
                    entity_refs: Vec::new(),
                }
            });
        // A note activation takes precedence over a queued launcher query-focus
        // request. The note consumes a one-shot layer handoff after its own
        // window has rendered, so later deliberate focus changes remain intact.
        self.focus_query = false;
        self.pending_show_query_focus = None;
        self.query_focus_session = None;
        if let Some(existing_idx) = self
            .note_panels
            .iter()
            .position(|panel| panel.note_slug() == note.slug)
        {
            let mut panel = self.note_panels.remove(existing_idx);
            panel.request_activation_handoff();
            self.note_panels.push(panel);
            self.update_panel_stack();
            return;
        }

        let word_count = note.content.split_whitespace().count();
        if self.enable_toasts {
            push_toast(
                &mut self.toasts,
                Toast {
                    text: format!(
                        "Opened note ({} words) – press Esc or Cmd+W to close",
                        word_count
                    )
                    .into(),
                    kind: ToastKind::Info,
                    options: ToastOptions::default()
                        .duration_in_seconds(self.toast_duration as f64),

                    style: Default::default(),
                },
            );
        }
        let mut panel = NotePanel::from_note_with_details_and_settings(
            note,
            self.note_show_details,
            &self.note_settings,
        );
        panel.request_activation_handoff();
        panel.load_collapsed_sections_state(self);
        self.note_panels.push(panel);
        // Allow keyboard shortcuts like Esc/Cmd+W to immediately close the panel
        self.update_panel_stack();
    }

    pub fn push_note_panel(&mut self, panel: NotePanel) {
        self.note_panels.push(panel);
        self.update_panel_stack();
    }

    /// Open an image viewer panel for the given file path.
    pub fn open_image_panel(&mut self, path: &Path) {
        if !path.exists() {
            self.report_error_message("launcher", format!("Image not found: {}", path.display()));
            return;
        }
        if image::ImageFormat::from_path(path).is_err() {
            self.report_error_message(
                "launcher",
                format!("Unsupported image format: {}", path.display()),
            );
            return;
        }
        self.image_panels.push(ImagePanel::new(path.to_path_buf()));
        self.update_panel_stack();
    }

    /// Open the screenshot editor for a captured image.
    pub fn open_screenshot_editor(&mut self, img: image::RgbaImage, clip: bool, tool: MarkupTool) {
        use chrono::Local;
        let dir = crate::plugins::screenshot::screenshot_dir();
        let _ = std::fs::create_dir_all(&dir);
        let filename = format!(
            "multi_launcher_{}.png",
            Local::now().format("%Y%m%d_%H%M%S")
        );
        let path = dir.join(filename);
        self.screenshot_editors.push(ScreenshotEditor::new(
            img,
            path,
            clip,
            self.screenshot_auto_save,
            tool,
        ));
        self.update_panel_stack();
    }

    /// Update query to show available note tags.
    pub fn open_note_tags(&mut self) {
        self.query = "note tags".into();
        self.search();
        self.focus_input();
        if self.enable_toasts {
            push_toast(
                &mut self.toasts,
                Toast {
                    text: "Showing note tags – press Esc to exit".into(),
                    kind: ToastKind::Info,
                    options: ToastOptions::default()
                        .duration_in_seconds(self.toast_duration as f64),

                    style: Default::default(),
                },
            );
        }
    }

    pub fn open_calendar_popover(&mut self, date: Option<NaiveDate>) {
        let today = chrono::Local::now().naive_local().date();
        self.calendar_selected_date = Some(date.unwrap_or(today));
        self.calendar_selected_event = None;
        self.calendar_popover_open = true;
    }

    pub fn open_calendar_editor(
        &mut self,
        event: Option<crate::plugins::calendar::CalendarEvent>,
        split_scope: Option<(
            crate::gui::calendar_event_details::RecurrenceScope,
            chrono::NaiveDateTime,
        )>,
    ) {
        if let Some(event) = event {
            self.calendar_event_editor.open(Some(event), split_scope);
        } else {
            let date = self
                .calendar_selected_date
                .unwrap_or_else(|| chrono::Local::now().naive_local().date());
            self.calendar_event_editor.open_new(date);
        }
        self.calendar_editor_open = true;
    }

    /// Filter the note list to only show notes containing the given tag.
    pub fn filter_notes_by_tag(&mut self, tag: &str) {
        self.query = format!("note list #{tag}");
        self.search();
        self.focus_input();
    }

    /// Open a link collected from notes in the system browser.
    pub fn open_note_link(&mut self, link: &str) {
        let url = if link.starts_with("www.") {
            format!("https://{link}")
        } else {
            link.to_string()
        };
        if link.starts_with("www.") || link.contains("://") {
            match Url::parse(&url) {
                Ok(url) if url.scheme() == "https" => match open_link(url.as_str()) {
                    Ok(_) => {
                        if self.enable_toasts {
                            push_toast(
                                &mut self.toasts,
                                Toast {
                                    text: "Opened note link".into(),
                                    kind: ToastKind::Info,
                                    options: ToastOptions::default()
                                        .duration_in_seconds(self.toast_duration as f64),

                                    style: Default::default(),
                                },
                            );
                        }
                    }
                    Err(e) => {
                        self.report_error_message("launcher", format!("Failed to open link: {e}"))
                    }
                },
                _ => {
                    self.add_error_toast(format!("Invalid link: {link}"));
                }
            }
        } else {
            self.open_note_panel(link, None);
        }
    }

    /// Resolve a note by slug and open it in Neovim using injected callbacks.
    ///
    /// `load` should mirror [`load_notes`](crate::plugins::note::load_notes) and
    /// is invoked once to fetch the available notes. `spawn` is called with the
    /// resolved path and should launch `nvim`. Both closures are `FnOnce` to
    /// support unit testing of this helper.
    ///
    /// Any error from either step is stored in [`self.error`](Self::error); the
    /// boolean return value is always `true` and currently unused.
    pub fn open_note_in_neovim<L, S>(&mut self, slug: &str, load: L, spawn: S) -> bool
    where
        L: FnOnce() -> anyhow::Result<Vec<crate::plugins::note::Note>>,
        S: FnOnce(&std::path::Path) -> std::io::Result<()>,
    {
        match load() {
            Ok(notes) => {
                if let Some(note) = notes.iter().find(|n| n.slug == slug) {
                    if let Err(e) = spawn(&note.path) {
                        self.report_error_message("launcher", e.to_string());
                    }
                } else {
                    self.report_error_message("launcher", "Note not found".to_string());
                }
            }
            Err(e) => {
                self.report_error_message("launcher", e.to_string());
            }
        }
        true
    }

    /// Delete a note by its slug identifier.
    pub fn delete_note(&mut self, slug: &str) {
        use crate::plugins::note::remove_note_by_identity;
        match remove_note_by_identity(slug) {
            Ok(removed) => {
                if let Some(note) = removed {
                    let word_count = note.content.split_whitespace().count();
                    let msg = format!(
                        "Removed note {} ({} words)",
                        note.alias.as_ref().unwrap_or(&note.title),
                        word_count
                    );
                    append_toast_log(&msg);
                    if self.enable_toasts {
                        push_toast(
                            &mut self.toasts,
                            Toast {
                                text: msg.clone().into(),
                                kind: ToastKind::Success,
                                options: ToastOptions::default()
                                    .duration_in_seconds(self.toast_duration as f64),

                                style: Default::default(),
                            },
                        );
                    }
                    if self.query.trim_start().starts_with("note list") {
                        self.publish_pending_query(self.query.clone());
                        self.search();
                    }
                    self.notes_dialog.open();
                } else {
                    self.report_error_message("launcher", "Note not found");
                }
            }
            Err(e) => self.report_error_message("launcher", format!("Failed to remove note: {e}")),
        }
        self.focus_input();
    }

    /// Process dropped files or directories.
    pub fn handle_dropped_files(&mut self, files: Vec<egui::DroppedFileHandle>) {
        for file in files {
            let path = file.path().to_path_buf();
            if path.is_dir() {
                if let Err(e) = folders::add(path.to_str().unwrap_or_default()) {
                    self.report_error_message("launcher", format!("Failed to add folder: {e}"));
                }
                if let Some(p) = path.to_str() {
                    self.alias_dialog.open(p);
                }
            } else if let Some(p) = path.to_str() {
                self.show_editor = true;
                self.editor.open_add_with_path(p);
            }
        }
    }
}

pub fn recv_test_event(rx: &EventReceiver) -> Option<TestWatchEvent> {
    while let Ok(ev) = rx.try_recv() {
        match ev {
            WatchEvent::Actions
            | WatchEvent::IndexReady
            | WatchEvent::Folders
            | WatchEvent::Bookmarks => {
                return Some(ev.into());
            }
            WatchEvent::Dashboard(_)
            | WatchEvent::Clipboard
            | WatchEvent::Snippets
            | WatchEvent::Notes
            | WatchEvent::Todos
            | WatchEvent::Favorites
            | WatchEvent::Gestures
            | WatchEvent::ExecuteAction(_)
            | WatchEvent::RadialDispatch(_)
            | WatchEvent::RadialPrepare(_)
            | WatchEvent::RadialResolveDeferred(_)
            | WatchEvent::RadialDeferredSearchReady { .. }
            | WatchEvent::RadialAuthoringSearchReady { .. }
            | WatchEvent::RadialAuthoringSearchFailed { .. }
            | WatchEvent::AuthoringProviderCapacityAvailable
            | WatchEvent::RadialInvalidate
            | WatchEvent::RadialConfigDiagnostic(_)
            | WatchEvent::RadialRuntimeDiagnostic(_)
            | WatchEvent::PasteSequenceRuntimeDiagnostic(_)
            | WatchEvent::PasteSequenceRoutes(_)
            | WatchEvent::HotkeyDirectorySource(_)
            | WatchEvent::RadialDiagnostic(_)
            | WatchEvent::RadialSubmenuPlacementFailure(_)
            | WatchEvent::RadialPlacementActionResult { .. }
            | WatchEvent::RadialMigrationNotice(_)
            | WatchEvent::RadialMigrationState { .. }
            | WatchEvent::ScreenDrawStart
            | WatchEvent::PhysicalPaste(_)
            | WatchEvent::PasteSequenceHotkey(_) => {
                continue;
            }
            WatchEvent::ScreenDrawRecover(_) | WatchEvent::ScreenDrawEmergency(_) => {
                return Some(ev.into());
            }
            WatchEvent::ClipboardModify(_) => return Some(ev.into()),
            WatchEvent::Recycle(_) => return Some(ev.into()),
            WatchEvent::VirtualDesktop(_) => return Some(ev.into()),
        }
    }
    None
}

pub fn recv_test_event_timeout(rx: &EventReceiver, timeout: Duration) -> Option<TestWatchEvent> {
    let deadline = Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let event = rx.recv_timeout(remaining).ok()?;
        match event {
            WatchEvent::Actions
            | WatchEvent::IndexReady
            | WatchEvent::Folders
            | WatchEvent::Bookmarks
            | WatchEvent::ScreenDrawRecover(_)
            | WatchEvent::ScreenDrawEmergency(_) => {
                return Some(event.into());
            }
            _ if Instant::now() < deadline => {}
            _ => return None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        common::slug::reset_slug_lookup,
        dashboard::config::OverflowMode,
        dashboard::layout::NormalizedSlot,
        mkmacro::{
            AlphaPolicy, ImageImportChoice, ImageImportResult, MkAction, MkImageNotFoundPolicy,
            MkImageOutputs, MkImagePayload, MkImageRef, MkWaitOptions, ReturnPoint, SearchRegion,
        },
        plugin::PluginManager,
        plugins::note::{NotePlugin, append_note, load_notes, save_notes},
        settings::Settings,
        toast_log::TOAST_LOG_FILE,
    };
    use eframe::egui;
    use image::RgbaImage;
    use serde_json::json;
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    };
    use std::time::{Duration, Instant};
    use tempfile::tempdir;

    #[test]
    fn panel_display_labels_are_friendly_and_serde_names_stay_stable() {
        for (panel, expected) in [
            (Panel::NotesDialog, "Quick Notes"),
            (Panel::NoteGraphDialog, "Note Graph"),
            (Panel::ClipboardDialog, "Clipboard History"),
            (Panel::ClipboardModifyDialog, "Clipboard Modify"),
            (Panel::MkMacroDialog, "MKMacro"),
            (Panel::DataRecoveryDialog, "Data & Recovery"),
            (Panel::CpuListDialog, "CPU Usage"),
        ] {
            assert_eq!(panel.display_label(), expected);
        }
        assert_eq!(
            serde_json::to_string(&Panel::NotesDialog).unwrap(),
            "\"NotesDialog\""
        );
        assert_eq!(
            serde_json::from_str::<Panel>("\"NotesDialog\"").unwrap(),
            Panel::NotesDialog
        );
    }

    fn reset_event_registry_for_test() {
        let mut registry = super::APP_EVENT_REGISTRY.lock().unwrap();
        registry.sinks.clear();
        registry.pending_before_owner.clear();
        registry.owner_registered = false;
    }

    #[test]
    fn disabling_paste_sequence_cancels_pending_preflight_and_closes_editor() {
        let directory = tempdir().unwrap();
        let ctx = egui::Context::default();
        let mut settings = Settings::default();
        settings.hotkey = None;
        settings.quit_hotkey = None;
        settings.help_hotkey = None;
        settings.paste_sequence.next_hotkey = None;
        settings.paste_sequence.cancel_hotkey = None;
        settings.dashboard.enabled = false;
        let mut app = LauncherApp::new(
            &ctx,
            Arc::new(Vec::new()),
            0,
            PluginManager::new_inert_for_test(),
            directory
                .path()
                .join("actions.json")
                .to_string_lossy()
                .into_owned(),
            directory
                .path()
                .join("settings.json")
                .to_string_lossy()
                .into_owned(),
            settings,
            None,
            None,
            None,
            None,
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        );
        app.sequence_playback.begin_preparing().unwrap();
        app.paste_sequence_dialog.open = true;

        app.apply_enabled_plugin_selection(Some(HashSet::new()));

        assert_eq!(
            app.sequence_playback.status(),
            crate::paste_sequence::SequencePlaybackStatus::Cancelled
        );
        assert!(!app.paste_sequence_dialog.open);
    }

    fn new_app(ctx: &egui::Context) -> LauncherApp {
        let mut settings = Settings::default();
        settings.paste_sequence.next_hotkey = None;
        settings.paste_sequence.cancel_hotkey = None;
        LauncherApp::new(
            ctx,
            Arc::new(Vec::new()),
            0,
            PluginManager::new(),
            "actions.json".into(),
            "settings.json".into(),
            settings,
            None,
            None,
            None,
            None,
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        )
    }

    fn new_isolated_index_app(
        ctx: &egui::Context,
        config: &crate::indexer::coordinator::IndexConfig,
    ) -> (
        crate::performance::workloads::IsolatedWorkspace,
        LauncherApp,
    ) {
        let workspace = crate::performance::workloads::IsolatedWorkspace::new();
        let app = new_index_app_in_workspace(ctx, &workspace, config, "index-owner");
        (workspace, app)
    }

    fn new_index_app_in_workspace(
        ctx: &egui::Context,
        workspace: &crate::performance::workloads::IsolatedWorkspace,
        config: &crate::indexer::coordinator::IndexConfig,
        name: &str,
    ) -> LauncherApp {
        let mut settings = Settings::default();
        settings.index_paths = Some(config.roots().to_vec());
        settings.max_indexed_items = config.max_items();
        settings.hotkey = None;
        settings.quit_hotkey = None;
        settings.help_hotkey = None;
        settings.paste_sequence.next_hotkey = None;
        settings.paste_sequence.cancel_hotkey = None;
        settings.enabled_plugins = Some(std::collections::HashSet::new());
        settings.dashboard.enabled = false;
        settings.enable_toasts = false;
        settings.show_inline_errors = false;
        settings.show_error_toasts = false;
        let actions_path = workspace.root().join(format!("{name}-actions.json"));
        let settings_path = workspace.root().join(format!("{name}-settings.json"));
        LauncherApp::new(
            ctx,
            Arc::new(Vec::new()),
            0,
            PluginManager::new_inert_for_test(),
            actions_path.to_string_lossy().into_owned(),
            settings_path.to_string_lossy().into_owned(),
            settings,
            None,
            Some(config.roots().to_vec()),
            Some(std::collections::HashSet::new()),
            Some(HashMap::new()),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        )
    }

    fn index_test_action(name: &str) -> Action {
        Action {
            label: name.into(),
            desc: format!("{name} description"),
            action: format!("index:{name}"),
            args: None,
        }
    }

    fn resolved_action_lookup(lookup: &ActionIdLookup) -> HashMap<String, Action> {
        lookup
            .iter()
            .map(|(id, action)| (id.to_owned(), action.clone()))
            .collect()
    }

    fn wait_for_index_ready(app: &mut LauncherApp) {
        let event = app
            .rx
            .recv_timeout(Duration::from_secs(5))
            .expect("index completion enqueues an app-scoped ready event");
        app.event_sink.event_consumed();
        assert!(matches!(event, WatchEvent::IndexReady));
        app.process_index_ready();
    }

    fn process_index_ready_through_watch_event(app: &mut LauncherApp) {
        let event = app
            .rx
            .recv_timeout(Duration::from_secs(5))
            .expect("index completion enqueues an app-scoped ready event");
        assert!(matches!(event, WatchEvent::IndexReady));
        app.event_tx
            .send(event)
            .expect("the app event receiver remains connected");
        app.process_watch_events();
        assert!(app.rx.try_recv().is_err(), "ready event was consumed");
    }

    #[test]
    fn gui_index_accepted_publication_updates_search_history_pins_and_radial_catalog() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let empty_config = crate::indexer::coordinator::IndexConfig::new(Vec::new(), None);
        let (_workspace, mut app) =
            new_isolated_index_app(&egui::Context::default(), &empty_config);
        app.watchers.clear();
        app.process_watch_events();

        let custom = custom_action("custom-prefix");
        let old_retained = Action {
            label: "Old retained target".into(),
            desc: "Indexed file".into(),
            action: "index:retained-id".into(),
            args: None,
        };
        let old_removed = Action {
            label: "Removed target".into(),
            desc: "Indexed file".into(),
            action: "index:removed-id".into(),
            args: None,
        };
        app.actions = Arc::new(vec![custom.clone(), old_retained, old_removed]);
        app.custom_len = 1;
        app.update_action_cache();
        app.query = "app needle".into();
        app.match_exact = true;
        app.search();
        assert!(app.last_results_valid);
        assert!(
            !app.results
                .iter()
                .any(|action| action.action == "index:retained-id")
        );
        let previous_query = app.last_search_query.clone();

        let renamed = Action {
            label: "Fresh needle retained target".into(),
            desc: "Indexed file".into(),
            action: "index:retained-id".into(),
            args: None,
        };
        let added = Action {
            label: "Other indexed file".into(),
            desc: "Indexed file".into(),
            action: "index:new-id".into(),
            args: None,
        };
        let next_tail = vec![renamed.clone(), added.clone()];
        let updated_added = Action {
            label: "Other refreshed indexed file".into(),
            ..added.clone()
        };
        let updated_tail = vec![renamed.clone(), updated_added.clone()];
        let scanner_tail = next_tail.clone();
        let scanner_updated_tail = updated_tail.clone();
        let coordinator =
            crate::indexer::coordinator::IndexCoordinator::with_test_scanner(move |config| {
                if config
                    .roots()
                    .first()
                    .is_some_and(|root| root == "version-only-publication")
                {
                    Ok(scanner_updated_tail.clone())
                } else if config
                    .roots()
                    .first()
                    .is_some_and(|root| root == "equal-accepted-publication")
                {
                    Ok(scanner_updated_tail.clone())
                } else {
                    Ok(scanner_tail.clone())
                }
            })
            .unwrap();
        app.install_test_index_coordinator(coordinator);

        let version_before = crate::actions::actions_version();
        let config = crate::indexer::coordinator::IndexConfig::new(
            vec!["accepted-publication".into()],
            None,
        );
        app.request_index_config(config);
        process_index_ready_through_watch_event(&mut app);

        assert_eq!(app.custom_len, 1);
        assert_eq!(
            &app.actions[..],
            &[custom.clone(), renamed.clone(), added.clone()]
        );
        assert_eq!(crate::actions::actions_version(), version_before + 1);
        assert_eq!(app.actions_by_id.get("index:retained-id"), Some(&renamed));
        assert_eq!(app.actions_by_id.get("index:new-id"), Some(&added));
        assert!(app.actions_by_id.get("index:removed-id").is_none());
        assert_eq!(app.action_cache[1].label_lc, "fresh needle retained target");
        assert_eq!(
            app.action_filter_metadata[1].normalized_id,
            "index:retained-id"
        );
        assert_eq!(app.query, "app needle");
        assert_eq!(app.last_search_query, previous_query);
        assert!(app.last_results_valid);
        assert_eq!(
            app.results
                .iter()
                .map(|action| action.action.as_str())
                .collect::<Vec<_>>(),
            ["index:retained-id"],
            "the unchanged query is recomputed against the accepted catalog"
        );

        let retained_pin = crate::history::HistoryPin {
            action_id: "index:retained-id".into(),
            label: "Saved retained label".into(),
            desc: "Saved indexed file".into(),
            args: None,
            query: "needle".into(),
            timestamp: 20,
        };
        let removed_pin = crate::history::HistoryPin {
            action_id: "index:removed-id".into(),
            label: "Saved removed label".into(),
            desc: "Saved indexed file".into(),
            args: None,
            query: "removed".into(),
            timestamp: 10,
        };
        let history_rows =
            crate::dashboard::widgets::CommandHistoryWidget::prepare_pinned_entries_for_test(
                vec![retained_pin.clone(), removed_pin.clone()],
                &app.plugins,
                app.enabled_plugins.as_ref(),
                &app.actions_by_id,
                &app.dashboard_data_cache,
            );
        assert_eq!(history_rows.len(), 2);
        assert_eq!(history_rows[0], (renamed.clone(), false));
        assert_eq!(
            history_rows[1],
            (
                Action {
                    label: "Saved removed label".into(),
                    desc: "Saved indexed file".into(),
                    action: "index:removed-id".into(),
                    args: None,
                },
                true,
            )
        );
        assert_eq!(app.resolve_pin_action(&retained_pin), Some(renamed.clone()));
        assert_eq!(app.resolve_pin_action(&removed_pin), None);

        // Keep query results stable across this next accepted publication so
        // the radial snapshot's rebuild is attributable to actions_version.
        let radial_before = app.cached_universal_action_catalog_snapshot();
        let radial_builds_before = app.authoring_catalog_build_count.load(Ordering::SeqCst);
        let results_before_version_only_change = app.results.clone();
        let query_before_version_only_change = app.query.clone();
        let version_before_version_only_change = crate::actions::actions_version();
        app.request_index_config(crate::indexer::coordinator::IndexConfig::new(
            vec!["version-only-publication".into()],
            None,
        ));
        process_index_ready_through_watch_event(&mut app);
        assert_eq!(
            &app.actions[..],
            &[custom.clone(), renamed.clone(), updated_added.clone()]
        );
        assert_eq!(
            crate::actions::actions_version(),
            version_before_version_only_change + 1
        );
        assert_eq!(app.query, query_before_version_only_change);
        assert_eq!(app.results, results_before_version_only_change);

        let radial_after = app.cached_universal_action_catalog_snapshot();
        assert!(!Arc::ptr_eq(&radial_before, &radial_after));
        assert_eq!(
            radial_after
                .entries
                .iter()
                .take(app.actions.len())
                .map(|entry| entry.selected_action.clone())
                .collect::<Vec<_>>(),
            app.actions.as_ref().clone(),
            "the accepted action catalog is the radial snapshot prefix"
        );
        let builds_after_publication = app.authoring_catalog_build_count.load(Ordering::SeqCst);
        assert_eq!(builds_after_publication, radial_builds_before + 1);
        let radial_reused = app.cached_universal_action_catalog_snapshot();
        assert!(Arc::ptr_eq(&radial_after, &radial_reused));
        assert_eq!(
            app.authoring_catalog_build_count.load(Ordering::SeqCst),
            builds_after_publication
        );

        let actions_after_publication = Arc::clone(&app.actions);
        let cache_after_publication = resolved_action_lookup(&app.actions_by_id);
        let results_after_publication = app.results.clone();
        let version_after_publication = crate::actions::actions_version();
        let second_config = crate::indexer::coordinator::IndexConfig::new(
            vec!["equal-accepted-publication".into()],
            None,
        );
        app.request_index_config(second_config);
        process_index_ready_through_watch_event(&mut app);
        assert!(Arc::ptr_eq(&app.actions, &actions_after_publication));
        assert_eq!(
            resolved_action_lookup(&app.actions_by_id),
            cache_after_publication
        );
        assert_eq!(app.results, results_after_publication);
        assert_eq!(crate::actions::actions_version(), version_after_publication);
        assert!(Arc::ptr_eq(
            &radial_reused,
            &app.cached_universal_action_catalog_snapshot()
        ));
    }

    #[test]
    fn gui_index_startup_transfer_keeps_the_acknowledged_catalog_without_resubmitting() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let config =
            crate::indexer::coordinator::IndexConfig::new(vec!["startup-root".into()], Some(17));
        let (_workspace, mut app) = new_isolated_index_app(&egui::Context::default(), &config);
        let custom = custom_action("startup-custom");
        let indexed = index_test_action("startup-indexed");
        let coordinator = crate::indexer::coordinator::IndexCoordinator::with_test_scanner({
            let indexed = indexed.clone();
            move |_| Ok(vec![indexed.clone()])
        })
        .unwrap();
        let generation = coordinator.submit(config.clone()).unwrap();
        let completed = coordinator.wait_for_completion(generation).unwrap();
        assert_eq!(completed.outcome().as_ref().unwrap().as_slice(), &[indexed]);
        let acknowledged = coordinator.take_result().unwrap();
        assert_eq!(acknowledged.generation(), generation);
        assert!(coordinator.take_result().is_none());
        coordinator
            .validate_acknowledged_result(generation, &config)
            .unwrap();

        app.actions = Arc::new(vec![custom.clone(), index_test_action("startup-indexed")]);
        app.custom_len = 1;
        app.update_action_cache();
        app.install_startup_indexing(coordinator, config.clone(), generation)
            .unwrap();
        app.request_index_config(config);

        assert_eq!(app.indexing.expected_generation, Some(generation));
        assert_eq!(app.custom_len, 1);
        assert_eq!(app.actions[0], custom);
        assert_eq!(app.actions[1].action, "index:startup-indexed");
        assert!(matches!(
            app.rx.try_recv(),
            Err(std::sync::mpsc::TryRecvError::Empty)
        ));
    }

    #[test]
    fn gui_index_startup_install_rejects_an_unacknowledged_completion() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let config =
            crate::indexer::coordinator::IndexConfig::new(vec!["startup-root".into()], Some(17));
        let (_workspace, mut app) = new_isolated_index_app(&egui::Context::default(), &config);
        let coordinator =
            crate::indexer::coordinator::IndexCoordinator::with_test_scanner(|_| Ok(Vec::new()))
                .unwrap();
        let generation = coordinator.submit(config.clone()).unwrap();
        coordinator.wait_for_completion(generation).unwrap();

        let error = app
            .install_startup_indexing(coordinator, config, generation)
            .unwrap_err();
        assert!(matches!(
            error,
            crate::indexer::coordinator::CoordinatorError::StartupResultNotAcknowledged(_)
        ));
        assert!(app.indexing.expected_generation.is_none());
    }

    #[test]
    fn gui_index_config_identity_includes_cap_and_root_order_and_coalesces_equal_requests() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let (_workspace, mut app) = new_isolated_index_app(
            &egui::Context::default(),
            &crate::indexer::coordinator::IndexConfig::new(Vec::new(), None),
        );
        let coordinator =
            crate::indexer::coordinator::IndexCoordinator::with_test_scanner(|_| Ok(Vec::new()))
                .unwrap();
        app.install_test_index_coordinator(coordinator);

        let first = crate::indexer::coordinator::IndexConfig::new(
            vec!["root-a".into(), "root-b".into()],
            None,
        );
        app.request_index_config(first.clone());
        let generation_one = app.indexing.expected_generation.unwrap();
        app.request_index_config(first.clone());
        assert_eq!(app.indexing.expected_generation, Some(generation_one));

        let cap_only = crate::indexer::coordinator::IndexConfig::new(
            vec!["root-a".into(), "root-b".into()],
            Some(25),
        );
        app.request_index_config(cap_only.clone());
        let generation_two = app.indexing.expected_generation.unwrap();
        assert!(generation_two > generation_one);

        let reordered = crate::indexer::coordinator::IndexConfig::new(
            vec!["root-b".into(), "root-a".into()],
            Some(25),
        );
        app.request_index_config(reordered.clone());
        let generation_three = app.indexing.expected_generation.unwrap();
        assert!(generation_three > generation_two);

        let roots_only = crate::indexer::coordinator::IndexConfig::new(
            vec!["root-c".into(), "root-a".into()],
            Some(25),
        );
        app.request_index_config(roots_only.clone());
        assert!(app.indexing.expected_generation.unwrap() > generation_three);
        assert_eq!(app.indexing.desired, roots_only);
    }

    #[test]
    fn gui_index_equal_completion_preserves_catalog_and_search_state() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let (_workspace, mut app) = new_isolated_index_app(
            &egui::Context::default(),
            &crate::indexer::coordinator::IndexConfig::new(Vec::new(), None),
        );
        let custom = custom_action("custom");
        let tail = index_test_action("same-tail");
        app.actions = Arc::new(vec![custom, tail.clone()]);
        app.custom_len = 1;
        app.update_action_cache();
        app.query = "same".into();
        app.results = vec![tail.clone()];
        app.selected = Some(0);
        let actions = Arc::clone(&app.actions);
        let by_id = resolved_action_lookup(&app.actions_by_id);
        let results = app.results.clone();
        let query = app.query.clone();
        let selected = app.selected;
        let version = crate::actions::actions_version();
        let coordinator =
            crate::indexer::coordinator::IndexCoordinator::with_test_scanner(move |_| {
                Ok(vec![tail.clone()])
            })
            .unwrap();
        app.install_test_index_coordinator(coordinator);
        app.request_index_config(crate::indexer::coordinator::IndexConfig::new(
            vec!["equal-result".into()],
            None,
        ));
        wait_for_index_ready(&mut app);

        assert!(Arc::ptr_eq(&app.actions, &actions));
        assert_eq!(resolved_action_lookup(&app.actions_by_id), by_id);
        assert_eq!(app.results, results);
        assert_eq!(app.query, query);
        assert_eq!(app.selected, selected);
        assert_eq!(crate::actions::actions_version(), version);
    }

    #[test]
    fn gui_index_wake_is_app_scoped_and_shutdown_ignores_late_completion() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let workspace = crate::performance::workloads::IsolatedWorkspace::new();
        let empty_config = crate::indexer::coordinator::IndexConfig::new(Vec::new(), None);
        let context_a = egui::Context::default();
        let context_b = egui::Context::default();
        let mut app_a =
            new_index_app_in_workspace(&context_a, &workspace, &empty_config, "owner-a");
        let app_b = new_index_app_in_workspace(&context_b, &workspace, &empty_config, "owner-b");
        let (blocked_started_tx, blocked_started_rx) = channel();
        let (release_tx, release_rx) = channel();
        let release_rx = Arc::new(Mutex::new(release_rx));
        struct ReleaseOnDrop(std::sync::mpsc::Sender<()>);
        impl Drop for ReleaseOnDrop {
            fn drop(&mut self) {
                let _ = self.0.send(());
            }
        }
        let _release_on_drop = ReleaseOnDrop(release_tx.clone());
        let coordinator = crate::indexer::coordinator::IndexCoordinator::with_test_scanner({
            let release_rx = Arc::clone(&release_rx);
            move |config| match config.roots().first().map(String::as_str) {
                Some("blocked") => {
                    blocked_started_tx.send(()).unwrap();
                    release_rx
                        .lock()
                        .unwrap()
                        .recv_timeout(Duration::from_secs(5))
                        .unwrap();
                    Ok(vec![index_test_action("late")])
                }
                _ => Ok(vec![index_test_action("ready")]),
            }
        })
        .unwrap();
        app_a.install_test_index_coordinator(coordinator);
        app_a.actions = Arc::new(vec![custom_action("custom"), index_test_action("old-tail")]);
        app_a.custom_len = 1;
        app_a.update_action_cache();

        let initial_version = crate::actions::actions_version();
        app_a.request_index_config(crate::indexer::coordinator::IndexConfig::new(
            vec!["ready".into()],
            None,
        ));
        wait_for_index_ready(&mut app_a);
        assert_eq!(app_a.actions[1], index_test_action("ready"));
        assert_eq!(crate::actions::actions_version(), initial_version + 1);
        assert!(matches!(
            app_b.rx.try_recv(),
            Err(std::sync::mpsc::TryRecvError::Empty)
        ));
        let ready_actions = Arc::clone(&app_a.actions);
        let ready_version = crate::actions::actions_version();
        app_a.process_index_ready();
        assert!(Arc::ptr_eq(&app_a.actions, &ready_actions));
        assert_eq!(crate::actions::actions_version(), ready_version);

        app_a.request_index_config(crate::indexer::coordinator::IndexConfig::new(
            vec!["blocked".into()],
            None,
        ));
        blocked_started_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("the second scan is blocked inside the test scanner");
        let retained_actions = Arc::clone(&app_a.actions);
        let retained_version = crate::actions::actions_version();
        app_a.shutdown_indexing().unwrap();
        assert!(app_a.indexing.expected_generation.is_none());
        app_a.event_tx.send(WatchEvent::IndexReady).unwrap();
        app_a.process_watch_events();
        assert!(Arc::ptr_eq(&app_a.actions, &retained_actions));
        assert_eq!(crate::actions::actions_version(), retained_version);
        drop(app_a);
        release_tx.send(()).unwrap();
        assert!(matches!(
            app_b.rx.try_recv(),
            Err(std::sync::mpsc::TryRecvError::Empty)
        ));
    }

    #[test]
    fn gui_index_completion_merges_the_latest_actions_prefix_after_a_blocked_scan() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let (slow_started_tx, slow_started_rx) = channel();
        let (release_slow_tx, release_slow_rx) = channel();
        let release_slow_rx = Arc::new(Mutex::new(release_slow_rx));
        let (fresh_started_tx, fresh_started_rx) = channel();
        let coordinator = crate::indexer::coordinator::IndexCoordinator::with_test_scanner({
            let release_slow_rx = Arc::clone(&release_slow_rx);
            move |config| match config.roots().first().map(String::as_str) {
                Some("slow") => {
                    slow_started_tx.send(()).unwrap();
                    release_slow_rx
                        .lock()
                        .unwrap()
                        .recv_timeout(Duration::from_secs(5))
                        .unwrap();
                    Ok(vec![index_test_action("stale")])
                }
                Some("fresh") => {
                    fresh_started_tx.send(()).unwrap();
                    Ok(vec![index_test_action("fresh")])
                }
                _ => Ok(Vec::new()),
            }
        })
        .unwrap();
        let (workspace, mut app) = new_isolated_index_app(
            &egui::Context::default(),
            &crate::indexer::coordinator::IndexConfig::new(Vec::new(), None),
        );
        app.install_test_index_coordinator(coordinator);
        let initial_custom = custom_action("committed");
        app.actions = Arc::new(vec![initial_custom, index_test_action("old-tail")]);
        app.custom_len = 1;
        app.update_action_cache();

        let actions_path = workspace.root().join("actions.json");
        app.actions_path = actions_path.to_string_lossy().into_owned();
        let before_version = crate::actions::actions_version();
        app.request_index_config(crate::indexer::coordinator::IndexConfig::new(
            vec!["slow".into()],
            None,
        ));
        slow_started_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("the first request reached its gated scanner");
        app.request_index_config(crate::indexer::coordinator::IndexConfig::new(
            vec!["fresh".into()],
            None,
        ));

        let external_custom = vec![custom_action("external-current")];
        std::fs::write(&actions_path, serde_json::to_vec(&external_custom).unwrap()).unwrap();
        app.event_tx.send(WatchEvent::Actions).unwrap();
        app.process_watch_events();
        assert_eq!(app.actions[..app.custom_len], external_custom);
        assert_eq!(app.actions[app.custom_len].action, "index:old-tail");

        release_slow_tx.send(()).unwrap();
        fresh_started_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("replacement request follows the cancelled traversal");
        wait_for_index_ready(&mut app);

        assert_eq!(app.actions[..app.custom_len], external_custom);
        assert_eq!(app.actions[app.custom_len..], [index_test_action("fresh")]);
        assert_eq!(crate::actions::actions_version(), before_version + 2);
        assert!(app.indexing.diagnostic().is_none());
    }

    #[test]
    fn gui_index_failure_retains_last_good_actions_then_recovers() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let failure_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let coordinator = {
            let failure_count = Arc::clone(&failure_count);
            crate::indexer::coordinator::IndexCoordinator::with_test_scanner(move |config| {
                if config.roots().first().map(String::as_str) == Some("failure")
                    && failure_count.fetch_add(1, Ordering::SeqCst) == 0
                {
                    Err("synthetic scan failure".into())
                } else {
                    Ok(vec![index_test_action("recovered")])
                }
            })
            .unwrap()
        };
        let (_workspace, mut app) = new_isolated_index_app(
            &egui::Context::default(),
            &crate::indexer::coordinator::IndexConfig::new(Vec::new(), None),
        );
        app.install_test_index_coordinator(coordinator);
        app.actions = Arc::new(vec![
            custom_action("custom"),
            index_test_action("last-good"),
        ]);
        app.custom_len = 1;
        app.update_action_cache();
        let last_good = Arc::clone(&app.actions);
        let version = crate::actions::actions_version();

        let failed_config =
            crate::indexer::coordinator::IndexConfig::new(vec!["failure".into()], None);
        app.request_index_config(failed_config.clone());
        let failed_generation = app.indexing.expected_generation.unwrap();
        wait_for_index_ready(&mut app);
        assert!(Arc::ptr_eq(&app.actions, &last_good));
        assert_eq!(crate::actions::actions_version(), version);
        assert!(
            app.indexing
                .diagnostic()
                .unwrap()
                .contains("synthetic scan failure")
        );
        assert!(app.actions_persistence_diagnostic.is_none());

        app.request_index_config(failed_config.clone());
        assert!(app.indexing.expected_generation.unwrap() > failed_generation);
        wait_for_index_ready(&mut app);
        assert_eq!(app.actions[0].action, "custom:action");
        assert_eq!(app.actions[1], index_test_action("recovered"));
        assert_eq!(crate::actions::actions_version(), version + 1);
        assert!(app.indexing.diagnostic().is_none());
    }

    #[test]
    fn gui_index_empty_config_clears_tail_immediately_and_ignores_cancelled_result() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let (started_tx, started_rx) = channel();
        let (release_tx, release_rx) = channel();
        let release_rx = Arc::new(Mutex::new(release_rx));
        let coordinator = crate::indexer::coordinator::IndexCoordinator::with_test_scanner({
            let release_rx = Arc::clone(&release_rx);
            move |config| {
                if config.roots().first().map(String::as_str) == Some("blocked") {
                    started_tx.send(()).unwrap();
                    release_rx
                        .lock()
                        .unwrap()
                        .recv_timeout(Duration::from_secs(5))
                        .unwrap();
                    Ok(vec![index_test_action("obsolete")])
                } else {
                    Ok(Vec::new())
                }
            }
        })
        .unwrap();
        let (_workspace, mut app) = new_isolated_index_app(
            &egui::Context::default(),
            &crate::indexer::coordinator::IndexConfig::new(Vec::new(), None),
        );
        app.install_test_index_coordinator(coordinator);
        app.actions = Arc::new(vec![custom_action("custom"), index_test_action("old-tail")]);
        app.custom_len = 1;
        app.update_action_cache();
        let version = crate::actions::actions_version();
        app.request_index_config(crate::indexer::coordinator::IndexConfig::new(
            vec!["blocked".into()],
            None,
        ));
        started_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("blocked scan started");

        app.request_index_config(crate::indexer::coordinator::IndexConfig::new(
            Vec::new(),
            None,
        ));
        assert_eq!(app.actions.as_slice(), &[custom_action("custom")]);
        assert_eq!(crate::actions::actions_version(), version + 1);
        let empty_publication = Arc::clone(&app.actions);
        release_tx.send(()).unwrap();
        wait_for_index_ready(&mut app);
        assert!(Arc::ptr_eq(&app.actions, &empty_publication));
        assert_eq!(app.actions.as_slice(), &[custom_action("custom")]);
        assert_eq!(crate::actions::actions_version(), version + 1);
    }

    #[test]
    fn saved_pin_rebinds_only_by_canonical_alias_not_literal_body() {
        let directory = tempdir().unwrap();
        let app = LauncherApp::new(
            &egui::Context::default(),
            Arc::new(Vec::new()),
            0,
            PluginManager::new(),
            directory
                .path()
                .join("actions.json")
                .to_string_lossy()
                .into_owned(),
            directory
                .path()
                .join("settings.json")
                .to_string_lossy()
                .into_owned(),
            Settings::default(),
            None,
            None,
            None,
            None,
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        );
        let shared_text = "same literal {{ body }}";
        let entries = vec![
            crate::plugins::snippets::SnippetEntry {
                alias: "first".into(),
                text: shared_text.into(),
                hide_contents: false,
                prompt_for_fields: false,
                fields: Vec::new(),
                tags: Vec::new(),
            },
            crate::plugins::snippets::SnippetEntry {
                alias: "second".into(),
                text: shared_text.into(),
                hide_contents: true,
                prompt_for_fields: true,
                fields: vec![crate::plugins::snippets::SnippetFieldDefinition::new(
                    "body",
                )],
                tags: Vec::new(),
            },
        ];
        let mut snapshot = crate::dashboard::data_cache::DashboardDataSnapshot::default();
        snapshot.snippets = Arc::new(entries.clone());
        app.dashboard_data_cache.set_snapshot_for_test(snapshot);

        let pin = |action_id: String, args| HistoryPin {
            action_id,
            label: "saved label".into(),
            desc: "Snippet".into(),
            args,
            query: String::new(),
            timestamp: 0,
        };
        let resolved = app
            .resolve_pin_action(&pin(
                crate::plugins::snippets::snippet_run_action("second"),
                None,
            ))
            .expect("canonical saved identity resolves by its exact alias");
        assert_eq!(resolved.label, "second");
        assert_eq!(
            resolved.action,
            crate::plugins::snippets::snippet_run_action("second")
        );

        let literal_pin = pin("clipboard:same literal {{ body }}".into(), None);
        let literal = app
            .resolve_pin_action(&literal_pin)
            .expect("opaque literal clipboard pins remain executable");
        assert_eq!(literal.label, literal_pin.label);
        assert_eq!(literal.desc, literal_pin.desc);
        assert_eq!(literal.action, literal_pin.action_id);
        assert_eq!(literal.args, literal_pin.args);
        assert!(
            app.resolve_pin_action(&pin(
                crate::plugins::snippets::snippet_run_action("missing"),
                None,
            ))
            .is_none()
        );
        assert!(
            app.resolve_pin_action(&pin("clipboard:copy:99".into(), None))
                .is_none(),
            "indexed clipboard copies still require a current entry"
        );
        assert!(
            app.resolve_pin_action(&pin(
                crate::plugins::snippets::snippet_run_action("second"),
                Some("unexpected".into()),
            ))
            .is_none()
        );

        let mut duplicate_snapshot = crate::dashboard::data_cache::DashboardDataSnapshot::default();
        duplicate_snapshot.snippets = Arc::new(vec![entries[1].clone(), entries[1].clone()]);
        app.dashboard_data_cache
            .set_snapshot_for_test(duplicate_snapshot);
        assert!(
            app.resolve_pin_action(&pin(
                crate::plugins::snippets::snippet_run_action("second"),
                None,
            ))
            .is_none()
        );
    }

    fn deferred_child_input(viewport_id: egui::ViewportId) -> egui::RawInput {
        let mut input = egui::RawInput {
            viewport_id,
            ..Default::default()
        };
        input.viewports.insert(
            viewport_id,
            egui::ViewportInfo {
                parent: Some(egui::ViewportId::ROOT),
                ..Default::default()
            },
        );
        input
    }

    fn focus_commands(output: &egui::FullOutput, viewport_id: egui::ViewportId) -> usize {
        output
            .viewport_output
            .get(&viewport_id)
            .map(|viewport| {
                viewport
                    .commands
                    .iter()
                    .filter(|command| matches!(command, egui::ViewportCommand::Focus))
                    .count()
            })
            .unwrap_or_default()
    }

    fn register_radial_deferred_callback(
        ctx: &egui::Context,
        app: &LauncherApp,
    ) -> (egui::FullOutput, Arc<egui::DeferredViewportUiCallback>) {
        ctx.set_embed_viewports(false);
        let output = ctx.run_ui(egui::RawInput::default(), |root| {
            radial_editor::RadialEditorState::show_deferred(
                &app.radial_editor,
                root,
                app,
                None,
                false,
                true,
            );
        });
        let callback = output
            .viewport_output
            .get(&radial_editor::radial_designer_viewport_id())
            .and_then(|viewport| viewport.viewport_ui_cb.clone())
            .expect("radial deferred callback registered");
        (output, callback)
    }

    #[test]
    fn physical_paste_delivery_targets_one_registered_owner_without_buffering() {
        use crate::paste_sequence::admission::PhysicalPasteNotice;
        let _guard = TEST_MUTEX.lock().unwrap();
        reset_event_registry_for_test();
        let notice = || PhysicalPasteNotice::Busy {
            owner: 1,
            generation: 7,
        };
        assert!(send_physical_paste_event(1, notice()).is_err());
        let (other_tx, other_rx) = channel();
        let other = register_event_sender_with_wake(
            other_tx,
            ViewportWake::from_callback(egui::ViewportId::ROOT, |_| {}),
        );
        assert!(
            other_rx.try_recv().is_err(),
            "physical work is never pre-owner buffered"
        );
        let (tx, rx) = channel();
        let wake_count = Arc::new(AtomicUsize::new(0));
        let wake_count_clone = Arc::clone(&wake_count);
        let owner = register_event_sender_with_wake(
            tx,
            ViewportWake::from_callback(egui::ViewportId::ROOT, move |_| {
                wake_count_clone.fetch_add(1, Ordering::SeqCst);
            }),
        );
        send_physical_paste_event(owner.owner_id(), notice()).unwrap();
        assert!(matches!(rx.try_recv(), Ok(WatchEvent::PhysicalPaste(_))));
        assert!(other_rx.try_recv().is_err());
        assert_eq!(wake_count.load(Ordering::SeqCst), 1);
        let old_id = owner.owner_id();
        drop(owner);
        assert!(send_physical_paste_event(old_id, notice()).is_err());
        drop(other);
        let (dead_tx, dead_rx) = channel();
        let dead = register_event_sender_with_wake(
            dead_tx,
            ViewportWake::from_callback(egui::ViewportId::ROOT, |_| {}),
        );
        drop(dead_rx);
        assert!(send_physical_paste_event(dead.owner_id(), notice()).is_err());
        let (no_wake_tx, _no_wake_rx) = channel();
        let no_wake = register_event_sender(no_wake_tx);
        assert!(send_physical_paste_event(no_wake.owner_id(), notice()).is_err());
    }

    #[test]
    fn event_sink_attaches_wake_to_work_already_in_queue() {
        let _guard = TEST_MUTEX.lock().unwrap();
        reset_event_registry_for_test();
        let (tx, rx) = channel();
        let registration = register_event_sender(tx);
        send_event(WatchEvent::Actions);
        assert!(rx.try_recv().is_ok());

        let wake_count = Arc::new(AtomicUsize::new(0));
        let wake_count_for_callback = Arc::clone(&wake_count);
        registration.attach_wake(ViewportWake::from_callback(
            egui::ViewportId::ROOT,
            move |_| {
                wake_count_for_callback.fetch_add(1, Ordering::SeqCst);
            },
        ));
        assert_eq!(wake_count.load(Ordering::SeqCst), 1);
        registration.event_consumed();
    }

    #[test]
    fn event_sink_enqueues_before_waking_and_handles_bursts() {
        let _guard = TEST_MUTEX.lock().unwrap();
        reset_event_registry_for_test();
        let (tx, rx) = channel();
        let rx = Arc::new(Mutex::new(rx));
        let saw_events = Arc::new(Mutex::new(Vec::new()));
        let rx_for_callback = Arc::clone(&rx);
        let saw_events_for_callback = Arc::clone(&saw_events);
        let registration = register_event_sender_with_wake(
            tx,
            ViewportWake::from_callback(egui::ViewportId::ROOT, move |_| {
                let event = rx_for_callback
                    .lock()
                    .unwrap()
                    .try_recv()
                    .expect("wake must follow enqueue");
                saw_events_for_callback.lock().unwrap().push(event);
            }),
        );

        send_event(WatchEvent::Actions);
        send_event(WatchEvent::Folders);
        send_event(WatchEvent::Bookmarks);

        let events = saw_events.lock().unwrap();
        assert!(matches!(
            events.as_slice(),
            [
                WatchEvent::Actions,
                WatchEvent::Folders,
                WatchEvent::Bookmarks
            ]
        ));
        drop(events);
        registration.event_consumed();
        registration.event_consumed();
        registration.event_consumed();
    }

    #[test]
    fn disposed_or_dead_event_sinks_are_not_called_again() {
        let _guard = TEST_MUTEX.lock().unwrap();
        reset_event_registry_for_test();
        let (tx, rx) = channel();
        let registration = register_event_sender(tx);
        drop(registration);
        send_event(WatchEvent::Actions);
        assert!(rx.try_recv().is_err());

        let (dead_tx, dead_rx) = channel();
        let _dead_registration = register_event_sender(dead_tx);
        drop(dead_rx);
        send_event(WatchEvent::Actions);
    }

    #[test]
    fn event_sink_delivers_work_emitted_before_first_owner_registers() {
        let _guard = TEST_MUTEX.lock().unwrap();
        reset_event_registry_for_test();
        send_event(WatchEvent::Actions);

        let (tx, rx) = channel();
        let registration = register_event_sender(tx);
        assert!(matches!(rx.try_recv(), Ok(WatchEvent::Actions)));
        assert!(
            rx.try_recv().is_err(),
            "pre-registration work delivered once"
        );

        let wake_count = Arc::new(AtomicUsize::new(0));
        let wake_count_for_callback = Arc::clone(&wake_count);
        registration.attach_wake(ViewportWake::from_callback(
            egui::ViewportId::ROOT,
            move |_| {
                wake_count_for_callback.fetch_add(1, Ordering::SeqCst);
            },
        ));
        assert_eq!(wake_count.load(Ordering::SeqCst), 1);
        registration.event_consumed();
    }

    fn custom_action(label: &str) -> Action {
        Action {
            label: label.into(),
            desc: "custom".into(),
            action: format!("{label}:action"),
            args: None,
        }
    }

    #[test]
    fn target_repair_ui_editor_close_retires_child_and_rejects_late_completion() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let saved = Action {
            label: "Repairable item".into(),
            desc: "Saved description".into(),
            action: r#"C:\missing\tool.exe"#.into(),
            args: Some(String::new()),
        };
        let selected = crate::target_repair::SelectedTarget::action_with_kind(
            saved.clone(),
            0,
            crate::target_repair::ActionTargetKind::FileOrFolder,
        );
        app.show_editor = true;
        app.panel_states.editor = true;
        app.panel_stack.push(Panel::Editor);
        app.editor.open_repair(selected.clone());
        let retired_token = app.editor.test_repair_token().unwrap();
        app.editor.test_set_repair_pending(true);
        let replacement = std::path::PathBuf::from(r#"C:\replacement\tool.exe"#);
        let late = crate::target_repair::RepairCompletion {
            request: crate::target_repair::RepairRequest::new(
                selected.clone(),
                replacement.clone(),
            )
            .with_expected_replacement_kind(crate::target_repair::FilesystemTargetKind::File)
            .with_presentation_token(retired_token),
            result: Err(crate::target_repair::RepairFailure {
                kind: crate::target_repair::RepairFailureKind::WorkerUnavailable,
                message: "late completion",
            }),
        };

        assert!(app.close_front_dialog());
        assert!(
            app.show_editor,
            "Escape first closes the nested repair child"
        );
        assert!(!app.editor.repair_dialog_is_open());
        assert_eq!(app.panel_stack.last(), Some(&Panel::Editor));

        app.editor.open_repair(selected);
        app.editor.test_set_repair_pending(true);
        assert_ne!(app.editor.test_repair_token(), Some(retired_token));
        assert!(!app.editor.complete_target_repair(&late));
        assert!(app.editor.repair_dialog_is_open());
        assert_eq!(app.editor.test_repair_feedback(), None);
        assert_eq!(app.editor.test_repair_dialog_capture(), Some(saved));
        assert_eq!(app.editor.test_repair_path(), Some(""));
    }

    #[test]
    fn target_repair_transaction_publishes_committed_custom_rows_and_preserves_indexed_tail() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let workspace = crate::performance::workloads::IsolatedWorkspace::new();
        let config = crate::indexer::coordinator::IndexConfig::new(Vec::new(), None);
        let mut app = new_index_app_in_workspace(
            &egui::Context::default(),
            &workspace,
            &config,
            "target-repair-publication",
        );

        let repair_root = workspace.root().join("target-repair");
        std::fs::create_dir_all(&repair_root).unwrap();
        app.actions_path = repair_root
            .join("actions.json")
            .to_string_lossy()
            .into_owned();
        let missing_path = repair_root.join("missing-tool.exe");
        let original = Action {
            label: "Selected saved tool".into(),
            desc: "The label and arguments stay intact".into(),
            action: missing_path.to_string_lossy().into_owned(),
            args: Some("--profile=blue".into()),
        };
        let unrelated_external = Action {
            label: "Added outside this app".into(),
            desc: "This row was not in the stale GUI cache".into(),
            action: "external:custom-row".into(),
            args: None,
        };
        std::fs::write(
            &app.actions_path,
            serde_json::to_vec(&[original.clone(), unrelated_external.clone()]).unwrap(),
        )
        .unwrap();
        let replacement = repair_root.join("replacement.exe");
        std::fs::write(&replacement, b"owned test target").unwrap();

        let stale_indexed_tail = index_test_action("preserved-indexed-tail");
        app.actions = Arc::new(vec![original.clone(), stale_indexed_tail.clone()]);
        app.custom_len = 1;
        app.update_action_cache();

        let data_root = crate::platform::app_data::AppDataRoot::from_path(repair_root.clone());
        let coordinator = crate::target_repair::TargetRepairCoordinator::new(
            repair_root.clone(),
            Arc::new(crate::radial::store::RadialStore::new(&data_root).unwrap()),
            crate::hotfolder::HotFolderRulesStore::new(&data_root),
        );
        let commit = coordinator
            .repair(crate::target_repair::RepairRequest::new(
                crate::target_repair::SelectedTarget::action_with_kind(
                    original.clone(),
                    0,
                    crate::target_repair::ActionTargetKind::File,
                ),
                replacement.clone(),
            ))
            .unwrap();
        let updated = Action {
            action: replacement.to_string_lossy().into_owned(),
            ..original
        };
        let committed_custom = vec![updated.clone(), unrelated_external.clone()];
        assert_eq!(
            commit.custom_actions.as_deref(),
            Some(committed_custom.as_slice())
        );
        let committed_version = commit.actions_version.unwrap();

        app.publish_target_repair_actions(commit.custom_actions.clone(), commit.actions_version);
        assert_eq!(app.custom_len, 2);
        assert_eq!(&app.actions[..app.custom_len], committed_custom.as_slice());
        assert_eq!(
            &app.actions[app.custom_len..],
            std::slice::from_ref(&stale_indexed_tail),
            "repair publication preserves the currently indexed tail"
        );

        // A later accepted Actions publication must remain authoritative if a
        // delayed completion for this earlier commit is processed afterward.
        let newer_external = Action {
            label: "Later external edit".into(),
            desc: "Already published after repair".into(),
            action: "external:later-row".into(),
            args: None,
        };
        let later_custom = vec![updated, unrelated_external, newer_external];
        app.publish_custom_actions_with_indexed_tail(later_custom.clone());
        crate::actions::bump_actions_version();
        let before_stale_completion = Arc::clone(&app.actions);
        app.publish_target_repair_actions(commit.custom_actions, Some(committed_version));
        assert!(Arc::ptr_eq(&app.actions, &before_stale_completion));
        assert_eq!(&app.actions[..app.custom_len], later_custom.as_slice());
    }

    #[test]
    fn failed_custom_action_save_retains_published_state_and_version() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.actions = Arc::new(vec![custom_action("committed")]);
        app.custom_len = 1;
        app.update_action_cache();
        app.rebuild_completion_index_now();
        let actions_before = Arc::clone(&app.actions);
        let cache_before = app.action_cache.clone();
        let version_before = crate::actions::actions_version();

        let directory = tempdir().unwrap();
        let blocker = directory.path().join("not-a-directory");
        std::fs::write(&blocker, "unchanged").unwrap();
        app.actions_path = blocker.join("actions.json").to_string_lossy().into_owned();

        let result = app.update_custom_actions(|actions| {
            actions.push(custom_action("uncommitted"));
            Ok(())
        });

        assert!(result.is_err());
        assert_eq!(app.actions.as_ref(), actions_before.as_ref());
        assert_eq!(app.custom_len, 1);
        assert_eq!(app.action_cache, cache_before);
        assert!(app.completion_index.is_some());
        assert_eq!(crate::actions::actions_version(), version_before);
        assert_eq!(std::fs::read_to_string(blocker).unwrap(), "unchanged");
    }

    #[test]
    fn actions_watcher_retains_invalid_then_publishes_valid_without_local_double_bump() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let indexed_tail = vec![
            custom_action("indexed-first"),
            custom_action("indexed-second"),
            custom_action("indexed-third"),
        ];
        app.actions = Arc::new(
            std::iter::once(custom_action("committed"))
                .chain(indexed_tail.iter().cloned())
                .collect(),
        );
        app.custom_len = 1;
        app.update_action_cache();
        let directory = tempdir().unwrap();
        let path = directory.path().join("actions.json");
        app.actions_path = path.to_string_lossy().into_owned();
        let initial_actions = Arc::clone(&app.actions);
        let initial_cache = app.action_cache.clone();

        std::fs::write(&path, "not valid actions JSON").unwrap();
        let version_before = crate::actions::actions_version();
        crate::indexer::reset_index_batch_factory_entries_for_test();
        app.event_tx.send(WatchEvent::Actions).unwrap();
        app.process_watch_events();

        assert!(Arc::ptr_eq(&app.actions, &initial_actions));
        assert_eq!(app.action_cache, initial_cache);
        assert_eq!(&app.actions[1..], indexed_tail.as_slice());
        assert_eq!(app.custom_len, 1);
        assert!(matches!(
            app.actions_persistence_diagnostic.as_ref(),
            Some(crate::common::persistence::PersistenceError::MalformedJson { .. })
        ));
        assert_eq!(crate::actions::actions_version(), version_before);
        assert_eq!(crate::indexer::index_batch_factory_entries_for_test(), 0);

        std::fs::remove_file(&path).unwrap();
        crate::indexer::reset_index_batch_factory_entries_for_test();
        app.event_tx.send(WatchEvent::Actions).unwrap();
        app.process_watch_events();
        assert!(Arc::ptr_eq(&app.actions, &initial_actions));
        assert_eq!(app.action_cache, initial_cache);
        assert_eq!(&app.actions[1..], indexed_tail.as_slice());
        assert!(matches!(
            app.actions_persistence_diagnostic.as_ref(),
            Some(crate::common::persistence::PersistenceError::Read { source, .. })
                if source.kind() == std::io::ErrorKind::NotFound
        ));
        assert_eq!(crate::actions::actions_version(), version_before);
        assert_eq!(crate::indexer::index_batch_factory_entries_for_test(), 0);

        let external = vec![custom_action("external")];
        std::fs::write(&path, serde_json::to_vec_pretty(&external).unwrap()).unwrap();
        crate::indexer::reset_index_batch_factory_entries_for_test();
        app.event_tx.send(WatchEvent::Actions).unwrap();
        app.process_watch_events();

        assert_eq!(&app.actions[..app.custom_len], external.as_slice());
        assert_eq!(&app.actions[app.custom_len..], indexed_tail.as_slice());
        assert_eq!(app.actions_by_id.get("external:action"), external.first());
        assert!(app.actions_by_id.get("committed:action").is_none());
        assert_eq!(
            app.actions_by_id.get("indexed-first:action"),
            indexed_tail.first()
        );
        assert!(
            app.action_cache
                .iter()
                .any(|entry| entry.label_lc == "external")
        );
        assert!(app.actions_persistence_diagnostic.is_none());
        assert_eq!(crate::actions::actions_version(), version_before + 1);
        assert_eq!(crate::indexer::index_batch_factory_entries_for_test(), 0);

        // An empty JSON array removes only the custom prefix, while an empty
        // file remains a valid no-op reload of the empty custom set.
        let empty_array = serde_json::to_vec(&Vec::<Action>::new()).unwrap();
        std::fs::write(&path, &empty_array).unwrap();
        crate::indexer::reset_index_batch_factory_entries_for_test();
        app.event_tx.send(WatchEvent::Actions).unwrap();
        app.process_watch_events();
        assert_eq!(app.custom_len, 0);
        assert_eq!(app.actions.as_slice(), indexed_tail.as_slice());
        assert!(app.actions_by_id.get("external:action").is_none());
        assert_eq!(
            app.actions_by_id.get("indexed-second:action"),
            indexed_tail.get(1)
        );
        assert_eq!(crate::actions::actions_version(), version_before + 2);
        assert_eq!(crate::indexer::index_batch_factory_entries_for_test(), 0);

        let empty_array_actions = Arc::clone(&app.actions);
        let empty_array_version = crate::actions::actions_version();
        std::fs::write(&path, "").unwrap();
        crate::indexer::reset_index_batch_factory_entries_for_test();
        app.event_tx.send(WatchEvent::Actions).unwrap();
        app.process_watch_events();
        assert!(Arc::ptr_eq(&app.actions, &empty_array_actions));
        assert_eq!(crate::actions::actions_version(), empty_array_version);
        assert_eq!(crate::indexer::index_batch_factory_entries_for_test(), 0);

        crate::indexer::reset_index_batch_factory_entries_for_test();
        for _ in 0..3 {
            app.event_tx.send(WatchEvent::Actions).unwrap();
        }
        app.process_watch_events();
        assert!(Arc::ptr_eq(&app.actions, &empty_array_actions));
        assert_eq!(crate::actions::actions_version(), empty_array_version);
        assert_eq!(crate::indexer::index_batch_factory_entries_for_test(), 0);

        crate::indexer::reset_index_batch_factory_entries_for_test();
        app.update_custom_actions(|actions| {
            actions.push(custom_action("local"));
            Ok(())
        })
        .unwrap();
        assert_eq!(app.custom_len, 1);
        assert_eq!(&app.actions[app.custom_len..], indexed_tail.as_slice());
        assert_eq!(crate::indexer::index_batch_factory_entries_for_test(), 0);
        let version_after_local = crate::actions::actions_version();
        let published_after_local = Arc::clone(&app.actions);
        crate::indexer::reset_index_batch_factory_entries_for_test();
        app.event_tx.send(WatchEvent::Actions).unwrap();
        app.process_watch_events();

        assert!(Arc::ptr_eq(&app.actions, &published_after_local));
        assert_eq!(&app.actions[app.custom_len..], indexed_tail.as_slice());
        assert_eq!(crate::actions::actions_version(), version_after_local);
        assert_eq!(crate::indexer::index_batch_factory_entries_for_test(), 0);
    }

    #[test]
    fn failed_file_search_preferences_save_restores_committed_runtime_state() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let committed_preferences = app.file_search_dialog.settings.ui_preferences.clone();
        let committed_runtime_preferences = app
            .file_search_coordinator
            .production_settings()
            .unwrap()
            .ui_preferences
            .clone();
        let committed_editor_value = app
            .settings_editor
            .get_plugin_setting_value("file_search")
            .cloned();

        let mut candidate = committed_preferences.clone();
        candidate.whole_word = !candidate.whole_word;
        app.file_search_dialog.ui_preferences = candidate.clone();
        app.file_search_dialog.ui_preferences_dirty = true;

        let directory = tempdir().unwrap();
        let blocker = directory.path().join("not-a-directory");
        std::fs::write(&blocker, "unchanged").unwrap();
        app.settings_path = blocker.join("settings.json").to_string_lossy().into_owned();

        app.handle_file_search_ui_command(FileSearchUiCommand::PersistPreferences(candidate));

        assert_eq!(app.file_search_dialog.ui_preferences, committed_preferences);
        assert_eq!(
            app.file_search_dialog.settings.ui_preferences,
            committed_preferences
        );
        assert!(!app.file_search_dialog.ui_preferences_dirty);
        assert_eq!(
            app.file_search_coordinator
                .production_settings()
                .unwrap()
                .ui_preferences,
            committed_runtime_preferences
        );
        assert_eq!(
            app.settings_editor
                .get_plugin_setting_value("file_search")
                .cloned(),
            committed_editor_value
        );
        assert_eq!(std::fs::read_to_string(blocker).unwrap(), "unchanged");
    }

    #[derive(Clone)]
    struct TestPlugin {
        name: &'static str,
        caps: Vec<&'static str>,
        prefixes: Vec<&'static str>,
    }

    impl crate::plugin::Plugin for TestPlugin {
        fn search(&self, _query: &str) -> Vec<Action> {
            vec![Action {
                label: "plugin".into(),
                desc: "test".into(),
                action: "plugin:test".into(),
                args: None,
            }]
        }

        fn name(&self) -> &str {
            self.name
        }

        fn description(&self) -> &str {
            "test"
        }

        fn capabilities(&self) -> &[&str] {
            &self.caps
        }

        fn query_prefixes(&self) -> &[&str] {
            &self.prefixes
        }
    }

    struct ExactFilterPlugin;

    impl crate::plugin::Plugin for ExactFilterPlugin {
        fn search(&self, query: &str) -> Vec<Action> {
            let query = query.trim().to_ascii_lowercase();
            if query == "note today" {
                return vec![Action {
                    label: "Create 2025 02 23".into(),
                    desc: "Note".into(),
                    action: "note:new:2025-02-23".into(),
                    args: None,
                }];
            }
            if query.starts_with("note search ") {
                return vec![Action {
                    label: "Alpha note".into(),
                    desc: "Note".into(),
                    action: "note:open:alpha".into(),
                    args: None,
                }];
            }
            if query.starts_with("note ") {
                return vec![Action {
                    label: "note search".into(),
                    desc: "Note".into(),
                    action: "query:note search ".into(),
                    args: None,
                }];
            }
            Vec::new()
        }

        fn name(&self) -> &str {
            "exact-filter-plugin"
        }

        fn description(&self) -> &str {
            "Exact filter test plugin"
        }

        fn capabilities(&self) -> &[&str] {
            &[]
        }

        fn query_prefixes(&self) -> &[&str] {
            &["note"]
        }
    }

    #[test]
    fn file_search_dialog_counts_as_any_panel_open() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);

        app.file_search_dialog.open = true;

        assert!(app.any_panel_open());
    }

    #[test]
    fn qr_panel_lifecycle_preserves_source_and_deduplicates_stack() {
        use crate::commands::DialogCommandHost;
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let source = "  caf\u{e9}\n\u{1f512} ";
        app.open_qr_dialog(Some(source));
        app.update_panel_stack();
        app.ensure_open(Panel::QrDialog);
        app.update_panel_stack();
        assert!(app.any_panel_open());
        assert_eq!(app.qr_dialog.source, source);
        assert_eq!(
            app.panel_stack
                .iter()
                .filter(|p| **p == Panel::QrDialog)
                .count(),
            1
        );
        app.qr_dialog.feedback = Some(Ok("stale".into()));
        assert!(app.close_front_dialog());
        assert!(app.qr_dialog.feedback.is_none());
        assert!(!app.qr_dialog.focus_source);
        assert!(!app.is_panel_open(Panel::QrDialog));
        assert!(!app.panel_stack.contains(&Panel::QrDialog));
        app.ensure_open(Panel::QrDialog);
        app.update_panel_stack();
        app.force_close_panel(Panel::QrDialog);
        assert!(!app.qr_dialog.open);
        assert!(!app.panel_stack.contains(&Panel::QrDialog));
        assert_eq!(app.qr_dialog.source, source);
        app.open_qr_dialog(None);
        assert!(app.qr_dialog.source.is_empty());
        assert_eq!(
            app.qr_dialog.error_correction,
            crate::qr::QrErrorCorrection::Medium
        );
    }

    #[test]
    fn regex_tester_panel_lifecycle_preserves_draft_and_deduplicates_stack() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.regex_tester_dialog.session.draft.pattern = "saved session".into();
        app.regex_tester_dialog.session.draft.test_text = "sample".into();
        let draft = app.regex_tester_dialog.session.draft.clone();
        app.ensure_open(Panel::RegexTesterDialog);
        app.update_panel_stack();
        app.ensure_open(Panel::RegexTesterDialog);
        app.update_panel_stack();
        assert!(app.any_panel_open());
        assert_eq!(
            app.panel_stack
                .iter()
                .filter(|panel| **panel == Panel::RegexTesterDialog)
                .count(),
            1
        );
        assert!(app.close_front_dialog());
        assert!(!app.is_panel_open(Panel::RegexTesterDialog));
        assert!(!app.panel_stack.contains(&Panel::RegexTesterDialog));
        app.ensure_open(Panel::RegexTesterDialog);
        app.update_panel_stack();
        app.force_close_panel(Panel::RegexTesterDialog);
        assert!(!app.regex_tester_dialog.open);
        assert!(!app.panel_stack.contains(&Panel::RegexTesterDialog));
        assert_eq!(app.regex_tester_dialog.session.draft, draft);
    }

    #[test]
    fn m7f_xml_toolkit_panel_open_close_and_force_close_share_the_dialog_lifecycle() {
        let ctx = egui::Context::default();
        let directory = tempdir().unwrap();
        let mut settings = Settings::default();
        settings.hotkey = None;
        settings.quit_hotkey = None;
        settings.help_hotkey = None;
        settings.enabled_plugins = Some(std::collections::HashSet::new());
        settings.dashboard.enabled = false;
        let mut app = LauncherApp::new(
            &ctx,
            Arc::new(Vec::new()),
            0,
            PluginManager::new_inert_for_test(),
            directory
                .path()
                .join("actions.json")
                .to_string_lossy()
                .into_owned(),
            directory
                .path()
                .join("settings.json")
                .to_string_lossy()
                .into_owned(),
            settings,
            None,
            None,
            None,
            None,
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        );
        app.ensure_open(Panel::XmlToolkitDialog);
        app.update_panel_stack();
        app.ensure_open(Panel::XmlToolkitDialog);
        app.update_panel_stack();

        assert!(app.is_panel_open(Panel::XmlToolkitDialog));
        assert!(app.xml_toolkit_dialog.needs_viewport_bootstrap());
        assert_eq!(
            app.panel_stack
                .iter()
                .filter(|panel| **panel == Panel::XmlToolkitDialog)
                .count(),
            1
        );
        assert!(app.close_front_dialog());
        assert!(!app.is_panel_open(Panel::XmlToolkitDialog));
        assert!(!app.panel_stack.contains(&Panel::XmlToolkitDialog));

        app.ensure_open(Panel::XmlToolkitDialog);
        app.update_panel_stack();
        app.force_close_panel(Panel::XmlToolkitDialog);
        assert!(!app.is_panel_open(Panel::XmlToolkitDialog));
        assert!(!app.panel_stack.contains(&Panel::XmlToolkitDialog));
    }

    #[test]
    fn m8c_batch_rename_panel_open_close_and_reopen_do_not_start_a_scan() {
        let ctx = egui::Context::default();
        let directory = tempdir().unwrap();
        let mut settings = Settings::default();
        settings.hotkey = None;
        settings.quit_hotkey = None;
        settings.help_hotkey = None;
        settings.enabled_plugins = Some(std::collections::HashSet::new());
        settings.dashboard.enabled = false;
        let mut app = LauncherApp::new(
            &ctx,
            Arc::new(Vec::new()),
            0,
            PluginManager::new_inert_for_test(),
            directory
                .path()
                .join("actions.json")
                .to_string_lossy()
                .into_owned(),
            directory
                .path()
                .join("settings.json")
                .to_string_lossy()
                .into_owned(),
            settings,
            None,
            None,
            None,
            None,
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        );

        let open_action = crate::actions::Action {
            label: "Open Batch Renamer".to_owned(),
            desc: "Read-only preview".to_owned(),
            action: "rename:open".to_owned(),
            args: None,
        };
        let invocation = crate::commands::parse_command(
            open_action,
            None,
            crate::commands::ActivationSource::Dashboard,
        )
        .unwrap();
        crate::commands::CommandBus
            .dispatch(&invocation, &mut app)
            .unwrap();
        app.update_panel_stack();
        app.ensure_open(Panel::BatchRenameDialog);
        app.update_panel_stack();
        assert!(app.batch_rename_dialog.open);
        assert!(app.batch_rename_dialog.root_path().is_none());
        assert!(!app.batch_rename_dialog.has_pending_scan());
        assert!(app.batch_rename_coordinator.is_none());
        assert_eq!(
            app.panel_stack
                .iter()
                .filter(|panel| **panel == Panel::BatchRenameDialog)
                .count(),
            1
        );

        let retained_root = directory.path().join("explicit-root");
        app.batch_rename_dialog
            .fail_to_start_scan(retained_root.clone(), "fixture only".to_owned());
        assert!(app.close_front_dialog());
        assert!(!app.is_panel_open(Panel::BatchRenameDialog));
        assert!(!app.panel_stack.contains(&Panel::BatchRenameDialog));
        assert_eq!(
            app.batch_rename_dialog.root_path(),
            Some(retained_root.as_path())
        );

        app.ensure_open(Panel::BatchRenameDialog);
        app.update_panel_stack();
        assert_eq!(
            app.batch_rename_dialog.root_path(),
            Some(retained_root.as_path())
        );
        assert!(app.batch_rename_coordinator.is_none());
        app.force_close_panel(Panel::BatchRenameDialog);
        assert!(!app.batch_rename_dialog.open);
        assert!(!app.panel_stack.contains(&Panel::BatchRenameDialog));
        app.shutdown_batch_rename_scanner();
    }

    #[test]
    fn m9d_search_replace_command_opens_without_root_or_implicit_search() {
        struct NotesDirectoryOverride(Option<std::ffi::OsString>);
        impl NotesDirectoryOverride {
            fn use_fixture(path: &Path) -> Self {
                let previous = std::env::var_os("ML_NOTES_DIR");
                unsafe { std::env::set_var("ML_NOTES_DIR", path) };
                Self(previous)
            }
        }
        impl Drop for NotesDirectoryOverride {
            fn drop(&mut self) {
                if let Some(previous) = self.0.take() {
                    unsafe { std::env::set_var("ML_NOTES_DIR", previous) };
                } else {
                    unsafe { std::env::remove_var("ML_NOTES_DIR") };
                }
            }
        }

        let _guard = TEST_MUTEX.lock().unwrap();
        let fixture_parent = PathBuf::from("target/productivity_expansion");
        std::fs::create_dir_all(&fixture_parent).unwrap();
        let fixture = tempfile::Builder::new()
            .prefix("replace_dialog_app_")
            .tempdir_in(&fixture_parent)
            .unwrap();
        let app_directory = fixture.path().join("app-data");
        let notes_directory = fixture.path().join("notes");
        std::fs::create_dir_all(&app_directory).unwrap();
        std::fs::create_dir_all(&notes_directory).unwrap();
        let _notes_directory = NotesDirectoryOverride::use_fixture(&notes_directory);

        let ctx = egui::Context::default();
        let mut settings = Settings::default();
        settings.hotkey = None;
        settings.quit_hotkey = None;
        settings.help_hotkey = None;
        settings.enabled_plugins = Some(std::collections::HashSet::new());
        settings.dashboard.enabled = false;
        settings.dashboard.config_path = Some(
            app_directory
                .join("dashboard.json")
                .to_string_lossy()
                .into_owned(),
        );
        let mut app = LauncherApp::new(
            &ctx,
            Arc::new(Vec::new()),
            0,
            PluginManager::new_inert_for_test_at(&app_directory),
            app_directory
                .join("actions.json")
                .to_string_lossy()
                .into_owned(),
            app_directory
                .join("settings.json")
                .to_string_lossy()
                .into_owned(),
            settings,
            None,
            None,
            None,
            None,
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        );
        app.watchers.clear();
        app.clipboard_modify_watcher = None;

        let action = crate::actions::Action {
            label: "Open Search and Replace".into(),
            desc: String::new(),
            action: "replace:open".into(),
            args: None,
        };
        let invocation = crate::commands::parse_command(
            action,
            None,
            crate::commands::ActivationSource::Dashboard,
        )
        .unwrap();
        crate::commands::CommandBus
            .dispatch(&invocation, &mut app)
            .unwrap();
        app.update_panel_stack();
        assert!(app.search_replace_dialog.open);
        assert!(app.search_replace_dialog.explicit_root().is_none());
        assert!(!app.search_replace_dialog.has_pending_search());
        assert!(app.replace_search_coordinator.is_some());
        assert!(app.replace_executor_worker.is_some());
        assert_eq!(
            app.panel_stack
                .iter()
                .filter(|panel| **panel == Panel::SearchReplaceDialog)
                .count(),
            1
        );

        assert!(app.close_front_dialog());
        assert!(!app.is_panel_open(Panel::SearchReplaceDialog));
        assert!(app.replace_executor_worker.is_some());
        app.ensure_open(Panel::SearchReplaceDialog);
        app.update_panel_stack();
        assert!(app.search_replace_dialog.explicit_root().is_none());
        assert!(!app.search_replace_dialog.has_pending_search());
        app.force_close_panel(Panel::SearchReplaceDialog);
        app.shutdown_search_replace_runtime();
        assert!(app.replace_search_coordinator.is_none());
        assert!(app.replace_preview_worker.is_none());
        assert!(app.replace_executor_worker.is_none());
    }

    #[test]
    #[cfg(windows)]
    fn m8c_closed_batch_rename_scan_retains_its_owner_until_hidden_poll_and_shutdown() {
        struct NotesDirectoryOverride(Option<std::ffi::OsString>);
        impl NotesDirectoryOverride {
            fn use_fixture(path: &Path) -> Self {
                let previous = std::env::var_os("ML_NOTES_DIR");
                unsafe { std::env::set_var("ML_NOTES_DIR", path) };
                Self(previous)
            }
        }
        impl Drop for NotesDirectoryOverride {
            fn drop(&mut self) {
                if let Some(previous) = self.0.take() {
                    unsafe { std::env::set_var("ML_NOTES_DIR", previous) };
                } else {
                    unsafe { std::env::remove_var("ML_NOTES_DIR") };
                }
            }
        }

        let _guard = TEST_MUTEX.lock().unwrap();
        let fixture_parent = PathBuf::from("target/productivity_expansion");
        std::fs::create_dir_all(&fixture_parent).unwrap();
        let fixture = tempfile::Builder::new()
            .prefix("batch_rename_app_")
            .tempdir_in(&fixture_parent)
            .unwrap();
        let root = fixture.path().join("selected-root");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("keep.txt"), "unchanged").unwrap();

        let ctx = egui::Context::default();
        let app_directory = fixture.path().join("app-data");
        std::fs::create_dir_all(&app_directory).unwrap();
        let _notes_directory = NotesDirectoryOverride::use_fixture(&fixture.path().join("notes"));
        let mut settings = Settings::default();
        settings.hotkey = None;
        settings.quit_hotkey = None;
        settings.help_hotkey = None;
        settings.enabled_plugins = Some(std::collections::HashSet::new());
        settings.dashboard.enabled = false;
        settings.dashboard.config_path = Some(
            app_directory
                .join("dashboard.json")
                .to_string_lossy()
                .into_owned(),
        );
        let mut app = LauncherApp::new(
            &ctx,
            Arc::new(Vec::new()),
            0,
            PluginManager::new_inert_for_test_at(&app_directory),
            app_directory
                .join("actions.json")
                .to_string_lossy()
                .into_owned(),
            app_directory
                .join("settings.json")
                .to_string_lossy()
                .into_owned(),
            settings,
            None,
            None,
            None,
            None,
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        );
        app.watchers.clear();
        app.clipboard_modify_watcher = None;

        app.ensure_open(Panel::BatchRenameDialog);
        app.update_panel_stack();
        app.handle_batch_rename_ui_command(BatchRenameUiCommand::SelectRoot(root.clone()));
        assert!(app.batch_rename_dialog.has_pending_scan());
        assert!(app.batch_rename_coordinator.is_some());
        assert!(app.close_front_dialog());
        assert!(app.batch_rename_coordinator.is_some());

        app.request_launcher_state(Some(false), None);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while app.batch_rename_dialog.has_pending_scan() && std::time::Instant::now() < deadline {
            app.poll_batch_rename_runtime(&ctx);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(!app.batch_rename_dialog.has_pending_scan());
        assert!(!app.batch_rename_dialog.has_displayed_scan_result());
        assert!(!app.batch_rename_dialog.has_accepted_snapshot());
        assert!(app.batch_rename_coordinator.is_some());

        app.ensure_open(Panel::BatchRenameDialog);
        app.update_panel_stack();
        assert_eq!(app.batch_rename_dialog.root_path(), Some(root.as_path()));
        assert!(!app.batch_rename_dialog.has_pending_scan());
        app.force_close_panel(Panel::BatchRenameDialog);
        app.shutdown_batch_rename_scanner();
        assert!(app.batch_rename_coordinator.is_none());
        assert!(!app.batch_rename_dialog.has_pending_scan());
        assert_eq!(
            std::fs::read_to_string(root.join("keep.txt")).unwrap(),
            "unchanged"
        );
    }

    #[test]
    fn theme_settings_dialog_apply_save_cancel_and_close_routes_restore_committed_theme() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let ctx = egui::Context::default();
        let directory = tempdir().unwrap();
        let settings_path = directory.path().join("settings.json");
        let mut persisted = Settings::default();
        persisted.theme.mode = crate::settings::ThemeMode::Dark;
        persisted.save(settings_path.to_str().unwrap()).unwrap();

        let mut app = new_app(&ctx);
        app.settings_path = settings_path.to_string_lossy().into_owned();
        app.open_theme_settings_dialog();
        assert_eq!(ctx.theme(), egui::Theme::Dark);

        app.theme_settings_dialog.draft.mode = crate::settings::ThemeMode::Light;
        app.theme_settings_dialog.dirty = true;
        crate::gui::theme::publish_preview(&ctx, &app.theme_settings_dialog.draft);
        app.focus_panel(Panel::ThemeSettingsDialog);
        assert_eq!(
            app.theme_settings_dialog.draft.mode,
            crate::settings::ThemeMode::Light,
            "refocusing an open dialog keeps its current draft"
        );
        assert_eq!(ctx.theme(), egui::Theme::Light);

        let mut state = std::mem::take(&mut app.theme_settings_dialog);
        state
            .save_to_path(&ctx, &app.settings_path)
            .expect("Apply commits the current draft");
        app.finish_theme_settings_dialog_ui(true, state);
        assert!(
            app.is_theme_settings_dialog_open(),
            "Apply keeps the editor open"
        );
        assert!(!app.theme_settings_dialog.dirty);
        assert!(app.theme_settings_dialog.can_edit());

        app.theme_settings_dialog.draft.mode = crate::settings::ThemeMode::Custom;
        app.theme_settings_dialog.dirty = true;
        crate::gui::theme::publish_preview(&ctx, &app.theme_settings_dialog.draft);
        let state = std::mem::take(&mut app.theme_settings_dialog);
        app.finish_theme_settings_dialog_ui(false, state);
        assert!(!app.is_theme_settings_dialog_open());
        assert_eq!(
            ctx.theme(),
            egui::Theme::Light,
            "Cancel keeps the applied theme"
        );
        assert_eq!(
            Settings::load(&app.settings_path).unwrap().theme.mode,
            crate::settings::ThemeMode::Light
        );

        app.focus_panel(Panel::ThemeSettingsDialog);
        app.theme_settings_dialog.draft.mode = crate::settings::ThemeMode::Dark;
        app.theme_settings_dialog.dirty = true;
        crate::gui::theme::publish_preview(&ctx, &app.theme_settings_dialog.draft);
        let mut state = std::mem::take(&mut app.theme_settings_dialog);
        state
            .save_to_path(&ctx, &app.settings_path)
            .expect("Save commits the current draft");
        app.finish_theme_settings_dialog_ui(false, state);
        assert!(
            !app.is_theme_settings_dialog_open(),
            "Save ends the session"
        );
        assert_eq!(ctx.theme(), egui::Theme::Dark);

        app.focus_panel(Panel::ThemeSettingsDialog);
        app.theme_settings_dialog.draft.mode = crate::settings::ThemeMode::Light;
        app.theme_settings_dialog.dirty = true;
        crate::gui::theme::publish_preview(&ctx, &app.theme_settings_dialog.draft);
        assert!(
            app.close_front_dialog(),
            "Escape/close-front cancels the session"
        );
        assert_eq!(ctx.theme(), egui::Theme::Dark);

        app.focus_panel(Panel::ThemeSettingsDialog);
        app.theme_settings_dialog.draft.mode = crate::settings::ThemeMode::Light;
        app.theme_settings_dialog.dirty = true;
        crate::gui::theme::publish_preview(&ctx, &app.theme_settings_dialog.draft);
        app.force_close_panel(Panel::ThemeSettingsDialog);
        assert!(
            !app.is_theme_settings_dialog_open(),
            "forced close cancels the session"
        );
        assert_eq!(ctx.theme(), egui::Theme::Dark);

        app.focus_panel(Panel::ThemeSettingsDialog);
        app.theme_settings_dialog.draft.mode = crate::settings::ThemeMode::Light;
        app.theme_settings_dialog.dirty = true;
        crate::gui::theme::publish_preview(&ctx, &app.theme_settings_dialog.draft);
        app.close_theme_settings_dialog();
        app.close_theme_settings_dialog();
        assert!(!app.is_theme_settings_dialog_open());
        assert!(!app.theme_settings_dialog.can_edit());
        assert_eq!(
            ctx.theme(),
            egui::Theme::Dark,
            "explicit close is idempotent"
        );
    }

    #[test]
    fn theme_settings_dialog_pinned_close_refusal_retains_draft_and_preview() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let ctx = egui::Context::default();
        let directory = tempdir().unwrap();
        let settings_path = directory.path().join("settings.json");
        let mut persisted = Settings::default();
        persisted.theme.mode = crate::settings::ThemeMode::Dark;
        persisted.save(settings_path.to_str().unwrap()).unwrap();

        let mut app = new_app(&ctx);
        app.settings_path = settings_path.to_string_lossy().into_owned();
        app.open_theme_settings_dialog();
        app.theme_settings_dialog.draft.mode = crate::settings::ThemeMode::Light;
        app.theme_settings_dialog.dirty = true;
        crate::gui::theme::publish_preview(&ctx, &app.theme_settings_dialog.draft);
        app.pinned_panels.push(Panel::ThemeSettingsDialog);
        app.focus_panel(Panel::ThemeSettingsDialog);

        assert!(!app.close_front_dialog(), "pinned close-front is refused");
        let state = std::mem::take(&mut app.theme_settings_dialog);
        app.finish_theme_settings_dialog_ui(false, state);
        assert!(
            app.is_theme_settings_dialog_open(),
            "pinned window close is refused"
        );
        assert!(app.theme_settings_dialog.can_edit());
        assert!(app.theme_settings_dialog.dirty);
        assert_eq!(
            app.theme_settings_dialog.draft.mode,
            crate::settings::ThemeMode::Light
        );
        assert_eq!(ctx.theme(), egui::Theme::Light);
        assert!(app.panel_stack.contains(&Panel::ThemeSettingsDialog));
        app.close_theme_settings_dialog();
        assert!(
            app.is_theme_settings_dialog_open(),
            "explicit pinned close is refused"
        );
        assert_eq!(ctx.theme(), egui::Theme::Light);

        app.toggle_pin(Panel::ThemeSettingsDialog);
        assert!(!app.pinned_panels.contains(&Panel::ThemeSettingsDialog));
        assert!(!app.is_theme_settings_dialog_open());
        assert!(!app.theme_settings_dialog.can_edit());
        assert_eq!(
            ctx.theme(),
            egui::Theme::Dark,
            "unpin force-close cancels preview"
        );
    }

    #[test]
    fn tracked_openable_panels_count_as_any_panel_open() {
        let ctx = egui::Context::default();
        let openable_panels = LauncherApp::TRACKED_PANELS.iter().copied().filter(|panel| {
            !matches!(
                panel,
                Panel::NotePanel | Panel::ImagePanel | Panel::ScreenshotEditor | Panel::CropDialog
            )
        });

        for panel in openable_panels {
            let mut app = new_app(&ctx);
            if panel == Panel::VolumeDialog {
                // An already-open synthetic snapshot keeps this fixture off audio hardware.
                app.volume_dialog.open = true;
            }
            app.ensure_open(panel);
            assert!(
                app.any_panel_open(),
                "expected {panel:?} to count as an open panel"
            );
        }
    }

    #[derive(Default)]
    struct HostCaptureLog {
        begins: Vec<mkmacro_dialog::visual_overlay::RectanglePurpose>,
        events: Vec<mkmacro_dialog::visual_capture_workflow::SelectionEvent>,
        captures: Vec<crate::mkmacro::ScreenRect>,
        writes: usize,
    }
    struct HostOverlay(Arc<Mutex<HostCaptureLog>>);
    impl mkmacro_dialog::visual_capture_workflow::RectangleOverlay for HostOverlay {
        fn begin(
            &mut self,
            purpose: mkmacro_dialog::visual_overlay::RectanglePurpose,
        ) -> Result<u64, String> {
            self.0.lock().unwrap().begins.push(purpose);
            Ok(101)
        }
        fn poll(&mut self) -> mkmacro_dialog::visual_capture_workflow::SelectionEvent {
            let mut log = self.0.lock().unwrap();
            if log.events.is_empty() {
                mkmacro_dialog::visual_capture_workflow::SelectionEvent::Pending
            } else {
                log.events.remove(0)
            }
        }
        fn cancel(&mut self, _expected_operation_id: u64) {}
    }
    struct SharedHostOverlay {
        controller: mkmacro_dialog::visual_capture_workflow::SharedVisualOverlayController,
        operation: Option<u64>,
    }
    impl mkmacro_dialog::visual_capture_workflow::RectangleOverlay for SharedHostOverlay {
        fn begin(
            &mut self,
            purpose: mkmacro_dialog::visual_overlay::RectanglePurpose,
        ) -> Result<u64, String> {
            let id = self
                .controller
                .begin_rectangle_pick(purpose, crate::mkmacro::ScreenRect::new(0, 0, 100, 100));
            self.operation = Some(id);
            Ok(id)
        }
        fn poll(&mut self) -> mkmacro_dialog::visual_capture_workflow::SelectionEvent {
            mkmacro_dialog::visual_capture_workflow::SelectionEvent::Pending
        }
        fn cancel(&mut self, expected: u64) {
            if self.operation.take() == Some(expected) {
                self.controller.cancel_operation(expected);
            }
        }
    }
    struct HostCapture(Arc<Mutex<HostCaptureLog>>);
    impl mkmacro_dialog::visual_capture_workflow::CaptureAdapter for HostCapture {
        fn capture_rect(&mut self, rect: crate::mkmacro::ScreenRect) -> Result<RgbaImage, String> {
            self.0.lock().unwrap().captures.push(rect);
            Ok(RgbaImage::new(rect.width, rect.height))
        }
    }
    struct HostAssets(Arc<Mutex<HostCaptureLog>>);
    impl mkmacro_dialog::visual_capture_workflow::AssetStoreAdapter for HostAssets {
        fn write_captured_image(
            &mut self,
            _: &RgbaImage,
            filename: MkImageRef,
            _: ImageImportChoice,
        ) -> Result<ImageImportResult, String> {
            self.0.lock().unwrap().writes += 1;
            Ok(ImageImportResult::Imported(filename))
        }
    }
    fn host_capture_dependencies(log: Arc<Mutex<HostCaptureLog>>) -> VisualCaptureDependencies {
        VisualCaptureDependencies {
            overlay: Box::new(HostOverlay(log.clone())),
            capture: Box::new(HostCapture(log.clone())),
            assets: Box::new(HostAssets(log)),
        }
    }

    fn host_image_action() -> MkAction {
        MkAction::ImageClick(MkImagePayload {
            image: MkImageRef::from_filename("7.png"),
            region: SearchRegion::Desktop,
            wait: MkWaitOptions::default(),
            tolerance: 0,
            alpha: AlphaPolicy::Ignore,
            return_point: ReturnPoint::TopLeft,
            not_found_policy: MkImageNotFoundPolicy::Fail,
            outputs: MkImageOutputs::default(),
        })
    }

    #[test]
    fn installed_visual_capture_issues_rectangle_request_immediately() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let log = Arc::new(Mutex::new(HostCaptureLog::default()));
        install_visual_capture(
            &mut app.mkmacro_dialog,
            host_capture_dependencies(log.clone()),
        );
        app.mkmacro_dialog
            .action_editor
            .begin_new(host_image_action());
        app.mkmacro_dialog
            .action_editor
            .request_rectangle_selection(
                9,
                mkmacro_dialog::visual_overlay::RectanglePurpose::SearchRegion,
                mkmacro_dialog::action_editor::VisualRegionDestination::ImageActionSearchRegion,
            )
            .unwrap();
        assert_eq!(
            log.lock().unwrap().begins,
            [mkmacro_dialog::visual_overlay::RectanglePurpose::SearchRegion]
        );
        assert!(
            app.mkmacro_dialog
                .action_editor
                .visual_capture
                .as_ref()
                .unwrap()
                .active()
        );
    }

    #[test]
    fn installed_workflow_and_action_editor_reuse_dialog_overlay_worker() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let fixture =
            mkmacro_dialog::visual_capture_workflow::SharedVisualOverlayController::test_fixture();
        app.mkmacro_dialog.visual_overlay = fixture.controller.clone();
        app.mkmacro_dialog.action_editor = mkmacro_dialog::action_editor::ActionEditorState::new(
            app.mkmacro_dialog.visual_overlay_controller(),
        );
        let log = Arc::new(Mutex::new(HostCaptureLog::default()));
        install_visual_capture(
            &mut app.mkmacro_dialog,
            VisualCaptureDependencies {
                overlay: Box::new(SharedHostOverlay {
                    controller: fixture.controller.clone(),
                    operation: None,
                }),
                capture: Box::new(HostCapture(log.clone())),
                assets: Box::new(HostAssets(log)),
            },
        );

        let editor_id = app
            .mkmacro_dialog
            .action_editor
            .visual_overlay
            .preview_rectangle(crate::mkmacro::ScreenRect::new(1, 1, 5, 5));
        fixture.observer.wait_for_commands(1);
        app.mkmacro_dialog
            .action_editor
            .begin_new(host_image_action());
        app.mkmacro_dialog
            .action_editor
            .request_rectangle_selection(
                9,
                mkmacro_dialog::visual_overlay::RectanglePurpose::SearchRegion,
                mkmacro_dialog::action_editor::VisualRegionDestination::ImageActionSearchRegion,
            )
            .unwrap();
        fixture.observer.wait_for_commands(3);
        let commands = fixture.observer.commands.lock().unwrap();
        assert!(
            matches!(commands[0], mkmacro_dialog::visual_overlay::VisualOverlayCommand::PreviewRectangle { operation_id, .. } if operation_id == editor_id)
        );
        assert!(matches!(
            commands[1],
            mkmacro_dialog::visual_overlay::VisualOverlayCommand::Cancel {
                expected_operation_id: Some(operation_id)
            } if operation_id == editor_id
        ));
        assert!(matches!(
            commands[2],
            mkmacro_dialog::visual_overlay::VisualOverlayCommand::BeginRectanglePick { .. }
        ));
        assert_eq!(fixture.observer.starts.load(Ordering::SeqCst), 1);
        assert_eq!(fixture.observer.worker_ids.lock().unwrap().len(), 1);
    }

    #[test]
    fn capture_authoring_never_changes_launcher_or_dialog_host_state() {
        use mkmacro_dialog::visual_capture_workflow::SelectionEvent;
        use mkmacro_dialog::visual_overlay::RectanglePurpose;
        for outcome in ["success", "cancel", "failure"] {
            let ctx = egui::Context::default();
            let mut app = new_app(&ctx);
            app.mkmacro_dialog.open = true;
            let log = Arc::new(Mutex::new(HostCaptureLog::default()));
            install_visual_capture(
                &mut app.mkmacro_dialog,
                host_capture_dependencies(log.clone()),
            );
            app.mkmacro_dialog
                .action_editor
                .begin_new(host_image_action());
            let host_before = (
                app.visible_flag.load(Ordering::SeqCst),
                app.last_visible,
                app.mkmacro_dialog.open,
                app.panel_states.mkmacro_dialog,
            );
            app.mkmacro_dialog
                .action_editor
                .request_rectangle_selection(
                    9,
                    RectanglePurpose::ReferenceImageCapture,
                    mkmacro_dialog::action_editor::VisualRegionDestination::ImageActionReferenceAsset,
                )
                .unwrap();
            assert_eq!(
                (
                    app.visible_flag.load(Ordering::SeqCst),
                    app.last_visible,
                    app.mkmacro_dialog.open,
                    app.panel_states.mkmacro_dialog
                ),
                host_before
            );
            log.lock().unwrap().events.push(match outcome {
                "success" => SelectionEvent::Confirmed {
                    operation_id: 101,
                    rect: crate::mkmacro::ScreenRect::new(-20, 30, 4, 5),
                },
                "cancel" => SelectionEvent::Cancelled { operation_id: 101 },
                _ => SelectionEvent::Failed {
                    operation_id: 101,
                    message: "overlay failed".into(),
                },
            });
            for _ in 0..4 {
                app.mkmacro_dialog
                    .action_editor
                    .tick_visual_capture(Some(9));
            }
            let effects = {
                let log = log.lock().unwrap();
                (log.begins.clone(), log.captures.clone(), log.writes)
            };
            for _ in 0..3 {
                app.mkmacro_dialog
                    .action_editor
                    .tick_visual_capture(Some(9));
            }
            assert_eq!(
                (
                    app.visible_flag.load(Ordering::SeqCst),
                    app.last_visible,
                    app.mkmacro_dialog.open,
                    app.panel_states.mkmacro_dialog
                ),
                host_before,
                "{outcome}"
            );
            let after = log.lock().unwrap();
            assert_eq!(
                (after.begins.clone(), after.captures.clone(), after.writes),
                effects,
                "terminal ticks must have no effects for {outcome}"
            );
        }
        let render = include_str!("render.rs");
        assert!(
            !render
                .lines()
                .any(|line| line.contains("ViewportCommand::Visible")
                    && line.to_ascii_lowercase().contains("capture")),
            "render host must not contain a capture-specific visibility command"
        );
    }

    #[test]
    fn inline_error_visibility_respects_setting() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.report_error("test.inline_error_visibility", "boom");
        app.show_inline_errors = true;
        assert!(app.should_render_inline_error());

        app.show_inline_errors = false;
        assert!(!app.should_render_inline_error());
    }

    #[test]
    fn report_error_respects_inline_and_toast_settings() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let temp = tempdir().unwrap();
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp.path()).unwrap();

        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.enable_toasts = true;
        app.show_error_toasts = true;
        app.show_inline_errors = true;

        app.report_error("test.report_error", "first");
        assert_eq!(app.error.as_deref(), Some("first"));
        let log = std::fs::read_to_string(TOAST_LOG_FILE).unwrap();
        assert_eq!(log.matches("[error:test.report_error] first").count(), 1);
        assert_eq!(log.matches("first").count(), 2);

        app.error = None;
        app.show_inline_errors = false;
        app.show_error_toasts = false;
        app.report_error("test.report_error", "second");
        assert!(app.error.is_none());
        let log = std::fs::read_to_string(TOAST_LOG_FILE).unwrap();
        assert_eq!(log.matches("[error:test.report_error] second").count(), 1);
        assert_eq!(log.matches("second").count(), 1);

        std::env::set_current_dir(original_dir).unwrap();
    }

    #[test]
    fn report_error_records_when_ui_is_disabled() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let temp = tempdir().unwrap();
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp.path()).unwrap();

        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.enable_toasts = false;
        app.show_error_toasts = false;
        app.show_inline_errors = false;

        app.report_error("test.recording", "record me");

        assert!(app.error.is_none());
        let log = std::fs::read_to_string(TOAST_LOG_FILE).unwrap();
        assert!(log.contains("[error:test.recording] record me"));
        assert_eq!(log.matches("record me").count(), 1);

        std::env::set_current_dir(original_dir).unwrap();
    }

    #[test]
    fn recycle_error_path_uses_unified_reporting() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let temp = tempdir().unwrap();
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp.path()).unwrap();

        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.enable_toasts = false;
        app.show_error_toasts = false;
        app.show_inline_errors = false;

        send_event(WatchEvent::Recycle(Err("boom".into())));
        app.process_watch_events();

        assert!(app.error.is_none());
        let log = std::fs::read_to_string(TOAST_LOG_FILE).unwrap();
        assert!(log.contains("[error:recycle.empty] Failed to empty recycle bin: boom"));
        assert_eq!(log.matches("Failed to empty recycle bin: boom").count(), 1);

        std::env::set_current_dir(original_dir).unwrap();
    }

    #[test]
    fn action_search_remains_case_insensitive_with_cached_aliases() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.fuzzy_weight = 0.0;
        app.actions = Arc::new(vec![Action {
            label: "Sample Action".into(),
            desc: "Demo description".into(),
            action: "demo:action".into(),
            args: None,
        }]);
        app.update_action_cache();
        app.bookmark_aliases
            .insert("demo:action".into(), Some("MiXeDCaSe Alias".into()));
        app.bookmark_aliases_lc
            .insert("demo:action".into(), Some("mixedcase alias".into()));

        app.query = "app SAMPLE".into();
        app.search();
        assert!(app.results.iter().any(|a| a.action == "demo:action"));

        app.query = "app MIXEDCASE".into();
        app.search();
        assert!(app.results.iter().any(|a| a.action == "demo:action"));
    }

    #[test]
    fn exact_display_match_uses_pre_normalized_query_substring() {
        let cached = CachedSearchEntry {
            label_lc: "testingeve123".into(),
            desc_lc: String::new(),
            action_lc: String::new(),
        };
        assert!(LauncherApp::matches_exact_display_text(
            &cached,
            &"Eve".to_lowercase()
        ));
        assert!(LauncherApp::matches_exact_display_text(&cached, "eve"));
        assert!(!LauncherApp::matches_exact_display_text(&cached, "night"));
    }

    #[test]
    fn action_and_command_search_cache_is_normalized() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.actions = Arc::new(vec![Action {
            label: "MiXeD Label".into(),
            desc: "MiXeD Desc".into(),
            action: "Action:ID".into(),
            args: None,
        }]);
        app.update_action_cache();

        assert_eq!(app.action_cache.len(), 1);
        assert_eq!(app.action_cache[0].label_lc, "mixed label");
        assert_eq!(app.action_cache[0].desc_lc, "mixed desc");
        assert_eq!(app.action_cache[0].action_lc, "action:id");

        app.plugins.register(Box::new(ExactFilterPlugin));
        app.update_command_cache();
        assert_eq!(app.command_cache.len(), app.command_search_cache.len());
        for (action, cached) in app
            .command_cache
            .iter()
            .zip(app.command_search_cache.iter())
        {
            assert_eq!(cached.label_lc, action.label.to_lowercase());
            assert_eq!(cached.desc_lc, action.desc.to_lowercase());
            assert_eq!(cached.action_lc, action.action.to_lowercase());
        }
    }

    #[test]
    fn match_exact_overrides_fuzzy_scoring() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.fuzzy_weight = 1.0;
        app.actions = Arc::new(vec![Action {
            label: "testing123".into(),
            desc: "Demo description".into(),
            action: "demo:action".into(),
            args: None,
        }]);
        app.update_action_cache();

        app.query = "app tstng123".into();
        app.match_exact = false;
        app.search();
        assert!(app.results.iter().any(|a| a.action == "demo:action"));

        app.query = "app tstng123".into();
        app.match_exact = true;
        app.last_results_valid = false;
        app.search();
        assert!(!app.results.iter().any(|a| a.action == "demo:action"));
    }

    #[test]
    fn exact_mode_keeps_plugin_resolved_results_but_filters_query_suggestions() {
        let ctx = egui::Context::default();
        let workspace = crate::performance::track_c_workloads::TrackCWorkspace::new();
        let mut settings = Settings::default();
        settings.hotkey = None;
        settings.quit_hotkey = None;
        settings.help_hotkey = None;
        settings.dashboard.enabled = false;
        settings.enable_toasts = false;
        settings.show_inline_errors = false;
        settings.show_error_toasts = false;
        let mut app = LauncherApp::new(
            &ctx,
            Arc::new(Vec::new()),
            0,
            PluginManager::new_inert_for_test(),
            workspace
                .root()
                .join("actions.json")
                .to_string_lossy()
                .into_owned(),
            workspace
                .root()
                .join("settings.json")
                .to_string_lossy()
                .into_owned(),
            settings,
            None,
            None,
            Some(std::collections::HashSet::from([
                "exact-filter-plugin".into()
            ])),
            Some(HashMap::new()),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        );
        app.match_exact = true;
        app.plugins.register(Box::new(ExactFilterPlugin));

        app.query = "note today".into();
        app.search();
        assert!(
            app.results
                .iter()
                .any(|a| a.action == "note:new:2025-02-23")
        );

        app.query = "note search alpha".into();
        app.last_results_valid = false;
        app.search();
        assert!(app.results.iter().any(|a| a.action == "note:open:alpha"));

        app.query = "note zz".into();
        app.last_results_valid = false;
        app.search();
        assert!(app.results.is_empty());
        drop(app);
        drop(workspace);
    }

    #[test]
    fn update_paths_refreshes_command_search_cache_for_plugin_reload() {
        struct CommandPlugin;

        impl crate::plugin::Plugin for CommandPlugin {
            fn search(&self, _query: &str) -> Vec<Action> {
                Vec::new()
            }

            fn name(&self) -> &str {
                "command-plugin"
            }

            fn description(&self) -> &str {
                "command plugin"
            }

            fn capabilities(&self) -> &[&str] {
                &[]
            }

            fn commands(&self) -> Vec<Action> {
                vec![Action {
                    label: "PlUgIn Command".into(),
                    desc: "PlUgIn Desc".into(),
                    action: "Plugin:Command".into(),
                    args: None,
                }]
            }
        }

        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.plugins.register(Box::new(CommandPlugin));

        app.update_paths(
            None,                                                            // plugin_dirs
            crate::indexer::coordinator::IndexConfig::new(Vec::new(), None), // index_config
            None,                                                            // enabled_plugins
            None,                                                            // enabled_capabilities
            None,                                                            // offscreen_pos
            None,                                                            // enable_toasts
            None,                                                            // show_inline_errors
            None,                                                            // show_error_toasts
            None,                                                            // toast_duration
            None,                                                            // fuzzy_weight
            None,                                                            // usage_weight
            None,                                                            // match_exact
            None,                                                            // follow_mouse
            None,                                                            // static_enabled
            None,                                                            // static_pos
            None,                                                            // static_size
            None,                                                            // hide_after_run
            None, // clear_query_after_run
            None, // require_confirm_destructive
            None, // timer_refresh
            None, // disable_timer_updates
            None, // preserve_command
            None, // query_autocomplete
            None, // net_refresh
            None, // net_unit
            None, // screenshot_dir
            None, // screenshot_save_file
            None, // screenshot_use_editor
            None, // screenshot_auto_save
            None, // always_on_top
            None, // page_jump
            None, // note_settings
            None, // note_panel_default_size
            None, // note_save_on_close
            None, // note_confirm_discard_unsaved_changes
            None, // note_always_overwrite
            None, // note_images_as_links
            None, // note_show_details
            None, // note_more_limit
            None, // show_dashboard_diagnostics
        );

        assert_eq!(app.command_cache.len(), 1);
        assert_eq!(app.command_search_cache.len(), 1);
        assert_eq!(app.command_search_cache[0].label_lc, "plugin command");
        assert_eq!(app.command_search_cache[0].desc_lc, "plugin desc");
        assert_eq!(app.command_search_cache[0].action_lc, "plugin:command");
    }

    #[test]
    fn update_paths_refreshes_note_settings() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        assert!(app.note_settings.split_view_enabled);

        let mut note_settings = app.note_settings.clone();
        note_settings.split_view_enabled = false;

        app.update_paths(
            None,                                                            // plugin_dirs
            crate::indexer::coordinator::IndexConfig::new(Vec::new(), None), // index_config
            None,                                                            // enabled_plugins
            None,                                                            // enabled_capabilities
            None,                                                            // offscreen_pos
            None,                                                            // enable_toasts
            None,                                                            // show_inline_errors
            None,                                                            // show_error_toasts
            None,                                                            // toast_duration
            None,                                                            // fuzzy_weight
            None,                                                            // usage_weight
            None,                                                            // match_exact
            None,                                                            // follow_mouse
            None,                                                            // static_enabled
            None,                                                            // static_pos
            None,                                                            // static_size
            None,                                                            // hide_after_run
            None,                // clear_query_after_run
            None,                // require_confirm_destructive
            None,                // timer_refresh
            None,                // disable_timer_updates
            None,                // preserve_command
            None,                // query_autocomplete
            None,                // net_refresh
            None,                // net_unit
            None,                // screenshot_dir
            None,                // screenshot_save_file
            None,                // screenshot_use_editor
            None,                // screenshot_auto_save
            None,                // always_on_top
            None,                // page_jump
            Some(note_settings), // note_settings
            None,                // note_panel_default_size
            Some(false),         // note_save_on_close
            Some(false),         // note_confirm_discard_unsaved_changes
            None,                // note_always_overwrite
            None,                // note_images_as_links
            None,                // note_show_details
            None,                // note_more_limit
            None,                // show_dashboard_diagnostics
        );

        assert!(!app.note_settings.split_view_enabled);
        assert!(!app.note_save_on_close);
        assert!(!app.note_confirm_discard_unsaved_changes);
    }

    #[test]
    fn malformed_note_new_action_reports_error_and_search_recovers() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.actions = Arc::new(vec![Action {
            label: "Sample Action".into(),
            desc: "Demo".into(),
            action: "demo:action".into(),
            args: None,
        }]);
        app.update_action_cache();

        app.activate_action(
            Action {
                label: "Malformed note".into(),
                desc: "Note".into(),
                action: "note:new:2025-02-23:today:extra".into(),
                args: None,
            },
            None,
            ActivationSource::Enter,
        );

        assert!(
            app.error
                .as_ref()
                .is_some_and(|msg| msg.contains("Malformed note action"))
        );

        app.query = "app sample".into();
        app.last_results_valid = false;
        app.search();
        assert!(app.results.iter().any(|a| a.action == "demo:action"));
    }

    #[test]
    fn watch_events_refresh_alias_and_lowercase_alias_caches() {
        let _lock = TEST_MUTEX.lock().unwrap();
        let dir = tempdir().unwrap();
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();

        let folders_json = serde_json::json!([{
            "label": "Folder",
            "path": "/tmp/folder-one",
            "alias": "MiXeDFolder"
        }]);
        std::fs::write(
            crate::plugins::folders::FOLDERS_FILE,
            serde_json::to_string_pretty(&folders_json).unwrap(),
        )
        .unwrap();

        let bookmarks_json = serde_json::json!([{
            "url": "https://example.com",
            "alias": "MiXeDBookmark"
        }]);
        std::fs::write(
            crate::plugins::bookmarks::BOOKMARKS_FILE,
            serde_json::to_string_pretty(&bookmarks_json).unwrap(),
        )
        .unwrap();

        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);

        assert_eq!(
            app.folder_aliases_lc.get("/tmp/folder-one"),
            Some(&Some("mixedfolder".into()))
        );
        assert_eq!(
            app.bookmark_aliases_lc.get("https://example.com"),
            Some(&Some("mixedbookmark".into()))
        );

        std::fs::write(crate::plugins::folders::FOLDERS_FILE, "invalid folders").unwrap();
        send_event(WatchEvent::Folders);
        app.process_watch_events();
        assert_eq!(
            app.folder_aliases.get("/tmp/folder-one"),
            Some(&Some("MiXeDFolder".into()))
        );
        assert_eq!(
            std::fs::read_to_string(crate::plugins::folders::FOLDERS_FILE).unwrap(),
            "invalid folders"
        );

        std::fs::write(
            crate::plugins::bookmarks::BOOKMARKS_FILE,
            "invalid bookmarks",
        )
        .unwrap();
        send_event(WatchEvent::Bookmarks);
        app.process_watch_events();
        assert_eq!(
            app.bookmark_aliases.get("https://example.com"),
            Some(&Some("MiXeDBookmark".into()))
        );
        assert_eq!(
            std::fs::read_to_string(crate::plugins::bookmarks::BOOKMARKS_FILE).unwrap(),
            "invalid bookmarks"
        );

        let updated_folders_json = serde_json::json!([{
            "label": "Folder",
            "path": "/tmp/folder-one",
            "alias": "NewAlias"
        }]);
        std::fs::write(
            crate::plugins::folders::FOLDERS_FILE,
            serde_json::to_string_pretty(&updated_folders_json).unwrap(),
        )
        .unwrap();
        send_event(WatchEvent::Folders);
        app.process_watch_events();
        assert_eq!(
            app.folder_aliases.get("/tmp/folder-one"),
            Some(&Some("NewAlias".into()))
        );
        assert_eq!(
            app.folder_aliases_lc.get("/tmp/folder-one"),
            Some(&Some("newalias".into()))
        );

        let updated_bookmarks_json = serde_json::json!([{
            "url": "https://example.com",
            "alias": "OtherAlias"
        }]);
        std::fs::write(
            crate::plugins::bookmarks::BOOKMARKS_FILE,
            serde_json::to_string_pretty(&updated_bookmarks_json).unwrap(),
        )
        .unwrap();
        send_event(WatchEvent::Bookmarks);
        app.process_watch_events();
        assert_eq!(
            app.bookmark_aliases.get("https://example.com"),
            Some(&Some("OtherAlias".into()))
        );
        assert_eq!(
            app.bookmark_aliases_lc.get("https://example.com"),
            Some(&Some("otheralias".into()))
        );

        std::env::set_current_dir(original_dir).unwrap();
    }

    #[test]
    fn watch_event_bursts_delay_completion_rebuild_until_debounce_window() {
        let _lock = TEST_MUTEX.lock().unwrap();
        let dir = tempdir().unwrap();
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();

        std::fs::write(
            "actions.json",
            serde_json::to_string_pretty(&serde_json::json!([
                {
                    "label": "Initial App",
                    "desc": "demo",
                    "action": "initial:app",
                    "args": null
                }
            ]))
            .unwrap(),
        )
        .unwrap();

        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.rebuild_completion_index_now();
        assert!(app.completion_index.is_some());

        std::fs::write(
            "actions.json",
            serde_json::to_string_pretty(&serde_json::json!([
                {
                    "label": "Updated App",
                    "desc": "demo",
                    "action": "updated:app",
                    "args": null
                }
            ]))
            .unwrap(),
        )
        .unwrap();

        send_event(WatchEvent::Actions);
        send_event(WatchEvent::Actions);
        app.process_watch_events();

        assert!(app.completion_index.is_none());
        assert!(app.action_completion_dirty);

        let scheduled = app
            .completion_rebuild_after
            .expect("rebuild should be scheduled");
        app.maybe_rebuild_completion_index(scheduled - Duration::from_millis(1));
        assert!(app.completion_index.is_none());

        app.maybe_rebuild_completion_index(scheduled + Duration::from_millis(1));
        assert!(app.completion_index.is_some());
        assert!(!app.action_completion_dirty);
        assert!(!app.command_completion_dirty);
        assert!(app.completion_rebuild_after.is_none());

        std::env::set_current_dir(original_dir).unwrap();
    }

    #[test]
    fn completion_suggestions_clear_until_rebuild_and_match_latest_entries() {
        let _lock = TEST_MUTEX.lock().unwrap();
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.query_autocomplete = true;

        app.actions = Arc::new(vec![Action {
            label: "Old App".into(),
            desc: "demo".into(),
            action: "old:app".into(),
            args: None,
        }]);
        app.update_action_cache();
        app.rebuild_completion_index_now();

        app.query = "app ".into();
        app.update_suggestions();
        assert!(app.suggestions.iter().any(|s| s == "app old app"));

        app.actions = Arc::new(vec![Action {
            label: "New App".into(),
            desc: "demo".into(),
            action: "new:app".into(),
            args: None,
        }]);
        app.update_action_cache();

        assert!(app.completion_index.is_none());
        assert!(app.suggestions.is_empty());

        app.maybe_rebuild_completion_index(
            Instant::now() + COMPLETION_REBUILD_DEBOUNCE + Duration::from_millis(1),
        );

        assert!(app.suggestions.iter().all(|s| s != "app old app"));
        assert!(app.suggestions.iter().any(|s| s == "app new app"));
    }

    #[test]
    fn open_note_panel_reuses_existing_panel_for_same_slug() {
        let _lock = TEST_MUTEX.lock().unwrap();
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let dir = tempdir().unwrap();
        let prev = std::env::var("ML_NOTES_DIR").ok();
        unsafe { std::env::set_var("ML_NOTES_DIR", dir.path()) };

        append_note("Second Note", "body").unwrap();
        app.open_note_panel("second-note", None);
        app.open_note_panel("second-note", None);

        assert_eq!(app.note_panels.len(), 1);

        if let Some(prev) = prev {
            unsafe { std::env::set_var("ML_NOTES_DIR", prev) };
        } else {
            unsafe { std::env::remove_var("ML_NOTES_DIR") };
        }
    }

    #[test]
    fn open_note_link_valid_and_invalid() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);

        super::OPEN_LINK_COUNT.store(0, Ordering::SeqCst);
        app.open_note_link("https://example.com");
        assert_eq!(super::OPEN_LINK_COUNT.load(Ordering::SeqCst), 1);
        super::OPEN_LINK_COUNT.store(0, Ordering::SeqCst);
        app.open_note_link("www.example.com");
        assert_eq!(super::OPEN_LINK_COUNT.load(Ordering::SeqCst), 1);

        super::OPEN_LINK_COUNT.store(0, Ordering::SeqCst);
        app.open_note_link("http://example.com");
        assert_eq!(super::OPEN_LINK_COUNT.load(Ordering::SeqCst), 0);

        super::OPEN_LINK_COUNT.store(0, Ordering::SeqCst);
        app.open_note_link("ftp://example.com");
        assert_eq!(super::OPEN_LINK_COUNT.load(Ordering::SeqCst), 0);

        super::OPEN_LINK_COUNT.store(0, Ordering::SeqCst);
        app.open_note_link("internal-note");
        assert_eq!(super::OPEN_LINK_COUNT.load(Ordering::SeqCst), 0);
        assert_eq!(app.note_panels.len(), 1);
    }

    #[test]
    fn tab_cycles_through_suggestions() {
        let _lock = TEST_MUTEX.lock().unwrap();
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.query_autocomplete = true;
        app.suggestions = vec!["alpha".to_string(), "beta".to_string(), "gamma".to_string()];

        for expected in ["alpha", "beta", "gamma", "alpha"] {
            assert!(app.accept_suggestion(true));
            assert_eq!(app.query, expected);
            assert_eq!(
                app.suggestions,
                vec!["alpha".to_string(), "beta".to_string(), "gamma".to_string(),]
            );
        }
    }

    #[test]
    fn pinned_panel_prevents_close() {
        let ctx = egui::Context::default();
        let mut settings = Settings::default();
        settings.pinned_panels = vec![Panel::ClipboardDialog];
        let dir = tempdir().unwrap();
        let actions_path = dir.path().join("actions.json");
        let settings_path = dir.path().join("settings.json");
        let mut app = LauncherApp::new(
            &ctx,
            Arc::new(Vec::new()),
            0,
            PluginManager::new(),
            actions_path.to_string_lossy().to_string(),
            settings_path.to_string_lossy().to_string(),
            settings,
            None,
            None,
            None,
            None,
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        );
        assert!(app.clipboard_dialog.open);
        assert!(!app.close_front_dialog());
        assert!(app.clipboard_dialog.open);
        app.toggle_pin(Panel::ClipboardDialog);
        assert!(!app.pinned_panels.contains(&Panel::ClipboardDialog));
        app.clipboard_dialog.open = true;
        app.update_panel_stack();
        assert!(app.close_front_dialog());
        assert!(!app.clipboard_dialog.open);
    }

    #[test]
    fn unpinning_dirty_radial_editor_requires_explicit_close_confirmation() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.radial_editor.lock().unwrap().open_test_snapshot();
        app.radial_editor.lock().unwrap().make_dirty_for_test();
        app.pinned_panels.push(Panel::RadialEditor);
        app.panel_stack.push(Panel::RadialEditor);

        app.toggle_pin(Panel::RadialEditor);

        assert!(!app.pinned_panels.contains(&Panel::RadialEditor));
        assert!(app.radial_editor.lock().unwrap().open);
        assert!(app.radial_editor.lock().unwrap().is_dirty());
        assert!(app.radial_editor.lock().unwrap().has_close_prompt());
        assert!(app.panel_stack.contains(&Panel::RadialEditor));
    }

    #[test]
    fn root_rearms_retained_radial_deferred_viewport_focus_once() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.focus_panel(Panel::RadialEditor);
        let viewport_id = radial_editor::radial_designer_viewport_id();
        let (root_output, callback) = register_radial_deferred_callback(&ctx, &app);
        assert_eq!(focus_commands(&root_output, viewport_id), 1);
        assert!(ctx.has_requested_repaint_for(&viewport_id));
        root_output.drop_without_applying_deltas();

        let first = ctx.run_ui(deferred_child_input(viewport_id), |child| {
            callback(child);
        });
        let second = ctx.run_ui(deferred_child_input(viewport_id), |child| {
            callback(child);
        });

        assert_eq!(focus_commands(&first, viewport_id), 0);
        assert_eq!(focus_commands(&second, viewport_id), 0);
        first.drop_without_applying_deltas();
        second.drop_without_applying_deltas();
    }

    #[test]
    fn pinned_radial_maintenance_is_idempotent_but_explicit_focus_rearms() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.pinned_panels.push(Panel::RadialEditor);
        app.enforce_pinned();
        let viewport_id = radial_editor::radial_designer_viewport_id();
        let (initial, callback) = register_radial_deferred_callback(&ctx, &app);
        assert_eq!(focus_commands(&initial, viewport_id), 1);
        initial.drop_without_applying_deltas();

        let initial_child = ctx.run_ui(deferred_child_input(viewport_id), |child| {
            callback(child);
        });
        assert_eq!(focus_commands(&initial_child, viewport_id), 0);
        initial_child.drop_without_applying_deltas();

        app.enforce_pinned();
        app.enforce_pinned();
        let (maintained, maintained_callback) = register_radial_deferred_callback(&ctx, &app);
        assert_eq!(focus_commands(&maintained, viewport_id), 0);
        maintained.drop_without_applying_deltas();
        let maintained_child = ctx.run_ui(deferred_child_input(viewport_id), |child| {
            maintained_callback(child);
        });
        assert_eq!(focus_commands(&maintained_child, viewport_id), 0);
        maintained_child.drop_without_applying_deltas();

        app.focus_panel(Panel::RadialEditor);
        let (explicit, explicit_callback) = register_radial_deferred_callback(&ctx, &app);
        assert_eq!(focus_commands(&explicit, viewport_id), 1);
        let explicit_child = ctx.run_ui(deferred_child_input(viewport_id), |child| {
            explicit_callback(child);
        });
        assert_eq!(focus_commands(&explicit_child, viewport_id), 0);
        explicit.drop_without_applying_deltas();
        explicit_child.drop_without_applying_deltas();
    }

    #[test]
    fn diagnostics_widget_layout_enables_context_flag() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.show_dashboard_diagnostics = false;
        app.dashboard.slots = vec![NormalizedSlot {
            id: None,
            widget: "diagnostics".to_string(),
            row: 0,
            col: 0,
            row_span: 1,
            col_span: 1,
            settings: json!({}),
            overflow: OverflowMode::Scroll,
        }];

        assert!(app.has_diagnostics_widget());
        assert!(app.show_dashboard_diagnostics || app.has_diagnostics_widget());
    }

    #[test]
    fn diagnostics_context_includes_snapshot_when_widget_present() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.show_dashboard_diagnostics = false;
        app.dashboard.slots = vec![NormalizedSlot {
            id: None,
            widget: "diagnostics".to_string(),
            row: 0,
            col: 0,
            row_span: 1,
            col_span: 1,
            settings: json!({}),
            overflow: OverflowMode::Scroll,
        }];

        let has_diagnostics_widget = app.has_diagnostics_widget();
        let show_diagnostics_widget = app.show_dashboard_diagnostics || has_diagnostics_widget;
        let diagnostics = if app.show_dashboard_diagnostics || has_diagnostics_widget {
            Some(app.dashboard.diagnostics_snapshot())
        } else {
            None
        };

        assert!(show_diagnostics_widget);
        assert!(diagnostics.is_some());
    }

    #[test]
    fn diagnostics_context_omits_snapshot_when_disabled_and_missing_widget() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.show_dashboard_diagnostics = false;
        app.dashboard.slots = vec![NormalizedSlot {
            id: None,
            widget: "notes".to_string(),
            row: 0,
            col: 0,
            row_span: 1,
            col_span: 1,
            settings: json!({}),
            overflow: OverflowMode::Scroll,
        }];

        let has_diagnostics_widget = app.has_diagnostics_widget();
        let show_diagnostics_widget = app.show_dashboard_diagnostics || has_diagnostics_widget;
        let diagnostics = if app.show_dashboard_diagnostics || has_diagnostics_widget {
            Some(app.dashboard.diagnostics_snapshot())
        } else {
            None
        };

        assert!(!show_diagnostics_widget);
        assert!(diagnostics.is_none());
    }

    #[test]
    fn image_panel_closes_with_escape() {
        let ctx = egui::Context::default();
        let dir = tempdir().unwrap();
        let path = dir.path().join("img.png");
        RgbaImage::new(1, 1).save(&path).unwrap();

        let mut app = new_app(&ctx);
        app.open_image_panel(&path);
        assert_eq!(app.image_panels.len(), 1);
        assert!(app.close_front_dialog());
        assert!(app.image_panels.is_empty());
    }

    #[test]
    fn open_note_in_neovim_resolves_and_handles_errors() {
        use crate::plugins::note::Note;
        use anyhow::anyhow;
        use std::cell::Cell;

        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);

        let note_path = tempdir().unwrap().path().join("alpha.md");
        let load_called = Cell::new(false);
        let spawn_called = Cell::new(false);

        let closed = app.open_note_in_neovim(
            "alpha",
            || {
                load_called.set(true);
                Ok(vec![Note {
                    title: "alpha".into(),
                    path: note_path.clone(),
                    content: String::new(),
                    tags: Vec::new(),
                    links: Vec::new(),
                    slug: "alpha".into(),
                    alias: None,
                    aliases: Vec::new(),
                    entity_refs: Vec::new(),
                }])
            },
            |p| {
                spawn_called.set(true);
                assert_eq!(p, &note_path);
                Ok(())
            },
        );
        assert!(closed);
        assert!(load_called.get());
        assert!(spawn_called.get());
        assert!(app.error.is_none());

        spawn_called.set(false);
        app.error = None;
        let closed = app.open_note_in_neovim(
            "missing",
            || Ok(Vec::new()),
            |_| {
                spawn_called.set(true);
                Ok(())
            },
        );
        assert!(closed);
        assert!(!spawn_called.get());
        assert_eq!(app.error.as_deref(), Some("Note not found"));

        spawn_called.set(false);
        app.error = None;
        let closed = app.open_note_in_neovim(
            "alpha",
            || Err(anyhow!("load failure")),
            |_| {
                spawn_called.set(true);
                Ok(())
            },
        );
        assert!(closed);
        assert!(!spawn_called.get());
        assert!(app.error.as_ref().unwrap().contains("load failure"));
    }

    #[test]
    fn delete_note_uses_alias_and_logs_message() {
        let _lock = TEST_MUTEX.lock().unwrap();
        let dir = tempdir().unwrap();
        let notes_dir = dir.path().join("notes");
        std::fs::create_dir_all(&notes_dir).unwrap();
        unsafe { std::env::set_var("ML_NOTES_DIR", &notes_dir) };
        unsafe { std::env::set_var("HOME", dir.path()) };
        let orig_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        std::env::set_current_dir(dir.path()).unwrap();
        save_notes(&[]).unwrap();
        reset_slug_lookup();
        append_note("alpha", "# alpha\nAlias: special-name\n\ncontent").unwrap();

        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.plugins.register(Box::new(NotePlugin::default()));
        app.enable_toasts = true;

        app.query = "note list".into();
        app.search();

        app.delete_note("special-name");
        assert!(load_notes().unwrap().is_empty());
        if let Some(p) = app.pending_query.take() {
            app.query = p;
        }
        app.last_results_valid = false;
        app.search();
        assert!(!app.results.iter().any(|a| a.action == "note:open:alpha"));
        let log_path = dir.path().join(TOAST_LOG_FILE);
        let log = std::fs::read_to_string(log_path).unwrap();
        assert!(log.contains("Removed note special-name"));

        std::env::set_current_dir(orig_dir).unwrap();
    }

    #[test]
    fn destructive_note_delete_is_queued_when_confirmation_required() {
        let _lock = TEST_MUTEX.lock().unwrap();
        let dir = tempdir().unwrap();
        let notes_dir = dir.path().join("notes");
        std::fs::create_dir_all(&notes_dir).unwrap();
        unsafe { std::env::set_var("ML_NOTES_DIR", &notes_dir) };
        unsafe { std::env::set_var("HOME", dir.path()) };
        let orig_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        std::env::set_current_dir(dir.path()).unwrap();
        save_notes(&[]).unwrap();
        reset_slug_lookup();
        append_note("alpha", "# alpha\n\ncontent").unwrap();

        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.require_confirm_destructive = true;

        app.activate_action(
            Action {
                label: "Delete note".into(),
                desc: "Notes".into(),
                action: "note:remove:alpha".into(),
                args: None,
            },
            None,
            ActivationSource::Enter,
        );

        assert!(app.pending_confirm.is_some());
        assert_eq!(load_notes().unwrap().len(), 1);

        std::env::set_current_dir(orig_dir).unwrap();
    }

    #[test]
    fn destructive_note_delete_executes_only_after_confirmation() {
        let _lock = TEST_MUTEX.lock().unwrap();
        let dir = tempdir().unwrap();
        let notes_dir = dir.path().join("notes");
        std::fs::create_dir_all(&notes_dir).unwrap();
        unsafe { std::env::set_var("ML_NOTES_DIR", &notes_dir) };
        unsafe { std::env::set_var("HOME", dir.path()) };
        let orig_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        std::env::set_current_dir(dir.path()).unwrap();
        save_notes(&[]).unwrap();
        reset_slug_lookup();
        append_note("alpha", "# alpha\n\ncontent").unwrap();

        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.require_confirm_destructive = true;

        app.activate_action(
            Action {
                label: "Delete note".into(),
                desc: "Notes".into(),
                action: "note:remove:alpha".into(),
                args: None,
            },
            None,
            ActivationSource::Enter,
        );

        assert_eq!(load_notes().unwrap().len(), 1);
        let pending = app.pending_confirm.take().expect("pending confirm action");
        app.dispatch_command_invocation(pending.invocation);
        assert!(load_notes().unwrap().is_empty());

        std::env::set_current_dir(orig_dir).unwrap();
    }

    #[test]
    fn note_graph_dialog_action_opens_dialog() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        assert!(!app.note_graph_dialog.open);

        app.activate_action(
            Action {
                label: "Open note graph".into(),
                desc: "Note".into(),
                action: "note:graph_dialog".into(),
                args: Some(r#"{"include_tags":["foo"],"root":"alpha"}"#.into()),
            },
            None,
            ActivationSource::Enter,
        );

        assert!(app.note_graph_dialog.open);
    }

    #[test]
    fn note_search_debounce_respects_delay() {
        let start = Instant::now();
        assert!(!LauncherApp::note_search_debounce_ready(
            Some(start),
            start,
            NOTE_SEARCH_DEBOUNCE,
        ));
        assert!(!LauncherApp::note_search_debounce_ready(
            Some(start),
            start + Duration::from_millis(999),
            NOTE_SEARCH_DEBOUNCE,
        ));
        assert!(LauncherApp::note_search_debounce_ready(
            Some(start),
            start + NOTE_SEARCH_DEBOUNCE,
            NOTE_SEARCH_DEBOUNCE,
        ));
    }
    #[test]
    fn launcher_new_shares_file_search_settings_with_dialog_and_coordinator() {
        let ctx = egui::Context::default();
        let mut settings = Settings::default();
        let file_search_settings = crate::file_search::settings::FileSearchSettings {
            global_search_roots: vec![std::path::PathBuf::from("/tmp/search-root")],
            excluded_directory_names: vec!["skip-me".to_owned()],
            max_search_results: 123,
            max_matches_per_content_file: 7,
            max_content_search_file_size_bytes: 456_789,
            include_hidden_files: true,
            case_sensitive: true,
            everything_executable_path: std::path::PathBuf::from("custom-everything.exe"),
            ripgrep_executable_path: std::path::PathBuf::from("custom-rg"),
            everything_enabled: false,
            ..crate::file_search::settings::FileSearchSettings::default()
        };
        settings.plugin_settings.insert(
            "file_search".to_owned(),
            serde_json::to_value(file_search_settings.clone()).expect("serialize settings"),
        );

        let app = LauncherApp::new(
            &ctx,
            Arc::new(Vec::new()),
            0,
            PluginManager::new(),
            "actions.json".into(),
            "settings.json".into(),
            settings,
            None,
            None,
            None,
            None,
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        );

        assert_eq!(app.file_search_dialog.settings, file_search_settings);
        assert_eq!(
            app.file_search_dialog.case_sensitive,
            file_search_settings.case_sensitive
        );
        assert_eq!(
            app.file_search_dialog.include_hidden,
            file_search_settings.include_hidden_files
        );
        assert_eq!(
            app.file_search_coordinator.production_settings(),
            Some(&file_search_settings)
        );
    }

    #[test]
    fn launcher_new_normalizes_conflicting_follow_mouse_static_config() {
        let ctx = egui::Context::default();
        let mut settings = Settings::default();
        settings.follow_mouse = true;
        settings.static_location_enabled = true;
        settings.static_pos = Some((10, 20));
        settings.static_size = Some((500, 400));

        let app = LauncherApp::new(
            &ctx,
            Arc::new(Vec::new()),
            0,
            PluginManager::new(),
            "actions.json".into(),
            "settings.json".into(),
            settings,
            None,
            None,
            None,
            None,
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        );

        assert!(app.follow_mouse);
        assert!(!app.static_location_enabled);
        assert_eq!(app.static_pos, None);
        assert_eq!(app.static_size, None);
    }

    #[test]
    fn grid_context_menu_uses_universal_bookmark_actions() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.resolved_grid_layout = true;
        app.bookmark_aliases
            .insert("https://example.com".into(), Some("Example".into()));
        app.results = vec![Action {
            label: "Example".into(),
            desc: "Bookmark".into(),
            action: "https://example.com".into(),
            args: None,
        }];

        let actions = app.resolve_context_menu_actions(
            &app.results[0],
            crate::universal_actions::PinCapability::Writable { is_pinned: false },
        );
        assert!(
            actions
                .iter()
                .any(|action| action.id == crate::universal_actions::action_ids::BOOKMARK_SET_ALIAS)
        );
    }

    #[test]
    fn context_menu_parity_bookmark_between_list_and_grid() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let action = Action {
            label: "Docs".into(),
            desc: "Bookmark".into(),
            action: "https://docs.rs".into(),
            args: None,
        };
        app.bookmark_aliases
            .insert(action.action.clone(), Some("docs".into()));

        app.resolved_grid_layout = false;
        let list = app.resolve_context_menu_actions(
            &action,
            crate::universal_actions::PinCapability::Writable { is_pinned: false },
        );
        app.resolved_grid_layout = true;
        let grid = app.resolve_context_menu_actions(
            &action,
            crate::universal_actions::PinCapability::Writable { is_pinned: false },
        );

        let ids = |actions: &[crate::universal_actions::UniversalAction]| {
            actions
                .iter()
                .map(|action| action.id.clone())
                .collect::<Vec<_>>()
        };
        assert!(ids(&list).contains(&crate::universal_actions::action_ids::BOOKMARK_SET_ALIAS));
        assert_eq!(ids(&grid), ids(&list));
    }

    #[test]
    fn context_menu_parity_todo_between_list_and_grid() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let action = Action {
            label: "[ ] parity".into(),
            desc: "Todo".into(),
            action: "todo:done:7".into(),
            args: None,
        };

        app.resolved_grid_layout = false;
        let list = app.resolve_context_menu_actions(
            &action,
            crate::universal_actions::PinCapability::Writable { is_pinned: false },
        );
        app.resolved_grid_layout = true;
        let grid = app.resolve_context_menu_actions(
            &action,
            crate::universal_actions::PinCapability::Writable { is_pinned: false },
        );

        let ids = |actions: &[crate::universal_actions::UniversalAction]| {
            actions
                .iter()
                .map(|action| action.id.clone())
                .collect::<Vec<_>>()
        };
        assert!(ids(&list).contains(&crate::universal_actions::action_ids::TODO_EDIT));
        assert_eq!(ids(&grid), ids(&list));
    }
    #[test]
    fn handle_key_grid_navigation_arrows_and_numpad() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.query_results_layout.enabled = true;
        app.query_results_layout.cols = 3;
        app.query_results_layout.rows = 2;
        app.resolved_grid_layout = true;
        app.results = (0..8)
            .map(|i| Action {
                label: format!("A{i}"),
                desc: "d".into(),
                action: format!("act:{i}"),
                args: None,
            })
            .collect();

        app.selected = Some(4);
        app.handle_key(egui::Key::ArrowRight);
        assert_eq!(app.selected, Some(5));
        app.handle_key(egui::Key::ArrowLeft);
        assert_eq!(app.selected, Some(4));
        app.handle_key(egui::Key::ArrowUp);
        assert_eq!(app.selected, Some(1));
        app.handle_key(egui::Key::ArrowDown);
        assert_eq!(app.selected, Some(4));

        app.handle_key(egui::Key::Num6);
        assert_eq!(app.selected, Some(5));
        app.handle_key(egui::Key::Num4);
        assert_eq!(app.selected, Some(4));
        app.handle_key(egui::Key::Num8);
        assert_eq!(app.selected, Some(1));
        app.handle_key(egui::Key::Num2);
        assert_eq!(app.selected, Some(4));

        app.selected = Some(7);
        app.handle_key(egui::Key::ArrowDown);
        assert_eq!(app.selected, Some(7));
    }

    #[test]
    fn handle_key_list_mode_remains_compatible() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.resolved_grid_layout = false;
        app.results = (0..4)
            .map(|i| Action {
                label: format!("A{i}"),
                desc: "d".into(),
                action: format!("act:{i}"),
                args: None,
            })
            .collect();

        app.selected = Some(1);
        app.handle_key(egui::Key::ArrowDown);
        assert_eq!(app.selected, Some(2));
        app.handle_key(egui::Key::ArrowUp);
        assert_eq!(app.selected, Some(1));
        app.handle_key(egui::Key::ArrowRight);
        assert_eq!(app.selected, Some(1));
    }

    #[test]
    fn enter_and_action_sheet_share_selected_or_sole_target_semantics() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.results.clear();
        app.selected = None;
        assert_eq!(app.current_actionable_result_index(), None);
        assert_eq!(app.handle_key(egui::Key::Enter), None);

        app.results.push(Action {
            label: "Only".into(),
            desc: "Test".into(),
            action: "only".into(),
            args: None,
        });
        assert_eq!(app.current_actionable_result_index(), Some(0));
        assert_eq!(app.handle_key(egui::Key::Enter), Some(0));

        app.results.push(Action {
            label: "Second".into(),
            desc: "Test".into(),
            action: "second".into(),
            args: None,
        });
        assert_eq!(app.current_actionable_result_index(), None);
        app.selected = Some(1);
        assert_eq!(app.current_actionable_result_index(), Some(1));
        assert_eq!(app.handle_key(egui::Key::Enter), Some(1));
    }

    #[test]
    fn action_sheet_shortcut_obeys_launcher_input_gates() {
        assert!(LauncherApp::launcher_action_sheet_shortcut_enabled(
            true, false, false
        ));
        assert!(!LauncherApp::launcher_action_sheet_shortcut_enabled(
            false, false, false
        ));
        assert!(!LauncherApp::launcher_action_sheet_shortcut_enabled(
            true, true, false
        ));
        assert!(!LauncherApp::launcher_action_sheet_shortcut_enabled(
            true, false, true
        ));
    }

    #[test]
    fn action_sheet_escape_preserves_query_and_does_not_activate_result() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.query = "keep this query".into();
        app.results = vec![Action {
            label: "Generic".into(),
            desc: "Test".into(),
            action: "generic:test".into(),
            args: None,
        }];

        assert!(app.open_action_sheet_for_index(0));
        assert!(app.action_sheet.is_open());
        assert!(app.test_last_activation.is_none());
        assert!(
            app.handle_action_sheet_key(Some(action_sheet::ActionSheetKey::Escape))
                .is_none()
        );

        assert!(!app.action_sheet.is_open());
        assert_eq!(app.query, "keep this query");
        assert!(app.test_last_activation.is_none());
    }

    #[test]
    fn action_sheet_resolves_the_same_actions_in_list_and_grid() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.results = vec![Action {
            label: "Generic".into(),
            desc: "Test".into(),
            action: "generic:test".into(),
            args: None,
        }];

        app.resolved_grid_layout = false;
        assert!(app.open_action_sheet_for_index(0));
        let list_ids = app
            .action_sheet
            .actions
            .iter()
            .map(|action| action.id.clone())
            .collect::<Vec<_>>();
        app.action_sheet.close();

        app.resolved_grid_layout = true;
        assert!(app.open_action_sheet_for_index(0));
        let grid_ids = app
            .action_sheet
            .actions
            .iter()
            .map(|action| action.id.clone())
            .collect::<Vec<_>>();

        assert_eq!(grid_ids, list_ids);
    }

    #[test]
    fn launcher_enter_activation_requires_query_focus_and_closed_file_search() {
        assert!(!LauncherApp::launcher_enter_activation_enabled(
            false, false
        ));
        assert!(LauncherApp::launcher_enter_activation_enabled(true, false));
        assert!(!LauncherApp::launcher_enter_activation_enabled(true, true));
    }

    #[test]
    fn fs_query_text_does_not_imply_launcher_keyboard_ownership() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.query = "fs".into();
        app.results = vec![Action {
            label: "File Search".into(),
            desc: "Open file search".into(),
            action: crate::file_search::actions::OPEN_ACTION.into(),
            args: None,
        }];
        app.selected = Some(0);

        assert!(!LauncherApp::launcher_query_keyboard_enabled(false));
        assert!(!LauncherApp::launcher_enter_activation_enabled(
            false,
            app.file_search_dialog.open
        ));
    }

    #[test]
    fn open_file_search_disables_launcher_enter_activation() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.query = "fs".into();
        app.file_search_dialog.open = true;

        assert!(!LauncherApp::launcher_enter_activation_enabled(
            true,
            app.file_search_dialog.open
        ));
    }

    #[test]
    fn launcher_escape_defers_to_file_search_and_native_popup() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.file_search_dialog.open = true;
        app.visible_flag.store(true, Ordering::SeqCst);

        assert!(!LauncherApp::launcher_escape_handling_enabled(
            app.file_search_dialog.open,
            false,
        ));
        assert!(!LauncherApp::launcher_escape_handling_enabled(false, true));
        assert!(LauncherApp::launcher_escape_handling_enabled(false, false));
        assert!(app.visible_flag.load(Ordering::SeqCst));
    }

    #[test]
    fn launcher_query_is_not_refocused_while_file_search_remains_open() {
        assert!(!LauncherApp::launcher_query_focus_should_be_requested(
            true, false, true
        ));
        assert!(!LauncherApp::launcher_query_focus_should_be_requested(
            false, true, true
        ));
        assert!(LauncherApp::launcher_query_focus_should_be_requested(
            false, true, false
        ));
    }

    #[test]
    fn arrow_page_tab_navigation_requires_query_focus() {
        assert!(!LauncherApp::launcher_query_keyboard_enabled(false));
        assert!(LauncherApp::launcher_query_keyboard_enabled(true));
    }

    #[test]
    fn grid_layout_defaults_to_enabled_for_non_prefixed_queries() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.query = "hello world".into();
        app.query_results_layout.enabled = true;
        app.query_results_layout.respect_plugin_capability = true;

        app.recompute_query_results_layout();
        assert!(app.resolved_grid_layout);
    }

    #[test]
    fn grid_layout_selection_respects_plugin_capability_and_opt_out() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.query = "note hi".into();
        app.query_results_layout.enabled = true;
        app.query_results_layout.respect_plugin_capability = true;

        app.plugins.register(Box::new(TestPlugin {
            name: "note",
            caps: vec![CAP_GRID_RESULTS_COMPATIBLE],
            prefixes: vec!["note"],
        }));
        app.recompute_query_results_layout();
        assert!(app.resolved_grid_layout);

        app.query_results_layout.plugin_opt_out = vec!["note".into()];
        app.recompute_query_results_layout();
        assert!(!app.resolved_grid_layout);

        app.query_results_layout.plugin_opt_out.clear();
        app.plugins.clear_plugins();
        app.plugins.register(Box::new(TestPlugin {
            name: "note",
            caps: vec![CAP_FORCE_LIST_RESULTS],
            prefixes: vec!["note"],
        }));
        app.recompute_query_results_layout();
        assert!(!app.resolved_grid_layout);
    }

    #[test]
    fn failing_action_reports_error_and_launcher_stays_responsive() {
        let _lock = TEST_MUTEX.lock().unwrap();
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.enable_toasts = true;

        set_execute_action_hook(Some(Box::new(|_| Err(anyhow::anyhow!("injected failure")))));

        app.activate_action(
            Action {
                label: "Broken".into(),
                desc: "Test".into(),
                action: "exec:broken".into(),
                args: None,
            },
            None,
            ActivationSource::Enter,
        );

        assert!(
            app.error
                .as_ref()
                .is_some_and(|msg| msg.contains("injected failure"))
        );
        assert!(app.error_time.is_some());

        set_execute_action_hook(Some(Box::new(|_| Ok(()))));
        app.activate_action(
            Action {
                label: "Healthy".into(),
                desc: "Test".into(),
                action: "exec:ok".into(),
                args: None,
            },
            None,
            ActivationSource::Enter,
        );
        assert!(app.error.is_some());

        set_execute_action_hook(None);
    }
}
