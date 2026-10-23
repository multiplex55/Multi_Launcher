#![windows_subsystem = "windows"]
#![allow(clippy::type_complexity)]

use multi_launcher as startup_runtime;
use multi_launcher::actions::{Action, load_startup_actions};
use multi_launcher::common::persistence::PersistenceError;
use multi_launcher::gui::LauncherApp;
#[cfg(feature = "qa-framebuffer-capture")]
use multi_launcher::gui::QaFramebufferRequest;
use multi_launcher::hotkey::launcher_invocation::{
    InvocationConfig, LauncherInvocationService, PasteSequenceHotkeyAction, PriorityOwner,
    RelatedAction, RelatedBinding, RouteHandoff, ServiceNotice, exclusive_owners,
    install_exclusive_wake,
};
use multi_launcher::hotkey::{HotkeyListener, HotkeyTrigger, parse_hotkey};
use multi_launcher::logging;
use multi_launcher::paste_sequence::admission::{
    PhysicalPasteAvailability, PhysicalPastePolicy, physical_paste_bridge,
};
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
use multi_launcher::screen_draw::{
    ScreenDrawRecoveryBridge, ScreenDrawRecoveryIntent, ScreenDrawRecoveryKind, ScreenDrawSettings,
};
use multi_launcher::settings::Settings;
use multi_launcher::startup::{SettingsStartupDiagnostic, load_startup_preload};
use multi_launcher::visibility::{
    RootViewportCtx, RootWindowBridge, ViewportCtx, VisibilityRevision, VisibilityToggleBatch,
    handle_visibility_toggle_batch_ordered, handle_visibility_trigger_with_owner_ordered,
};
use startup_catalog::{StartupIndexingTransfer, startup_action_catalog};
#[cfg(test)]
use startup_catalog::{startup_indexed_actions, track_c_startup_phase_observer};

use eframe::{egui, icon_data};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc::{Receiver, RecvError, Sender, channel},
};
use std::thread;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, VecDeque},
    path::PathBuf,
    time::{Duration, Instant},
};

const RADIAL_ACCEPTANCE_PREPARE_HOLD_FILE_ENV: &str =
    "MULTI_LAUNCHER_RADIAL_ACCEPTANCE_PREPARE_HOLD_FILE";

#[path = "startup_catalog.rs"]
mod startup_catalog;

fn startup_index_coordinator_for_owner() -> Result<
    multi_launcher::indexer::coordinator::IndexCoordinator,
    multi_launcher::indexer::coordinator::CoordinatorError,
> {
    multi_launcher::indexer::coordinator::IndexCoordinator::new()
}

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

#[derive(Clone, Debug, PartialEq, Eq)]
struct PasteSequenceHotkeyRoute {
    hotkey: multi_launcher::hotkey::Hotkey,
    chord: String,
    action: PasteSequenceHotkeyAction,
}

fn paste_sequence_hotkey_routes(
    settings: &Settings,
) -> Result<Vec<PasteSequenceHotkeyRoute>, String> {
    if settings
        .enabled_plugins
        .as_ref()
        .is_some_and(|plugins| !plugins.contains("paste_sequence"))
    {
        return Ok(Vec::new());
    }
    let snapshot = multi_launcher::paste_sequence::SequenceStore::new(
        multi_launcher::paste_sequence::PASTE_SEQUENCES_FILE,
    )
    .load()
    .map_err(|error| format!("Paste Sequence shortcuts could not be loaded: {error}"))?;
    paste_sequence_hotkey_routes_from_snapshot(settings, &snapshot)
}

fn paste_sequence_hotkey_routes_from_snapshot(
    settings: &Settings,
    snapshot: &multi_launcher::paste_sequence::SequenceSnapshot,
) -> Result<Vec<PasteSequenceHotkeyRoute>, String> {
    paste_sequence_hotkey_routes_from_bindings(settings, &snapshot.hotkey_bindings())
}

fn paste_sequence_hotkey_routes_from_bindings(
    settings: &Settings,
    bindings: &[multi_launcher::paste_sequence::SequenceHotkeyBinding],
) -> Result<Vec<PasteSequenceHotkeyRoute>, String> {
    if settings
        .enabled_plugins
        .as_ref()
        .is_some_and(|plugins| !plugins.contains("paste_sequence"))
    {
        return Ok(Vec::new());
    }
    let mut routes = Vec::new();
    for (chord, action) in [
        (
            settings.paste_sequence.next_hotkey.as_deref(),
            PasteSequenceHotkeyAction::Next,
        ),
        (
            settings.paste_sequence.cancel_hotkey.as_deref(),
            PasteSequenceHotkeyAction::Cancel,
        ),
    ] {
        let Some(hotkey) =
            multi_launcher::settings::PasteSequencePreferences::parse_optional_hotkey(chord)
                .map_err(|error| format!("Paste Sequence {action:?}: {error}"))?
        else {
            continue;
        };
        routes.push(PasteSequenceHotkeyRoute {
            hotkey,
            chord: chord.unwrap_or_default().trim().into(),
            action,
        });
    }
    for binding in bindings {
        let hotkey = parse_hotkey(binding.chord())
            .ok_or_else(|| format!("sequence {} has an invalid shortcut", binding.id().get()))?;
        routes.push(PasteSequenceHotkeyRoute {
            hotkey,
            chord: binding.chord().to_owned(),
            action: PasteSequenceHotkeyAction::Start(binding.id()),
        });
    }
    Ok(routes)
}

#[cfg(test)]
fn reserved_launcher_hotkeys(settings: &Settings) -> Vec<(String, String)> {
    reserved_launcher_hotkeys_with_sequences(settings, &[])
}

fn reserved_launcher_hotkeys_with_sequences(
    settings: &Settings,
    sequence_routes: &[PasteSequenceHotkeyRoute],
) -> Vec<(String, String)> {
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
        reserved.push(("launch Screen Draw".into(), hotkey));
    }
    if let Ok(Some(hotkey)) = screen_draw_emergency_hotkey_text(settings) {
        reserved.push(("Screen Draw emergency".into(), hotkey));
    }
    for route in sequence_routes {
        let name = match route.action {
            PasteSequenceHotkeyAction::Start(id) => {
                format!("Paste Sequence {}", id.get())
            }
            PasteSequenceHotkeyAction::Next => "Paste Sequence Next".into(),
            PasteSequenceHotkeyAction::Cancel => "Paste Sequence Cancel".into(),
        };
        reserved.push((name, route.chord.clone()));
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
    multi_launcher::hotkey::validation::hotkeys_can_cofire(left, right)
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

#[cfg(test)]
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
    if settings.radial.enabled {
        owned.extend(
            document
                .custom_triggers
                .iter()
                .filter(|trigger| {
                    trigger.scope == multi_launcher::radial::model::TriggerScope::Global
                        && parse_hotkey(&trigger.chord).is_some()
                })
                .map(|trigger| {
                    (
                        format!("radial trigger {}", trigger.id.as_str()),
                        trigger.chord.clone(),
                    )
                }),
        );
    }
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

fn refresh_macro_hotkey_reservations(
    settings: &Settings,
    document: &RadialDocument,
    sequence_routes: &[PasteSequenceHotkeyRoute],
) {
    physical_paste_bridge().suspend_route_policy();
    let mut owned = reserved_launcher_hotkeys_with_sequences(settings, sequence_routes);
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
    sequence_routes: &[PasteSequenceHotkeyRoute],
) -> Vec<multi_launcher::radial::item_input::ItemInputBinding> {
    let reserved = reserved_launcher_hotkeys_with_sequences(settings, sequence_routes)
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
    sequence_routes: &[PasteSequenceHotkeyRoute],
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
        item_inputs: if settings.radial.enabled {
            radial_item_inputs(settings, document, sequence_routes)
        } else {
            Vec::new()
        },
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

#[cfg(test)]
fn related_launcher_bindings(
    settings: &Settings,
    document: &RadialDocument,
    include_process_hotkeys: bool,
) -> Vec<RelatedBinding> {
    let sequence_routes = match paste_sequence_hotkey_routes(settings) {
        Ok(routes) => routes,
        Err(error) => {
            tracing::warn!(%error, "Paste Sequence shortcuts are unavailable");
            Vec::new()
        }
    };
    related_launcher_bindings_with_sequences(
        settings,
        document,
        include_process_hotkeys,
        &sequence_routes,
    )
}

fn related_launcher_bindings_with_sequences(
    settings: &Settings,
    document: &RadialDocument,
    include_process_hotkeys: bool,
    sequence_routes: &[PasteSequenceHotkeyRoute],
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
    if include_process_hotkeys
        && let Ok(Some(text)) = screen_draw_launch_hotkey_text(settings)
        && let Some(hotkey) = parse_hotkey(&text)
    {
        bindings.push(RelatedBinding {
            hotkey,
            action: RelatedAction::ScreenDrawLaunch,
        });
    }
    if include_process_hotkeys
        && let Ok(Some(text)) = screen_draw_emergency_hotkey_text(settings)
        && let Some(hotkey) = parse_hotkey(&text)
    {
        bindings.push(RelatedBinding {
            hotkey,
            action: RelatedAction::ScreenDrawEmergency,
        });
    }
    for trigger in document.custom_triggers.iter().filter(|v| {
        settings.radial.enabled && v.scope == multi_launcher::radial::model::TriggerScope::Global
    }) {
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
    if include_process_hotkeys
        && settings
            .enabled_plugins
            .as_ref()
            .is_none_or(|plugins| plugins.contains("paste_sequence"))
    {
        bindings.extend(sequence_routes.iter().map(|route| RelatedBinding {
            hotkey: route.hotkey,
            action: RelatedAction::PasteSequence(route.action.clone()),
        }));
    }
    bindings
}

fn shared_drain_control_bindings(
    settings: &Settings,
    document: &RadialDocument,
    sequence_routes: &[PasteSequenceHotkeyRoute],
) -> Vec<RelatedBinding> {
    related_launcher_bindings_with_sequences(
        settings,
        document,
        settings.radial.enabled && settings.radial.shared_tap_hold,
        sequence_routes,
    )
    .into_iter()
    .filter(|binding| {
        matches!(
            binding.action,
            RelatedAction::Quit
                | RelatedAction::Help
                | RelatedAction::ScreenDrawLaunch
                | RelatedAction::ScreenDrawEmergency
                | RelatedAction::PasteSequence(_)
        )
    })
    .collect()
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

type PasteSequenceHotkeyTrigger = (Arc<HotkeyTrigger>, PasteSequenceHotkeyAction);

fn sequence_hotkey_triggers_from_routes(
    routes: &[PasteSequenceHotkeyRoute],
) -> Vec<PasteSequenceHotkeyTrigger> {
    routes
        .iter()
        .map(|route| {
            (
                Arc::new(HotkeyTrigger::new(route.hotkey)),
                route.action.clone(),
            )
        })
        .collect()
}

fn legacy_listener_triggers_with_sequences(
    shared_invocation: bool,
    launcher: &Arc<HotkeyTrigger>,
    quit: Option<&Arc<HotkeyTrigger>>,
    help: Option<&Arc<HotkeyTrigger>>,
    screen_draw: Option<&Arc<HotkeyTrigger>>,
    emergency: Option<&Arc<HotkeyTrigger>>,
    sequence_triggers: &[PasteSequenceHotkeyTrigger],
) -> Vec<Arc<HotkeyTrigger>> {
    if shared_invocation {
        return Vec::new();
    }
    let mut watched = vec![Arc::clone(launcher)];
    watched.extend(quit.cloned());
    watched.extend(help.cloned());
    watched.extend(screen_draw.cloned());
    watched.extend(emergency.cloned());
    watched.extend(
        sequence_triggers
            .iter()
            .map(|(trigger, _)| Arc::clone(trigger)),
    );
    watched
}

fn take_paste_sequence_trigger_actions(
    triggers: &[PasteSequenceHotkeyTrigger],
) -> Vec<PasteSequenceHotkeyAction> {
    triggers
        .iter()
        .filter_map(|(trigger, action)| {
            trigger.open.lock().ok().and_then(|mut open| {
                if std::mem::take(&mut *open) {
                    Some(action.clone())
                } else {
                    None
                }
            })
        })
        .collect()
}

struct PendingLauncherRoute {
    settings_generation: u64,
    handoff: Option<Box<dyn HotkeyRouteHandoffReceipt>>,
    policy_applied: Option<Result<(), String>>,
    completion: PendingLauncherRouteCompletion,
}

trait HotkeyRouteHandoffReceipt {
    fn try_policy_applied(&self) -> Option<Result<(), String>>;
    fn try_complete(&self) -> Option<Result<(), String>>;
}

impl HotkeyRouteHandoffReceipt for RouteHandoff {
    fn try_policy_applied(&self) -> Option<Result<(), String>> {
        RouteHandoff::try_policy_applied(self)
    }

    fn try_complete(&self) -> Option<Result<(), String>> {
        RouteHandoff::try_complete(self)
    }
}

trait HotkeyRouteHandoffIssuer {
    fn service_available(&self, service: Option<&LauncherInvocationService>) -> bool {
        service.is_some()
    }
    fn begin(
        &mut self,
        service: Option<&LauncherInvocationService>,
        config: InvocationConfig,
        related: Vec<RelatedBinding>,
        cancellation: LifecycleCancellation,
    ) -> Result<Box<dyn HotkeyRouteHandoffReceipt>, String>;
}

struct NativeHotkeyRouteHandoffIssuer;

impl HotkeyRouteHandoffIssuer for NativeHotkeyRouteHandoffIssuer {
    fn begin(
        &mut self,
        service: Option<&LauncherInvocationService>,
        config: InvocationConfig,
        related: Vec<RelatedBinding>,
        cancellation: LifecycleCancellation,
    ) -> Result<Box<dyn HotkeyRouteHandoffReceipt>, String> {
        service
            .ok_or_else(|| "launcher invocation service is unavailable".to_owned())?
            .begin_route_handoff(config, related, cancellation)
            .map(|handoff| Box::new(handoff) as Box<dyn HotkeyRouteHandoffReceipt>)
    }
}

enum PendingLauncherRouteCompletion {
    Reconcile {
        start_legacy: bool,
        stop_service: bool,
        paste_presentation: multi_launcher::paste_sequence::SequenceRoutePresentation,
    },
    Hotkey(Box<PendingHotkeyApplication>),
}

fn settings_hotkey_route_is_pending(pending: &Option<PendingLauncherRoute>) -> bool {
    pending.as_ref().is_some_and(|pending| {
        matches!(
            pending.completion,
            PendingLauncherRouteCompletion::Hotkey(_)
        )
    })
}

fn try_recv_when_hotkey_route_is_idle<T>(
    pending: &Option<PendingLauncherRoute>,
    receiver: &Receiver<T>,
) -> Option<T> {
    if settings_hotkey_route_is_pending(pending) {
        None
    } else {
        receiver.try_recv().ok()
    }
}

fn transition_process_controls_are_live(pending: &Option<PendingLauncherRoute>) -> bool {
    pending.as_ref().is_some_and(|pending| {
        matches!(
            &pending.completion,
            PendingLauncherRouteCompletion::Hotkey(application)
                if application.phase == HotkeyApplyPhase::RestoreOldRoute
                    || (application.old_service_existed
                        && application.phase == HotkeyApplyPhase::DrainOldRoute)
        )
    })
}

fn send_current_route_feedback(
    service: Option<&LauncherInvocationService>,
    pending_route: &Option<PendingLauncherRoute>,
    event: InvocationEvent,
) {
    if !settings_hotkey_route_is_pending(pending_route)
        && let Some(service) = service
    {
        let _ = service.feedback(event);
    }
}

fn apply_service_related_action(
    notice: &mut ServiceNotice,
    quit: Option<&Arc<HotkeyTrigger>>,
    help: Option<&Arc<HotkeyTrigger>>,
    screen_draw: Option<&Arc<HotkeyTrigger>>,
    emergency: Option<&Arc<HotkeyTrigger>>,
) {
    let Some(action) = notice.action.take() else {
        return;
    };
    let trigger = match action {
        RelatedAction::Quit => quit,
        RelatedAction::Help => help,
        RelatedAction::ScreenDrawLaunch => screen_draw,
        RelatedAction::ScreenDrawEmergency => emergency,
        RelatedAction::DirectMenu { .. } => None,
        RelatedAction::PasteSequence(action) => {
            multi_launcher::gui::send_event(multi_launcher::gui::WatchEvent::PasteSequenceHotkey(
                action,
            ));
            None
        }
    };
    if let Some(trigger) = trigger
        && let Ok(mut flag) = trigger.open.lock()
    {
        *flag = true;
    }
}

struct PreparedHotkeyRoute {
    settings: Settings,
    dependencies: HotkeyRouteDependencies,
    generation: multi_launcher::hotkey::reconfiguration::HotkeyRuntimeGeneration,
    listener: Option<HotkeyListener>,
    launcher: Arc<HotkeyTrigger>,
    quit: Option<Arc<HotkeyTrigger>>,
    help: Option<Arc<HotkeyTrigger>>,
    screen_draw: Option<Arc<HotkeyTrigger>>,
    emergency: Option<Arc<HotkeyTrigger>>,
    sequence_routes: Vec<PasteSequenceHotkeyRoute>,
    sequence_omissions: Vec<PasteSequenceRouteOmission>,
    physical_paste: PhysicalPastePolicy,
    sequence_triggers: Vec<PasteSequenceHotkeyTrigger>,
    staged_service: Option<LauncherInvocationService>,
}

struct PreparedLegacyHotkeyFallback {
    settings: Settings,
    listener: HotkeyListener,
    launcher: Arc<HotkeyTrigger>,
    quit: Option<Arc<HotkeyTrigger>>,
    help: Option<Arc<HotkeyTrigger>>,
    screen_draw: Option<Arc<HotkeyTrigger>>,
    emergency: Option<Arc<HotkeyTrigger>>,
    sequence_routes: Vec<PasteSequenceHotkeyRoute>,
    sequence_triggers: Vec<PasteSequenceHotkeyTrigger>,
}

#[derive(Clone, Debug, PartialEq)]
struct HotkeyRouteDependencies {
    launcher: multi_launcher::hotkey::Hotkey,
    quit: Option<multi_launcher::hotkey::Hotkey>,
    help: Option<multi_launcher::hotkey::Hotkey>,
    screen_draw_launch: Option<multi_launcher::hotkey::Hotkey>,
    screen_draw_emergency: Option<multi_launcher::hotkey::Hotkey>,
    radial: multi_launcher::radial::model::RadialFeatureSettings,
    paste_sequence: Vec<(multi_launcher::hotkey::Hotkey, PasteSequenceHotkeyAction)>,
}

impl HotkeyRouteDependencies {
    fn from_routes(
        settings: &Settings,
        routes: &multi_launcher::hotkey::validation::ValidatedHotkeyRoutes,
        sequence_routes: &[PasteSequenceHotkeyRoute],
    ) -> Self {
        Self {
            launcher: routes.launcher,
            quit: routes.quit,
            help: routes.help,
            screen_draw_launch: routes.screen_draw_launch,
            screen_draw_emergency: routes.screen_draw_emergency,
            radial: settings.radial.clone(),
            paste_sequence: sequence_routes
                .iter()
                .map(|route| (route.hotkey, route.action.clone()))
                .collect(),
        }
    }
}

fn keep_dialog_visibility_only_changed(previous: &Settings, candidate: &Settings) -> bool {
    let previous_value = previous.paste_sequence.keep_dialog_visible_during_playback;
    if previous_value == candidate.paste_sequence.keep_dialog_visible_during_playback {
        return false;
    }

    let mut comparable = candidate.clone();
    comparable
        .paste_sequence
        .keep_dialog_visible_during_playback = previous_value;
    match (
        serde_json::to_value(previous),
        serde_json::to_value(comparable),
    ) {
        (Ok(previous), Ok(candidate)) => previous == candidate,
        _ => false,
    }
}

fn active_hotkey_route_dependencies(
    settings: &Settings,
    radial_document: &RadialDocument,
    sequence_routes: &[PasteSequenceHotkeyRoute],
) -> Option<HotkeyRouteDependencies> {
    let macro_document =
        multi_launcher::mkmacro::runtime::shared_hotkey_document_snapshot().ok()?;
    let routes = multi_launcher::hotkey::validation::validate_hotkey_routes(
        settings,
        radial_document,
        macro_document.as_deref(),
    )
    .ok()?;
    Some(HotkeyRouteDependencies::from_routes(
        settings,
        &routes,
        sequence_routes,
    ))
}

fn validate_hotkey_route_candidate(
    settings: &Settings,
    radial_document: &RadialDocument,
) -> Result<
    multi_launcher::hotkey::validation::ValidatedHotkeyRoutes,
    multi_launcher::hotkey::reconfiguration::HotkeyReconfigurationError,
> {
    validate_hotkey_route_candidate_with_sequence_bindings(settings, radial_document, None)
}

fn validate_hotkey_route_candidate_with_sequence_bindings(
    settings: &Settings,
    radial_document: &RadialDocument,
    sequence_bindings: Option<&[multi_launcher::paste_sequence::SequenceHotkeyBinding]>,
) -> Result<
    multi_launcher::hotkey::validation::ValidatedHotkeyRoutes,
    multi_launcher::hotkey::reconfiguration::HotkeyReconfigurationError,
> {
    resolve_hotkey_route_candidate(settings, radial_document, sequence_bindings)
        .map(|resolved| resolved.base)
}

#[derive(Clone, Debug)]
struct PasteSequenceRouteOmission {
    action: PasteSequenceHotkeyAction,
    reason: String,
}

struct ResolvedHotkeyRouteCandidate {
    base: multi_launcher::hotkey::validation::ValidatedHotkeyRoutes,
    sequences: Vec<PasteSequenceHotkeyRoute>,
    omissions: Vec<PasteSequenceRouteOmission>,
    physical_paste: PhysicalPastePolicy,
}

fn resolve_hotkey_route_candidate(
    settings: &Settings,
    radial_document: &RadialDocument,
    sequence_bindings: Option<&[multi_launcher::paste_sequence::SequenceHotkeyBinding]>,
) -> Result<
    ResolvedHotkeyRouteCandidate,
    multi_launcher::hotkey::reconfiguration::HotkeyReconfigurationError,
> {
    let revision = physical_paste_bridge().route_revision();
    let macro_document = multi_launcher::mkmacro::runtime::shared_hotkey_document_snapshot()
        .map_err(multi_launcher::hotkey::reconfiguration::HotkeyReconfigurationError::Validation)?;
    let mut resolved = resolve_hotkey_route_candidate_with_macro(
        settings,
        radial_document,
        macro_document.as_deref(),
        sequence_bindings,
    )?;
    resolved.physical_paste.revision = revision;
    Ok(resolved)
}

fn resolve_hotkey_route_candidate_with_macro(
    settings: &Settings,
    radial_document: &RadialDocument,
    macro_document: Option<&multi_launcher::mkmacro::MkMacroDocument>,
    sequence_bindings: Option<&[multi_launcher::paste_sequence::SequenceHotkeyBinding]>,
) -> Result<
    ResolvedHotkeyRouteCandidate,
    multi_launcher::hotkey::reconfiguration::HotkeyReconfigurationError,
> {
    use multi_launcher::hotkey::reconfiguration::HotkeyReconfigurationError as Error;
    let base = multi_launcher::hotkey::validation::validate_hotkey_routes(
        settings,
        radial_document,
        macro_document,
    )
    .map_err(|issues| {
        Error::Validation(
            issues
                .into_iter()
                .map(|issue| format!("{}: {}", issue.field, issue.message))
                .collect::<Vec<_>>()
                .join("; "),
        )
    })?;
    let raw = paste_sequence_hotkey_routes_for_candidate(settings, sequence_bindings)
        .map_err(Error::Validation)?;
    // Saved starts have priority over optional built-in playback controls.
    let mut sequences = raw
        .iter()
        .filter(|route| matches!(route.action, PasteSequenceHotkeyAction::Start(_)))
        .cloned()
        .collect::<Vec<_>>();
    validate_paste_sequence_hotkey_routes(
        settings,
        radial_document,
        macro_document,
        &base,
        &sequences,
    )?;
    let defaults = multi_launcher::settings::PasteSequencePreferences::default();
    let mut omissions = Vec::new();
    for route in raw
        .into_iter()
        .filter(|route| !matches!(route.action, PasteSequenceHotkeyAction::Start(_)))
    {
        sequences.push(route.clone());
        if let Err(error) = validate_paste_sequence_hotkey_routes(
            settings,
            radial_document,
            macro_document,
            &base,
            &sequences,
        ) {
            let default = match route.action {
                PasteSequenceHotkeyAction::Next => defaults.next_hotkey.as_deref(),
                PasteSequenceHotkeyAction::Cancel => defaults.cancel_hotkey.as_deref(),
                PasteSequenceHotkeyAction::Start(_) => unreachable!(),
            }
            .and_then(parse_hotkey);
            if default != Some(route.hotkey) {
                return Err(error);
            }
            sequences.pop();
            omissions.push(PasteSequenceRouteOmission {
                action: route.action,
                reason: format!(
                    "{error}. Choose a different shortcut in Settings > Paste Sequences."
                ),
            });
        }
    }
    let physical_paste = PhysicalPastePolicy {
        revision: physical_paste_bridge().route_revision(),
        availability: physical_paste_availability(
            settings,
            radial_document,
            macro_document,
            &sequences,
        ),
    };
    Ok(ResolvedHotkeyRouteCandidate {
        base,
        sequences,
        omissions,
        physical_paste,
    })
}

/// Fixed Ctrl+V has no Shift/Alt/Win. Existing routes accept extra Control,
/// so both V and Ctrl+V can co-fire; routes requiring other modifiers cannot.
fn physical_paste_availability(
    settings: &Settings,
    radial_document: &RadialDocument,
    macro_document: Option<&multi_launcher::mkmacro::MkMacroDocument>,
    sequences: &[PasteSequenceHotkeyRoute],
) -> PhysicalPasteAvailability {
    let mut reserved = reserved_launcher_hotkeys_with_sequences(settings, sequences);
    if settings.radial.enabled {
        reserved.extend(radial_global_hotkey_reservations(settings, radial_document));
    }
    for (owner, chord) in reserved {
        if parse_hotkey(&chord).is_some_and(|hotkey| {
            hotkey.key == multi_launcher::hotkey::Key::KeyV
                && !hotkey.shift
                && !hotkey.alt
                && !hotkey.alt_gr
                && !hotkey.win
        }) {
            return PhysicalPasteAvailability::Conflict { owner };
        }
    }
    if let Some(document) = macro_document {
        let mut hotkeys = document
            .macros
            .iter()
            .filter(|entry| entry.enabled)
            .filter_map(|entry| {
                entry
                    .hotkey
                    .as_ref()
                    .map(|hotkey| (format!("macro {} ({})", entry.id, entry.name), hotkey))
            })
            .collect::<Vec<_>>();
        hotkeys.push((
            "MkMacro recording toggle".into(),
            &document.settings.record_toggle_hotkey,
        ));
        for (name, hotkey) in [
            (
                "MkMacro recorder pause/resume",
                document.settings.recorder.pause_resume_hotkey.as_ref(),
            ),
            (
                "MkMacro recorder marker",
                document.settings.recorder.marker_hotkey.as_ref(),
            ),
        ] {
            if let Some(hotkey) = hotkey {
                hotkeys.push((name.into(), hotkey));
            }
        }
        for (owner, hotkey) in hotkeys {
            if matches!(
                multi_launcher::mkmacro::hotkeys::canonical_hotkey(hotkey).as_str(),
                "V" | "CONTROL+V"
            ) {
                return PhysicalPasteAvailability::Conflict { owner };
            }
        }
    }
    PhysicalPasteAvailability::Available
}
fn active_physical_paste_policy(
    settings: &Settings,
    document: &RadialDocument,
    sequences: &[PasteSequenceHotkeyRoute],
) -> PhysicalPastePolicy {
    // Capture BEFORE acquiring the store snapshot; publication fences under its
    // write lock. A stale snapshot can therefore never re-acknowledge Available.
    let revision = physical_paste_bridge().route_revision();
    let availability = multi_launcher::mkmacro::runtime::shared_hotkey_document_snapshot()
        .map(|macros| physical_paste_availability(settings, document, macros.as_deref(), sequences))
        .unwrap_or_default();
    PhysicalPastePolicy {
        revision,
        availability,
    }
}

fn paste_sequence_presentation_for_polling(
    mut presentation: multi_launcher::paste_sequence::SequenceRoutePresentation,
) -> multi_launcher::paste_sequence::SequenceRoutePresentation {
    use multi_launcher::paste_sequence::SequenceShortcutPresentation;
    let supported = |chord: &str| {
        parse_hotkey(chord).is_some_and(|hotkey| {
            multi_launcher::hotkey::runtime::legacy_virtual_key(hotkey.key).is_some()
        })
    };
    for control in [&mut presentation.next, &mut presentation.cancel] {
        if let SequenceShortcutPresentation::Active(chord) = control
            && !supported(chord)
        {
            *control = SequenceShortcutPresentation::Inactive {
                configured: chord.clone(),
                reason: "The active polling listener does not support this primary key".into(),
            };
        }
    }
    presentation.starts.retain(|(_, chord)| supported(chord));
    presentation
}

fn paste_sequence_route_presentation(
    settings: &Settings,
    generation: u64,
    routes: &[PasteSequenceHotkeyRoute],
    omissions: &[PasteSequenceRouteOmission],
    physical_paste: PhysicalPastePolicy,
) -> multi_launcher::paste_sequence::SequenceRoutePresentation {
    use multi_launcher::paste_sequence::{SequenceRoutePresentation, SequenceShortcutPresentation};
    let control = |action: PasteSequenceHotkeyAction, configured: Option<&str>| {
        let Some(chord) = configured.map(str::trim).filter(|chord| !chord.is_empty()) else {
            return SequenceShortcutPresentation::Disabled;
        };
        if let Some(route) = routes.iter().find(|route| route.action == action) {
            return SequenceShortcutPresentation::Active(route.chord.clone());
        }
        let reason = omissions
            .iter()
            .find(|omission| omission.action == action)
            .map(|omission| omission.reason.clone())
            .unwrap_or_else(|| {
                if settings
                    .enabled_plugins
                    .as_ref()
                    .is_some_and(|enabled| !enabled.contains("paste_sequence"))
                {
                    "Paste Sequences plugin is disabled".into()
                } else {
                    "Shortcut unavailable; see the runtime diagnostic".into()
                }
            });
        SequenceShortcutPresentation::Inactive {
            configured: chord.into(),
            reason,
        }
    };
    let presentation = SequenceRoutePresentation {
        generation,
        physical_paste,
        next: control(
            PasteSequenceHotkeyAction::Next,
            settings.paste_sequence.next_hotkey.as_deref(),
        ),
        cancel: control(
            PasteSequenceHotkeyAction::Cancel,
            settings.paste_sequence.cancel_hotkey.as_deref(),
        ),
        starts: routes
            .iter()
            .filter_map(|route| {
                if let PasteSequenceHotkeyAction::Start(id) = route.action {
                    Some((id, route.chord.clone()))
                } else {
                    None
                }
            })
            .collect(),
        escape_cancel_enabled: settings.paste_sequence.escape_cancel_enabled,
        keep_dialog_visible_during_playback: settings
            .paste_sequence
            .keep_dialog_visible_during_playback,
    };
    if settings.radial.enabled && settings.radial.shared_tap_hold {
        presentation
    } else {
        paste_sequence_presentation_for_polling(presentation)
    }
}

fn publish_paste_sequence_route_presentation(
    mut presentation: multi_launcher::paste_sequence::SequenceRoutePresentation,
) {
    if !physical_paste_bridge().acknowledge_route_policy(&presentation.physical_paste) {
        presentation.physical_paste = PhysicalPastePolicy {
            revision: physical_paste_bridge().route_revision(),
            availability: PhysicalPasteAvailability::Unknown,
        };
    }
    multi_launcher::gui::send_event(multi_launcher::gui::WatchEvent::PasteSequenceRoutes(
        presentation,
    ));
}

fn directory_candidate(
    id: impl Into<String>,
    owner: multi_launcher::hotkey::directory::Owner,
    owner_label: &str,
    action_label: impl Into<String>,
    chord: Option<String>,
    route: multi_launcher::hotkey::directory::RouteKind,
    scope: multi_launcher::hotkey::directory::Scope,
    source_revision: u64,
    expected_runtime_generation: Option<u64>,
    state: multi_launcher::hotkey::directory::CandidateState,
    reason: Option<multi_launcher::hotkey::directory::Reason>,
    edit: Option<multi_launcher::hotkey::directory::EditDestination>,
) -> multi_launcher::hotkey::directory::Candidate {
    use multi_launcher::hotkey::directory::{Candidate, EntryId};
    Candidate {
        id: EntryId::new(id),
        owner,
        owner_label: owner_label.into(),
        action_label: action_label.into(),
        chord,
        route,
        scope,
        source_revision,
        expected_runtime_generation,
        state,
        reason,
        edit,
    }
}

fn candidate_runtime_state(
    chord: &str,
    generation: u64,
    route_pending: bool,
) -> (
    multi_launcher::hotkey::directory::CandidateState,
    Option<multi_launcher::hotkey::directory::Reason>,
) {
    use multi_launcher::hotkey::directory::{CandidateState, Reason};
    if parse_hotkey(chord).is_none() {
        (CandidateState::Invalid, Some(Reason::InvalidChord))
    } else if route_pending {
        (CandidateState::Pending { generation }, None)
    } else {
        (CandidateState::Configured, None)
    }
}

fn exact_legacy_trigger_active(
    active_triggers: &[multi_launcher::hotkey::Hotkey],
    chord: &str,
) -> bool {
    parse_hotkey(chord).is_some_and(|candidate| active_triggers.contains(&candidate))
}

fn legacy_listener_hotkeys(listener: &HotkeyListener) -> Vec<multi_launcher::hotkey::Hotkey> {
    if !listener.is_active() {
        return Vec::new();
    }
    listener
        .triggers()
        .iter()
        .map(|trigger| multi_launcher::hotkey::Hotkey {
            key: trigger._key,
            ctrl: trigger._ctrl,
            shift: trigger._shift,
            alt: trigger._alt,
            alt_gr: trigger._alt_gr,
            win: trigger._win,
        })
        .collect()
}

/// Payload-free evidence retained only after a successful shared-route startup,
/// handoff, or restoration. Service presence alone is deliberately insufficient.
#[derive(Clone, Debug, PartialEq, Eq)]
struct AppliedSharedRouteEvidence {
    runtime_generation: u64,
    radial_revision: u64,
    launcher_hotkey: multi_launcher::hotkey::Hotkey,
    related: Vec<RelatedBinding>,
}

impl AppliedSharedRouteEvidence {
    fn matches(
        &self,
        settings: &Settings,
        radial_document: &RadialDocument,
        sequence_routes: &[PasteSequenceHotkeyRoute],
        runtime_generation: u64,
    ) -> bool {
        settings.radial.enabled
            && settings.radial.shared_tap_hold
            && self.runtime_generation == runtime_generation
            && self.radial_revision == radial_document.revision.0
            && self.launcher_hotkey == settings.hotkey()
            && related_bindings_equal(
                &self.related,
                &related_launcher_bindings_with_sequences(
                    settings,
                    radial_document,
                    true,
                    sequence_routes,
                ),
            )
    }
}

fn related_bindings_equal(left: &[RelatedBinding], right: &[RelatedBinding]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(left, right)| left.hotkey == right.hotkey && left.action == right.action)
}

fn applied_shared_route_after_successful_install(
    settings: &Settings,
    radial_document: &RadialDocument,
    sequence_routes: &[PasteSequenceHotkeyRoute],
    runtime_generation: u64,
) -> Option<AppliedSharedRouteEvidence> {
    if !settings.radial.enabled || !settings.radial.shared_tap_hold {
        return None;
    }
    Some(AppliedSharedRouteEvidence {
        runtime_generation,
        radial_revision: radial_document.revision.0,
        launcher_hotkey: settings.hotkey(),
        related: related_launcher_bindings_with_sequences(
            settings,
            radial_document,
            true,
            sequence_routes,
        ),
    })
}

fn directory_source_snapshot(
    source: multi_launcher::hotkey::directory::Source,
    revision: u64,
    candidates: Vec<multi_launcher::hotkey::directory::Candidate>,
    receipts: Vec<multi_launcher::hotkey::directory::AppliedRouteReceipt>,
    pending_generation: Option<u64>,
) -> multi_launcher::hotkey::directory::SourceSnapshot {
    let snapshot = multi_launcher::hotkey::directory::SourceSnapshot::new(
        source, revision, candidates, receipts,
    );
    match pending_generation {
        Some(generation) => snapshot.with_pending_generation(generation),
        None => snapshot,
    }
}

fn hotkey_directory_source_snapshots(
    settings: &Settings,
    radial_document: &RadialDocument,
    sequence_snapshot: Option<&multi_launcher::paste_sequence::SequenceSnapshot>,
    sequence_routes: &[PasteSequenceHotkeyRoute],
    sequence_presentation: Option<&multi_launcher::paste_sequence::SequenceRoutePresentation>,
    sequence_omissions: &[PasteSequenceRouteOmission],
    macro_document: Option<&multi_launcher::mkmacro::MkMacroDocument>,
    macro_revision: u64,
    active_legacy_triggers: &[multi_launcher::hotkey::Hotkey],
    applied_shared_route: Option<&AppliedSharedRouteEvidence>,
    runtime_generation: u64,
    sequence_runtime_acknowledged: bool,
    route_pending: bool,
    source_publication_revision: u64,
) -> Vec<multi_launcher::hotkey::directory::SourceSnapshot> {
    use multi_launcher::hotkey::directory::{
        AppliedRouteReceipt, CandidateState, EditDestination, MacroRecorderSetting, MainSetting,
        Owner, PasteSetting, Reason, RouteKind, Scope, Source, SourceSnapshot,
    };
    use multi_launcher::hotkey::launcher_invocation::RelatedAction;
    use multi_launcher::paste_sequence::SequenceShortcutPresentation;
    let runtime_route_current = applied_shared_route.is_some_and(|evidence| {
        !route_pending
            && evidence.matches(
                settings,
                radial_document,
                sequence_routes,
                runtime_generation,
            )
    });
    let shared_route_current =
        runtime_route_current && settings.radial.enabled && settings.radial.shared_tap_hold;
    let runtime_related_bindings = if runtime_route_current {
        applied_shared_route.map_or(&[][..], |evidence| evidence.related.as_slice())
    } else {
        &[]
    };
    let shared_related_bindings = if shared_route_current {
        runtime_related_bindings
    } else {
        &[]
    };

    let receipt = |id: &str, chord: &str, source_revision: u64| AppliedRouteReceipt {
        id: multi_launcher::hotkey::directory::EntryId::new(id),
        chord: chord.into(),
        source_revision,
        generation: runtime_generation,
    };

    let mut main_candidates = Vec::new();
    let mut main_receipts = Vec::new();
    for (id, action, configured, setting) in [
        (
            "main.launcher",
            "Toggle launcher",
            Some(settings.hotkey.as_deref().unwrap_or("F2")),
            MainSetting::Launcher,
        ),
        (
            "main.quit",
            "Quit launcher",
            settings.quit_hotkey.as_deref(),
            MainSetting::Quit,
        ),
        (
            "main.help",
            "Open launcher help",
            settings.help_hotkey.as_deref(),
            MainSetting::Help,
        ),
    ] {
        let state = configured.map_or((CandidateState::Disabled, None), |chord| {
            candidate_runtime_state(chord, runtime_generation, route_pending)
        });
        let chord = configured.map(|chord| chord.trim().to_owned());
        let route = if settings.radial.enabled && settings.radial.shared_tap_hold {
            RouteKind::SharedHook
        } else {
            RouteKind::LegacyPolling
        };
        let candidate = directory_candidate(
            id,
            Owner::Launcher,
            "Launcher",
            action,
            chord.clone(),
            route,
            Scope::ProcessWide,
            runtime_generation,
            configured.map(|_| runtime_generation),
            state.0.clone(),
            state.1,
            Some(EditDestination::MainSettings(setting)),
        );
        let related_action = match setting {
            MainSetting::Launcher => None,
            MainSetting::Quit => Some(RelatedAction::Quit),
            MainSetting::Help => Some(RelatedAction::Help),
        };
        let exact_shared = if setting == MainSetting::Launcher {
            shared_route_current
                && chord
                    .as_deref()
                    .and_then(parse_hotkey)
                    .is_some_and(|hotkey| {
                        applied_shared_route
                            .is_some_and(|evidence| evidence.launcher_hotkey == hotkey)
                    })
        } else {
            related_action.as_ref().is_some_and(|action| {
                shared_related_bindings.iter().any(|binding| {
                    Some(binding.hotkey) == chord.as_deref().and_then(parse_hotkey)
                        && &binding.action == action
                })
            })
        };
        if !route_pending
            && let Some(chord) = chord.as_deref()
            && (exact_shared
                || (!settings.radial.enabled || !settings.radial.shared_tap_hold)
                    && exact_legacy_trigger_active(active_legacy_triggers, chord))
        {
            main_receipts.push(receipt(id, chord, runtime_generation));
        }
        main_candidates.push(candidate);
    }
    let main_source = directory_source_snapshot(
        Source::MainSettings,
        source_publication_revision,
        main_candidates,
        main_receipts,
        route_pending.then_some(runtime_generation),
    );

    let mut screen_draw_candidates = Vec::new();
    let mut screen_draw_receipts = Vec::new();
    let screen_draw = match settings
        .plugin_settings
        .get("screen_draw")
        .cloned()
        .map(serde_json::from_value::<ScreenDrawSettings>)
        .transpose()
    {
        Ok(screen_draw) => Some(screen_draw.unwrap_or_default()),
        Err(_) => None,
    };
    let mut add_screen_route = |id: &str,
                                label: &str,
                                configured: Option<String>,
                                action: RelatedAction| {
        let parsed = configured.as_deref().and_then(parse_hotkey);
        let (state, reason) = match configured.as_deref() {
            None => (CandidateState::Disabled, None),
            Some(chord) => candidate_runtime_state(chord, runtime_generation, route_pending),
        };
        let route = if settings.radial.enabled && settings.radial.shared_tap_hold {
            RouteKind::SharedHook
        } else {
            RouteKind::LegacyPolling
        };
        let exact_shared = shared_related_bindings
            .iter()
            .any(|binding| binding.action == action && parsed == Some(binding.hotkey));
        let shared_mode = settings.radial.enabled && settings.radial.shared_tap_hold;
        let exact_legacy = !shared_mode
            && configured
                .as_deref()
                .is_some_and(|chord| exact_legacy_trigger_active(active_legacy_triggers, chord));
        let id_owned = id.to_owned();
        let chord_owned = configured.as_ref().map(|chord| chord.trim().to_owned());
        let expected_generation = configured.as_ref().map(|_| runtime_generation);
        // Screen Draw has no current settings editor. Keep these as truthful
        // inventory rows until a real editing owner exists.
        let edit = None;
        if !route_pending && chord_owned.is_some() && (exact_shared || exact_legacy) {
            screen_draw_receipts.push(receipt(
                &id_owned,
                chord_owned.as_deref().unwrap(),
                runtime_generation,
            ));
        }
        screen_draw_candidates.push(directory_candidate(
            id_owned,
            Owner::ScreenDraw,
            "Screen Draw",
            label,
            chord_owned,
            route,
            Scope::ProcessWide,
            runtime_generation,
            expected_generation,
            state,
            reason.or_else(|| {
                (expected_generation.is_some() && !route_pending && !(exact_shared || exact_legacy))
                    .then_some(Reason::RouteNotAcknowledged)
            }),
            edit,
        ));
    };
    match screen_draw.as_ref() {
        Some(screen_draw) => {
            add_screen_route(
                "screen_draw.launch",
                "Launch Screen Draw",
                screen_draw
                    .launch_hotkey
                    .as_ref()
                    .map(|chord| chord.as_str().to_owned()),
                RelatedAction::ScreenDrawLaunch,
            );
            add_screen_route(
                "screen_draw.emergency",
                "Emergency Screen Draw control",
                Some(screen_draw.emergency_hotkey.as_str().to_owned()),
                RelatedAction::ScreenDrawEmergency,
            );
            for (tool, chord) in &screen_draw.tool_hotkeys {
                let raw = chord.as_str().to_owned();
                let (state, reason) = if !chord.is_valid() {
                    (CandidateState::Invalid, Some(Reason::InvalidChord))
                } else if parse_hotkey(&raw).is_none() {
                    (
                        CandidateState::Unsupported,
                        Some(Reason::UnsupportedByRoute),
                    )
                } else {
                    (
                        CandidateState::Unknown,
                        Some(Reason::LocalRouteHasNoGlobalReceipt),
                    )
                };
                screen_draw_candidates.push(directory_candidate(
                    format!("screen_draw.tool.{tool:?}"),
                    Owner::ScreenDraw,
                    "Screen Draw",
                    format!("{tool:?} tool"),
                    Some(raw),
                    RouteKind::Local,
                    Scope::LocalSession,
                    runtime_generation,
                    None,
                    state,
                    reason,
                    None,
                ));
            }
            for (id, label, chord) in [
                (
                    "screen_draw.thickness.increase",
                    "Increase thickness",
                    screen_draw.increase_thickness_hotkey.as_ref(),
                ),
                (
                    "screen_draw.thickness.decrease",
                    "Decrease thickness",
                    screen_draw.decrease_thickness_hotkey.as_ref(),
                ),
            ] {
                let raw = chord.map(|chord| chord.as_str().to_owned());
                let (state, reason) =
                    raw.as_deref()
                        .map_or((CandidateState::Disabled, None), |raw| {
                            if !chord.is_some_and(|chord| chord.is_valid()) {
                                (CandidateState::Invalid, Some(Reason::InvalidChord))
                            } else if parse_hotkey(raw).is_none() {
                                (
                                    CandidateState::Unsupported,
                                    Some(Reason::UnsupportedByRoute),
                                )
                            } else {
                                (
                                    CandidateState::Unknown,
                                    Some(Reason::LocalRouteHasNoGlobalReceipt),
                                )
                            }
                        });
                screen_draw_candidates.push(directory_candidate(
                    id,
                    Owner::ScreenDraw,
                    "Screen Draw",
                    label,
                    raw,
                    RouteKind::Local,
                    Scope::LocalSession,
                    runtime_generation,
                    None,
                    state,
                    reason,
                    None,
                ));
            }
            for (index, chord) in screen_draw.quick_color_hotkeys.iter().enumerate() {
                let raw = chord.as_ref().map(|chord| chord.as_str().to_owned());
                let (state, reason) =
                    raw.as_deref()
                        .map_or((CandidateState::Disabled, None), |raw| {
                            if !chord.as_ref().is_some_and(|chord| chord.is_valid()) {
                                (CandidateState::Invalid, Some(Reason::InvalidChord))
                            } else if parse_hotkey(raw).is_none() {
                                (
                                    CandidateState::Unsupported,
                                    Some(Reason::UnsupportedByRoute),
                                )
                            } else {
                                (
                                    CandidateState::Unknown,
                                    Some(Reason::LocalRouteHasNoGlobalReceipt),
                                )
                            }
                        });
                screen_draw_candidates.push(directory_candidate(
                    format!("screen_draw.quick_color.{index}"),
                    Owner::ScreenDraw,
                    "Screen Draw",
                    format!("Select quick color {}", index + 1),
                    raw,
                    RouteKind::Local,
                    Scope::LocalSession,
                    runtime_generation,
                    None,
                    state,
                    reason,
                    None,
                ));
            }
        }
        None => {
            for (id, label) in [
                ("screen_draw.launch", "Launch Screen Draw"),
                ("screen_draw.emergency", "Emergency Screen Draw control"),
            ] {
                screen_draw_candidates.push(directory_candidate(
                    id,
                    Owner::ScreenDraw,
                    "Screen Draw",
                    label,
                    None,
                    RouteKind::SharedHook,
                    Scope::ProcessWide,
                    runtime_generation,
                    Some(runtime_generation),
                    CandidateState::Unavailable,
                    Some(Reason::OwnerUnavailable),
                    None,
                ));
            }
        }
    }
    let screen_draw_source = directory_source_snapshot(
        Source::ScreenDraw,
        source_publication_revision,
        screen_draw_candidates,
        screen_draw_receipts,
        route_pending.then_some(runtime_generation),
    );

    let mut paste_candidates = Vec::new();
    let mut paste_receipts = Vec::new();
    let paste_enabled = settings
        .enabled_plugins
        .as_ref()
        .is_none_or(|plugins| plugins.contains("paste_sequence"));
    let paste_route_kind = if settings.radial.enabled && settings.radial.shared_tap_hold {
        RouteKind::SharedHook
    } else {
        RouteKind::LegacyPolling
    };
    for (setting, id, label, configured, presentation, action) in [
        (
            PasteSetting::Next,
            "paste.next",
            "Advance to the next Paste Sequence step",
            settings.paste_sequence.next_hotkey.as_deref(),
            sequence_presentation.map(|presentation| &presentation.next),
            PasteSequenceHotkeyAction::Next,
        ),
        (
            PasteSetting::Cancel,
            "paste.cancel",
            "Cancel Paste Sequence playback",
            settings.paste_sequence.cancel_hotkey.as_deref(),
            sequence_presentation.map(|presentation| &presentation.cancel),
            PasteSequenceHotkeyAction::Cancel,
        ),
    ] {
        let (mut state, mut reason) = match configured {
            None => (CandidateState::Disabled, None),
            Some(_) if !paste_enabled => (CandidateState::Unavailable, Some(Reason::OwnerDisabled)),
            Some(chord) => candidate_runtime_state(chord, runtime_generation, route_pending),
        };
        let mut acknowledged_chord = None;
        if configured.is_some() && paste_enabled && !route_pending {
            match presentation {
                Some(SequenceShortcutPresentation::Active(chord))
                    if configured.is_some_and(|configured| configured.trim() == chord.trim())
                        && sequence_presentation.is_some_and(|presentation| {
                            presentation.generation == runtime_generation
                        })
                        && sequence_runtime_acknowledged
                        && if shared_route_current {
                            runtime_related_bindings.iter().any(|binding| {
                                binding.action == RelatedAction::PasteSequence(action.clone())
                                    && parse_hotkey(chord) == Some(binding.hotkey)
                            })
                        } else {
                            exact_legacy_trigger_active(active_legacy_triggers, chord)
                        } =>
                {
                    state = CandidateState::Configured;
                    reason = None;
                    acknowledged_chord = Some(chord.as_str());
                }
                Some(SequenceShortcutPresentation::Inactive { reason: detail, .. }) => {
                    if sequence_omissions
                        .iter()
                        .any(|omission| omission.action == action)
                    {
                        state = CandidateState::OmittedByPrecedence;
                        reason = Some(Reason::OmittedByPrecedence);
                    } else if detail.to_ascii_lowercase().contains("unsupported") {
                        state = CandidateState::Unsupported;
                        reason = Some(Reason::UnsupportedByRoute);
                    } else if detail.to_ascii_lowercase().contains("disabled") {
                        state = CandidateState::Unavailable;
                        reason = Some(Reason::OwnerDisabled);
                    }
                }
                _ => {}
            }
            if reason.is_none()
                && sequence_runtime_acknowledged
                && !route_pending
                && !sequence_routes.iter().any(|route| route.action == action)
            {
                state = CandidateState::OmittedByPrecedence;
                reason = Some(Reason::OmittedByPrecedence);
            }
        }
        let chord = configured.map(|chord| chord.trim().to_owned());
        let candidate = directory_candidate(
            id,
            Owner::PasteSequences,
            "Paste Sequences",
            label,
            chord.clone(),
            paste_route_kind,
            Scope::ProcessWide,
            runtime_generation,
            configured.map(|_| runtime_generation),
            state,
            reason.or_else(|| {
                (configured.is_some()
                    && acknowledged_chord.is_none()
                    && paste_enabled
                    && !route_pending)
                    .then_some(Reason::RouteNotAcknowledged)
            }),
            Some(EditDestination::PasteSequence {
                sequence_id: None,
                setting,
            }),
        );
        if let (Some(chord), Some(acknowledged_chord)) = (chord.as_deref(), acknowledged_chord)
            && chord == acknowledged_chord
        {
            paste_receipts.push(receipt(id, chord, runtime_generation));
        }
        paste_candidates.push(candidate);
    }
    if let Some(snapshot) = sequence_snapshot {
        for sequence in snapshot.sequences() {
            let id = format!("paste.sequence.{}.start", sequence.id.get());
            let Some(hotkey) = sequence.hotkey.as_ref() else {
                paste_candidates.push(directory_candidate(
                    id,
                    Owner::PasteSequences,
                    "Paste Sequences",
                    sequence.name.clone(),
                    None,
                    paste_route_kind,
                    Scope::ProcessWide,
                    runtime_generation,
                    None,
                    CandidateState::Disabled,
                    None,
                    Some(EditDestination::PasteSequence {
                        sequence_id: Some(sequence.id.get()),
                        setting: PasteSetting::Start,
                    }),
                ));
                continue;
            };
            let chord = hotkey.as_str().trim().to_owned();
            let presentation_chord = sequence_presentation
                .filter(|presentation| presentation.generation == runtime_generation)
                .and_then(|presentation| {
                    presentation
                        .starts
                        .iter()
                        .find(|(active_id, active_chord)| {
                            *active_id == sequence.id && active_chord.trim() == chord
                        })
                        .map(|(_, chord)| chord.as_str())
                });
            let active_chord = presentation_chord.filter(|active_chord| {
                sequence_runtime_acknowledged
                    && if shared_route_current {
                        runtime_related_bindings.iter().any(|binding| {
                            binding.action
                                == RelatedAction::PasteSequence(PasteSequenceHotkeyAction::Start(
                                    sequence.id,
                                ))
                                && parse_hotkey(active_chord) == Some(binding.hotkey)
                        })
                    } else {
                        exact_legacy_trigger_active(active_legacy_triggers, active_chord)
                    }
            });
            let (state, reason) = if !paste_enabled {
                (CandidateState::Unavailable, Some(Reason::OwnerDisabled))
            } else if route_pending {
                (
                    CandidateState::Pending {
                        generation: runtime_generation,
                    },
                    None,
                )
            } else if parse_hotkey(&chord).is_none() {
                (CandidateState::Invalid, Some(Reason::InvalidChord))
            } else {
                (
                    CandidateState::Configured,
                    active_chord
                        .is_none()
                        .then_some(Reason::RouteNotAcknowledged),
                )
            };
            if !route_pending && let Some(active_chord) = active_chord {
                paste_receipts.push(receipt(&id, active_chord, runtime_generation));
            }
            paste_candidates.push(directory_candidate(
                id,
                Owner::PasteSequences,
                "Paste Sequences",
                sequence.name.clone(),
                Some(chord),
                paste_route_kind,
                Scope::ProcessWide,
                runtime_generation,
                Some(runtime_generation),
                state,
                reason,
                Some(EditDestination::PasteSequence {
                    sequence_id: Some(sequence.id.get()),
                    setting: PasteSetting::Start,
                }),
            ));
        }
    } else {
        paste_candidates.push(directory_candidate(
            "paste.sequence.document",
            Owner::PasteSequences,
            "Paste Sequences",
            "Saved sequence start shortcuts unavailable",
            None,
            paste_route_kind,
            Scope::ProcessWide,
            runtime_generation,
            None,
            CandidateState::Unavailable,
            Some(Reason::OwnerUnavailable),
            None,
        ));
    }
    let paste_source = directory_source_snapshot(
        Source::PasteSequences,
        source_publication_revision,
        paste_candidates,
        paste_receipts,
        route_pending.then_some(runtime_generation),
    );

    let related_acknowledged = &runtime_related_bindings;
    let mut radial_candidates = Vec::new();
    let mut radial_receipts = Vec::new();
    for trigger in &radial_document.custom_triggers {
        let id = format!("radial.trigger.{}", trigger.id.as_str());
        let chord = trigger.chord.trim().to_owned();
        let local_scope = trigger.scope == multi_launcher::radial::model::TriggerScope::MenuLocal;
        let (mut state, mut reason) = if !settings.radial.enabled {
            (CandidateState::Disabled, Some(Reason::OwnerDisabled))
        } else if parse_hotkey(&chord).is_none() {
            (CandidateState::Invalid, Some(Reason::InvalidChord))
        } else if route_pending {
            (
                CandidateState::Pending {
                    generation: runtime_generation,
                },
                None,
            )
        } else {
            (
                CandidateState::Configured,
                Some(if local_scope {
                    Reason::LocalRouteHasNoGlobalReceipt
                } else {
                    Reason::RouteNotAcknowledged
                }),
            )
        };
        let exact_related = !local_scope
            && related_acknowledged.iter().any(|binding| {
                binding.action
                    == RelatedAction::DirectMenu {
                        trigger_id: trigger.id.as_str().to_owned(),
                        menu_id: trigger.menu_id.clone(),
                    }
                    && parse_hotkey(&chord) == Some(binding.hotkey)
            });
        if exact_related && !route_pending {
            state = CandidateState::Configured;
            reason = None;
            radial_receipts.push(receipt(&id, &chord, radial_document.revision.0));
        }
        let scope = if local_scope {
            Scope::MenuLocal(trigger.menu_id.as_str().to_owned())
        } else {
            Scope::Global
        };
        radial_candidates.push(directory_candidate(
            id,
            Owner::Radial,
            "Radial",
            "Open radial menu",
            Some(chord),
            RouteKind::Radial,
            scope,
            radial_document.revision.0,
            Some(runtime_generation),
            state,
            reason,
            Some(EditDestination::RadialTrigger {
                trigger_id: trigger.id.as_str().to_owned(),
                menu_id: trigger.menu_id.as_str().to_owned(),
            }),
        ));
    }
    for menu in &radial_document.menus {
        for cell in menu.rings.iter().flat_map(|ring| &ring.cells) {
            for shortcut in &cell.shortcuts {
                let chord = shortcut.chord.trim().to_owned();
                let local_scope =
                    shortcut.scope == multi_launcher::radial::model::TriggerScope::MenuLocal;
                let (state, reason) = if !settings.radial.enabled {
                    (CandidateState::Disabled, Some(Reason::OwnerDisabled))
                } else if shortcut.scope == multi_launcher::radial::model::TriggerScope::Global
                    && !settings.radial.global_item_inputs
                {
                    (CandidateState::Disabled, Some(Reason::OwnerDisabled))
                } else if parse_hotkey(&chord).is_none() {
                    (CandidateState::Invalid, Some(Reason::InvalidChord))
                } else if route_pending {
                    (
                        CandidateState::Pending {
                            generation: runtime_generation,
                        },
                        None,
                    )
                } else {
                    (CandidateState::Unknown, Some(Reason::RouteNotAcknowledged))
                };
                radial_candidates.push(directory_candidate(
                    format!("radial.shortcut.{}", shortcut.id.as_str()),
                    Owner::Radial,
                    "Radial",
                    format!("Shortcut for {}", cell.label),
                    Some(chord),
                    RouteKind::Radial,
                    if local_scope {
                        Scope::MenuLocal(menu.id.as_str().to_owned())
                    } else {
                        Scope::Global
                    },
                    radial_document.revision.0,
                    Some(runtime_generation),
                    state,
                    reason,
                    Some(EditDestination::RadialShortcut {
                        menu_id: menu.id.as_str().to_owned(),
                        cell_id: cell.id.as_str().to_owned(),
                        shortcut_id: shortcut.id.as_str().to_owned(),
                    }),
                ));
            }
        }
    }
    let radial_source = directory_source_snapshot(
        Source::Radial,
        source_publication_revision,
        radial_candidates,
        radial_receipts,
        route_pending.then_some(runtime_generation),
    );

    let mut macro_candidates = Vec::new();
    let macro_receipts = Vec::new();
    let mkmacro_enabled = settings
        .enabled_plugins
        .as_ref()
        .is_none_or(|plugins| plugins.contains("mkmacro"));
    if let Some(document) = macro_document {
        let mut reserved = reserved_launcher_hotkeys_with_sequences(settings, sequence_routes);
        if settings.radial.enabled {
            reserved.extend(radial_global_hotkey_reservations(settings, radial_document));
        }
        let reserved_refs = reserved
            .iter()
            .map(|(name, chord)| (name.as_str(), chord.as_str()))
            .collect::<Vec<_>>();
        let macro_diagnostics =
            multi_launcher::mkmacro::hotkeys::validate_hotkeys(document, &reserved_refs);
        for entry in &document.macros {
            let id = format!("macro.playback.{}", entry.id);
            let chord = entry
                .hotkey
                .as_ref()
                .map(multi_launcher::mkmacro::hotkeys::canonical_hotkey);
            let diagnostics = macro_diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.macro_id == entry.id)
                .collect::<Vec<_>>();
            let invalid_scope = diagnostics.iter().any(|diagnostic| {
                diagnostic
                    .message
                    .starts_with("Invalid active-window hotkey matcher:")
            });
            let has_conflict = !diagnostics.is_empty();
            let (state, reason) = match chord.as_deref() {
                None => (CandidateState::Disabled, None),
                Some(_) if !mkmacro_enabled => {
                    (CandidateState::Unavailable, Some(Reason::OwnerDisabled))
                }
                Some(_) if !entry.enabled => {
                    (CandidateState::Disabled, Some(Reason::OwnerDisabled))
                }
                Some(chord) if parse_hotkey(chord).is_none() => {
                    (CandidateState::Invalid, Some(Reason::InvalidChord))
                }
                Some(_) if invalid_scope => (CandidateState::Invalid, Some(Reason::InvalidScope)),
                Some(_) if has_conflict => (
                    CandidateState::Conflict,
                    Some(Reason::ConflictsWithConfiguredRoute),
                ),
                Some(_) => (CandidateState::Unknown, Some(Reason::RouteNotAcknowledged)),
            };
            let scope = match &entry.hotkey_scope {
                multi_launcher::mkmacro::MkHotkeyScope::AnyWindow => Scope::Global,
                multi_launcher::mkmacro::MkHotkeyScope::ActiveWindow(_) => Scope::ActiveWindow,
            };
            macro_candidates.push(directory_candidate(
                id,
                Owner::MkMacro,
                "MkMacro",
                entry.name.clone(),
                chord,
                RouteKind::MacroPolling,
                scope,
                macro_revision,
                None,
                state,
                reason,
                Some(EditDestination::MacroPlayback { macro_id: entry.id }),
            ));
        }
        for (id, label, hotkey, setting) in [
            (
                "macro.recorder.toggle",
                "Toggle macro recording",
                Some(&document.settings.record_toggle_hotkey),
                MacroRecorderSetting::RecordToggle,
            ),
            (
                "macro.recorder.pause_resume",
                "Pause or resume recording",
                document.settings.recorder.pause_resume_hotkey.as_ref(),
                MacroRecorderSetting::PauseResume,
            ),
            (
                "macro.recorder.marker",
                "Insert recording marker",
                document.settings.recorder.marker_hotkey.as_ref(),
                MacroRecorderSetting::Marker,
            ),
        ] {
            let chord = hotkey.map(multi_launcher::mkmacro::hotkeys::canonical_hotkey);
            let (state, reason) =
                chord
                    .as_deref()
                    .map_or((CandidateState::Disabled, None), |chord| {
                        if !mkmacro_enabled {
                            (CandidateState::Unavailable, Some(Reason::OwnerDisabled))
                        } else if parse_hotkey(chord).is_some() {
                            (CandidateState::Unknown, Some(Reason::RouteNotAcknowledged))
                        } else {
                            (CandidateState::Invalid, Some(Reason::InvalidChord))
                        }
                    });
            macro_candidates.push(directory_candidate(
                id,
                Owner::MkMacro,
                "MkMacro recorder",
                label,
                chord,
                RouteKind::MacroPolling,
                Scope::Global,
                macro_revision,
                None,
                state,
                reason,
                Some(EditDestination::MacroRecorder(setting)),
            ));
        }
    } else {
        macro_candidates.push(directory_candidate(
            "macro.document",
            Owner::MkMacro,
            "MkMacro",
            "Macro shortcuts unavailable",
            None,
            RouteKind::MacroPolling,
            Scope::Global,
            macro_revision,
            None,
            CandidateState::Unavailable,
            Some(Reason::OwnerUnavailable),
            None,
        ));
    }
    let macro_source = SourceSnapshot::new(
        Source::Macros,
        source_publication_revision,
        macro_candidates,
        macro_receipts,
    );

    vec![
        main_source,
        screen_draw_source,
        paste_source,
        radial_source,
        macro_source,
    ]
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct HotkeyDirectoryPublicationKey {
    settings_generation: u64,
    radial_revision: u64,
    paste_document_revision: u64,
    macro_revision: u64,
    pending_route_generation: Option<u64>,
    listener_active: bool,
    shared_service_active: bool,
    shared_route_acknowledged: bool,
}

fn hotkey_directory_publication_key(
    settings_generation: u64,
    radial_document: &RadialDocument,
    paste_document_revision: u64,
    macro_revision: u64,
    pending_route: Option<&PendingLauncherRoute>,
    listener: &HotkeyListener,
    invocation_service: Option<&LauncherInvocationService>,
    applied_shared_route: Option<&AppliedSharedRouteEvidence>,
    settings: &Settings,
    sequence_routes: &[PasteSequenceHotkeyRoute],
) -> HotkeyDirectoryPublicationKey {
    HotkeyDirectoryPublicationKey {
        settings_generation,
        radial_revision: radial_document.revision.0,
        paste_document_revision,
        macro_revision,
        pending_route_generation: pending_route.map(|pending| pending.settings_generation),
        listener_active: listener.is_active(),
        shared_service_active: invocation_service.is_some(),
        shared_route_acknowledged: applied_shared_route.is_some_and(|evidence| {
            invocation_service.is_some()
                && evidence.matches(
                    settings,
                    radial_document,
                    sequence_routes,
                    settings_generation,
                )
        }),
    }
}

fn publish_hotkey_directory_if_changed(
    last_publication: &mut Option<HotkeyDirectoryPublicationKey>,
    source_publication_revision: &mut u64,
    settings: &Settings,
    settings_generation: u64,
    radial_document: &RadialDocument,
    paste_document_revision: u64,
    sequence_routes: &[PasteSequenceHotkeyRoute],
    sequence_omissions: &[PasteSequenceRouteOmission],
    macro_revision: u64,
    listener: &HotkeyListener,
    invocation_service: Option<&LauncherInvocationService>,
    applied_shared_route: Option<&AppliedSharedRouteEvidence>,
    pending_route: Option<&PendingLauncherRoute>,
) {
    let (candidate_settings, candidate_generation, candidate_sequence_routes, candidate_omissions) =
        match pending_route.map(|pending| &pending.completion) {
            Some(PendingLauncherRouteCompletion::Hotkey(application)) => (
                &application.prepared.settings,
                application.candidate_generation,
                application.prepared.sequence_routes.as_slice(),
                application.prepared.sequence_omissions.as_slice(),
            ),
            _ => (
                settings,
                settings_generation,
                sequence_routes,
                sequence_omissions,
            ),
        };
    let publication = hotkey_directory_publication_key(
        candidate_generation,
        radial_document,
        paste_document_revision,
        macro_revision,
        pending_route,
        listener,
        invocation_service,
        applied_shared_route,
        candidate_settings,
        candidate_sequence_routes,
    );
    if last_publication.as_ref() == Some(&publication) {
        return;
    }
    let Some(next_source_publication_revision) = source_publication_revision.checked_add(1) else {
        tracing::error!("hotkey directory source publication identity exhausted");
        return;
    };
    *source_publication_revision = next_source_publication_revision;

    let route_pending = pending_route.is_some();
    let sequence_runtime_acknowledged = !route_pending
        && if candidate_settings.radial.enabled && candidate_settings.radial.shared_tap_hold {
            applied_shared_route.is_some_and(|evidence| {
                invocation_service.is_some()
                    && evidence.matches(
                        candidate_settings,
                        radial_document,
                        candidate_sequence_routes,
                        candidate_generation,
                    )
            })
        } else {
            listener.is_active()
        };
    let active_routes = if sequence_runtime_acknowledged {
        candidate_sequence_routes
    } else {
        &[]
    };
    let sequence_presentation = paste_sequence_route_presentation(
        candidate_settings,
        candidate_generation,
        active_routes,
        candidate_omissions,
        active_physical_paste_policy(candidate_settings, radial_document, active_routes),
    );
    let sequence_snapshot = multi_launcher::paste_sequence::SequenceStore::new(
        multi_launcher::paste_sequence::PASTE_SEQUENCES_FILE,
    )
    .load()
    .ok();
    let macro_document = multi_launcher::mkmacro::runtime::shared_hotkey_document_snapshot()
        .ok()
        .flatten();
    for source in hotkey_directory_source_snapshots(
        candidate_settings,
        radial_document,
        sequence_snapshot.as_ref(),
        candidate_sequence_routes,
        sequence_runtime_acknowledged.then_some(&sequence_presentation),
        candidate_omissions,
        macro_document.as_deref(),
        macro_revision,
        &legacy_listener_hotkeys(listener),
        applied_shared_route,
        candidate_generation,
        sequence_runtime_acknowledged,
        route_pending,
        next_source_publication_revision,
    ) {
        multi_launcher::gui::send_event(multi_launcher::gui::WatchEvent::HotkeyDirectorySource(
            source,
        ));
    }
    multi_launcher::gui::send_event(multi_launcher::gui::WatchEvent::HotkeyDirectorySource(
        multi_launcher::plugins::mouse_gestures::hotkey_directory_source(),
    ));
    *last_publication = Some(publication);
}

fn report_paste_sequence_route_omissions(omissions: &[PasteSequenceRouteOmission]) {
    for omission in omissions {
        let message = format!(
            "Paste Sequence {:?} is inactive: {}",
            omission.action, omission.reason
        );
        tracing::warn!(%message, "optional Paste Sequence shortcut omitted");
        multi_launcher::gui::send_event(
            multi_launcher::gui::WatchEvent::PasteSequenceRuntimeDiagnostic(message),
        );
    }
}

fn paste_sequence_hotkey_routes_for_candidate(
    settings: &Settings,
    sequence_bindings: Option<&[multi_launcher::paste_sequence::SequenceHotkeyBinding]>,
) -> Result<Vec<PasteSequenceHotkeyRoute>, String> {
    if let Some(bindings) = sequence_bindings {
        paste_sequence_hotkey_routes_from_bindings(settings, bindings)
    } else {
        paste_sequence_hotkey_routes(settings)
    }
}

fn validate_paste_sequence_hotkey_routes(
    settings: &Settings,
    radial_document: &RadialDocument,
    macro_document: Option<&multi_launcher::mkmacro::MkMacroDocument>,
    routes: &multi_launcher::hotkey::validation::ValidatedHotkeyRoutes,
    sequence_routes: &[PasteSequenceHotkeyRoute],
) -> Result<(), multi_launcher::hotkey::reconfiguration::HotkeyReconfigurationError> {
    use multi_launcher::hotkey::reconfiguration::HotkeyReconfigurationError as Error;

    let mut issues = Vec::new();
    let mut protected = vec![("launcher".to_owned(), routes.launcher)];
    protected.extend(routes.quit.map(|hotkey| ("quit".to_owned(), hotkey)));
    protected.extend(routes.help.map(|hotkey| ("help".to_owned(), hotkey)));
    protected.extend(
        routes
            .screen_draw_launch
            .map(|hotkey| ("Screen Draw launch".to_owned(), hotkey)),
    );
    protected.extend(
        routes
            .screen_draw_emergency
            .map(|hotkey| ("Screen Draw emergency".to_owned(), hotkey)),
    );
    for (index, route) in sequence_routes.iter().enumerate() {
        for (name, hotkey) in &protected {
            if hotkeys_can_cofire(route.hotkey, *hotkey) {
                issues.push(format!("Paste Sequence shortcut can co-fire with {name}"));
            }
        }
        for other in sequence_routes.iter().take(index) {
            if hotkeys_can_cofire(route.hotkey, other.hotkey) {
                issues.push("Paste Sequence shortcuts can co-fire with one another".into());
            }
        }
        if settings.radial.enabled {
            for (name, chord) in radial_global_hotkey_reservations(settings, radial_document) {
                if parse_hotkey(&chord)
                    .is_some_and(|hotkey| hotkeys_can_cofire(route.hotkey, hotkey))
                {
                    issues.push(format!("Paste Sequence shortcut can co-fire with {name}"));
                }
            }
        }
    }

    if let Some(document) = macro_document {
        let reserved = sequence_routes
            .iter()
            .map(|route| ("Paste Sequence shortcut", route.chord.as_str()))
            .collect::<Vec<_>>();
        for diagnostic in multi_launcher::mkmacro::hotkeys::validate_hotkeys(document, &reserved) {
            if diagnostic.severity
                == multi_launcher::mkmacro::hotkeys::HotkeyDiagnosticSeverity::Error
                && diagnostic.message.starts_with("hotkey conflicts with ")
            {
                issues.push(format!(
                    "macro {} {}",
                    diagnostic.macro_id, diagnostic.message
                ));
            }
        }
    }

    if issues.is_empty() {
        Ok(())
    } else {
        Err(Error::Validation(issues.join("; ")))
    }
}

fn prepare_legacy_hotkey_fallback(
    old_settings: &Settings,
    radial_document: &RadialDocument,
    sequence_routes: &[PasteSequenceHotkeyRoute],
    event_tx: &Sender<()>,
) -> Result<PreparedLegacyHotkeyFallback, String> {
    let mut settings = old_settings.clone();
    settings.radial.enabled = false;
    let routes = validate_hotkey_route_candidate(&settings, radial_document)
        .map_err(|error| error.to_string())?;
    let launcher = Arc::new(HotkeyTrigger::new(routes.launcher));
    let quit = routes
        .quit
        .map(|hotkey| Arc::new(HotkeyTrigger::new(hotkey)));
    let help = routes
        .help
        .map(|hotkey| Arc::new(HotkeyTrigger::new(hotkey)));
    let screen_draw = routes
        .screen_draw_launch
        .map(|hotkey| Arc::new(HotkeyTrigger::new(hotkey)));
    let emergency = routes
        .screen_draw_emergency
        .map(|hotkey| Arc::new(HotkeyTrigger::new(hotkey)));
    let sequence_triggers = sequence_hotkey_triggers_from_routes(sequence_routes);
    let listener = HotkeyTrigger::prepare_listener(
        legacy_listener_triggers_with_sequences(
            false,
            &launcher,
            quit.as_ref(),
            help.as_ref(),
            screen_draw.as_ref(),
            emergency.as_ref(),
            &sequence_triggers,
        ),
        "hotkey-owner-failure-fallback",
        event_tx.clone(),
    )?;
    Ok(PreparedLegacyHotkeyFallback {
        settings,
        listener,
        launcher,
        quit,
        help,
        screen_draw,
        emergency,
        sequence_routes: sequence_routes.to_vec(),
        sequence_triggers,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HotkeyApplyPhase {
    DrainOldRoute,
    ActivateCandidate,
    RestoreOldRoute,
}

struct PendingHotkeyApplication {
    request: Option<multi_launcher::hotkey::reconfiguration::HotkeyApplicationRequest>,
    prepared: PreparedHotkeyRoute,
    old_settings: Settings,
    old_generation: u64,
    candidate_generation: u64,
    old_service_existed: bool,
    old_sequence_routes: Vec<PasteSequenceHotkeyRoute>,
    old_listener_stopped: bool,
    fallback_listener: Option<HotkeyListener>,
    phase: HotkeyApplyPhase,
    cancellation_requested: bool,
    failure: Option<multi_launcher::hotkey::reconfiguration::HotkeyReconfigurationError>,
    restore_generation: Option<u64>,
}

struct RadialRoutePlan {
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

#[cfg(test)]
fn radial_route_plan(
    settings: &Settings,
    document: &RadialDocument,
    generation: u64,
) -> RadialRoutePlan {
    let sequences = paste_sequence_hotkey_routes(settings).unwrap_or_default();
    radial_route_plan_with_sequences(settings, document, generation, &sequences)
}

fn radial_route_plan_with_sequences(
    settings: &Settings,
    document: &RadialDocument,
    generation: u64,
    sequence_routes: &[PasteSequenceHotkeyRoute],
) -> RadialRoutePlan {
    let shared_invocation = settings.radial.enabled && settings.radial.shared_tap_hold;
    RadialRoutePlan {
        config: radial_invocation_config(
            settings,
            document,
            shared_invocation,
            generation,
            sequence_routes,
        ),
        related: if settings.radial.enabled {
            related_launcher_bindings_with_sequences(
                settings,
                document,
                shared_invocation,
                sequence_routes,
            )
        } else {
            Vec::new()
        },
        start_legacy: !shared_invocation,
        stop_service: !invocation_service_needed(settings),
    }
}

fn paste_sequence_enabled(settings: &Settings) -> bool {
    settings
        .enabled_plugins
        .as_ref()
        .is_none_or(|plugins| plugins.contains("paste_sequence"))
}

fn invocation_service_needed(settings: &Settings) -> bool {
    settings.radial.enabled || paste_sequence_enabled(settings)
}

fn drain_physical_paste_notices(route_transition: bool) {
    use multi_launcher::paste_sequence::admission::{PhysicalPasteNotice, physical_paste_bridge};
    let bridge = physical_paste_bridge();
    while let Some(notice) = bridge.take_notice() {
        let (owner, generation) = match &notice {
            PhysicalPasteNotice::Paste(admission) => {
                if route_transition || !bridge.valid(admission) {
                    if route_transition {
                        bridge.delivery_failed_for(admission.owner, admission.generation);
                    }
                    continue;
                }
                (admission.owner, admission.generation)
            }
            PhysicalPasteNotice::Busy { owner, generation }
            | PhysicalPasteNotice::Cancel { owner, generation }
            | PhysicalPasteNotice::Failed {
                owner, generation, ..
            } => (*owner, *generation),
        };
        if multi_launcher::gui::send_physical_paste_event(owner, notice).is_err() {
            bridge.delivery_failed_for(owner, generation);
            tracing::warn!(
                owner,
                generation,
                "session input event owner is unavailable"
            );
        }
    }
    if let Some((owner, generation, reason)) = bridge.take_failure() {
        tracing::warn!(owner, reason, "physical paste admission disabled");
        let _ = multi_launcher::gui::send_physical_paste_event(
            owner,
            PhysicalPasteNotice::Failed {
                owner,
                generation,
                reason,
            },
        );
    }
}

fn inert_invocation_config(
    settings: &Settings,
    document: &RadialDocument,
    generation: u64,
) -> InvocationConfig {
    let mut config = radial_invocation_config(settings, document, false, generation, &[]);
    config.launcher_enabled = false;
    config.item_inputs.clear();
    config
}

fn invocation_priority_owner(
    recovery_bridge: Arc<ScreenDrawRecoveryBridge>,
) -> Arc<dyn Fn() -> PriorityOwner + Send + Sync> {
    Arc::new(move || {
        if let Some(lifetime) = recovery_bridge.active_lifetime() {
            PriorityOwner::ScreenDrawRecovery(lifetime)
        } else if exclusive_owners() != 0 {
            PriorityOwner::ExclusiveTool
        } else {
            PriorityOwner::Launcher
        }
    })
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
    sequence_triggers: &[PasteSequenceHotkeyTrigger],
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
        legacy_listener_triggers_with_sequences(
            false,
            launcher,
            quit,
            help,
            screen_draw,
            emergency,
            sequence_triggers,
        ),
        "main",
        event_tx.clone(),
    );
}

#[derive(Debug, Default, PartialEq, Eq)]
struct ScreenDrawTriggerActions {
    launch: bool,
    recover: Vec<ScreenDrawRecoveryIntent>,
    emergency: Option<ScreenDrawRecoveryIntent>,
}

fn take_screen_draw_trigger_actions(
    launcher: &HotkeyTrigger,
    launch: Option<&HotkeyTrigger>,
    emergency: Option<&HotkeyTrigger>,
    recovery_bridge: &ScreenDrawRecoveryBridge,
    notices: &mut [ServiceNotice],
) -> ScreenDrawTriggerActions {
    // A hook notice is already a one-shot admission. Do not collapse several
    // accepted cycles into the legacy trigger's single boolean slot.
    let mut recover = notices
        .iter_mut()
        .filter_map(|notice| notice.take_screen_draw_recovery(recovery_bridge))
        .collect::<Vec<_>>();
    let emergency_fired = emergency.is_some_and(HotkeyTrigger::take);
    if emergency_fired && recovery_bridge.is_active() {
        if let Some(launch) = launch {
            let _ = launch.take();
        }
        let _ = launcher.take();
        return ScreenDrawTriggerActions {
            recover,
            emergency: recovery_bridge.admit(ScreenDrawRecoveryKind::Emergency, None),
            ..Default::default()
        };
    }

    let launch = launch.is_some_and(|trigger| recovery_bridge.stage_start_if_triggered(trigger));
    if take_screen_draw_recovery_trigger(launcher, recovery_bridge.is_active())
        && recover.is_empty()
        && let Some(intent) = recovery_bridge.admit(ScreenDrawRecoveryKind::LauncherToggle, None)
    {
        recover.push(intent);
    }
    ScreenDrawTriggerActions {
        launch,
        recover,
        emergency: None,
    }
}

fn take_screen_draw_recovery_trigger(trigger: &HotkeyTrigger, screen_draw_active: bool) -> bool {
    screen_draw_active && trigger.take()
}

fn dispatch_screen_draw_trigger_actions(
    actions: ScreenDrawTriggerActions,
    bridge: &ScreenDrawRecoveryBridge,
    mut emit: impl FnMut(multi_launcher::gui::WatchEvent),
) -> bool {
    let handled = actions.emergency.is_some() || actions.launch || !actions.recover.is_empty();
    if let Some(intent) = actions.emergency {
        if let Err(error) = bridge.emergency_pause(intent) {
            tracing::error!(%error, "failed to deliver Screen Draw emergency pause");
        }
        emit(multi_launcher::gui::WatchEvent::ScreenDrawEmergency(intent));
    }
    if actions.launch {
        emit(multi_launcher::gui::WatchEvent::ScreenDrawStart);
    }
    for intent in actions.recover {
        if intent.kind == ScreenDrawRecoveryKind::Emergency {
            if let Err(error) = bridge.emergency_pause(intent) {
                tracing::error!(%error, "failed to deliver Screen Draw emergency pause");
            }
            emit(multi_launcher::gui::WatchEvent::ScreenDrawEmergency(intent));
        } else {
            emit(multi_launcher::gui::WatchEvent::ScreenDrawRecover(intent));
        }
    }
    handled
}

fn reconcile_screen_draw_cycle_notices(
    handled: bool,
    exclusive: bool,
    notices: &mut Vec<ServiceNotice>,
) {
    if handled {
        // The owned Screen Draw cycle suppresses its co-fired launcher notices.
        notices.clear();
    }
    if exclusive {
        reject_radial_opens_while_exclusive(notices);
    }
}

fn prepare_hotkey_runtime_message(
    request: multi_launcher::hotkey::reconfiguration::HotkeyPreparationRequest,
    radial_document: &RadialDocument,
    settings_generation: u64,
    event_tx: &Sender<()>,
    has_invocation_service: bool,
    owner: Arc<dyn Fn() -> PriorityOwner + Send + Sync>,
    prepared_routes: &mut HashMap<
        multi_launcher::hotkey::reconfiguration::HotkeyReconfigurationId,
        PreparedHotkeyRoute,
    >,
) {
    use multi_launcher::hotkey::reconfiguration::{
        HotkeyReconfigurationError, HotkeyRuntimeGeneration, PreparedHotkeyReconfiguration,
    };

    let request_id = request.request_id();
    let preparation = (|| {
        let settings = request.settings().clone();
        let sequence_bindings = request
            .sequence_hotkey_bindings()
            .map(|bindings| bindings.to_vec());
        let resolved = resolve_hotkey_route_candidate(
            &settings,
            radial_document,
            sequence_bindings.as_deref(),
        )?;
        let routes = resolved.base;
        let sequence_routes = resolved.sequences;
        let sequence_triggers = sequence_hotkey_triggers_from_routes(&sequence_routes);
        let dependencies =
            HotkeyRouteDependencies::from_routes(&settings, &routes, &sequence_routes);
        let generation =
            HotkeyRuntimeGeneration(settings_generation.checked_add(1).ok_or_else(|| {
                HotkeyReconfigurationError::RuntimeTransition(
                    "hotkey settings generation space exhausted".into(),
                )
            })?);
        let launcher = Arc::new(HotkeyTrigger::new(routes.launcher));
        let quit = routes
            .quit
            .map(|hotkey| Arc::new(HotkeyTrigger::new(hotkey)));
        let help = routes
            .help
            .map(|hotkey| Arc::new(HotkeyTrigger::new(hotkey)));
        let screen_draw = routes
            .screen_draw_launch
            .map(|hotkey| Arc::new(HotkeyTrigger::new(hotkey)));
        let emergency = routes
            .screen_draw_emergency
            .map(|hotkey| Arc::new(HotkeyTrigger::new(hotkey)));
        let triggers = legacy_listener_triggers_with_sequences(
            routes.shared_invocation,
            &launcher,
            quit.as_ref(),
            help.as_ref(),
            screen_draw.as_ref(),
            emergency.as_ref(),
            &sequence_triggers,
        );
        let listener = HotkeyTrigger::prepare_listener(triggers, "prepared", event_tx.clone())
            .map_err(HotkeyReconfigurationError::ListenerPreparation)?;
        let staged_service = if invocation_service_needed(&settings) && !has_invocation_service {
            LauncherInvocationService::start(
                inert_invocation_config(&settings, radial_document, generation.0),
                Vec::new(),
                owner,
                event_tx.clone(),
            )
            .map(Some)
            .map_err(HotkeyReconfigurationError::RuntimeTransition)?
        } else {
            None
        };
        let prepared = PreparedHotkeyReconfiguration {
            request_id,
            generation,
            shared_invocation: routes.shared_invocation,
        };
        Ok((
            prepared,
            PreparedHotkeyRoute {
                settings,
                dependencies,
                generation,
                listener: Some(listener),
                launcher,
                quit,
                help,
                screen_draw,
                emergency,
                sequence_routes,
                physical_paste: resolved.physical_paste,
                sequence_omissions: resolved.omissions,
                sequence_triggers,
                staged_service,
            },
        ))
    })();

    match preparation {
        Ok((prepared, route)) => {
            prepared_routes.insert(request_id, route);
            if let Err(error) = request.complete(Ok(prepared)) {
                prepared_routes.remove(&request_id);
                tracing::debug!(%error, "hotkey preparation requester disconnected before acknowledgement");
            }
        }
        Err(error) => {
            let _ = request.complete(Err(error));
        }
    }
}

fn application_error(
    request: multi_launcher::hotkey::reconfiguration::HotkeyApplicationRequest,
    error: multi_launcher::hotkey::reconfiguration::HotkeyReconfigurationError,
) {
    let _ = request.complete_restored(error);
}

fn install_candidate_hotkey_listener(
    application: &mut PendingHotkeyApplication,
    listener: &mut HotkeyListener,
) {
    if let Some(candidate) = application.prepared.listener.take() {
        listener.stop();
        application.old_listener_stopped = true;
        candidate.activate();
        *listener = candidate;
    }
}

fn restore_fallback_hotkey_listener(
    application: &mut PendingHotkeyApplication,
    listener: &mut HotkeyListener,
) {
    if application.old_listener_stopped {
        listener.stop();
        if let Some(previous) = application.fallback_listener.take() {
            previous.activate();
            *listener = previous;
        }
        application.old_listener_stopped = false;
    }
}

struct HotkeyApplicationStart<'a> {
    request: multi_launcher::hotkey::reconfiguration::HotkeyApplicationRequest,
    prepared_routes: &'a mut HashMap<
        multi_launcher::hotkey::reconfiguration::HotkeyReconfigurationId,
        PreparedHotkeyRoute,
    >,
    settings: &'a mut Settings,
    settings_generation: &'a mut u64,
    sequence_routes: &'a mut Vec<PasteSequenceHotkeyRoute>,
    radial_document: &'a RadialDocument,
    event_tx: &'a Sender<()>,
    listener: &'a mut HotkeyListener,
    invocation_service: &'a mut Option<LauncherInvocationService>,
    pending_launcher_route: &'a mut Option<PendingLauncherRoute>,
}

fn begin_hotkey_application(start: HotkeyApplicationStart<'_>) {
    begin_hotkey_application_with_issuer(start, &mut NativeHotkeyRouteHandoffIssuer);
}

fn begin_hotkey_application_with_issuer(
    start: HotkeyApplicationStart<'_>,
    handoff_issuer: &mut impl HotkeyRouteHandoffIssuer,
) {
    let HotkeyApplicationStart {
        request,
        prepared_routes,
        settings,
        settings_generation,
        sequence_routes,
        radial_document,
        event_tx,
        listener,
        invocation_service,
        pending_launcher_route,
    } = start;
    let current_settings_generation = *settings_generation;
    use multi_launcher::hotkey::reconfiguration::{
        HotkeyReconfigurationError, HotkeyRuntimeGeneration,
    };

    let request_id = request.request_id();
    let Some(mut prepared) = prepared_routes.remove(&request_id) else {
        application_error(request, HotkeyReconfigurationError::InvalidPhase);
        return;
    };
    let Some(expected_generation) = current_settings_generation.checked_add(1) else {
        application_error(
            request,
            HotkeyReconfigurationError::RuntimeTransition(
                "hotkey settings generation space exhausted".into(),
            ),
        );
        return;
    };
    let candidate = request.settings().clone();
    let candidate_sequence_bindings = request
        .sequence_hotkey_bindings()
        .map(|bindings| bindings.to_vec());
    let resolved = match resolve_hotkey_route_candidate(
        &candidate,
        radial_document,
        candidate_sequence_bindings.as_deref(),
    ) {
        Ok(routes) => routes,
        Err(error) => {
            application_error(request, error);
            return;
        }
    };
    let candidate_routes = resolved.base;
    let candidate_sequence_routes = resolved.sequences;
    let candidate_dependencies = HotkeyRouteDependencies::from_routes(
        &candidate,
        &candidate_routes,
        &candidate_sequence_routes,
    );
    if prepared.generation != HotkeyRuntimeGeneration(expected_generation)
        || request.generation() != prepared.generation
        || prepared.dependencies != candidate_dependencies
    {
        application_error(request, HotkeyReconfigurationError::StaleGeneration);
        return;
    }
    // Apply may contain unrelated settings merged after preparation. Preserve
    // those latest values while reusing only resources whose route dependencies
    // still match and whose candidate remains valid against current inputs.
    prepared.settings = candidate;
    prepared.sequence_routes = candidate_sequence_routes;
    prepared.physical_paste = resolved.physical_paste;
    prepared.sequence_omissions = resolved.omissions;
    if pending_launcher_route.is_some() {
        application_error(
            request,
            HotkeyReconfigurationError::RuntimeTransition(
                "another launcher route transition is still pending".into(),
            ),
        );
        return;
    }
    if invocation_service_needed(&prepared.settings)
        && !handoff_issuer.service_available(
            invocation_service
                .as_ref()
                .or(prepared.staged_service.as_ref()),
        )
    {
        application_error(
            request,
            HotkeyReconfigurationError::RuntimeTransition(
                "prepared shared invocation service is unavailable".into(),
            ),
        );
        return;
    }

    let presentation_only = keep_dialog_visibility_only_changed(settings, &prepared.settings)
        && active_hotkey_route_dependencies(settings, radial_document, sequence_routes)
            .is_some_and(|active| active == candidate_dependencies);
    if presentation_only {
        if let Some(mut listener) = prepared.listener.take() {
            listener.stop();
        }
        if let Some(mut service) = prepared.staged_service.take() {
            service.stop();
        }

        *settings = prepared.settings.clone();
        let physical_paste = PhysicalPastePolicy {
            revision: physical_paste_bridge().route_revision(),
            availability: physical_paste_bridge().route_availability(),
        };
        let presentation = paste_sequence_route_presentation(
            settings,
            current_settings_generation,
            sequence_routes,
            &prepared.sequence_omissions,
            physical_paste,
        );
        multi_launcher::gui::send_event(multi_launcher::gui::WatchEvent::PasteSequenceRoutes(
            presentation,
        ));
        let request_id = request.request_id();
        let _ = request.complete_applied(
            multi_launcher::hotkey::reconfiguration::AppliedHotkeyReconfiguration {
                request_id,
                generation: HotkeyRuntimeGeneration(current_settings_generation),
            },
        );
        return;
    }

    let fallback_listener = match HotkeyTrigger::prepare_listener(
        listener.triggers().to_vec(),
        "hotkey-rollback",
        event_tx.clone(),
    ) {
        Ok(listener) => listener,
        Err(error) => {
            application_error(
                request,
                HotkeyReconfigurationError::ListenerPreparation(error),
            );
            return;
        }
    };
    physical_paste_bridge().suspend_route_policy();
    let old_service_existed = invocation_service.is_some();
    let mut application = PendingHotkeyApplication {
        request: Some(request),
        prepared,
        old_settings: settings.clone(),
        old_generation: current_settings_generation,
        candidate_generation: expected_generation,
        old_service_existed,
        old_sequence_routes: sequence_routes.clone(),
        old_listener_stopped: false,
        fallback_listener: Some(fallback_listener),
        phase: if old_service_existed {
            HotkeyApplyPhase::DrainOldRoute
        } else {
            HotkeyApplyPhase::ActivateCandidate
        },
        cancellation_requested: false,
        failure: None,
        restore_generation: None,
    };

    let handoff = if old_service_existed {
        handoff_issuer.begin(
            invocation_service.as_ref(),
            inert_invocation_config(
                &application.old_settings,
                radial_document,
                application.candidate_generation,
            ),
            shared_drain_control_bindings(
                &application.old_settings,
                radial_document,
                &application.old_sequence_routes,
            ),
            LifecycleCancellation::SettingsReload,
        )
    } else if invocation_service_needed(&application.prepared.settings) {
        install_candidate_hotkey_listener(&mut application, listener);
        let plan = radial_route_plan_with_sequences(
            &application.prepared.settings,
            radial_document,
            application.candidate_generation,
            &application.prepared.sequence_routes,
        );
        handoff_issuer.begin(
            application.prepared.staged_service.as_ref(),
            plan.config,
            plan.related,
            LifecycleCancellation::SettingsReload,
        )
    } else {
        // No native hook participates in this route. Resolve the listener swap
        // through the same owner state machine so cancellation can still win
        // before terminal Applied is published.
        *pending_launcher_route = Some(PendingLauncherRoute {
            settings_generation: current_settings_generation,
            handoff: None,
            policy_applied: None,
            completion: PendingLauncherRouteCompletion::Hotkey(Box::new(application)),
        });
        return;
    };

    match handoff {
        Ok(handoff) => {
            *pending_launcher_route = Some(PendingLauncherRoute {
                settings_generation: current_settings_generation,
                handoff: Some(handoff),
                policy_applied: None,
                completion: PendingLauncherRouteCompletion::Hotkey(Box::new(application)),
            });
        }
        Err(error) => {
            application.failure = Some(HotkeyReconfigurationError::RuntimeTransition(error));
            if application.old_service_existed {
                // The route is still expected to be old; no candidate listener
                // has been admitted while the inert handoff was rejected.
                finish_hotkey_application_error(application, listener, invocation_service, false);
            } else {
                restore_fallback_hotkey_listener(&mut application, listener);
                if let Some(mut staged) = application.prepared.staged_service.take() {
                    staged.stop();
                }
                finish_hotkey_application_error(application, listener, invocation_service, false);
            }
        }
    }
}

fn finish_hotkey_application_error(
    mut application: PendingHotkeyApplication,
    listener: &mut HotkeyListener,
    invocation_service: &mut Option<LauncherInvocationService>,
    stop_authoritative_service: bool,
) {
    if !application.old_service_existed
        && let Some(mut staged) = application.prepared.staged_service.take()
    {
        staged.stop();
    }
    if stop_authoritative_service && let Some(mut service) = invocation_service.take() {
        service.stop();
    }
    if application.old_listener_stopped {
        restore_fallback_hotkey_listener(&mut application, listener);
    }
    if let Some(request) = application.request.take() {
        let error = application.failure.take().unwrap_or_else(|| {
            multi_launcher::hotkey::reconfiguration::HotkeyReconfigurationError::RuntimeTransition(
                "hotkey route transition ended without a terminal state".into(),
            )
        });
        if matches!(
            &error,
            multi_launcher::hotkey::reconfiguration::HotkeyReconfigurationError::Rollback(_)
        ) {
            let _ = request.complete_restoration_unavailable(error);
        } else {
            let _ = request.complete_restored(error);
        }
    }
}

fn begin_hotkey_route_rollback(
    mut application: PendingHotkeyApplication,
    listener: &mut HotkeyListener,
    invocation_service: &mut Option<LauncherInvocationService>,
    radial_document: &RadialDocument,
    pending_launcher_route: &mut Option<PendingLauncherRoute>,
    handoff_issuer: &mut dyn HotkeyRouteHandoffIssuer,
) {
    use multi_launcher::hotkey::reconfiguration::HotkeyReconfigurationError;

    if !application.old_service_existed && application.prepared.staged_service.is_none() {
        restore_fallback_hotkey_listener(&mut application, listener);
        finish_hotkey_application_error(application, listener, invocation_service, false);
        return;
    }

    let service = if application.old_service_existed {
        invocation_service.as_ref()
    } else {
        application.prepared.staged_service.as_ref()
    };
    let Some(restore_generation) = application.candidate_generation.checked_add(1) else {
        application.failure = Some(HotkeyReconfigurationError::Rollback(
            "hotkey settings generation space exhausted during route restoration".into(),
        ));
        finish_hotkey_application_error(application, listener, invocation_service, true);
        return;
    };
    let (restore_config, restore_related) = if application.old_service_existed {
        let old_plan = radial_route_plan_with_sequences(
            &application.old_settings,
            radial_document,
            restore_generation,
            &application.old_sequence_routes,
        );
        (old_plan.config, old_plan.related)
    } else {
        (
            inert_invocation_config(
                &application.old_settings,
                radial_document,
                restore_generation,
            ),
            shared_drain_control_bindings(
                &application.old_settings,
                radial_document,
                &application.old_sequence_routes,
            ),
        )
    };
    match handoff_issuer.begin(
        service,
        restore_config,
        restore_related,
        LifecycleCancellation::SettingsReload,
    ) {
        Ok(handoff) => {
            if application.old_listener_stopped {
                listener.stop();
            }
            application.phase = HotkeyApplyPhase::RestoreOldRoute;
            application.restore_generation = Some(restore_generation);
            *pending_launcher_route = Some(PendingLauncherRoute {
                settings_generation: application.old_generation,
                handoff: Some(handoff),
                policy_applied: None,
                completion: PendingLauncherRouteCompletion::Hotkey(Box::new(application)),
            });
        }
        Err(restore_error) => {
            application.failure = Some(HotkeyReconfigurationError::Rollback(format!(
                "previous route could not be restored after transition failure: {restore_error}"
            )));
            finish_hotkey_application_error(application, listener, invocation_service, true);
        }
    }
}

fn finish_hotkey_candidate_activation(
    mut application: PendingHotkeyApplication,
    commit_context: &mut HotkeyRuntimeCommitContext<'_>,
    pending_launcher_route: &mut Option<PendingLauncherRoute>,
    handoff_issuer: &mut dyn HotkeyRouteHandoffIssuer,
) {
    if application.cancellation_requested {
        application.failure = Some(
            multi_launcher::hotkey::reconfiguration::HotkeyReconfigurationError::
                TransactionCancelled,
        );
        begin_hotkey_route_rollback(
            application,
            commit_context.listener,
            commit_context.invocation_service,
            commit_context.radial_document,
            pending_launcher_route,
            handoff_issuer,
        );
    } else {
        commit_context.commit(application);
    }
}

struct HotkeyRuntimeCommitContext<'a> {
    settings: &'a mut Settings,
    settings_generation: &'a mut u64,
    applied_shared_route: &'a mut Option<AppliedSharedRouteEvidence>,
    trigger: &'a mut Arc<HotkeyTrigger>,
    quit_trigger: &'a mut Option<Arc<HotkeyTrigger>>,
    help_trigger: &'a mut Option<Arc<HotkeyTrigger>>,
    screen_draw_trigger: &'a mut Option<Arc<HotkeyTrigger>>,
    emergency_trigger: &'a mut Option<Arc<HotkeyTrigger>>,
    listener: &'a mut HotkeyListener,
    invocation_service: &'a mut Option<LauncherInvocationService>,
    radial_control_endpoint: &'a RadialControlMainEndpoint,
    radial_resources: &'a mut RadialRuntimeResources,
    radial_controller: &'a mut RadialController,
    native_preview:
        &'a mut multi_launcher::radial::authoring::native_preview::NativePreviewCoordinator,
    radial_document: &'a RadialDocument,
    authoring_reply_tx: &'a multi_launcher::radial::authoring::AuthoringReplySender,
    event_tx: &'a Sender<()>,
    sequence_routes: &'a mut Vec<PasteSequenceHotkeyRoute>,
    sequence_triggers: &'a mut Vec<PasteSequenceHotkeyTrigger>,
}

impl HotkeyRuntimeCommitContext<'_> {
    fn finish_after_authoritative_hook_failure(
        &mut self,
        mut application: PendingHotkeyApplication,
    ) {
        use multi_launcher::hotkey::reconfiguration::HotkeyReconfigurationError;

        *self.applied_shared_route = None;
        let fallback = prepare_legacy_hotkey_fallback(
            &application.old_settings,
            self.radial_document,
            &application.old_sequence_routes,
            self.event_tx,
        );
        *self.settings = application.old_settings.clone();
        self.settings.radial.enabled = false;
        *self.settings_generation = self.settings_generation.checked_add(1).unwrap_or(1);
        self.radial_control_endpoint.set_enabled(false);
        self.radial_controller.disable();
        self.radial_resources.set_runtime_enabled(false);
        self.radial_resources.reconcile(self.radial_controller);

        match fallback {
            Ok(fallback) => {
                *self.settings = fallback.settings;
                *self.trigger = fallback.launcher;
                *self.quit_trigger = fallback.quit;
                *self.help_trigger = fallback.help;
                *self.screen_draw_trigger = fallback.screen_draw;
                *self.emergency_trigger = fallback.emergency;
                *self.sequence_routes = fallback.sequence_routes;
                *self.sequence_triggers = fallback.sequence_triggers;
                self.listener.stop();
                fallback.listener.activate();
                *self.listener = fallback.listener;
                application.old_listener_stopped = false;
                application.fallback_listener.take();
                application.failure = Some(HotkeyReconfigurationError::RuntimeTransition(
                    "the shared input hook failed; radial runtime was disabled and the prior legacy launcher and process-control routes were restored".into(),
                ));
                multi_launcher::gui::send_event(
                    multi_launcher::gui::WatchEvent::RadialRuntimeDiagnostic(
                        "The shared launcher input hook stopped unexpectedly. Radial menus were disabled and the prior legacy launcher and process-control routes were restored.".into(),
                    ),
                );
            }
            Err(error) => {
                self.listener.stop();
                application.failure = Some(HotkeyReconfigurationError::Rollback(format!(
                    "the shared input hook failed and the prior legacy route could not be restored: {error}"
                )));
                multi_launcher::gui::send_event(
                    multi_launcher::gui::WatchEvent::RadialRuntimeDiagnostic(format!(
                        "The shared launcher input hook stopped unexpectedly. Radial menus were disabled, but the prior legacy launcher route could not be restored: {error}"
                    )),
                );
            }
        }

        if let Some(mut service) = self.invocation_service.take() {
            service.stop();
        }
        finish_hotkey_application_error(application, self.listener, self.invocation_service, false);
        publish_paste_sequence_route_presentation(paste_sequence_route_presentation(
            self.settings,
            *self.settings_generation,
            if self.listener.is_active() {
                self.sequence_routes
            } else {
                &[]
            },
            &[],
            active_physical_paste_policy(self.settings, self.radial_document, self.sequence_routes),
        ));
    }

    fn commit(&mut self, mut application: PendingHotkeyApplication) {
        use multi_launcher::hotkey::reconfiguration::AppliedHotkeyReconfiguration;

        install_candidate_hotkey_listener(&mut application, self.listener);
        let candidate = application.prepared.settings.clone();
        *self.trigger = Arc::clone(&application.prepared.launcher);
        *self.quit_trigger = application.prepared.quit.clone();
        *self.help_trigger = application.prepared.help.clone();
        *self.screen_draw_trigger = application.prepared.screen_draw.clone();
        *self.emergency_trigger = application.prepared.emergency.clone();
        *self.sequence_routes = application.prepared.sequence_routes.clone();
        *self.sequence_triggers = application.prepared.sequence_triggers.clone();
        *self.settings = candidate;
        *self.settings_generation = application.candidate_generation;
        if invocation_service_needed(self.settings) {
            if self.invocation_service.is_none() {
                *self.invocation_service = application.prepared.staged_service.take();
            }
        } else if let Some(mut service) = self.invocation_service.take() {
            service.stop();
        }
        *self.applied_shared_route = applied_shared_route_after_successful_install(
            self.settings,
            self.radial_document,
            self.sequence_routes,
            *self.settings_generation,
        );
        self.radial_control_endpoint
            .set_enabled(self.settings.radial.enabled);
        self.radial_resources
            .set_runtime_enabled(self.settings.radial.enabled);
        self.radial_resources.reconcile(self.radial_controller);
        let tooltip_preferences =
            multi_launcher::radial::tooltip::TooltipPreferences::from(&self.settings.radial);
        match self
            .native_preview
            .set_tooltip_preferences(tooltip_preferences)
        {
            Ok(Some(diagnostics)) => {
                if let Some(lease) = self.native_preview.active_lease() {
                    let reply = AuthoringReply::NativePreviewDiagnostics {
                        editor_session: lease.editor_session,
                        lease,
                        diagnostics,
                    };
                    if self.authoring_reply_tx.send(reply).is_err() {
                        tracing::debug!("native preview diagnostics receiver disconnected");
                    }
                } else {
                    tracing::error!(
                        "native preview returned tooltip diagnostics without an active lease"
                    );
                    for diagnostic in diagnostics {
                        multi_launcher::gui::send_event(
                            multi_launcher::gui::WatchEvent::RadialDiagnostic(diagnostic),
                        );
                    }
                }
            }
            Ok(None) => {}
            Err(error) => {
                multi_launcher::gui::send_event(
                    multi_launcher::gui::WatchEvent::RadialRuntimeDiagnostic(format!(
                        "The radial authoring preview could not refresh tooltip preferences: {error}"
                    )),
                );
            }
        }
        for event in self
            .radial_controller
            .set_tooltip_preferences(tooltip_preferences)
        {
            match event {
                ControllerEvent::Diagnostic(diagnostic) => {
                    multi_launcher::gui::send_event(
                        multi_launcher::gui::WatchEvent::RadialDiagnostic(diagnostic),
                    );
                }
                ControllerEvent::Error(error) => {
                    multi_launcher::gui::send_event(
                        multi_launcher::gui::WatchEvent::RadialRuntimeDiagnostic(error),
                    );
                }
                ControllerEvent::LayoutFailed { menu_id, error } => {
                    multi_launcher::gui::send_event(
                        multi_launcher::gui::WatchEvent::RadialRuntimeDiagnostic(format!(
                            "Radial menu {menu_id} could not be placed after a tooltip preference change: {error:?}"
                        )),
                    );
                }
                other => tracing::warn!(
                    ?other,
                    "unexpected radial event while applying tooltip preferences"
                ),
            }
        }
        self.radial_controller.close(
            multi_launcher::radial::native::CloseReason::SettingsReload,
            None,
        );
        refresh_macro_hotkey_reservations(
            self.settings,
            self.radial_document,
            self.sequence_routes,
        );
        report_paste_sequence_route_omissions(&application.prepared.sequence_omissions);
        publish_paste_sequence_route_presentation(paste_sequence_route_presentation(
            self.settings,
            *self.settings_generation,
            self.sequence_routes,
            &application.prepared.sequence_omissions,
            application.prepared.physical_paste.clone(),
        ));
        multi_launcher::gui::send_event(multi_launcher::gui::WatchEvent::RadialInvalidate);

        if let Some(request) = application.request.take() {
            let request_id = request.request_id();
            let _ = request.complete_applied(AppliedHotkeyReconfiguration {
                request_id,
                generation: multi_launcher::hotkey::reconfiguration::HotkeyRuntimeGeneration(
                    application.candidate_generation,
                ),
            });
        }
    }
}

struct HotkeyRouteCompletionContext<'a> {
    runtime: HotkeyRuntimeCommitContext<'a>,
    pending_route: &'a mut Option<PendingLauncherRoute>,
    deferred_notices_follow_failure: &'a mut bool,
}

/// Advances the same owner state machine used by the main loop. The return
/// value preserves the old loop behavior of yielding immediately after a
/// confirmed route restoration.
fn complete_hotkey_route_step(
    mut application: PendingHotkeyApplication,
    result: Result<(), String>,
    context: &mut HotkeyRouteCompletionContext<'_>,
    handoff_issuer: &mut dyn HotkeyRouteHandoffIssuer,
) -> bool {
    use multi_launcher::hotkey::reconfiguration::HotkeyReconfigurationError;

    if application.phase == HotkeyApplyPhase::RestoreOldRoute {
        match result {
            Ok(()) => {
                *context.deferred_notices_follow_failure = true;
                *context.runtime.settings_generation = application
                    .restore_generation
                    .unwrap_or(application.candidate_generation);
                restore_fallback_hotkey_listener(&mut application, context.runtime.listener);
                if application.cancellation_requested {
                    application.failure = Some(HotkeyReconfigurationError::TransactionCancelled);
                }
                finish_hotkey_application_error(
                    application,
                    context.runtime.listener,
                    context.runtime.invocation_service,
                    false,
                );
                *context.runtime.applied_shared_route =
                    applied_shared_route_after_successful_install(
                        context.runtime.settings,
                        context.runtime.radial_document,
                        context.runtime.sequence_routes,
                        *context.runtime.settings_generation,
                    );
            }
            Err(restore_error) => {
                *context.deferred_notices_follow_failure = true;
                *context.runtime.settings_generation = application
                    .restore_generation
                    .unwrap_or(application.candidate_generation);
                let prior = application.failure.take().map_or_else(
                    || "the candidate route was cancelled".to_owned(),
                    |error| error.to_string(),
                );
                application.failure = Some(HotkeyReconfigurationError::Rollback(format!(
                    "failed to restore the prior route after {prior}: {restore_error}"
                )));
                finish_hotkey_application_error(
                    application,
                    context.runtime.listener,
                    context.runtime.invocation_service,
                    true,
                );
                *context.runtime.applied_shared_route = None;
            }
        }
        return true;
    }

    match application.phase {
        HotkeyApplyPhase::DrainOldRoute => match result {
            Err(error) => {
                application.failure = Some(HotkeyReconfigurationError::RuntimeTransition(error));
                begin_hotkey_route_rollback(
                    application,
                    context.runtime.listener,
                    context.runtime.invocation_service,
                    context.runtime.radial_document,
                    context.pending_route,
                    handoff_issuer,
                );
            }
            Ok(()) if application.cancellation_requested => {
                application.failure = Some(HotkeyReconfigurationError::TransactionCancelled);
                begin_hotkey_route_rollback(
                    application,
                    context.runtime.listener,
                    context.runtime.invocation_service,
                    context.runtime.radial_document,
                    context.pending_route,
                    handoff_issuer,
                );
            }
            Ok(()) if invocation_service_needed(&application.prepared.settings) => {
                install_candidate_hotkey_listener(&mut application, context.runtime.listener);
                let plan = radial_route_plan_with_sequences(
                    &application.prepared.settings,
                    context.runtime.radial_document,
                    application.candidate_generation,
                    &application.prepared.sequence_routes,
                );
                let route_service = if application.old_service_existed {
                    context.runtime.invocation_service.as_ref()
                } else {
                    application.prepared.staged_service.as_ref()
                };
                match handoff_issuer.begin(
                    route_service,
                    plan.config,
                    plan.related,
                    LifecycleCancellation::SettingsReload,
                ) {
                    Ok(handoff) => {
                        application.phase = HotkeyApplyPhase::ActivateCandidate;
                        *context.pending_route = Some(PendingLauncherRoute {
                            settings_generation: application.old_generation,
                            handoff: Some(handoff),
                            policy_applied: None,
                            completion: PendingLauncherRouteCompletion::Hotkey(Box::new(
                                application,
                            )),
                        });
                    }
                    Err(error) => {
                        application.failure =
                            Some(HotkeyReconfigurationError::RuntimeTransition(error));
                        begin_hotkey_route_rollback(
                            application,
                            context.runtime.listener,
                            context.runtime.invocation_service,
                            context.runtime.radial_document,
                            context.pending_route,
                            handoff_issuer,
                        );
                    }
                }
            }
            Ok(()) => {
                install_candidate_hotkey_listener(&mut application, context.runtime.listener);
                if let Some(mut service) = context.runtime.invocation_service.take() {
                    service.stop();
                }
                context.runtime.commit(application);
            }
        },
        HotkeyApplyPhase::ActivateCandidate => match result {
            Ok(()) => finish_hotkey_candidate_activation(
                application,
                &mut context.runtime,
                context.pending_route,
                handoff_issuer,
            ),
            Err(error) => {
                application.failure = Some(HotkeyReconfigurationError::RuntimeTransition(error));
                begin_hotkey_route_rollback(
                    application,
                    context.runtime.listener,
                    context.runtime.invocation_service,
                    context.runtime.radial_document,
                    context.pending_route,
                    handoff_issuer,
                );
            }
        },
        HotkeyApplyPhase::RestoreOldRoute => {
            unreachable!("restoration phase is handled before candidate phases")
        }
    }
    false
}

struct GuiWorker {
    handle: thread::JoinHandle<()>,
    completed: Arc<AtomicBool>,
}

struct GuiWorkerCompletion {
    completed: Arc<AtomicBool>,
    wake: Sender<()>,
}

impl GuiWorkerCompletion {
    fn complete(self) {
        drop(self);
    }
}

impl Drop for GuiWorkerCompletion {
    fn drop(&mut self) {
        // The wake can be consumed before the thread finishes its tail. Publish
        // the GUI completion fact first, including when GUI work unwinds.
        self.completed.store(true, Ordering::Release);
        let _ = self.wake.send(());
    }
}

#[derive(Debug, PartialEq, Eq)]
enum GuiWorkerEvent {
    Wake,
    Completed,
    ChannelClosed(RecvError),
}

impl GuiWorker {
    fn spawn(wake: Sender<()>, run: impl FnOnce(GuiWorkerCompletion) + Send + 'static) -> Self {
        let completed = Arc::new(AtomicBool::new(false));
        let completion = GuiWorkerCompletion {
            completed: Arc::clone(&completed),
            wake,
        };
        let handle = thread::spawn(move || run(completion));
        Self { handle, completed }
    }

    fn wait_for_event(&self, events: &Receiver<()>) -> GuiWorkerEvent {
        match events.recv() {
            Err(error) => GuiWorkerEvent::ChannelClosed(error),
            Ok(()) if self.completed.load(Ordering::Acquire) => GuiWorkerEvent::Completed,
            Ok(()) => GuiWorkerEvent::Wake,
        }
    }

    fn join(self) -> thread::Result<()> {
        self.handle.join()
    }
}

struct GuiStartupServices {
    settings_diagnostic: Option<SettingsStartupDiagnostic>,
    actions_diagnostic: Option<PersistenceError>,
    recovery: multi_launcher::persistence::RecoveryStartupResult,
    enabled_capabilities: Option<std::collections::HashMap<String, Vec<String>>>,
    screen_draw_recovery_bridge: Arc<ScreenDrawRecoveryBridge>,
    visibility_revision: VisibilityRevision,
    paste_sequence_document_revision: Arc<AtomicU64>,
    event_tx: Sender<()>,
    settings_save_workers: multi_launcher::gui::SettingsSaveWorkerRegistry,
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
    startup_indexing: Option<StartupIndexingTransfer>,
    settings: Settings,
    settings_path: String,
    mut plugins: PluginManager,
    startup_services: GuiStartupServices,
) -> (
    GuiWorker,
    Arc<AtomicBool>,
    Arc<AtomicBool>,
    Arc<AtomicBool>,
    Arc<Mutex<Option<RootViewportCtx>>>,
    RootWindowBridge,
) {
    let GuiStartupServices {
        settings_diagnostic: startup_settings_diagnostic,
        actions_diagnostic: startup_actions_diagnostic,
        recovery: startup_recovery,
        enabled_capabilities,
        screen_draw_recovery_bridge,
        visibility_revision,
        paste_sequence_document_revision,
        event_tx,
        settings_save_workers,
    } = startup_services;
    #[cfg(feature = "qa-framebuffer-capture")]
    let qa_framebuffer_request = match QaFramebufferRequest::from_environment() {
        Ok(request) => request,
        Err(error) => {
            tracing::error!(%error, "QA framebuffer capture request was rejected");
            None
        }
    };
    let custom_len_for_window = custom_len;
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
    let root_window_bridge = RootWindowBridge::default();
    let root_window_bridge_for_gui = root_window_bridge.clone();
    let actions_for_window = Arc::clone(&actions);
    let startup_indexing_for_window = startup_indexing;
    let settings_save_workers_for_window = settings_save_workers;
    let paste_sequence_document_revision_for_window = Arc::clone(&paste_sequence_document_revision);
    let hotkey_directory_main_wake = event_tx.clone();
    #[cfg(feature = "qa-framebuffer-capture")]
    let qa_framebuffer_request_for_window = qa_framebuffer_request;

    let worker = GuiWorker::spawn(event_tx, move |completion| {
        let viewport = build_viewport_with_icon(
            &settings,
            include_bytes!("../Resources/Green_MultiLauncher.png"),
        );
        let native_options = eframe::NativeOptions {
            viewport,
            renderer: eframe::Renderer::Glow,
            event_loop_builder: Some(Box::new(|_builder| {
                use winit::platform::windows::EventLoopBuilderExtWindows;
                _builder.with_any_thread(true);
            })),
            ..Default::default()
        };
        // Snapshot the real external foreground before eframe creates and focuses ROOT.
        let initial_snippet_origin =
            multi_launcher::active_window::capture_external_foreground_window();

        let _ = eframe::run_native(
            "Multi Lnchr",
            native_options,
            Box::new(move |cc| {
                if let Ok(mut guard) = ctx_clone.lock() {
                    *guard = Some(RootViewportCtx::with_window_bridge(
                        &cc.egui_ctx,
                        root_window_bridge_for_gui.clone(),
                    ));
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
                app.install_hotkey_directory_document_revision(
                    paste_sequence_document_revision_for_window,
                    hotkey_directory_main_wake,
                );
                app.set_initial_snippet_origin_window(initial_snippet_origin);
                app.install_settings_save_worker_registry(settings_save_workers_for_window);
                #[cfg(feature = "qa-framebuffer-capture")]
                if let Some(request) = qa_framebuffer_request_for_window
                    && let Err(error) = app.install_qa_framebuffer_request(request)
                {
                    tracing::error!(%error, "failed to install QA framebuffer capture");
                }
                if let Some(startup) = startup_indexing_for_window
                    && let Err(error) = app.install_startup_indexing(
                        startup.coordinator,
                        startup.config,
                        startup.generation,
                    )
                {
                    app.report_error_message("index.startup", error.to_string());
                }
                app.install_visibility_revision(visibility_revision);
                app.install_root_window_bridge(root_window_bridge_for_gui.clone());
                app.install_screen_draw_recovery_bridge(screen_draw_recovery_bridge);
                app.startup_settings_diagnostic = startup_settings_diagnostic;
                app.actions_persistence_diagnostic = startup_actions_diagnostic;
                app.set_startup_persistence_context(startup_recovery);
                launcher_timer.finish("startup.launcher_app");
                Ok(Box::new(app))
            }),
        );
        completion.complete();
    });

    (
        worker,
        visible_flag,
        restore_flag,
        help_flag,
        ctx_handle,
        root_window_bridge,
    )
}

fn main() -> anyhow::Result<()> {
    if let Some(exit_code) = startup_runtime::brightness_service::run_internal_helper_if_requested()
    {
        std::process::exit(exit_code);
    }
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
    let acceptance_prepare_hold_file =
        std::env::var_os(RADIAL_ACCEPTANCE_PREPARE_HOLD_FILE_ENV).map(PathBuf::from);
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
    let actions_vec = startup_actions.actions;
    let startup_actions_diagnostic = startup_actions.diagnostic;
    tracing::debug!("{} actions loaded", actions_vec.len());
    if let Some(diagnostic) = startup_actions_diagnostic.as_ref() {
        tracing::error!(
            error = %diagnostic,
            "actions startup used a temporary empty list without replacing the persisted file"
        );
    }
    actions_timer.finish("startup.action_load");

    let initial_sequence_document_revision = multi_launcher::paste_sequence::SequenceStore::new(
        multi_launcher::paste_sequence::PASTE_SEQUENCES_FILE,
    )
    .load()
    .map_or(0, |snapshot| snapshot.revision());
    let paste_sequence_document_revision =
        Arc::new(AtomicU64::new(initial_sequence_document_revision));
    let (event_tx, event_rx) = channel::<()>();
    let mut hotkey_runtime_receiver =
        Some(multi_launcher::hotkey::reconfiguration::install_main_runtime(event_tx.clone())?);
    install_exclusive_wake(event_tx.clone());

    let index_timer = multi_launcher::performance::Timer::start();
    let index_config = multi_launcher::indexer::coordinator::IndexConfig::new(
        settings.index_paths.clone().unwrap_or_default(),
        settings.max_indexed_items,
    );
    let (actions, custom_len, startup_indexing) =
        startup_action_catalog(actions_vec, index_config)?;
    index_timer.finish("startup.action_indexing");
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
    let mut paste_sequence_triggers = Vec::new();
    let watched = legacy_listener_triggers_with_sequences(
        shared_invocation,
        &trigger,
        quit_trigger.as_ref(),
        help_trigger.as_ref(),
        screen_draw_trigger.as_ref(),
        emergency_trigger.as_ref(),
        &paste_sequence_triggers,
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
    // Initialize the existing macro owner before resolving Paste priorities.
    // No Paste routes are active yet; reserve process/radial routes while the
    // macro runtime loads, then install the selected Paste reservations below.
    let mut startup_reservations = reserved_launcher_hotkeys_with_sequences(&settings, &[]);
    startup_reservations.extend(radial_global_hotkey_reservations(
        &settings,
        &radial_document,
    ));
    let startup_reservation_refs = startup_reservations
        .iter()
        .map(|(name, chord)| (name.as_str(), chord.as_str()))
        .collect::<Vec<_>>();
    let manager_timer = multi_launcher::performance::Timer::start();
    let plugins = PluginManager::new_with_reserved_hotkeys(&startup_reservation_refs);
    manager_timer.finish("startup.plugin_manager");
    let macro_policy_dirty = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let macro_dirty = Arc::clone(&macro_policy_dirty);
    let macro_directory_revision = Arc::new(std::sync::atomic::AtomicU64::new(1));
    let macro_directory_revision_after = Arc::clone(&macro_directory_revision);
    let macro_wake = event_tx.clone();
    let observer = Arc::new(
        multi_launcher::mkmacro::store::DocumentPublicationObserver {
            before: Arc::new(|| physical_paste_bridge().begin_source_publication()),
            after: Arc::new(move || {
                physical_paste_bridge().finish_source_publication();
                macro_directory_revision_after.fetch_add(1, Ordering::AcqRel);
                macro_dirty.store(true, Ordering::Release);
                let _ = macro_wake.send(());
            }),
        },
    );
    physical_paste_bridge().begin_source_publication();
    match multi_launcher::mkmacro::runtime::observe_shared_document_publication(observer) {
        Ok(()) => physical_paste_bridge().finish_source_publication(),
        Err(error) => {
            // Keep the fence closed: no unfenced macro source may coexist
            // with a physical-paste lease.
            tracing::warn!(%error, "physical paste shortcut policy unavailable without macro publication fencing");
        }
    }
    let startup_radial_issues = multi_launcher::radial::settings::validate(
        &settings.radial,
        &radial_document,
        &reserved_launcher_hotkeys_with_sequences(&settings, &[]),
    );
    if !startup_radial_issues.is_empty() {
        settings.radial.enabled = false;
        shared_invocation = false;
        listener.stop();
        listener = HotkeyTrigger::start_listener(
            legacy_listener_triggers_with_sequences(
                false,
                &trigger,
                quit_trigger.as_ref(),
                help_trigger.as_ref(),
                screen_draw_trigger.as_ref(),
                emergency_trigger.as_ref(),
                &paste_sequence_triggers,
            ),
            "main",
            event_tx.clone(),
        );
        for issue in startup_radial_issues {
            tracing::error!(field = issue.field, message = %issue.message, "invalid radial settings at startup; radial runtime disabled fail-closed");
        }
    }
    let (mut paste_sequence_routes, startup_sequence_omissions) =
        match resolve_hotkey_route_candidate(&settings, &radial_document, None) {
            Ok(resolved) => {
                report_paste_sequence_route_omissions(&resolved.omissions);
                (resolved.sequences, resolved.omissions)
            }
            Err(error) => {
                multi_launcher::gui::send_event(
                    multi_launcher::gui::WatchEvent::PasteSequenceRuntimeDiagnostic(format!(
                        "Paste Sequence shortcuts are inactive: {error}"
                    )),
                );
                (Vec::new(), Vec::new())
            }
        };
    refresh_macro_hotkey_reservations(&settings, &radial_document, &paste_sequence_routes);
    paste_sequence_triggers = sequence_hotkey_triggers_from_routes(&paste_sequence_routes);
    listener.stop();
    listener = HotkeyTrigger::start_listener(
        legacy_listener_triggers_with_sequences(
            shared_invocation,
            &trigger,
            quit_trigger.as_ref(),
            help_trigger.as_ref(),
            screen_draw_trigger.as_ref(),
            emergency_trigger.as_ref(),
            &paste_sequence_triggers,
        ),
        "main",
        event_tx.clone(),
    );
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
    let gallery_service = match multi_launcher::radial::gallery::GalleryPreparationService::new(
        app_data_root.path().to_path_buf(),
        authoring_endpoint.gallery_reply_sink(),
    ) {
        Ok(service) => Some(service),
        Err(error) => {
            tracing::warn!(%error,"radial gallery worker unavailable");
            None
        }
    };
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
    let mut gallery_editor_session = None;
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
    let mut applied_shared_route = None;
    if invocation_service_needed(&settings) {
        match LauncherInvocationService::start(
            radial_invocation_config(
                &settings,
                &radial_document,
                shared_invocation,
                settings_generation,
                &paste_sequence_routes,
            ),
            related_launcher_bindings_with_sequences(
                &settings,
                &radial_document,
                shared_invocation,
                &paste_sequence_routes,
            ),
            Arc::new(move || {
                if let Some(lifetime) = owner_bridge.active_lifetime() {
                    PriorityOwner::ScreenDrawRecovery(lifetime)
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
                applied_shared_route = applied_shared_route_after_successful_install(
                    &settings,
                    &radial_document,
                    &paste_sequence_routes,
                    settings_generation,
                );
            }
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
                    &paste_sequence_triggers,
                    &event_tx,
                );
                radial_resources.set_runtime_enabled(false);
                radial_resources.reconcile(&mut radial_controller);
            }
        }
    }
    // `visibility` holds whether the window is currently restored (true) or
    // minimized (false).
    let visibility_revision = VisibilityRevision::default();
    let settings_save_workers = multi_launcher::gui::SettingsSaveWorkerRegistry::default();
    publish_paste_sequence_route_presentation(paste_sequence_route_presentation(
        &settings,
        settings_generation,
        if invocation_service.is_some() || listener.is_active() {
            &paste_sequence_routes
        } else {
            &[]
        },
        &startup_sequence_omissions,
        active_physical_paste_policy(&settings, &radial_document, &paste_sequence_routes),
    ));
    let startup_services = GuiStartupServices {
        settings_diagnostic: startup_settings_diagnostic,
        actions_diagnostic: startup_actions_diagnostic,
        recovery: startup_recovery,
        enabled_capabilities: settings.enabled_capabilities.clone(),
        screen_draw_recovery_bridge: Arc::clone(&screen_draw_recovery_bridge),
        visibility_revision: visibility_revision.clone(),
        paste_sequence_document_revision: Arc::clone(&paste_sequence_document_revision),
        event_tx: event_tx.clone(),
        settings_save_workers: settings_save_workers.clone(),
    };
    let (gui_worker, visibility, restore_flag, help_flag, ctx, root_window_bridge) = spawn_gui(
        Arc::clone(&actions),
        custom_len,
        startup_indexing,
        settings.clone(),
        "settings.json".to_string(),
        plugins,
        startup_services,
    );
    let mut queued_visibility: Option<bool> = None;
    let mut previous_exclusive = false;
    let mut pending_launcher_route: Option<PendingLauncherRoute> = None;
    let mut prepared_hotkey_routes: HashMap<_, PreparedHotkeyRoute> = HashMap::new();
    let mut deferred_service_notices = VecDeque::new();
    let mut deferred_paste_sequence_actions = VecDeque::new();
    let mut deferred_notices_follow_hotkey_failure = false;
    let mut hotkey_handoff_issuer = NativeHotkeyRouteHandoffIssuer;
    let hotkey_service_owner = invocation_priority_owner(Arc::clone(&screen_draw_recovery_bridge));
    let mut external_radial_invocations: HashMap<InvocationId, ExternalRadialMetadata> =
        HashMap::new();
    let mut last_hotkey_directory_publication = None;
    let mut hotkey_directory_source_publication_revision = 0u64;
    publish_hotkey_directory_if_changed(
        &mut last_hotkey_directory_publication,
        &mut hotkey_directory_source_publication_revision,
        &settings,
        settings_generation,
        &radial_document,
        paste_sequence_document_revision.load(Ordering::Acquire),
        &paste_sequence_routes,
        &[],
        macro_directory_revision.load(Ordering::Acquire),
        &listener,
        invocation_service.as_ref(),
        applied_shared_route.as_ref(),
        pending_launcher_route.as_ref(),
    );

    loop {
        let shutdown = match gui_worker.wait_for_event(&event_rx) {
            GuiWorkerEvent::Wake => false,
            GuiWorkerEvent::Completed => true,
            GuiWorkerEvent::ChannelClosed(err) => {
                tracing::error!(?err, "event channel closed; shutting down launcher loop");
                true
            }
        };
        if shutdown {
            settings_save_workers.begin_shutdown();
            drop(hotkey_runtime_receiver.take());
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
            let _ = gui_worker.join();
            settings_save_workers.join_all();
            break Ok(());
        }

        let radial_changes = if settings_hotkey_route_is_pending(&pending_launcher_route) {
            multi_launcher::radial::watch::RadialWatchChanges::default()
        } else {
            radial_resources.take_changes()
        };
        let mut radial_generation_replaced = false;
        if radial_changes.document {
            let mut admitted_routes = None;
            let reload = radial_store.reload_external_with(|candidate| {
                admitted_routes = Some(
                    resolve_hotkey_route_candidate(&settings, candidate, None)
                        .map_err(|error| error.to_string())?,
                );
                multi_launcher::gui::send_event(multi_launcher::gui::WatchEvent::RadialInvalidate);
                radial_controller.close(
                    multi_launcher::radial::native::CloseReason::SettingsReload,
                    None,
                );
                Ok(())
            });
            match reload {
                Ok(ExternalReloadOutcome::Unchanged) => {
                    tracing::debug!("ignored radial document self-save/unchanged echo");
                }
                Ok(ExternalReloadOutcome::Published(document)) => {
                    let Some(resolved) = admitted_routes.take() else {
                        // Published requires successful owner admission under the store lock.
                        tracing::error!("radial reload published without admitted routes");
                        continue;
                    };
                    paste_sequence_routes = resolved.sequences;
                    paste_sequence_triggers =
                        sequence_hotkey_triggers_from_routes(&paste_sequence_routes);
                    report_paste_sequence_route_omissions(&resolved.omissions);
                    // The shared owner changes at its acknowledged handoff. In
                    // fallback mode Paste belongs only to the polling listener.
                    if !(settings.radial.enabled && settings.radial.shared_tap_hold) {
                        listener.stop();
                        listener = HotkeyTrigger::start_listener(
                            legacy_listener_triggers_with_sequences(
                                false,
                                &trigger,
                                quit_trigger.as_ref(),
                                help_trigger.as_ref(),
                                screen_draw_trigger.as_ref(),
                                emergency_trigger.as_ref(),
                                &paste_sequence_triggers,
                            ),
                            "main",
                            event_tx.clone(),
                        );
                    }
                    native_preview.cancel_all();
                    if let Some(service) = &gallery_service {
                        service.invalidate_resources();
                    }
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
                    refresh_macro_hotkey_reservations(
                        &settings,
                        &radial_document,
                        &paste_sequence_routes,
                    );
                    settings_generation = settings_generation.checked_add(1).unwrap_or(1);
                    let issues = multi_launcher::radial::settings::validate(
                        &settings.radial,
                        &radial_document,
                        &reserved_launcher_hotkeys_with_sequences(
                            &settings,
                            &paste_sequence_routes,
                        ),
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
                    let route_plan = radial_route_plan_with_sequences(
                        &settings,
                        &radial_document,
                        settings_generation,
                        &paste_sequence_routes,
                    );
                    if let Some(service) = invocation_service.as_ref() {
                        match service.begin_route_handoff(
                            route_plan.config,
                            route_plan.related,
                            LifecycleCancellation::SettingsReload,
                        ) {
                            Ok(handoff) => {
                                pending_launcher_route = Some(PendingLauncherRoute {
                                    settings_generation,
                                    handoff: Some(Box::new(handoff)),
                                    policy_applied: None,
                                    completion: PendingLauncherRouteCompletion::Reconcile {
                                        start_legacy: route_plan.start_legacy,
                                        stop_service: route_plan.stop_service,
                                        paste_presentation: paste_sequence_route_presentation(
                                            &settings,
                                            settings_generation,
                                            &paste_sequence_routes,
                                            &resolved.omissions,
                                            resolved.physical_paste.clone(),
                                        ),
                                    },
                                });
                            }
                            Err(error) => {
                                tracing::error!(%error, "radial trigger reload failed closed");
                                applied_shared_route = None;
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
                                    &paste_sequence_triggers,
                                    &event_tx,
                                );
                            }
                        }
                    } else if invocation_service_needed(&settings) {
                        let owner_bridge = Arc::clone(&screen_draw_recovery_bridge);
                        match LauncherInvocationService::start(
                            route_plan.config,
                            route_plan.related,
                            Arc::new(move || {
                                if let Some(lifetime) = owner_bridge.active_lifetime() {
                                    PriorityOwner::ScreenDrawRecovery(lifetime)
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
                                applied_shared_route =
                                    applied_shared_route_after_successful_install(
                                        &settings,
                                        &radial_document,
                                        &paste_sequence_routes,
                                        settings_generation,
                                    );
                            }
                            Err(error) => {
                                tracing::error!(%error, "radial trigger service restart failed closed");
                                applied_shared_route = None;
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
                                    &paste_sequence_triggers,
                                    &event_tx,
                                );
                            }
                        }
                    } else {
                        listener.stop();
                        listener = HotkeyTrigger::start_listener(
                            legacy_listener_triggers_with_sequences(
                                false,
                                &trigger,
                                quit_trigger.as_ref(),
                                help_trigger.as_ref(),
                                screen_draw_trigger.as_ref(),
                                emergency_trigger.as_ref(),
                                &paste_sequence_triggers,
                            ),
                            "main",
                            event_tx.clone(),
                        );
                    }
                    if pending_launcher_route.is_none() {
                        publish_paste_sequence_route_presentation(
                            paste_sequence_route_presentation(
                                &settings,
                                settings_generation,
                                &paste_sequence_routes,
                                &resolved.omissions,
                                resolved.physical_paste.clone(),
                            ),
                        );
                    }
                    multi_launcher::gui::send_event(
                        multi_launcher::gui::WatchEvent::RadialConfigDiagnostic(None),
                    );
                }
                Err(error) => {
                    tracing::warn!(%error, "retaining last valid radial document after external change");
                    multi_launcher::gui::send_event(
                        multi_launcher::gui::WatchEvent::RadialConfigDiagnostic(Some(
                            error.to_string(),
                        )),
                    );
                }
            }
        }
        if radial_changes.assets {
            native_preview.invalidate_resources();
            if let Some(service) = &gallery_service {
                service.invalidate_resources();
            }
            if !radial_generation_replaced {
                multi_launcher::gui::send_event(multi_launcher::gui::WatchEvent::RadialInvalidate);
                radial_controller.invalidate_resources();
            }
            // If the asset tree was created after watcher acquisition, rebuild
            // the narrow watch set so subsequent descendants are observed.
            radial_resources.refresh_watcher();
        }

        loop {
            let message = match hotkey_runtime_receiver
                .as_ref()
                .expect("hotkey runtime receiver stays installed until shutdown")
                .try_recv()
            {
                Ok(Some(message)) => message,
                Ok(None) => break,
                Err(error) => {
                    tracing::error!(%error, "hotkey reconfiguration receiver disconnected");
                    break;
                }
            };
            use multi_launcher::hotkey::reconfiguration::{
                HotkeyReconfigurationError, HotkeyRuntimeMessage,
            };
            match message {
                HotkeyRuntimeMessage::Prepare(request) => prepare_hotkey_runtime_message(
                    request,
                    &radial_document,
                    settings_generation,
                    &event_tx,
                    invocation_service.is_some(),
                    Arc::clone(&hotkey_service_owner),
                    &mut prepared_hotkey_routes,
                ),
                HotkeyRuntimeMessage::Apply(request) => {
                    begin_hotkey_application(HotkeyApplicationStart {
                        request,
                        prepared_routes: &mut prepared_hotkey_routes,
                        settings: &mut settings,
                        settings_generation: &mut settings_generation,
                        sequence_routes: &mut paste_sequence_routes,
                        radial_document: &radial_document,
                        event_tx: &event_tx,
                        listener: &mut listener,
                        invocation_service: &mut invocation_service,
                        pending_launcher_route: &mut pending_launcher_route,
                    });
                }
                HotkeyRuntimeMessage::Cancel(mut request) => {
                    let request_id = request.request_id();
                    if let Some(PendingLauncherRoute {
                        completion: PendingLauncherRouteCompletion::Hotkey(application),
                        ..
                    }) = pending_launcher_route.as_mut()
                        && application
                            .request
                            .as_ref()
                            .is_some_and(|pending| pending.request_id() == request_id)
                    {
                        application.cancellation_requested = true;
                        request.defer_terminal();
                    } else if let Some(mut prepared) = prepared_hotkey_routes.remove(&request_id) {
                        if let Some(mut staged) = prepared.staged_service.take() {
                            staged.stop();
                        }
                        let _ = request
                            .complete_restored(HotkeyReconfigurationError::TransactionCancelled);
                    } else {
                        let _ = request
                            .complete_restored(HotkeyReconfigurationError::TransactionCancelled);
                    }
                }
            }
        }

        while let Some(demand) = try_recv_when_hotkey_route_is_idle(
            &pending_launcher_route,
            &authoring_endpoint.resource_rx,
        ) {
            let released = match demand {
                AuthoringResourceDemand::Acquire(editor_session) => {
                    authoring_editor_session = Some(editor_session);
                    gallery_editor_session = Some(editor_session);
                    false
                }
                AuthoringResourceDemand::Release(editor_session) => {
                    if gallery_editor_session == Some(editor_session) {
                        gallery_editor_session = None;
                        if let Some(service) = &gallery_service {
                            service.invalidate_resources();
                        }
                    }
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

        while let Some(request) = try_recv_when_hotkey_route_is_idle(
            &pending_launcher_route,
            &authoring_endpoint.request_rx,
        ) {
            hold_acceptance_prepare_request(&request, acceptance_prepare_hold_file.as_deref());
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
                        resolve_hotkey_route_candidate(&settings, &plan.document, None)
                            .map_err(|error| error.to_string())
                            .and_then(|resolved| {
                                radial_store
                                    .replace_package_with_backup(
                                        plan,
                                        expected_revision,
                                        expected_disk_sha256.0,
                                        backup_path.clone(),
                                    )
                                    .map(|published| (published, resolved))
                                    .map_err(|error| error.to_string())
                            })
                    };
                    match result {
                        Ok((published, resolved)) => {
                            paste_sequence_routes = resolved.sequences;
                            paste_sequence_triggers =
                                sequence_hotkey_triggers_from_routes(&paste_sequence_routes);
                            report_paste_sequence_route_omissions(&resolved.omissions);
                            if !(settings.radial.enabled && settings.radial.shared_tap_hold) {
                                listener.stop();
                                listener = HotkeyTrigger::start_listener(
                                    legacy_listener_triggers_with_sequences(
                                        false,
                                        &trigger,
                                        quit_trigger.as_ref(),
                                        help_trigger.as_ref(),
                                        screen_draw_trigger.as_ref(),
                                        emergency_trigger.as_ref(),
                                        &paste_sequence_triggers,
                                    ),
                                    "main",
                                    event_tx.clone(),
                                );
                            }
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
                            refresh_macro_hotkey_reservations(
                                &settings,
                                &radial_document,
                                &paste_sequence_routes,
                            );
                            settings_generation = settings_generation.checked_add(1).unwrap_or(1);
                            let route_plan = radial_route_plan_with_sequences(
                                &settings,
                                &radial_document,
                                settings_generation,
                                &paste_sequence_routes,
                            );
                            if let Some(service) = invocation_service.as_ref() {
                                match service.begin_route_handoff(
                                    route_plan.config,
                                    route_plan.related,
                                    LifecycleCancellation::SettingsReload,
                                ) {
                                    Ok(handoff) => {
                                        pending_launcher_route = Some(PendingLauncherRoute {
                                            settings_generation,
                                            handoff: Some(Box::new(handoff)),
                                            policy_applied: None,
                                            completion: PendingLauncherRouteCompletion::Reconcile {
                                                start_legacy: route_plan.start_legacy,
                                                stop_service: route_plan.stop_service,
                                                paste_presentation:
                                                    paste_sequence_route_presentation(
                                                        &settings,
                                                        settings_generation,
                                                        &paste_sequence_routes,
                                                        &resolved.omissions,
                                                        resolved.physical_paste.clone(),
                                                    ),
                                            },
                                        });
                                    }
                                    Err(error) => {
                                        tracing::error!(%error, "radial package replacement trigger refresh failed closed");
                                        applied_shared_route = None;
                                        settings.radial.enabled = false;
                                        radial_control_endpoint.set_enabled(false);
                                        radial_controller.disable();
                                        if let Some(mut service) = invocation_service.take() {
                                            service.stop();
                                        }
                                        listener.stop();
                                        listener = HotkeyTrigger::start_listener(
                                            legacy_listener_triggers_with_sequences(
                                                false,
                                                &trigger,
                                                quit_trigger.as_ref(),
                                                help_trigger.as_ref(),
                                                screen_draw_trigger.as_ref(),
                                                emergency_trigger.as_ref(),
                                                &paste_sequence_triggers,
                                            ),
                                            "main",
                                            event_tx.clone(),
                                        );
                                    }
                                }
                            }
                            if pending_launcher_route.is_none() {
                                publish_paste_sequence_route_presentation(
                                    paste_sequence_route_presentation(
                                        &settings,
                                        settings_generation,
                                        &paste_sequence_routes,
                                        &resolved.omissions,
                                        resolved.physical_paste.clone(),
                                    ),
                                );
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
                        resolve_hotkey_route_candidate(&settings, &candidate, None)
                            .map_err(|error| error.to_string())
                    })
                    .and_then(|resolved| {
                        radial_store
                            .commit_authoring(
                                expected_revision,
                                &expected_disk_sha256,
                                candidate,
                                assets,
                            )
                            .map(|published| (published, resolved))
                            .map_err(|error| error.to_string())
                    });
                    match result {
                        Ok((published, resolved)) => {
                            paste_sequence_routes = resolved.sequences;
                            paste_sequence_triggers =
                                sequence_hotkey_triggers_from_routes(&paste_sequence_routes);
                            report_paste_sequence_route_omissions(&resolved.omissions);
                            if !(settings.radial.enabled && settings.radial.shared_tap_hold) {
                                listener.stop();
                                listener = HotkeyTrigger::start_listener(
                                    legacy_listener_triggers_with_sequences(
                                        false,
                                        &trigger,
                                        quit_trigger.as_ref(),
                                        help_trigger.as_ref(),
                                        screen_draw_trigger.as_ref(),
                                        emergency_trigger.as_ref(),
                                        &paste_sequence_triggers,
                                    ),
                                    "main",
                                    event_tx.clone(),
                                );
                            }
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
                            refresh_macro_hotkey_reservations(
                                &settings,
                                &radial_document,
                                &paste_sequence_routes,
                            );
                            settings_generation = settings_generation.checked_add(1).unwrap_or(1);
                            let route_plan = radial_route_plan_with_sequences(
                                &settings,
                                &radial_document,
                                settings_generation,
                                &paste_sequence_routes,
                            );
                            if let Some(service) = invocation_service.as_ref() {
                                match service.begin_route_handoff(
                                    route_plan.config,
                                    route_plan.related,
                                    LifecycleCancellation::SettingsReload,
                                ) {
                                    Ok(handoff) => {
                                        pending_launcher_route = Some(PendingLauncherRoute {
                                            settings_generation,
                                            handoff: Some(Box::new(handoff)),
                                            policy_applied: None,
                                            completion: PendingLauncherRouteCompletion::Reconcile {
                                                start_legacy: route_plan.start_legacy,
                                                stop_service: route_plan.stop_service,
                                                paste_presentation:
                                                    paste_sequence_route_presentation(
                                                        &settings,
                                                        settings_generation,
                                                        &paste_sequence_routes,
                                                        &resolved.omissions,
                                                        resolved.physical_paste.clone(),
                                                    ),
                                            },
                                        });
                                    }
                                    Err(error) => {
                                        tracing::error!(%error, "radial authoring trigger refresh failed closed");
                                        applied_shared_route = None;
                                        settings.radial.enabled = false;
                                        radial_control_endpoint.set_enabled(false);
                                        radial_controller.disable();
                                        if let Some(mut service) = invocation_service.take() {
                                            service.stop();
                                        }
                                        listener.stop();
                                        listener = HotkeyTrigger::start_listener(
                                            legacy_listener_triggers_with_sequences(
                                                false,
                                                &trigger,
                                                quit_trigger.as_ref(),
                                                help_trigger.as_ref(),
                                                screen_draw_trigger.as_ref(),
                                                emergency_trigger.as_ref(),
                                                &paste_sequence_triggers,
                                            ),
                                            "main",
                                            event_tx.clone(),
                                        );
                                    }
                                }
                            }
                            if pending_launcher_route.is_none() {
                                publish_paste_sequence_route_presentation(
                                    paste_sequence_route_presentation(
                                        &settings,
                                        settings_generation,
                                        &paste_sequence_routes,
                                        &resolved.omissions,
                                        resolved.physical_paste.clone(),
                                    ),
                                );
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

        // Controls, resource release and native lifecycle replies take priority
        // over the separate capacity-one thumbnail queue. No native tile window.
        if !settings_hotkey_route_is_pending(&pending_launcher_route)
            && let Ok(request) = authoring_endpoint.gallery_rx.try_recv()
        {
            let correlation = request.correlation;
            let result = if gallery_editor_session != Some(correlation.editor_session)
                || !request.interest.load(std::sync::atomic::Ordering::Acquire)
            {
                Err("gallery editor interest was released".into())
            } else if let Some(service) = &gallery_service {
                service.try_prepare(request)
            } else {
                Err("gallery preparation service unavailable".into())
            };
            if let Err(message) = result {
                authoring_endpoint.send_gallery_reply(
                    multi_launcher::radial::gallery::GalleryReply {
                        correlation,
                        result: Err(message),
                    },
                );
            }
        }

        if let Some(pending) = pending_launcher_route.as_mut()
            && pending.policy_applied.is_none()
        {
            pending.policy_applied = pending
                .handoff
                .as_ref()
                .and_then(|handoff| handoff.try_policy_applied());
        }

        let cancel_after_policy_install = pending_launcher_route.as_ref().is_some_and(|pending| {
            pending.policy_applied == Some(Ok(()))
                && matches!(
                    &pending.completion,
                    PendingLauncherRouteCompletion::Hotkey(application)
                        if application.cancellation_requested
                            && application.phase != HotkeyApplyPhase::RestoreOldRoute
                )
        });
        if cancel_after_policy_install
            && let Some(PendingLauncherRoute {
                completion: PendingLauncherRouteCompletion::Hotkey(application),
                ..
            }) = pending_launcher_route.take()
        {
            use multi_launcher::hotkey::reconfiguration::HotkeyReconfigurationError;
            let mut application = *application;
            application.failure = Some(HotkeyReconfigurationError::TransactionCancelled);
            begin_hotkey_route_rollback(
                application,
                &mut listener,
                &mut invocation_service,
                &radial_document,
                &mut pending_launcher_route,
                &mut hotkey_handoff_issuer,
            );
        }

        let restored_shared_policy = pending_launcher_route.as_ref().is_some_and(|pending| {
            pending.policy_applied == Some(Ok(()))
                && matches!(
                    &pending.completion,
                    PendingLauncherRouteCompletion::Hotkey(application)
                        if application.old_service_existed
                            && application.phase == HotkeyApplyPhase::RestoreOldRoute
                )
        });
        if restored_shared_policy
            && let Some(PendingLauncherRoute {
                completion: PendingLauncherRouteCompletion::Hotkey(application),
                ..
            }) = pending_launcher_route.take()
        {
            let mut application = *application;
            deferred_notices_follow_hotkey_failure = true;
            settings_generation = application
                .restore_generation
                .unwrap_or(application.candidate_generation);
            if application.failure.is_none() {
                application.failure = Some(
                    multi_launcher::hotkey::reconfiguration::HotkeyReconfigurationError::
                        TransactionCancelled,
                );
            }
            finish_hotkey_application_error(
                application,
                &mut listener,
                &mut invocation_service,
                false,
            );
            applied_shared_route = applied_shared_route_after_successful_install(
                &settings,
                &radial_document,
                &paste_sequence_routes,
                settings_generation,
            );
        }

        let handoff_result = pending_launcher_route.as_ref().and_then(|pending| {
            pending
                .handoff
                .as_ref()
                .map_or(Some(Ok(())), |handoff| handoff.try_complete())
        });
        if let Some(result) = handoff_result {
            let pending = pending_launcher_route
                .take()
                .expect("completed launcher handoff remains pending");
            match pending.completion {
                PendingLauncherRouteCompletion::Reconcile {
                    start_legacy,
                    stop_service,
                    paste_presentation,
                } if pending.settings_generation == settings_generation => match result {
                    Ok(()) => {
                        if stop_service && let Some(mut service) = invocation_service.take() {
                            service.stop();
                        }
                        if start_legacy {
                            listener.stop();
                            listener = HotkeyTrigger::start_listener(
                                legacy_listener_triggers_with_sequences(
                                    false,
                                    &trigger,
                                    quit_trigger.as_ref(),
                                    help_trigger.as_ref(),
                                    screen_draw_trigger.as_ref(),
                                    emergency_trigger.as_ref(),
                                    &paste_sequence_triggers,
                                ),
                                "main",
                                event_tx.clone(),
                            );
                        }
                        publish_paste_sequence_route_presentation(paste_presentation);
                        applied_shared_route = applied_shared_route_after_successful_install(
                            &settings,
                            &radial_document,
                            &paste_sequence_routes,
                            settings_generation,
                        );
                    }
                    Err(error) => {
                        applied_shared_route = None;
                        tracing::error!(%error, "launcher route handoff failed closed");
                        settings.radial.enabled = false;
                        radial_control_endpoint.set_enabled(false);
                        radial_controller.disable();
                        if let Some(mut service) = invocation_service.take() {
                            service.stop();
                        }
                        listener.stop();
                        listener = HotkeyTrigger::start_listener(
                            legacy_listener_triggers_with_sequences(
                                false,
                                &trigger,
                                quit_trigger.as_ref(),
                                help_trigger.as_ref(),
                                screen_draw_trigger.as_ref(),
                                emergency_trigger.as_ref(),
                                &paste_sequence_triggers,
                            ),
                            "main",
                            event_tx.clone(),
                        );
                        publish_paste_sequence_route_presentation(if listener.is_active() {
                            paste_sequence_presentation_for_polling(paste_presentation)
                        } else {
                            paste_sequence_route_presentation(
                                &settings,
                                settings_generation,
                                &[],
                                &[],
                                active_physical_paste_policy(
                                    &settings,
                                    &radial_document,
                                    &paste_sequence_routes,
                                ),
                            )
                        });
                    }
                },
                PendingLauncherRouteCompletion::Reconcile { .. } => {
                    tracing::warn!(
                        pending_generation = pending.settings_generation,
                        current_generation = settings_generation,
                        "discarded stale launcher route handoff completion"
                    );
                }
                PendingLauncherRouteCompletion::Hotkey(application) => {
                    let mut context = HotkeyRouteCompletionContext {
                        runtime: HotkeyRuntimeCommitContext {
                            settings: &mut settings,
                            settings_generation: &mut settings_generation,
                            trigger: &mut trigger,
                            quit_trigger: &mut quit_trigger,
                            help_trigger: &mut help_trigger,
                            screen_draw_trigger: &mut screen_draw_trigger,
                            emergency_trigger: &mut emergency_trigger,
                            listener: &mut listener,
                            invocation_service: &mut invocation_service,
                            radial_control_endpoint: &radial_control_endpoint,
                            radial_resources: &mut radial_resources,
                            radial_controller: &mut radial_controller,
                            native_preview: &mut native_preview,
                            radial_document: &radial_document,
                            authoring_reply_tx: &authoring_endpoint.reply_tx,
                            event_tx: &event_tx,
                            sequence_routes: &mut paste_sequence_routes,
                            applied_shared_route: &mut applied_shared_route,
                            sequence_triggers: &mut paste_sequence_triggers,
                        },
                        pending_route: &mut pending_launcher_route,
                        deferred_notices_follow_failure:
                            &mut deferred_notices_follow_hotkey_failure,
                    };
                    if complete_hotkey_route_step(
                        *application,
                        result,
                        &mut context,
                        &mut hotkey_handoff_issuer,
                    ) {
                        continue;
                    }
                }
            }
        }

        let mut radial_notices = Vec::new();
        while let Some(request) = try_recv_when_hotkey_route_is_idle(
            &pending_launcher_route,
            &radial_control_endpoint.request_rx,
        ) {
            match request {
                RadialControlRequest::Close => radial_notices.push(ServiceNotice {
                    recovery: None,
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
                                recovery: None,
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
        // Physical admissions are never deferred with configured actions. Their
        // exact playback lease must still be current after any route transition.
        drain_physical_paste_notices(settings_hotkey_route_is_pending(&pending_launcher_route));
        if let Some(service) = invocation_service.as_ref() {
            while let Some(mut notice) = service.try_recv() {
                let hook_failed = notice.cancellation == Some(LifecycleCancellation::HookFailure);
                if settings_hotkey_route_is_pending(&pending_launcher_route) && !hook_failed {
                    if transition_process_controls_are_live(&pending_launcher_route) {
                        apply_service_related_action(
                            &mut notice,
                            quit_trigger.as_ref(),
                            help_trigger.as_ref(),
                            screen_draw_trigger.as_ref(),
                            emergency_trigger.as_ref(),
                        );
                    }
                    deferred_service_notices.push_back(notice);
                    continue;
                }
                apply_service_related_action(
                    &mut notice,
                    quit_trigger.as_ref(),
                    help_trigger.as_ref(),
                    screen_draw_trigger.as_ref(),
                    emergency_trigger.as_ref(),
                );
                radial_notices.push(notice)
            }
        }
        if !settings_hotkey_route_is_pending(&pending_launcher_route) {
            for mut notice in deferred_service_notices.drain(..) {
                if deferred_notices_follow_hotkey_failure {
                    notice.action = None;
                    notice.intents.retain(|intent| {
                        matches!(
                            intent,
                            InvocationIntent::CloseRadial { .. }
                                | InvocationIntent::CancelRadialLifecycle { .. }
                        )
                    });
                    if notice.intents.is_empty() && notice.cancellation.is_none() {
                        continue;
                    }
                }
                apply_service_related_action(
                    &mut notice,
                    quit_trigger.as_ref(),
                    help_trigger.as_ref(),
                    screen_draw_trigger.as_ref(),
                    emergency_trigger.as_ref(),
                );
                radial_notices.push(notice);
            }
            deferred_notices_follow_hotkey_failure = false;
        }
        deferred_paste_sequence_actions.extend(take_paste_sequence_trigger_actions(
            &paste_sequence_triggers,
        ));
        if !settings_hotkey_route_is_pending(&pending_launcher_route) {
            for action in deferred_paste_sequence_actions.drain(..) {
                multi_launcher::gui::send_event(
                    multi_launcher::gui::WatchEvent::PasteSequenceHotkey(action),
                );
            }
        }
        let screen_draw_actions = if !settings_hotkey_route_is_pending(&pending_launcher_route)
            || transition_process_controls_are_live(&pending_launcher_route)
        {
            take_screen_draw_trigger_actions(
                &trigger,
                screen_draw_trigger.as_deref(),
                emergency_trigger.as_deref(),
                &screen_draw_recovery_bridge,
                &mut radial_notices,
            )
        } else {
            ScreenDrawTriggerActions::default()
        };
        let exclusive = exclusive_owners() != 0;
        if exclusive != previous_exclusive {
            if !settings_hotkey_route_is_pending(&pending_launcher_route) {
                if let Some(service) = invocation_service.as_ref() {
                    let _ = service.set_exclusive(exclusive);
                }
                previous_exclusive = exclusive;
            }
            if exclusive {
                radial_controller.close(
                    multi_launcher::radial::native::CloseReason::ExclusiveTool,
                    None,
                );
            }
        }
        let screen_draw_cycle_handled = dispatch_screen_draw_trigger_actions(
            screen_draw_actions,
            &screen_draw_recovery_bridge,
            |event| {
                multi_launcher::gui::send_event(event);
                if let Ok(guard) = ctx.lock()
                    && let Some(c) = &*guard
                {
                    c.request_repaint();
                }
            },
        );

        if (!settings_hotkey_route_is_pending(&pending_launcher_route)
            || transition_process_controls_are_live(&pending_launcher_route))
            && let Some(qt) = &quit_trigger
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
            settings_save_workers.begin_shutdown();
            drop(hotkey_runtime_receiver.take());
            if let Ok(guard) = ctx.lock()
                && let Some(c) = &*guard
            {
                c.send_viewport_cmd(egui::ViewportCommand::Close);
                c.request_repaint();
            }
            let _ = gui_worker.join();
            settings_save_workers.join_all();
            break Ok(());
        }

        reconcile_screen_draw_cycle_notices(
            screen_draw_cycle_handled,
            exclusive,
            &mut radial_notices,
        );
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
                    ControllerEvent::ToggleLegacyLauncher { invocation_id } => {
                        let (was_visible, _) = grid_toggle_batch.record_toggle_ordered_with_intent(
                            &visibility_revision,
                            &visibility,
                            Some(invocation_id.0),
                            root_window_bridge.focus_intent_for_launcher_toggle(),
                        );
                        for dismissed_id in radial_controller
                            .handle_legacy_grid_toggle(Some(invocation_id), was_visible)
                        {
                            send_current_route_feedback(
                                invocation_service.as_ref(),
                                &pending_launcher_route,
                                InvocationEvent::RadialSessionClosed { id: dismissed_id },
                            );
                        }
                    }
                    ControllerEvent::ExternalSessionAdmitted { invocation_id } => {
                        let metadata = take_external_radial_metadata(
                            &mut external_radial_invocations,
                            invocation_id,
                        );
                        if let Some((menu_id, interaction, context_token)) = metadata {
                            send_current_route_feedback(
                                invocation_service.as_ref(),
                                &pending_launcher_route,
                                InvocationEvent::ExternalSessionAdmitted {
                                    id: invocation_id,
                                    menu_id,
                                    context_token,
                                    interaction,
                                },
                            );
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
                        send_current_route_feedback(
                            invocation_service.as_ref(),
                            &pending_launcher_route,
                            InvocationEvent::RadialSessionOpened {
                                id: invocation_id,
                                session_id,
                            },
                        );
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
                        let event = if reason
                            == multi_launcher::radial::native::CloseReason::ActionHandoff
                        {
                            multi_launcher::radial::invocation::InvocationEvent::RadialClosedForAction{id:invocation_id}
                        } else {
                            multi_launcher::radial::invocation::InvocationEvent::RadialSessionClosed{id:invocation_id}
                        };
                        send_current_route_feedback(
                            invocation_service.as_ref(),
                            &pending_launcher_route,
                            event,
                        );
                    }
                    ControllerEvent::DispatchRequested(request) => {
                        multi_launcher::radial::acceptance_trace::emit_radial_dispatch_requested(
                            request.identity.invocation_id.0,
                            request.identity.session_generation,
                        );
                        multi_launcher::gui::send_event(
                            multi_launcher::gui::WatchEvent::RadialDispatch(request),
                        );
                    }
                    ControllerEvent::DeferredResolutionRequested(envelope) => {
                        multi_launcher::gui::send_event(
                            multi_launcher::gui::WatchEvent::RadialResolveDeferred(envelope),
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
            deferred_notices_follow_hotkey_failure =
                settings_hotkey_route_is_pending(&pending_launcher_route);
            tracing::error!(
                "launcher invocation lifecycle failed; disabling radial runtime and restoring legacy launcher route"
            );
            if let Some(PendingLauncherRoute {
                completion: PendingLauncherRouteCompletion::Hotkey(application),
                ..
            }) = pending_launcher_route.take()
            {
                let mut runtime = HotkeyRuntimeCommitContext {
                    settings: &mut settings,
                    settings_generation: &mut settings_generation,
                    applied_shared_route: &mut applied_shared_route,
                    trigger: &mut trigger,
                    quit_trigger: &mut quit_trigger,
                    help_trigger: &mut help_trigger,
                    screen_draw_trigger: &mut screen_draw_trigger,
                    emergency_trigger: &mut emergency_trigger,
                    listener: &mut listener,
                    invocation_service: &mut invocation_service,
                    radial_control_endpoint: &radial_control_endpoint,
                    radial_resources: &mut radial_resources,
                    radial_controller: &mut radial_controller,
                    native_preview: &mut native_preview,
                    radial_document: &radial_document,
                    authoring_reply_tx: &authoring_endpoint.reply_tx,
                    event_tx: &event_tx,
                    sequence_routes: &mut paste_sequence_routes,
                    sequence_triggers: &mut paste_sequence_triggers,
                };
                runtime.finish_after_authoritative_hook_failure(*application);
            } else {
                multi_launcher::gui::send_event(
                    multi_launcher::gui::WatchEvent::RadialRuntimeDiagnostic(
                        "The radial launcher input hook stopped unexpectedly. Radial menus were disabled and the legacy launcher hotkey was restored.".into(),
                    ),
                );
                pending_launcher_route = None;
                applied_shared_route = None;
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
                    &paste_sequence_triggers,
                    &event_tx,
                );
                publish_paste_sequence_route_presentation(paste_sequence_route_presentation(
                    &settings,
                    settings_generation,
                    if listener.is_active() {
                        &paste_sequence_routes
                    } else {
                        &[]
                    },
                    &[],
                    active_physical_paste_policy(
                        &settings,
                        &radial_document,
                        &paste_sequence_routes,
                    ),
                ));
            }
        }
        for event in radial_controller.poll() {
            match event {
                ControllerEvent::ExternalSessionAdmitted { invocation_id } => {
                    let metadata = take_external_radial_metadata(
                        &mut external_radial_invocations,
                        invocation_id,
                    );
                    if let Some((menu_id, interaction, context_token)) = metadata {
                        send_current_route_feedback(
                            invocation_service.as_ref(),
                            &pending_launcher_route,
                            InvocationEvent::ExternalSessionAdmitted {
                                id: invocation_id,
                                menu_id,
                                context_token,
                                interaction,
                            },
                        );
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
                    send_current_route_feedback(
                        invocation_service.as_ref(),
                        &pending_launcher_route,
                        InvocationEvent::RadialSessionOpened {
                            id: invocation_id,
                            session_id,
                        },
                    );
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
                    let event = if reason
                        == multi_launcher::radial::native::CloseReason::ActionHandoff
                    {
                        multi_launcher::radial::invocation::InvocationEvent::RadialClosedForAction {
                            id: invocation_id,
                        }
                    } else {
                        multi_launcher::radial::invocation::InvocationEvent::RadialSessionClosed {
                            id: invocation_id,
                        }
                    };
                    send_current_route_feedback(
                        invocation_service.as_ref(),
                        &pending_launcher_route,
                        event,
                    );
                }
                ControllerEvent::ToggleLegacyLauncher { invocation_id } => {
                    let (was_visible, _) = grid_toggle_batch.record_toggle_ordered_with_intent(
                        &visibility_revision,
                        &visibility,
                        Some(invocation_id.0),
                        root_window_bridge.focus_intent_for_launcher_toggle(),
                    );
                    for dismissed_id in radial_controller
                        .handle_legacy_grid_toggle(Some(invocation_id), was_visible)
                    {
                        send_current_route_feedback(
                            invocation_service.as_ref(),
                            &pending_launcher_route,
                            InvocationEvent::RadialSessionClosed { id: dismissed_id },
                        );
                    }
                }
                ControllerEvent::DispatchRequested(request) => {
                    multi_launcher::radial::acceptance_trace::emit_radial_dispatch_requested(
                        request.identity.invocation_id.0,
                        request.identity.session_generation,
                    );
                    multi_launcher::gui::send_event(
                        multi_launcher::gui::WatchEvent::RadialDispatch(request),
                    );
                }
                ControllerEvent::DeferredResolutionRequested(envelope) => {
                    multi_launcher::gui::send_event(
                        multi_launcher::gui::WatchEvent::RadialResolveDeferred(envelope),
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
        if !settings_hotkey_route_is_pending(&pending_launcher_route)
            && let Some(service) = invocation_service.as_ref()
        {
            let _ = service.set_active_menu(radial_controller.active_input_scope());
        }

        if (!settings_hotkey_route_is_pending(&pending_launcher_route)
            || transition_process_controls_are_live(&pending_launcher_route))
            && let Some(ht) = &help_trigger
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

        if pending_launcher_route.is_none() && macro_policy_dirty.swap(false, Ordering::AcqRel) {
            publish_paste_sequence_route_presentation(paste_sequence_route_presentation(
                &settings,
                settings_generation,
                &paste_sequence_routes,
                &[],
                active_physical_paste_policy(&settings, &radial_document, &paste_sequence_routes),
            ));
        }

        publish_hotkey_directory_if_changed(
            &mut last_hotkey_directory_publication,
            &mut hotkey_directory_source_publication_revision,
            &settings,
            settings_generation,
            &radial_document,
            paste_sequence_document_revision.load(Ordering::Acquire),
            &paste_sequence_routes,
            &[],
            macro_directory_revision.load(Ordering::Acquire),
            &listener,
            invocation_service.as_ref(),
            applied_shared_route.as_ref(),
            pending_launcher_route.as_ref(),
        );

        // Consolidate every fail-closed path through the same demand owner.
        // This is idempotent, so repeated disable/error notifications cannot
        // construct or retire a resource set more than once.
        radial_resources.set_runtime_enabled(settings.radial.enabled);
        radial_resources.reconcile(&mut radial_controller);

        let visibility_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let grid_toggles_changed = handle_visibility_toggle_batch_ordered(
                &grid_toggle_batch,
                &visibility_revision,
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
            let legacy_trigger_changed =
                if settings_hotkey_route_is_pending(&pending_launcher_route) {
                    false
                } else {
                    handle_visibility_trigger_with_owner_ordered(
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
                        &visibility_revision,
                        |was_visible| {
                            for dismissed_id in
                                radial_controller.handle_legacy_grid_toggle(None, was_visible)
                            {
                                send_current_route_feedback(
                                    invocation_service.as_ref(),
                                    &pending_launcher_route,
                                    InvocationEvent::RadialSessionClosed { id: dismissed_id },
                                );
                            }
                        },
                    )
                };
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

fn hold_acceptance_prepare_request(
    request: &AuthoringRequest,
    hold_file: Option<&std::path::Path>,
) {
    if !matches!(request, AuthoringRequest::PrepareEmbeddedPreview { .. }) {
        return;
    }
    let Some(hold_file) = hold_file.filter(|path| path.is_file()) else {
        return;
    };

    use multi_launcher::radial::authoring::{
        AcceptancePrepareGateState, trace_acceptance_prepare_gate,
    };

    trace_acceptance_prepare_gate(request, AcceptancePrepareGateState::Held);
    let deadline = Instant::now() + Duration::from_secs(4);
    while hold_file.exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(5));
    }
    trace_acceptance_prepare_gate(
        request,
        if hold_file.exists() {
            AcceptancePrepareGateState::TimedOut
        } else {
            AcceptancePrepareGateState::Released
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use multi_launcher::performance::{Metric, snapshot_metrics};
    use tempfile::tempdir;

    const TRACK_C_STARTUP_WARMUPS: usize = 5;
    const TRACK_C_STARTUP_SAMPLES: usize = 20;

    struct TestHotkeyHandoff {
        completion: std::cell::RefCell<Option<Result<(), String>>>,
    }

    impl HotkeyRouteHandoffReceipt for TestHotkeyHandoff {
        fn try_policy_applied(&self) -> Option<Result<(), String>> {
            None
        }

        fn try_complete(&self) -> Option<Result<(), String>> {
            self.completion.borrow_mut().take()
        }
    }

    struct RecordedHotkeyPolicy {
        config: InvocationConfig,
        related: Vec<RelatedBinding>,
        cancellation: LifecycleCancellation,
    }

    struct TestHotkeyHandoffIssuer {
        results: VecDeque<Result<Result<(), String>, String>>,
        issued: usize,
        policies: Vec<RecordedHotkeyPolicy>,
    }

    impl TestHotkeyHandoffIssuer {
        fn successful() -> Self {
            Self {
                results: VecDeque::from([Ok(Ok(()))]),
                issued: 0,
                policies: Vec::new(),
            }
        }
    }

    impl HotkeyRouteHandoffIssuer for TestHotkeyHandoffIssuer {
        fn service_available(&self, _service: Option<&LauncherInvocationService>) -> bool {
            true
        }
        fn begin(
            &mut self,
            _service: Option<&LauncherInvocationService>,
            config: InvocationConfig,
            related: Vec<RelatedBinding>,
            cancellation: LifecycleCancellation,
        ) -> Result<Box<dyn HotkeyRouteHandoffReceipt>, String> {
            self.issued += 1;
            self.policies.push(RecordedHotkeyPolicy {
                config,
                related,
                cancellation,
            });
            let result = self
                .results
                .pop_front()
                .expect("test handoff result is queued");
            result.map(|completion| {
                Box::new(TestHotkeyHandoff {
                    completion: std::cell::RefCell::new(Some(completion)),
                }) as Box<dyn HotkeyRouteHandoffReceipt>
            })
        }
    }

    struct HotkeyCompletionFixture {
        settings: Settings,
        settings_generation: u64,
        trigger: Arc<HotkeyTrigger>,
        quit_trigger: Option<Arc<HotkeyTrigger>>,
        help_trigger: Option<Arc<HotkeyTrigger>>,
        screen_draw_trigger: Option<Arc<HotkeyTrigger>>,
        emergency_trigger: Option<Arc<HotkeyTrigger>>,
        sequence_routes: Vec<PasteSequenceHotkeyRoute>,
        sequence_triggers: Vec<PasteSequenceHotkeyTrigger>,
        listener: HotkeyListener,
        invocation_service: Option<LauncherInvocationService>,
        applied_shared_route: Option<AppliedSharedRouteEvidence>,
        radial_control_endpoint: RadialControlMainEndpoint,
        radial_resources: RadialRuntimeResources,
        radial_controller: RadialController,
        native_preview: multi_launcher::radial::authoring::native_preview::NativePreviewCoordinator,
        radial_document: Arc<RadialDocument>,
        authoring_endpoint: multi_launcher::radial::authoring::AuthoringMainEndpoint,
        _authoring_client: multi_launcher::radial::authoring::AuthoringClient,
        event_tx: Sender<()>,
        pending_route: Option<PendingLauncherRoute>,
        deferred_notices_follow_failure: bool,
        _workspace: tempfile::TempDir,
    }

    impl HotkeyCompletionFixture {
        fn new(
            settings: Settings,
            trigger: Arc<HotkeyTrigger>,
            listener: HotkeyListener,
            radial_document: Arc<RadialDocument>,
            wake: Sender<()>,
            settings_generation: u64,
        ) -> Self {
            let (control_client, radial_control_endpoint) =
                radial_control_service_with_wake(settings.radial.enabled, None);
            assert_eq!(control_client.is_enabled(), settings.radial.enabled);
            let (controller_wake, _controller_wake_rx) = channel();
            let radial_controller =
                RadialController::new(Arc::clone(&radial_document), false, controller_wake);
            let (authoring_client, authoring_endpoint) = authoring_control_service_with_wake(None);
            let radial_resources = RadialRuntimeResources::new(
                PathBuf::from("unused-test-root"),
                wake.clone(),
                settings.radial.enabled,
            );
            let workspace = tempdir().unwrap();
            let event_tx = wake.clone();
            let native_preview =
                multi_launcher::radial::authoring::native_preview::NativePreviewCoordinator::new(
                    wake,
                    workspace.path().to_path_buf(),
                );
            Self {
                settings,
                settings_generation,
                trigger,
                quit_trigger: None,
                help_trigger: None,
                screen_draw_trigger: None,
                emergency_trigger: None,
                sequence_routes: Vec::new(),
                sequence_triggers: Vec::new(),
                listener,
                invocation_service: None,
                applied_shared_route: None,
                radial_control_endpoint,
                radial_resources,
                radial_controller,
                native_preview,
                radial_document,
                authoring_endpoint,
                _authoring_client: authoring_client,
                event_tx,
                pending_route: None,
                deferred_notices_follow_failure: false,
                _workspace: workspace,
            }
        }

        fn context(&mut self) -> HotkeyRouteCompletionContext<'_> {
            HotkeyRouteCompletionContext {
                runtime: HotkeyRuntimeCommitContext {
                    settings: &mut self.settings,
                    settings_generation: &mut self.settings_generation,
                    applied_shared_route: &mut self.applied_shared_route,
                    trigger: &mut self.trigger,
                    quit_trigger: &mut self.quit_trigger,
                    help_trigger: &mut self.help_trigger,
                    screen_draw_trigger: &mut self.screen_draw_trigger,
                    emergency_trigger: &mut self.emergency_trigger,
                    listener: &mut self.listener,
                    invocation_service: &mut self.invocation_service,
                    radial_control_endpoint: &self.radial_control_endpoint,
                    radial_resources: &mut self.radial_resources,
                    radial_controller: &mut self.radial_controller,
                    native_preview: &mut self.native_preview,
                    radial_document: &self.radial_document,
                    authoring_reply_tx: &self.authoring_endpoint.reply_tx,
                    event_tx: &self.event_tx,
                    sequence_routes: &mut self.sequence_routes,
                    sequence_triggers: &mut self.sequence_triggers,
                },
                pending_route: &mut self.pending_route,
                deferred_notices_follow_failure: &mut self.deferred_notices_follow_failure,
            }
        }
    }

    #[derive(Clone, Copy, Debug)]
    enum TrackCStartupMode {
        Small,
        Full,
    }

    impl TrackCStartupMode {
        fn from_process_env() -> Self {
            match std::env::var("ML_TRACK_C_BENCH_MODE")
                .unwrap_or_else(|_| "full".to_owned())
                .as_str()
            {
                "small" => Self::Small,
                "full" => Self::Full,
                other => panic!("unsupported ML_TRACK_C_BENCH_MODE={other:?}"),
            }
        }

        fn indexed_file_sizes(self) -> Vec<usize> {
            match self {
                Self::Small => vec![16],
                Self::Full => vec![16, 1_000, 10_000],
            }
        }
    }

    /// Small binary-local framed FNV signature. The binary test target cannot
    /// use the library's cfg(test) workload helpers without adding a new API.
    struct TrackCStartupSignature(u64);

    impl TrackCStartupSignature {
        fn new(domain: &str) -> Self {
            let mut signature = Self(0xcbf2_9ce4_8422_2325);
            signature.string(domain);
            signature
        }

        fn raw(&mut self, bytes: &[u8]) {
            for byte in bytes {
                self.0 ^= u64::from(*byte);
                self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
            }
        }

        fn bytes(&mut self, bytes: &[u8]) {
            self.raw(&(bytes.len() as u64).to_le_bytes());
            self.raw(bytes);
        }

        fn string(&mut self, value: &str) {
            self.bytes(value.as_bytes());
        }

        fn number(&mut self, value: u64) {
            self.bytes(&value.to_le_bytes());
        }

        fn finish(self) -> u64 {
            self.0
        }
    }

    struct TrackCStartupFixture {
        root: PathBuf,
        custom_actions: Vec<Action>,
        config: multi_launcher::indexer::coordinator::IndexConfig,
        expected_catalog: Vec<Action>,
        fixture_signature: u64,
        // Declared last so the owned workspace outlives all fixture paths/data.
        _workspace: tempfile::TempDir,
    }

    impl TrackCStartupFixture {
        fn new(indexed_file_count: usize) -> Self {
            let workspace = tempdir().expect("startup fixture owns a temporary workspace");
            let root = workspace.path().join("startup-index-root");
            std::fs::create_dir_all(&root).expect("startup fixture root is created");
            let root = std::fs::canonicalize(root).expect("startup fixture root canonicalizes");
            let root_text = root.to_string_lossy().into_owned();
            let custom_actions = track_c_startup_custom_actions();
            let config = multi_launcher::indexer::coordinator::IndexConfig::new(
                vec![root_text],
                Some(indexed_file_count),
            );

            let mut fixture_signature =
                TrackCStartupSignature::new("multi-launcher-track-c-startup-fixture-v1");
            fixture_signature.number(indexed_file_count as u64);
            fixture_signature.string("$TRACK_C_ROOT");
            fixture_signature.number(1); // exactly one configured root
            fixture_signature.number(1); // max_items is Some
            fixture_signature.number(indexed_file_count as u64);
            fixture_signature.number(custom_actions.len() as u64);
            for action in &custom_actions {
                hash_track_c_startup_action(&mut fixture_signature, action, None);
            }

            for index in 0..indexed_file_count {
                let name = format!("entry-{index:05}.bin");
                let content = format!("Track C startup fixture\nindex={index:05}\n");
                std::fs::write(root.join(&name), content.as_bytes())
                    .expect("startup fixture file is written outside owner timing");
                fixture_signature.string(&name);
                fixture_signature.bytes(content.as_bytes());
            }

            // The tree is deliberately flat. Read its direct entries independently
            // of the coordinator and retain filesystem enumeration order.
            let mut indexed_actions = Vec::with_capacity(indexed_file_count);
            for entry in std::fs::read_dir(&root).expect("startup fixture root is readable") {
                let entry = entry.expect("startup fixture entry is readable");
                if !entry
                    .file_type()
                    .expect("startup fixture entry type is readable")
                    .is_file()
                {
                    continue;
                }
                let canonical = std::fs::canonicalize(entry.path())
                    .expect("startup fixture entry canonicalizes");
                let name = canonical
                    .file_name()
                    .and_then(std::ffi::OsStr::to_str)
                    .expect("fixture filenames are valid Unicode");
                let display = canonical.display().to_string();
                indexed_actions.push(Action {
                    label: name.to_owned(),
                    desc: display.clone(),
                    action: display,
                    args: None,
                });
            }
            assert_eq!(indexed_actions.len(), indexed_file_count);
            let mut expected_catalog = custom_actions.clone();
            expected_catalog.extend(indexed_actions);

            Self {
                root,
                custom_actions,
                config,
                expected_catalog,
                fixture_signature: fixture_signature.finish(),
                _workspace: workspace,
            }
        }

        fn custom_len(&self) -> usize {
            self.custom_actions.len()
        }

        fn indexed_len(&self) -> usize {
            self.expected_catalog.len() - self.custom_len()
        }
    }

    fn track_c_startup_custom_actions() -> Vec<Action> {
        vec![
            Action {
                label: "Track C custom entry".into(),
                desc: "synthetic startup prefix".into(),
                action: "custom:track-c-startup".into(),
                args: None,
            },
            Action {
                label: "Track C empty arguments".into(),
                desc: "preserve Some empty arguments".into(),
                action: "custom:track-c-empty-args".into(),
                args: Some(String::new()),
            },
            Action {
                label: "Track C Unicode λ".into(),
                desc: "synthetic startup Unicode payload".into(),
                action: "custom:track-c-unicode".into(),
                args: Some("--origin=synthetic".into()),
            },
        ]
    }

    fn hash_track_c_startup_action(
        signature: &mut TrackCStartupSignature,
        action: &Action,
        root_prefix: Option<&str>,
    ) {
        for field in [&action.label, &action.desc, &action.action] {
            let field = field.as_str();
            let normalized = root_prefix
                .map(|root| normalize_track_c_owned_root_prefix(field, root))
                .unwrap_or_else(|| std::borrow::Cow::Borrowed(field));
            signature.string(normalized.as_ref());
        }
        match &action.args {
            None => signature.number(0),
            Some(args) => {
                signature.number(1);
                signature.string(args);
            }
        }
    }

    fn normalize_track_c_owned_root_prefix<'a>(
        value: &'a str,
        root: &str,
    ) -> std::borrow::Cow<'a, str> {
        let Some(suffix) = value.strip_prefix(root) else {
            return std::borrow::Cow::Borrowed(value);
        };
        if !suffix.is_empty() && !suffix.starts_with('\\') && !suffix.starts_with('/') {
            return std::borrow::Cow::Borrowed(value);
        }
        std::borrow::Cow::Owned(format!("$TRACK_C_ROOT{suffix}"))
    }

    fn hash_track_c_startup_config(
        signature: &mut TrackCStartupSignature,
        config: &multi_launcher::indexer::coordinator::IndexConfig,
        fixture_root: &str,
        normalize_root: bool,
    ) {
        signature.number(config.roots().len() as u64);
        for root in config.roots() {
            if normalize_root {
                assert_eq!(root, fixture_root, "only the owned root is normalized");
                signature.string("$TRACK_C_ROOT");
            } else {
                signature.string(root);
            }
        }
        match config.max_items() {
            None => signature.number(0),
            Some(max_items) => {
                signature.number(1);
                signature.number(max_items as u64);
            }
        }
    }

    #[derive(Clone, Copy)]
    struct TrackCStartupSample {
        elapsed_nanos: u64,
        complete_output_state_id: u64,
        structural_output_id: u64,
        generation: u64,
        work_units: u64,
        work_counters: [u64; 3],
    }

    #[derive(Clone, Copy)]
    struct TrackCStartupIndexScan {
        calls: u64,
        work_units: u64,
        elapsed_ns_total: u64,
        completed: u64,
        errors: u64,
        abandoned: u64,
    }

    struct TrackCStartupAttributedSample {
        sample: TrackCStartupSample,
        phase_ns: [u64; 7],
        index_scan: TrackCStartupIndexScan,
    }

    fn run_track_c_startup_action_catalog_owner(
        fixture: &TrackCStartupFixture,
    ) -> TrackCStartupSample {
        let custom_actions = fixture.custom_actions.clone();
        let config = fixture.config.clone();
        let started = Instant::now();
        let result = startup_action_catalog(custom_actions, config);
        let elapsed_nanos = started.elapsed().as_nanos().min(u64::MAX as u128) as u64;
        let (catalog, custom_len, transfer) =
            result.expect("owned synthetic startup catalog completes successfully");

        assert_eq!(custom_len, fixture.custom_len(), "custom prefix boundary");
        assert_eq!(catalog.len(), fixture.expected_catalog.len());
        assert_eq!(
            catalog.as_ref().as_slice(),
            fixture.expected_catalog.as_slice(),
            "complete custom-first Actions catalog, including every field and args Option"
        );
        assert_eq!(
            &catalog[custom_len..],
            &fixture.expected_catalog[fixture.custom_len()..],
            "indexed membership and order match the independent flat-tree enumeration"
        );
        for action in &catalog[custom_len..] {
            let path = std::path::Path::new(&action.action);
            assert_eq!(
                action.label.as_str(),
                path.file_name().unwrap().to_string_lossy().as_ref(),
                "indexed label is the complete filename"
            );
            assert_eq!(action.desc, action.action);
            assert_eq!(action.args.as_deref(), None);
            assert!(path.starts_with(&fixture.root));
        }

        let transfer = transfer.expect("nonempty indexed roots transfer the coordinator");
        assert_eq!(transfer.config, fixture.config, "transferred scan config");
        assert_eq!(transfer.generation, 1, "new coordinator first generation");
        transfer
            .coordinator
            .validate_acknowledged_result(transfer.generation, &transfer.config)
            .expect("startup result remains acknowledged for exact transfer config");
        assert!(
            transfer.coordinator.take_result().is_none(),
            "startup scan result was consumed exactly once before transfer"
        );

        let root_text = fixture.root.to_string_lossy();
        let mut complete = TrackCStartupSignature::new("track-c-startup-raw-output-v1");
        complete.number(custom_len as u64);
        complete.number(catalog.len() as u64);
        for action in catalog.iter() {
            hash_track_c_startup_action(&mut complete, action, None);
        }
        hash_track_c_startup_config(&mut complete, &transfer.config, &root_text, false);
        complete.number(transfer.generation);
        complete.number(1); // validate_acknowledged_result succeeded
        complete.number(1); // take_result returned None

        let mut structural = TrackCStartupSignature::new("track-c-startup-structural-output-v1");
        structural.number(custom_len as u64);
        structural.number(catalog.len() as u64);
        for action in catalog.iter() {
            hash_track_c_startup_action(&mut structural, action, Some(&root_text));
        }
        hash_track_c_startup_config(&mut structural, &transfer.config, &root_text, true);
        structural.number(transfer.generation);
        structural.number(1);
        structural.number(1);

        let sample = TrackCStartupSample {
            elapsed_nanos,
            complete_output_state_id: complete.finish(),
            structural_output_id: structural.finish(),
            generation: transfer.generation,
            work_units: catalog.len() as u64,
            work_counters: [
                custom_len as u64,
                catalog.len().saturating_sub(custom_len) as u64,
                catalog.len() as u64,
            ],
        };
        // Drop the acknowledged coordinator before the fixture workspace can
        // leave scope; its worker has completed and no result remains queued.
        drop(transfer);
        sample
    }

    fn run_track_c_startup_action_catalog_attributed_owner(
        fixture: &TrackCStartupFixture,
    ) -> TrackCStartupAttributedSample {
        let before = snapshot_metrics()[Metric::IndexScan as usize];
        let observation = track_c_startup_phase_observer::Observation::begin();
        let sample = run_track_c_startup_action_catalog_owner(fixture);
        let phase_ns = observation.finish();
        let after = snapshot_metrics()[Metric::IndexScan as usize];
        let index_scan = TrackCStartupIndexScan {
            calls: after.calls.saturating_sub(before.calls),
            work_units: after.work_units.saturating_sub(before.work_units),
            elapsed_ns_total: after
                .elapsed_nanos_total
                .saturating_sub(before.elapsed_nanos_total),
            completed: after.completed.saturating_sub(before.completed),
            errors: after.errors.saturating_sub(before.errors),
            abandoned: after.abandoned.saturating_sub(before.abandoned),
        };
        TrackCStartupAttributedSample {
            sample,
            phase_ns,
            index_scan,
        }
    }

    fn emit_track_c_startup_attribution(
        mode: TrackCStartupMode,
        fixture: &TrackCStartupFixture,
        samples: &[TrackCStartupAttributedSample],
    ) {
        assert_eq!(samples.len(), TRACK_C_STARTUP_SAMPLES);
        let phase_names = [
            "coordinator_construct",
            "submit",
            "wait",
            "take_result",
            "validate_transfer",
            "clone_indexed_actions",
            "wrap_catalog_arc",
        ];
        let elapsed_ns = samples
            .iter()
            .map(|sample| sample.sample.elapsed_nanos)
            .collect::<Vec<_>>();
        let phase_ns = samples
            .iter()
            .map(|sample| sample.phase_ns)
            .collect::<Vec<_>>();
        for sample in samples {
            let scan = sample.index_scan;
            assert!(scan.calls > 0);
            assert_eq!(scan.work_units, fixture.indexed_len() as u64);
            assert_eq!(scan.completed, 1);
            assert_eq!(scan.errors, 0);
            assert_eq!(scan.abandoned, 0);
        }
        let index_scan_calls = samples
            .iter()
            .map(|sample| sample.index_scan.calls)
            .collect::<Vec<_>>();
        let index_scan_work_units = samples
            .iter()
            .map(|sample| sample.index_scan.work_units)
            .collect::<Vec<_>>();
        let index_scan_elapsed_ns_total = samples
            .iter()
            .map(|sample| sample.index_scan.elapsed_ns_total)
            .collect::<Vec<_>>();
        let index_scan_completed = samples
            .iter()
            .map(|sample| sample.index_scan.completed)
            .collect::<Vec<_>>();
        let index_scan_errors = samples
            .iter()
            .map(|sample| sample.index_scan.errors)
            .collect::<Vec<_>>();
        let index_scan_abandoned = samples
            .iter()
            .map(|sample| sample.index_scan.abandoned)
            .collect::<Vec<_>>();
        let source = std::env::var("ML_TRACK_C_SOURCE_SHA")
            .unwrap_or_else(|_| "UNCOMMITTED_SOURCE".to_owned());
        let profile = std::env::var("ML_TRACK_C_PROFILE")
            .unwrap_or_else(|_| "unspecified-profile".to_owned());
        let mode = match mode {
            TrackCStartupMode::Small => "Small",
            TrackCStartupMode::Full => "Full",
        };
        let record = serde_json::json!({
            "schema": 1,
            "stage": "startup_action_catalog_phase_attribution",
            "owner": "startup_action_catalog",
            "fixture": "owned_synthetic_index_tree",
            "source": source,
            "profile": profile,
            "mode": mode,
            "telemetry_enabled": multi_launcher::performance::enabled(),
            "fixture_signature": format!("{:016x}", fixture.fixture_signature),
            "count": fixture.indexed_len(),
            "warmups": TRACK_C_STARTUP_WARMUPS,
            "samples": TRACK_C_STARTUP_SAMPLES,
            "exclusions": 0,
            "phase_names": phase_names,
            "elapsed_ns": elapsed_ns,
            "phase_ns": phase_ns,
            "index_scan_calls": index_scan_calls,
            "index_scan_work_units": index_scan_work_units,
            "index_scan_elapsed_ns_total": index_scan_elapsed_ns_total,
            "index_scan_completed": index_scan_completed,
            "index_scan_errors": index_scan_errors,
            "index_scan_abandoned": index_scan_abandoned,
            "index_scan_scope": "sum_of_index_batch_iter_next_step",
            "index_scan_timing_relation": "overlaps_wait",
            "latency_claim": "not_process_startup",
        });
        eprintln!("TRACK_C3A_STARTUP_RESULT {record}");
    }

    fn track_c_startup_nearest_rank(samples: &[u64], percentile: usize) -> u64 {
        assert!(!samples.is_empty());
        assert!((1..=100).contains(&percentile));
        let mut ordered = samples.to_vec();
        ordered.sort_unstable();
        let rank = percentile.saturating_mul(ordered.len()).saturating_add(99) / 100;
        ordered[rank.saturating_sub(1)]
    }

    fn emit_track_c_startup_samples(
        mode: TrackCStartupMode,
        fixture: &TrackCStartupFixture,
        samples: &[TrackCStartupSample],
    ) {
        assert_eq!(samples.len(), TRACK_C_STARTUP_SAMPLES);
        assert!(samples.iter().all(|sample| {
            sample.structural_output_id == samples[0].structural_output_id
                && sample.complete_output_state_id == samples[0].complete_output_state_id
                && sample.generation == 1
                && sample.work_units == samples[0].work_units
                && sample.work_counters == samples[0].work_counters
        }));
        let elapsed = samples
            .iter()
            .map(|sample| sample.elapsed_nanos)
            .collect::<Vec<_>>();
        let complete = samples
            .iter()
            .map(|sample| format!("{:016x}", sample.complete_output_state_id))
            .collect::<Vec<_>>();
        let structural = samples
            .iter()
            .map(|sample| format!("{:016x}", sample.structural_output_id))
            .collect::<Vec<_>>();
        let generations = samples
            .iter()
            .map(|sample| sample.generation.to_string())
            .collect::<Vec<_>>();
        let work_units = samples
            .iter()
            .map(|sample| sample.work_units.to_string())
            .collect::<Vec<_>>();
        let work_counters = samples
            .iter()
            .map(|sample| {
                sample
                    .work_counters
                    .iter()
                    .map(u64::to_string)
                    .collect::<Vec<_>>()
                    .join(":")
            })
            .collect::<Vec<_>>();
        let source = std::env::var("ML_TRACK_C_SOURCE_SHA")
            .unwrap_or_else(|_| "UNCOMMITTED_SOURCE".to_owned());
        let profile = std::env::var("ML_TRACK_C_PROFILE")
            .unwrap_or_else(|_| "unspecified-profile".to_owned());
        eprintln!(
            "TRACK_C_RESULT schema=1 owner=startup_action_catalog source={source} profile={profile} mode={mode:?} telemetry_enabled={} fixture=owned_synthetic_index_tree fixture_signature={:016x} count={} viewport=none scale_milli=1000 fonts=not_applicable settings=custom_first_initial_catalog cold_type=new_coordinator_warm_os_cache warmups={} samples={} elapsed_ns=[{}] p50_ns={} p95_ns={} max_ns={} complete_output_state_ids=[{}] structural_output_ids=[{}] revision_receipts=[{}] viewport_receipts=[{}] work_units=[{}] work_counters=[{}] exclusions=0",
            multi_launcher::performance::enabled(),
            fixture.fixture_signature,
            fixture.indexed_len(),
            TRACK_C_STARTUP_WARMUPS,
            samples.len(),
            elapsed
                .iter()
                .map(u64::to_string)
                .collect::<Vec<_>>()
                .join(","),
            track_c_startup_nearest_rank(&elapsed, 50),
            track_c_startup_nearest_rank(&elapsed, 95),
            elapsed.iter().copied().max().unwrap_or_default(),
            complete.join(","),
            structural.join(","),
            generations.join(","),
            vec!["none"; samples.len()].join(","),
            work_units.join(","),
            work_counters.join("|"),
        );
    }

    #[test]
    fn startup_index_returns_the_complete_catalog_and_acknowledges_before_transfer() {
        let directory = tempdir().unwrap();
        for name in ["one.txt", "two.txt", "three.txt"] {
            std::fs::write(directory.path().join(name), name).unwrap();
        }
        let config = multi_launcher::indexer::coordinator::IndexConfig::new(
            vec![directory.path().to_string_lossy().into_owned()],
            Some(2),
        );

        let custom = Action {
            label: "custom entry".into(),
            desc: "custom entry description".into(),
            action: "custom:entry".into(),
            args: None,
        };
        let (catalog, custom_len, transfer) =
            startup_action_catalog(vec![custom.clone()], config.clone()).unwrap();
        assert_eq!(custom_len, 1);
        assert_eq!(catalog[0], custom);
        let indexed = &catalog[custom_len..];
        assert_eq!(indexed.len(), 2);
        assert!(indexed.iter().all(|action| {
            action.args.is_none()
                && action.label
                    == std::path::Path::new(&action.action)
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                && action.desc == action.action
        }));
        let transfer = transfer.expect("nonempty roots retain the coordinator");
        assert_eq!(transfer.config, config);
        assert_eq!(transfer.generation, 1);
        transfer
            .coordinator
            .validate_acknowledged_result(transfer.generation, &transfer.config)
            .unwrap();
        assert!(matches!(
            transfer
                .coordinator
                .validate_acknowledged_result(transfer.generation + 1, &transfer.config),
            Err(multi_launcher::indexer::coordinator::CoordinatorError::Superseded { .. })
        ));
        assert!(
            transfer.coordinator.take_result().is_none(),
            "startup result is acknowledged exactly once before transfer"
        );
    }

    #[test]
    fn startup_index_skips_empty_roots_and_propagates_scan_failures() {
        let (empty, transfer) = startup_indexed_actions(
            multi_launcher::indexer::coordinator::IndexConfig::new(Vec::new(), Some(25)),
        )
        .unwrap();
        assert!(empty.is_empty());
        assert!(transfer.is_none(), "empty startup roots create no worker");

        let custom = Action {
            label: "kept custom".into(),
            desc: String::new(),
            action: "custom:kept".into(),
            args: None,
        };
        let (catalog, custom_len, transfer) = startup_action_catalog(
            vec![custom.clone()],
            multi_launcher::indexer::coordinator::IndexConfig::new(Vec::new(), Some(25)),
        )
        .unwrap();
        assert_eq!(custom_len, 1);
        assert_eq!(catalog.as_slice(), &[custom]);
        assert!(transfer.is_none());

        let directory = tempdir().unwrap();
        let missing = directory.path().join("missing");
        let result =
            startup_indexed_actions(multi_launcher::indexer::coordinator::IndexConfig::new(
                vec![missing.to_string_lossy().into_owned()],
                None,
            ));
        let error = match result {
            Err(error) => error,
            Ok(_) => panic!("a missing startup root must propagate its scan failure"),
        };
        assert!(error.to_string().contains("startup index scan failed"));
    }

    #[test]
    fn track_c_oracle_startup_action_catalog_matches_complete_acknowledged_output() {
        let fixture = TrackCStartupFixture::new(16);
        let sample = run_track_c_startup_action_catalog_owner(&fixture);

        assert_eq!(sample.generation, 1);
        assert_eq!(
            sample.work_units,
            (fixture.custom_len() + fixture.indexed_len()) as u64
        );
        assert_eq!(
            sample.work_counters,
            [
                fixture.custom_len() as u64,
                fixture.indexed_len() as u64,
                fixture.expected_catalog.len() as u64,
            ]
        );
    }

    #[test]
    #[ignore = "opt-in Track C benchmark; use an owned isolated test process and small mode for smoke"]
    fn track_c_benchmark_startup_action_catalog_owner() {
        assert!(multi_launcher::performance::enabled());
        let mode = TrackCStartupMode::from_process_env();

        for indexed_file_count in mode.indexed_file_sizes() {
            let fixture = TrackCStartupFixture::new(indexed_file_count);
            for _ in 0..TRACK_C_STARTUP_WARMUPS {
                let _ = run_track_c_startup_action_catalog_attributed_owner(&fixture);
            }

            let samples = (0..TRACK_C_STARTUP_SAMPLES)
                .map(|_| run_track_c_startup_action_catalog_attributed_owner(&fixture))
                .collect::<Vec<_>>();
            emit_track_c_startup_samples(
                mode,
                &fixture,
                &samples
                    .iter()
                    .map(|sample| sample.sample)
                    .collect::<Vec<_>>(),
            );
            emit_track_c_startup_attribution(mode, &fixture, &samples);
        }
    }

    #[test]
    fn gui_worker_completion_wake_shuts_down_before_worker_tail_returns() {
        let (wake, events) = channel();
        let keep_channel_connected = wake.clone();
        let (tail_entered, tail_entered_rx) = channel();
        let (release_tail, release_tail_rx) = channel();
        let worker = GuiWorker::spawn(wake, move |completion| {
            completion.complete();
            tail_entered.send(()).unwrap();
            let _ = release_tail_rx.recv_timeout(Duration::from_secs(5));
        });
        tail_entered_rx
            .recv_timeout(Duration::from_secs(5))
            .unwrap();

        assert!(!worker.handle.is_finished());
        assert_eq!(worker.wait_for_event(&events), GuiWorkerEvent::Completed);
        assert!(!worker.handle.is_finished(), "shutdown must precede join");
        assert_eq!(events.try_recv(), Err(std::sync::mpsc::TryRecvError::Empty));

        release_tail.send(()).unwrap();
        worker.join().unwrap();
        assert_eq!(events.try_recv(), Err(std::sync::mpsc::TryRecvError::Empty));
        drop(keep_channel_connected);
    }

    #[test]
    fn gui_worker_prequeued_wake_observes_already_published_completion() {
        let (wake, events) = channel();
        wake.send(()).unwrap();
        let keep_channel_connected = wake.clone();
        let (tail_entered, tail_entered_rx) = channel();
        let (release_tail, release_tail_rx) = channel();
        let worker = GuiWorker::spawn(wake, move |completion| {
            completion.complete();
            tail_entered.send(()).unwrap();
            let _ = release_tail_rx.recv_timeout(Duration::from_secs(5));
        });
        tail_entered_rx
            .recv_timeout(Duration::from_secs(5))
            .unwrap();

        assert!(!worker.handle.is_finished());
        assert_eq!(worker.wait_for_event(&events), GuiWorkerEvent::Completed);
        assert_eq!(
            events.try_recv(),
            Ok(()),
            "only the completion wake remains"
        );
        assert_eq!(events.try_recv(), Err(std::sync::mpsc::TryRecvError::Empty));

        release_tail.send(()).unwrap();
        worker.join().unwrap();
        drop(keep_channel_connected);
    }

    #[test]
    fn gui_worker_ordinary_wake_keeps_running_until_gui_returns() {
        let (wake, events) = channel();
        let ordinary_wake = wake.clone();
        let (work_entered, work_entered_rx) = channel();
        let (finish_gui, finish_gui_rx) = channel();
        let worker = GuiWorker::spawn(wake, move |_completion| {
            work_entered.send(()).unwrap();
            finish_gui_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        });
        work_entered_rx
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        ordinary_wake.send(()).unwrap();

        assert_eq!(worker.wait_for_event(&events), GuiWorkerEvent::Wake);
        assert!(!worker.completed.load(Ordering::Acquire));
        assert!(!worker.handle.is_finished());
        assert_eq!(events.try_recv(), Err(std::sync::mpsc::TryRecvError::Empty));

        finish_gui.send(()).unwrap();
        assert_eq!(worker.wait_for_event(&events), GuiWorkerEvent::Completed);
        worker.join().unwrap();
        assert_eq!(events.try_recv(), Err(std::sync::mpsc::TryRecvError::Empty));
    }

    #[test]
    fn gui_worker_gui_error_return_publishes_completion() {
        let (wake, events) = channel();
        let (gui_result, gui_result_rx) = channel();
        let worker = GuiWorker::spawn(wake, move |completion| {
            let result: Result<(), &str> = Err("GUI startup failed");
            gui_result.send(result).unwrap();
            completion.complete();
        });

        assert_eq!(
            gui_result_rx.recv_timeout(Duration::from_secs(5)).unwrap(),
            Err("GUI startup failed")
        );
        assert_eq!(worker.wait_for_event(&events), GuiWorkerEvent::Completed);
        worker.join().unwrap();
        assert_eq!(
            events.try_recv(),
            Err(std::sync::mpsc::TryRecvError::Disconnected)
        );
    }

    #[test]
    fn gui_worker_gui_panic_publishes_completion_and_retains_join_error() {
        let (wake, events) = channel();
        let worker = GuiWorker::spawn(wake, move |_completion| {
            panic!("GUI worker test panic");
        });

        assert_eq!(worker.wait_for_event(&events), GuiWorkerEvent::Completed);
        assert!(worker.join().is_err());
        assert_eq!(
            events.try_recv(),
            Err(std::sync::mpsc::TryRecvError::Disconnected)
        );
    }

    #[test]
    fn gui_worker_channel_close_retains_shutdown_before_join() {
        let (wake, events) = channel();
        let (tail_entered, tail_entered_rx) = channel();
        let (release_tail, release_tail_rx) = channel();
        let worker = GuiWorker::spawn(wake, move |completion| {
            completion.complete();
            tail_entered.send(()).unwrap();
            let _ = release_tail_rx.recv_timeout(Duration::from_secs(5));
        });
        tail_entered_rx
            .recv_timeout(Duration::from_secs(5))
            .unwrap();

        assert_eq!(events.try_recv(), Ok(()));
        assert!(!worker.handle.is_finished());
        assert_eq!(
            worker.wait_for_event(&events),
            GuiWorkerEvent::ChannelClosed(RecvError)
        );
        assert!(!worker.handle.is_finished());

        release_tail.send(()).unwrap();
        worker.join().unwrap();
    }

    #[test]
    fn gui_worker_quit_path_joins_after_requested_gui_close() {
        let (wake, events) = channel();
        let (close_gui, close_gui_rx) = channel();
        let closed = Arc::new(AtomicBool::new(false));
        let closed_by_gui = Arc::clone(&closed);
        let worker = GuiWorker::spawn(wake, move |_completion| {
            close_gui_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            closed_by_gui.store(true, Ordering::Release);
        });

        assert!(!closed.load(Ordering::Acquire));
        close_gui.send(()).unwrap();
        worker.join().unwrap();
        assert!(closed.load(Ordering::Acquire));
        assert_eq!(events.try_recv(), Ok(()));
        assert_eq!(
            events.try_recv(),
            Err(std::sync::mpsc::TryRecvError::Disconnected)
        );
    }

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
        assert!(!enabled_plan.start_legacy);
        assert!(!enabled_plan.stop_service);
        assert!(enabled_plan.config.launcher_enabled);
        assert_eq!(enabled_plan.config.generation, 41);

        let mut disabled = enabled;
        disabled.radial.enabled = false;
        let disabled_plan = radial_route_plan(&disabled, &document, 42);
        assert!(disabled_plan.start_legacy);
        assert!(
            !disabled_plan.stop_service,
            "paste sessions retain the single observer"
        );
        assert!(!disabled_plan.config.launcher_enabled);
        assert!(disabled_plan.config.item_inputs.is_empty());
        assert!(disabled_plan.related.is_empty());
        assert_eq!(disabled_plan.config.generation, 42);
        disabled.enabled_plugins = Some(Default::default());
        assert!(radial_route_plan(&disabled, &document, 43).stop_service);
        assert!(!invocation_service_needed(&disabled));
    }

    #[test]
    fn physical_paste_service_policy_does_not_duplicate_configured_routes() {
        let mut settings = Settings::default();
        settings.radial.enabled = false;
        let document = RadialDocument::starter();
        let routes = paste_sequence_hotkey_routes_from_bindings(&settings, &[]).unwrap();
        let plan = radial_route_plan_with_sequences(&settings, &document, 77, &routes);
        assert!(invocation_service_needed(&settings));
        assert!(plan.start_legacy);
        assert!(!plan.stop_service);
        assert!(!plan.config.launcher_enabled);
        assert!(
            plan.related.is_empty(),
            "legacy listener owns Next/Cancel/Start while radial is off"
        );
        assert!(plan.config.item_inputs.is_empty());
        settings.enabled_plugins = Some(Default::default());
        assert!(!invocation_service_needed(&settings));
        settings.radial.enabled = true;
        assert!(invocation_service_needed(&settings));
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
            &[],
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
        let config = radial_invocation_config(&settings, &document, true, 9, &[]);
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

    fn paste_route_test_snapshot(chord: &str) -> multi_launcher::paste_sequence::SequenceSnapshot {
        use multi_launcher::paste_sequence::{
            HotkeyChord, PlaybackMode, SequenceDraft, SequenceStore, Step,
        };
        let directory = tempdir().unwrap();
        let mut snapshot = SequenceStore::new(directory.path().join("sequences.json"))
            .load()
            .unwrap();
        snapshot
            .upsert(SequenceDraft {
                id: None,
                name: "Start fixture".into(),
                hotkey: Some(HotkeyChord::new(chord).unwrap()),
                mode: PlaybackMode::Automatic,
                delay_ms: 0,
                steps: vec![Step::LiteralText {
                    text: "fixture".into(),
                }],
            })
            .unwrap();
        snapshot
    }

    #[test]
    fn paste_route_presentation_never_claims_unsupported_fallback_keys_are_active() {
        use multi_launcher::paste_sequence::SequenceShortcutPresentation as Presentation;
        let mut settings = Settings::default();
        settings.paste_sequence.next_hotkey = Some("F13".into());
        let snapshot = paste_route_test_snapshot("F14");
        let routes = paste_sequence_hotkey_routes_from_snapshot(&settings, &snapshot).unwrap();
        let shared = paste_sequence_route_presentation(
            &settings,
            7,
            &routes,
            &[],
            PhysicalPastePolicy::default(),
        );
        assert_eq!(shared.next, Presentation::Active("F13".into()));
        assert_eq!(shared.starts.len(), 1);
        let failed_over = paste_sequence_presentation_for_polling(shared);
        assert!(
            matches!(failed_over.next, Presentation::Inactive { configured, reason } if configured == "F13" && reason.contains("polling listener"))
        );
        assert!(failed_over.starts.is_empty());
        settings.radial.enabled = false;
        let fallback = paste_sequence_route_presentation(
            &settings,
            7,
            &routes,
            &[],
            PhysicalPastePolicy::default(),
        );
        assert!(matches!(fallback.next, Presentation::Inactive { .. }));
        assert!(fallback.starts.is_empty());
        let snapshot = multi_launcher::paste_sequence::SequenceRoutePresentation {
            starts: (0..100)
                .map(|_| (snapshot.sequences()[0].id, "Ctrl+F8".into()))
                .collect(),
            ..fallback
        };
        assert!(snapshot.summary().contains("100 active"));
        assert!(
            snapshot.summary().lines().count() <= 5,
            "hover/settings summary stays bounded"
        );
    }

    #[test]
    fn paste_route_presentation_tracks_omissions_reload_disable_and_clean_reenable() {
        use multi_launcher::paste_sequence::SequenceShortcutPresentation as Presentation;
        let mut settings = Settings {
            hotkey: Some("Shift+Alt+Win+End".into()),
            ..Settings::default()
        };
        let document = RadialDocument::starter();
        let snapshot = paste_route_test_snapshot("Ctrl+F8");
        let bindings = snapshot.hotkey_bindings();
        let resolve = |settings: &Settings| {
            resolve_hotkey_route_candidate_with_macro(settings, &document, None, Some(&bindings))
                .unwrap()
        };
        let initial = resolve(&settings);
        let displayed = paste_sequence_route_presentation(
            &settings,
            1,
            &initial.sequences,
            &initial.omissions,
            initial.physical_paste.clone(),
        );
        assert!(!displayed.keep_dialog_visible_during_playback);
        assert!(matches!(displayed.next, Presentation::Active(_)));
        assert!(
            matches!(&displayed.cancel, Presentation::Inactive { configured, reason } if configured == "Ctrl+Alt+Shift+End" && reason.contains("launcher"))
        );
        assert_eq!(displayed.starts, vec![(bindings[0].id(), "Ctrl+F8".into())]);
        assert!(displayed.summary().contains("inactive"));
        // Reload with a nonconflicting configured control clears the old reason.
        settings.paste_sequence.cancel_hotkey = Some("Ctrl+F10".into());
        settings.paste_sequence.next_hotkey = None;
        settings.paste_sequence.escape_cancel_enabled = false;
        settings.paste_sequence.keep_dialog_visible_during_playback = true;
        let reloaded = resolve(&settings);
        let displayed = paste_sequence_route_presentation(
            &settings,
            2,
            &reloaded.sequences,
            &reloaded.omissions,
            reloaded.physical_paste.clone(),
        );
        assert_eq!(displayed.next, Presentation::Disabled);
        assert_eq!(displayed.cancel, Presentation::Active("Ctrl+F10".into()));
        assert!(!displayed.summary().contains("inactive"));
        assert!(!displayed.escape_cancel_enabled);
        assert!(displayed.keep_dialog_visible_during_playback);
        settings.enabled_plugins = Some(std::collections::HashSet::new());
        let disabled = resolve(&settings);
        let displayed = paste_sequence_route_presentation(
            &settings,
            3,
            &disabled.sequences,
            &disabled.omissions,
            disabled.physical_paste.clone(),
        );
        assert!(displayed.starts.is_empty());
        assert!(
            matches!(displayed.cancel, Presentation::Inactive { reason, .. } if reason.contains("plugin is disabled"))
        );
        settings
            .enabled_plugins
            .as_mut()
            .unwrap()
            .insert("paste_sequence".into());
        let enabled = resolve(&settings);
        let displayed = paste_sequence_route_presentation(
            &settings,
            4,
            &enabled.sequences,
            &enabled.omissions,
            enabled.physical_paste.clone(),
        );
        assert_eq!(displayed.cancel, Presentation::Active("Ctrl+F10".into()));
        assert_eq!(displayed.starts.len(), 1);
        assert!(!displayed.summary().contains("inactive"));
        assert_eq!(settings.hotkey.as_deref(), Some("Shift+Alt+Win+End"));
    }

    #[test]
    fn physical_paste_policy_preserves_configured_v_routes_and_ignores_context_only_radial() {
        use multi_launcher::radial::model::{TriggerDefinition, TriggerId, TriggerScope};
        let mut settings = Settings::default();
        settings.radial.enabled = false;
        settings.hotkey = Some("Ctrl+V".into());
        let mut document = RadialDocument::starter();
        let candidate =
            resolve_hotkey_route_candidate_with_macro(&settings, &document, None, Some(&[]))
                .unwrap();
        assert!(
            candidate
                .physical_paste
                .availability
                .reason()
                .unwrap()
                .contains("launcher toggle")
        );
        assert_eq!(settings.hotkey.as_deref(), Some("Ctrl+V"));
        settings.hotkey = Some("F2".into());
        settings.paste_sequence.cancel_hotkey = Some("V".into());
        let candidate =
            resolve_hotkey_route_candidate_with_macro(&settings, &document, None, Some(&[]))
                .unwrap();
        assert!(
            candidate
                .physical_paste
                .availability
                .reason()
                .unwrap()
                .contains("Cancel")
        );
        assert!(
            candidate
                .sequences
                .iter()
                .any(|route| route.action == PasteSequenceHotkeyAction::Cancel)
        );
        settings.paste_sequence.cancel_hotkey = None;
        let macros: multi_launcher::mkmacro::MkMacroDocument = serde_json::from_value(serde_json::json!({"macros": [{"id": 73, "name": "V owner", "enabled": true, "hotkey": {"key": {"character":"V"}, "modifiers": ["control"]}}]})).unwrap();
        assert!(
            physical_paste_availability(&settings, &document, Some(&macros), &[])
                .reason()
                .unwrap()
                .contains("macro 73")
        );
        let mut disabled = macros.clone();
        disabled.macros[0].enabled = false;
        assert_eq!(
            physical_paste_availability(&settings, &document, Some(&disabled), &[]),
            PhysicalPasteAvailability::Available
        );
        disabled.settings.recorder.marker_hotkey = macros.macros[0].hotkey.clone();
        assert!(
            physical_paste_availability(&settings, &document, Some(&disabled), &[])
                .reason()
                .unwrap()
                .contains("recorder marker")
        );
        document.custom_triggers.push(TriggerDefinition {
            id: TriggerId::new("v-menu"),
            chord: "Ctrl+V".into(),
            menu_id: document.default_menu_id.clone(),
            scope: TriggerScope::Global,
        });
        assert_eq!(
            physical_paste_availability(&settings, &document, None, &[]),
            PhysicalPasteAvailability::Available,
            "disabled radial cannot reserve a live route"
        );
        settings.radial.enabled = true;
        assert!(
            physical_paste_availability(&settings, &document, None, &[])
                .reason()
                .unwrap()
                .contains("v-menu")
        );
        document.custom_triggers[0].scope = TriggerScope::MenuLocal;
        assert_eq!(
            physical_paste_availability(&settings, &document, None, &[]),
            PhysicalPasteAvailability::Available,
            "menu/context-only selection has no fresh global V route"
        );
        assert!(
            !reserved_launcher_hotkeys_with_sequences(&settings, &[])
                .iter()
                .any(|(_, chord)| chord == "Ctrl+V"),
            "policy never installs a permanent reservation"
        );
    }

    #[test]
    fn paste_routes_default_cancel_loses_to_launcher_without_losing_next_or_start() {
        let settings = Settings {
            hotkey: Some("Shift+Alt+Win+End".into()),
            ..Settings::default()
        };
        let before = settings.clone();
        let document = RadialDocument::starter();
        let snapshot = paste_route_test_snapshot("Ctrl+F8");
        let resolved = resolve_hotkey_route_candidate_with_macro(
            &settings,
            &document,
            None,
            Some(&snapshot.hotkey_bindings()),
        )
        .unwrap();
        assert_eq!(resolved.sequences.len(), 2);
        assert!(
            resolved
                .sequences
                .iter()
                .any(|route| route.action == PasteSequenceHotkeyAction::Next)
        );
        assert!(
            resolved
                .sequences
                .iter()
                .any(|route| matches!(route.action, PasteSequenceHotkeyAction::Start(_)))
        );
        assert!(
            !resolved
                .sequences
                .iter()
                .any(|route| route.action == PasteSequenceHotkeyAction::Cancel)
        );
        assert_eq!(resolved.omissions.len(), 1);
        assert!(matches!(
            resolved.omissions[0].action,
            PasteSequenceHotkeyAction::Cancel
        ));
        assert!(resolved.omissions[0].reason.contains("launcher"));
        assert!(
            resolved.omissions[0]
                .reason
                .contains("Settings > Paste Sequences")
        );
        assert_eq!(settings.hotkey, before.hotkey);
        assert_eq!(settings.paste_sequence, before.paste_sequence);
        let reserved = reserved_launcher_hotkeys_with_sequences(&settings, &resolved.sequences);
        assert!(
            !reserved
                .iter()
                .any(|(name, _)| name == "Paste Sequence Cancel")
        );
        assert!(
            reserved
                .iter()
                .any(|(name, _)| name == "Paste Sequence Next")
        );
        let triggers = sequence_hotkey_triggers_from_routes(&resolved.sequences);
        assert_eq!(triggers.len(), 2);
        let plan = radial_route_plan_with_sequences(&settings, &document, 11, &resolved.sequences);
        assert_eq!(
            plan.related
                .iter()
                .filter(|binding| matches!(binding.action, RelatedAction::PasteSequence(_)))
                .count(),
            2
        );
    }

    #[test]
    fn paste_routes_defaults_lose_to_saved_starts_radial_and_macro_routes() {
        use multi_launcher::radial::model::{TriggerDefinition, TriggerId, TriggerScope};
        let settings = Settings::default();
        let mut document = RadialDocument::starter();
        let snapshot = paste_route_test_snapshot("Alt+PageDown");
        let resolved = resolve_hotkey_route_candidate_with_macro(
            &settings,
            &document,
            None,
            Some(&snapshot.hotkey_bindings()),
        )
        .unwrap();
        assert!(
            resolved
                .sequences
                .iter()
                .any(|route| matches!(route.action, PasteSequenceHotkeyAction::Start(_)))
        );
        assert!(
            !resolved
                .sequences
                .iter()
                .any(|route| route.action == PasteSequenceHotkeyAction::Next)
        );
        assert!(resolved.omissions[0].reason.contains("one another"));
        document.custom_triggers.push(TriggerDefinition {
            id: TriggerId::new("end-route"),
            chord: "Alt+End".into(),
            menu_id: document.default_menu_id.clone(),
            scope: TriggerScope::Global,
        });
        let resolved =
            resolve_hotkey_route_candidate_with_macro(&settings, &document, None, Some(&[]))
                .unwrap();
        assert_eq!(resolved.sequences.len(), 1);
        assert_eq!(
            resolved.sequences[0].action,
            PasteSequenceHotkeyAction::Next
        );
        assert!(resolved.omissions[0].reason.contains("end-route"));
        assert!(
            !reserved_launcher_hotkeys_with_sequences(&settings, &resolved.sequences)
                .iter()
                .any(|(name, _)| name == "Paste Sequence Cancel")
        );
        // An omitted default must not filter the valid radial binding from its
        // own compiled item-input view either.
        document.custom_triggers.clear();
        let macros: multi_launcher::mkmacro::MkMacroDocument = serde_json::from_value(serde_json::json!({"macros": [{"id": 73, "name": "Next-key owner", "hotkey": {"key": "page_down", "modifiers": []}}]})).unwrap();
        let resolved = resolve_hotkey_route_candidate_with_macro(
            &settings,
            &document,
            Some(&macros),
            Some(&[]),
        )
        .unwrap();
        assert_eq!(resolved.sequences.len(), 1);
        assert_eq!(
            resolved.sequences[0].action,
            PasteSequenceHotkeyAction::Cancel
        );
        assert!(resolved.omissions[0].reason.contains("macro 73"));
        assert!(
            !reserved_launcher_hotkeys_with_sequences(&settings, &resolved.sequences)
                .iter()
                .any(|(name, _)| name == "Paste Sequence Next")
        );
    }

    #[test]
    fn paste_routes_custom_conflicts_reject_and_disabled_preferences_reserve_nothing() {
        let mut settings = Settings::default();
        let document = RadialDocument::starter();
        settings.paste_sequence.next_hotkey = Some("Alt+F2".into());
        let error =
            resolve_hotkey_route_candidate_with_macro(&settings, &document, None, Some(&[]))
                .err()
                .unwrap();
        assert!(error.to_string().contains("launcher"));
        settings.paste_sequence.next_hotkey = Some("not a shortcut".into());
        assert!(
            resolve_hotkey_route_candidate_with_macro(&settings, &document, None, Some(&[]))
                .is_err()
        );
        settings.paste_sequence.next_hotkey = Some("  ".into());
        settings.paste_sequence.cancel_hotkey = None;
        let resolved =
            resolve_hotkey_route_candidate_with_macro(&settings, &document, None, Some(&[]))
                .unwrap();
        assert!(resolved.sequences.is_empty());
        assert!(resolved.omissions.is_empty());
        assert!(
            !reserved_launcher_hotkeys_with_sequences(&settings, &resolved.sequences)
                .iter()
                .any(|(name, _)| name.starts_with("Paste Sequence"))
        );
    }

    #[test]
    fn paste_routes_prepare_omits_defaults_and_rejects_custom_without_replacing_listener() {
        use multi_launcher::hotkey::reconfiguration::{
            HotkeyRuntimeMessage, prepare_hotkey_reconfiguration,
        };
        let (wake_tx, _wake_rx) = channel();
        let receiver =
            multi_launcher::hotkey::reconfiguration::install_main_runtime(wake_tx.clone()).unwrap();
        let mut settings = Settings {
            hotkey: Some("Shift+Alt+Win+End".into()),
            ..Settings::default()
        };
        settings.radial.enabled = false;
        let document = RadialDocument::starter();
        let existing = Arc::new(HotkeyTrigger::new(settings.hotkey()));
        let mut listener = HotkeyTrigger::start_listener(
            vec![existing.clone()],
            "paste-route-test",
            wake_tx.clone(),
        );
        let mut prepared_routes = HashMap::new();
        let mut ticket = prepare_hotkey_reconfiguration(settings.clone()).unwrap();
        let HotkeyRuntimeMessage::Prepare(request) = receiver.try_recv().unwrap().unwrap() else {
            panic!("expected prepare")
        };
        prepare_hotkey_runtime_message(
            request,
            &document,
            1,
            &wake_tx,
            false,
            invocation_priority_owner(Arc::new(ScreenDrawRecoveryBridge::default())),
            &mut prepared_routes,
        );
        ticket.wait_prepared(Duration::from_secs(1)).unwrap();
        let prepared = prepared_routes.get(&ticket.request_id()).unwrap();
        assert_eq!(prepared.sequence_routes.len(), 1);
        assert_eq!(
            prepared.sequence_routes[0].action,
            PasteSequenceHotkeyAction::Next
        );
        assert!(!prepared.sequence_omissions.is_empty());
        assert!(listener.is_active());
        let before = settings.hotkey.clone();
        settings.paste_sequence.next_hotkey = Some("Ctrl+End".into());
        let mut rejected = prepare_hotkey_reconfiguration(settings).unwrap();
        let HotkeyRuntimeMessage::Prepare(request) = receiver.try_recv().unwrap().unwrap() else {
            panic!("expected prepare")
        };
        prepare_hotkey_runtime_message(
            request,
            &document,
            1,
            &wake_tx,
            false,
            invocation_priority_owner(Arc::new(ScreenDrawRecoveryBridge::default())),
            &mut prepared_routes,
        );
        assert!(rejected.wait_prepared(Duration::from_secs(1)).is_err());
        assert!(!prepared_routes.contains_key(&rejected.request_id()));
        assert_eq!(
            existing._key,
            parse_hotkey(before.as_deref().unwrap()).unwrap().key
        );
        assert!(listener.is_active());
        listener.stop();
    }

    #[test]
    fn paste_routes_external_reload_rejection_preserves_store_identity_and_allows_retry() {
        use multi_launcher::radial::model::{
            RADIAL_FILE, TriggerDefinition, TriggerId, TriggerScope,
        };
        let directory = tempdir().unwrap();
        let root = AppDataRoot::from_settings_path(directory.path().join("settings.json")).unwrap();
        let store = RadialStore::new(&root).unwrap();
        let initial = store.snapshot().unwrap();
        let saved = store.save(initial.revision, (*initial).clone()).unwrap();
        let mut settings = Settings::default();
        settings.paste_sequence.next_hotkey = Some("Ctrl+F9".into());
        let old_routes =
            resolve_hotkey_route_candidate_with_macro(&settings, &saved, None, Some(&[]))
                .unwrap()
                .sequences;
        let mut external = (*saved).clone();
        external.custom_triggers.push(TriggerDefinition {
            id: TriggerId::new("external"),
            chord: "Alt+F9".into(),
            menu_id: external.default_menu_id.clone(),
            scope: TriggerScope::Global,
        });
        let path = directory.path().join(RADIAL_FILE);
        let bytes = serde_json::to_vec_pretty(&external).unwrap();
        std::fs::write(&path, &bytes).unwrap();
        let mut calls = 0;
        for _ in 0..2 {
            let rejected = store.reload_external_with(|candidate| {
                calls += 1;
                resolve_hotkey_route_candidate_with_macro(&settings, candidate, None, Some(&[]))
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            });
            assert!(matches!(
                rejected,
                Err(multi_launcher::radial::store::StoreError::PublicationRejected(_))
            ));
            assert_eq!(store.snapshot().unwrap(), saved);
            assert_eq!(std::fs::read(&path).unwrap(), bytes);
            assert_eq!(
                resolve_hotkey_route_candidate_with_macro(
                    &settings,
                    &store.snapshot().unwrap(),
                    None,
                    Some(&[])
                )
                .unwrap()
                .sequences,
                old_routes
            );
        }
        assert_eq!(
            calls, 2,
            "rejection must not remember disk bytes as an unchanged echo"
        );
        external.custom_triggers[0].chord = "Alt+F10".into();
        std::fs::write(&path, serde_json::to_vec_pretty(&external).unwrap()).unwrap();
        let mut admitted = None;
        let outcome = store
            .reload_external_with(|candidate| {
                admitted = Some(
                    resolve_hotkey_route_candidate_with_macro(
                        &settings,
                        candidate,
                        None,
                        Some(&[]),
                    )
                    .map_err(|error| error.to_string())?,
                );
                Ok(())
            })
            .unwrap();
        let ExternalReloadOutcome::Published(published) = outcome else {
            panic!("valid retry must publish")
        };
        assert_eq!(published.revision.0, saved.revision.0 + 1);
        assert_eq!(admitted.unwrap().sequences, old_routes);
        assert_eq!(
            store
                .reload_external_with(|_| panic!("successful identity must be remembered"))
                .unwrap(),
            ExternalReloadOutcome::Unchanged
        );
    }

    #[test]
    fn paste_routes_selected_slice_has_one_owner_in_shared_and_fallback_modes() {
        let mut settings = Settings::default();
        let document = RadialDocument::starter();
        let resolved =
            resolve_hotkey_route_candidate_with_macro(&settings, &document, None, Some(&[]))
                .unwrap();
        for shared in [true, false] {
            settings.radial.shared_tap_hold = shared;
            let plan =
                radial_route_plan_with_sequences(&settings, &document, 9, &resolved.sequences);
            let hook_count = plan
                .related
                .iter()
                .filter(|binding| matches!(binding.action, RelatedAction::PasteSequence(_)))
                .count();
            assert_eq!(hook_count, if shared { 2 } else { 0 });
            let draining = shared_drain_control_bindings(&settings, &document, &resolved.sequences);
            assert_eq!(
                draining
                    .iter()
                    .filter(|binding| matches!(binding.action, RelatedAction::PasteSequence(_)))
                    .count(),
                if shared { 2 } else { 0 }
            );
            if !shared {
                assert!(
                    draining.is_empty(),
                    "fallback controls must stay with the polling owner during drain"
                );
            }
            let launcher = Arc::new(HotkeyTrigger::new(settings.hotkey()));
            let sequence_triggers = sequence_hotkey_triggers_from_routes(&resolved.sequences);
            let polling = legacy_listener_triggers_with_sequences(
                shared,
                &launcher,
                None,
                None,
                None,
                None,
                &sequence_triggers,
            );
            assert_eq!(polling.len(), if shared { 0 } else { 3 });
        }
    }

    #[test]
    fn paste_settings_routes_use_custom_disabled_and_invalid_preferences() {
        let mut settings = Settings::default();
        settings.paste_sequence.next_hotkey = Some("Control+F9".into());
        settings.paste_sequence.cancel_hotkey = None;
        let routes = paste_sequence_hotkey_routes_from_bindings(&settings, &[]).unwrap();
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].hotkey, parse_hotkey("Ctrl+F9").unwrap());
        assert_eq!(routes[0].action, PasteSequenceHotkeyAction::Next);
        settings.paste_sequence.next_hotkey = Some("  ".into());
        assert!(
            paste_sequence_hotkey_routes_from_bindings(&settings, &[])
                .unwrap()
                .is_empty()
        );
        settings.paste_sequence.cancel_hotkey = Some("Ctrl+invalid".into());
        assert!(
            paste_sequence_hotkey_routes_from_bindings(&settings, &[])
                .unwrap_err()
                .contains("Cancel")
        );
        assert_eq!(settings.hotkey, Settings::default().hotkey);
    }

    #[test]
    fn paste_sequence_routes_are_stable_id_only_and_disabled_with_the_plugin() {
        use multi_launcher::hotkey::launcher_invocation::{
            PasteSequenceHotkeyAction, RelatedAction,
        };
        use multi_launcher::paste_sequence::{PlaybackMode, SequenceDraft, SequenceStore, Step};

        let directory = tempdir().unwrap();
        let store = SequenceStore::new(directory.path().join("paste_sequences.json"));
        let mut snapshot = store.load().unwrap();
        let id = snapshot
            .upsert(SequenceDraft {
                id: None,
                name: "Private sequence title".into(),
                hotkey: Some(multi_launcher::paste_sequence::HotkeyChord::new("Ctrl+F7").unwrap()),
                mode: PlaybackMode::Automatic,
                delay_ms: 0,
                steps: vec![Step::LiteralText {
                    text: "private sequence body".into(),
                }],
            })
            .unwrap();
        let settings = Settings::default();
        let routes = paste_sequence_hotkey_routes_from_snapshot(&settings, &snapshot).unwrap();
        assert_eq!(routes.len(), 3);
        assert!(routes.iter().any(|route| {
            route.chord == "Ctrl+Alt+Shift+PageDown"
                && route.action == PasteSequenceHotkeyAction::Next
        }));
        assert!(routes.iter().any(|route| {
            route.chord == "Ctrl+Alt+Shift+End" && route.action == PasteSequenceHotkeyAction::Cancel
        }));
        let start = routes
            .iter()
            .find(|route| route.chord == "Ctrl+F7")
            .expect("saved sequence shortcut is routed");
        assert_eq!(start.action, PasteSequenceHotkeyAction::Start(id));
        assert!(!format!("{:?}", start.action).contains("private sequence body"));
        let bindings = related_launcher_bindings_with_sequences(
            &settings,
            &RadialDocument::starter(),
            true,
            &routes,
        );
        assert!(bindings.iter().any(|binding| {
            binding.hotkey == start.hotkey
                && binding.action
                    == RelatedAction::PasteSequence(PasteSequenceHotkeyAction::Start(id))
        }));
        let draining_bindings =
            shared_drain_control_bindings(&settings, &RadialDocument::starter(), &routes);
        assert!(draining_bindings.iter().any(|binding| {
            binding.hotkey == start.hotkey
                && binding.action
                    == RelatedAction::PasteSequence(PasteSequenceHotkeyAction::Start(id))
        }));

        let mut disabled = settings.clone();
        disabled.enabled_plugins = Some(std::collections::HashSet::new());
        assert!(
            paste_sequence_hotkey_routes_from_snapshot(&disabled, &snapshot)
                .unwrap()
                .is_empty()
        );
        assert!(
            related_launcher_bindings_with_sequences(
                &disabled,
                &RadialDocument::starter(),
                true,
                &routes,
            )
            .iter()
            .all(|binding| !matches!(binding.action, RelatedAction::PasteSequence(_)))
        );
    }

    #[test]
    fn reloaded_sequence_binding_dispatches_through_the_shared_route_event() {
        use multi_launcher::hotkey::launcher_invocation::{
            PasteSequenceHotkeyAction, RelatedAction, ServiceNotice,
        };
        use multi_launcher::paste_sequence::{PlaybackMode, SequenceDraft, SequenceStore, Step};

        let directory = tempdir().unwrap();
        let store = SequenceStore::new(directory.path().join("paste_sequences.json"));
        let mut snapshot = store.load().unwrap();
        let id = snapshot
            .upsert(SequenceDraft {
                id: None,
                name: "Reloaded route".into(),
                hotkey: Some(multi_launcher::paste_sequence::HotkeyChord::new("Ctrl+F8").unwrap()),
                mode: PlaybackMode::Automatic,
                delay_ms: 0,
                steps: vec![Step::LiteralText {
                    text: "private body".into(),
                }],
            })
            .unwrap();
        store.commit(&mut snapshot).unwrap();
        let reloaded = store.load().unwrap();
        let settings = Settings::default();
        let routes = paste_sequence_hotkey_routes_from_snapshot(&settings, &reloaded).unwrap();
        let bindings = related_launcher_bindings_with_sequences(
            &settings,
            &RadialDocument::starter(),
            true,
            &routes,
        );
        let shared_action = bindings
            .iter()
            .find_map(|binding| match &binding.action {
                RelatedAction::PasteSequence(
                    action @ PasteSequenceHotkeyAction::Start(route_id),
                ) if *route_id == id => Some(action.clone()),
                _ => None,
            })
            .expect("the reloaded stable ID is present in shared related bindings");

        let (event_tx, event_rx) = std::sync::mpsc::channel();
        let _registration = multi_launcher::gui::register_event_sender(event_tx);
        let mut notice = ServiceNotice {
            recovery: None,
            intents: Vec::new(),
            error: None,
            action: Some(RelatedAction::PasteSequence(shared_action)),
            cancellation: None,
        };
        apply_service_related_action(&mut notice, None, None, None, None);

        let mut delivered = false;
        while let Ok(event) = event_rx.try_recv() {
            if let multi_launcher::gui::WatchEvent::PasteSequenceHotkey(
                PasteSequenceHotkeyAction::Start(delivered_id),
            ) = event
            {
                assert_eq!(delivered_id, id);
                delivered = true;
                break;
            }
        }
        assert!(
            delivered,
            "the shared route must deliver a typed GUI start event"
        );
    }

    #[test]
    fn paste_sequence_hotkey_conflicts_with_existing_launcher_routes() {
        use multi_launcher::paste_sequence::{PlaybackMode, SequenceDraft, SequenceStore, Step};

        let directory = tempdir().unwrap();
        let store = SequenceStore::new(directory.path().join("paste_sequences.json"));
        let mut snapshot = store.load().unwrap();
        snapshot
            .upsert(SequenceDraft {
                id: None,
                name: "Conflicting sequence".into(),
                hotkey: Some(multi_launcher::paste_sequence::HotkeyChord::new("F2").unwrap()),
                mode: PlaybackMode::Automatic,
                delay_ms: 0,
                steps: vec![Step::LiteralText { text: "x".into() }],
            })
            .unwrap();
        let settings = Settings::default();
        let document = RadialDocument::starter();
        let routes =
            multi_launcher::hotkey::validation::validate_hotkey_routes(&settings, &document, None)
                .unwrap();
        let sequence_routes =
            paste_sequence_hotkey_routes_from_snapshot(&settings, &snapshot).unwrap();
        let error = validate_paste_sequence_hotkey_routes(
            &settings,
            &document,
            None,
            &routes,
            &sequence_routes,
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("Paste Sequence shortcut can co-fire with launcher")
        );
    }

    #[test]
    fn paste_sequence_candidate_routes_validate_before_the_candidate_is_persisted() {
        use multi_launcher::paste_sequence::{
            HotkeyChord, PlaybackMode, SequenceDraft, SequenceStore, Step,
        };

        let directory = tempdir().unwrap();
        let store = SequenceStore::new(directory.path().join("paste_sequences.json"));
        let mut candidate = store.load().unwrap();
        candidate
            .upsert(SequenceDraft {
                id: None,
                name: "Prospective conflict".into(),
                hotkey: Some(HotkeyChord::new("F2").unwrap()),
                mode: PlaybackMode::Automatic,
                delay_ms: 0,
                steps: vec![Step::LiteralText {
                    text: "payload".into(),
                }],
            })
            .unwrap();
        let bindings = candidate.hotkey_bindings();
        let settings = Settings::default();
        let document = RadialDocument::starter();
        let base_routes =
            multi_launcher::hotkey::validation::validate_hotkey_routes(&settings, &document, None)
                .unwrap();
        let candidate_routes =
            paste_sequence_hotkey_routes_from_bindings(&settings, &bindings).unwrap();
        assert!(candidate_routes.iter().any(|route| route.chord == "F2"));
        assert!(
            validate_paste_sequence_hotkey_routes(
                &settings,
                &document,
                None,
                &base_routes,
                &candidate_routes,
            )
            .is_err()
        );
        assert!(store.load().unwrap().sequences().is_empty());
    }

    #[test]
    fn shared_route_drain_keeps_old_process_controls_without_direct_radial_admission() {
        let mut settings = Settings {
            quit_hotkey: Some("Ctrl+Q".into()),
            help_hotkey: Some("Ctrl+H".into()),
            ..Settings::default()
        };
        settings.plugin_settings.insert(
            "screen_draw".into(),
            serde_json::json!({
                "launch_hotkey": "Ctrl+Shift+D",
                "emergency_hotkey": "Ctrl+Shift+F11"
            }),
        );

        let bindings = shared_drain_control_bindings(&settings, &RadialDocument::starter(), &[]);
        assert!(
            bindings
                .iter()
                .any(|binding| matches!(&binding.action, RelatedAction::Quit))
        );
        assert!(
            bindings
                .iter()
                .any(|binding| matches!(&binding.action, RelatedAction::Help))
        );
        assert!(
            bindings
                .iter()
                .any(|binding| matches!(&binding.action, RelatedAction::ScreenDrawLaunch))
        );
        assert!(
            bindings
                .iter()
                .any(|binding| matches!(&binding.action, RelatedAction::ScreenDrawEmergency))
        );
        assert!(
            bindings
                .iter()
                .all(|binding| !matches!(&binding.action, RelatedAction::DirectMenu { .. }))
        );
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
    fn hotkey_directory_runtime_cancelled_save_keeps_the_accepted_route_visible() {
        let (wake_tx, wake_rx) = channel();
        let receiver =
            multi_launcher::hotkey::reconfiguration::install_main_runtime(wake_tx.clone()).unwrap();
        let mut settings = Settings {
            hotkey: Some("Ctrl+F2".into()),
            radial: multi_launcher::radial::model::RadialFeatureSettings {
                enabled: false,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut candidate = settings.clone();
        candidate.hotkey = Some("Ctrl+F3".into());
        settings.radial.shared_tap_hold = false;
        let settings_before = serde_json::to_value(&settings).unwrap();
        let radial_document = RadialDocument::starter();
        let radial_before = serde_json::to_value(&radial_document).unwrap();
        let directory = tempdir().unwrap();
        let settings_file = directory.path().join("settings.json");
        std::fs::write(&settings_file, b"keep persisted bytes").unwrap();
        let (old_wake_tx, old_wake_rx) = channel();
        let existing_trigger = Arc::new(HotkeyTrigger::new(parse_hotkey("Ctrl+F2").unwrap()));
        let mut existing_listener =
            HotkeyTrigger::start_listener(vec![existing_trigger], "existing-test", old_wake_tx);
        assert!(existing_listener.is_active());
        let directory_macros = multi_launcher::mkmacro::MkMacroDocument::default();
        let directory_source = |settings: &Settings,
                                generation: u64,
                                active: &[multi_launcher::hotkey::Hotkey],
                                pending: bool,
                                source_revision: u64| {
            hotkey_directory_source_snapshots(
                settings,
                &radial_document,
                None,
                &[],
                None,
                &[],
                Some(&directory_macros),
                1,
                active,
                None,
                generation,
                false,
                pending,
                source_revision,
            )
            .into_iter()
            .find(|source| source.source == multi_launcher::hotkey::directory::Source::MainSettings)
            .expect("the main Settings directory source exists")
        };
        let before_cancel = multi_launcher::hotkey::directory::Snapshot::from_sources(
            1,
            vec![directory_source(
                &settings,
                4,
                &legacy_listener_hotkeys(&existing_listener),
                false,
                1,
            )],
        );
        let prior_launcher = before_cancel
            .entries()
            .iter()
            .find(|entry| entry.id.as_str() == "main.launcher")
            .unwrap();
        assert_eq!(prior_launcher.chord.as_deref(), Some("Ctrl+F2"));
        assert_eq!(
            prior_launcher.status,
            multi_launcher::hotkey::directory::Status::Active { generation: 4 }
        );

        let policy_before = physical_paste_bridge().route_availability();
        let revision_before = physical_paste_bridge().route_revision();
        let mut ticket = multi_launcher::hotkey::reconfiguration::prepare_hotkey_reconfiguration(
            candidate.clone(),
        )
        .unwrap();
        assert_eq!(wake_rx.try_recv(), Ok(()));
        let multi_launcher::hotkey::reconfiguration::HotkeyRuntimeMessage::Prepare(request) =
            receiver.try_recv().unwrap().unwrap()
        else {
            panic!("expected prepare request")
        };
        let mut prepared_routes = HashMap::new();
        prepare_hotkey_runtime_message(
            request,
            &radial_document,
            4,
            &wake_tx,
            true, // The injected handoff issuer supplies the existing observer.
            invocation_priority_owner(Arc::new(ScreenDrawRecoveryBridge::default())),
            &mut prepared_routes,
        );

        assert_eq!(physical_paste_bridge().route_availability(), policy_before);
        assert_eq!(physical_paste_bridge().route_revision(), revision_before);
        let prepared = ticket.try_complete().unwrap().unwrap();
        assert_eq!(prepared.generation.0, 5);
        assert_eq!(prepared.request_id, ticket.request_id());
        assert!(existing_listener.is_active());
        assert!(
            !prepared_routes
                .get(&prepared.request_id)
                .unwrap()
                .listener
                .as_ref()
                .unwrap()
                .is_active()
        );
        assert_eq!(serde_json::to_value(&settings).unwrap(), settings_before);
        assert_eq!(
            serde_json::to_value(&radial_document).unwrap(),
            radial_before
        );
        assert_eq!(
            std::fs::read(&settings_file).unwrap(),
            b"keep persisted bytes"
        );

        ticket.apply(candidate.clone()).unwrap();
        assert_eq!(wake_rx.try_recv(), Ok(()));
        let multi_launcher::hotkey::reconfiguration::HotkeyRuntimeMessage::Apply(request) =
            receiver.try_recv().unwrap().unwrap()
        else {
            panic!("expected apply request")
        };
        let mut pending_launcher_route = None;
        let mut invocation_service = None;
        let mut sequence_routes = Vec::new();
        let mut settings_generation = 4;
        let mut route_handoff_issuer = TestHotkeyHandoffIssuer::successful();
        begin_hotkey_application_with_issuer(
            HotkeyApplicationStart {
                request,
                prepared_routes: &mut prepared_routes,
                settings: &mut settings,
                settings_generation: &mut settings_generation,
                sequence_routes: &mut sequence_routes,
                radial_document: &radial_document,
                event_tx: &wake_tx,
                listener: &mut existing_listener,
                invocation_service: &mut invocation_service,
                pending_launcher_route: &mut pending_launcher_route,
            },
            &mut route_handoff_issuer,
        );
        assert!(existing_listener.is_active());
        assert_eq!(settings.hotkey, Some("Ctrl+F2".into()));
        assert!(pending_launcher_route.is_some());
        assert_eq!(ticket.try_terminal_disposition(), None);
        let pending_application = pending_launcher_route
            .as_ref()
            .and_then(|pending| match &pending.completion {
                PendingLauncherRouteCompletion::Hotkey(application) => Some(application),
                _ => None,
            })
            .expect("the draft replacement is still pending owner acknowledgement");
        let pending_source = directory_source(
            &pending_application.prepared.settings,
            pending_application.candidate_generation,
            &legacy_listener_hotkeys(&existing_listener),
            true,
            2,
        );
        let pending_snapshot = before_cancel
            .replacing_source(2, pending_source)
            .expect("pending settings publish a newer directory source");
        let pending_launcher = pending_snapshot
            .entries()
            .iter()
            .find(|entry| entry.id.as_str() == "main.launcher")
            .unwrap();
        assert_eq!(pending_launcher.chord.as_deref(), Some("Ctrl+F3"));
        assert_eq!(
            pending_launcher.status,
            multi_launcher::hotkey::directory::Status::Pending { generation: 5 }
        );
        assert_eq!(
            pending_launcher
                .last_applied
                .as_ref()
                .map(|receipt| receipt.chord.as_str()),
            Some("Ctrl+F2")
        );

        ticket.cancel().unwrap();
        assert_eq!(wake_rx.try_recv(), Ok(()));
        let multi_launcher::hotkey::reconfiguration::HotkeyRuntimeMessage::Cancel(mut cancel) =
            receiver.try_recv().unwrap().unwrap()
        else {
            panic!("expected cancel request")
        };
        let Some(PendingLauncherRoute {
            completion: PendingLauncherRouteCompletion::Hotkey(application),
            ..
        }) = pending_launcher_route.take()
        else {
            panic!("an accepted apply must still own the route before cancellation")
        };
        let mut application = *application;
        application.cancellation_requested = true;
        application.failure = Some(
            multi_launcher::hotkey::reconfiguration::HotkeyReconfigurationError::
                TransactionCancelled,
        );
        cancel.defer_terminal();
        begin_hotkey_route_rollback(
            application,
            &mut existing_listener,
            &mut invocation_service,
            &radial_document,
            &mut pending_launcher_route,
            &mut route_handoff_issuer,
        );
        assert_eq!(
            ticket.try_terminal_disposition(),
            Some(Ok(
                multi_launcher::hotkey::reconfiguration::HotkeyTerminalDisposition::
                    ConfirmedNotAppliedOrRestored(
                        multi_launcher::hotkey::reconfiguration::HotkeyReconfigurationError::
                            TransactionCancelled,
                    ),
            ))
        );
        assert!(existing_listener.is_active());
        assert_eq!(settings.hotkey, Some("Ctrl+F2".into()));
        assert!(pending_launcher_route.is_none());
        assert!(prepared_routes.is_empty());
        let restored_source = directory_source(
            &settings,
            settings_generation,
            &legacy_listener_hotkeys(&existing_listener),
            false,
            3,
        );
        let restored_snapshot = pending_snapshot
            .replacing_source(3, restored_source)
            .expect("cancellation publishes the restored owner state");
        let restored_launcher = restored_snapshot
            .entries()
            .iter()
            .find(|entry| entry.id.as_str() == "main.launcher")
            .unwrap();
        assert_eq!(restored_launcher.chord.as_deref(), Some("Ctrl+F2"));
        assert_eq!(
            restored_launcher.status,
            multi_launcher::hotkey::directory::Status::Active { generation: 4 }
        );
        assert_eq!(
            restored_launcher
                .last_applied
                .as_ref()
                .map(|receipt| receipt.chord.as_str()),
            Some("Ctrl+F2")
        );
        existing_listener.stop();
        drop(old_wake_rx);
    }

    #[test]
    fn hotkey_directory_runtime_main_save_publishes_only_accepted_route_state() {
        let (presentation_tx, presentation_rx) = channel();
        let _presentation_registration =
            multi_launcher::gui::register_event_sender(presentation_tx);
        use multi_launcher::hotkey::reconfiguration::{
            AppliedHotkeyReconfiguration, HotkeyRuntimeMessage,
        };

        let (wake_tx, wake_rx) = channel();
        let receiver =
            multi_launcher::hotkey::reconfiguration::install_main_runtime(wake_tx.clone()).unwrap();
        let old_settings = Settings {
            hotkey: Some("Ctrl+F2".into()),
            radial: multi_launcher::radial::model::RadialFeatureSettings {
                enabled: false,
                shared_tap_hold: false,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut candidate = old_settings.clone();
        candidate.hotkey = Some("Ctrl+F3".into());
        candidate.paste_sequence.next_hotkey = Some("Ctrl+F10".into());
        let radial_document = Arc::new(RadialDocument::starter());
        let old_trigger = Arc::new(HotkeyTrigger::new(parse_hotkey("Ctrl+F2").unwrap()));
        let mut listener = HotkeyTrigger::start_listener(
            vec![Arc::clone(&old_trigger)],
            "hotkey-apply-old",
            wake_tx.clone(),
        );
        let directory_macros = multi_launcher::mkmacro::MkMacroDocument::default();
        let directory_main_source = |settings: &Settings,
                                     generation: u64,
                                     active: &[multi_launcher::hotkey::Hotkey],
                                     pending: bool,
                                     source_revision: u64| {
            hotkey_directory_source_snapshots(
                settings,
                &radial_document,
                None,
                &[],
                None,
                &[],
                Some(&directory_macros),
                1,
                active,
                None,
                generation,
                false,
                pending,
                source_revision,
            )
            .into_iter()
            .find(|source| source.source == multi_launcher::hotkey::directory::Source::MainSettings)
            .expect("the main Settings source is published")
        };
        let old_directory_snapshot = multi_launcher::hotkey::directory::Snapshot::from_sources(
            1,
            vec![directory_main_source(
                &old_settings,
                12,
                &legacy_listener_hotkeys(&listener),
                false,
                1,
            )],
        );
        let old_launcher = old_directory_snapshot
            .entries()
            .iter()
            .find(|entry| entry.id.as_str() == "main.launcher")
            .expect("the acknowledged old launcher row is visible");
        assert_eq!(old_launcher.chord.as_deref(), Some("Ctrl+F2"));
        assert_eq!(
            old_launcher.status,
            multi_launcher::hotkey::directory::Status::Active { generation: 12 }
        );
        let mut ticket = multi_launcher::hotkey::reconfiguration::prepare_hotkey_reconfiguration(
            candidate.clone(),
        )
        .unwrap();
        assert_eq!(wake_rx.try_recv(), Ok(()));
        let HotkeyRuntimeMessage::Prepare(request) = receiver.try_recv().unwrap().unwrap() else {
            panic!("expected prepare request")
        };
        let mut prepared_routes = HashMap::new();
        prepare_hotkey_runtime_message(
            request,
            &radial_document,
            12,
            &wake_tx,
            true, // The injected handoff issuer supplies the existing observer.
            invocation_priority_owner(Arc::new(ScreenDrawRecoveryBridge::default())),
            &mut prepared_routes,
        );
        let prepared = ticket.wait_prepared(Duration::from_secs(1)).unwrap();
        assert_eq!(prepared.generation.0, 13);
        candidate.radial_designer.zoom = 1.25;
        ticket.apply(candidate.clone()).unwrap();
        assert_eq!(wake_rx.try_recv(), Ok(()));
        let HotkeyRuntimeMessage::Apply(request) = receiver.try_recv().unwrap().unwrap() else {
            panic!("expected apply request")
        };

        let mut settings = old_settings.clone();
        let mut settings_generation = 12;
        let mut trigger = old_trigger;
        let mut quit_trigger = None;
        let mut help_trigger = None;
        let mut screen_draw_trigger = None;
        let mut emergency_trigger = None;
        let mut invocation_service = None;
        let mut pending_launcher_route = None;
        let mut sequence_routes = Vec::new();
        let mut route_handoff_issuer = TestHotkeyHandoffIssuer::successful();
        begin_hotkey_application_with_issuer(
            HotkeyApplicationStart {
                request,
                prepared_routes: &mut prepared_routes,
                settings: &mut settings,
                settings_generation: &mut settings_generation,
                sequence_routes: &mut sequence_routes,
                radial_document: &radial_document,
                event_tx: &wake_tx,
                listener: &mut listener,
                invocation_service: &mut invocation_service,
                pending_launcher_route: &mut pending_launcher_route,
            },
            &mut route_handoff_issuer,
        );

        assert_eq!(settings.hotkey, old_settings.hotkey);
        assert_eq!(settings_generation, 12);
        assert_eq!(trigger._key, parse_hotkey("Ctrl+F2").unwrap().key);
        assert!(listener.is_active());
        assert_eq!(ticket.try_terminal_disposition(), None);
        let pending_application = pending_launcher_route
            .as_ref()
            .and_then(|pending| match &pending.completion {
                PendingLauncherRouteCompletion::Hotkey(application) => Some(application),
                _ => None,
            })
            .expect("the prepared candidate remains owned by the pending route");
        let pending_source = directory_main_source(
            &pending_application.prepared.settings,
            pending_application.candidate_generation,
            &legacy_listener_hotkeys(&listener),
            true,
            2,
        );
        let pending_directory_snapshot = old_directory_snapshot
            .replacing_source(2, pending_source)
            .expect("the pending candidate publishes a newer Main Settings source");
        let pending_launcher = pending_directory_snapshot
            .entries()
            .iter()
            .find(|entry| entry.id.as_str() == "main.launcher")
            .expect("the pending launcher row remains visible");
        assert_eq!(pending_launcher.chord.as_deref(), Some("Ctrl+F3"));
        assert_eq!(
            pending_launcher.status,
            multi_launcher::hotkey::directory::Status::Pending { generation: 13 }
        );
        assert_eq!(
            pending_launcher
                .last_applied
                .as_ref()
                .map(|receipt| receipt.chord.as_str()),
            Some("Ctrl+F2"),
            "the candidate does not erase the previously acknowledged route"
        );
        assert_eq!(
            old_directory_snapshot
                .entries()
                .iter()
                .find(|entry| entry.id.as_str() == "main.launcher")
                .unwrap()
                .chord
                .as_deref(),
            Some("Ctrl+F2"),
            "the retained displayed snapshot stays on A until the owner accepts B"
        );
        let settings_document_before = serde_json::to_value(radial_document.as_ref()).unwrap();
        let (control_client, radial_control_endpoint) =
            radial_control_service_with_wake(false, None);
        control_client
            .send(RadialControlRequest::RestoreSubmenuPresentationMigration)
            .unwrap();
        assert_eq!(
            try_recv_when_hotkey_route_is_idle(
                &pending_launcher_route,
                &radial_control_endpoint.request_rx,
            ),
            None
        );
        assert_eq!(settings_generation, 12);
        assert_eq!(
            serde_json::to_value(radial_document.as_ref()).unwrap(),
            settings_document_before
        );
        let Some(PendingLauncherRoute {
            completion: PendingLauncherRouteCompletion::Hotkey(application),
            handoff: Some(_),
            ..
        }) = pending_launcher_route.take()
        else {
            panic!("paste-only observer apply should await the main owner acknowledgement")
        };

        let (controller_wake, _controller_wake_rx) = channel();
        let mut radial_controller =
            RadialController::new(Arc::clone(&radial_document), false, controller_wake);
        let mut radial_resources = disabled_radial_resources();
        let (_authoring_client, authoring_endpoint) = authoring_control_service_with_wake(None);
        let workspace = tempdir().unwrap();
        let preview_wake = wake_tx.clone();
        let mut native_preview =
            multi_launcher::radial::authoring::native_preview::NativePreviewCoordinator::new(
                preview_wake,
                workspace.path().to_path_buf(),
            );
        let mut sequence_routes = Vec::new();
        let mut sequence_triggers = Vec::new();
        let mut applied_shared_route = None;
        let commit_context = HotkeyRuntimeCommitContext {
            settings: &mut settings,
            settings_generation: &mut settings_generation,
            applied_shared_route: &mut applied_shared_route,
            trigger: &mut trigger,
            quit_trigger: &mut quit_trigger,
            help_trigger: &mut help_trigger,
            screen_draw_trigger: &mut screen_draw_trigger,
            emergency_trigger: &mut emergency_trigger,
            listener: &mut listener,
            invocation_service: &mut invocation_service,
            radial_control_endpoint: &radial_control_endpoint,
            radial_resources: &mut radial_resources,
            radial_controller: &mut radial_controller,
            native_preview: &mut native_preview,
            radial_document: &radial_document,
            authoring_reply_tx: &authoring_endpoint.reply_tx,
            event_tx: &wake_tx,
            sequence_routes: &mut sequence_routes,
            sequence_triggers: &mut sequence_triggers,
        };
        let mut deferred_notices_follow_failure = false;
        let mut completion_context = HotkeyRouteCompletionContext {
            runtime: commit_context,
            pending_route: &mut pending_launcher_route,
            deferred_notices_follow_failure: &mut deferred_notices_follow_failure,
        };
        let mut route_handoff_issuer = TestHotkeyHandoffIssuer::successful();
        assert!(!complete_hotkey_route_step(
            *application,
            Ok(()),
            &mut completion_context,
            &mut route_handoff_issuer,
        ));

        let applied = AppliedHotkeyReconfiguration {
            request_id: ticket.request_id(),
            generation: prepared.generation,
        };
        assert_eq!(
            ticket.try_terminal_disposition(),
            Some(Ok(
                multi_launcher::hotkey::reconfiguration::HotkeyTerminalDisposition::Applied(
                    applied,
                ),
            ))
        );
        assert_eq!(settings.hotkey, candidate.hotkey);
        assert_eq!(settings.radial_designer.zoom, 1.25);
        assert_eq!(settings_generation, 13);
        let accepted_source = directory_main_source(
            &settings,
            settings_generation,
            &legacy_listener_hotkeys(&listener),
            false,
            3,
        );
        let accepted_directory_snapshot = pending_directory_snapshot
            .replacing_source(3, accepted_source)
            .expect("only the accepted runtime publication replaces the pending row");
        let accepted_launcher = accepted_directory_snapshot
            .entries()
            .iter()
            .find(|entry| entry.id.as_str() == "main.launcher")
            .expect("the accepted launcher row remains visible");
        assert_eq!(accepted_launcher.chord.as_deref(), Some("Ctrl+F3"));
        assert_eq!(
            accepted_launcher.status,
            multi_launcher::hotkey::directory::Status::Active { generation: 13 }
        );
        assert_eq!(
            accepted_launcher
                .last_applied
                .as_ref()
                .map(|receipt| (receipt.chord.as_str(), receipt.generation)),
            Some(("Ctrl+F3", 13)),
            "the displayed replacement is active only after its exact runtime receipt"
        );
        let published = presentation_rx
            .try_iter()
            .find_map(|event| match event {
                multi_launcher::gui::WatchEvent::PasteSequenceRoutes(snapshot) => Some(snapshot),
                _ => None,
            })
            .expect("successful owner application publishes its acknowledged controls");
        assert_eq!(
            published.physical_paste.availability,
            PhysicalPasteAvailability::Available
        );
        assert_eq!(
            physical_paste_bridge().route_availability(),
            PhysicalPasteAvailability::Available
        );
        assert_eq!(published.generation, 13);
        assert_eq!(
            published.next,
            multi_launcher::paste_sequence::SequenceShortcutPresentation::Active("Ctrl+F10".into())
        );
        assert_eq!(trigger._key, parse_hotkey("Ctrl+F3").unwrap().key);
        assert!(listener.is_active());
        assert!(pending_launcher_route.is_none());
        assert!(prepared_routes.is_empty());
        assert_eq!(control_client.is_enabled(), settings.radial.enabled);
        assert_eq!(
            try_recv_when_hotkey_route_is_idle(
                &pending_launcher_route,
                &radial_control_endpoint.request_rx,
            ),
            Some(RadialControlRequest::RestoreSubmenuPresentationMigration)
        );
        assert_eq!(
            try_recv_when_hotkey_route_is_idle(
                &pending_launcher_route,
                &radial_control_endpoint.request_rx,
            ),
            None
        );

        let mut route_candidate = settings.clone();
        route_candidate.hotkey = Some("Ctrl+F4".into());
        let mut stale_ticket =
            multi_launcher::hotkey::reconfiguration::prepare_hotkey_reconfiguration(
                route_candidate.clone(),
            )
            .unwrap();
        assert_eq!(wake_rx.try_recv(), Ok(()));
        let HotkeyRuntimeMessage::Prepare(request) = receiver.try_recv().unwrap().unwrap() else {
            panic!("expected follow-up prepare request")
        };
        prepare_hotkey_runtime_message(
            request,
            &radial_document,
            settings_generation,
            &wake_tx,
            true, // The injected handoff issuer supplies the observer.
            invocation_priority_owner(Arc::new(ScreenDrawRecoveryBridge::default())),
            &mut prepared_routes,
        );
        stale_ticket.wait_prepared(Duration::from_secs(1)).unwrap();
        route_candidate.paste_sequence.next_hotkey = Some("Ctrl+F5".into());
        stale_ticket.apply(route_candidate).unwrap();
        assert_eq!(wake_rx.try_recv(), Ok(()));
        let HotkeyRuntimeMessage::Apply(request) = receiver.try_recv().unwrap().unwrap() else {
            panic!("expected stale apply request")
        };
        let mut sequence_routes = Vec::new();
        let mut route_handoff_issuer = TestHotkeyHandoffIssuer::successful();
        begin_hotkey_application_with_issuer(
            HotkeyApplicationStart {
                request,
                prepared_routes: &mut prepared_routes,
                settings: &mut settings,
                settings_generation: &mut settings_generation,
                sequence_routes: &mut sequence_routes,
                radial_document: &radial_document,
                event_tx: &wake_tx,
                listener: &mut listener,
                invocation_service: &mut invocation_service,
                pending_launcher_route: &mut pending_launcher_route,
            },
            &mut route_handoff_issuer,
        );
        assert_eq!(
            stale_ticket.try_terminal_disposition(),
            Some(Ok(
                multi_launcher::hotkey::reconfiguration::HotkeyTerminalDisposition::
                    ConfirmedNotAppliedOrRestored(
                        multi_launcher::hotkey::reconfiguration::HotkeyReconfigurationError::
                            StaleGeneration,
                    ),
            ))
        );
        assert_eq!(settings.hotkey, candidate.hotkey);
        assert_eq!(trigger._key, parse_hotkey("Ctrl+F3").unwrap().key);
        assert!(listener.is_active());
        assert!(prepared_routes.is_empty());
        assert!(
            !presentation_rx.try_iter().any(|event| matches!(
                event,
                multi_launcher::gui::WatchEvent::PasteSequenceRoutes(_)
            )),
            "rejected stale Apply must not replace the acknowledged presentation"
        );

        radial_controller.shutdown();
        listener.stop();
        if let Some(mut service) = invocation_service {
            service.stop();
        }
    }

    #[test]
    fn hotkey_directory_runtime_shared_owner_restores_route_before_candidate_failure() {
        use multi_launcher::hotkey::reconfiguration::HotkeyRuntimeMessage;

        let (wake_tx, wake_rx) = channel();
        let receiver =
            multi_launcher::hotkey::reconfiguration::install_main_runtime(wake_tx.clone()).unwrap();
        let mut old_settings = Settings {
            hotkey: Some("Ctrl+F2".into()),
            quit_hotkey: Some("Ctrl+Q".into()),
            help_hotkey: Some("Ctrl+H".into()),
            radial: multi_launcher::radial::model::RadialFeatureSettings {
                enabled: true,
                shared_tap_hold: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut screen_draw = multi_launcher::screen_draw::settings::ScreenDrawSettings::default();
        screen_draw.launch_hotkey = Some(
            multi_launcher::screen_draw::settings::HotkeyChord::from_unchecked("Ctrl+Shift+S"),
        );
        screen_draw.emergency_hotkey =
            multi_launcher::screen_draw::settings::HotkeyChord::from_unchecked("Ctrl+Shift+E");
        old_settings.plugin_settings.insert(
            "screen_draw".into(),
            serde_json::to_value(screen_draw).unwrap(),
        );
        let mut candidate = old_settings.clone();
        candidate.hotkey = Some("Ctrl+F3".into());
        let radial_document = Arc::new(RadialDocument::starter());
        let old_trigger = Arc::new(HotkeyTrigger::new(parse_hotkey("Ctrl+F2").unwrap()));
        let old_listener = HotkeyTrigger::start_listener(Vec::new(), "old-shared", wake_tx.clone());
        let mut fixture = HotkeyCompletionFixture::new(
            old_settings.clone(),
            old_trigger,
            old_listener,
            Arc::clone(&radial_document),
            wake_tx.clone(),
            19,
        );
        let directory_macros = multi_launcher::mkmacro::MkMacroDocument::default();
        let directory_source =
            |settings: &Settings,
             generation: u64,
             route_pending: bool,
             publication_revision: u64,
             applied: Option<&AppliedSharedRouteEvidence>| {
                hotkey_directory_source_snapshots(
                    settings,
                    &radial_document,
                    None,
                    &[],
                    None,
                    &[],
                    Some(&directory_macros),
                    1,
                    &[],
                    applied,
                    generation,
                    false,
                    route_pending,
                    publication_revision,
                )
                .into_iter()
                .find(|source| {
                    source.source == multi_launcher::hotkey::directory::Source::MainSettings
                })
                .expect("the main Settings source is published")
            };
        let old_applied =
            applied_shared_route_after_successful_install(&old_settings, &radial_document, &[], 19)
                .expect("the prior shared route has an exact applied receipt");
        let old_directory = multi_launcher::hotkey::directory::Snapshot::from_sources(
            1,
            vec![directory_source(
                &old_settings,
                19,
                false,
                1,
                Some(&old_applied),
            )],
        );
        let old_directory_launcher = old_directory
            .entries()
            .iter()
            .find(|entry| entry.id.as_str() == "main.launcher")
            .expect("the previously applied launcher route is visible");
        assert_eq!(old_directory_launcher.chord.as_deref(), Some("Ctrl+F2"));
        assert_eq!(
            old_directory_launcher.status,
            multi_launcher::hotkey::directory::Status::Active { generation: 19 }
        );

        let mut ticket = multi_launcher::hotkey::reconfiguration::prepare_hotkey_reconfiguration(
            candidate.clone(),
        )
        .unwrap();
        assert_eq!(wake_rx.try_recv(), Ok(()));
        let HotkeyRuntimeMessage::Prepare(request) = receiver.try_recv().unwrap().unwrap() else {
            panic!("expected prepare request")
        };
        let mut prepared_routes = HashMap::new();
        prepare_hotkey_runtime_message(
            request,
            &radial_document,
            19,
            &wake_tx,
            true,
            invocation_priority_owner(Arc::new(ScreenDrawRecoveryBridge::default())),
            &mut prepared_routes,
        );
        let prepared = ticket.wait_prepared(Duration::from_secs(1)).unwrap();
        assert!(prepared.shared_invocation);
        ticket.apply(candidate.clone()).unwrap();
        assert_eq!(wake_rx.try_recv(), Ok(()));
        let HotkeyRuntimeMessage::Apply(request) = receiver.try_recv().unwrap().unwrap() else {
            panic!("expected apply request")
        };
        let pending_directory = old_directory
            .replacing_source(
                2,
                directory_source(&candidate, 20, true, 2, Some(&old_applied)),
            )
            .expect("the pending candidate is a newer Settings publication");
        let pending_launcher = pending_directory
            .entries()
            .iter()
            .find(|entry| entry.id.as_str() == "main.launcher")
            .expect("the candidate launcher route remains visible during apply");
        assert_eq!(pending_launcher.chord.as_deref(), Some("Ctrl+F3"));
        assert_eq!(
            pending_launcher.status,
            multi_launcher::hotkey::directory::Status::Pending { generation: 20 }
        );
        assert_eq!(
            pending_launcher
                .last_applied
                .as_ref()
                .map(|receipt| (receipt.chord.as_str(), receipt.generation)),
            Some(("Ctrl+F2", 19)),
            "a failed candidate does not replace the previous applied receipt"
        );
        let prepared_route = prepared_routes.remove(&prepared.request_id).unwrap();
        let fallback_listener = HotkeyTrigger::prepare_listener(
            fixture.listener.triggers().to_vec(),
            "test-shared-fallback",
            wake_tx.clone(),
        )
        .unwrap();
        fixture.pending_route = Some(PendingLauncherRoute {
            settings_generation: 19,
            handoff: Some(Box::new(TestHotkeyHandoff {
                completion: std::cell::RefCell::new(Some(Ok(()))),
            })),
            policy_applied: Some(Ok(())),
            completion: PendingLauncherRouteCompletion::Hotkey(Box::new(
                PendingHotkeyApplication {
                    request: Some(request),
                    prepared: prepared_route,
                    old_settings: old_settings.clone(),
                    old_generation: 19,
                    candidate_generation: 20,
                    old_service_existed: true,
                    old_sequence_routes: Vec::new(),
                    old_listener_stopped: false,
                    fallback_listener: Some(fallback_listener),
                    phase: HotkeyApplyPhase::DrainOldRoute,
                    cancellation_requested: false,
                    failure: None,
                    restore_generation: None,
                },
            )),
        });
        let mut issuer = TestHotkeyHandoffIssuer {
            results: VecDeque::from([Ok(Err("candidate hook failed".into())), Ok(Ok(()))]),
            issued: 0,
            policies: Vec::new(),
        };

        let drain_result = fixture
            .pending_route
            .as_ref()
            .and_then(|pending| pending.handoff.as_ref())
            .and_then(|handoff| handoff.try_complete())
            .expect("old route drain receipt is available");
        let PendingLauncherRoute {
            completion: PendingLauncherRouteCompletion::Hotkey(application),
            ..
        } = fixture.pending_route.take().unwrap()
        else {
            panic!("expected settings route completion")
        };
        {
            let mut context = fixture.context();
            assert!(!complete_hotkey_route_step(
                *application,
                drain_result,
                &mut context,
                &mut issuer,
            ));
        }

        assert_eq!(issuer.issued, 1);
        assert_eq!(
            serde_json::to_value(&fixture.settings).unwrap(),
            serde_json::to_value(&old_settings).unwrap()
        );
        assert_eq!(fixture.settings_generation, 19);
        assert_eq!(ticket.try_terminal_disposition(), None);
        let candidate_result = fixture
            .pending_route
            .as_ref()
            .and_then(|pending| pending.handoff.as_ref())
            .and_then(|handoff| handoff.try_complete())
            .expect("candidate route failure receipt is available");
        assert_eq!(candidate_result, Err("candidate hook failed".into()));
        let PendingLauncherRoute {
            completion: PendingLauncherRouteCompletion::Hotkey(application),
            ..
        } = fixture.pending_route.take().unwrap()
        else {
            panic!("expected candidate activation completion")
        };
        {
            let mut context = fixture.context();
            assert!(!complete_hotkey_route_step(
                *application,
                candidate_result,
                &mut context,
                &mut issuer,
            ));
        }

        assert_eq!(issuer.issued, 2);
        assert_eq!(issuer.policies.len(), 2);
        let candidate_policy = &issuer.policies[0];
        assert!(candidate_policy.config.launcher_enabled);
        assert_eq!(
            candidate_policy.config.hotkey,
            parse_hotkey("Ctrl+F3").unwrap()
        );
        assert_eq!(candidate_policy.config.generation, 20);
        assert_eq!(
            candidate_policy.cancellation,
            LifecycleCancellation::SettingsReload
        );
        for (action, chord) in [
            (RelatedAction::Quit, "Ctrl+Q"),
            (RelatedAction::Help, "Ctrl+H"),
            (RelatedAction::ScreenDrawLaunch, "Ctrl+Shift+S"),
            (RelatedAction::ScreenDrawEmergency, "Ctrl+Shift+E"),
        ] {
            assert!(candidate_policy.related.iter().any(|binding| {
                binding.action == action && binding.hotkey == parse_hotkey(chord).unwrap()
            }));
        }
        assert_eq!(
            serde_json::to_value(&fixture.settings).unwrap(),
            serde_json::to_value(&old_settings).unwrap()
        );
        assert_eq!(fixture.settings_generation, 19);
        assert_eq!(ticket.try_terminal_disposition(), None);
        let restore_result = fixture
            .pending_route
            .as_ref()
            .and_then(|pending| pending.handoff.as_ref())
            .and_then(|handoff| handoff.try_complete())
            .expect("prior route restoration receipt is available");
        let PendingLauncherRoute {
            completion: PendingLauncherRouteCompletion::Hotkey(application),
            ..
        } = fixture.pending_route.take().unwrap()
        else {
            panic!("expected restore completion")
        };
        {
            let mut context = fixture.context();
            assert!(complete_hotkey_route_step(
                *application,
                restore_result,
                &mut context,
                &mut issuer,
            ));
        }

        assert_eq!(
            serde_json::to_value(&fixture.settings).unwrap(),
            serde_json::to_value(&old_settings).unwrap()
        );
        assert_eq!(fixture.settings_generation, 21);
        assert!(!fixture.listener.is_active());
        assert!(fixture.listener.triggers().is_empty());
        assert!(fixture.pending_route.is_none());
        assert!(fixture.deferred_notices_follow_failure);
        assert_eq!(
            ticket.try_terminal_disposition(),
            Some(Ok(
                multi_launcher::hotkey::reconfiguration::HotkeyTerminalDisposition::
                    ConfirmedNotAppliedOrRestored(
                        multi_launcher::hotkey::reconfiguration::HotkeyReconfigurationError::
                            RuntimeTransition("candidate hook failed".into()),
                    ),
            ))
        );
        let restored_directory = multi_launcher::hotkey::directory::Snapshot::from_sources(
            3,
            vec![directory_source(
                &fixture.settings,
                fixture.settings_generation,
                false,
                3,
                fixture.applied_shared_route.as_ref(),
            )],
        );
        let restored_launcher = restored_directory
            .entries()
            .iter()
            .find(|entry| entry.id.as_str() == "main.launcher")
            .expect("the restored launcher route is visible");
        assert_eq!(restored_launcher.chord.as_deref(), Some("Ctrl+F2"));
        assert_eq!(
            restored_launcher.status,
            multi_launcher::hotkey::directory::Status::Active { generation: 21 }
        );
        assert_eq!(
            restored_launcher
                .last_applied
                .as_ref()
                .map(|receipt| (receipt.chord.as_str(), receipt.generation)),
            Some(("Ctrl+F2", 21)),
            "rollback publishes only the exact restored route receipt"
        );
        assert_eq!(issuer.issued, 2);
        let restored_policy = &issuer.policies[1];
        assert!(restored_policy.config.launcher_enabled);
        assert_eq!(
            restored_policy.config.hotkey,
            parse_hotkey("Ctrl+F2").unwrap()
        );
        assert_eq!(restored_policy.config.generation, 21);
        assert_eq!(
            restored_policy.cancellation,
            LifecycleCancellation::SettingsReload
        );
        for (action, chord) in [
            (RelatedAction::Quit, "Ctrl+Q"),
            (RelatedAction::Help, "Ctrl+H"),
            (RelatedAction::ScreenDrawLaunch, "Ctrl+Shift+S"),
            (RelatedAction::ScreenDrawEmergency, "Ctrl+Shift+E"),
        ] {
            assert!(restored_policy.related.iter().any(|binding| {
                binding.action == action && binding.hotkey == parse_hotkey(chord).unwrap()
            }));
        }

        fixture.radial_controller.shutdown();
        fixture.listener.stop();
    }

    #[test]
    fn shared_hook_failure_installs_legacy_controls_before_terminal_error() {
        use multi_launcher::hotkey::reconfiguration::{
            HotkeyReconfigurationError, HotkeyRuntimeMessage,
        };

        let (wake_tx, wake_rx) = channel();
        let receiver =
            multi_launcher::hotkey::reconfiguration::install_main_runtime(wake_tx.clone()).unwrap();
        let old_settings = Settings {
            hotkey: Some("Ctrl+F2".into()),
            quit_hotkey: Some("Ctrl+Q".into()),
            help_hotkey: Some("Ctrl+H".into()),
            radial: multi_launcher::radial::model::RadialFeatureSettings {
                enabled: true,
                shared_tap_hold: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut candidate = old_settings.clone();
        candidate.hotkey = Some("Ctrl+F3".into());
        let radial_document = Arc::new(RadialDocument::starter());
        let old_trigger = Arc::new(HotkeyTrigger::new(parse_hotkey("Ctrl+F2").unwrap()));
        let listener =
            HotkeyTrigger::start_listener(Vec::new(), "old-shared-owner-loss", wake_tx.clone());
        let mut fixture = HotkeyCompletionFixture::new(
            old_settings.clone(),
            old_trigger,
            listener,
            Arc::clone(&radial_document),
            wake_tx.clone(),
            31,
        );

        let mut ticket = multi_launcher::hotkey::reconfiguration::prepare_hotkey_reconfiguration(
            candidate.clone(),
        )
        .unwrap();
        assert_eq!(wake_rx.try_recv(), Ok(()));
        let HotkeyRuntimeMessage::Prepare(request) = receiver.try_recv().unwrap().unwrap() else {
            panic!("expected prepare request")
        };
        let mut prepared_routes = HashMap::new();
        prepare_hotkey_runtime_message(
            request,
            &radial_document,
            31,
            &wake_tx,
            true,
            invocation_priority_owner(Arc::new(ScreenDrawRecoveryBridge::default())),
            &mut prepared_routes,
        );
        let prepared = ticket.wait_prepared(Duration::from_secs(1)).unwrap();
        ticket.apply(candidate).unwrap();
        assert_eq!(wake_rx.try_recv(), Ok(()));
        let HotkeyRuntimeMessage::Apply(request) = receiver.try_recv().unwrap().unwrap() else {
            panic!("expected apply request")
        };
        let prepared_route = prepared_routes.remove(&prepared.request_id).unwrap();
        let fallback_listener = HotkeyTrigger::prepare_listener(
            fixture.listener.triggers().to_vec(),
            "test-hook-failure-old-listener",
            wake_tx.clone(),
        )
        .unwrap();
        let application = PendingHotkeyApplication {
            request: Some(request),
            prepared: prepared_route,
            old_settings: old_settings.clone(),
            old_generation: 31,
            candidate_generation: 32,
            old_service_existed: true,
            old_sequence_routes: Vec::new(),
            old_listener_stopped: false,
            fallback_listener: Some(fallback_listener),
            phase: HotkeyApplyPhase::DrainOldRoute,
            cancellation_requested: false,
            failure: None,
            restore_generation: None,
        };

        fixture
            .context()
            .runtime
            .finish_after_authoritative_hook_failure(application);

        assert_eq!(fixture.settings.hotkey, old_settings.hotkey);
        assert!(!fixture.settings.radial.enabled);
        assert!(fixture.listener.is_active());
        let trigger_keys = fixture
            .listener
            .triggers()
            .iter()
            .map(|trigger| trigger._key)
            .collect::<Vec<_>>();
        for chord in ["Ctrl+F2", "Ctrl+Q", "Ctrl+H"] {
            assert!(trigger_keys.contains(&parse_hotkey(chord).unwrap().key));
        }
        assert!(matches!(
            ticket.try_terminal_disposition(),
            Some(Ok(
                multi_launcher::hotkey::reconfiguration::HotkeyTerminalDisposition::
                    ConfirmedNotAppliedOrRestored(
                        HotkeyReconfigurationError::RuntimeTransition(message)
                    )
            ))
                if message.contains("prior legacy launcher and process-control routes were restored")
        ));
        assert!(fixture.listener.is_active());

        fixture.radial_controller.shutdown();
        fixture.listener.stop();
    }

    #[test]
    fn unsupported_legacy_control_does_not_reject_a_shared_route() {
        let settings = Settings {
            hotkey: Some("Ctrl+F2".into()),
            quit_hotkey: Some("F13".into()),
            radial: multi_launcher::radial::model::RadialFeatureSettings {
                enabled: true,
                shared_tap_hold: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let document = RadialDocument::starter();

        let routes = validate_hotkey_route_candidate(&settings, &document)
            .expect("the shared route supports the configured control key");
        assert!(routes.shared_invocation);

        let (event_tx, _event_rx) = channel();
        assert!(prepare_legacy_hotkey_fallback(&settings, &document, &[], &event_tx).is_err());
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

    fn admitted_recovery_notices(bridge: &ScreenDrawRecoveryBridge) -> Vec<ServiceNotice> {
        use multi_launcher::hotkey::launcher_invocation::{
            InvocationConfig, KeyEvent, KeyTransition, LauncherInvocationAdapter,
        };
        use multi_launcher::radial::invocation::InputProvenance;
        let mut adapter = LauncherInvocationAdapter::new(InvocationConfig {
            launcher_enabled: true,
            hotkey: parse_hotkey("Shift+Alt+Win+End").unwrap(),
            threshold_ms: 350,
            generation: 4,
            context_token: 9,
            menu_id: multi_launcher::radial::model::MenuId::new("starter"),
            interaction: multi_launcher::radial::model::InteractionMode::StickyClick,
            accept_external_injected: true,
            item_inputs: Vec::new(),
        })
        .unwrap();
        let key = |vk, transition, at| KeyEvent {
            vk,
            transition,
            at,
            provenance: InputProvenance::Physical,
        };
        for vk in [0xA0, 0xA4, 0x5B] {
            adapter.process(key(vk, KeyTransition::Down, 0), PriorityOwner::Launcher);
        }
        let mut notices = Vec::new();
        for cycle in 0..2 {
            let down = adapter.process(
                key(0x23, KeyTransition::Down, cycle * 10 + 1),
                PriorityOwner::ScreenDrawRecovery(bridge.active_lifetime().unwrap()),
            );
            assert!(down.consume && down.recovery.is_some());
            notices.push(down.into());
            let repeat = adapter.process(
                key(0x23, KeyTransition::Repeat, cycle * 10 + 2),
                PriorityOwner::ScreenDrawRecovery(bridge.active_lifetime().unwrap()),
            );
            assert!(repeat.consume && repeat.recovery.is_none());
            let up = adapter.process(
                key(0x23, KeyTransition::Up, cycle * 10 + 3),
                PriorityOwner::ScreenDrawRecovery(bridge.active_lifetime().unwrap()),
            );
            assert!(up.consume && up.recovery.is_none());
        }
        notices
    }

    #[test]
    fn actual_recovery_notices_preserve_each_admission_through_main_trigger_consumption() {
        let bridge = ScreenDrawRecoveryBridge::default();
        bridge.stage_start();
        let launcher = HotkeyTrigger::new(parse_hotkey("Shift+Alt+Win+End").unwrap());
        let mut notices = admitted_recovery_notices(&bridge);
        let admitted = notices
            .iter()
            .map(|notice| notice.recovery.unwrap())
            .collect::<Vec<_>>();
        let actions =
            take_screen_draw_trigger_actions(&launcher, None, None, &bridge, &mut notices);
        assert!(!actions.launch && actions.emergency.is_none());
        assert_eq!(actions.recover.len(), 2);
        assert_eq!(
            actions
                .recover
                .iter()
                .map(|intent| intent.admission.unwrap())
                .collect::<Vec<_>>(),
            admitted
        );
        assert_ne!(actions.recover[0].id(), actions.recover[1].id());
        assert!(
            actions
                .recover
                .iter()
                .all(|intent| intent.activity_epoch() == bridge.activity_epoch())
        );
        assert!(
            notices
                .iter()
                .all(|notice| notice.recovery.is_none() && notice.intents.is_empty())
        );
        let repeated =
            take_screen_draw_trigger_actions(&launcher, None, None, &bridge, &mut notices);
        assert!(repeated.recover.is_empty());
        assert!(!launcher.take());
    }

    #[test]
    fn emergency_defensive_cofire_keeps_separately_admitted_recoveries_and_owns_one_emergency() {
        let bridge = ScreenDrawRecoveryBridge::default();
        bridge.stage_start();
        let launcher = HotkeyTrigger::new(parse_hotkey("F12").unwrap());
        let launch = HotkeyTrigger::new(parse_hotkey("Ctrl+F12").unwrap());
        let emergency = HotkeyTrigger::new(parse_hotkey("Shift+F12").unwrap());
        for trigger in [&launcher, &launch, &emergency] {
            *trigger.open.lock().unwrap() = true;
        }
        let mut notices = admitted_recovery_notices(&bridge);
        let actions = take_screen_draw_trigger_actions(
            &launcher,
            Some(&launch),
            Some(&emergency),
            &bridge,
            &mut notices,
        );
        assert!(!actions.launch);
        assert_eq!(actions.recover.len(), 2);
        let emergency_intent = actions.emergency.unwrap();
        assert_eq!(emergency_intent.kind, ScreenDrawRecoveryKind::Emergency);
        assert!(
            actions
                .recover
                .iter()
                .all(|intent| intent.id() != emergency_intent.id())
        );
        assert!(!launcher.take() && !launch.take() && !emergency.take());
    }

    #[test]
    fn dispatched_screen_draw_cycle_preserves_each_intent_and_only_clears_its_cycle_notices() {
        use multi_launcher::gui::WatchEvent;
        let bridge = Arc::new(ScreenDrawRecoveryBridge::default());
        bridge.stage_start();
        let launcher = HotkeyTrigger::new(parse_hotkey("F12").unwrap());
        let emergency = HotkeyTrigger::new(parse_hotkey("Shift+F12").unwrap());
        *emergency.open.lock().unwrap() = true;
        let mut notices = admitted_recovery_notices(&bridge);
        let actions = take_screen_draw_trigger_actions(
            &launcher,
            None,
            Some(&emergency),
            &bridge,
            &mut notices,
        );
        let recovery_ids = actions
            .recover
            .iter()
            .map(|intent| intent.id())
            .collect::<Vec<_>>();
        let emergency_id = actions.emergency.unwrap().id();
        let mut emitted = Vec::new();
        let handled =
            dispatch_screen_draw_trigger_actions(actions, &bridge, |event| emitted.push(event));
        assert!(
            handled,
            "handled must be captured before the recovery vector is consumed"
        );
        assert!(
            matches!(emitted.first(), Some(WatchEvent::ScreenDrawEmergency(intent)) if intent.id() == emergency_id)
        );
        assert_eq!(
            emitted
                .iter()
                .filter_map(|event| match event {
                    WatchEvent::ScreenDrawRecover(intent) => Some(intent.id()),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            recovery_ids
        );
        assert_eq!(emitted.len(), 3);
        reconcile_screen_draw_cycle_notices(handled, false, &mut notices);
        assert!(notices.is_empty());

        let make_notice = || ServiceNotice {
            recovery: None,
            intents: vec![
                multi_launcher::radial::invocation::InvocationIntent::ToggleLegacyLauncher {
                    id: InvocationId(9),
                },
                multi_launcher::radial::invocation::InvocationIntent::CancelDeadline {
                    id: InvocationId(10),
                },
            ],
            error: None,
            action: None,
            cancellation: None,
        };
        let mut later_notices = vec![make_notice()];
        let idle = dispatch_screen_draw_trigger_actions(
            ScreenDrawTriggerActions::default(),
            &bridge,
            |_| panic!("idle cycle emitted an event"),
        );
        assert!(!idle);
        reconcile_screen_draw_cycle_notices(idle, false, &mut later_notices);
        assert_eq!(
            later_notices[0].intents.len(),
            2,
            "unrelated cycle must remain owned by radial routing"
        );
        reconcile_screen_draw_cycle_notices(idle, true, &mut later_notices);
        assert!(matches!(
            later_notices[0].intents.as_slice(),
            [
                multi_launcher::radial::invocation::InvocationIntent::CancelDeadline {
                    id: InvocationId(10)
                }
            ]
        ));

        let launch = HotkeyTrigger::new(parse_hotkey("Ctrl+Shift+D").unwrap());
        *launch.open.lock().unwrap() = true;
        *launcher.open.lock().unwrap() = true;
        let actions =
            take_screen_draw_trigger_actions(&launcher, Some(&launch), None, &bridge, &mut []);
        emitted.clear();
        let handled =
            dispatch_screen_draw_trigger_actions(actions, &bridge, |event| emitted.push(event));
        assert!(
            handled
                && matches!(
                    emitted.as_slice(),
                    [
                        WatchEvent::ScreenDrawStart,
                        WatchEvent::ScreenDrawRecover(_)
                    ]
                )
        );
        let mut cofire = vec![make_notice()];
        reconcile_screen_draw_cycle_notices(handled, false, &mut cofire);
        assert!(cofire.is_empty());
        multi_launcher::hotkey::launcher_invocation::set_exclusive_owner(
            multi_launcher::hotkey::launcher_invocation::ExclusiveOwner::ScreenDraw,
            false,
        );
    }

    #[test]
    fn same_loop_launch_then_summon_routes_recovery_without_visibility() {
        let bridge = ScreenDrawRecoveryBridge::default();
        let launcher = HotkeyTrigger::new(parse_hotkey("F2").unwrap());
        let launch = HotkeyTrigger::new(parse_hotkey("Ctrl+Shift+D").unwrap());
        *launcher.open.lock().unwrap() = true;
        *launch.open.lock().unwrap() = true;

        let actions =
            take_screen_draw_trigger_actions(&launcher, Some(&launch), None, &bridge, &mut []);

        assert!(actions.launch);
        assert_eq!(actions.recover.len(), 1);
        assert_eq!(
            actions.recover[0].kind,
            ScreenDrawRecoveryKind::LauncherToggle
        );
        assert_eq!(actions.recover[0].admission, None);
        assert_eq!(actions.emergency, None);
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

        let actions = take_screen_draw_trigger_actions(
            &launcher,
            Some(&launch),
            Some(&emergency),
            &bridge,
            &mut [],
        );
        assert!(!actions.launch);
        assert!(actions.recover.is_empty());
        assert_eq!(
            actions.emergency.unwrap().kind,
            ScreenDrawRecoveryKind::Emergency
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
            recovery: None,
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

#[cfg(test)]
mod hotkey_directory_runtime_tests {
    use super::*;
    use multi_launcher::hotkey::directory::{
        CandidateState, Owner, Reason, RouteKind, Snapshot, Source, Status,
    };

    fn main_source(
        settings: &Settings,
        radial: &RadialDocument,
        macro_document: &multi_launcher::mkmacro::MkMacroDocument,
        generation: u64,
        route_pending: bool,
        active_triggers: &[multi_launcher::hotkey::Hotkey],
    ) -> multi_launcher::hotkey::directory::SourceSnapshot {
        hotkey_directory_source_snapshots(
            settings,
            radial,
            None,
            &[],
            None,
            &[],
            Some(macro_document),
            1,
            active_triggers,
            None,
            generation,
            false,
            route_pending,
            generation,
        )
        .into_iter()
        .find(|source| source.source == Source::MainSettings)
        .expect("main Settings source is always published")
    }

    fn main_source_with_shared_evidence(
        settings: &Settings,
        radial: &RadialDocument,
        macro_document: &multi_launcher::mkmacro::MkMacroDocument,
        generation: u64,
        route_pending: bool,
        active_triggers: &[multi_launcher::hotkey::Hotkey],
        evidence: Option<&AppliedSharedRouteEvidence>,
    ) -> multi_launcher::hotkey::directory::SourceSnapshot {
        hotkey_directory_source_snapshots(
            settings,
            radial,
            None,
            &[],
            None,
            &[],
            Some(macro_document),
            1,
            active_triggers,
            evidence,
            generation,
            false,
            route_pending,
            generation,
        )
        .into_iter()
        .find(|source| source.source == Source::MainSettings)
        .expect("main Settings source is always published")
    }

    #[test]
    fn hotkey_directory_runtime_requires_an_exact_applied_listener_receipt() {
        use multi_launcher::hotkey::parse_hotkey;

        let mut settings = Settings::default();
        settings.hotkey = Some("F2".into());
        settings.radial.enabled = false;
        let radial = RadialDocument::default();
        let macros = multi_launcher::mkmacro::MkMacroDocument::default();
        let f2 = parse_hotkey("F2").expect("valid test chord");
        let f3 = parse_hotkey("F3").expect("valid test chord");

        let initial = Snapshot::from_sources(
            1,
            vec![main_source(&settings, &radial, &macros, 10, false, &[f2])],
        );
        let launcher = initial
            .entries()
            .iter()
            .find(|entry| entry.id.as_str() == "main.launcher")
            .expect("launcher candidate exists");
        assert_eq!(launcher.status, Status::Active { generation: 10 });

        settings.hotkey = Some("F3".into());
        let unacknowledged = initial
            .replacing_source(
                2,
                main_source(&settings, &radial, &macros, 11, false, &[f2]),
            )
            .expect("new source revision replaces the prior source");
        let launcher = unacknowledged
            .entries()
            .iter()
            .find(|entry| entry.id.as_str() == "main.launcher")
            .expect("launcher candidate remains visible");
        assert_eq!(launcher.status, Status::Unknown);
        assert!(launcher.last_applied.is_none());

        let pending = initial
            .replacing_source(3, main_source(&settings, &radial, &macros, 12, true, &[f3]))
            .expect("prepared source revision replaces the prior source");
        let launcher = pending
            .entries()
            .iter()
            .find(|entry| entry.id.as_str() == "main.launcher")
            .expect("launcher candidate remains visible while pending");
        assert_eq!(launcher.status, Status::Pending { generation: 12 });
        assert_eq!(launcher.chord.as_deref(), Some("F3"));
        assert_eq!(
            launcher
                .last_applied
                .as_ref()
                .map(|receipt| receipt.chord.as_str()),
            Some("F2"),
            "the prior applied chord remains evidence, not proof of the pending replacement"
        );
    }

    #[test]
    fn hotkey_directory_runtime_shared_route_requires_exact_applied_evidence() {
        use multi_launcher::hotkey::parse_hotkey;

        let mut settings = Settings::default();
        settings.radial.enabled = true;
        settings.radial.shared_tap_hold = true;
        settings.help_hotkey = Some("Ctrl+F3".into());
        let radial = RadialDocument::starter();
        let macros = multi_launcher::mkmacro::MkMacroDocument::default();
        let generation = 31;

        let unacknowledged = Snapshot::from_sources(
            1,
            vec![main_source_with_shared_evidence(
                &settings,
                &radial,
                &macros,
                generation,
                false,
                &[],
                None,
            )],
        );
        let launcher = unacknowledged
            .entries()
            .iter()
            .find(|entry| entry.id.as_str() == "main.launcher")
            .unwrap();
        assert_eq!(launcher.status, Status::Unknown);

        let evidence =
            applied_shared_route_after_successful_install(&settings, &radial, &[], generation)
                .expect("successful shared route installation records exact evidence");
        let acknowledged = Snapshot::from_sources(
            2,
            vec![main_source_with_shared_evidence(
                &settings,
                &radial,
                &macros,
                generation,
                false,
                &[],
                Some(&evidence),
            )],
        );
        let launcher = acknowledged
            .entries()
            .iter()
            .find(|entry| entry.id.as_str() == "main.launcher")
            .unwrap();
        assert_eq!(launcher.status, Status::Active { generation });

        settings.help_hotkey = Some("Ctrl+F4".into());
        let stale = Snapshot::from_sources(
            3,
            vec![main_source_with_shared_evidence(
                &settings,
                &radial,
                &macros,
                generation,
                false,
                &[],
                Some(&evidence),
            )],
        );
        let launcher = stale
            .entries()
            .iter()
            .find(|entry| entry.id.as_str() == "main.launcher")
            .unwrap();
        assert_eq!(launcher.status, Status::Unknown);
        assert!(launcher.last_applied.is_none());

        let legacy_generation = generation + 1;
        let mut legacy_settings = settings.clone();
        legacy_settings.radial.enabled = false;
        legacy_settings.radial.shared_tap_hold = false;
        let legacy_triggers = [
            legacy_settings
                .hotkey
                .as_deref()
                .and_then(parse_hotkey)
                .unwrap(),
            legacy_settings
                .help_hotkey
                .as_deref()
                .and_then(parse_hotkey)
                .unwrap(),
        ];
        let legacy = Snapshot::from_sources(
            4,
            vec![main_source_with_shared_evidence(
                &legacy_settings,
                &radial,
                &macros,
                legacy_generation,
                false,
                &legacy_triggers,
                None,
            )],
        );
        let launcher = legacy
            .entries()
            .iter()
            .find(|entry| entry.id.as_str() == "main.launcher")
            .unwrap();
        assert_eq!(
            launcher.status,
            Status::Active {
                generation: legacy_generation
            },
            "a published transition to the legacy listener reports its current acknowledgement"
        );

        let reenabled_generation = legacy_generation + 1;
        let reenabled_without_fresh_receipt = Snapshot::from_sources(
            5,
            vec![main_source_with_shared_evidence(
                &settings,
                &radial,
                &macros,
                reenabled_generation,
                false,
                &[],
                Some(&evidence),
            )],
        );
        let launcher = reenabled_without_fresh_receipt
            .entries()
            .iter()
            .find(|entry| entry.id.as_str() == "main.launcher")
            .unwrap();
        assert_eq!(launcher.status, Status::Unknown);
        assert!(launcher.last_applied.is_none());

        let fresh_evidence = applied_shared_route_after_successful_install(
            &settings,
            &radial,
            &[],
            reenabled_generation,
        )
        .expect("re-enabled shared route receives new exact runtime evidence");
        let reenabled = Snapshot::from_sources(
            6,
            vec![main_source_with_shared_evidence(
                &settings,
                &radial,
                &macros,
                reenabled_generation,
                false,
                &[],
                Some(&fresh_evidence),
            )],
        );
        let launcher = reenabled
            .entries()
            .iter()
            .find(|entry| entry.id.as_str() == "main.launcher")
            .unwrap();
        assert_eq!(
            launcher.status,
            Status::Active {
                generation: reenabled_generation
            }
        );
        assert_eq!(
            launcher
                .last_applied
                .as_ref()
                .map(|receipt| receipt.generation),
            Some(reenabled_generation)
        );
    }

    #[test]
    fn hotkey_directory_runtime_accepted_radial_and_macro_publications_refresh_rows() {
        use multi_launcher::hotkey::directory::{Reason, Snapshot, Source, Status};
        use multi_launcher::mkmacro::{
            DocumentPublicationObserver, MkHotkey, MkHotkeyScope, MkKey, MkMacro, MkMacroDocument,
            MkMacroStore,
        };
        use multi_launcher::radial::model::{TriggerDefinition, TriggerId, TriggerScope};
        use multi_launcher::radial::store::{ExternalReloadOutcome, RadialStore};
        use std::sync::atomic::{AtomicU64, Ordering};

        let mut settings = Settings::default();
        settings.radial.enabled = true;
        settings.radial.shared_tap_hold = true;
        let macros = MkMacroDocument::default();
        let starter = RadialDocument::starter();
        let radial_directory = tempfile::tempdir().unwrap();
        let radial_path = radial_directory.path().join("radial.json");
        let runtime_store = RadialStore::at_path(&radial_path, starter.clone()).unwrap();
        let authoring_store = RadialStore::at_path(&radial_path, starter.clone()).unwrap();
        let mut radial_a = starter;
        radial_a.custom_triggers.push(TriggerDefinition {
            id: TriggerId::new("directory-reload"),
            chord: "Ctrl+F2".into(),
            menu_id: radial_a.default_menu_id.clone(),
            scope: TriggerScope::Global,
        });
        authoring_store
            .save(authoring_store.snapshot().unwrap().revision, radial_a)
            .unwrap();
        let ExternalReloadOutcome::Published(radial_a) =
            runtime_store.reload_external_with(|_| Ok(())).unwrap()
        else {
            panic!("the initial external Radial document is accepted")
        };
        let radial_sources = hotkey_directory_source_snapshots(
            &settings,
            &radial_a,
            None,
            &[],
            None,
            &[],
            Some(&macros),
            1,
            &[],
            None,
            7,
            false,
            false,
            1,
        );
        let radial_source_a = radial_sources
            .into_iter()
            .find(|source| source.source == Source::Radial)
            .unwrap();
        let radial_a_snapshot = Snapshot::from_sources(1, vec![radial_source_a.clone()]);
        let radial_entry_a = radial_a_snapshot
            .entries()
            .iter()
            .find(|entry| entry.id.as_str() == "radial.trigger.directory-reload")
            .unwrap();
        assert_eq!(radial_entry_a.chord.as_deref(), Some("Ctrl+F2"));
        assert_eq!(radial_entry_a.status, Status::Unknown);

        let mut radial_b = (*radial_a).clone();
        radial_b.custom_triggers[0].chord = "Ctrl+F3".into();
        authoring_store
            .save(authoring_store.snapshot().unwrap().revision, radial_b)
            .unwrap();
        let ExternalReloadOutcome::Published(radial_b) =
            runtime_store.reload_external_with(|_| Ok(())).unwrap()
        else {
            panic!("the changed external Radial document is accepted")
        };
        let radial_sources = hotkey_directory_source_snapshots(
            &settings,
            &radial_b,
            None,
            &[],
            None,
            &[],
            Some(&macros),
            1,
            &[],
            None,
            7,
            false,
            false,
            2,
        );
        let radial_source_b = radial_sources
            .into_iter()
            .find(|source| source.source == Source::Radial)
            .unwrap();
        let radial_b_snapshot = radial_a_snapshot
            .replacing_source(2, radial_source_b)
            .expect("an accepted Radial revision replaces the displayed source");
        let radial_entry_b = radial_b_snapshot
            .entries()
            .iter()
            .find(|entry| entry.id.as_str() == "radial.trigger.directory-reload")
            .unwrap();
        assert_eq!(radial_entry_b.id, radial_entry_a.id);
        assert_eq!(radial_entry_b.source_revision, radial_b.revision.0);
        assert_eq!(radial_entry_b.chord.as_deref(), Some("Ctrl+F3"));
        assert_eq!(radial_entry_b.status, Status::Unknown);
        assert!(
            radial_b_snapshot
                .replacing_source(3, radial_source_a)
                .is_none(),
            "a delayed lower Radial revision cannot roll back the accepted reload"
        );

        let macro_directory = tempfile::tempdir().unwrap();
        let (macro_store, _) = MkMacroStore::open(macro_directory.path()).unwrap();
        let macro_revision = Arc::new(AtomicU64::new(0));
        let after_revision = Arc::clone(&macro_revision);
        macro_store.set_publication_observer(Arc::new(DocumentPublicationObserver {
            before: Arc::new(|| {}),
            after: Arc::new(move || {
                after_revision.fetch_add(1, Ordering::SeqCst);
            }),
        }));
        let revision_before_save = macro_revision.load(Ordering::SeqCst);
        let mut macro_document = MkMacroDocument::default();
        macro_document.macros.push(MkMacro {
            id: 901,
            name: "Directory publication test".into(),
            description: String::new(),
            enabled: true,
            hotkey: Some(MkHotkey {
                key: MkKey::Function(8),
                modifiers: Vec::new(),
            }),
            hotkey_scope: MkHotkeyScope::AnyWindow,
            folder_id: None,
            playback: Default::default(),
            signature: Default::default(),
            steps: Vec::new(),
        });
        let accepted_macro_document = macro_store.save(macro_document).unwrap();
        let accepted_macro_revision = macro_revision.load(Ordering::SeqCst);
        assert!(accepted_macro_revision > revision_before_save);
        let macro_sources = hotkey_directory_source_snapshots(
            &settings,
            &radial_b,
            None,
            &[],
            None,
            &[],
            Some(&accepted_macro_document),
            accepted_macro_revision,
            &[],
            None,
            7,
            false,
            false,
            3,
        );
        let macro_source = macro_sources
            .into_iter()
            .find(|source| source.source == Source::Macros)
            .unwrap();
        let macro_snapshot = radial_b_snapshot
            .replacing_source(3, macro_source)
            .expect("the accepted macro publication advances its source");
        let macro_entry = macro_snapshot
            .entries()
            .iter()
            .find(|entry| entry.id.as_str() == "macro.playback.901")
            .unwrap();
        assert_eq!(macro_entry.source_revision, accepted_macro_revision);
        assert_eq!(macro_entry.action_label, "Directory publication test");
        assert_eq!(macro_entry.status, Status::Unknown);
        assert!(macro_entry.last_applied.is_none());

        let mut disabled_settings = settings.clone();
        disabled_settings.enabled_plugins = Some(std::collections::HashSet::new());
        let disabled_macro_source = hotkey_directory_source_snapshots(
            &disabled_settings,
            &radial_b,
            None,
            &[],
            None,
            &[],
            Some(&accepted_macro_document),
            accepted_macro_revision,
            &[],
            None,
            7,
            false,
            false,
            4,
        )
        .into_iter()
        .find(|source| source.source == Source::Macros)
        .unwrap();
        let disabled_snapshot = macro_snapshot
            .replacing_source(4, disabled_macro_source)
            .expect("the disabled owner selection is a new source publication");
        let disabled_entry = disabled_snapshot
            .entries()
            .iter()
            .find(|entry| entry.id.as_str() == "macro.playback.901")
            .unwrap();
        assert_eq!(disabled_entry.status, Status::Unavailable);
        assert_eq!(disabled_entry.reason, Some(Reason::OwnerDisabled));
        assert!(disabled_entry.last_applied.is_none());

        let mut reenabled_settings = disabled_settings;
        reenabled_settings.enabled_plugins =
            Some(std::collections::HashSet::from(["mkmacro".to_owned()]));
        let reenabled_macro_source = hotkey_directory_source_snapshots(
            &reenabled_settings,
            &radial_b,
            None,
            &[],
            None,
            &[],
            Some(&accepted_macro_document),
            accepted_macro_revision,
            &[],
            None,
            7,
            false,
            false,
            5,
        )
        .into_iter()
        .find(|source| source.source == Source::Macros)
        .unwrap();
        let reenabled_snapshot = disabled_snapshot
            .replacing_source(5, reenabled_macro_source)
            .expect("re-enabling publishes the current owner state");
        let reenabled_entry = reenabled_snapshot
            .entries()
            .iter()
            .find(|entry| entry.id.as_str() == "macro.playback.901")
            .unwrap();
        assert_eq!(reenabled_entry.status, Status::Unknown);
        assert_eq!(reenabled_entry.reason, Some(Reason::RouteNotAcknowledged));
        assert!(reenabled_entry.last_applied.is_none());
    }

    #[test]
    fn hotkey_directory_runtime_macro_diagnostics_omit_private_matcher_payloads() {
        use multi_launcher::mkmacro::{MkHotkey, MkHotkeyScope, MkKey, MkMacro, MkMacroDocument};

        let mut settings = Settings::default();
        settings.radial.enabled = false;
        settings.hotkey = Some("F2".into());
        let radial = RadialDocument::default();
        let mut macros = MkMacroDocument::default();
        let macro_with_hotkey =
            |id: u64, name: &str, key: u8, hotkey_scope: MkHotkeyScope| MkMacro {
                id,
                name: name.into(),
                description: String::new(),
                enabled: true,
                hotkey: Some(MkHotkey {
                    key: MkKey::Function(key),
                    modifiers: Vec::new(),
                }),
                hotkey_scope,
                folder_id: None,
                playback: Default::default(),
                signature: Default::default(),
                steps: Vec::new(),
            };
        macros.macros = vec![
            macro_with_hotkey(201, "Reserved macro", 2, MkHotkeyScope::AnyWindow),
            macro_with_hotkey(
                202,
                "Invalid matcher",
                3,
                MkHotkeyScope::ActiveWindow(multi_launcher::mkmacro::MkWindowMatcher {
                    title_regex: Some("private-window-pattern [".into()),
                    ..Default::default()
                }),
            ),
            macro_with_hotkey(203, "Duplicate macro one", 5, MkHotkeyScope::AnyWindow),
            macro_with_hotkey(204, "Duplicate macro two", 5, MkHotkeyScope::AnyWindow),
        ];
        let sources = hotkey_directory_source_snapshots(
            &settings,
            &radial,
            None,
            &[],
            None,
            &[],
            Some(&macros),
            9,
            &[],
            None,
            4,
            false,
            false,
            9,
        );
        let macro_source = sources
            .iter()
            .find(|source| source.source == Source::Macros)
            .unwrap();
        let reserved = macro_source
            .candidates
            .iter()
            .find(|candidate| candidate.id.as_str() == "macro.playback.201")
            .unwrap();
        assert_eq!(reserved.owner, Owner::MkMacro);
        assert_eq!(reserved.state, CandidateState::Conflict);
        assert_eq!(reserved.reason, Some(Reason::ConflictsWithConfiguredRoute));
        let duplicate = macro_source
            .candidates
            .iter()
            .find(|candidate| candidate.id.as_str() == "macro.playback.203")
            .unwrap();
        assert_eq!(duplicate.state, CandidateState::Conflict);
        assert_eq!(duplicate.reason, Some(Reason::ConflictsWithConfiguredRoute));
        let invalid_matcher = macro_source
            .candidates
            .iter()
            .find(|candidate| candidate.id.as_str() == "macro.playback.202")
            .unwrap();
        assert_eq!(invalid_matcher.state, CandidateState::Invalid);
        assert_eq!(invalid_matcher.reason, Some(Reason::InvalidScope));
        assert!(
            !format!("{macro_source:?}").contains("private-window-pattern"),
            "matcher text is private macro data and never enters the directory snapshot"
        );
    }

    #[test]
    fn hotkey_directory_runtime_paste_revision_and_route_metadata_are_current() {
        let (wake, _events) = channel();
        let listener = HotkeyTrigger::start_listener(Vec::new(), "hotkey-directory-test", wake);
        let mut settings = Settings::default();
        settings.radial.enabled = false;
        settings.radial.shared_tap_hold = false;
        let radial = RadialDocument::default();
        let before = hotkey_directory_publication_key(
            1,
            &radial,
            10,
            2,
            None,
            &listener,
            None,
            None,
            &settings,
            &[],
        );
        let after = hotkey_directory_publication_key(
            1,
            &radial,
            11,
            2,
            None,
            &listener,
            None,
            None,
            &settings,
            &[],
        );
        assert_ne!(
            before, after,
            "accepted sequence edits trigger republishing"
        );

        let macros = multi_launcher::mkmacro::MkMacroDocument::default();
        let legacy_sources = hotkey_directory_source_snapshots(
            &settings,
            &radial,
            None,
            &[],
            None,
            &[],
            Some(&macros),
            2,
            &[],
            None,
            1,
            false,
            false,
            1,
        );
        let screen_draw = legacy_sources
            .iter()
            .find(|source| source.source == Source::ScreenDraw)
            .unwrap();
        assert!(
            screen_draw.candidates.iter().all(|candidate| {
                candidate.owner == Owner::ScreenDraw && candidate.edit.is_none()
            })
        );
        let paste_next = legacy_sources
            .iter()
            .find(|source| source.source == Source::PasteSequences)
            .unwrap()
            .candidates
            .iter()
            .find(|candidate| candidate.id.as_str() == "paste.next")
            .unwrap();
        assert_eq!(paste_next.route, RouteKind::LegacyPolling);

        let mut shared_settings = settings;
        shared_settings.radial.enabled = true;
        shared_settings.radial.shared_tap_hold = true;
        let shared_sources = hotkey_directory_source_snapshots(
            &shared_settings,
            &radial,
            None,
            &[],
            None,
            &[],
            Some(&macros),
            2,
            &[],
            None,
            1,
            false,
            false,
            1,
        );
        let paste_next = shared_sources
            .iter()
            .find(|source| source.source == Source::PasteSequences)
            .unwrap()
            .candidates
            .iter()
            .find(|candidate| candidate.id.as_str() == "paste.next")
            .unwrap();
        assert_eq!(paste_next.route, RouteKind::SharedHook);
        assert_eq!(paste_next.state, CandidateState::Configured);
        assert!(
            !shared_sources
                .iter()
                .find(|source| source.source == Source::MainSettings)
                .unwrap()
                .applied_routes
                .iter()
                .any(|receipt| receipt.id.as_str() == "main.launcher"),
            "shared mode without a receipt must not claim the hook is applied"
        );
    }
}
