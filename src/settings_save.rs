//! Failure-safe settings edits coordinated with the main-owned hotkey route.

use crate::gui::LauncherApp;
use crate::hotkey::reconfiguration::{
    AppliedHotkeyReconfiguration, HotkeyApplySubmissionError, HotkeyReconfigurationError,
};
use crate::settings::Settings;
use eframe::egui;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const PREPARATION_TIMEOUT: Duration = Duration::from_secs(30);
const TERMINAL_TIMEOUT: Duration = Duration::from_secs(30);
const CANCELLATION_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SettingsSaveSource {
    General,
    PluginSelection,
}

macro_rules! rollback_field {
    ($latest:ident, $before:ident, $written:ident, $field:ident, $conflicts:ident) => {
        rollback_setting_field(
            &mut $latest.$field,
            &$before.$field,
            &$written.$field,
            stringify!($field),
            &mut $conflicts,
        );
    };
}

fn rollback_setting_field<T: serde::Serialize + Clone>(
    latest: &mut T,
    before: &T,
    written: &T,
    field: &str,
    conflicts: &mut Vec<String>,
) {
    let before_json = serde_json::to_value(before).expect("settings fields are serializable");
    let written_json = serde_json::to_value(written).expect("settings fields are serializable");
    if before_json == written_json {
        return;
    }
    let latest_json = serde_json::to_value(&*latest).expect("settings fields are serializable");
    if latest_json == written_json {
        *latest = before.clone();
    } else {
        conflicts.push(field.to_owned());
    }
}

macro_rules! define_general_settings_fields {
    ($($field:ident),+ $(,)?) => {
        const GENERAL_SETTINGS_FIELDS: &[&str] = &[$(stringify!($field)),+];

        fn copy_general_fields(target: &mut Settings, desired: &Settings) {
            $(target.$field = desired.$field.clone();)+
        }

        fn rollback_general_fields(
            latest: &mut Settings,
            before: &Settings,
            written: &Settings,
            conflicts: &mut Vec<String>,
        ) {
            let mut field_conflicts = Vec::new();
            $(rollback_field!(latest, before, written, $field, field_conflicts);)+
            conflicts.extend(field_conflicts);
        }
    };
}

define_general_settings_fields!(
    hotkey,
    quit_hotkey,
    help_hotkey,
    paste_sequence,
    debug_logging,
    enable_toasts,
    show_inline_errors,
    show_error_toasts,
    toast_duration,
    offscreen_pos,
    window_size,
    note_panel_default_size,
    note_save_on_close,
    note_confirm_discard_unsaved_changes,
    note_always_overwrite,
    note_images_as_links,
    note_show_details,
    note_more_limit,
    query_scale,
    list_scale,
    history_limit,
    clipboard_limit,
    fuzzy_weight,
    usage_weight,
    match_exact,
    page_jump,
    query_results_layout,
    follow_mouse,
    static_location_enabled,
    static_pos,
    static_size,
    hide_after_run,
    always_on_top,
    show_launcher_icon,
    show_launcher_title,
    timer_refresh,
    disable_timer_updates,
    preserve_command,
    clear_query_after_run,
    require_confirm_destructive,
    query_autocomplete,
    net_refresh,
    net_unit,
    screenshot_dir,
    screenshot_save_file,
    screenshot_auto_save,
    screenshot_use_editor,
    reduce_dashboard_work_when_unfocused,
    show_dashboard_diagnostics,
    dashboard,
    note,
    radial,
);

#[derive(Clone, Debug)]
enum SaveOwnership {
    General {
        plugin_settings: HashMap<String, Option<serde_json::Value>>,
    },
    PluginSelection,
}

#[derive(Clone, Debug, PartialEq)]
struct InvocationSettingsBaseline {
    paste_sequence: crate::settings::PasteSequencePreferences,
    hotkey: Option<String>,
    quit_hotkey: Option<String>,
    help_hotkey: Option<String>,
    radial: crate::radial::model::RadialFeatureSettings,
    screen_draw: ScreenDrawChordBaseline,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ScreenDrawChordBaseline {
    Parsed {
        launch: Option<String>,
        emergency: String,
    },
    Invalid(serde_json::Value),
}

impl InvocationSettingsBaseline {
    fn from_settings(settings: &Settings) -> Self {
        let screen_draw = match settings.plugin_settings.get("screen_draw") {
            Some(value) => match serde_json::from_value::<crate::screen_draw::ScreenDrawSettings>(
                value.clone(),
            ) {
                Ok(screen_draw) => ScreenDrawChordBaseline::Parsed {
                    launch: screen_draw
                        .launch_hotkey
                        .map(|hotkey| hotkey.as_str().to_owned()),
                    emergency: screen_draw.emergency_hotkey.as_str().to_owned(),
                },
                Err(_) => ScreenDrawChordBaseline::Invalid(value.clone()),
            },
            None => {
                let screen_draw = crate::screen_draw::ScreenDrawSettings::default();
                ScreenDrawChordBaseline::Parsed {
                    launch: None,
                    emergency: screen_draw.emergency_hotkey.as_str().to_owned(),
                }
            }
        };
        Self {
            hotkey: settings.hotkey.clone(),
            paste_sequence: settings.paste_sequence.clone(),
            quit_hotkey: settings.quit_hotkey.clone(),
            help_hotkey: settings.help_hotkey.clone(),
            radial: settings.radial.clone(),
            screen_draw,
        }
    }
}

/// Frozen editor values and the exact settings fields this submission owns.
/// Independent settings writers can continue to update other fields while the
/// hotkey runtime prepares and acknowledges this candidate.
#[derive(Clone, Debug)]
pub(crate) struct SettingsSaveIntent {
    desired: Settings,
    ownership: SaveOwnership,
    source: SettingsSaveSource,
    invocation_baseline: Option<InvocationSettingsBaseline>,
}

impl SettingsSaveIntent {
    pub(crate) fn general(desired: Settings, baseline: &Settings) -> Self {
        let mut keys = baseline
            .plugin_settings
            .keys()
            .chain(desired.plugin_settings.keys())
            .cloned()
            .collect::<HashSet<_>>();
        let plugin_settings = keys
            .drain()
            .filter_map(|key| {
                let before = baseline.plugin_settings.get(&key);
                let after = desired.plugin_settings.get(&key);
                (before != after).then(|| (key, after.cloned()))
            })
            .collect();
        Self {
            desired,
            ownership: SaveOwnership::General { plugin_settings },
            source: SettingsSaveSource::General,
            invocation_baseline: Some(InvocationSettingsBaseline::from_settings(baseline)),
        }
    }

    pub(crate) fn plugin_selection(
        enabled_plugins: Option<HashSet<String>>,
        enabled_capabilities: Option<HashMap<String, Vec<String>>>,
    ) -> Self {
        let mut desired = Settings::default();
        desired.enabled_plugins = enabled_plugins;
        desired.enabled_capabilities = enabled_capabilities;
        Self {
            desired,
            ownership: SaveOwnership::PluginSelection,
            source: SettingsSaveSource::PluginSelection,
            invocation_baseline: None,
        }
    }

    fn source(&self) -> SettingsSaveSource {
        self.source
    }

    fn apply(&self, latest: &mut Settings) {
        match &self.ownership {
            SaveOwnership::General { plugin_settings } => {
                copy_general_fields(latest, &self.desired);
                for (key, value) in plugin_settings {
                    if let Some(value) = value {
                        latest.plugin_settings.insert(key.clone(), value.clone());
                    } else {
                        latest.plugin_settings.remove(key);
                    }
                }
            }
            SaveOwnership::PluginSelection => {
                latest.enabled_plugins = self.desired.enabled_plugins.clone();
                latest.enabled_capabilities = self.desired.enabled_capabilities.clone();
            }
        }
    }

    fn verify_invocation_baseline(&self, latest: &Settings) -> anyhow::Result<()> {
        if self
            .invocation_baseline
            .as_ref()
            .is_some_and(|expected| *expected != InvocationSettingsBaseline::from_settings(latest))
        {
            anyhow::bail!(
                "launcher, radial, Screen Draw, or Paste Sequence settings changed since this editor was opened"
            );
        }
        Ok(())
    }

    fn overlay_applied(&self, latest: &mut Settings, committed: &Settings) {
        match &self.ownership {
            SaveOwnership::General { plugin_settings } => {
                copy_general_fields(latest, committed);
                for key in plugin_settings.keys() {
                    restore_map_value(
                        &mut latest.plugin_settings,
                        key,
                        committed.plugin_settings.get(key),
                    );
                }
            }
            SaveOwnership::PluginSelection => {
                latest.enabled_plugins = committed.enabled_plugins.clone();
                latest.enabled_capabilities = committed.enabled_capabilities.clone();
            }
        }
        overlay_acknowledged_invocation(latest, committed);
    }

    fn changed_owned_fields(&self, latest: &Settings, committed: &Settings) -> Vec<String> {
        let latest_value = serde_json::to_value(latest).ok();
        let committed_value = serde_json::to_value(committed).ok();
        let fields = match &self.ownership {
            SaveOwnership::General { .. } => GENERAL_SETTINGS_FIELDS,
            SaveOwnership::PluginSelection => &["enabled_plugins", "enabled_capabilities"],
        };
        let mut changed = Vec::new();
        if let (Some(latest_value), Some(committed_value)) = (latest_value, committed_value) {
            for field in fields {
                if latest_value.get(field) != committed_value.get(field) {
                    changed.push((*field).to_owned());
                }
            }
        }
        if let SaveOwnership::General { plugin_settings } = &self.ownership {
            for key in plugin_settings.keys() {
                if latest.plugin_settings.get(key) != committed.plugin_settings.get(key) {
                    push_changed_field(&mut changed, format!("plugin_settings.{key}"));
                }
            }
        }
        append_invocation_route_changes(&mut changed, latest, committed);
        changed
    }

    fn rollback_owned(
        &self,
        latest: &mut Settings,
        before: &Settings,
        written: &Settings,
    ) -> Vec<String> {
        let mut conflicts = Vec::new();
        match &self.ownership {
            SaveOwnership::General { plugin_settings } => {
                rollback_general_fields(latest, before, written, &mut conflicts);
                for key in plugin_settings.keys() {
                    if before.plugin_settings.get(key) == written.plugin_settings.get(key) {
                        continue;
                    }
                    if latest.plugin_settings.get(key) == written.plugin_settings.get(key) {
                        restore_map_value(
                            &mut latest.plugin_settings,
                            key,
                            before.plugin_settings.get(key),
                        );
                    } else {
                        conflicts.push(format!("plugin_settings.{key}"));
                    }
                }
            }
            SaveOwnership::PluginSelection => {
                rollback_field!(latest, before, written, enabled_plugins, conflicts);
                rollback_field!(latest, before, written, enabled_capabilities, conflicts);
            }
        }
        conflicts
    }
}

fn overlay_acknowledged_invocation(latest: &mut Settings, committed: &Settings) {
    latest.paste_sequence = committed.paste_sequence.clone();
    latest.hotkey = committed.hotkey.clone();
    latest.quit_hotkey = committed.quit_hotkey.clone();
    latest.help_hotkey = committed.help_hotkey.clone();
    latest.radial = committed.radial.clone();
    overlay_screen_draw_route_chords(latest, committed);
}

fn overlay_screen_draw_route_chords(latest: &mut Settings, committed: &Settings) {
    const SCREEN_DRAW: &str = "screen_draw";
    let default = serde_json::to_value(crate::screen_draw::ScreenDrawSettings::default())
        .unwrap_or(serde_json::Value::Null);
    let committed_value = committed
        .plugin_settings
        .get(SCREEN_DRAW)
        .cloned()
        .unwrap_or(default.clone());
    let Some(committed_object) = committed_value.as_object() else {
        return;
    };
    let launch_hotkey = committed_object
        .get("launch_hotkey")
        .cloned()
        .or_else(|| default.get("launch_hotkey").cloned());
    let emergency_hotkey = committed_object
        .get("emergency_hotkey")
        .cloned()
        .or_else(|| default.get("emergency_hotkey").cloned());
    let Some(launch_hotkey) = launch_hotkey else {
        return;
    };
    let Some(emergency_hotkey) = emergency_hotkey else {
        return;
    };

    match latest.plugin_settings.get_mut(SCREEN_DRAW) {
        Some(latest_value) => {
            if let Some(latest_object) = latest_value.as_object_mut() {
                latest_object.insert("launch_hotkey".into(), launch_hotkey);
                latest_object.insert("emergency_hotkey".into(), emergency_hotkey);
            } else {
                *latest_value = committed_value;
            }
        }
        None if committed.plugin_settings.contains_key(SCREEN_DRAW) => {
            latest
                .plugin_settings
                .insert(SCREEN_DRAW.into(), committed_value);
        }
        None => {}
    }
}

fn append_invocation_route_changes(
    changed: &mut Vec<String>,
    latest: &Settings,
    committed: &Settings,
) {
    let latest = InvocationSettingsBaseline::from_settings(latest);
    let committed = InvocationSettingsBaseline::from_settings(committed);
    if latest.paste_sequence != committed.paste_sequence {
        push_changed_field(changed, "paste_sequence".into());
    }
    if latest.hotkey != committed.hotkey {
        push_changed_field(changed, "hotkey".into());
    }
    if latest.quit_hotkey != committed.quit_hotkey {
        push_changed_field(changed, "quit_hotkey".into());
    }
    if latest.help_hotkey != committed.help_hotkey {
        push_changed_field(changed, "help_hotkey".into());
    }
    if latest.radial != committed.radial {
        push_changed_field(changed, "radial".into());
    }
    match (latest.screen_draw, committed.screen_draw) {
        (
            ScreenDrawChordBaseline::Parsed {
                launch: latest_launch,
                emergency: latest_emergency,
            },
            ScreenDrawChordBaseline::Parsed {
                launch: committed_launch,
                emergency: committed_emergency,
            },
        ) => {
            if latest_launch != committed_launch {
                push_changed_field(changed, "plugin_settings.screen_draw.launch_hotkey".into());
            }
            if latest_emergency != committed_emergency {
                push_changed_field(
                    changed,
                    "plugin_settings.screen_draw.emergency_hotkey".into(),
                );
            }
        }
        (latest, committed) if latest != committed => {
            push_changed_field(changed, "plugin_settings.screen_draw.hotkeys".into());
        }
        _ => {}
    }
}

fn push_changed_field(changed: &mut Vec<String>, field: String) {
    if !changed.contains(&field) {
        changed.push(field);
    }
}

fn restore_map_value(
    map: &mut HashMap<String, serde_json::Value>,
    key: &str,
    value: Option<&serde_json::Value>,
) {
    if let Some(value) = value {
        map.insert(key.to_owned(), value.clone());
    } else {
        map.remove(key);
    }
}

#[derive(Clone, Debug)]
pub(crate) struct LauncherClosePreferences {
    pub(crate) window_size: (i32, i32),
    pub(crate) pinned_panels: Vec<crate::gui::Panel>,
    pub(crate) clipboard_modify_dialog_size: (f32, f32),
    pub(crate) file_search_ui_preferences:
        Option<crate::file_search::settings::FileSearchUiPreferences>,
}

#[derive(Default)]
struct ShutdownPreferencesSlot {
    complete: bool,
    preferences: Option<LauncherClosePreferences>,
}

#[derive(Default)]
struct WorkerRegistryState {
    accepting: bool,
    workers: Vec<JoinHandle<()>>,
}

#[derive(Clone)]
pub struct SettingsSaveWorkerRegistry(Arc<Mutex<WorkerRegistryState>>);

impl Default for SettingsSaveWorkerRegistry {
    fn default() -> Self {
        Self(Arc::new(Mutex::new(WorkerRegistryState {
            accepting: true,
            workers: Vec::new(),
        })))
    }
}

impl SettingsSaveWorkerRegistry {
    pub(crate) fn spawn(&self, run: impl FnOnce() + Send + 'static) -> Result<(), std::io::Error> {
        let mut state = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !state.accepting {
            return Err(std::io::Error::other("launcher is shutting down"));
        }
        let worker = thread::Builder::new()
            .name("settings-save-transaction".into())
            .spawn(run)?;
        state.workers.push(worker);
        drop(state);
        self.reap_finished();
        Ok(())
    }

    fn reap_finished(&self) {
        let finished = {
            let mut state = self
                .0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let mut finished = Vec::new();
            let mut index = 0;
            while index < state.workers.len() {
                if state.workers[index].is_finished() {
                    finished.push(state.workers.swap_remove(index));
                } else {
                    index += 1;
                }
            }
            finished
        };
        for worker in finished {
            if let Err(payload) = worker.join() {
                tracing::error!(?payload, "settings save worker panicked");
            }
        }
    }

    pub fn begin_shutdown(&self) {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .accepting = false;
    }

    pub fn join_all(&self) {
        self.begin_shutdown();
        let workers = {
            let mut state = self
                .0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            std::mem::take(&mut state.workers)
        };
        for worker in workers {
            if let Err(payload) = worker.join() {
                tracing::error!(?payload, "settings save worker panicked during shutdown");
            }
        }
    }
}

pub(crate) struct PendingSettingsSave {
    receiver: Receiver<SettingsSaveWorkerMessage>,
    shutdown_preferences: Arc<Mutex<ShutdownPreferencesSlot>>,
    cancel_requested: Arc<std::sync::atomic::AtomicBool>,
}

struct SettingsSaveWorkerResult {
    source: SettingsSaveSource,
    result: Result<SettingsSaveCompletion, String>,
    reconciliation_required: bool,
}

enum SettingsSaveWorkerMessage {
    ReconciliationRequired {
        source: SettingsSaveSource,
        message: String,
    },
    Completed(Box<SettingsSaveWorkerResult>),
}

#[derive(Debug)]
struct SettingsSaveCompletion {
    settings: Settings,
    warning: Option<String>,
}

impl PendingSettingsSave {
    fn defer_shutdown_preferences(&self, preferences: LauncherClosePreferences) -> bool {
        let mut slot = self
            .shutdown_preferences
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if slot.complete {
            true
        } else {
            slot.preferences = Some(preferences);
            false
        }
    }

    fn request_cancel(&self) {
        self.cancel_requested
            .store(true, std::sync::atomic::Ordering::Release);
    }
}

impl LauncherApp {
    pub fn install_settings_save_worker_registry(&mut self, workers: SettingsSaveWorkerRegistry) {
        self.settings_save_workers = workers;
    }

    pub(crate) fn settings_save_pending(&self) -> bool {
        self.pending_settings_save.is_some() || self.settings_save_reconciliation_required
    }

    pub(crate) fn settings_save_reconciliation_required(&self) -> bool {
        self.settings_save_reconciliation_required
    }

    pub(crate) fn begin_settings_save(
        &mut self,
        context: &egui::Context,
        intent: SettingsSaveIntent,
    ) {
        self.begin_settings_save_with_prepare(context, intent, |settings| {
            crate::hotkey::reconfiguration::prepare_hotkey_reconfiguration(settings)
        });
    }

    fn begin_settings_save_with_prepare<F>(
        &mut self,
        context: &egui::Context,
        intent: SettingsSaveIntent,
        prepare: F,
    ) where
        F: FnOnce(
                Settings,
            ) -> Result<
                crate::hotkey::reconfiguration::HotkeyReconfigurationTicket,
                HotkeyReconfigurationError,
            > + Send
            + 'static,
    {
        if self.settings_save_pending() {
            return;
        }
        let path = self.settings_path.clone();
        let source = intent.source();
        let (sender, receiver) = mpsc::channel();
        let repaint = context.clone();
        let shutdown_preferences = Arc::new(Mutex::new(ShutdownPreferencesSlot::default()));
        let cancel_requested = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let worker_shutdown_preferences = Arc::clone(&shutdown_preferences);
        let worker_cancel_requested = Arc::clone(&cancel_requested);
        let reconciliation_required = Arc::new(AtomicBool::new(false));
        let worker_reconciliation_required = Arc::clone(&reconciliation_required);
        let worker = self.settings_save_workers.spawn(move || {
            let result = run_settings_save(
                &path,
                &intent,
                source,
                &worker_cancel_requested,
                &worker_reconciliation_required,
                &sender,
                &repaint,
                prepare,
                PREPARATION_TIMEOUT,
                TERMINAL_TIMEOUT,
                CANCELLATION_TIMEOUT,
            );
            let preferences = {
                let mut slot = worker_shutdown_preferences
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                slot.complete = true;
                slot.preferences.take()
            };
            if let Some(preferences) = preferences
                && let Err(error) = persist_launcher_close_preferences(&path, &preferences)
            {
                tracing::error!(%error, "failed to persist launcher preferences during settings-save shutdown");
            }
            let _ = sender.send(SettingsSaveWorkerMessage::Completed(Box::new(
                SettingsSaveWorkerResult {
                source,
                result,
                reconciliation_required: worker_reconciliation_required.load(
                    std::sync::atomic::Ordering::Acquire,
                ),
                },
            )));
            repaint.request_repaint();
            repaint.request_repaint_of(egui::ViewportId::ROOT);
        });
        match worker {
            Ok(()) => {
                self.pending_settings_save = Some(PendingSettingsSave {
                    receiver,
                    shutdown_preferences,
                    cancel_requested,
                });
            }
            Err(error) => self.report_ui_error(crate::gui::UiErrorEvent::new(
                match source {
                    SettingsSaveSource::General => "settings_editor.save",
                    SettingsSaveSource::PluginSelection => "plugin_editor.save",
                },
                format!("Could not start settings save worker: {error}"),
            )),
        }
    }

    pub(crate) fn drain_settings_save(&mut self, context: &egui::Context) {
        let Some(pending) = self.pending_settings_save.as_ref() else {
            return;
        };
        let message = match pending.receiver.try_recv() {
            Ok(message) => Some(message),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(SettingsSaveWorkerMessage::Completed(
                Box::new(SettingsSaveWorkerResult {
                    source: SettingsSaveSource::General,
                    result: Err("settings save worker ended without a completion result".into()),
                    reconciliation_required: true,
                }),
            )),
        };
        let Some(message) = message else {
            return;
        };
        let result = match message {
            SettingsSaveWorkerMessage::ReconciliationRequired { source, message } => {
                self.settings_save_reconciliation_required = true;
                self.report_ui_error(crate::gui::UiErrorEvent::new(
                    match source {
                        SettingsSaveSource::General => "settings_editor.save",
                        SettingsSaveSource::PluginSelection => "plugin_editor.save",
                    },
                    message,
                ));
                return;
            }
            SettingsSaveWorkerMessage::Completed(result) => *result,
        };
        self.pending_settings_save = None;
        self.settings_save_reconciliation_required = result.reconciliation_required;
        self.settings_save_workers.reap_finished();
        match result.result {
            Ok(completion) => {
                let show_success_toast =
                    result.source == SettingsSaveSource::General && completion.warning.is_none();
                crate::settings_editor::SettingsEditor::apply_saved_settings(
                    context,
                    self,
                    completion.settings,
                    show_success_toast,
                );
                if let Some(warning) = completion.warning {
                    self.report_ui_error(crate::gui::UiErrorEvent::new(
                        match result.source {
                            SettingsSaveSource::General => "settings_editor.save",
                            SettingsSaveSource::PluginSelection => "plugin_editor.save",
                        },
                        warning,
                    ));
                }
            }
            Err(message) => self.report_ui_error(crate::gui::UiErrorEvent::new(
                match result.source {
                    SettingsSaveSource::General => "settings_editor.save",
                    SettingsSaveSource::PluginSelection => "plugin_editor.save",
                },
                message,
            )),
        }
    }

    pub(crate) fn prepare_settings_save_shutdown(&mut self) {
        let preferences = self.settings_save_close_preferences();
        self.settings_save_workers.begin_shutdown();
        if let Some(pending) = self.pending_settings_save.as_ref() {
            pending.request_cancel();
            if !pending.defer_shutdown_preferences(preferences.clone()) {
                return;
            }
        }
        if let Err(error) = persist_launcher_close_preferences(&self.settings_path, &preferences) {
            tracing::error!(%error, "failed to persist launcher close preferences");
        }
    }
}

fn run_settings_save<P>(
    path: &str,
    intent: &SettingsSaveIntent,
    source: SettingsSaveSource,
    cancel_requested: &AtomicBool,
    reconciliation_required: &AtomicBool,
    sender: &mpsc::Sender<SettingsSaveWorkerMessage>,
    repaint: &egui::Context,
    prepare: P,
    preparation_timeout: Duration,
    terminal_timeout: Duration,
    cancellation_timeout: Duration,
) -> Result<SettingsSaveCompletion, String>
where
    P: FnOnce(
        Settings,
    ) -> Result<
        crate::hotkey::reconfiguration::HotkeyReconfigurationTicket,
        HotkeyReconfigurationError,
    >,
{
    let mut candidate = Settings::read_current(path).map_err(|error| error.to_string())?;
    intent
        .verify_invocation_baseline(&candidate)
        .map_err(|error| error.to_string())?;
    intent.apply(&mut candidate);
    candidate.paste_sequence.validate().map_err(str::to_owned)?;
    let mut ticket = prepare(candidate).map_err(|error| error.to_string())?;
    if cancel_requested.load(std::sync::atomic::Ordering::Acquire) {
        return cancel_before_apply(
            &mut ticket,
            source,
            reconciliation_required,
            sender,
            repaint,
            cancellation_timeout,
        );
    }

    if let Err(error) = ticket.wait_prepared(preparation_timeout) {
        if !matches!(
            error,
            HotkeyReconfigurationError::TimedOut | HotkeyReconfigurationError::Disconnected
        ) {
            return Err(format!(
                "hotkey route preparation failed before persistence: {error}"
            ));
        }
        let terminal = cancel_and_resolve(
            &mut ticket,
            source,
            reconciliation_required,
            sender,
            repaint,
            cancellation_timeout,
            "Hotkey preparation cancellation is awaiting confirmation; this save remains pending.",
        );
        if !matches!(
            &terminal,
            Ok(crate::hotkey::reconfiguration::HotkeyTerminalDisposition::ConfirmedNotAppliedOrRestored(_))
        ) {
            reconciliation_required.store(true, std::sync::atomic::Ordering::Release);
        }
        return Err(format_resolution_error(
            "hotkey route preparation failed",
            error,
            &terminal,
        ));
    }
    if cancel_requested.load(std::sync::atomic::Ordering::Acquire) {
        return cancel_before_apply(
            &mut ticket,
            source,
            reconciliation_required,
            sender,
            repaint,
            cancellation_timeout,
        );
    }

    let mut before = None;
    let committed = match Settings::update(path, |latest| {
        intent.verify_invocation_baseline(latest)?;
        before = Some(latest.clone());
        intent.apply(latest);
        Ok(())
    }) {
        Ok(committed) => committed,
        Err(error) => {
            let terminal = cancel_and_resolve(
                &mut ticket,
                source,
                reconciliation_required,
                sender,
                repaint,
                cancellation_timeout,
                "Settings persistence failed; hotkey cancellation is awaiting confirmation.",
            );
            if !matches!(
                &terminal,
                Ok(crate::hotkey::reconfiguration::HotkeyTerminalDisposition::ConfirmedNotAppliedOrRestored(_))
            ) {
                reconciliation_required.store(true, std::sync::atomic::Ordering::Release);
            }
            return Err(format!(
                "settings persistence failed after preparation ({error}); {}",
                resolution_description(&terminal)
            ));
        }
    };
    let before = before.expect("Settings::update invokes its mutator before returning");

    match ticket.apply(committed.clone()) {
        Ok(()) => {}
        Err(HotkeyApplySubmissionError::NotEnqueued(apply_error)) => {
            // A prepared listener is dormant. Since the Apply command is
            // proven absent from the owner queue, no runtime receipt is needed
            // to restore the settings fields this transaction actually wrote.
            let _ = ticket.cancel();
            return match rollback_after_failure(
                path,
                intent,
                &before,
                &committed,
                format!("hotkey apply was not enqueued: {apply_error}"),
            ) {
                Ok(message) => {
                    reconciliation_required.store(false, std::sync::atomic::Ordering::Release);
                    Err(message)
                }
                Err(error) => {
                    reconciliation_required.store(true, std::sync::atomic::Ordering::Release);
                    Err(error)
                }
            };
        }
        Err(HotkeyApplySubmissionError::AlreadyEnqueued(apply_error)) => {
            let terminal = cancel_and_resolve(
                &mut ticket,
                source,
                reconciliation_required,
                sender,
                repaint,
                cancellation_timeout,
                "Apply submission was already accepted; hotkey route ownership is awaiting confirmation.",
            );
            return resolve_persisted_failure(
                path,
                intent,
                &before,
                &committed,
                &ticket,
                reconciliation_required,
                format!("hotkey apply submission failed: {apply_error}"),
                terminal,
            );
        }
    }

    let terminal = match ticket.wait_terminal_disposition(terminal_timeout) {
        Ok(disposition) => Ok(disposition),
        Err(error) => {
            let terminal = cancel_and_resolve(
                &mut ticket,
                source,
                reconciliation_required,
                sender,
                repaint,
                cancellation_timeout,
                "Hotkey apply timed out; cancellation is awaiting an authoritative route result. This save remains pending.",
            );
            return resolve_persisted_failure(
                path,
                intent,
                &before,
                &committed,
                &ticket,
                reconciliation_required,
                format!("hotkey apply did not complete: {error}"),
                terminal,
            );
        }
    };

    resolve_persisted_failure(
        path,
        intent,
        &before,
        &committed,
        &ticket,
        reconciliation_required,
        "hotkey route application failed".into(),
        terminal,
    )
}

fn finish_applied(
    path: &str,
    intent: &SettingsSaveIntent,
    committed: &Settings,
    applied: AppliedHotkeyReconfiguration,
) -> SettingsSaveCompletion {
    match Settings::read_current(path) {
        Ok(mut latest) => {
            let changed_fields = intent.changed_owned_fields(&latest, committed);
            intent.overlay_applied(&mut latest, committed);
            SettingsSaveCompletion {
                settings: latest,
                warning: (!changed_fields.is_empty()).then(|| {
                    format!(
                        "The hotkey route was applied from save request {}, but later writes changed these same saved fields: {}. The acknowledged values are active until another save is confirmed.",
                        applied.request_id.get(),
                        changed_fields.join(", ")
                    )
                }),
            }
        }
        Err(error) => SettingsSaveCompletion {
            settings: committed.clone(),
            warning: Some(format!(
                "The hotkey route was applied, but the latest unrelated saved settings could not be read for GUI publication: {error}"
            )),
        },
    }
}

fn cancel_before_apply(
    ticket: &mut crate::hotkey::reconfiguration::HotkeyReconfigurationTicket,
    source: SettingsSaveSource,
    reconciliation_required: &AtomicBool,
    sender: &mpsc::Sender<SettingsSaveWorkerMessage>,
    repaint: &egui::Context,
    timeout: Duration,
) -> Result<SettingsSaveCompletion, String> {
    use crate::hotkey::reconfiguration::HotkeyTerminalDisposition as Disposition;
    match cancel_and_resolve(
        ticket,
        source,
        reconciliation_required,
        sender,
        repaint,
        timeout,
        "Hotkey preparation cancellation is awaiting confirmation; this save remains pending.",
    ) {
        Ok(Disposition::ConfirmedNotAppliedOrRestored(error)) => {
            reconciliation_required.store(false, std::sync::atomic::Ordering::Release);
            Err(format!(
                "settings save cancelled before persistence: {error}"
            ))
        }
        Ok(Disposition::Applied(applied)) => {
            reconciliation_required.store(true, std::sync::atomic::Ordering::Release);
            Err(format!(
                "settings save was cancelled before persistence, but the runtime reports request {} Applied; runtime and disk need reconciliation",
                applied.request_id.get()
            ))
        }
        Ok(Disposition::RestorationUnavailable(error)) => {
            reconciliation_required.store(true, std::sync::atomic::Ordering::Release);
            Err(format!(
                "settings save was cancelled before persistence, but the runtime could not confirm restoration: {error}"
            ))
        }
        Err(error) => {
            reconciliation_required.store(true, std::sync::atomic::Ordering::Release);
            Err(format!(
                "settings save cancellation before persistence could not be confirmed: {error}"
            ))
        }
    }
}

fn cancel_and_resolve(
    ticket: &mut crate::hotkey::reconfiguration::HotkeyReconfigurationTicket,
    source: SettingsSaveSource,
    reconciliation_required: &AtomicBool,
    sender: &mpsc::Sender<SettingsSaveWorkerMessage>,
    repaint: &egui::Context,
    timeout: Duration,
    pending_message: &str,
) -> Result<crate::hotkey::reconfiguration::HotkeyTerminalDisposition, HotkeyReconfigurationError> {
    if let Some(terminal) = ticket.try_terminal_disposition() {
        return terminal;
    }
    let _ = ticket.cancel();
    if let Some(terminal) = ticket.try_terminal_disposition() {
        return terminal;
    }
    match ticket.wait_terminal_disposition(timeout) {
        Ok(terminal) => Ok(terminal),
        Err(HotkeyReconfigurationError::TimedOut) => {
            reconciliation_required.store(true, std::sync::atomic::Ordering::Release);
            let _ = sender.send(SettingsSaveWorkerMessage::ReconciliationRequired {
                source,
                message: pending_message.to_owned(),
            });
            repaint.request_repaint();
            repaint.request_repaint_of(egui::ViewportId::ROOT);
            loop {
                match ticket.wait_terminal_disposition(TERMINAL_TIMEOUT) {
                    Ok(terminal) => return Ok(terminal),
                    Err(HotkeyReconfigurationError::TimedOut) => continue,
                    Err(error) => return Err(error),
                }
            }
        }
        Err(error) => Err(error),
    }
}

fn format_resolution_error(
    operation: &str,
    error: HotkeyReconfigurationError,
    terminal: &Result<
        crate::hotkey::reconfiguration::HotkeyTerminalDisposition,
        HotkeyReconfigurationError,
    >,
) -> String {
    use crate::hotkey::reconfiguration::HotkeyTerminalDisposition as Disposition;
    match terminal {
        Ok(Disposition::Applied(applied)) => format!(
            "{operation}: {error}; the runtime reports request {} Applied despite no persisted candidate",
            applied.request_id.get()
        ),
        Ok(Disposition::ConfirmedNotAppliedOrRestored(terminal)) => {
            format!("{operation}: {error}; runtime confirmed the prior route: {terminal}")
        }
        Ok(Disposition::RestorationUnavailable(terminal)) => {
            format!("{operation}: {error}; runtime could not restore its prior route: {terminal}")
        }
        Err(terminal) => {
            format!("{operation}: {error}; runtime state could not be confirmed: {terminal}")
        }
    }
}

fn resolution_description(
    terminal: &Result<
        crate::hotkey::reconfiguration::HotkeyTerminalDisposition,
        HotkeyReconfigurationError,
    >,
) -> String {
    use crate::hotkey::reconfiguration::HotkeyTerminalDisposition as Disposition;
    match terminal {
        Ok(Disposition::Applied(applied)) => format!(
            "the runtime reports request {} Applied despite persistence failure",
            applied.request_id.get()
        ),
        Ok(Disposition::ConfirmedNotAppliedOrRestored(error)) => {
            format!("the prior route was restored ({error})")
        }
        Ok(Disposition::RestorationUnavailable(error)) => {
            format!("the prior route could not be restored ({error})")
        }
        Err(error) => format!("runtime state could not be confirmed ({error})"),
    }
}

fn resolve_persisted_failure(
    path: &str,
    intent: &SettingsSaveIntent,
    before: &Settings,
    written: &Settings,
    ticket: &crate::hotkey::reconfiguration::HotkeyReconfigurationTicket,
    reconciliation_required: &AtomicBool,
    operation_error: String,
    terminal: Result<
        crate::hotkey::reconfiguration::HotkeyTerminalDisposition,
        HotkeyReconfigurationError,
    >,
) -> Result<SettingsSaveCompletion, String> {
    use crate::hotkey::reconfiguration::HotkeyTerminalDisposition as Disposition;
    match terminal {
        Ok(Disposition::Applied(applied)) if applied.request_id == ticket.request_id() => {
            reconciliation_required.store(false, std::sync::atomic::Ordering::Release);
            Ok(finish_applied(path, intent, written, applied))
        }
        Ok(Disposition::Applied(applied)) => {
            reconciliation_required.store(true, std::sync::atomic::Ordering::Release);
            Err(format!(
                "{operation_error}; runtime acknowledged unexpected request {} instead of {}; persisted fields were retained for reconciliation",
                applied.request_id.get(),
                ticket.request_id().get()
            ))
        }
        Ok(Disposition::ConfirmedNotAppliedOrRestored(runtime_error)) => {
            match rollback_after_failure(path, intent, before, written, operation_error) {
                Ok(message) => {
                    reconciliation_required.store(false, std::sync::atomic::Ordering::Release);
                    Err(format!(
                        "{message}; runtime confirmed the prior route: {runtime_error}"
                    ))
                }
                Err(error) => {
                    reconciliation_required.store(true, std::sync::atomic::Ordering::Release);
                    Err(error)
                }
            }
        }
        Ok(Disposition::RestorationUnavailable(runtime_error)) => {
            reconciliation_required.store(true, std::sync::atomic::Ordering::Release);
            Err(format!(
                "{operation_error}; runtime could not confirm restoration ({runtime_error}), so persisted fields were retained for reconciliation"
            ))
        }
        Err(transport_error) => {
            reconciliation_required.store(true, std::sync::atomic::Ordering::Release);
            Err(format!(
                "{operation_error}; no authoritative hotkey terminal result arrived ({transport_error}), so persisted fields were retained for reconciliation"
            ))
        }
    }
}

fn rollback_after_failure(
    path: &str,
    intent: &SettingsSaveIntent,
    before: &Settings,
    written: &Settings,
    operation_error: String,
) -> Result<String, String> {
    let mut conflicts = Vec::new();
    let rollback = Settings::update(path, |latest| {
        conflicts = intent.rollback_owned(latest, before, written);
        Ok(())
    });
    match rollback {
        Ok(_) if conflicts.is_empty() => Ok(format!(
            "{operation_error}; persisted fields owned by this save were restored"
        )),
        Ok(_) => Err(format!(
            "{operation_error}; concurrent writes prevented rollback of owned fields: {}",
            conflicts.join(", ")
        )),
        Err(error) => Err(format!(
            "{operation_error}; restoring this save's persisted fields failed: {error}"
        )),
    }
}

fn persist_launcher_close_preferences(
    path: &str,
    preferences: &LauncherClosePreferences,
) -> anyhow::Result<()> {
    crate::gui::persist_launcher_close_preferences(
        path,
        preferences.window_size,
        preferences.pinned_panels.clone(),
        preferences.clipboard_modify_dialog_size,
    )?;
    if let Some(ui_preferences) = &preferences.file_search_ui_preferences {
        Settings::update(path, |settings| {
            let mut file_search: crate::file_search::settings::FileSearchSettings = settings
                .plugin_settings
                .get("file_search")
                .and_then(|value| serde_json::from_value(value.clone()).ok())
                .unwrap_or_default();
            file_search.ui_preferences = ui_preferences.clone();
            settings
                .plugin_settings
                .insert("file_search".into(), serde_json::to_value(file_search)?);
            Ok(())
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hotkey::reconfiguration::{
        AppliedHotkeyReconfiguration, HotkeyRuntimeGeneration, HotkeyRuntimeMessage,
        PreparedHotkeyReconfiguration,
    };
    use std::path::Path;
    use std::thread::JoinHandle;
    use std::time::Instant;

    type TestSaveResult = (Result<SettingsSaveCompletion, String>, bool);

    fn spawn_save<F>(
        path: String,
        intent: SettingsSaveIntent,
        source: SettingsSaveSource,
        prepare: F,
        terminal_timeout: Duration,
        cancellation_timeout: Duration,
    ) -> (
        JoinHandle<TestSaveResult>,
        Receiver<SettingsSaveWorkerMessage>,
    )
    where
        F: FnOnce(
                Settings,
            ) -> Result<
                crate::hotkey::reconfiguration::HotkeyReconfigurationTicket,
                HotkeyReconfigurationError,
            > + Send
            + 'static,
    {
        let (sender, receiver) = mpsc::channel();
        let handle = thread::spawn(move || {
            let reconciliation = AtomicBool::new(false);
            let result = run_settings_save(
                &path,
                &intent,
                source,
                &AtomicBool::new(false),
                &reconciliation,
                &sender,
                &egui::Context::default(),
                prepare,
                Duration::from_secs(2),
                terminal_timeout,
                cancellation_timeout,
            );
            (
                result,
                reconciliation.load(std::sync::atomic::Ordering::Acquire),
            )
        });
        (handle, receiver)
    }

    fn next_runtime_message(
        receiver: &crate::hotkey::reconfiguration::HotkeyReconfigurationReceiver,
    ) -> HotkeyRuntimeMessage {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            match receiver.try_recv() {
                Ok(Some(message)) => return message,
                Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(2)),
                Ok(None) => panic!("timed out waiting for hotkey request"),
                Err(error) => panic!("hotkey request receiver disconnected: {error}"),
            }
        }
    }

    fn complete_prepare(
        request: crate::hotkey::reconfiguration::HotkeyPreparationRequest,
        generation: u64,
    ) {
        let prepared = PreparedHotkeyReconfiguration {
            request_id: request.request_id(),
            generation: HotkeyRuntimeGeneration(generation),
            shared_invocation: false,
        };
        request.complete(Ok(prepared)).unwrap();
    }

    fn save_path(root: &Path) -> String {
        root.join("settings.json").to_string_lossy().into_owned()
    }

    fn intent_with_hotkey(baseline: &Settings, hotkey: &str) -> SettingsSaveIntent {
        let mut desired = baseline.clone();
        desired.hotkey = Some(hotkey.to_owned());
        SettingsSaveIntent::general(desired, baseline)
    }

    fn test_app(root: &Path, settings: Settings) -> (egui::Context, LauncherApp) {
        let context = egui::Context::default();
        let settings_path = save_path(root);
        let app = LauncherApp::new(
            &context,
            Arc::new(Vec::new()),
            0,
            crate::plugin::PluginManager::new_inert_for_test(),
            root.join("actions.json").to_string_lossy().into_owned(),
            settings_path,
            settings,
            None,
            None,
            Some(HashSet::new()),
            Some(HashMap::new()),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        );
        (context, app)
    }

    fn drain_until_complete(app: &mut LauncherApp, context: &egui::Context) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.settings_save_pending() && Instant::now() < deadline {
            app.drain_settings_save(context);
            thread::sleep(Duration::from_millis(2));
        }
        app.drain_settings_save(context);
        assert!(!app.settings_save_pending(), "settings save did not finish");
    }

    #[test]
    fn general_intent_changes_only_editor_owned_fields_and_plugin_keys() {
        let mut baseline = Settings::default();
        baseline.theme.mode = crate::settings::ThemeMode::Light;
        baseline.enabled_plugins = Some(HashSet::from(["old".into()]));
        baseline
            .plugin_settings
            .insert("one".into(), serde_json::json!({"v": 1}));
        baseline
            .plugin_settings
            .insert("unrelated".into(), serde_json::json!({"x": 1}));
        let mut desired = baseline.clone();
        desired.hotkey = Some("F7".into());
        desired.show_launcher_icon = false;
        desired.show_launcher_title = false;
        desired
            .plugin_settings
            .insert("one".into(), serde_json::json!({"v": 2}));
        let intent = SettingsSaveIntent::general(desired, &baseline);

        let mut latest = baseline.clone();
        latest.theme.mode = crate::settings::ThemeMode::Dark;
        latest.enabled_plugins = Some(HashSet::from(["new".into()]));
        latest
            .plugin_settings
            .insert("unrelated".into(), serde_json::json!({"x": 2}));
        latest.mkmacro_properties.master_open = true;
        latest.mkmacro_properties.focus_steps = true;
        intent.apply(&mut latest);

        assert_eq!(latest.hotkey.as_deref(), Some("F7"));
        assert!(!latest.show_launcher_icon);
        assert!(!latest.show_launcher_title);
        assert_eq!(latest.theme.mode, crate::settings::ThemeMode::Dark);
        assert_eq!(latest.enabled_plugins, Some(HashSet::from(["new".into()])));
        assert_eq!(latest.plugin_settings["one"], serde_json::json!({"v": 2}));
        assert_eq!(
            latest.plugin_settings["unrelated"],
            serde_json::json!({"x": 2})
        );
        assert!(latest.mkmacro_properties.master_open);
        assert!(latest.mkmacro_properties.focus_steps);
    }

    #[test]
    fn paste_preferences_follow_save_ownership_rollback_and_acknowledgement() {
        let mut before = Settings::default();
        before.paste_sequence.next_hotkey = Some("Alt+F8".into());
        let mut desired = before.clone();
        desired.paste_sequence.next_hotkey = None;
        desired.paste_sequence.tooltip_enabled = false;
        let intent = SettingsSaveIntent::general(desired.clone(), &before);
        let mut written = before.clone();
        intent.apply(&mut written);
        assert_eq!(written.paste_sequence, desired.paste_sequence);

        let mut latest = written.clone();
        latest.note_more_limit += 1;
        let conflicts = intent.rollback_owned(&mut latest, &before, &written);
        assert!(conflicts.is_empty());
        assert_eq!(latest.paste_sequence, before.paste_sequence);
        assert!(latest.note_more_limit > before.note_more_limit);

        latest = written.clone();
        latest.paste_sequence.cancel_hotkey = Some("Alt+F10".into());
        assert_eq!(
            intent.rollback_owned(&mut latest, &before, &written),
            vec!["paste_sequence"]
        );
        assert_eq!(
            latest.paste_sequence.cancel_hotkey.as_deref(),
            Some("Alt+F10")
        );
        assert!(intent.verify_invocation_baseline(&latest).is_err());

        let selection = SettingsSaveIntent::plugin_selection(None, None);
        assert!(
            selection
                .changed_owned_fields(&latest, &written)
                .contains(&"paste_sequence".into())
        );
        selection.overlay_applied(&mut latest, &written);
        assert_eq!(latest.paste_sequence, written.paste_sequence);
    }

    #[test]
    fn rollback_restores_owned_fields_and_plugin_keys_without_replacing_latest_map() {
        let before = Settings::default();
        let mut desired = before.clone();
        desired.hotkey = Some("F7".into());
        desired.show_launcher_icon = false;
        desired.show_launcher_title = false;
        desired
            .plugin_settings
            .insert("owned".into(), serde_json::json!({"v": 2}));
        let intent = SettingsSaveIntent::general(desired.clone(), &before);
        let mut written = before.clone();
        intent.apply(&mut written);
        let mut latest = written.clone();
        latest.theme.mode = crate::settings::ThemeMode::Dark;
        latest
            .plugin_settings
            .insert("independent".into(), serde_json::json!(true));

        let conflicts = intent.rollback_owned(&mut latest, &before, &written);

        assert!(conflicts.is_empty());
        assert_eq!(latest.hotkey, before.hotkey);
        assert!(latest.show_launcher_icon);
        assert!(latest.show_launcher_title);
        assert!(!latest.plugin_settings.contains_key("owned"));
        assert_eq!(
            latest.plugin_settings["independent"],
            serde_json::json!(true)
        );
        assert_eq!(latest.theme.mode, crate::settings::ThemeMode::Dark);
    }

    #[test]
    fn rollback_preserves_concurrent_change_to_same_owned_plugin_key() {
        let before = Settings::default();
        let mut desired = before.clone();
        desired
            .plugin_settings
            .insert("screen_draw".into(), serde_json::json!({"v": 2}));
        let intent = SettingsSaveIntent::general(desired.clone(), &before);
        let mut written = before.clone();
        intent.apply(&mut written);
        let mut latest = written.clone();
        latest
            .plugin_settings
            .insert("screen_draw".into(), serde_json::json!({"v": 3}));

        let conflicts = intent.rollback_owned(&mut latest, &before, &written);

        assert_eq!(
            latest.plugin_settings["screen_draw"],
            serde_json::json!({"v": 3})
        );
        assert_eq!(conflicts, ["plugin_settings.screen_draw"]);
    }

    #[test]
    fn plugin_selection_intent_preserves_latest_other_settings_and_plugin_settings() {
        let mut latest = Settings::default();
        latest.theme.mode = crate::settings::ThemeMode::Dark;
        latest
            .plugin_settings
            .insert("file_search".into(), serde_json::json!({"limit": 2}));
        let intent = SettingsSaveIntent::plugin_selection(
            Some(HashSet::from(["screen_draw".into()])),
            Some(HashMap::from([(
                "screen_draw".into(),
                vec!["global-shortcuts".into()],
            )])),
        );

        intent.apply(&mut latest);

        assert_eq!(
            latest.enabled_plugins,
            Some(HashSet::from(["screen_draw".into()]))
        );
        assert_eq!(latest.theme.mode, crate::settings::ThemeMode::Dark);
        assert_eq!(
            latest.plugin_settings["file_search"],
            serde_json::json!({"limit": 2})
        );
    }

    #[test]
    fn preparation_failure_does_not_change_settings_bytes() {
        let directory = tempfile::tempdir().unwrap();
        let path = save_path(directory.path());
        let mut baseline = Settings::default();
        baseline.hotkey = Some("F2".into());
        baseline.save(&path).unwrap();
        let before_bytes = std::fs::read(&path).unwrap();
        let intent = intent_with_hotkey(&baseline, "F8");
        let (client, owner, _wake) = crate::hotkey::reconfiguration::test_reconfiguration_runtime();
        let client_for_worker = client.clone();
        let (worker, _messages) = spawn_save(
            path.clone(),
            intent,
            SettingsSaveSource::General,
            move |settings| client_for_worker.prepare(settings),
            Duration::from_secs(2),
            Duration::from_secs(2),
        );

        let HotkeyRuntimeMessage::Prepare(request) = next_runtime_message(&owner) else {
            panic!("expected preparation request")
        };
        request
            .complete(Err(HotkeyReconfigurationError::Validation(
                "injected route conflict".into(),
            )))
            .unwrap();

        let (result, reconciliation_required) = worker.join().unwrap();
        assert!(result.unwrap_err().contains("injected route conflict"));
        assert!(!reconciliation_required);
        assert_eq!(std::fs::read(&path).unwrap(), before_bytes);
    }

    #[test]
    fn persistence_failure_cancels_prepared_route_before_returning() {
        let directory = tempfile::tempdir().unwrap();
        let settings_directory = directory.path().join("settings");
        std::fs::create_dir(&settings_directory).unwrap();
        let path = settings_directory
            .join("settings.json")
            .to_string_lossy()
            .into_owned();
        let mut baseline = Settings::default();
        baseline.hotkey = Some("F2".into());
        baseline.save(&path).unwrap();
        let intent = intent_with_hotkey(&baseline, "F8");
        let (client, owner, _wake) = crate::hotkey::reconfiguration::test_reconfiguration_runtime();
        let client_for_worker = client.clone();
        let directory_for_prepare = settings_directory.clone();
        let (worker, _messages) = spawn_save(
            path,
            intent,
            SettingsSaveSource::General,
            move |settings| {
                std::fs::remove_dir_all(&directory_for_prepare).unwrap();
                std::fs::write(&directory_for_prepare, b"blocked settings directory").unwrap();
                client_for_worker.prepare(settings)
            },
            Duration::from_secs(2),
            Duration::from_secs(2),
        );
        let HotkeyRuntimeMessage::Prepare(request) = next_runtime_message(&owner) else {
            panic!("expected hotkey preparation")
        };
        complete_prepare(request, 82);
        let HotkeyRuntimeMessage::Cancel(cancellation) = next_runtime_message(&owner) else {
            panic!("persistence failure should cancel the dormant route")
        };
        cancellation
            .complete_restored(HotkeyReconfigurationError::TransactionCancelled)
            .unwrap();

        let (result, reconciliation_required) = worker.join().unwrap();
        assert!(result.unwrap_err().contains("settings persistence failed"));
        assert!(!reconciliation_required);
        assert!(settings_directory.is_file());
    }

    #[test]
    fn acknowledged_apply_publication_merges_post_persistence_unrelated_writes() {
        let directory = tempfile::tempdir().unwrap();
        let path = save_path(directory.path());
        let mut baseline = Settings::default();
        baseline.hotkey = Some("F2".into());
        let mut screen_draw = crate::screen_draw::ScreenDrawSettings::default();
        screen_draw.emergency_hotkey =
            crate::screen_draw::HotkeyChord::from_unchecked("Ctrl+Shift+F12");
        baseline.plugin_settings.insert(
            "screen_draw".into(),
            serde_json::to_value(screen_draw).unwrap(),
        );
        baseline.save(&path).unwrap();
        let intent = intent_with_hotkey(&baseline, "F8");
        let (client, owner, _wake) = crate::hotkey::reconfiguration::test_reconfiguration_runtime();
        let client_for_worker = client.clone();
        let (worker, _messages) = spawn_save(
            path.clone(),
            intent,
            SettingsSaveSource::General,
            move |settings| client_for_worker.prepare(settings),
            Duration::from_secs(2),
            Duration::from_secs(2),
        );

        let HotkeyRuntimeMessage::Prepare(request) = next_runtime_message(&owner) else {
            panic!("expected prepared hotkey route")
        };
        complete_prepare(request, 71);
        let HotkeyRuntimeMessage::Apply(application) = next_runtime_message(&owner) else {
            panic!("expected persisted route apply")
        };
        assert_eq!(application.settings().hotkey.as_deref(), Some("F8"));
        Settings::update(&path, |latest| {
            latest.theme.mode = crate::settings::ThemeMode::Dark;
            latest.mkmacro_properties.master_open = true;
            latest.mkmacro_properties.focus_steps = true;
            let mut screen_draw: crate::screen_draw::ScreenDrawSettings =
                serde_json::from_value(latest.plugin_settings["screen_draw"].clone())?;
            screen_draw.emergency_hotkey =
                crate::screen_draw::HotkeyChord::from_unchecked("Ctrl+Shift+F11");
            screen_draw.default_thickness = 12.0;
            latest
                .plugin_settings
                .insert("screen_draw".into(), serde_json::to_value(screen_draw)?);
            Ok(())
        })
        .unwrap();
        let applied = AppliedHotkeyReconfiguration {
            request_id: application.request_id(),
            generation: application.generation(),
        };
        application.complete_applied(applied).unwrap();

        let (result, reconciliation_required) = worker.join().unwrap();
        let completion = result.unwrap();
        assert!(!reconciliation_required);
        assert_eq!(completion.settings.hotkey.as_deref(), Some("F8"));
        assert_eq!(
            completion.settings.theme.mode,
            crate::settings::ThemeMode::Dark
        );
        assert!(completion.settings.mkmacro_properties.master_open);
        assert!(completion.settings.mkmacro_properties.focus_steps);
        assert!(
            completion
                .warning
                .as_deref()
                .is_some_and(|warning| warning.contains("screen_draw.emergency_hotkey"))
        );
        let published_screen_draw: crate::screen_draw::ScreenDrawSettings =
            serde_json::from_value(completion.settings.plugin_settings["screen_draw"].clone())
                .unwrap();
        assert_eq!(
            published_screen_draw.emergency_hotkey.as_str(),
            "Ctrl+Shift+F12"
        );
        assert_eq!(published_screen_draw.default_thickness, 12.0);
        let saved = Settings::load(&path).unwrap();
        assert_eq!(saved.hotkey.as_deref(), Some("F8"));
        assert_eq!(saved.theme.mode, crate::settings::ThemeMode::Dark);
        assert!(saved.mkmacro_properties.master_open);
        assert!(saved.mkmacro_properties.focus_steps);
        let saved_screen_draw: crate::screen_draw::ScreenDrawSettings =
            serde_json::from_value(saved.plugin_settings["screen_draw"].clone()).unwrap();
        assert_eq!(
            saved_screen_draw.emergency_hotkey.as_str(),
            "Ctrl+Shift+F11"
        );
        assert_eq!(saved_screen_draw.default_thickness, 12.0);
    }

    #[test]
    fn plugin_selection_publication_keeps_acknowledged_hotkey_after_later_route_write() {
        let directory = tempfile::tempdir().unwrap();
        let path = save_path(directory.path());
        let mut baseline = Settings::default();
        baseline.hotkey = Some("F2".into());
        baseline.save(&path).unwrap();
        let intent = SettingsSaveIntent::plugin_selection(
            Some(HashSet::from(["screen_draw".into()])),
            Some(HashMap::from([(
                "screen_draw".into(),
                vec!["global-shortcuts".into()],
            )])),
        );
        let (client, owner, _wake) = crate::hotkey::reconfiguration::test_reconfiguration_runtime();
        let client_for_worker = client.clone();
        let (worker, _messages) = spawn_save(
            path.clone(),
            intent,
            SettingsSaveSource::PluginSelection,
            move |settings| client_for_worker.prepare(settings),
            Duration::from_secs(2),
            Duration::from_secs(2),
        );
        let HotkeyRuntimeMessage::Prepare(request) = next_runtime_message(&owner) else {
            panic!("expected plugin-selection preparation")
        };
        complete_prepare(request, 79);
        let HotkeyRuntimeMessage::Apply(application) = next_runtime_message(&owner) else {
            panic!("expected plugin-selection Apply")
        };
        Settings::update(&path, |latest| {
            latest.hotkey = Some("F9".into());
            Ok(())
        })
        .unwrap();
        let applied = AppliedHotkeyReconfiguration {
            request_id: application.request_id(),
            generation: application.generation(),
        };
        application.complete_applied(applied).unwrap();

        let (result, reconciliation_required) = worker.join().unwrap();
        let completion = result.unwrap();
        assert!(!reconciliation_required);
        assert_eq!(completion.settings.hotkey.as_deref(), Some("F2"));
        assert!(
            completion
                .warning
                .as_deref()
                .is_some_and(|warning| warning.contains("hotkey"))
        );
        assert_eq!(Settings::load(&path).unwrap().hotkey.as_deref(), Some("F9"));
    }

    #[test]
    fn confirmed_runtime_failure_rolls_back_owned_fields_and_keeps_concurrent_values() {
        let directory = tempfile::tempdir().unwrap();
        let path = save_path(directory.path());
        let mut baseline = Settings::default();
        baseline.hotkey = Some("F2".into());
        baseline.save(&path).unwrap();
        let intent = intent_with_hotkey(&baseline, "F8");
        let (client, owner, _wake) = crate::hotkey::reconfiguration::test_reconfiguration_runtime();
        let client_for_worker = client.clone();
        let (worker, _messages) = spawn_save(
            path.clone(),
            intent,
            SettingsSaveSource::General,
            move |settings| client_for_worker.prepare(settings),
            Duration::from_secs(2),
            Duration::from_secs(2),
        );
        let HotkeyRuntimeMessage::Prepare(request) = next_runtime_message(&owner) else {
            panic!("expected prepared hotkey route")
        };
        complete_prepare(request, 72);
        let HotkeyRuntimeMessage::Apply(application) = next_runtime_message(&owner) else {
            panic!("expected persisted route apply")
        };
        Settings::update(&path, |latest| {
            latest.theme.mode = crate::settings::ThemeMode::Dark;
            latest.mkmacro_properties.master_open = true;
            latest.mkmacro_properties.focus_steps = true;
            Ok(())
        })
        .unwrap();
        application
            .complete_restored(HotkeyReconfigurationError::RuntimeTransition(
                "injected failure".into(),
            ))
            .unwrap();

        let (result, reconciliation_required) = worker.join().unwrap();
        assert!(result.is_err());
        assert!(!reconciliation_required);
        let saved = Settings::load(&path).unwrap();
        assert_eq!(saved.hotkey, baseline.hotkey);
        assert_eq!(saved.theme.mode, crate::settings::ThemeMode::Dark);
        assert!(saved.mkmacro_properties.master_open);
        assert!(saved.mkmacro_properties.focus_steps);
    }

    #[test]
    fn rollback_conflict_preserves_later_owned_field_write_and_requests_reconciliation() {
        let directory = tempfile::tempdir().unwrap();
        let path = save_path(directory.path());
        let mut baseline = Settings::default();
        baseline.hotkey = Some("F2".into());
        baseline.save(&path).unwrap();
        let intent = intent_with_hotkey(&baseline, "F8");
        let (client, owner, _wake) = crate::hotkey::reconfiguration::test_reconfiguration_runtime();
        let client_for_worker = client.clone();
        let (worker, _messages) = spawn_save(
            path.clone(),
            intent,
            SettingsSaveSource::General,
            move |settings| client_for_worker.prepare(settings),
            Duration::from_secs(2),
            Duration::from_secs(2),
        );
        let HotkeyRuntimeMessage::Prepare(request) = next_runtime_message(&owner) else {
            panic!("expected hotkey preparation")
        };
        complete_prepare(request, 80);
        let HotkeyRuntimeMessage::Apply(application) = next_runtime_message(&owner) else {
            panic!("expected route Apply")
        };
        Settings::update(&path, |latest| {
            latest.hotkey = Some("F9".into());
            Ok(())
        })
        .unwrap();
        application
            .complete_restored(HotkeyReconfigurationError::RuntimeTransition(
                "injected failure".into(),
            ))
            .unwrap();

        let (result, reconciliation_required) = worker.join().unwrap();
        assert!(
            result
                .unwrap_err()
                .contains("concurrent writes prevented rollback of owned fields")
        );
        assert!(reconciliation_required);
        assert_eq!(Settings::load(&path).unwrap().hotkey.as_deref(), Some("F9"));
    }

    #[test]
    fn failed_rollback_reports_reconciliation_when_settings_storage_disappears() {
        let directory = tempfile::tempdir().unwrap();
        let settings_directory = directory.path().join("settings");
        std::fs::create_dir(&settings_directory).unwrap();
        let path = settings_directory
            .join("settings.json")
            .to_string_lossy()
            .into_owned();
        let mut baseline = Settings::default();
        baseline.hotkey = Some("F2".into());
        baseline.save(&path).unwrap();
        let intent = intent_with_hotkey(&baseline, "F8");
        let (client, owner, _wake) = crate::hotkey::reconfiguration::test_reconfiguration_runtime();
        let client_for_worker = client.clone();
        let (worker, _messages) = spawn_save(
            path.clone(),
            intent,
            SettingsSaveSource::General,
            move |settings| client_for_worker.prepare(settings),
            Duration::from_secs(2),
            Duration::from_secs(2),
        );
        let HotkeyRuntimeMessage::Prepare(request) = next_runtime_message(&owner) else {
            panic!("expected hotkey preparation")
        };
        complete_prepare(request, 81);
        let HotkeyRuntimeMessage::Apply(application) = next_runtime_message(&owner) else {
            panic!("expected route Apply")
        };
        std::fs::remove_dir_all(&settings_directory).unwrap();
        std::fs::write(&settings_directory, b"blocked settings directory").unwrap();
        application
            .complete_restored(HotkeyReconfigurationError::RuntimeTransition(
                "injected failure".into(),
            ))
            .unwrap();

        let (result, reconciliation_required) = worker.join().unwrap();
        assert!(
            result
                .unwrap_err()
                .contains("restoring this save's persisted fields failed")
        );
        assert!(reconciliation_required);
        assert!(settings_directory.is_file());
    }

    #[test]
    fn apply_never_enqueued_rolls_back_owned_fields_and_preserves_unrelated_write() {
        let directory = tempfile::tempdir().unwrap();
        let path = save_path(directory.path());
        let mut baseline = Settings::default();
        baseline.hotkey = Some("F2".into());
        baseline.save(&path).unwrap();
        let intent = intent_with_hotkey(&baseline, "F8");
        let (client, owner, _wake) = crate::hotkey::reconfiguration::test_reconfiguration_runtime();
        let client_for_worker = client.clone();
        let path_for_prepare = path.clone();
        let (worker, _messages) = spawn_save(
            path.clone(),
            intent,
            SettingsSaveSource::General,
            move |settings| {
                Settings::update(&path_for_prepare, |latest| {
                    latest.theme.mode = crate::settings::ThemeMode::Dark;
                    Ok(())
                })
                .unwrap();
                client_for_worker.prepare(settings)
            },
            Duration::from_secs(2),
            Duration::from_secs(2),
        );

        let HotkeyRuntimeMessage::Prepare(request) = next_runtime_message(&owner) else {
            panic!("expected hotkey preparation")
        };
        drop(owner);
        complete_prepare(request, 78);

        let (result, reconciliation_required) = worker.join().unwrap();
        assert!(
            result
                .unwrap_err()
                .contains("hotkey apply was not enqueued"),
            "failed save should report its rejected Apply submission"
        );
        assert!(!reconciliation_required);
        let saved = Settings::load(&path).unwrap();
        assert_eq!(saved.hotkey, baseline.hotkey);
        assert_eq!(saved.theme.mode, crate::settings::ThemeMode::Dark);
    }

    #[test]
    fn apply_timeout_waits_for_authoritative_applied_receipt_before_rollback() {
        let directory = tempfile::tempdir().unwrap();
        let path = save_path(directory.path());
        let mut baseline = Settings::default();
        baseline.hotkey = Some("F2".into());
        baseline.save(&path).unwrap();
        let intent = intent_with_hotkey(&baseline, "F8");
        let (client, owner, _wake) = crate::hotkey::reconfiguration::test_reconfiguration_runtime();
        let client_for_worker = client.clone();
        let (worker, messages) = spawn_save(
            path.clone(),
            intent,
            SettingsSaveSource::General,
            move |settings| client_for_worker.prepare(settings),
            Duration::from_millis(10),
            Duration::from_millis(100),
        );
        let HotkeyRuntimeMessage::Prepare(request) = next_runtime_message(&owner) else {
            panic!("expected prepared hotkey route")
        };
        complete_prepare(request, 73);
        let HotkeyRuntimeMessage::Apply(application) = next_runtime_message(&owner) else {
            panic!("expected persisted route apply")
        };
        let HotkeyRuntimeMessage::Cancel(mut cancellation) = next_runtime_message(&owner) else {
            panic!("expected timeout cancellation")
        };
        cancellation.defer_terminal();
        assert!(matches!(
            messages.recv_timeout(Duration::from_secs(1)).unwrap(),
            SettingsSaveWorkerMessage::ReconciliationRequired { .. }
        ));
        let applied = AppliedHotkeyReconfiguration {
            request_id: application.request_id(),
            generation: application.generation(),
        };
        application.complete_applied(applied).unwrap();

        let (result, reconciliation_required) = worker.join().unwrap();
        let completion = result.unwrap();
        assert!(!reconciliation_required);
        assert_eq!(completion.settings.hotkey.as_deref(), Some("F8"));
        assert_eq!(Settings::load(&path).unwrap().hotkey.as_deref(), Some("F8"));
    }

    #[test]
    fn invocation_baseline_is_checked_inside_settings_update_after_preparation() {
        let directory = tempfile::tempdir().unwrap();
        let path = save_path(directory.path());
        let mut baseline = Settings::default();
        baseline.hotkey = Some("F2".into());
        baseline.save(&path).unwrap();
        let intent = intent_with_hotkey(&baseline, "F8");
        let (client, owner, _wake) = crate::hotkey::reconfiguration::test_reconfiguration_runtime();
        let path_for_prepare = path.clone();
        let client_for_worker = client.clone();
        let (worker, _messages) = spawn_save(
            path.clone(),
            intent,
            SettingsSaveSource::General,
            move |settings| {
                Settings::update(&path_for_prepare, |latest| {
                    latest.hotkey = Some("F9".into());
                    Ok(())
                })
                .unwrap();
                client_for_worker.prepare(settings)
            },
            Duration::from_secs(2),
            Duration::from_secs(2),
        );
        let HotkeyRuntimeMessage::Prepare(request) = next_runtime_message(&owner) else {
            panic!("expected prepared hotkey route")
        };
        complete_prepare(request, 74);
        let HotkeyRuntimeMessage::Cancel(cancellation) = next_runtime_message(&owner) else {
            panic!("route baseline conflict should cancel the prepared route")
        };
        cancellation
            .complete_restored(HotkeyReconfigurationError::TransactionCancelled)
            .unwrap();

        let (result, reconciliation_required) = worker.join().unwrap();
        assert!(
            result
                .unwrap_err()
                .contains("changed since this editor was opened")
        );
        assert!(!reconciliation_required);
        assert_eq!(Settings::load(&path).unwrap().hotkey.as_deref(), Some("F9"));
        assert!(owner.try_recv().unwrap().is_none());
    }

    #[test]
    fn hidden_settings_editor_completes_and_a_second_save_uses_the_new_baseline() {
        let directory = tempfile::tempdir().unwrap();
        let path = save_path(directory.path());
        let mut baseline = Settings::default();
        baseline.hotkey = Some("F2".into());
        baseline.enable_toasts = false;
        baseline.mkmacro_properties.master_open = true;
        baseline.save(&path).unwrap();
        let (context, mut app) = test_app(directory.path(), baseline.clone());
        app.settings_editor.hotkey = "F7".into();
        app.settings_editor.show_launcher_icon = false;
        app.settings_editor.show_launcher_title = false;
        let mut first_desired = baseline.clone();
        first_desired.hotkey = Some("F7".into());
        first_desired.show_launcher_icon = false;
        first_desired.show_launcher_title = false;
        let (client, owner, _wake) = crate::hotkey::reconfiguration::test_reconfiguration_runtime();
        let first_client = client.clone();
        app.begin_settings_save_with_prepare(
            &context,
            SettingsSaveIntent::general(first_desired, &baseline),
            move |settings| first_client.prepare(settings),
        );
        let HotkeyRuntimeMessage::Prepare(request) = next_runtime_message(&owner) else {
            panic!("expected first preparation")
        };
        complete_prepare(request, 75);
        let HotkeyRuntimeMessage::Apply(application) = next_runtime_message(&owner) else {
            panic!("expected first apply")
        };
        let staged = Settings::load(&path).unwrap();
        assert!(!staged.show_launcher_icon && !staged.show_launcher_title);
        assert!(app.show_launcher_icon && app.show_launcher_title);
        app.show_settings = false;
        app.drain_settings_save(&context);
        assert_eq!(app.hotkey_str, baseline.hotkey);
        assert!(app.show_launcher_icon && app.show_launcher_title);
        assert_eq!(
            app.settings_editor.baseline_settings.hotkey,
            baseline.hotkey
        );
        app.show_settings = true;
        app.drain_settings_save(&context);
        assert_eq!(app.hotkey_str, baseline.hotkey);
        let applied = AppliedHotkeyReconfiguration {
            request_id: application.request_id(),
            generation: application.generation(),
        };
        application.complete_applied(applied).unwrap();
        drain_until_complete(&mut app, &context);
        assert_eq!(app.hotkey_str.as_deref(), Some("F7"));
        assert!(!app.show_launcher_icon && !app.show_launcher_title);
        assert_eq!(
            app.settings_editor.baseline_settings.hotkey.as_deref(),
            Some("F7")
        );
        assert!(!app.settings_editor.baseline_settings.show_launcher_icon);
        assert!(!app.settings_editor.baseline_settings.show_launcher_title);
        assert!(
            app.settings_editor
                .baseline_settings
                .mkmacro_properties
                .master_open
        );

        let second_baseline = app.settings_editor.baseline_settings.clone();
        app.settings_editor.hotkey = "F8".into();
        app.settings_editor.show_launcher_icon = true;
        app.settings_editor.show_launcher_title = true;
        let second_desired = app.settings_editor.to_settings(&second_baseline);
        let second_client = client.clone();
        app.begin_settings_save_with_prepare(
            &context,
            SettingsSaveIntent::general(second_desired, &second_baseline),
            move |settings| second_client.prepare(settings),
        );
        let HotkeyRuntimeMessage::Prepare(request) = next_runtime_message(&owner) else {
            panic!("expected second preparation")
        };
        complete_prepare(request, 76);
        let HotkeyRuntimeMessage::Apply(application) = next_runtime_message(&owner) else {
            panic!("expected second apply")
        };
        assert!(!app.show_launcher_icon && !app.show_launcher_title);
        let applied = AppliedHotkeyReconfiguration {
            request_id: application.request_id(),
            generation: application.generation(),
        };
        application.complete_applied(applied).unwrap();
        drain_until_complete(&mut app, &context);
        assert_eq!(app.hotkey_str.as_deref(), Some("F8"));
        assert!(app.show_launcher_icon && app.show_launcher_title);
        let persisted = Settings::load(&path).unwrap();
        let (_, relaunched_app) = test_app(directory.path(), persisted.clone());
        assert!(relaunched_app.show_launcher_icon && relaunched_app.show_launcher_title);
        assert!(persisted.mkmacro_properties.master_open);
        assert!(
            relaunched_app
                .mkmacro_dialog
                .mkmacro_properties_preferences
                .master_open
        );
        assert_eq!(
            app.settings_editor.baseline_settings.hotkey.as_deref(),
            Some("F8")
        );
        assert_eq!(Settings::load(&path).unwrap().hotkey.as_deref(), Some("F8"));
        app.settings_save_workers.join_all();
    }

    #[test]
    fn shutdown_closes_worker_admission_and_persists_deferred_close_preferences() {
        let directory = tempfile::tempdir().unwrap();
        let path = save_path(directory.path());
        let mut baseline = Settings::default();
        baseline.hotkey = Some("F2".into());
        baseline.window_size = Some((430, 310));
        baseline.save(&path).unwrap();
        let (context, mut app) = test_app(directory.path(), baseline.clone());
        app.window_size = (777, 555);
        let intent = intent_with_hotkey(&baseline, "F8");
        let (client, owner, _wake) = crate::hotkey::reconfiguration::test_reconfiguration_runtime();
        let client_for_worker = client.clone();
        let (continue_prepare, wait_for_prepare) = mpsc::channel();
        app.begin_settings_save_with_prepare(&context, intent, move |settings| {
            wait_for_prepare.recv().unwrap();
            client_for_worker.prepare(settings)
        });

        app.prepare_settings_save_shutdown();
        drop(owner);
        continue_prepare.send(()).unwrap();
        app.settings_save_workers.join_all();
        app.drain_settings_save(&context);

        let saved = Settings::load(&path).unwrap();
        assert_eq!(saved.window_size, Some((777, 555)));
        assert_eq!(saved.hotkey, baseline.hotkey);
        assert!(!app.settings_save_pending());
        assert!(
            app.settings_save_workers.spawn(|| {}).is_err(),
            "shutdown must reject new save workers"
        );
    }
}
