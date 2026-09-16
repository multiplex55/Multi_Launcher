#![windows_subsystem = "windows"]
#![allow(clippy::type_complexity)]

use multi_launcher::actions::{Action, load_startup_actions};
use multi_launcher::common::persistence::PersistenceError;
use multi_launcher::gui::LauncherApp;
use multi_launcher::hotkey::launcher_invocation::{
    InvocationConfig, LauncherInvocationService, PriorityOwner, RelatedAction, RelatedBinding,
    RouteHandoff, ServiceNotice, exclusive_owners, install_exclusive_wake,
};
use multi_launcher::hotkey::{HotkeyListener, HotkeyTrigger, parse_hotkey};
use multi_launcher::platform::{
    app_data::AppDataRoot,
    single_instance::{SingleInstanceAcquire, SingleInstanceGuard},
};
use multi_launcher::plugin::PluginManager;
use multi_launcher::radial::authoring::{
    AuthoringReply, AuthoringRequest, AuthoringResourceDemand, AuthoringSessionId,
    authoring_control_service_with_wake,
};
use multi_launcher::radial::control::{
    RadialControlMainEndpoint, RadialControlRequest, radial_control_service_with_wake, resolve_menu,
};
use multi_launcher::radial::controller::{ControllerEvent, RadialController};
use multi_launcher::radial::invocation::{
    InvocationEvent, InvocationIntent, LifecycleCancellation,
};
use multi_launcher::radial::item_input::compile_item_inputs;
use multi_launcher::radial::model::{InteractionMode, InvocationId, RadialDocument};
use multi_launcher::radial::store::{ExternalReloadOutcome, RadialStore};
use multi_launcher::radial::validation::validate as validate_radial_document;
use multi_launcher::radial::watch::RadialConfigWatcher;
use multi_launcher::screen_draw::{ScreenDrawRecoveryBridge, ScreenDrawSettings};
use multi_launcher::settings::Settings;
use multi_launcher::startup::{SettingsStartupDiagnostic, load_startup_preload};
use multi_launcher::visibility::{
    VisibilityToggleBatch, handle_visibility_toggle_batch, handle_visibility_trigger_with_owner,
};
use multi_launcher::{indexer, logging};

use eframe::{egui, icon_data};
use once_cell::sync::Lazy;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc::{Sender, channel},
};
use std::thread;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    path::PathBuf,
};

fn build_viewport_with_icon(settings: &Settings, icon_bytes: &[u8]) -> egui::ViewportBuilder {
    let (w, h) = settings.window_size.unwrap_or((400, 220));
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([w as f32, h as f32])
        .with_min_inner_size([320.0, 160.0])
        .with_visible(true);

    match icon_data::from_png_bytes(icon_bytes) {
        Ok(icon) => {
            viewport = viewport.with_icon(icon);
        }
        Err(err) => {
            tracing::warn!(
                ?err,
                "failed to decode launcher icon; continuing without custom icon"
            );
        }
    }

    if settings.always_on_top {
        viewport = viewport.with_always_on_top();
    }

    viewport
}

static RESTART_TX: Lazy<Mutex<Option<Sender<Settings>>>> = Lazy::new(|| Mutex::new(None));
static EVENT_TX: Lazy<Mutex<Option<Sender<()>>>> = Lazy::new(|| Mutex::new(None));

fn reserved_launcher_hotkeys(settings: &Settings) -> Vec<(String, String)> {
    let mut reserved = Vec::new();
    let launcher = settings
        .hotkey
        .as_deref()
        .filter(|hotkey| parse_hotkey(hotkey).is_some())
        .unwrap_or("F2");
    reserved.push(("launcher toggle".into(), launcher.into()));
    if let Some(hotkey) = settings
        .quit_hotkey
        .as_deref()
        .filter(|hotkey| parse_hotkey(hotkey).is_some())
    {
        reserved.push(("quit launcher".into(), hotkey.into()));
    }
    if let Some(hotkey) = settings
        .help_hotkey
        .as_deref()
        .filter(|hotkey| parse_hotkey(hotkey).is_some())
    {
        reserved.push(("help launcher".into(), hotkey.into()));
    }
    if let Ok(Some(hotkey)) = screen_draw_launch_hotkey_text(settings) {
        reserved.push(("launch Screen Draw".into(), hotkey.into()));
    }
    if let Ok(Some(hotkey)) = screen_draw_emergency_hotkey_text(settings) {
        reserved.push(("Screen Draw emergency".into(), hotkey));
    }
    reserved
}

fn configured_screen_draw_settings(settings: &Settings) -> Result<ScreenDrawSettings, String> {
    settings
        .plugin_settings
        .get("screen_draw")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|error| format!("invalid Screen Draw settings: {error}"))
        .map(|settings| settings.unwrap_or_default())
}

fn screen_draw_launch_hotkey_text(settings: &Settings) -> Result<Option<String>, String> {
    if !settings.plugin_settings.contains_key("screen_draw") {
        return Ok(None);
    }
    let screen_draw = configured_screen_draw_settings(settings)?;
    match screen_draw.launch_hotkey.as_ref() {
        Some(chord) if chord.is_valid() => {
            let parsed = parse_hotkey(chord.as_str()).expect("validated Screen Draw hotkey");
            let conflicts = [
                ("launcher toggle", Some(settings.hotkey())),
                ("quit launcher", settings.quit_hotkey()),
                ("help launcher", settings.help_hotkey()),
            ];
            if let Some((name, _)) = conflicts.into_iter().find(|(_, existing)| {
                existing.is_some_and(|existing| hotkeys_can_cofire(parsed, existing))
            }) {
                return Err(format!(
                    "Screen Draw launch hotkey '{}' conflicts with {name}; global launch hotkey disabled",
                    chord.as_str()
                ));
            }
            if screen_draw_emergency_hotkey_text(settings)
                .ok()
                .flatten()
                .and_then(|emergency| parse_hotkey(&emergency))
                .is_some_and(|emergency| hotkeys_can_cofire(parsed, emergency))
            {
                return Err(format!(
                    "Screen Draw launch hotkey '{}' conflicts with Screen Draw emergency; global launch hotkey disabled",
                    chord.as_str()
                ));
            }
            Ok(Some(chord.as_str().to_owned()))
        }
        Some(chord) => Err(format!(
            "invalid Screen Draw launch hotkey '{}'; global launch hotkey disabled",
            chord.as_str()
        )),
        None => Ok(None),
    }
}

fn screen_draw_emergency_hotkey_text(settings: &Settings) -> Result<Option<String>, String> {
    let screen_draw = configured_screen_draw_settings(settings)?;
    let chord = &screen_draw.emergency_hotkey;
    if !chord.is_valid() {
        return Err(format!(
            "invalid Screen Draw emergency hotkey '{}'; emergency hotkey disabled",
            chord.as_str()
        ));
    }
    let parsed = parse_hotkey(chord.as_str()).expect("validated Screen Draw emergency hotkey");
    let conflicts = [
        ("launcher toggle", Some(settings.hotkey())),
        ("quit launcher", settings.quit_hotkey()),
        ("help launcher", settings.help_hotkey()),
    ];
    if let Some((name, _)) = conflicts
        .into_iter()
        .find(|(_, existing)| existing.is_some_and(|existing| hotkeys_can_cofire(parsed, existing)))
    {
        return Err(format!(
            "Screen Draw emergency hotkey '{}' conflicts with {name}; emergency hotkey disabled",
            chord.as_str()
        ));
    }
    Ok(Some(chord.as_str().to_owned()))
}

fn hotkeys_can_cofire(
    left: multi_launcher::hotkey::Hotkey,
    right: multi_launcher::hotkey::Hotkey,
) -> bool {
    // HotkeyTrigger treats configured modifiers as required subsets: extra
    // modifiers remain accepted. Two chords with the same primary key can
    // therefore fire together under the union of their modifier sets.
    if left.key != right.key {
        return false;
    }
    let unmodified_caps_lock = |hotkey: multi_launcher::hotkey::Hotkey| {
        hotkey.key == multi_launcher::hotkey::Key::CapsLock
            && !hotkey.ctrl
            && !hotkey.shift
            && !hotkey.alt
            && !hotkey.alt_gr
            && !hotkey.win
    };
    let left_exact_caps_lock = unmodified_caps_lock(left);
    let right_exact_caps_lock = unmodified_caps_lock(right);
    (!left_exact_caps_lock && !right_exact_caps_lock)
        || (left_exact_caps_lock && right_exact_caps_lock)
}

fn screen_draw_launch_trigger(settings: &Settings) -> Option<Arc<HotkeyTrigger>> {
    match screen_draw_launch_hotkey_text(settings) {
        Ok(Some(chord)) => parse_hotkey(&chord).map(|hotkey| Arc::new(HotkeyTrigger::new(hotkey))),
        Ok(None) => None,
        Err(error) => {
            tracing::warn!(%error, "Screen Draw launch hotkey is unavailable");
            None
        }
    }
}

fn screen_draw_emergency_trigger(settings: &Settings) -> Option<Arc<HotkeyTrigger>> {
    match screen_draw_emergency_hotkey_text(settings) {
        Ok(Some(chord)) => parse_hotkey(&chord).map(|hotkey| Arc::new(HotkeyTrigger::new(hotkey))),
        Ok(None) => None,
        Err(error) => {
            tracing::warn!(%error, "Screen Draw emergency hotkey is unavailable");
            None
        }
    }
}

fn rebuild_screen_draw_triggers(
    settings: &Settings,
    launch: &mut Option<Arc<HotkeyTrigger>>,
    emergency: &mut Option<Arc<HotkeyTrigger>>,
) {
    *launch = screen_draw_launch_trigger(settings);
    *emergency = screen_draw_emergency_trigger(settings);
}

fn radial_global_hotkey_reservations(
    settings: &Settings,
    document: &RadialDocument,
) -> Vec<(String, String)> {
    let mut owned = Vec::new();
    if settings.radial.global_item_inputs {
        for menu in &document.menus {
            for shortcut in menu
                .rings
                .iter()
                .flat_map(|ring| &ring.cells)
                .flat_map(|cell| &cell.shortcuts)
                .filter(|shortcut| {
                    shortcut.scope == multi_launcher::radial::model::TriggerScope::Global
                        && parse_hotkey(&shortcut.chord).is_some()
                })
            {
                owned.push((
                    format!("radial item shortcut {}", shortcut.id.as_str()),
                    shortcut.chord.clone(),
                ));
            }
        }
    }
    owned
}

fn refresh_macro_hotkey_reservations(settings: &Settings, document: &RadialDocument) {
    let mut owned = reserved_launcher_hotkeys(settings);
    owned.extend(radial_global_hotkey_reservations(settings, document));
    let borrowed = owned
        .iter()
        .map(|(name, chord)| (name.as_str(), chord.as_str()))
        .collect::<Vec<_>>();
    if let Err(error) =
        multi_launcher::mkmacro::runtime::refresh_shared_hotkey_reservations(&borrowed)
    {
        tracing::warn!(%error, "failed to refresh macro hotkey reservations");
    }
}

fn radial_item_inputs(
    settings: &Settings,
    document: &RadialDocument,
) -> Vec<multi_launcher::radial::item_input::ItemInputBinding> {
    let reserved = reserved_launcher_hotkeys(settings)
        .into_iter()
        .filter_map(|(_, chord)| parse_hotkey(&chord))
        .collect::<Vec<_>>();
    compile_item_inputs(document, settings.radial.global_item_inputs, &reserved)
}

fn radial_invocation_config(
    settings: &Settings,
    document: &RadialDocument,
    shared_invocation: bool,
    generation: u64,
) -> InvocationConfig {
    let default_menu_id = settings.radial.effective_default_menu_id(document);
    InvocationConfig {
        launcher_enabled: shared_invocation,
        hotkey: settings.hotkey(),
        threshold_ms: settings.radial.hold_threshold_ms,
        generation,
        // Each admitted invocation replaces this seed with its correlated ID.
        context_token: 0,
        menu_id: default_menu_id.clone(),
        interaction: document
            .menus
            .iter()
            .find(|menu| menu.id == default_menu_id)
            .map_or(settings.radial.default_interaction, |menu| menu.interaction),
        accept_external_injected: true,
        item_inputs: radial_item_inputs(settings, document),
    }
}

fn external_radial_lifecycle_metadata(
    intent: &InvocationIntent,
    document: &RadialDocument,
) -> Option<(
    InvocationId,
    multi_launcher::radial::model::MenuId,
    InteractionMode,
    u64,
)> {
    let (id, menu_id, requested_interaction, requested_context) = match intent {
        InvocationIntent::OpenExternalRadial {
            id,
            menu_id,
            context_token,
            interaction,
            ..
        } => (*id, menu_id, Some(*interaction), Some(*context_token)),
        InvocationIntent::ToggleDirectMenu { id, menu_id, .. } => (*id, menu_id, None, None),
        InvocationIntent::ActivateItem {
            id,
            menu_id,
            scope: multi_launcher::radial::model::TriggerScope::Global,
            ..
        } => (*id, menu_id, None, None),
        _ => return None,
    };
    let interaction = requested_interaction.unwrap_or_else(|| {
        document
            .menus
            .iter()
            .find(|menu| menu.id == *menu_id)
            .map_or(InteractionMode::StickyClick, |menu| menu.interaction)
    });
    Some((
        id,
        menu_id.clone(),
        interaction,
        requested_context.unwrap_or(id.0),
    ))
}

type ExternalRadialMetadata = (multi_launcher::radial::model::MenuId, InteractionMode, u64);

fn retire_external_radial_invocation(
    invocations: &mut HashMap<InvocationId, ExternalRadialMetadata>,
    invocation_id: InvocationId,
) -> bool {
    take_external_radial_metadata(invocations, invocation_id).is_some()
}

fn take_external_radial_metadata(
    invocations: &mut HashMap<InvocationId, ExternalRadialMetadata>,
    invocation_id: InvocationId,
) -> Option<ExternalRadialMetadata> {
    invocations.remove(&invocation_id)
}

fn related_launcher_bindings(
    settings: &Settings,
    document: &RadialDocument,
    include_process_hotkeys: bool,
) -> Vec<RelatedBinding> {
    let mut bindings = Vec::new();
    if include_process_hotkeys && let Some(hotkey) = settings.quit_hotkey() {
        bindings.push(RelatedBinding {
            hotkey,
            action: RelatedAction::Quit,
        });
    }
    if include_process_hotkeys && let Some(hotkey) = settings.help_hotkey() {
        bindings.push(RelatedBinding {
            hotkey,
            action: RelatedAction::Help,
        });
    }
    if include_process_hotkeys && let Ok(Some(text)) = screen_draw_launch_hotkey_text(settings) {
        if let Some(hotkey) = parse_hotkey(&text) {
            bindings.push(RelatedBinding {
                hotkey,
                action: RelatedAction::ScreenDrawLaunch,
            });
        }
    }
    if include_process_hotkeys && let Ok(Some(text)) = screen_draw_emergency_hotkey_text(settings) {
        if let Some(hotkey) = parse_hotkey(&text) {
            bindings.push(RelatedBinding {
                hotkey,
                action: RelatedAction::ScreenDrawEmergency,
            });
        }
    }
    for trigger in document
        .custom_triggers
        .iter()
        .filter(|v| v.scope == multi_launcher::radial::model::TriggerScope::Global)
    {
        if let Some(hotkey) = parse_hotkey(&trigger.chord) {
            bindings.push(RelatedBinding {
                hotkey,
                action: RelatedAction::DirectMenu {
                    trigger_id: trigger.id.as_str().to_owned(),
                    menu_id: trigger.menu_id.clone(),
                },
            })
        }
    }
    bindings
}

fn reject_radial_opens_while_exclusive(
    notices: &mut [multi_launcher::hotkey::launcher_invocation::ServiceNotice],
) {
    for notice in notices {
        notice.intents.retain(|intent| {
            !matches!(
                intent,
                multi_launcher::radial::invocation::InvocationIntent::OpenRadial { .. }
                    | multi_launcher::radial::invocation::InvocationIntent::OpenExternalRadial {
                        ..
                    }
                    | multi_launcher::radial::invocation::InvocationIntent::ToggleDirectMenu { .. }
                    | multi_launcher::radial::invocation::InvocationIntent::ToggleLegacyLauncher { .. }
                    | multi_launcher::radial::invocation::InvocationIntent::ActivateItem { .. }
            )
        });
    }
}

fn legacy_listener_triggers(
    shared_invocation: bool,
    launcher: &Arc<HotkeyTrigger>,
    quit: Option<&Arc<HotkeyTrigger>>,
    help: Option<&Arc<HotkeyTrigger>>,
    screen_draw: Option<&Arc<HotkeyTrigger>>,
    emergency: Option<&Arc<HotkeyTrigger>>,
) -> Vec<Arc<HotkeyTrigger>> {
    if shared_invocation {
        return Vec::new();
    }
    let mut watched = vec![Arc::clone(launcher)];
    watched.extend(quit.cloned());
    watched.extend(help.cloned());
    watched.extend(screen_draw.cloned());
    watched.extend(emergency.cloned());
    watched
}

struct PendingLauncherRoute {
    settings_generation: u64,
    handoff: RouteHandoff,
    start_legacy: bool,
    stop_service: bool,
}

struct RadialRoutePlan {
    shared_invocation: bool,
    config: InvocationConfig,
    related: Vec<RelatedBinding>,
    start_legacy: bool,
    stop_service: bool,
}

/// Main-owned boundary for every resource that exists only while radial
/// runtime or authoring work is demanded. The document/control mailboxes stay
/// lightweight and available while disabled; native workers and caches do not.
struct RadialRuntimeResources {
    runtime_enabled: bool,
    authoring_sessions: BTreeSet<AuthoringSessionId>,
    active: bool,
    watcher: Option<RadialConfigWatcher>,
    auditions: BTreeMap<AuthoringSessionId, Vec<(String, u64)>>,
    application_data: PathBuf,
    wake: Sender<()>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RadialResourceTransition {
    None,
    Acquire,
    Release,
}

impl RadialRuntimeResources {
    fn new(application_data: PathBuf, wake: Sender<()>, runtime_enabled: bool) -> Self {
        Self {
            runtime_enabled,
            authoring_sessions: BTreeSet::new(),
            active: false,
            watcher: None,
            auditions: BTreeMap::new(),
            application_data,
            wake,
        }
    }

    fn set_runtime_enabled(&mut self, enabled: bool) {
        self.runtime_enabled = enabled;
    }

    fn set_authoring_demand(&mut self, demand: AuthoringResourceDemand) {
        match demand {
            AuthoringResourceDemand::Acquire(session) => {
                self.authoring_sessions.insert(session);
            }
            AuthoringResourceDemand::Release(session) => {
                self.authoring_sessions.remove(&session);
            }
        }
    }

    fn demanded(&self) -> bool {
        self.runtime_enabled || !self.authoring_sessions.is_empty()
    }

    fn take_transition(&mut self) -> RadialResourceTransition {
        match (self.active, self.demanded()) {
            (false, true) => {
                self.active = true;
                RadialResourceTransition::Acquire
            }
            (true, false) => {
                self.active = false;
                RadialResourceTransition::Release
            }
            _ => RadialResourceTransition::None,
        }
    }

    fn reconcile(&mut self, controller: &mut RadialController) {
        match self.take_transition() {
            RadialResourceTransition::Acquire => {
                controller.configure_resources(self.application_data.clone());
                self.watcher = match RadialConfigWatcher::start(
                    &self.application_data,
                    self.wake.clone(),
                ) {
                    Ok(watcher) => Some(watcher),
                    Err(error) => {
                        tracing::error!(%error, "radial config watch unavailable; external changes require settings reload");
                        None
                    }
                };
            }
            RadialResourceTransition::Release => {
                self.watcher = None;
                controller.release_resources();
            }
            RadialResourceTransition::None => {}
        }
    }

    fn refresh_watcher(&mut self) {
        if !self.active {
            return;
        }
        let asset_tree = self
            .application_data
            .join(multi_launcher::radial::model::RADIAL_ASSETS_DIRECTORY);
        if !asset_tree.is_dir()
            || self
                .watcher
                .as_ref()
                .is_some_and(RadialConfigWatcher::watches_asset_tree)
        {
            return;
        }
        self.watcher = None;
        self.watcher = match RadialConfigWatcher::start(&self.application_data, self.wake.clone()) {
            Ok(watcher) => Some(watcher),
            Err(error) => {
                tracing::error!(%error, "radial config watch refresh failed");
                None
            }
        };
    }

    fn take_changes(&self) -> multi_launcher::radial::watch::RadialWatchChanges {
        self.watcher
            .as_ref()
            .map(RadialConfigWatcher::take)
            .unwrap_or_default()
    }

    fn track_audition(
        &mut self,
        editor_session: AuthoringSessionId,
        session: &multi_launcher::radial::model::SessionId,
        generation: u64,
    ) {
        self.auditions
            .entry(editor_session)
            .or_default()
            .push((session.as_str().to_owned(), generation));
    }

    fn stop_editor_auditions(&mut self, editor_session: AuthoringSessionId) {
        for (session, generation) in self.auditions.remove(&editor_session).unwrap_or_default() {
            let _ = multi_launcher::sound::stop(multi_launcher::sound::PlaybackScope::Radial {
                session,
                generation,
            });
        }
    }

    fn shutdown(&mut self, controller: &mut RadialController) {
        self.runtime_enabled = false;
        self.authoring_sessions.clear();
        for editor_session in self.auditions.keys().copied().collect::<Vec<_>>() {
            self.stop_editor_auditions(editor_session);
        }
        if self.active {
            self.watcher = None;
            controller.release_resources();
            self.active = false;
        }
    }
}

impl Drop for RadialRuntimeResources {
    fn drop(&mut self) {
        self.watcher = None;
        for editor_session in self.auditions.keys().copied().collect::<Vec<_>>() {
            self.stop_editor_auditions(editor_session);
        }
        self.authoring_sessions.clear();
        self.active = false;
    }
}

fn radial_route_plan(
    settings: &Settings,
    document: &RadialDocument,
    generation: u64,
) -> RadialRoutePlan {
    let shared_invocation = settings.radial.enabled && settings.radial.shared_tap_hold;
    RadialRoutePlan {
        shared_invocation,
        config: radial_invocation_config(settings, document, shared_invocation, generation),
        related: if settings.radial.enabled {
            related_launcher_bindings(settings, document, shared_invocation)
        } else {
            Vec::new()
        },
        start_legacy: !shared_invocation,
        stop_service: !settings.radial.enabled,
    }
}

#[allow(clippy::too_many_arguments)]
fn fail_closed_radial_route(
    settings: &mut Settings,
    control: &RadialControlMainEndpoint,
    controller: &mut RadialController,
    service: &mut Option<LauncherInvocationService>,
    listener: &mut HotkeyListener,
    launcher: &Arc<HotkeyTrigger>,
    quit: Option<&Arc<HotkeyTrigger>>,
    help: Option<&Arc<HotkeyTrigger>>,
    screen_draw: Option<&Arc<HotkeyTrigger>>,
    emergency: Option<&Arc<HotkeyTrigger>>,
    event_tx: &Sender<()>,
) {
    settings.radial.enabled = false;
    control.set_enabled(false);
    if let Some(active) = service.as_ref() {
        let _ = active.cancel_lifecycle(LifecycleCancellation::FeatureDisabled);
    }
    controller.disable();
    if let Some(mut active) = service.take() {
        active.stop();
    }
    listener.stop();
    *listener = HotkeyTrigger::start_listener(
        legacy_listener_triggers(false, launcher, quit, help, screen_draw, emergency),
        "main",
        event_tx.clone(),
    );
}

#[derive(Debug, Default, PartialEq, Eq)]
struct ScreenDrawTriggerActions {
    launch: bool,
    recover: bool,
    emergency: bool,
}

fn take_screen_draw_trigger_actions(
    launcher: &HotkeyTrigger,
    launch: Option<&HotkeyTrigger>,
    emergency: Option<&HotkeyTrigger>,
    recovery_bridge: &ScreenDrawRecoveryBridge,
) -> ScreenDrawTriggerActions {
    let emergency_fired = emergency.is_some_and(HotkeyTrigger::take);
    if emergency_fired && recovery_bridge.is_active() {
        // Emergency owns this event-loop turn. Consume any defensive co-fire
        // so the same physical chord cannot also start or recover Screen Draw.
        if let Some(launch) = launch {
            let _ = launch.take();
        }
        let _ = launcher.take();
        return ScreenDrawTriggerActions {
            emergency: true,
            ..Default::default()
        };
    }

    let launch = launch.is_some_and(HotkeyTrigger::take);
    if launch {
        // Publish before enqueueing the GUI event. A launcher summon observed
        // in this same turn is then routed to recovery, never visibility.
        recovery_bridge.stage_start();
    }
    let recover = take_screen_draw_recovery_trigger(launcher, recovery_bridge.is_active());
    ScreenDrawTriggerActions {
        launch,
        recover,
        emergency: false,
    }
}

fn take_screen_draw_recovery_trigger(trigger: &HotkeyTrigger, screen_draw_active: bool) -> bool {
    screen_draw_active && trigger.take()
}

pub fn request_hotkey_restart(settings: Settings) {
    match RESTART_TX.lock() {
        Ok(guard) => {
            if let Some(tx) = guard.as_ref() {
                let _ = tx.send(settings);
            }
        }
        Err(e) => {
            tracing::error!("failed to lock RESTART_TX: {e}");
        }
    }
    if let Ok(guard) = EVENT_TX.lock()
        && let Some(tx) = guard.as_ref()
    {
        let _ = tx.send(());
    }
}

/// Spawn the GUI on a separate thread.
///
/// `actions` is wrapped in an [`Arc`] so the main thread and GUI worker can
/// share a single action list without copying the underlying `Vec`. Cloning the
/// `Arc` only clones the pointer, leaving the `Vec<Action>` itself shared for
/// thread-safe reads. When passing the list to other threads or windows,
/// callers should [`Arc::clone`] the pointer instead of cloning the vector.
fn spawn_gui(
    actions: Arc<Vec<Action>>,
    custom_len: usize,
    settings: Settings,
    settings_path: String,
    startup_settings_diagnostic: Option<SettingsStartupDiagnostic>,
    startup_actions_diagnostic: Option<PersistenceError>,
    startup_recovery: multi_launcher::persistence::RecoveryStartupResult,
    enabled_capabilities: Option<std::collections::HashMap<String, Vec<String>>>,
    screen_draw_recovery_bridge: Arc<ScreenDrawRecoveryBridge>,
    event_tx: Sender<()>,
    radial_hotkey_reservations: Vec<(String, String)>,
) -> (
    thread::JoinHandle<()>,
    Arc<AtomicBool>,
    Arc<AtomicBool>,
    Arc<AtomicBool>,
    Arc<Mutex<Option<egui::Context>>>,
) {
    let custom_len_for_window = custom_len;
    let mut reserved_launcher_hotkeys = reserved_launcher_hotkeys(&settings);
    reserved_launcher_hotkeys.extend(radial_hotkey_reservations);
    let reserved_launcher_hotkey_refs = reserved_launcher_hotkeys
        .iter()
        .map(|(name, hotkey)| (name.as_str(), hotkey.as_str()))
        .collect::<Vec<_>>();
    let manager_timer = multi_launcher::performance::Timer::start();
    let mut plugins = PluginManager::new_with_reserved_hotkeys(&reserved_launcher_hotkey_refs);
    manager_timer.finish("startup.plugin_manager");
    let empty_dirs = Vec::new();
    let dirs = settings.plugin_dirs.as_ref().unwrap_or(&empty_dirs);
    let mut plugin_settings = settings.plugin_settings.clone();
    plugin_settings.insert(
        "note".into(),
        multi_launcher::plugins::note::note_plugin_settings_with_backlinks(
            plugin_settings.get("note"),
            settings.note.backlinks_enabled,
            settings.note.aliases_enabled,
            settings.note.templates_enabled,
        ),
    );
    let registration_timer = multi_launcher::performance::Timer::start();
    plugins.reload_from_dirs(
        dirs,
        settings.clipboard_limit,
        settings.net_unit,
        true,
        &plugin_settings,
        Arc::clone(&actions),
    );
    registration_timer.finish("startup.plugin_registration");
    // Ensure MG service starts even when there is no settings.json/plugin_settings entry yet.
    // Also ensures it is OFF if the plugin is disabled in enabled_plugins.
    multi_launcher::plugins::mouse_gestures::sync_enabled_plugins(
        settings.enabled_plugins.as_ref(),
    );

    let actions_path = "actions.json".to_string();
    let settings_path_for_window = settings_path.clone();
    let plugin_dirs = settings.plugin_dirs.clone();
    let index_paths = settings.index_paths.clone();
    let enabled_plugins = settings.enabled_plugins.clone();
    let visible_flag = Arc::new(AtomicBool::new(true));
    let restore_flag = Arc::new(AtomicBool::new(false));
    let help_flag = Arc::new(AtomicBool::new(false));
    let flag_clone = visible_flag.clone();
    let restore_clone = restore_flag.clone();
    let help_clone = help_flag.clone();
    let ctx_handle = Arc::new(Mutex::new(None));
    let ctx_clone = ctx_handle.clone();
    let actions_for_window = Arc::clone(&actions);

    let handle = thread::spawn(move || {
        let viewport = build_viewport_with_icon(
            &settings,
            include_bytes!("../Resources/Green_MultiLauncher.png"),
        );
        let native_options = eframe::NativeOptions {
            viewport,
            event_loop_builder: Some(Box::new(|_builder| {
                use winit::platform::windows::EventLoopBuilderExtWindows;
                _builder.with_any_thread(true);
            })),
            ..Default::default()
        };

        let _ = eframe::run_native(
            "Multi Lnchr",
            native_options,
            Box::new(move |cc| {
                if let Ok(mut guard) = ctx_clone.lock() {
                    *guard = Some(cc.egui_ctx.clone());
                } else {
                    tracing::error!("failed to lock ctx_clone");
                }
                tracing::debug!("egui context stored");
                let launcher_timer = multi_launcher::performance::Timer::start();
                let mut app = LauncherApp::new(
                    &cc.egui_ctx,
                    actions_for_window,
                    custom_len_for_window,
                    plugins,
                    actions_path,
                    settings_path_for_window,
                    settings.clone(),
                    plugin_dirs,
                    index_paths,
                    enabled_plugins,
                    enabled_capabilities,
                    flag_clone,
                    restore_clone,
                    help_clone,
                );
                app.install_screen_draw_recovery_bridge(screen_draw_recovery_bridge);
                app.startup_settings_diagnostic = startup_settings_diagnostic;
                app.actions_persistence_diagnostic = startup_actions_diagnostic;
                app.set_startup_persistence_context(startup_recovery);
                launcher_timer.finish("startup.launcher_app");
                Box::new(app)
            }),
        );
        let _ = event_tx.send(());
    });

    (handle, visible_flag, restore_flag, help_flag, ctx_handle)
}

fn main() -> anyhow::Result<()> {
    multi_launcher::performance::init_process_timer();
    // This ownership boundary must stay ahead of every persistent read,
    // migration, watcher, worker, plugin, and GUI initialization.
    let app_data_root = AppDataRoot::from_settings_path("settings.json")?;
    let _single_instance_guard = match SingleInstanceGuard::acquire(&app_data_root)? {
        SingleInstanceAcquire::Acquired(guard) => guard,
        SingleInstanceAcquire::AlreadyRunning => return Ok(()),
    };
    let settings_timer = multi_launcher::performance::Timer::start();
    let startup_preload = load_startup_preload(&app_data_root, "settings.json");
    let startup_recovery = startup_preload.recovery;
    let startup_settings = startup_preload.settings;
    let mut settings = startup_settings.settings;
    let startup_settings_diagnostic = startup_settings.diagnostic;
    multi_launcher::settings::set_settings_path("settings.json");
    let _logging_guard = logging::init(settings.debug_logging, settings.log_file_path());
    settings_timer.finish("startup.settings_load");
    if let Some(diagnostic) = startup_recovery.diagnostic.as_ref() {
        tracing::error!(error = %diagnostic, "startup recovery remains pending for retry");
    } else if let Some(action) = startup_recovery.applied.as_ref() {
        tracing::info!(?action, "applied staged startup recovery");
    }
    tracing::debug!(?settings, "settings loaded");
    if let Some(diagnostic) = startup_settings_diagnostic.as_ref() {
        tracing::error!(
            error = %diagnostic.error(),
            "settings startup used temporary defaults without replacing the persisted file"
        );
    }
    multi_launcher::plugins::mouse_gestures::sync_enabled_plugins(
        settings.enabled_plugins.as_ref(),
    );
    if let Some(value) = settings.plugin_settings.get("mouse_gestures")
        && let Ok(cfg) = serde_json::from_value::<
            multi_launcher::plugins::mouse_gestures::MouseGestureSettings,
        >(value.clone())
    {
        multi_launcher::plugins::mouse_gestures::apply_runtime_settings(cfg);
    }
    let actions_timer = multi_launcher::performance::Timer::start();
    let startup_actions = load_startup_actions("actions.json");
    let mut actions_vec = startup_actions.actions;
    let startup_actions_diagnostic = startup_actions.diagnostic;
    let custom_len = actions_vec.len();
    tracing::debug!("{} actions loaded", actions_vec.len());
    if let Some(diagnostic) = startup_actions_diagnostic.as_ref() {
        tracing::error!(
            error = %diagnostic,
            "actions startup used a temporary empty list without replacing the persisted file"
        );
    }
    actions_timer.finish("startup.action_load");

    let (restart_tx, restart_rx) = channel::<Settings>();
    if let Ok(mut guard) = RESTART_TX.lock() {
        *guard = Some(restart_tx);
    } else {
        tracing::error!("failed to lock RESTART_TX while starting");
    }

    let (event_tx, event_rx) = channel::<()>();
    if let Ok(mut guard) = EVENT_TX.lock() {
        *guard = Some(event_tx.clone());
    }
    install_exclusive_wake(event_tx.clone());

    let index_timer = multi_launcher::performance::Timer::start();
    if let Some(paths) = &settings.index_paths {
        let options = indexer::IndexOptions::with_max_items(settings.max_indexed_items);
        for batch in indexer::index_paths_batched(paths, options) {
            actions_vec.extend(batch?);
        }
    }
    index_timer.finish("startup.action_indexing");
    let actions = Arc::new(actions_vec);
    let screen_draw_recovery_bridge = Arc::new(ScreenDrawRecoveryBridge::default());

    let hotkey = settings.hotkey();
    tracing::debug!(?hotkey, "configuring hotkeys");
    let mut trigger = Arc::new(HotkeyTrigger::new(hotkey));
    let mut quit_trigger = settings
        .quit_hotkey()
        .map(|hk| Arc::new(HotkeyTrigger::new(hk)));
    let mut help_trigger = settings
        .help_hotkey()
        .map(|hk| Arc::new(HotkeyTrigger::new(hk)));
    let mut screen_draw_trigger = screen_draw_launch_trigger(&settings);
    let mut emergency_trigger = screen_draw_emergency_trigger(&settings);

    let mut shared_invocation = settings.radial.enabled && settings.radial.shared_tap_hold;
    let mut settings_generation = 1u64;
    let watched = legacy_listener_triggers(
        shared_invocation,
        &trigger,
        quit_trigger.as_ref(),
        help_trigger.as_ref(),
        screen_draw_trigger.as_ref(),
        emergency_trigger.as_ref(),
    );

    let mut listener = HotkeyTrigger::start_listener(watched, "main", event_tx.clone());
    let radial_store = RadialStore::new(&app_data_root).map_err(anyhow::Error::msg)?;
    let (mut radial_document, radial_store_loaded) = match radial_store.reload() {
        Ok(document) => (document, true),
        Err(error) => {
            tracing::warn!(%error, "using retained starter radial definition");
            (radial_store.snapshot().map_err(anyhow::Error::msg)?, false)
        }
    };
    if startup_settings_diagnostic.is_none() && radial_store_loaded {
        match multi_launcher::radial::submenu_migration::startup_migrate(
            &radial_store,
            app_data_root.path().join("settings.json"),
            true,
        ) {
            Ok(outcome) => {
                settings = outcome.settings;
                radial_document = outcome.document;
                if let Some(notice) = outcome.notice {
                    multi_launcher::gui::send_event(
                        multi_launcher::gui::WatchEvent::RadialMigrationNotice(notice),
                    );
                }
            }
            Err(error) => {
                tracing::error!(%error, "radial submenu migration did not modify either source store");
                multi_launcher::gui::send_event(
                    multi_launcher::gui::WatchEvent::RadialRuntimeDiagnostic(error),
                );
            }
        }
    }
    multi_launcher::gui::send_event(multi_launcher::gui::WatchEvent::RadialMigrationState {
        receipt: settings.radial_submenu_migration.clone(),
        default_submenu_presentation: settings.radial.default_submenu_presentation,
    });
    let startup_radial_issues = multi_launcher::radial::settings::validate(
        &settings.radial,
        &radial_document,
        &reserved_launcher_hotkeys(&settings),
    );
    if !startup_radial_issues.is_empty() {
        settings.radial.enabled = false;
        shared_invocation = false;
        listener.stop();
        listener = HotkeyTrigger::start_listener(
            legacy_listener_triggers(
                false,
                &trigger,
                quit_trigger.as_ref(),
                help_trigger.as_ref(),
                screen_draw_trigger.as_ref(),
                emergency_trigger.as_ref(),
            ),
            "main",
            event_tx.clone(),
        );
        for issue in startup_radial_issues {
            tracing::error!(field = issue.field, message = %issue.message, "invalid radial settings at startup; radial runtime disabled fail-closed");
        }
    }
    multi_launcher::gui::install_radial_published_document(Arc::clone(&radial_document));
    let authoring_wake = event_tx.clone();
    let (authoring_client, authoring_endpoint) =
        authoring_control_service_with_wake(Some(Arc::new(move || {
            let _ = authoring_wake.send(());
        })));
    let mut native_preview =
        multi_launcher::radial::authoring::native_preview::NativePreviewCoordinator::new(
            event_tx.clone(),
            app_data_root.path().to_path_buf(),
        );
    let tooltip_preferences =
        multi_launcher::radial::tooltip::TooltipPreferences::from(&settings.radial);
    let _ = native_preview.set_tooltip_preferences(tooltip_preferences);
    multi_launcher::gui::install_radial_authoring_client(authoring_client);
    let control_wake = event_tx.clone();
    let (radial_control_client, radial_control_endpoint) = radial_control_service_with_wake(
        settings.radial.enabled,
        Some(Arc::new(move || {
            let _ = control_wake.send(());
        })),
    );
    multi_launcher::gui::install_radial_control_client(radial_control_client);
    // Keep command-originated controller correlation disjoint from IDs owned
    // by the native invocation service.
    let mut radial_command_invocation_id = 1u64 << 63;
    let mut authoring_preview_document: Option<Arc<RadialDocument>> = None;
    let mut authoring_editor_session = None;
    let mut radial_controller = RadialController::new(
        Arc::clone(&radial_document),
        settings.debug_logging,
        event_tx.clone(),
    );
    let _ = radial_controller.set_tooltip_preferences(tooltip_preferences);
    let mut radial_resources = RadialRuntimeResources::new(
        app_data_root.path().to_path_buf(),
        event_tx.clone(),
        settings.radial.enabled,
    );
    radial_resources.reconcile(&mut radial_controller);
    let owner_bridge = Arc::clone(&screen_draw_recovery_bridge);
    let mut invocation_service = None;
    if settings.radial.enabled {
        match LauncherInvocationService::start(
            radial_invocation_config(
                &settings,
                &radial_document,
                shared_invocation,
                settings_generation,
            ),
            related_launcher_bindings(&settings, &radial_document, shared_invocation),
            Arc::new(move || {
                if owner_bridge.is_active() {
                    PriorityOwner::ScreenDrawRecovery
                } else if exclusive_owners() != 0 {
                    PriorityOwner::ExclusiveTool
                } else {
                    PriorityOwner::Launcher
                }
            }),
            event_tx.clone(),
        ) {
            Ok(service) => invocation_service = Some(service),
            Err(error) => {
                tracing::error!(%error, "shared radial invocation unavailable; disabling radial runtime and restoring legacy launcher hotkey");
                fail_closed_radial_route(
                    &mut settings,
                    &radial_control_endpoint,
                    &mut radial_controller,
                    &mut invocation_service,
                    &mut listener,
                    &trigger,
                    quit_trigger.as_ref(),
                    help_trigger.as_ref(),
                    screen_draw_trigger.as_ref(),
                    emergency_trigger.as_ref(),
                    &event_tx,
                );
                radial_resources.set_runtime_enabled(false);
                radial_resources.reconcile(&mut radial_controller);
            }
        }
    }

    // `visibility` holds whether the window is currently restored (true) or
    // minimized (false).
    let (handle, visibility, restore_flag, help_flag, ctx) = spawn_gui(
        Arc::clone(&actions),
        custom_len,
        settings.clone(),
        "settings.json".to_string(),
        startup_settings_diagnostic,
        startup_actions_diagnostic,
        startup_recovery,
        settings.enabled_capabilities.clone(),
        Arc::clone(&screen_draw_recovery_bridge),
        event_tx.clone(),
        radial_global_hotkey_reservations(&settings, &radial_document),
    );
    let mut queued_visibility: Option<bool> = None;
    let mut previous_exclusive = false;
    let mut pending_launcher_route: Option<PendingLauncherRoute> = None;
    let mut external_radial_invocations: HashMap<InvocationId, ExternalRadialMetadata> =
        HashMap::new();

    loop {
        if let Err(err) = event_rx.recv() {
            tracing::error!(?err, "event channel closed; shutting down launcher loop");
            if let Some(service) = invocation_service.as_ref() {
                let _ = service.cancel_lifecycle(LifecycleCancellation::Shutdown);
            }
            radial_controller.shutdown();
            native_preview.cancel_all();
            radial_resources.shutdown(&mut radial_controller);
            if let Some(service) = invocation_service.as_mut() {
                service.stop();
            }
            listener.stop();
            let _ = handle.join();
            break Ok(());
        }

        if handle.is_finished() {
            if let Some(service) = invocation_service.as_ref() {
                let _ = service.cancel_lifecycle(LifecycleCancellation::Shutdown);
            }
            radial_controller.shutdown();
            native_preview.cancel_all();
            radial_resources.shutdown(&mut radial_controller);
            if let Some(service) = invocation_service.as_mut() {
                service.stop();
            }
            listener.stop();
            let _ = handle.join();
            break Ok(());
        }

        let radial_changes = radial_resources.take_changes();
        let mut radial_generation_replaced = false;
        if radial_changes.document {
            let reload = radial_store.reload_external_with(|_| {
                let _ = multi_launcher::gui::send_event(
                    multi_launcher::gui::WatchEvent::RadialInvalidate,
                );
                radial_controller.close(
                    multi_launcher::radial::native::CloseReason::SettingsReload,
                    None,
                );
            });
            match reload {
                Ok(ExternalReloadOutcome::Unchanged) => {
                    tracing::debug!("ignored radial document self-save/unchanged echo");
                }
                Ok(ExternalReloadOutcome::Published(document)) => {
                    native_preview.cancel_all();
                    radial_generation_replaced = true;
                    radial_document = document;
                    multi_launcher::gui::install_radial_published_document(Arc::clone(
                        &radial_document,
                    ));
                    if let (Some(editor_session), Ok(snapshot)) =
                        (authoring_editor_session, radial_store.authoring_snapshot())
                    {
                        let _ =
                            authoring_endpoint
                                .reply_tx
                                .send(AuthoringReply::ExternalPublished {
                                    editor_session,
                                    snapshot,
                                });
                    }
                    radial_controller.replace_document(Arc::clone(&radial_document));
                    refresh_macro_hotkey_reservations(&settings, &radial_document);
                    settings_generation = settings_generation.checked_add(1).unwrap_or(1);
                    let issues = multi_launcher::radial::settings::validate(
                        &settings.radial,
                        &radial_document,
                        &reserved_launcher_hotkeys(&settings),
                    );
                    if !issues.is_empty() {
                        settings.radial.enabled = false;
                        radial_control_endpoint.set_enabled(false);
                        radial_controller.disable();
                        for issue in issues {
                            multi_launcher::gui::send_event(
                                multi_launcher::gui::WatchEvent::RadialRuntimeDiagnostic(
                                    issue.message,
                                ),
                            );
                        }
                    }
                    let route_plan =
                        radial_route_plan(&settings, &radial_document, settings_generation);
                    if let Some(service) = invocation_service.as_ref() {
                        match service.begin_route_handoff(
                            route_plan.config,
                            route_plan.related,
                            LifecycleCancellation::SettingsReload,
                        ) {
                            Ok(handoff) => {
                                pending_launcher_route = Some(PendingLauncherRoute {
                                    settings_generation,
                                    handoff,
                                    start_legacy: route_plan.start_legacy,
                                    stop_service: route_plan.stop_service,
                                });
                            }
                            Err(error) => {
                                tracing::error!(%error, "radial trigger reload failed closed");
                                fail_closed_radial_route(
                                    &mut settings,
                                    &radial_control_endpoint,
                                    &mut radial_controller,
                                    &mut invocation_service,
                                    &mut listener,
                                    &trigger,
                                    quit_trigger.as_ref(),
                                    help_trigger.as_ref(),
                                    screen_draw_trigger.as_ref(),
                                    emergency_trigger.as_ref(),
                                    &event_tx,
                                );
                            }
                        }
                    } else if settings.radial.enabled {
                        let owner_bridge = Arc::clone(&screen_draw_recovery_bridge);
                        match LauncherInvocationService::start(
                            route_plan.config,
                            route_plan.related,
                            Arc::new(move || {
                                if owner_bridge.is_active() {
                                    PriorityOwner::ScreenDrawRecovery
                                } else if exclusive_owners() != 0 {
                                    PriorityOwner::ExclusiveTool
                                } else {
                                    PriorityOwner::Launcher
                                }
                            }),
                            event_tx.clone(),
                        ) {
                            Ok(service) => invocation_service = Some(service),
                            Err(error) => {
                                tracing::error!(%error, "radial trigger service restart failed closed");
                                fail_closed_radial_route(
                                    &mut settings,
                                    &radial_control_endpoint,
                                    &mut radial_controller,
                                    &mut invocation_service,
                                    &mut listener,
                                    &trigger,
                                    quit_trigger.as_ref(),
                                    help_trigger.as_ref(),
                                    screen_draw_trigger.as_ref(),
                                    emergency_trigger.as_ref(),
                                    &event_tx,
                                );
                            }
                        }
                    } else {
                        listener.stop();
                        listener = HotkeyTrigger::start_listener(
                            legacy_listener_triggers(
                                false,
                                &trigger,
                                quit_trigger.as_ref(),
                                help_trigger.as_ref(),
                                screen_draw_trigger.as_ref(),
                                emergency_trigger.as_ref(),
                            ),
                            "main",
                            event_tx.clone(),
                        );
                    }
                    let _ = multi_launcher::gui::send_event(
                        multi_launcher::gui::WatchEvent::RadialConfigDiagnostic(None),
                    );
                }
                Err(error) => {
                    tracing::warn!(%error, "retaining last valid radial document after external change");
                    let _ = multi_launcher::gui::send_event(
                        multi_launcher::gui::WatchEvent::RadialConfigDiagnostic(Some(
                            error.to_string(),
                        )),
                    );
                }
            }
        }
        if radial_changes.assets {
            if !radial_generation_replaced {
                let _ = multi_launcher::gui::send_event(
                    multi_launcher::gui::WatchEvent::RadialInvalidate,
                );
                radial_controller.invalidate_resources();
            }
            // If the asset tree was created after watcher acquisition, rebuild
            // the narrow watch set so subsequent descendants are observed.
            radial_resources.refresh_watcher();
        }

        while let Ok(demand) = authoring_endpoint.resource_rx.try_recv() {
            let released = match demand {
                AuthoringResourceDemand::Acquire(editor_session) => {
                    authoring_editor_session = Some(editor_session);
                    false
                }
                AuthoringResourceDemand::Release(editor_session) => {
                    if authoring_editor_session == Some(editor_session) {
                        authoring_editor_session = None;
                    }
                    native_preview.cancel_all();
                    radial_resources.stop_editor_auditions(editor_session);
                    if authoring_preview_document.take().is_some() {
                        radial_controller.replace_document(Arc::clone(&radial_document));
                    }
                    true
                }
            };
            radial_resources.set_authoring_demand(demand);
            if released && !settings.radial.enabled {
                radial_controller.disable();
            }
            radial_resources.reconcile(&mut radial_controller);
        }

        while let Ok(request) = authoring_endpoint.request_rx.try_recv() {
            let request_id = request.id();
            let request_generation = request.generation();
            let editor_session = request.editor_session();
            authoring_editor_session = Some(editor_session);
            let reply = match request {
                AuthoringRequest::Snapshot { .. } => radial_store
                    .authoring_snapshot()
                    .map(|snapshot| AuthoringReply::Snapshot {
                        id: request_id,
                        generation: request_generation,
                        editor_session,
                        snapshot,
                    })
                    .unwrap_or_else(|error| AuthoringReply::Failed {
                        id: request_id,
                        generation: request_generation,
                        editor_session,
                        message: error.to_string(),
                    }),
                AuthoringRequest::ExportPackage {
                    roots,
                    expected_revision,
                    expected_disk_sha256,
                    ..
                } => radial_store
                    .export_package(&roots, expected_revision, &expected_disk_sha256.0)
                    .map(|bytes| AuthoringReply::PackageExported {
                        id: request_id,
                        generation: request_generation,
                        editor_session,
                        bytes: bytes.into(),
                    })
                    .unwrap_or_else(|error| AuthoringReply::Failed {
                        id: request_id,
                        generation: request_generation,
                        editor_session,
                        message: error.to_string(),
                    }),
                AuthoringRequest::ExportSkin {
                    skin_id,
                    expected_revision,
                    expected_disk_sha256,
                    ..
                } => radial_store
                    .export_skin_bundle(&skin_id, expected_revision, &expected_disk_sha256.0)
                    .map(|bytes| AuthoringReply::PackageExported {
                        id: request_id,
                        generation: request_generation,
                        editor_session,
                        bytes: bytes.into(),
                    })
                    .unwrap_or_else(|error| AuthoringReply::Failed {
                        id: request_id,
                        generation: request_generation,
                        editor_session,
                        message: error.to_string(),
                    }),
                AuthoringRequest::AuditionManagedAsset { asset_id, .. } => {
                    radial_resources.stop_editor_auditions(editor_session);
                    let reference =
                        multi_launcher::radial::model::MediaReference::Managed { asset_id };
                    let audition_session = multi_launcher::radial::model::SessionId::new(format!(
                        "radial-editor-audition-{}",
                        request_id.0
                    ));
                    let result = radial_controller
                        .prepare_authoring_asset(
                            &reference,
                            multi_launcher::radial::model::MediaKind::Sound,
                            &radial_document.assets,
                            multi_launcher::radial::assets::PrepareVariant {
                                effective_style: 0,
                                dpi_milli: 1_000,
                                logical_width_milli: 1_000,
                                logical_height_milli: 1_000,
                                quality: multi_launcher::radial::model::RenderingQuality::Balanced,
                            },
                        )
                        .and_then(|prepared| match &*prepared.media {
                            multi_launcher::radial::assets::PreparedMedia::Sound(sound) => {
                                multi_launcher::radial::audio::audition_wav(
                                    audition_session.clone(),
                                    request_generation.0,
                                    std::sync::Arc::clone(&sound.wav),
                                )
                                .then_some(())
                                .ok_or(multi_launcher::radial::assets::AssetDiagnostic::InvalidWave)
                            }
                            multi_launcher::radial::assets::PreparedMedia::Image(_) => Err(
                                multi_launcher::radial::assets::AssetDiagnostic::UnsupportedFormat,
                            ),
                        });
                    if result.is_ok() {
                        radial_resources.track_audition(
                            editor_session,
                            &audition_session,
                            request_generation.0,
                        );
                    }
                    match result {
                        Ok(()) => AuthoringReply::AssetAuditioned {
                            id: request_id,
                            generation: request_generation,
                            editor_session,
                        },
                        Err(error) => AuthoringReply::Failed {
                            id: request_id,
                            generation: request_generation,
                            editor_session,
                            message: format!("Managed sound audition failed: {error}"),
                        },
                    }
                }
                AuthoringRequest::FontCatalog { .. } => AuthoringReply::FontCatalog {
                    id: request_id,
                    generation: request_generation,
                    editor_session,
                    families: radial_controller.font_families().into(),
                },
                AuthoringRequest::PrepareEmbeddedPreview {
                    candidate,
                    menu_id,
                    selected,
                    anchor,
                    work_area,
                    scale,
                    token,
                    projection,
                    ..
                } => native_preview
                    .prepare_frame(
                        &candidate,
                        &menu_id,
                        anchor,
                        work_area,
                        scale,
                        request_generation.0,
                        selected.as_ref(),
                        &projection,
                    )
                    .map(|input| AuthoringReply::EmbeddedPreviewPrepared {
                        id: request_id,
                        generation: request_generation,
                        editor_session,
                        token,
                        input: Arc::new(input),
                    })
                    .unwrap_or_else(|message| AuthoringReply::Failed {
                        id: request_id,
                        generation: request_generation,
                        editor_session,
                        message,
                    }),
                AuthoringRequest::ReplacePackage {
                    expected_revision,
                    expected_disk_sha256,
                    plan,
                    backup_path,
                    confirmed,
                    ..
                } => {
                    native_preview.cancel_all();
                    let result = if !confirmed {
                        Err(multi_launcher::radial::store::StoreError::BackupRequired.to_string())
                    } else if let Err(issue) =
                        multi_launcher::radial::settings::validate_publication_default(
                            &settings.radial,
                            &plan.document,
                        )
                    {
                        Err(issue.message)
                    } else {
                        radial_store
                            .replace_package_with_backup(
                                plan,
                                expected_revision,
                                expected_disk_sha256.0,
                                backup_path.clone(),
                            )
                            .map_err(|error| error.to_string())
                    };
                    match result {
                        Ok(published) => {
                            authoring_preview_document = None;
                            radial_document = published;
                            multi_launcher::gui::install_radial_published_document(Arc::clone(
                                &radial_document,
                            ));
                            multi_launcher::gui::send_event(
                                multi_launcher::gui::WatchEvent::RadialInvalidate,
                            );
                            radial_controller.close(
                                multi_launcher::radial::native::CloseReason::SettingsReload,
                                None,
                            );
                            radial_controller.replace_document(Arc::clone(&radial_document));
                            radial_controller.invalidate_resources();
                            refresh_macro_hotkey_reservations(&settings, &radial_document);
                            settings_generation = settings_generation.checked_add(1).unwrap_or(1);
                            let route_plan =
                                radial_route_plan(&settings, &radial_document, settings_generation);
                            if let Some(service) = invocation_service.as_ref() {
                                match service.begin_route_handoff(
                                    route_plan.config,
                                    route_plan.related,
                                    LifecycleCancellation::SettingsReload,
                                ) {
                                    Ok(handoff) => {
                                        pending_launcher_route = Some(PendingLauncherRoute {
                                            settings_generation,
                                            handoff,
                                            start_legacy: route_plan.start_legacy,
                                            stop_service: route_plan.stop_service,
                                        });
                                    }
                                    Err(error) => {
                                        tracing::error!(%error, "radial package replacement trigger refresh failed closed");
                                        settings.radial.enabled = false;
                                        radial_control_endpoint.set_enabled(false);
                                        radial_controller.disable();
                                        if let Some(mut service) = invocation_service.take() {
                                            service.stop();
                                        }
                                        listener.stop();
                                        listener = HotkeyTrigger::start_listener(
                                            legacy_listener_triggers(
                                                false,
                                                &trigger,
                                                quit_trigger.as_ref(),
                                                help_trigger.as_ref(),
                                                screen_draw_trigger.as_ref(),
                                                emergency_trigger.as_ref(),
                                            ),
                                            "main",
                                            event_tx.clone(),
                                        );
                                    }
                                }
                            }
                            radial_store
                                .authoring_snapshot()
                                .map(|snapshot| AuthoringReply::PackageReplaced {
                                    id: request_id,
                                    generation: request_generation,
                                    editor_session,
                                    snapshot,
                                    backup_path,
                                })
                                .unwrap_or_else(|error| AuthoringReply::Failed {
                                    id: request_id,
                                    generation: request_generation,
                                    editor_session,
                                    message: error.to_string(),
                                })
                        }
                        Err(message) => AuthoringReply::Failed {
                            id: request_id,
                            generation: request_generation,
                            editor_session,
                            message,
                        },
                    }
                }
                AuthoringRequest::LivePreview { candidate, .. } => {
                    match validate_radial_document(&candidate) {
                        Ok(()) => {
                            if authoring_preview_document.is_none() {
                                authoring_preview_document = Some(Arc::clone(&radial_document));
                            }
                            radial_controller.replace_document(candidate);
                            radial_controller.invalidate_resources();
                            AuthoringReply::PreviewAccepted {
                                id: request_id,
                                generation: request_generation,
                                editor_session,
                            }
                        }
                        Err(error) => AuthoringReply::Failed {
                            id: request_id,
                            generation: request_generation,
                            editor_session,
                            message: error.to_string(),
                        },
                    }
                }
                AuthoringRequest::CancelPreview { .. } => {
                    if authoring_preview_document.take().is_some() {
                        radial_controller.replace_document(Arc::clone(&radial_document));
                        radial_controller.invalidate_resources();
                    }
                    AuthoringReply::PreviewCancelled {
                        id: request_id,
                        generation: request_generation,
                        editor_session,
                    }
                }
                AuthoringRequest::Commit {
                    disposition,
                    expected_revision,
                    expected_disk_sha256,
                    candidate,
                    assets,
                    ..
                } => {
                    native_preview.cancel_all();
                    let result = multi_launcher::radial::settings::validate_publication_default(
                        &settings.radial,
                        &candidate,
                    )
                    .map_err(|issue| issue.message)
                    .and_then(|()| {
                        radial_store
                            .commit_authoring(
                                expected_revision,
                                &expected_disk_sha256,
                                candidate,
                                assets,
                            )
                            .map_err(|error| error.to_string())
                    });
                    match result {
                        Ok(published) => {
                            authoring_preview_document = None;
                            radial_document = Arc::clone(&published.snapshot.document);
                            multi_launcher::gui::install_radial_published_document(Arc::clone(
                                &radial_document,
                            ));
                            multi_launcher::gui::send_event(
                                multi_launcher::gui::WatchEvent::RadialInvalidate,
                            );
                            radial_controller.close(
                                multi_launcher::radial::native::CloseReason::SettingsReload,
                                None,
                            );
                            radial_controller.replace_document(Arc::clone(&radial_document));
                            radial_controller.invalidate_resources();
                            refresh_macro_hotkey_reservations(&settings, &radial_document);
                            settings_generation = settings_generation.checked_add(1).unwrap_or(1);
                            let route_plan =
                                radial_route_plan(&settings, &radial_document, settings_generation);
                            if let Some(service) = invocation_service.as_ref() {
                                match service.begin_route_handoff(
                                    route_plan.config,
                                    route_plan.related,
                                    LifecycleCancellation::SettingsReload,
                                ) {
                                    Ok(handoff) => {
                                        pending_launcher_route = Some(PendingLauncherRoute {
                                            settings_generation,
                                            handoff,
                                            start_legacy: route_plan.start_legacy,
                                            stop_service: route_plan.stop_service,
                                        });
                                    }
                                    Err(error) => {
                                        tracing::error!(%error, "radial authoring trigger refresh failed closed");
                                        settings.radial.enabled = false;
                                        radial_control_endpoint.set_enabled(false);
                                        radial_controller.disable();
                                        if let Some(mut service) = invocation_service.take() {
                                            service.stop();
                                        }
                                        listener.stop();
                                        listener = HotkeyTrigger::start_listener(
                                            legacy_listener_triggers(
                                                false,
                                                &trigger,
                                                quit_trigger.as_ref(),
                                                help_trigger.as_ref(),
                                                screen_draw_trigger.as_ref(),
                                                emergency_trigger.as_ref(),
                                            ),
                                            "main",
                                            event_tx.clone(),
                                        );
                                    }
                                }
                            }
                            AuthoringReply::Published {
                                id: request_id,
                                generation: request_generation,
                                editor_session,
                                disposition,
                                snapshot: published.snapshot,
                                rollback_assets: published.rollback_assets,
                            }
                        }
                        Err(message) => AuthoringReply::Failed {
                            id: request_id,
                            generation: request_generation,
                            editor_session,
                            message,
                        },
                    }
                }
                AuthoringRequest::StartNativePreview {
                    editor_session,
                    expected_revision,
                    expected_disk_sha256,
                    candidate,
                    menu_id,
                    sample_external_context,
                    projection,
                    ..
                } => {
                    radial_controller.close(
                        multi_launcher::radial::native::CloseReason::SettingsReload,
                        None,
                    );
                    radial_store
                        .authoring_snapshot()
                        .map_err(|error| error.to_string())
                        .and_then(|snapshot| {
                            if snapshot.revision != expected_revision
                                || snapshot.disk_sha256 != expected_disk_sha256
                            {
                                return Err("native preview draft baseline is stale".into());
                            }
                            native_preview.start_with_projection(
                                editor_session,
                                request_generation,
                                request_id,
                                candidate,
                                menu_id,
                                sample_external_context,
                                projection,
                            )
                        })
                        .map(|result| AuthoringReply::NativePreviewStarted {
                            id: request_id,
                            generation: request_generation,
                            editor_session,
                            lease: result.lease,
                            sampled_context: result.sampled_context,
                            diagnostics: result.diagnostics,
                        })
                        .unwrap_or_else(|message| AuthoringReply::Failed {
                            id: request_id,
                            generation: request_generation,
                            editor_session,
                            message,
                        })
                }
                AuthoringRequest::UpdateNativePreview {
                    previous,
                    expected_revision,
                    expected_disk_sha256,
                    candidate,
                    menu_id,
                    sample_external_context,
                    projection,
                    ..
                } => radial_store
                    .authoring_snapshot()
                    .map_err(|error| error.to_string())
                    .and_then(|snapshot| {
                        if snapshot.revision != expected_revision
                            || snapshot.disk_sha256 != expected_disk_sha256
                        {
                            return Err("native preview draft baseline is stale".into());
                        }
                        native_preview.update_with_projection(
                            &previous,
                            candidate,
                            menu_id,
                            request_generation,
                            request_id,
                            sample_external_context,
                            projection,
                        )
                    })
                    .map(|result| AuthoringReply::NativePreviewUpdated {
                        id: request_id,
                        generation: request_generation,
                        editor_session,
                        lease: result.lease,
                        sampled_context: result.sampled_context,
                        diagnostics: result.diagnostics,
                    })
                    .unwrap_or_else(|message| AuthoringReply::Failed {
                        id: request_id,
                        generation: request_generation,
                        editor_session,
                        message,
                    }),
                AuthoringRequest::StopNativePreview { editor_session, .. } => {
                    native_preview.stop(editor_session, request_generation, request_id);
                    AuthoringReply::NativePreviewStopped {
                        id: request_id,
                        generation: request_generation,
                        editor_session,
                    }
                }
            };
            let _ = authoring_endpoint.reply_tx.send(reply);
        }
        for notice in native_preview.poll() {
            let reply = match notice {
                multi_launcher::radial::authoring::native_preview::NativePreviewNotice::Diagnostics {
                    lease,
                    diagnostics,
                } => AuthoringReply::NativePreviewDiagnostics {
                    editor_session: lease.editor_session,
                    lease,
                    diagnostics,
                },
                multi_launcher::radial::authoring::native_preview::NativePreviewNotice::Failed {
                    lease,
                    message,
                } => AuthoringReply::NativePreviewFailed {
                    editor_session: lease.editor_session,
                    lease,
                    message,
                },
                multi_launcher::radial::authoring::native_preview::NativePreviewNotice::Stopped(
                    lease,
                ) => AuthoringReply::NativePreviewStopped {
                    id: lease.request_id,
                    generation: lease.generation,
                    editor_session: lease.editor_session,
                },
            };
            let _ = authoring_endpoint.reply_tx.send(reply);
        }

        let handoff_result = pending_launcher_route
            .as_ref()
            .and_then(|pending| pending.handoff.try_complete());
        if let Some(result) = handoff_result {
            let pending = pending_launcher_route
                .take()
                .expect("completed launcher handoff remains pending");
            if pending.settings_generation == settings_generation {
                match result {
                    Ok(()) => {
                        if pending.stop_service
                            && let Some(mut service) = invocation_service.take()
                        {
                            service.stop();
                        }
                        if pending.start_legacy {
                            listener.stop();
                            listener = HotkeyTrigger::start_listener(
                                legacy_listener_triggers(
                                    false,
                                    &trigger,
                                    quit_trigger.as_ref(),
                                    help_trigger.as_ref(),
                                    screen_draw_trigger.as_ref(),
                                    emergency_trigger.as_ref(),
                                ),
                                "main",
                                event_tx.clone(),
                            );
                        }
                    }
                    Err(error) => {
                        tracing::error!(%error, "launcher route handoff failed closed");
                        settings.radial.enabled = false;
                        radial_control_endpoint.set_enabled(false);
                        radial_controller.disable();
                        if let Some(mut service) = invocation_service.take() {
                            service.stop();
                        }
                        listener.stop();
                        listener = HotkeyTrigger::start_listener(
                            legacy_listener_triggers(
                                false,
                                &trigger,
                                quit_trigger.as_ref(),
                                help_trigger.as_ref(),
                                screen_draw_trigger.as_ref(),
                                emergency_trigger.as_ref(),
                            ),
                            "main",
                            event_tx.clone(),
                        );
                    }
                }
            }
        }

        let mut radial_notices = Vec::new();
        while let Ok(request) = radial_control_endpoint.request_rx.try_recv() {
            match request {
                RadialControlRequest::Close => radial_notices.push(ServiceNotice {
                    recovery: false,
                    intents: vec![InvocationIntent::CloseRadial { session_id: None }],
                    error: None,
                    action: None,
                    cancellation: None,
                }),
                RadialControlRequest::RestoreSubmenuPresentationMigration => {
                    native_preview.cancel_all();
                    match multi_launcher::radial::submenu_migration::restore(
                        &radial_store,
                        app_data_root.path().join("settings.json"),
                    ) {
                        Ok(outcome) => {
                            settings = outcome.settings;
                            radial_document = outcome.document;
                            radial_controller.close(
                                multi_launcher::radial::native::CloseReason::SettingsReload,
                                None,
                            );
                            radial_controller.replace_document(Arc::clone(&radial_document));
                            radial_resources.reconcile(&mut radial_controller);
                            radial_control_endpoint.set_enabled(settings.radial.enabled);
                            settings_generation = settings_generation.checked_add(1).unwrap_or(1);
                            multi_launcher::gui::install_radial_published_document(Arc::clone(
                                &radial_document,
                            ));
                            multi_launcher::gui::send_event(
                                multi_launcher::gui::WatchEvent::RadialInvalidate,
                            );
                            multi_launcher::gui::send_event(
                                multi_launcher::gui::WatchEvent::RadialMigrationState {
                                    receipt: settings.radial_submenu_migration.clone(),
                                    default_submenu_presentation: settings
                                        .radial
                                        .default_submenu_presentation,
                                },
                            );
                            if let Some(notice) = outcome.notice {
                                multi_launcher::gui::send_event(
                                    multi_launcher::gui::WatchEvent::RadialMigrationNotice(notice),
                                );
                            }
                        }
                        Err(error) => {
                            tracing::error!(%error, "radial submenu migration restore did not complete");
                            if let Ok(reloaded) = radial_store.reload() {
                                radial_document = reloaded;
                            }
                            radial_controller.close(
                                multi_launcher::radial::native::CloseReason::SettingsReload,
                                None,
                            );
                            radial_controller.replace_document(Arc::clone(&radial_document));
                            radial_resources.reconcile(&mut radial_controller);
                            multi_launcher::gui::install_radial_published_document(Arc::clone(
                                &radial_document,
                            ));
                            if let Ok(reloaded) = Settings::load(
                                app_data_root
                                    .path()
                                    .join("settings.json")
                                    .to_string_lossy()
                                    .as_ref(),
                            ) {
                                settings = reloaded;
                                radial_control_endpoint.set_enabled(settings.radial.enabled);
                            }
                            settings_generation = settings_generation.checked_add(1).unwrap_or(1);
                            multi_launcher::gui::send_event(
                                multi_launcher::gui::WatchEvent::RadialInvalidate,
                            );
                            multi_launcher::gui::send_event(
                                multi_launcher::gui::WatchEvent::RadialMigrationState {
                                    receipt: settings.radial_submenu_migration.clone(),
                                    default_submenu_presentation: settings
                                        .radial
                                        .default_submenu_presentation,
                                },
                            );
                            multi_launcher::gui::send_event(
                                multi_launcher::gui::WatchEvent::RadialRuntimeDiagnostic(error),
                            );
                        }
                    }
                }
                RadialControlRequest::SetActiveParentSubmenuCascade {
                    session_id,
                    parent_frame_id,
                    parent_menu_id,
                } => {
                    let correlation = (session_id.clone(), parent_frame_id);
                    let result = (|| -> Result<Arc<RadialDocument>, String> {
                        if !radial_controller.active_frame_matches(
                            &session_id,
                            parent_frame_id,
                            &parent_menu_id,
                        ) {
                            return Err("the radial parent frame is no longer active".into());
                        }
                        let snapshot = radial_store
                            .authoring_snapshot()
                            .map_err(|error| error.to_string())?;
                        let mut candidate = (*snapshot.document).clone();
                        let menu = candidate
                            .menus
                            .iter_mut()
                            .find(|menu| menu.id == parent_menu_id)
                            .ok_or_else(|| "the radial parent menu no longer exists".to_owned())?;
                        if menu.submenu_presentation
                            != multi_launcher::radial::model::SubmenuPresentation::SameCenter
                        {
                            return Err(
                                "the parent presentation changed before the request was applied"
                                    .into(),
                            );
                        }
                        menu.submenu_presentation =
                            multi_launcher::radial::model::SubmenuPresentation::Cascade;
                        let committed = radial_store
                            .commit_authoring(
                                snapshot.revision,
                                &snapshot.disk_sha256,
                                candidate,
                                multi_launcher::radial::authoring::AssetMutations::default(),
                            )
                            .map_err(|error| error.to_string())?;
                        Ok(Arc::clone(&committed.snapshot.document))
                    })();
                    match result {
                        Ok(document) => {
                            native_preview.cancel_all();
                            radial_controller.close(
                                multi_launcher::radial::native::CloseReason::SettingsReload,
                                None,
                            );
                            radial_document = document;
                            radial_controller.replace_document(Arc::clone(&radial_document));
                            radial_resources.reconcile(&mut radial_controller);
                            multi_launcher::gui::install_radial_published_document(Arc::clone(
                                &radial_document,
                            ));
                            multi_launcher::gui::send_event(
                                multi_launcher::gui::WatchEvent::RadialInvalidate,
                            );
                            multi_launcher::gui::send_event(
                                multi_launcher::gui::WatchEvent::RadialPlacementActionResult {
                                    session_id: correlation.0,
                                    parent_frame_id: correlation.1,
                                    result: Ok(()),
                                },
                            );
                        }
                        Err(error) => {
                            multi_launcher::gui::send_event(
                                multi_launcher::gui::WatchEvent::RadialPlacementActionResult {
                                    session_id: correlation.0,
                                    parent_frame_id: correlation.1,
                                    result: Err(error),
                                },
                            );
                        }
                    }
                }
                RadialControlRequest::Show(selector) => {
                    if !settings.radial.enabled {
                        multi_launcher::gui::send_event(
                            multi_launcher::gui::WatchEvent::RadialRuntimeDiagnostic(
                                "radial menus are disabled in settings".into(),
                            ),
                        );
                        continue;
                    }
                    let selector = match selector {
                        multi_launcher::radial::control::RadialMenuSelector::Default => {
                            multi_launcher::radial::control::RadialMenuSelector::IdOrName(
                                settings
                                    .radial
                                    .effective_default_menu_id(&radial_document)
                                    .as_str()
                                    .to_owned(),
                            )
                        }
                        selector => selector,
                    };
                    match resolve_menu(&radial_document, &selector) {
                        Ok(menu_id) => {
                            let interaction = radial_document
                                .menus
                                .iter()
                                .find(|menu| menu.id == menu_id)
                                .map_or(InteractionMode::StickyClick, |menu| menu.interaction);
                            let invocation_id = InvocationId(radial_command_invocation_id);
                            radial_command_invocation_id =
                                radial_command_invocation_id.checked_add(1).unwrap_or(1);
                            radial_notices.push(ServiceNotice {
                                recovery: false,
                                intents: vec![InvocationIntent::OpenExternalRadial {
                                    id: invocation_id,
                                    menu_id,
                                    context_token: invocation_id.0,
                                    interaction,
                                    trigger_still_down: false,
                                }],
                                error: None,
                                action: None,
                                cancellation: None,
                            });
                        }
                        Err(error) => {
                            let message = error.to_string();
                            tracing::warn!(%message, "radial command could not resolve menu");
                            multi_launcher::gui::send_event(
                                multi_launcher::gui::WatchEvent::RadialRuntimeDiagnostic(message),
                            );
                        }
                    }
                }
            }
        }
        if let Some(service) = invocation_service.as_ref() {
            while let Some(mut notice) = service.try_recv() {
                if notice.recovery {
                    if let Ok(mut value) = trigger.open.lock() {
                        *value = true
                    }
                    notice.recovery = false;
                }
                if let Some(action) = notice.action.take() {
                    match action {
                        RelatedAction::Quit => {
                            if let Some(v) = &quit_trigger {
                                if let Ok(mut flag) = v.open.lock() {
                                    *flag = true
                                }
                            }
                        }
                        RelatedAction::Help => {
                            if let Some(v) = &help_trigger {
                                if let Ok(mut flag) = v.open.lock() {
                                    *flag = true
                                }
                            }
                        }
                        RelatedAction::ScreenDrawLaunch => {
                            if let Some(v) = &screen_draw_trigger {
                                if let Ok(mut flag) = v.open.lock() {
                                    *flag = true
                                }
                            }
                        }
                        RelatedAction::ScreenDrawEmergency => {
                            if let Some(v) = &emergency_trigger {
                                if let Ok(mut flag) = v.open.lock() {
                                    *flag = true
                                }
                            }
                        }
                        RelatedAction::DirectMenu { .. } => {}
                    }
                }
                radial_notices.push(notice)
            }
        }
        let screen_draw_actions = take_screen_draw_trigger_actions(
            &trigger,
            screen_draw_trigger.as_deref(),
            emergency_trigger.as_deref(),
            &screen_draw_recovery_bridge,
        );
        let exclusive = exclusive_owners() != 0;
        if exclusive != previous_exclusive {
            if let Some(service) = invocation_service.as_ref() {
                let _ = service.set_exclusive(exclusive);
            }
            if exclusive {
                radial_controller.close(
                    multi_launcher::radial::native::CloseReason::ExclusiveTool,
                    None,
                );
            }
            previous_exclusive = exclusive;
        }
        if screen_draw_actions.emergency {
            if let Err(error) = screen_draw_recovery_bridge.emergency_pause() {
                tracing::error!(%error, "failed to deliver Screen Draw emergency pause");
            }
            multi_launcher::gui::send_event(multi_launcher::gui::WatchEvent::ScreenDrawEmergency);
            if let Ok(guard) = ctx.lock()
                && let Some(c) = &*guard
            {
                c.request_repaint();
            }
        }

        if screen_draw_actions.launch {
            multi_launcher::gui::send_event(multi_launcher::gui::WatchEvent::ScreenDrawStart);
            if let Ok(guard) = ctx.lock()
                && let Some(c) = &*guard
            {
                c.request_repaint();
            }
        }
        if screen_draw_actions.recover {
            multi_launcher::gui::send_event(multi_launcher::gui::WatchEvent::ScreenDrawRecover);
            if let Ok(guard) = ctx.lock()
                && let Some(c) = &*guard
            {
                c.request_repaint();
            }
        }

        if let Some(qt) = &quit_trigger
            && qt.take()
        {
            if let Some(service) = invocation_service.as_ref() {
                let _ = service.cancel_lifecycle(LifecycleCancellation::Shutdown);
            }
            radial_controller.shutdown();
            native_preview.cancel_all();
            radial_resources.shutdown(&mut radial_controller);
            if let Some(service) = invocation_service.as_mut() {
                service.stop();
            }
            listener.stop();
            if let Ok(guard) = ctx.lock()
                && let Some(c) = &*guard
            {
                c.send_viewport_cmd(egui::ViewportCommand::Close);
                c.request_repaint();
            }
            let _ = handle.join();
            break Ok(());
        }

        if screen_draw_actions.emergency
            || screen_draw_actions.launch
            || screen_draw_actions.recover
        {
            // Emergency owns the entire event-loop cycle, including any native
            // launcher notice already queued by the same physical chord.
            radial_notices.clear();
        }
        if exclusive {
            reject_radial_opens_while_exclusive(&mut radial_notices);
        }
        let mut grid_toggle_batch = VisibilityToggleBatch::default();
        let mut invocation_route_failed = false;
        for notice in radial_notices {
            if let Some(cancellation) = notice.cancellation {
                tracing::debug!(?cancellation, "radial invocation lifecycle cancelled");
                invocation_route_failed |= cancellation == LifecycleCancellation::HookFailure;
            }
            if let Some(error) = notice.error {
                tracing::error!(%error, "launcher invocation service failed closed");
            }
            for intent in &notice.intents {
                if let Some((id, menu_id, interaction, context_token)) =
                    external_radial_lifecycle_metadata(intent, &radial_document)
                {
                    external_radial_invocations.insert(id, (menu_id, interaction, context_token));
                }
            }
            for event in radial_controller.handle_intents(notice.intents, settings.always_on_top) {
                match event {
                    ControllerEvent::ToggleLegacyLauncher => {
                        let was_visible = grid_toggle_batch.record_toggle(&visibility);
                        radial_controller.handle_legacy_grid_toggle(was_visible);
                    }
                    ControllerEvent::ExternalSessionAdmitted { invocation_id } => {
                        let metadata = take_external_radial_metadata(
                            &mut external_radial_invocations,
                            invocation_id,
                        );
                        if let Some((menu_id, interaction, context_token)) = metadata {
                            if let Some(service) = invocation_service.as_ref() {
                                let _ =
                                    service.feedback(InvocationEvent::ExternalSessionAdmitted {
                                        id: invocation_id,
                                        menu_id,
                                        context_token,
                                        interaction,
                                    });
                            }
                        } else {
                            tracing::error!(
                                invocation = invocation_id.0,
                                "external radial admission lacked its registered metadata"
                            );
                        }
                    }
                    ControllerEvent::InvocationCompleted { invocation_id } => {
                        retire_external_radial_invocation(
                            &mut external_radial_invocations,
                            invocation_id,
                        );
                    }
                    ControllerEvent::Error(error) => {
                        tracing::error!(%error,"radial controller error");
                        multi_launcher::gui::send_event(
                            multi_launcher::gui::WatchEvent::RadialRuntimeDiagnostic(error),
                        );
                    }
                    ControllerEvent::Diagnostic(diagnostic) => {
                        multi_launcher::gui::send_event(
                            multi_launcher::gui::WatchEvent::RadialDiagnostic(diagnostic),
                        );
                    }
                    ControllerEvent::LayoutFailed { menu_id, error } => {
                        multi_launcher::gui::send_event(
                            multi_launcher::gui::WatchEvent::RadialRuntimeDiagnostic(format!(
                                "Radial menu {menu_id} could not be placed: {error:?}"
                            )),
                        );
                    }
                    ControllerEvent::SubmenuPlacementFailed {
                        session_id,
                        parent_frame_id,
                        parent_menu_id,
                        child_menu_id,
                        parent_presentation,
                        message,
                    } => {
                        multi_launcher::gui::send_event(
                            multi_launcher::gui::WatchEvent::RadialSubmenuPlacementFailure(
                                multi_launcher::gui::RadialPlacementFailureNotice {
                                    session_id,
                                    parent_frame_id,
                                    parent_menu_id,
                                    child_menu_id,
                                    parent_presentation,
                                    message,
                                },
                            ),
                        );
                    }
                    ControllerEvent::InvocationFailed {
                        invocation_id,
                        message,
                    } => {
                        retire_external_radial_invocation(
                            &mut external_radial_invocations,
                            invocation_id,
                        );
                        tracing::error!(%message,"radial invocation failed");
                        if let Some(service) = invocation_service.as_ref() {
                            let _ = service.cancel_lifecycle(LifecycleCancellation::HostFailure);
                        }
                    }
                    ControllerEvent::Opened {
                        invocation_id,
                        session_id,
                    } => {
                        retire_external_radial_invocation(
                            &mut external_radial_invocations,
                            invocation_id,
                        );
                        if let Some(service) = invocation_service.as_ref() {
                            let _ = service.feedback(InvocationEvent::RadialSessionOpened {
                                id: invocation_id,
                                session_id,
                            });
                        }
                    }
                    ControllerEvent::Closed {
                        invocation_id,
                        reason,
                        ..
                    } => {
                        retire_external_radial_invocation(
                            &mut external_radial_invocations,
                            invocation_id,
                        );
                        if let Some(service) = invocation_service.as_ref() {
                            let event = if reason
                                == multi_launcher::radial::native::CloseReason::ActionHandoff
                            {
                                multi_launcher::radial::invocation::InvocationEvent::RadialClosedForAction{id:invocation_id}
                            } else {
                                multi_launcher::radial::invocation::InvocationEvent::RadialSessionClosed{id:invocation_id}
                            };
                            let _ = service.feedback(event);
                        }
                    }
                    ControllerEvent::DispatchRequested(request) => {
                        multi_launcher::gui::send_event(
                            multi_launcher::gui::WatchEvent::RadialDispatch(request),
                        );
                    }
                    ControllerEvent::InvocationReleaseAcknowledged { .. } => {}
                    ControllerEvent::PrepareRequested(envelope) => {
                        multi_launcher::gui::send_event(
                            multi_launcher::gui::WatchEvent::RadialPrepare(envelope),
                        );
                    }
                }
            }
        }
        if invocation_route_failed {
            pending_launcher_route = None;
            tracing::error!(
                "launcher invocation lifecycle failed; disabling radial runtime and restoring legacy launcher route"
            );
            multi_launcher::gui::send_event(
                multi_launcher::gui::WatchEvent::RadialRuntimeDiagnostic(
                    "The radial launcher input hook stopped unexpectedly. Radial menus were disabled and the legacy launcher hotkey was restored.".into(),
                ),
            );
            fail_closed_radial_route(
                &mut settings,
                &radial_control_endpoint,
                &mut radial_controller,
                &mut invocation_service,
                &mut listener,
                &trigger,
                quit_trigger.as_ref(),
                help_trigger.as_ref(),
                screen_draw_trigger.as_ref(),
                emergency_trigger.as_ref(),
                &event_tx,
            );
        }
        for event in radial_controller.poll() {
            match event {
                ControllerEvent::ExternalSessionAdmitted { invocation_id } => {
                    let metadata = take_external_radial_metadata(
                        &mut external_radial_invocations,
                        invocation_id,
                    );
                    if let Some((menu_id, interaction, context_token)) = metadata {
                        if let Some(service) = invocation_service.as_ref() {
                            let _ = service.feedback(InvocationEvent::ExternalSessionAdmitted {
                                id: invocation_id,
                                menu_id,
                                context_token,
                                interaction,
                            });
                        }
                    } else {
                        tracing::error!(
                            invocation = invocation_id.0,
                            "external radial admission lacked its registered metadata"
                        );
                    }
                }
                ControllerEvent::InvocationCompleted { invocation_id } => {
                    retire_external_radial_invocation(
                        &mut external_radial_invocations,
                        invocation_id,
                    );
                }
                ControllerEvent::Error(error) => {
                    tracing::error!(%error,"radial native host error");
                    multi_launcher::gui::send_event(
                        multi_launcher::gui::WatchEvent::RadialRuntimeDiagnostic(error),
                    );
                }
                ControllerEvent::Diagnostic(diagnostic) => {
                    multi_launcher::gui::send_event(
                        multi_launcher::gui::WatchEvent::RadialDiagnostic(diagnostic),
                    );
                }
                ControllerEvent::LayoutFailed { menu_id, error } => {
                    multi_launcher::gui::send_event(
                        multi_launcher::gui::WatchEvent::RadialRuntimeDiagnostic(format!(
                            "Radial menu {menu_id} could not be placed: {error:?}"
                        )),
                    );
                }
                ControllerEvent::SubmenuPlacementFailed {
                    session_id,
                    parent_frame_id,
                    parent_menu_id,
                    child_menu_id,
                    parent_presentation,
                    message,
                } => {
                    multi_launcher::gui::send_event(
                        multi_launcher::gui::WatchEvent::RadialSubmenuPlacementFailure(
                            multi_launcher::gui::RadialPlacementFailureNotice {
                                session_id,
                                parent_frame_id,
                                parent_menu_id,
                                child_menu_id,
                                parent_presentation,
                                message,
                            },
                        ),
                    );
                }
                ControllerEvent::InvocationFailed {
                    invocation_id,
                    message,
                } => {
                    retire_external_radial_invocation(
                        &mut external_radial_invocations,
                        invocation_id,
                    );
                    tracing::error!(%message,"radial native host failed");
                    if let Some(service) = invocation_service.as_ref() {
                        let _ = service.cancel_lifecycle(LifecycleCancellation::HostFailure);
                    }
                }
                ControllerEvent::Opened {
                    invocation_id,
                    session_id,
                } => {
                    retire_external_radial_invocation(
                        &mut external_radial_invocations,
                        invocation_id,
                    );
                    if let Some(service) = invocation_service.as_ref() {
                        let _ = service.feedback(InvocationEvent::RadialSessionOpened {
                            id: invocation_id,
                            session_id,
                        });
                    }
                }
                ControllerEvent::Closed {
                    invocation_id,
                    reason,
                    ..
                } => {
                    retire_external_radial_invocation(
                        &mut external_radial_invocations,
                        invocation_id,
                    );
                    if let Some(service) = invocation_service.as_ref() {
                        let event = if reason
                            == multi_launcher::radial::native::CloseReason::ActionHandoff
                        {
                            multi_launcher::radial::invocation::InvocationEvent::RadialClosedForAction{id:invocation_id}
                        } else {
                            multi_launcher::radial::invocation::InvocationEvent::RadialSessionClosed{id:invocation_id}
                        };
                        let _ = service.feedback(event);
                    }
                }
                ControllerEvent::ToggleLegacyLauncher => {
                    let was_visible = grid_toggle_batch.record_toggle(&visibility);
                    radial_controller.handle_legacy_grid_toggle(was_visible);
                }
                ControllerEvent::DispatchRequested(request) => {
                    multi_launcher::gui::send_event(
                        multi_launcher::gui::WatchEvent::RadialDispatch(request),
                    );
                }
                ControllerEvent::InvocationReleaseAcknowledged { .. } => {}
                ControllerEvent::PrepareRequested(envelope) => {
                    multi_launcher::gui::send_event(
                        multi_launcher::gui::WatchEvent::RadialPrepare(envelope),
                    );
                }
            }
        }
        if let Some(service) = invocation_service.as_ref() {
            let _ = service.set_active_menu(radial_controller.active_input_scope());
        }

        if let Some(ht) = &help_trigger
            && ht.take()
            && visibility.load(Ordering::SeqCst)
        {
            help_flag.store(true, Ordering::SeqCst);
            if let Ok(guard) = ctx.lock()
                && let Some(c) = &*guard
            {
                c.request_repaint();
            }
        }

        if let Ok(new_settings) = restart_rx.try_recv() {
            let _ =
                multi_launcher::gui::send_event(multi_launcher::gui::WatchEvent::RadialInvalidate);
            radial_controller.close(
                multi_launcher::radial::native::CloseReason::SettingsReload,
                None,
            );
            listener.stop();
            let mut runtime_settings = new_settings.clone();
            let radial_settings_issues = multi_launcher::radial::settings::validate(
                &runtime_settings.radial,
                &radial_document,
                &reserved_launcher_hotkeys(&runtime_settings),
            );
            if !radial_settings_issues.is_empty() {
                runtime_settings.radial.enabled = false;
                for issue in radial_settings_issues {
                    tracing::error!(field = issue.field, message = %issue.message, "invalid radial settings; radial runtime disabled fail-closed");
                    multi_launcher::gui::send_event(
                        multi_launcher::gui::WatchEvent::RadialRuntimeDiagnostic(issue.message),
                    );
                }
            }
            settings = runtime_settings;
            let tooltip_preferences =
                multi_launcher::radial::tooltip::TooltipPreferences::from(&settings.radial);
            for event in radial_controller.set_tooltip_preferences(tooltip_preferences) {
                if let ControllerEvent::Diagnostic(diagnostic) = event {
                    multi_launcher::gui::send_event(
                        multi_launcher::gui::WatchEvent::RadialDiagnostic(diagnostic),
                    );
                }
            }
            match native_preview.set_tooltip_preferences(tooltip_preferences) {
                Ok(Some(diagnostics)) => {
                    if let Some(lease) = native_preview.active_lease() {
                        let _ = authoring_endpoint.reply_tx.send(
                            AuthoringReply::NativePreviewDiagnostics {
                                editor_session: lease.editor_session,
                                lease,
                                diagnostics,
                            },
                        );
                    }
                }
                Ok(None) => {}
                Err(message) => multi_launcher::gui::send_event(
                    multi_launcher::gui::WatchEvent::RadialRuntimeDiagnostic(format!(
                        "Native radial preview could not apply tooltip settings: {message}"
                    )),
                ),
            }
            radial_control_endpoint.set_enabled(settings.radial.enabled);
            settings_generation = settings_generation.checked_add(1).unwrap_or(1);
            if let Ok(document) = radial_store.reload() {
                radial_document = document;
                multi_launcher::gui::install_radial_published_document(Arc::clone(
                    &radial_document,
                ));
                radial_controller.replace_document(Arc::clone(&radial_document));
            }
            trigger = Arc::new(HotkeyTrigger::new(settings.hotkey()));
            quit_trigger = settings
                .quit_hotkey()
                .map(|hk| Arc::new(HotkeyTrigger::new(hk)));
            help_trigger = settings
                .help_hotkey()
                .map(|hk| Arc::new(HotkeyTrigger::new(hk)));
            rebuild_screen_draw_triggers(
                &settings,
                &mut screen_draw_trigger,
                &mut emergency_trigger,
            );
            refresh_macro_hotkey_reservations(&settings, &radial_document);
            let route_plan = radial_route_plan(&settings, &radial_document, settings_generation);
            shared_invocation = route_plan.shared_invocation;
            if !settings.radial.enabled {
                radial_controller.disable();
            }
            radial_resources.set_runtime_enabled(settings.radial.enabled);
            radial_resources.reconcile(&mut radial_controller);
            pending_launcher_route = None;
            if let Some(service) = invocation_service.as_ref() {
                let cancellation = if route_plan.stop_service {
                    LifecycleCancellation::FeatureDisabled
                } else {
                    LifecycleCancellation::SettingsReload
                };
                match service.begin_route_handoff(
                    route_plan.config,
                    route_plan.related,
                    cancellation,
                ) {
                    Ok(handoff) => {
                        pending_launcher_route = Some(PendingLauncherRoute {
                            settings_generation,
                            handoff,
                            start_legacy: route_plan.start_legacy,
                            stop_service: route_plan.stop_service,
                        });
                    }
                    Err(error) => {
                        tracing::error!(%error, "launcher route handoff could not be started; remaining fail-closed");
                        settings.radial.enabled = false;
                        radial_control_endpoint.set_enabled(false);
                        radial_controller.disable();
                        if let Some(mut service) = invocation_service.take() {
                            service.stop();
                        }
                        listener = HotkeyTrigger::start_listener(
                            legacy_listener_triggers(
                                false,
                                &trigger,
                                quit_trigger.as_ref(),
                                help_trigger.as_ref(),
                                screen_draw_trigger.as_ref(),
                                emergency_trigger.as_ref(),
                            ),
                            "main",
                            event_tx.clone(),
                        );
                    }
                }
            } else if settings.radial.enabled {
                let owner_bridge = Arc::clone(&screen_draw_recovery_bridge);
                match LauncherInvocationService::start(
                    route_plan.config,
                    route_plan.related,
                    Arc::new(move || {
                        if owner_bridge.is_active() {
                            PriorityOwner::ScreenDrawRecovery
                        } else if exclusive_owners() != 0 {
                            PriorityOwner::ExclusiveTool
                        } else {
                            PriorityOwner::Launcher
                        }
                    }),
                    event_tx.clone(),
                ) {
                    Ok(service) => {
                        invocation_service = Some(service);
                        if !shared_invocation {
                            listener = HotkeyTrigger::start_listener(
                                legacy_listener_triggers(
                                    false,
                                    &trigger,
                                    quit_trigger.as_ref(),
                                    help_trigger.as_ref(),
                                    screen_draw_trigger.as_ref(),
                                    emergency_trigger.as_ref(),
                                ),
                                "main",
                                event_tx.clone(),
                            );
                        }
                    }
                    Err(error) => {
                        tracing::error!(%error,"shared radial invocation reload failed; disabling radial runtime and restoring legacy launcher route");
                        fail_closed_radial_route(
                            &mut settings,
                            &radial_control_endpoint,
                            &mut radial_controller,
                            &mut invocation_service,
                            &mut listener,
                            &trigger,
                            quit_trigger.as_ref(),
                            help_trigger.as_ref(),
                            screen_draw_trigger.as_ref(),
                            emergency_trigger.as_ref(),
                            &event_tx,
                        );
                    }
                }
            } else {
                listener = HotkeyTrigger::start_listener(
                    legacy_listener_triggers(
                        false,
                        &trigger,
                        quit_trigger.as_ref(),
                        help_trigger.as_ref(),
                        screen_draw_trigger.as_ref(),
                        emergency_trigger.as_ref(),
                    ),
                    "main",
                    event_tx.clone(),
                );
            }
        }

        // Consolidate every fail-closed path through the same demand owner.
        // This is idempotent, so repeated disable/error notifications cannot
        // construct or retire a resource set more than once.
        radial_resources.set_runtime_enabled(settings.radial.enabled);
        radial_resources.reconcile(&mut radial_controller);

        let visibility_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let grid_toggles_changed = handle_visibility_toggle_batch(
                &grid_toggle_batch,
                &restore_flag,
                &ctx,
                &mut queued_visibility,
                {
                    let (x, y) = settings.offscreen_pos.unwrap_or((2000, 2000));
                    (x as f32, y as f32)
                },
                settings.follow_mouse,
                settings.static_location_enabled,
                settings.static_pos.map(|(x, y)| (x as f32, y as f32)),
                settings.static_size.map(|(w, h)| (w as f32, h as f32)),
                {
                    let (w, h) = settings.window_size.unwrap_or((400, 220));
                    (w as f32, h as f32)
                },
            );
            let legacy_trigger_changed = handle_visibility_trigger_with_owner(
                trigger.as_ref(),
                &visibility,
                &restore_flag,
                &ctx,
                &mut queued_visibility,
                {
                    let (x, y) = settings.offscreen_pos.unwrap_or((2000, 2000));
                    (x as f32, y as f32)
                },
                settings.follow_mouse,
                settings.static_location_enabled,
                settings.static_pos.map(|(x, y)| (x as f32, y as f32)),
                settings.static_size.map(|(w, h)| (w as f32, h as f32)),
                {
                    let (w, h) = settings.window_size.unwrap_or((400, 220));
                    (w as f32, h as f32)
                },
                |was_visible| radial_controller.handle_legacy_grid_toggle(was_visible),
            );
            grid_toggles_changed || legacy_trigger_changed
        }));

        match visibility_result {
            Ok(true) => {
                let _ = event_tx.send(());
            }
            Ok(false) => {}
            Err(_panic_payload) => {
                tracing::error!("visibility handler panicked; continuing event loop");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disabled_radial_resources() -> RadialRuntimeResources {
        let (wake, _rx) = channel();
        RadialRuntimeResources::new(PathBuf::from("unused-test-root"), wake, false)
    }

    #[test]
    fn disabled_startup_has_no_runtime_resource_transition() {
        let (wake, wake_rx) = channel();
        let mut resources =
            RadialRuntimeResources::new(PathBuf::from("unused-test-root"), wake, false);
        let (controller_wake, _controller_wake_rx) = channel();
        let controller =
            RadialController::new(Arc::new(RadialDocument::starter()), false, controller_wake);
        assert_eq!(resources.take_transition(), RadialResourceTransition::None);
        assert!(!resources.active);
        assert!(resources.watcher.is_none());
        assert!(resources.authoring_sessions.is_empty());
        assert!(resources.auditions.is_empty());
        assert!(!controller.resources_configured());
        assert!(wake_rx.try_recv().is_err(), "disabled startup repainted");
    }

    #[test]
    fn editor_demand_acquires_and_releases_without_enabling_runtime() {
        let mut resources = disabled_radial_resources();
        let editor = AuthoringSessionId(7);
        resources.set_authoring_demand(AuthoringResourceDemand::Acquire(editor));
        assert_eq!(
            resources.take_transition(),
            RadialResourceTransition::Acquire
        );
        assert!(!resources.runtime_enabled);
        assert_eq!(resources.take_transition(), RadialResourceTransition::None);
        resources.set_authoring_demand(AuthoringResourceDemand::Release(editor));
        assert_eq!(
            resources.take_transition(),
            RadialResourceTransition::Release
        );
        assert_eq!(resources.take_transition(), RadialResourceTransition::None);
    }

    #[test]
    fn repeated_enable_disable_transitions_construct_and_retire_once() {
        let mut resources = disabled_radial_resources();
        let mut transitions = Vec::new();
        for enabled in [true, true, false, false, true, true, false, false] {
            resources.set_runtime_enabled(enabled);
            let transition = resources.take_transition();
            if transition != RadialResourceTransition::None {
                transitions.push(transition);
            }
        }
        assert_eq!(
            transitions,
            vec![
                RadialResourceTransition::Acquire,
                RadialResourceTransition::Release,
                RadialResourceTransition::Acquire,
                RadialResourceTransition::Release,
            ]
        );
    }

    #[test]
    fn build_viewport_with_invalid_icon_bytes_does_not_panic() {
        let settings = Settings::default();
        let result = std::panic::catch_unwind(|| build_viewport_with_icon(&settings, b"not-a-png"));
        assert!(result.is_ok());
    }

    #[test]
    fn radial_route_plan_coherently_hands_off_enable_and_disable() {
        let document = RadialDocument::starter();
        let enabled = Settings::default();
        let enabled_plan = radial_route_plan(&enabled, &document, 41);
        assert!(enabled_plan.shared_invocation);
        assert!(!enabled_plan.start_legacy);
        assert!(!enabled_plan.stop_service);
        assert!(enabled_plan.config.launcher_enabled);
        assert_eq!(enabled_plan.config.generation, 41);

        let mut disabled = enabled;
        disabled.radial.enabled = false;
        let disabled_plan = radial_route_plan(&disabled, &document, 42);
        assert!(!disabled_plan.shared_invocation);
        assert!(disabled_plan.start_legacy);
        assert!(disabled_plan.stop_service);
        assert!(!disabled_plan.config.launcher_enabled);
        assert!(disabled_plan.related.is_empty());
        assert_eq!(disabled_plan.config.generation, 42);
    }

    #[test]
    fn route_start_failures_disable_control_and_restore_legacy_plan() {
        let (event_tx, _event_rx) = channel();
        let mut settings = Settings::default();
        settings.radial.enabled = true;
        let document = Arc::new(RadialDocument::starter());
        let launcher = Arc::new(HotkeyTrigger::new(settings.hotkey()));
        let (client, endpoint) = radial_control_service_with_wake(true, None);
        let mut controller = RadialController::new(document, false, event_tx.clone());
        let mut service = None;
        let mut listener = HotkeyTrigger::start_listener(Vec::new(), "main", event_tx.clone());

        fail_closed_radial_route(
            &mut settings,
            &endpoint,
            &mut controller,
            &mut service,
            &mut listener,
            &launcher,
            None,
            None,
            None,
            None,
            &event_tx,
        );

        assert!(!settings.radial.enabled);
        assert!(!client.is_enabled());
        assert!(matches!(
            client.send(RadialControlRequest::Show(
                multi_launcher::radial::control::RadialMenuSelector::Default
            )),
            Err(multi_launcher::radial::control::RadialControlError::Disabled)
        ));
        assert!(service.is_none());
        listener.stop();
    }

    #[test]
    fn radial_invocation_uses_configured_default_menu() {
        let mut document = RadialDocument::starter();
        let mut second = document.menus[0].clone();
        second.id = multi_launcher::radial::model::MenuId::new("work");
        second.name = "Work".into();
        second.interaction = InteractionMode::ReleaseToSelect;
        document.menus.push(second);
        let mut settings = Settings::default();
        settings.radial.default_menu_id = Some(multi_launcher::radial::model::MenuId::new("work"));
        let config = radial_invocation_config(&settings, &document, true, 9);
        assert_eq!(config.menu_id.as_str(), "work");
        assert_eq!(config.interaction, InteractionMode::ReleaseToSelect);
    }

    #[test]
    fn screen_draw_launch_hotkey_is_reserved_and_parsed() {
        let mut settings = Settings::default();
        settings.plugin_settings.insert(
            "screen_draw".into(),
            serde_json::json!({ "launch_hotkey": "Ctrl+Shift+D" }),
        );
        assert_eq!(
            screen_draw_launch_hotkey_text(&settings),
            Ok(Some("Ctrl+Shift+D".into()))
        );
        assert!(
            reserved_launcher_hotkeys(&settings)
                .contains(&("launch Screen Draw".into(), "Ctrl+Shift+D".into()))
        );
        assert!(screen_draw_launch_trigger(&settings).is_some());
    }

    #[test]
    fn invalid_screen_draw_launch_hotkey_is_safely_disabled() {
        let mut settings = Settings::default();
        settings.plugin_settings.insert(
            "screen_draw".into(),
            serde_json::json!({ "launch_hotkey": "Ctrl+DefinitelyNotAKey" }),
        );
        assert!(screen_draw_launch_hotkey_text(&settings).is_err());
        assert!(screen_draw_launch_trigger(&settings).is_none());
        assert!(
            !reserved_launcher_hotkeys(&settings)
                .iter()
                .any(|(name, _)| name == "launch Screen Draw")
        );
    }

    #[test]
    fn conflicting_screen_draw_launch_hotkey_is_safely_disabled() {
        let mut settings = Settings {
            quit_hotkey: Some("Ctrl+Q".into()),
            help_hotkey: Some("Ctrl+H".into()),
            ..Settings::default()
        };
        for chord in ["F2", "Ctrl+Q", "Ctrl+H"] {
            settings.plugin_settings.insert(
                "screen_draw".into(),
                serde_json::json!({ "launch_hotkey": chord }),
            );
            let error = screen_draw_launch_hotkey_text(&settings).unwrap_err();
            assert!(error.contains("conflicts"), "{error}");
            assert!(screen_draw_launch_trigger(&settings).is_none());
        }
    }

    #[test]
    fn emergency_hotkey_is_process_reserved_and_wins_screen_draw_conflict() {
        let mut settings = Settings::default();
        settings.plugin_settings.insert(
            "screen_draw".into(),
            serde_json::json!({
                "launch_hotkey": "Ctrl+Shift+F11",
                "emergency_hotkey": "Ctrl+Shift+F11"
            }),
        );
        assert!(screen_draw_launch_hotkey_text(&settings).is_err());
        assert_eq!(
            screen_draw_emergency_hotkey_text(&settings),
            Ok(Some("Ctrl+Shift+F11".into()))
        );
        let reserved = reserved_launcher_hotkeys(&settings);
        assert!(
            !reserved
                .iter()
                .any(|(name, _)| name == "launch Screen Draw")
        );
        assert!(reserved.contains(&("Screen Draw emergency".into(), "Ctrl+Shift+F11".into())));
    }

    #[test]
    fn emergency_hotkey_conflicting_with_launcher_is_disabled() {
        let mut settings = Settings::default();
        settings.plugin_settings.insert(
            "screen_draw".into(),
            serde_json::json!({ "emergency_hotkey": "F2" }),
        );
        assert!(screen_draw_emergency_hotkey_text(&settings).is_err());
        assert!(screen_draw_emergency_trigger(&settings).is_none());
        assert!(
            !reserved_launcher_hotkeys(&settings)
                .iter()
                .any(|(name, _)| name == "Screen Draw emergency")
        );
    }

    #[test]
    fn active_screen_draw_consumes_launcher_trigger_exactly_once() {
        let trigger = HotkeyTrigger::new(parse_hotkey("F2").unwrap());
        *trigger.open.lock().unwrap() = true;

        assert!(take_screen_draw_recovery_trigger(&trigger, true));
        assert!(!trigger.take());

        *trigger.open.lock().unwrap() = true;
        assert!(!take_screen_draw_recovery_trigger(&trigger, false));
        assert!(trigger.take());
    }

    #[test]
    fn same_primary_conflicts_are_rejected_in_both_modifier_directions() {
        for (launcher, emergency) in [("F12", "Ctrl+Shift+F12"), ("Ctrl+Shift+F12", "F12")] {
            let mut settings = Settings {
                hotkey: Some(launcher.into()),
                ..Settings::default()
            };
            settings.plugin_settings.insert(
                "screen_draw".into(),
                serde_json::json!({ "emergency_hotkey": emergency }),
            );
            assert!(screen_draw_emergency_hotkey_text(&settings).is_err());

            settings.plugin_settings.insert(
                "screen_draw".into(),
                serde_json::json!({ "launch_hotkey": emergency }),
            );
            assert!(screen_draw_launch_hotkey_text(&settings).is_err());
        }
    }

    #[test]
    fn unmodified_caps_lock_does_not_cofire_with_modified_caps_lock() {
        let plain = parse_hotkey("CapsLock").unwrap();
        let modified = parse_hotkey("Ctrl+CapsLock").unwrap();

        assert!(!hotkeys_can_cofire(plain, modified));
        assert!(!hotkeys_can_cofire(modified, plain));
        assert!(hotkeys_can_cofire(plain, plain));

        for (launcher, emergency) in [("CapsLock", "Ctrl+CapsLock"), ("Ctrl+CapsLock", "CapsLock")]
        {
            let mut settings = Settings {
                hotkey: Some(launcher.into()),
                ..Settings::default()
            };
            settings.plugin_settings.insert(
                "screen_draw".into(),
                serde_json::json!({ "emergency_hotkey": emergency }),
            );
            assert_eq!(
                screen_draw_emergency_hotkey_text(&settings),
                Ok(Some(emergency.into()))
            );
        }
    }

    #[test]
    fn emergency_wins_same_primary_launch_conflict_with_different_modifiers() {
        for (launch, emergency) in [("F12", "Ctrl+Shift+F12"), ("Ctrl+Shift+F12", "F12")] {
            let mut settings = Settings::default();
            settings.plugin_settings.insert(
                "screen_draw".into(),
                serde_json::json!({
                    "launch_hotkey": launch,
                    "emergency_hotkey": emergency
                }),
            );
            assert!(screen_draw_launch_hotkey_text(&settings).is_err());
            assert_eq!(
                screen_draw_emergency_hotkey_text(&settings),
                Ok(Some(emergency.into()))
            );
        }
    }

    #[test]
    fn same_loop_launch_then_summon_routes_recovery_without_visibility() {
        let bridge = ScreenDrawRecoveryBridge::default();
        let launcher = HotkeyTrigger::new(parse_hotkey("F2").unwrap());
        let launch = HotkeyTrigger::new(parse_hotkey("Ctrl+Shift+D").unwrap());
        *launcher.open.lock().unwrap() = true;
        *launch.open.lock().unwrap() = true;

        let actions = take_screen_draw_trigger_actions(&launcher, Some(&launch), None, &bridge);

        assert_eq!(
            actions,
            ScreenDrawTriggerActions {
                launch: true,
                recover: true,
                emergency: false,
            }
        );
        assert!(bridge.is_active());
        assert!(!launcher.take(), "visibility must not see the summon edge");
    }

    #[test]
    fn emergency_priority_consumes_defensive_cofire_without_double_route() {
        let bridge = ScreenDrawRecoveryBridge::default();
        bridge.stage_start();
        let launcher = HotkeyTrigger::new(parse_hotkey("F12").unwrap());
        let launch = HotkeyTrigger::new(parse_hotkey("Ctrl+F12").unwrap());
        let emergency = HotkeyTrigger::new(parse_hotkey("Shift+F12").unwrap());
        for trigger in [&launcher, &launch, &emergency] {
            *trigger.open.lock().unwrap() = true;
        }

        assert_eq!(
            take_screen_draw_trigger_actions(&launcher, Some(&launch), Some(&emergency), &bridge,),
            ScreenDrawTriggerActions {
                emergency: true,
                ..Default::default()
            }
        );
        assert!(!launcher.take());
        assert!(!launch.take());
        assert!(!emergency.take());
    }

    #[test]
    fn settings_reload_replaces_screen_draw_triggers_without_carrying_edges() {
        let mut settings = Settings::default();
        settings.plugin_settings.insert(
            "screen_draw".into(),
            serde_json::json!({
                "launch_hotkey": "Ctrl+Shift+F11",
                "emergency_hotkey": "Ctrl+Shift+F12"
            }),
        );
        let mut launch = None;
        let mut emergency = None;
        rebuild_screen_draw_triggers(&settings, &mut launch, &mut emergency);
        let old_launch = launch.as_ref().unwrap().clone();
        let old_emergency = emergency.as_ref().unwrap().clone();
        *old_launch.open.lock().unwrap() = true;
        *old_emergency.open.lock().unwrap() = true;

        settings.plugin_settings.insert(
            "screen_draw".into(),
            serde_json::json!({
                "launch_hotkey": "Ctrl+Shift+F9",
                "emergency_hotkey": "Ctrl+Shift+F10"
            }),
        );
        rebuild_screen_draw_triggers(&settings, &mut launch, &mut emergency);

        assert!(!Arc::ptr_eq(&old_launch, launch.as_ref().unwrap()));
        assert!(!Arc::ptr_eq(&old_emergency, emergency.as_ref().unwrap()));
        assert_eq!(
            launch.as_ref().unwrap()._key,
            parse_hotkey("F9").unwrap().key
        );
        assert_eq!(
            emergency.as_ref().unwrap()._key,
            parse_hotkey("F10").unwrap().key
        );
        assert!(!launch.as_ref().unwrap().take());
        assert!(!emergency.as_ref().unwrap().take());
        assert!(old_launch.take());
        assert!(old_emergency.take());
    }

    #[test]
    fn shared_disabled_route_still_registers_stable_direct_trigger_identity_only() {
        let settings = Settings::default();
        let mut document = RadialDocument::starter();
        document
            .custom_triggers
            .push(multi_launcher::radial::model::TriggerDefinition {
                id: multi_launcher::radial::model::TriggerId::new("tools-hotkey"),
                chord: "Ctrl+Alt+T".into(),
                menu_id: document.default_menu_id.clone(),
                scope: multi_launcher::radial::model::TriggerScope::Global,
            });
        let bindings = related_launcher_bindings(&settings, &document, false);
        assert!(bindings.iter().any(|binding| matches!(
            &binding.action,
            RelatedAction::DirectMenu { trigger_id, menu_id }
                if trigger_id == "tools-hotkey" && menu_id == &document.default_menu_id
        )));
        assert!(
            bindings
                .iter()
                .all(|binding| matches!(binding.action, RelatedAction::DirectMenu { .. }))
        );
    }

    #[test]
    fn global_item_shortcuts_require_opt_in_before_reserving_mkmacro_ownership() {
        let mut settings = Settings::default();
        let mut document = RadialDocument::starter();
        document.menus[0].rings[0].cells[0].shortcuts.push(
            multi_launcher::radial::model::ItemShortcut {
                id: multi_launcher::radial::model::ShortcutId::new("global-item"),
                chord: "Ctrl+K".into(),
                gesture: multi_launcher::radial::model::ClickGesture::Primary,
                scope: multi_launcher::radial::model::TriggerScope::Global,
            },
        );
        assert!(radial_global_hotkey_reservations(&settings, &document).is_empty());
        settings.radial.global_item_inputs = true;
        assert_eq!(
            radial_global_hotkey_reservations(&settings, &document),
            vec![("radial item shortcut global-item".into(), "Ctrl+K".into())]
        );
    }

    #[test]
    fn queued_radial_opens_are_rejected_for_an_exclusive_cycle() {
        let mut notices = vec![multi_launcher::hotkey::launcher_invocation::ServiceNotice {
            recovery: false,
            intents: vec![
                multi_launcher::radial::invocation::InvocationIntent::OpenRadial {
                    id: multi_launcher::radial::model::InvocationId(1),
                    menu_id: multi_launcher::radial::model::MenuId::new("starter"),
                    context_token: 0,
                    interaction: InteractionMode::StickyClick,
                    trigger_still_down: true,
                },
                multi_launcher::radial::invocation::InvocationIntent::ToggleDirectMenu {
                    id: multi_launcher::radial::model::InvocationId(2),
                    menu_id: multi_launcher::radial::model::MenuId::new("starter"),
                    primary_key: 0x54,
                    provenance: multi_launcher::radial::invocation::InputProvenance::Physical,
                    trigger_still_down: true,
                },
                multi_launcher::radial::invocation::InvocationIntent::ToggleLegacyLauncher {
                    id: multi_launcher::radial::model::InvocationId(3),
                },
                multi_launcher::radial::invocation::InvocationIntent::ActivateItem {
                    id: multi_launcher::radial::model::InvocationId(4),
                    menu_id: multi_launcher::radial::model::MenuId::new("starter"),
                    cell_id: multi_launcher::radial::model::CellId::new("starter-0"),
                    gesture: multi_launcher::radial::model::ClickGesture::Primary,
                    scope: multi_launcher::radial::model::TriggerScope::Global,
                    source: multi_launcher::commands::ActivationSource::RadialShortcut,
                    trigger_still_down: true,
                },
                multi_launcher::radial::invocation::InvocationIntent::CancelDeadline {
                    id: multi_launcher::radial::model::InvocationId(1),
                },
            ],
            error: None,
            action: None,
            cancellation: None,
        }];
        reject_radial_opens_while_exclusive(&mut notices);
        assert!(matches!(
            notices[0].intents.as_slice(),
            [multi_launcher::radial::invocation::InvocationIntent::CancelDeadline { .. }]
        ));
    }

    #[test]
    fn external_radial_terminal_metadata_is_retired_exactly_once() {
        let mut invocations = HashMap::new();
        let id = InvocationId(9_001);
        let metadata = (
            multi_launcher::radial::model::MenuId::new("starter"),
            InteractionMode::StickyClick,
            id.0,
        );
        invocations.insert(id, metadata.clone());
        assert_eq!(
            take_external_radial_metadata(&mut invocations, id),
            Some(metadata)
        );
        assert!(!retire_external_radial_invocation(&mut invocations, id));

        for raw_id in 9_002..9_012 {
            let id = InvocationId(raw_id);
            invocations.insert(
                id,
                (
                    multi_launcher::radial::model::MenuId::new("starter"),
                    InteractionMode::StickyClick,
                    id.0,
                ),
            );
            assert!(retire_external_radial_invocation(&mut invocations, id));
        }
        assert!(invocations.is_empty());
    }

    #[test]
    fn external_open_metadata_preserves_requested_context_and_interaction() {
        let document = RadialDocument::starter();
        let id = InvocationId(9_100);
        let intent = InvocationIntent::OpenExternalRadial {
            id,
            menu_id: document.default_menu_id.clone(),
            context_token: 42,
            interaction: InteractionMode::HoldAndClick,
            trigger_still_down: false,
        };
        assert_eq!(
            external_radial_lifecycle_metadata(&intent, &document),
            Some((
                id,
                document.default_menu_id.clone(),
                InteractionMode::HoldAndClick,
                42,
            ))
        );
    }
}
