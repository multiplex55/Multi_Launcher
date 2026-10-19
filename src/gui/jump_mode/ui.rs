//! Frame-scoped egui target collection and safe focus handoff for Jump Mode.
//!
//! The bridge stores only copied IDs and geometry. A renderer keeps its
//! `Response` for the current call, applies any returned focus directive, and
//! then drops it normally.

use super::{
    HintMap, HintSession, MatchOutcome, SemanticTargetId, SessionLifecycle, SurfaceId,
    TargetGeneration, TargetId, TargetKind, TargetMetadata,
};
use eframe::egui;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct NavigationEpoch(pub(crate) u64);

impl NavigationEpoch {
    fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }
}

/// A generic renderer instruction. `Select` deliberately leaves selection to
/// the owning component and neither directive invokes an action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FocusDirective {
    Focus,
    Select,
}

impl FocusDirective {
    pub(crate) fn apply_to_response(self, response: &egui::Response) {
        if self == Self::Focus {
            response.request_focus();
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RegisteredTarget {
    metadata: TargetMetadata,
    viewport: egui::ViewportId,
    response_id: egui::Id,
    layer_id: egui::LayerId,
    rect: egui::Rect,
    clipped_rect: egui::Rect,
    enabled: bool,
    eligible: bool,
    epoch: NavigationEpoch,
}

impl RegisteredTarget {
    pub(crate) fn metadata(&self) -> &TargetMetadata {
        &self.metadata
    }

    pub(crate) fn id(&self) -> &TargetId {
        self.metadata.id()
    }

    pub(crate) fn viewport(&self) -> egui::ViewportId {
        self.viewport
    }

    pub(crate) fn response_id(&self) -> egui::Id {
        self.response_id
    }

    pub(crate) fn layer_id(&self) -> egui::LayerId {
        self.layer_id
    }

    pub(crate) fn rect(&self) -> egui::Rect {
        self.rect
    }

    pub(crate) fn clipped_rect(&self) -> egui::Rect {
        self.clipped_rect
    }

    pub(crate) fn enabled(&self) -> bool {
        self.enabled
    }

    pub(crate) fn eligible(&self) -> bool {
        self.eligible
    }

    pub(crate) fn navigation_epoch(&self) -> NavigationEpoch {
        self.epoch
    }
}

/// An immutable snapshot of the eligible targets from one completed UI pass.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct TargetRegistry {
    targets: Vec<RegisteredTarget>,
}

impl TargetRegistry {
    pub(crate) fn targets(&self) -> &[RegisteredTarget] {
        &self.targets
    }

    pub(crate) fn metadata(&self) -> impl Iterator<Item = &TargetMetadata> {
        self.targets.iter().map(RegisteredTarget::metadata)
    }

    pub(crate) fn get(&self, id: &TargetId) -> Option<&RegisteredTarget> {
        self.targets.iter().find(|target| target.id() == id)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.targets.is_empty()
    }

    pub(crate) fn len(&self) -> usize {
        self.targets.len()
    }

    fn prefer_active_surface(&mut self) {
        let Some(priority) = self
            .targets
            .iter()
            .map(|target| surface_priority(target.id().surface()))
            .max()
        else {
            return;
        };

        // Dashboard and launcher results share the root owner, so both may be
        // collected in one frame. Other owners at the same priority are not
        // safely orderable here (for example, two open dialogs or nested
        // popups). Fail closed instead of letting both surfaces compete.
        let active_non_root_surfaces: HashSet<_> = self
            .targets
            .iter()
            .filter(|target| {
                surface_priority(target.id().surface()) == priority
                    && !is_root_surface(target.id().surface())
            })
            .map(|target| target.id().surface())
            .collect();
        if active_non_root_surfaces.len() > 1 {
            self.targets.clear();
            return;
        }

        self.targets
            .retain(|target| surface_priority(target.id().surface()) == priority);
    }

    fn revalidate(&mut self, ctx: &egui::Context, root_owner_allowed: bool) {
        let root_focused = ctx.input(|input| input.focused);
        let root_viewport = ctx.viewport_id() == egui::ViewportId::ROOT;
        let popup_open = egui::Popup::is_any_open(ctx);
        self.targets.retain(|target| {
            let root_surface = is_root_surface(target.id().surface());
            root_focused
                && root_viewport
                && target.viewport == egui::ViewportId::ROOT
                && target.eligible
                && target.enabled
                && ctx.memory(|memory| memory.allows_interaction(target.layer_id))
                && (!root_surface || (root_owner_allowed && !popup_open))
                && (!popup_open || is_popup_surface(target.id().surface()))
        });
        self.prefer_active_surface();
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FocusIntent {
    target: TargetId,
    viewport: egui::ViewportId,
    epoch: NavigationEpoch,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ConsumedFocusIntent {
    intent: FocusIntent,
    response_id: egui::Id,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RegistrationRejection {
    Disabled,
    NonInteractive,
    EmptyOrClipped,
    BlockedLayer,
    WrongViewport,
    UnfocusedViewport,
    InactiveOwner,
    IdentityOutsideScope,
    DuplicateIdentity,
    FinishedPass,
    CollectorUnavailable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RegistrationResult {
    Registered {
        id: TargetId,
        directive: Option<FocusDirective>,
    },
    Rejected(RegistrationRejection),
}

#[derive(Default)]
struct PassCollector {
    registry: TargetRegistry,
    duplicate_ids: HashSet<TargetId>,
    pending_focus_intent: Option<FocusIntent>,
    consumed_focus_intent: Option<ConsumedFocusIntent>,
    closed: bool,
}

#[derive(Default)]
struct RootInputFrame {
    owner_blocked: bool,
    resolved: bool,
    capture_text_remainder: bool,
    captured_events: Vec<egui::Event>,
}

/// A per-surface adapter. Cloning a scope keeps only a handle to this frame's
/// collector and its value-only surface namespace.
#[derive(Clone)]
pub struct JumpModeScope {
    collector: Arc<Mutex<PassCollector>>,
    surface: SurfaceId,
    generation: TargetGeneration,
    identity_namespace: String,
    viewport: egui::ViewportId,
    epoch: NavigationEpoch,
    root_owner_allowed: bool,
}

impl JumpModeScope {
    pub(crate) fn child_namespace(&self, child: impl AsRef<str>) -> Self {
        let child = child.as_ref();
        let mut identity_namespace = self.identity_namespace.clone();
        identity_namespace.push('/');
        identity_namespace.push_str(&child.len().to_string());
        identity_namespace.push(':');
        identity_namespace.push_str(child);
        Self {
            identity_namespace,
            ..self.clone()
        }
    }

    pub(crate) fn with_surface(&self, surface: SurfaceId, generation: TargetGeneration) -> Self {
        Self {
            surface,
            generation,
            ..self.clone()
        }
    }

    pub(crate) fn target_id(&self, local_identity: impl AsRef<str>) -> TargetId {
        let mut semantic = self.identity_namespace.clone();
        semantic.push('/');
        let local_identity = local_identity.as_ref();
        semantic.push_str(&local_identity.len().to_string());
        semantic.push(':');
        semantic.push_str(local_identity);
        TargetId::new(
            self.surface.clone(),
            SemanticTargetId::named(semantic),
            self.generation,
        )
    }

    /// Register a visible control using its real egui response and current UI
    /// clip. The response itself is never stored in the registry.
    pub(crate) fn register(
        &self,
        ui: &egui::Ui,
        response: &egui::Response,
        local_identity: impl AsRef<str>,
        label: impl Into<String>,
        kind: TargetKind,
        visual_order: u64,
    ) -> RegistrationResult {
        let metadata =
            TargetMetadata::new(self.target_id(local_identity), label, kind, visual_order);
        self.register_metadata(ui, response, metadata)
    }

    pub(crate) fn register_metadata(
        &self,
        ui: &egui::Ui,
        response: &egui::Response,
        metadata: TargetMetadata,
    ) -> RegistrationResult {
        if metadata.id().surface() != &self.surface || metadata.id().generation() != self.generation
        {
            return RegistrationResult::Rejected(RegistrationRejection::IdentityOutsideScope);
        }

        let ctx = ui.ctx();
        if self.viewport != egui::ViewportId::ROOT
            || ctx.viewport_id() != self.viewport
            || response.ctx.viewport_id() != self.viewport
        {
            return RegistrationResult::Rejected(RegistrationRejection::WrongViewport);
        }
        if !ctx.input(|input| input.focused) {
            return RegistrationResult::Rejected(RegistrationRejection::UnfocusedViewport);
        }
        if is_root_surface(&self.surface)
            && (!self.root_owner_allowed || egui::Popup::is_any_open(ctx))
        {
            return RegistrationResult::Rejected(RegistrationRejection::InactiveOwner);
        }
        if !ctx.memory(|memory| memory.allows_interaction(response.layer_id)) {
            return RegistrationResult::Rejected(RegistrationRejection::BlockedLayer);
        }
        if !ui.is_enabled() || !response.enabled() {
            return RegistrationResult::Rejected(RegistrationRejection::Disabled);
        }
        if !response.sense.interactive() && !response.sense.is_focusable() {
            return RegistrationResult::Rejected(RegistrationRejection::NonInteractive);
        }

        let rect = response.rect;
        let clipped_rect = rect.intersect(ui.clip_rect());
        if !rect_is_usable(rect) || !rect_is_usable(clipped_rect) {
            return RegistrationResult::Rejected(RegistrationRejection::EmptyOrClipped);
        }
        let target = RegisteredTarget {
            metadata,
            viewport: self.viewport,
            response_id: response.id,
            layer_id: response.layer_id,
            rect,
            clipped_rect,
            enabled: true,
            eligible: true,
            epoch: self.epoch,
        };

        let Ok(mut collector) = self.collector.lock() else {
            return RegistrationResult::Rejected(RegistrationRejection::CollectorUnavailable);
        };
        if collector.closed {
            return RegistrationResult::Rejected(RegistrationRejection::FinishedPass);
        }
        let id = target.id().clone();
        if collector.duplicate_ids.contains(&id)
            || collector
                .registry
                .targets
                .iter()
                .any(|existing| existing.id() == &id)
        {
            collector
                .registry
                .targets
                .retain(|existing| existing.id() != &id);
            collector.duplicate_ids.insert(id);
            return RegistrationResult::Rejected(RegistrationRejection::DuplicateIdentity);
        }

        let directive = match collector.pending_focus_intent.clone() {
            Some(intent)
                if intent.target == id
                    && intent.viewport == target.viewport
                    && intent.epoch == target.epoch =>
            {
                collector.pending_focus_intent = None;
                collector.consumed_focus_intent = Some(ConsumedFocusIntent {
                    intent,
                    response_id: target.response_id,
                });
                Some(if target.metadata.kind() == TargetKind::Action {
                    FocusDirective::Select
                } else {
                    FocusDirective::Focus
                })
            }
            _ => None,
        };
        collector.registry.targets.push(target);

        RegistrationResult::Registered { id, directive }
    }
}

/// App-instance-owned coordinator. It separates the in-progress pass, the
/// completed pass, and the last fully completed frame snapshot.
#[derive(Default)]
pub(crate) struct JumpModeUi {
    enabled: bool,
    navigation_epoch: NavigationEpoch,
    active_session: Option<HintSession>,
    last_outcome: Option<MatchOutcome>,
    root_input_frame: RootInputFrame,
    held_captured_keys: HashSet<egui::Key>,
    pending_focus_intent: Option<FocusIntent>,
    collector: Option<Arc<Mutex<PassCollector>>>,
    pass_registry: Option<TargetRegistry>,
    retained_registry: TargetRegistry,
    frame_consumed_focus_intent: Option<ConsumedFocusIntent>,
}

impl JumpModeUi {
    pub(crate) fn new(enabled: bool) -> Self {
        Self {
            enabled,
            ..Self::default()
        }
    }

    pub(crate) fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub(crate) fn set_enabled(&mut self, enabled: bool) {
        if self.enabled == enabled {
            if !enabled {
                self.clear();
            }
            return;
        }
        self.enabled = enabled;
        if !enabled {
            self.clear();
        }
    }

    pub(crate) fn begin_frame(&mut self) {
        self.collector = None;
        self.pass_registry = None;
        self.frame_consumed_focus_intent = None;
        self.root_input_frame = RootInputFrame::default();
        if !self.enabled {
            self.clear();
        }
    }

    /// Resolve the root's first visible-frame input owner before any menu or
    /// widget can observe the events. The first pass performs transitions;
    /// later passes reapply the same event removals to egui's restored input.
    pub(crate) fn handle_root_input(
        &mut self,
        ctx: &egui::Context,
        root_owner_allowed: bool,
        first_ui_pass: bool,
    ) {
        if !root_owner_allowed {
            self.root_input_frame.owner_blocked = true;
            if !ctx.input(|input| input.focused) && self.active_session.is_some() {
                self.capture_unfocused_owner_loss_events(ctx);
            }
        }

        if first_ui_pass {
            if !root_owner_allowed {
                self.cancel_for_owner_loss();
            } else if self.enabled && !self.root_input_frame.owner_blocked {
                self.process_root_input_events(ctx);
            }
        }

        self.apply_root_input_capture(ctx);
    }

    pub(crate) fn active_session(&self) -> Option<&HintSession> {
        self.active_session
            .as_ref()
            .filter(|session| matches!(session.lifecycle(), SessionLifecycle::Active))
    }

    pub(crate) fn last_outcome(&self) -> Option<&MatchOutcome> {
        self.last_outcome.as_ref()
    }

    /// Start a new pass collector. Calling this again replaces all geometry
    /// collected by a pass that egui later discards.
    pub(crate) fn begin_ui_pass(&mut self) {
        self.pass_registry = None;
        self.collector = self.enabled.then(|| {
            Arc::new(Mutex::new(PassCollector {
                pending_focus_intent: self
                    .frame_consumed_focus_intent
                    .is_none()
                    .then(|| self.pending_focus_intent.clone())
                    .flatten(),
                ..PassCollector::default()
            }))
        });
    }

    pub(crate) fn scope(
        &self,
        surface: SurfaceId,
        generation: TargetGeneration,
        identity_namespace: impl Into<String>,
        viewport: egui::ViewportId,
        root_owner_allowed: bool,
    ) -> Option<JumpModeScope> {
        Some(JumpModeScope {
            collector: Arc::clone(self.collector.as_ref()?),
            surface,
            generation,
            identity_namespace: identity_namespace.into(),
            viewport,
            epoch: self.navigation_epoch,
            root_owner_allowed,
        })
    }

    pub(crate) fn finish_ui_pass(&mut self) {
        let Some(collector) = self.collector.take() else {
            self.pass_registry = None;
            return;
        };
        let Ok(mut collector) = collector.lock() else {
            self.pass_registry = None;
            return;
        };
        collector.closed = true;
        collector.registry.prefer_active_surface();
        if let Some(consumed) = collector.consumed_focus_intent.clone() {
            self.frame_consumed_focus_intent = Some(consumed);
        }
        self.pass_registry = Some(collector.registry.clone());
    }

    /// Retain only the final UI pass after egui has completed the whole frame.
    pub(crate) fn complete_frame(
        &mut self,
        ctx: &egui::Context,
        root_owner_allowed: bool,
        root_visible: bool,
    ) {
        if !self.enabled || !root_visible || ctx.viewport_id() != egui::ViewportId::ROOT {
            if let Some(consumed) = self.frame_consumed_focus_intent.as_ref() {
                ctx.memory_mut(|memory| memory.surrender_focus(consumed.response_id));
            }
            self.clear();
            return;
        }

        if !root_owner_allowed {
            self.cancel_for_owner_loss();
        }

        let mut completed = self.pass_registry.take().unwrap_or_default();
        completed.revalidate(ctx, root_owner_allowed);
        if root_owner_allowed && let Some(session) = self.active_session.as_mut() {
            let population_changed = session
                .reconcile_targets(completed.targets().iter().map(|target| target.id().clone()))
                .map(|change| change.requires_new_session())
                .unwrap_or(true);
            if population_changed {
                self.cancel_active_session();
            }
        }
        let pending_intent = self.pending_focus_intent.take();
        if let Some(consumed) = self.frame_consumed_focus_intent.as_ref() {
            let still_matches = pending_intent.as_ref() == Some(&consumed.intent)
                && completed.targets.iter().any(|target| {
                    target.id() == &consumed.intent.target
                        && target.viewport == consumed.intent.viewport
                        && target.epoch == consumed.intent.epoch
                });
            if !still_matches {
                ctx.memory_mut(|memory| memory.surrender_focus(consumed.response_id));
            }
        }
        self.retained_registry = completed;
        self.collector = None;
        self.frame_consumed_focus_intent = None;
    }

    pub(crate) fn retained_registry(&self) -> &TargetRegistry {
        &self.retained_registry
    }

    pub(crate) fn pass_registry(&self) -> Option<&TargetRegistry> {
        self.pass_registry.as_ref()
    }

    pub(crate) fn navigation_epoch(&self) -> NavigationEpoch {
        self.navigation_epoch
    }

    pub(crate) fn advance_navigation_epoch(&mut self) -> NavigationEpoch {
        self.navigation_epoch = self.navigation_epoch.next();
        self.pending_focus_intent = None;
        self.navigation_epoch
    }

    /// Queue a one-shot focus/selection intent for the next UI pass. Call this
    /// after `begin_frame` and before `begin_ui_pass`; resolution stays with
    /// the target owner and this bridge never invokes an action.
    pub(crate) fn queue_focus_intent(
        &mut self,
        target: TargetId,
        viewport: egui::ViewportId,
        epoch: NavigationEpoch,
    ) -> bool {
        if !self.enabled || self.collector.is_some() || self.pass_registry.is_some() {
            return false;
        }
        self.pending_focus_intent = Some(FocusIntent {
            target,
            viewport,
            epoch,
        });
        true
    }

    pub(crate) fn cancel_focus_intent(&mut self) {
        self.pending_focus_intent = None;
    }

    pub(crate) fn cancel_all(&mut self) {
        self.navigation_epoch = self.navigation_epoch.next();
        self.cancel_active_session();
        self.pending_focus_intent = None;
        self.collector = None;
        self.pass_registry = None;
        self.retained_registry = TargetRegistry::default();
        self.frame_consumed_focus_intent = None;
    }

    fn clear(&mut self) {
        self.active_session = None;
        self.last_outcome = None;
        self.root_input_frame = RootInputFrame::default();
        self.held_captured_keys.clear();
        self.pending_focus_intent = None;
        self.collector = None;
        self.pass_registry = None;
        self.retained_registry = TargetRegistry::default();
        self.frame_consumed_focus_intent = None;
    }

    pub(crate) fn root_input_owner_allowed(&self, root_owner_allowed: bool) -> bool {
        root_owner_allowed && !self.root_input_frame.owner_blocked
    }

    fn process_root_input_events(&mut self, ctx: &egui::Context) {
        let events = ctx.input(|input| input.events.clone());
        let input_modifiers = ctx.input(|input| input.modifiers);
        let paired_text_indices = paired_text_indices(&events);
        let mut captured_text_indices = HashSet::new();
        let mut captured_text_only_indices = HashSet::new();
        let mut shortcut_text_indices = HashSet::new();

        for (event_index, event) in events.iter().enumerate() {
            if matches!(
                event,
                egui::Event::Cut | egui::Event::Paste(_) | egui::Event::Ime(_)
            ) {
                if self.active_session.is_some()
                    || self.root_input_frame.resolved
                    || self.root_input_frame.capture_text_remainder
                {
                    self.capture_event(event);
                }
                continue;
            }

            if let egui::Event::Key {
                key,
                pressed,
                repeat,
                modifiers,
                ..
            } = event
            {
                let paired_text_index = paired_text_indices.get(&event_index).copied();
                if !pressed {
                    if self.held_captured_keys.remove(key) {
                        self.capture_event(event);
                    }
                    continue;
                }
                if self.held_captured_keys.contains(key) {
                    self.capture_event(event);
                    if let Some(text_index) = paired_text_index {
                        captured_text_indices.insert(text_index);
                        captured_text_only_indices.insert(text_index);
                    }
                    continue;
                }

                let leader = is_leader_key(*key, *modifiers);
                if leader && !self.root_input_frame.resolved {
                    self.capture_event(event);
                    self.held_captured_keys.insert(*key);
                    if let Some(text_index) = paired_text_index {
                        captured_text_indices.insert(text_index);
                        captured_text_only_indices.insert(text_index);
                    }
                    if !*repeat
                        && !self.root_input_frame.capture_text_remainder
                        && self.toggle_session()
                    {
                        self.root_input_frame.capture_text_remainder = true;
                    }
                    continue;
                }
                if matches!(*key, egui::Key::Enter | egui::Key::Space)
                    && (self.active_session.is_some()
                        || self.root_input_frame.resolved
                        || self.root_input_frame.capture_text_remainder)
                {
                    self.capture_event(event);
                    self.held_captured_keys.insert(*key);
                    if let Some(text_index) = paired_text_index {
                        captured_text_indices.insert(text_index);
                        captured_text_only_indices.insert(text_index);
                    }
                    continue;
                }

                if has_shortcut_modifier(*modifiers) {
                    if let Some(text_index) = paired_text_index {
                        shortcut_text_indices.insert(text_index);
                    }
                    continue;
                }

                if self.active_session.is_none()
                    && !self.root_input_frame.resolved
                    && !self.root_input_frame.capture_text_remainder
                {
                    continue;
                }

                // While Jump Mode owns the root, all plain key presses are
                // isolated from egui controls. Only the documented keys below
                // alter the hint session.
                self.capture_event(event);
                self.held_captured_keys.insert(*key);
                if let Some(text_index) = paired_text_index {
                    captured_text_indices.insert(text_index);
                    if *repeat {
                        captured_text_only_indices.insert(text_index);
                    }
                }
                if self.root_input_frame.resolved || self.root_input_frame.capture_text_remainder {
                    if matches!(*key, egui::Key::Enter | egui::Key::Space) {
                        continue;
                    }
                }
                if *repeat {
                    continue;
                }

                match key {
                    egui::Key::Escape if self.active_session.is_some() => {
                        self.cancel_active_session();
                        self.root_input_frame.capture_text_remainder = true;
                    }
                    egui::Key::Backspace if self.active_session.is_some() => {
                        self.update_session(HintSession::backspace);
                    }
                    egui::Key::Enter | egui::Key::Space => {}
                    _ => {
                        if paired_text_index.is_none()
                            && let Some(letter) = key_letter(*key)
                            && self.active_session.is_some()
                        {
                            self.push_hint_character(letter);
                        }
                    }
                }
                continue;
            }

            let egui::Event::Text(text) = event else {
                continue;
            };
            if shortcut_text_indices.contains(&event_index) {
                continue;
            }
            if captured_text_indices.contains(&event_index) {
                self.capture_event(event);
                if !captured_text_only_indices.contains(&event_index)
                    && self.active_session.is_some()
                    && !has_shortcut_modifier(input_modifiers)
                {
                    for character in text.chars() {
                        self.push_hint_character(character);
                    }
                }
                continue;
            }
            let active = self.active_session.is_some();
            if !active
                && !self.root_input_frame.resolved
                && !self.root_input_frame.capture_text_remainder
            {
                continue;
            }
            if has_shortcut_modifier(input_modifiers) {
                continue;
            }

            self.capture_event(event);
            if !active {
                continue;
            }

            for character in text.chars() {
                self.push_hint_character(character);
                if self.root_input_frame.resolved {
                    self.root_input_frame.capture_text_remainder = true;
                }
            }
        }
    }

    /// Returns true when the leader canceled an existing session.
    fn toggle_session(&mut self) -> bool {
        if let Some(mut session) = self.active_session.take() {
            self.last_outcome = Some(session.cancel());
            return true;
        }

        let map = HintMap::assign(self.retained_registry.metadata().cloned()).unwrap_or_default();
        self.last_outcome = Some(if map.is_empty() {
            MatchOutcome::NoTargets
        } else {
            MatchOutcome::Partial {
                matching_targets: map.len(),
            }
        });
        self.active_session = Some(HintSession::new(map));
        false
    }

    fn cancel_active_session(&mut self) {
        if let Some(mut session) = self.active_session.take() {
            self.last_outcome = Some(session.cancel());
        }
    }

    fn cancel_for_owner_loss(&mut self) {
        self.cancel_active_session();
        self.held_captured_keys.clear();
    }

    fn capture_unfocused_owner_loss_events(&mut self, ctx: &egui::Context) {
        let events = ctx.input(|input| input.events.clone());
        for event in events {
            self.capture_event(&event);
        }
    }

    fn update_session(&mut self, update: impl FnOnce(&mut HintSession) -> MatchOutcome) {
        let Some(session) = self.active_session.as_mut() else {
            return;
        };
        let outcome = update(session);
        self.last_outcome = Some(outcome);
    }

    fn push_hint_character(&mut self, character: char) {
        let Some(session) = self.active_session.as_mut() else {
            return;
        };
        let outcome = session.push_char(character);
        let completed = match &outcome {
            MatchOutcome::Completed(target) => Some(target.clone()),
            _ => None,
        };
        self.last_outcome = Some(outcome);
        if let Some(target) = completed {
            self.active_session = None;
            self.root_input_frame.resolved = true;
            let epoch = self.advance_navigation_epoch();
            if !self.queue_focus_intent(target, egui::ViewportId::ROOT, epoch) {
                self.last_outcome = Some(MatchOutcome::NoTarget);
            }
        }
    }

    fn capture_event(&mut self, event: &egui::Event) {
        if is_capturable_input_event(event) {
            self.root_input_frame.captured_events.push(event.clone());
        }
    }

    fn apply_root_input_capture(&self, ctx: &egui::Context) {
        if self.root_input_frame.captured_events.is_empty() {
            return;
        }
        ctx.input_mut(|input| {
            let mut remaining = self.root_input_frame.captured_events.clone();
            input.events.retain(|event| {
                if let Some(index) = remaining
                    .iter()
                    .position(|captured| same_keyboard_event(captured, event))
                {
                    remaining.remove(index);
                    false
                } else {
                    true
                }
            });
        });
    }
}

fn has_shortcut_modifier(modifiers: egui::Modifiers) -> bool {
    modifiers.ctrl || modifiers.alt || modifiers.command || modifiers.mac_cmd
}

fn is_leader_key(key: egui::Key, modifiers: egui::Modifiers) -> bool {
    key == egui::Key::Space && modifiers.shift && !has_shortcut_modifier(modifiers)
}

fn paired_text_indices(events: &[egui::Event]) -> std::collections::HashMap<usize, usize> {
    let mut pairs = std::collections::HashMap::new();
    for (key_index, event) in events.iter().enumerate() {
        if !matches!(event, egui::Event::Key { pressed: true, .. }) {
            continue;
        }
        for (index, following) in events.iter().enumerate().skip(key_index + 1) {
            match following {
                egui::Event::Text(_) => {
                    pairs.insert(key_index, index);
                    break;
                }
                egui::Event::Key { .. } => break,
                _ => {}
            }
        }
    }
    pairs
}

fn key_letter(key: egui::Key) -> Option<char> {
    Some(match key {
        egui::Key::A => 'A',
        egui::Key::B => 'B',
        egui::Key::C => 'C',
        egui::Key::D => 'D',
        egui::Key::E => 'E',
        egui::Key::F => 'F',
        egui::Key::G => 'G',
        egui::Key::H => 'H',
        egui::Key::I => 'I',
        egui::Key::J => 'J',
        egui::Key::K => 'K',
        egui::Key::L => 'L',
        egui::Key::M => 'M',
        egui::Key::N => 'N',
        egui::Key::O => 'O',
        egui::Key::P => 'P',
        egui::Key::Q => 'Q',
        egui::Key::R => 'R',
        egui::Key::S => 'S',
        egui::Key::T => 'T',
        egui::Key::U => 'U',
        egui::Key::V => 'V',
        egui::Key::W => 'W',
        egui::Key::X => 'X',
        egui::Key::Y => 'Y',
        egui::Key::Z => 'Z',
        _ => return None,
    })
}

fn same_keyboard_event(left: &egui::Event, right: &egui::Event) -> bool {
    match (left, right) {
        (
            egui::Event::Key {
                key: left_key,
                physical_key: left_physical,
                pressed: left_pressed,
                repeat: left_repeat,
                modifiers: left_modifiers,
                ..
            },
            egui::Event::Key {
                key: right_key,
                physical_key: right_physical,
                pressed: right_pressed,
                repeat: right_repeat,
                modifiers: right_modifiers,
                ..
            },
        ) => {
            left_key == right_key
                && left_physical == right_physical
                && left_pressed == right_pressed
                && left_repeat == right_repeat
                && left_modifiers == right_modifiers
        }
        (egui::Event::Text(left_text), egui::Event::Text(right_text)) => left_text == right_text,
        (egui::Event::Paste(left_text), egui::Event::Paste(right_text)) => left_text == right_text,
        (egui::Event::Ime(left_ime), egui::Event::Ime(right_ime)) => left_ime == right_ime,
        (egui::Event::Cut, egui::Event::Cut) => true,
        _ => false,
    }
}

fn is_capturable_input_event(event: &egui::Event) -> bool {
    matches!(
        event,
        egui::Event::Key { .. }
            | egui::Event::Text(_)
            | egui::Event::Cut
            | egui::Event::Paste(_)
            | egui::Event::Ime(_)
    )
}

fn rect_is_usable(rect: egui::Rect) -> bool {
    rect.min.x.is_finite()
        && rect.min.y.is_finite()
        && rect.max.x.is_finite()
        && rect.max.y.is_finite()
        && rect.width() > 0.0
        && rect.height() > 0.0
}

fn is_root_surface(surface: &SurfaceId) -> bool {
    matches!(surface, SurfaceId::Dashboard | SurfaceId::LauncherResults)
}

fn is_popup_surface(surface: &SurfaceId) -> bool {
    matches!(surface, SurfaceId::Popup { .. })
}

fn surface_priority(surface: &SurfaceId) -> u8 {
    match surface {
        SurfaceId::Popup { .. } => 3,
        SurfaceId::Dialog { .. } | SurfaceId::Settings { .. } => 2,
        SurfaceId::Dashboard | SurfaceId::LauncherResults => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input_context(run: impl FnMut(&mut egui::Ui)) {
        let ctx = egui::Context::default();
        ctx.run_ui(egui::RawInput::default(), run)
            .drop_without_applying_deltas();
    }

    fn setup(enabled: bool) -> (JumpModeUi, JumpModeScope) {
        let mut jump = JumpModeUi::new(enabled);
        jump.begin_frame();
        jump.begin_ui_pass();
        let scope = jump
            .scope(
                SurfaceId::LauncherResults,
                TargetGeneration(1),
                "test",
                egui::ViewportId::ROOT,
                true,
            )
            .expect("enabled bridge creates a render scope");
        (jump, scope)
    }

    fn root_input_raw(events: Vec<egui::Event>) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 650.0),
            )),
            focused: true,
            events,
            ..Default::default()
        }
    }

    fn register_root_input_targets(jump: &mut JumpModeUi, ctx: &egui::Context, count: usize) {
        jump.begin_frame();
        jump.begin_ui_pass();
        let scope = jump
            .scope(
                SurfaceId::LauncherResults,
                TargetGeneration(1),
                "input-test",
                egui::ViewportId::ROOT,
                true,
            )
            .expect("enabled bridge creates a render scope");
        ctx.run_ui(root_input_raw(Vec::new()), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                ui.columns(3, |columns| {
                    for index in 0..count {
                        let label = format!("Target {index}");
                        let column = &mut columns[index % 3];
                        let response = column.button(&label);
                        let registration = scope.register(
                            column,
                            &response,
                            format!("target-{index}"),
                            label,
                            TargetKind::Button,
                            index as u64,
                        );
                        assert!(
                            matches!(registration, RegistrationResult::Registered { .. }),
                            "target {index} registration rejected: {registration:?}"
                        );
                    }
                });
            });
        })
        .drop_without_applying_deltas();
        jump.finish_ui_pass();
        jump.complete_frame(ctx, true, true);
    }

    fn run_root_input_frame(
        jump: &mut JumpModeUi,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> Vec<egui::Event> {
        let mut remaining = Vec::new();
        ctx.run_ui(root_input_raw(events), |ui| {
            jump.begin_frame();
            jump.handle_root_input(ctx, true, true);
            remaining = ctx.input(|input| input.events.clone());
            let _ = ui;
        })
        .drop_without_applying_deltas();
        remaining
    }

    fn key_event(
        key: egui::Key,
        pressed: bool,
        repeat: bool,
        modifiers: egui::Modifiers,
    ) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat,
            modifiers,
        }
    }

    fn key_for_letter(character: char) -> egui::Key {
        match character.to_ascii_uppercase() {
            'A' => egui::Key::A,
            'B' => egui::Key::B,
            'C' => egui::Key::C,
            'D' => egui::Key::D,
            'E' => egui::Key::E,
            'F' => egui::Key::F,
            'G' => egui::Key::G,
            'H' => egui::Key::H,
            'I' => egui::Key::I,
            'J' => egui::Key::J,
            'K' => egui::Key::K,
            'L' => egui::Key::L,
            'M' => egui::Key::M,
            'N' => egui::Key::N,
            'O' => egui::Key::O,
            'P' => egui::Key::P,
            'Q' => egui::Key::Q,
            'R' => egui::Key::R,
            'S' => egui::Key::S,
            'T' => egui::Key::T,
            'U' => egui::Key::U,
            'V' => egui::Key::V,
            'W' => egui::Key::W,
            'X' => egui::Key::X,
            'Y' => egui::Key::Y,
            'Z' => egui::Key::Z,
            _ => panic!("test key must be an ASCII letter"),
        }
    }

    #[test]
    fn jump_mode_root_input_deduplicates_text_repeats_and_cancels_without_leaks() {
        let ctx = egui::Context::default();
        let mut jump = JumpModeUi::new(true);
        register_root_input_targets(&mut jump, &ctx, 27);
        let leader = egui::Modifiers::SHIFT;

        assert!(
            run_root_input_frame(
                &mut jump,
                &ctx,
                vec![
                    key_event(egui::Key::Space, true, false, leader),
                    egui::Event::Text(" ".into()),
                ],
            )
            .is_empty()
        );
        let active = jump.active_session().expect("leader starts a session");
        assert_eq!(active.hint_map().len(), 27);
        let two_letter_hint = active
            .hint_map()
            .assignments()
            .iter()
            .find(|assignment| assignment.hint.as_str().len() == 2)
            .expect("27 targets allocate one two-letter hint")
            .hint
            .as_str()
            .to_owned();
        let first_letter = two_letter_hint.chars().next().unwrap();

        assert!(
            run_root_input_frame(
                &mut jump,
                &ctx,
                vec![key_event(egui::Key::Space, false, false, leader)],
            )
            .is_empty()
        );
        assert!(
            run_root_input_frame(
                &mut jump,
                &ctx,
                vec![
                    key_event(
                        key_for_letter(first_letter),
                        true,
                        false,
                        egui::Modifiers::NONE,
                    ),
                    egui::Event::Text(first_letter.to_ascii_lowercase().to_string()),
                ],
            )
            .is_empty()
        );
        assert_eq!(
            jump.active_session().unwrap().prefix(),
            first_letter.to_string(),
            "paired Key and Text input contributes one hint character"
        );

        assert!(
            run_root_input_frame(
                &mut jump,
                &ctx,
                vec![key_event(
                    key_for_letter(first_letter),
                    false,
                    false,
                    egui::Modifiers::NONE,
                )],
            )
            .is_empty()
        );
        assert!(
            run_root_input_frame(
                &mut jump,
                &ctx,
                vec![
                    key_event(
                        key_for_letter(first_letter),
                        true,
                        true,
                        egui::Modifiers::NONE,
                    ),
                    egui::Event::Text(first_letter.to_ascii_lowercase().to_string()),
                ],
            )
            .is_empty()
        );
        assert_eq!(
            jump.active_session().unwrap().prefix(),
            first_letter.to_string(),
            "autorepeat does not advance the prefix"
        );

        let control_a = egui::Modifiers::CTRL;
        let remaining = run_root_input_frame(
            &mut jump,
            &ctx,
            vec![
                key_event(egui::Key::A, true, false, control_a),
                egui::Event::Text("a".into()),
                key_event(egui::Key::A, false, false, egui::Modifiers::NONE),
                key_event(egui::Key::ControlLeft, false, false, egui::Modifiers::NONE),
            ],
        );
        assert_eq!(
            remaining.len(),
            4,
            "shortcut-owned key, text, and unowned releases remain visible"
        );
        assert_eq!(
            jump.active_session().unwrap().prefix(),
            first_letter.to_string()
        );

        assert!(
            run_root_input_frame(&mut jump, &ctx, vec![egui::Event::Text("1".into())],).is_empty()
        );
        assert_eq!(
            jump.active_session().unwrap().prefix(),
            first_letter.to_string(),
            "invalid input leaves the prior valid prefix unchanged"
        );
        assert_eq!(jump.last_outcome(), Some(&MatchOutcome::Invalid));

        assert!(
            run_root_input_frame(
                &mut jump,
                &ctx,
                vec![key_event(
                    key_for_letter(first_letter),
                    false,
                    false,
                    egui::Modifiers::NONE,
                )],
            )
            .is_empty()
        );
        // Exercise Backspace after building a prefix again.
        assert!(
            run_root_input_frame(
                &mut jump,
                &ctx,
                vec![key_event(
                    key_for_letter(first_letter),
                    true,
                    false,
                    egui::Modifiers::NONE,
                )],
            )
            .is_empty()
        );
        assert_eq!(
            jump.active_session().unwrap().prefix(),
            first_letter.to_string()
        );
        assert!(
            run_root_input_frame(
                &mut jump,
                &ctx,
                vec![key_event(
                    egui::Key::Backspace,
                    true,
                    false,
                    egui::Modifiers::NONE,
                )],
            )
            .is_empty()
        );
        assert_eq!(jump.active_session().unwrap().prefix(), "");

        assert!(
            run_root_input_frame(
                &mut jump,
                &ctx,
                vec![key_event(
                    key_for_letter(first_letter),
                    false,
                    false,
                    egui::Modifiers::NONE,
                )],
            )
            .is_empty()
        );

        assert!(
            run_root_input_frame(
                &mut jump,
                &ctx,
                vec![
                    key_event(
                        key_for_letter(first_letter),
                        true,
                        false,
                        egui::Modifiers::NONE,
                    ),
                    egui::Event::Text("\u{00e5}".into()),
                ],
            )
            .is_empty()
        );
        assert_eq!(
            jump.active_session().unwrap().prefix(),
            "",
            "a composed non-ASCII Text event cannot resolve from its physical key"
        );

        assert!(
            run_root_input_frame(
                &mut jump,
                &ctx,
                vec![
                    key_event(egui::Key::Escape, true, false, egui::Modifiers::NONE,),
                    egui::Event::Text("z".into()),
                ],
            )
            .is_empty()
        );
        assert!(jump.active_session().is_none());
        assert_eq!(jump.last_outcome(), Some(&MatchOutcome::Cancelled));

        assert!(
            run_root_input_frame(
                &mut jump,
                &ctx,
                vec![key_event(
                    egui::Key::Escape,
                    false,
                    false,
                    egui::Modifiers::NONE,
                )],
            )
            .is_empty()
        );
        assert!(
            run_root_input_frame(
                &mut jump,
                &ctx,
                vec![
                    key_event(egui::Key::Space, true, false, leader),
                    egui::Event::Text(" ".into()),
                ],
            )
            .is_empty()
        );
        assert!(jump.active_session().is_some());
        assert!(
            run_root_input_frame(
                &mut jump,
                &ctx,
                vec![
                    key_event(egui::Key::Space, true, true, leader),
                    egui::Event::Text(" ".into()),
                ],
            )
            .is_empty()
        );
        assert!(
            jump.active_session().is_some(),
            "leader repeat does not toggle"
        );
        assert!(
            run_root_input_frame(
                &mut jump,
                &ctx,
                vec![key_event(egui::Key::Space, false, false, leader)],
            )
            .is_empty()
        );
        assert!(
            run_root_input_frame(
                &mut jump,
                &ctx,
                vec![
                    key_event(egui::Key::Space, true, false, leader),
                    egui::Event::Text(" ".into()),
                ],
            )
            .is_empty()
        );
        assert!(jump.active_session().is_none(), "second leader cancels");
    }

    fn named_scope_target(
        surface: SurfaceId,
        generation: TargetGeneration,
        namespace: &str,
        local_identity: &str,
    ) -> TargetId {
        TargetId::new(
            surface,
            SemanticTargetId::named(format!(
                "{namespace}/{}:{local_identity}",
                local_identity.len()
            )),
            generation,
        )
    }

    fn register_button(
        ui: &mut egui::Ui,
        scope: &JumpModeScope,
        local_id: &str,
        label: &str,
        kind: TargetKind,
    ) -> RegistrationResult {
        let response = ui.button(label);
        scope.register(ui, &response, local_id, label, kind, 0)
    }

    #[test]
    fn registry_keeps_equal_labels_distinct_and_rejects_duplicate_ids() {
        let (mut jump, scope) = setup(true);
        input_context(|ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                assert!(matches!(
                    register_button(ui, &scope, "first", "Same label", TargetKind::Button),
                    RegistrationResult::Registered { .. }
                ));
                assert!(matches!(
                    register_button(ui, &scope, "second", "Same label", TargetKind::Button),
                    RegistrationResult::Registered { .. }
                ));
                assert_eq!(
                    register_button(ui, &scope, "first", "Duplicate", TargetKind::Button),
                    RegistrationResult::Rejected(RegistrationRejection::DuplicateIdentity)
                );
            });
        });
        jump.finish_ui_pass();
        let registry = jump.pass_registry().expect("pass was collected");
        assert_eq!(registry.len(), 1);
        assert_eq!(registry.targets()[0].metadata().label(), "Same label");
        assert!(registry.targets()[0].eligible());
    }

    #[test]
    fn registry_excludes_disabled_clipped_zero_area_blocked_and_wrong_viewport_targets() {
        let (mut jump, scope) = setup(true);
        let wrong_viewport_scope =
            scope.with_surface(SurfaceId::dialog("wrong-viewport", 1), TargetGeneration(1));
        let wrong_viewport_scope = JumpModeScope {
            viewport: egui::ViewportId::from_hash_of("other viewport"),
            ..wrong_viewport_scope
        };
        let context = egui::Context::default();
        context
            .run_ui(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.add_enabled_ui(false, |ui| {
                        assert_eq!(
                            register_button(ui, &scope, "disabled", "Disabled", TargetKind::Button),
                            RegistrationResult::Rejected(RegistrationRejection::Disabled)
                        );
                    });

                    let mut clipped_ui =
                        ui.new_child(egui::UiBuilder::new().max_rect(ui.max_rect()));
                    clipped_ui.set_clip_rect(egui::Rect::from_min_max(
                        egui::pos2(0.0, 0.0),
                        egui::pos2(0.0, 0.0),
                    ));
                    assert_eq!(
                        register_button(
                            &mut clipped_ui,
                            &scope,
                            "clipped",
                            "Clipped",
                            TargetKind::Button,
                        ),
                        RegistrationResult::Rejected(RegistrationRejection::EmptyOrClipped)
                    );

                    let zero = ui.allocate_response(egui::Vec2::ZERO, egui::Sense::click());
                    assert_eq!(
                        scope.register(ui, &zero, "zero", "Zero", TargetKind::Button, 2),
                        RegistrationResult::Rejected(RegistrationRejection::EmptyOrClipped)
                    );

                    let hover_only = ui.add(
                        egui::Label::new("Hover only")
                            .selectable(false)
                            .sense(egui::Sense::hover()),
                    );
                    assert_eq!(
                        scope.register(
                            ui,
                            &hover_only,
                            "hover-only",
                            "Hover only",
                            TargetKind::Button,
                            3,
                        ),
                        RegistrationResult::Rejected(RegistrationRejection::NonInteractive)
                    );

                    let response = ui.button("Wrong viewport");
                    assert_eq!(
                        wrong_viewport_scope.register(
                            ui,
                            &response,
                            "other",
                            "Other",
                            TargetKind::Button,
                            4
                        ),
                        RegistrationResult::Rejected(RegistrationRejection::WrongViewport)
                    );
                });
            })
            .drop_without_applying_deltas();

        let modal_layer =
            egui::LayerId::new(egui::Order::Foreground, egui::Id::new("active modal"));
        context
            .run_ui(egui::RawInput::default(), |ui| {
                ui.ctx()
                    .memory_mut(|memory| memory.set_modal_layer(modal_layer));
            })
            .drop_without_applying_deltas();
        let blocked_layer =
            egui::LayerId::new(egui::Order::Background, egui::Id::new("background target"));
        context
            .run_ui(egui::RawInput::default(), |ui| {
                let mut blocked_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(ui.max_rect())
                        .layer_id(blocked_layer),
                );
                let response = blocked_ui.button("Behind modal");
                assert_eq!(
                    scope.register(
                        &blocked_ui,
                        &response,
                        "blocked",
                        "Behind modal",
                        TargetKind::Button,
                        4,
                    ),
                    RegistrationResult::Rejected(RegistrationRejection::BlockedLayer)
                );
            })
            .drop_without_applying_deltas();
        jump.finish_ui_pass();
        assert!(jump.pass_registry().unwrap().is_empty());
    }

    #[test]
    fn popup_surface_precedes_dialog_and_root_targets() {
        let (mut jump, root) = setup(true);
        let dialog = root.with_surface(SurfaceId::dialog("settings", 3), TargetGeneration(7));
        let popup = dialog.with_surface(
            SurfaceId::popup("choice", 4, SurfaceId::dialog("settings", 3)),
            TargetGeneration(9),
        );
        input_context(|ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                register_button(ui, &root, "root", "Equal", TargetKind::Button);
                register_button(ui, &dialog, "dialog", "Equal", TargetKind::Button);
                register_button(ui, &popup, "popup", "Equal", TargetKind::Button);
            });
        });
        jump.finish_ui_pass();
        let targets = jump.pass_registry().unwrap().targets();
        assert_eq!(targets.len(), 1);
        assert!(matches!(targets[0].id().surface(), SurfaceId::Popup { .. }));
    }

    #[test]
    fn dashboard_and_launcher_results_share_the_root_owner() {
        let (mut jump, root) = setup(true);
        let dashboard = root.with_surface(SurfaceId::Dashboard, TargetGeneration(4));
        input_context(|ui| {
            register_button(ui, &root, "result", "Root result", TargetKind::Button);
            register_button(
                ui,
                &dashboard,
                "dashboard",
                "Dashboard widget",
                TargetKind::Button,
            );
        });
        jump.finish_ui_pass();

        let targets = jump.pass_registry().unwrap().targets();
        assert_eq!(targets.len(), 2);
        assert!(
            targets
                .iter()
                .any(|target| matches!(target.id().surface(), SurfaceId::LauncherResults))
        );
        assert!(
            targets
                .iter()
                .any(|target| matches!(target.id().surface(), SurfaceId::Dashboard))
        );
    }

    #[test]
    fn ambiguous_same_priority_dialogs_and_nested_popups_fail_closed() {
        let (mut jump, root) = setup(true);
        let first_dialog = root.with_surface(SurfaceId::dialog("notes", 1), TargetGeneration(1));
        let second_dialog =
            root.with_surface(SurfaceId::dialog("settings", 2), TargetGeneration(1));
        input_context(|ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                register_button(ui, &root, "root", "Root", TargetKind::Button);
                register_button(
                    ui,
                    &first_dialog,
                    "first",
                    "First dialog",
                    TargetKind::Button,
                );
                register_button(
                    ui,
                    &second_dialog,
                    "second",
                    "Second dialog",
                    TargetKind::Button,
                );
            });
        });
        jump.finish_ui_pass();
        assert!(jump.pass_registry().unwrap().is_empty());

        let (mut jump, root) = setup(true);
        let dialog = root.with_surface(SurfaceId::dialog("notes", 1), TargetGeneration(1));
        let parent_popup = dialog.with_surface(
            SurfaceId::popup("parent", 1, SurfaceId::dialog("notes", 1)),
            TargetGeneration(1),
        );
        let nested_popup = parent_popup.with_surface(
            SurfaceId::popup(
                "nested",
                2,
                SurfaceId::popup("parent", 1, SurfaceId::dialog("notes", 1)),
            ),
            TargetGeneration(1),
        );
        input_context(|ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                register_button(ui, &root, "root", "Root", TargetKind::Button);
                register_button(
                    ui,
                    &parent_popup,
                    "parent",
                    "Parent popup",
                    TargetKind::Button,
                );
                register_button(
                    ui,
                    &nested_popup,
                    "nested",
                    "Nested popup",
                    TargetKind::Button,
                );
            });
        });
        jump.finish_ui_pass();
        assert!(jump.pass_registry().unwrap().is_empty());
    }

    #[test]
    fn a_real_response_receives_focus_once_without_clicking_or_activating() {
        let mut jump = JumpModeUi::new(true);
        jump.begin_frame();
        let target_id = named_scope_target(
            SurfaceId::LauncherResults,
            TargetGeneration(1),
            "test",
            "button",
        );
        let epoch = jump.navigation_epoch();
        assert!(jump.queue_focus_intent(target_id.clone(), egui::ViewportId::ROOT, epoch));
        jump.begin_ui_pass();
        let scope = jump
            .scope(
                SurfaceId::LauncherResults,
                TargetGeneration(1),
                "test",
                egui::ViewportId::ROOT,
                true,
            )
            .unwrap();
        let ctx = egui::Context::default();
        let mut clicked = false;
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let response = ui.button("Focus target");
            let result = scope.register(
                ui,
                &response,
                "button",
                "Focus target",
                TargetKind::Button,
                0,
            );
            assert_eq!(
                result,
                RegistrationResult::Registered {
                    id: target_id.clone(),
                    directive: Some(FocusDirective::Focus),
                }
            );
            if let RegistrationResult::Registered {
                directive: Some(directive),
                ..
            } = result
            {
                directive.apply_to_response(&response);
            }
            clicked = response.clicked();
        });
        assert!(!clicked);
        jump.finish_ui_pass();
        jump.complete_frame(&ctx, true, true);
        assert_eq!(
            ctx.memory(|memory| memory.focused()),
            jump.retained_registry()
                .targets()
                .first()
                .map(RegisteredTarget::response_id)
        );
        assert!(jump.pending_focus_intent.is_none());
        let has_shapes = !output.shapes.is_empty();
        output.drop_without_applying_deltas();
        assert!(has_shapes);
    }

    #[test]
    fn action_targets_request_logical_selection_without_running_actions() {
        let mut jump = JumpModeUi::new(true);
        jump.begin_frame();
        let intent = named_scope_target(
            SurfaceId::LauncherResults,
            TargetGeneration(1),
            "test",
            "card",
        );
        jump.queue_focus_intent(intent, egui::ViewportId::ROOT, jump.navigation_epoch());
        jump.begin_ui_pass();
        let scope = jump
            .scope(
                SurfaceId::LauncherResults,
                TargetGeneration(1),
                "test",
                egui::ViewportId::ROOT,
                true,
            )
            .unwrap();
        let mut actions_run = 0;
        input_context(|ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let result = register_button(ui, &scope, "card", "Action card", TargetKind::Action);
                assert_eq!(
                    result,
                    RegistrationResult::Registered {
                        id: scope.target_id("card"),
                        directive: Some(FocusDirective::Select),
                    }
                );
                if let RegistrationResult::Registered {
                    directive: Some(FocusDirective::Select),
                    ..
                } = result
                {
                    // The owning surface will update selection in a later milestone.
                    actions_run += 0;
                }
            });
        });
        jump.finish_ui_pass();
        assert_eq!(actions_run, 0);
    }

    #[test]
    fn generation_owner_and_epoch_mismatches_fail_closed_and_consume_once() {
        let mut jump = JumpModeUi::new(true);
        jump.begin_frame();
        let old_id = named_scope_target(
            SurfaceId::LauncherResults,
            TargetGeneration(1),
            "test",
            "target",
        );
        let epoch = jump.navigation_epoch();
        jump.queue_focus_intent(old_id.clone(), egui::ViewportId::ROOT, epoch);
        jump.begin_ui_pass();
        let root_scope = jump
            .scope(
                SurfaceId::LauncherResults,
                TargetGeneration(1),
                "test",
                egui::ViewportId::ROOT,
                true,
            )
            .unwrap();
        assert_eq!(root_scope.target_id("target"), old_id);

        // A newer pass replaces the old pass geometry without inheriting it.
        jump.finish_ui_pass();
        jump.begin_ui_pass();
        let new_scope = jump
            .scope(
                SurfaceId::LauncherResults,
                TargetGeneration(2),
                "test",
                egui::ViewportId::ROOT,
                true,
            )
            .unwrap();
        let mut stale_directive = None;
        input_context(|ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                if let RegistrationResult::Registered { directive, .. } = register_button(
                    ui,
                    &new_scope,
                    "target",
                    "New generation",
                    TargetKind::Button,
                ) {
                    stale_directive = directive;
                }
            });
        });
        assert_eq!(stale_directive, None);
        jump.finish_ui_pass();
        assert_eq!(jump.pass_registry().unwrap().len(), 1);
        let ctx = egui::Context::default();
        jump.complete_frame(&ctx, true, true);
        assert!(jump.pending_focus_intent.is_none());

        // A target on another owner and a target from an older navigation epoch
        // cannot consume a newly queued intent.
        let mut jump = JumpModeUi::new(true);
        jump.begin_frame();
        let epoch = jump.navigation_epoch();
        let expected = named_scope_target(
            SurfaceId::LauncherResults,
            TargetGeneration(1),
            "test",
            "target",
        );
        jump.queue_focus_intent(expected, egui::ViewportId::ROOT, epoch);
        jump.begin_ui_pass();
        let root_scope = jump
            .scope(
                SurfaceId::LauncherResults,
                TargetGeneration(1),
                "test",
                egui::ViewportId::ROOT,
                true,
            )
            .unwrap();
        let wrong_owner =
            root_scope.with_surface(SurfaceId::dialog("notes", 1), TargetGeneration(1));
        let mut wrong_owner_directive = None;
        input_context(|ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                if let RegistrationResult::Registered { directive, .. } = register_button(
                    ui,
                    &wrong_owner,
                    "target",
                    "Same local ID",
                    TargetKind::Button,
                ) {
                    wrong_owner_directive = directive;
                }
            });
        });
        assert_eq!(wrong_owner_directive, None);
        jump.finish_ui_pass();
        jump.complete_frame(&egui::Context::default(), true, true);
        assert!(jump.pending_focus_intent.is_none());

        let mut jump = JumpModeUi::new(true);
        jump.begin_frame();
        let stale_epoch = jump.navigation_epoch();
        jump.advance_navigation_epoch();
        let stale_target = named_scope_target(
            SurfaceId::LauncherResults,
            TargetGeneration(1),
            "test",
            "target",
        );
        assert!(jump.queue_focus_intent(stale_target, egui::ViewportId::ROOT, stale_epoch));
        jump.begin_ui_pass();
        let current_scope = jump
            .scope(
                SurfaceId::LauncherResults,
                TargetGeneration(1),
                "test",
                egui::ViewportId::ROOT,
                true,
            )
            .unwrap();
        let mut old_epoch_directive = None;
        input_context(|ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                if let RegistrationResult::Registered { directive, .. } =
                    register_button(ui, &current_scope, "target", "Target", TargetKind::Button)
                {
                    old_epoch_directive = directive;
                }
            });
        });
        assert_eq!(old_epoch_directive, None);
        jump.finish_ui_pass();
        jump.complete_frame(&egui::Context::default(), true, true);
        assert!(jump.pending_focus_intent.is_none());
    }

    #[test]
    fn a_new_ui_pass_replaces_geometry_and_only_the_completed_pass_is_retained() {
        let mut jump = JumpModeUi::new(true);
        jump.begin_frame();
        jump.begin_ui_pass();
        let first_scope = jump
            .scope(
                SurfaceId::LauncherResults,
                TargetGeneration(1),
                "results",
                egui::ViewportId::ROOT,
                true,
            )
            .unwrap();
        let ctx = egui::Context::default();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            register_button(
                ui,
                &first_scope,
                "old",
                "Discarded pass",
                TargetKind::Button,
            );
        })
        .drop_without_applying_deltas();
        jump.finish_ui_pass();
        assert_eq!(jump.pass_registry().unwrap().len(), 1);

        jump.begin_ui_pass();
        let final_scope = jump
            .scope(
                SurfaceId::LauncherResults,
                TargetGeneration(2),
                "results",
                egui::ViewportId::ROOT,
                true,
            )
            .unwrap();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            register_button(ui, &final_scope, "new", "Final pass", TargetKind::Button);
        })
        .drop_without_applying_deltas();
        jump.finish_ui_pass();

        let final_registry = jump.pass_registry().unwrap();
        assert_eq!(final_registry.len(), 1);
        assert_eq!(final_registry.targets()[0].metadata().label(), "Final pass");
        assert_eq!(
            final_registry.targets()[0].id().generation(),
            TargetGeneration(2)
        );
        let expected_retained = final_registry.clone();

        jump.complete_frame(&ctx, true, true);
        assert_eq!(jump.retained_registry(), &expected_retained);
    }

    #[test]
    fn disabled_bridge_has_no_scope_or_registry_and_clears_retained_state() {
        let mut jump = JumpModeUi::new(false);
        jump.begin_frame();
        jump.begin_ui_pass();
        assert!(
            jump.scope(
                SurfaceId::LauncherResults,
                TargetGeneration(1),
                "test",
                egui::ViewportId::ROOT,
                true,
            )
            .is_none()
        );
        assert!(jump.pass_registry().is_none());
        assert!(jump.retained_registry().is_empty());
        jump.queue_focus_intent(
            TargetId::root_result(TargetGeneration(1), 0),
            egui::ViewportId::ROOT,
            jump.navigation_epoch(),
        );
        assert!(jump.pending_focus_intent.is_none());
    }

    #[test]
    fn focus_intents_are_rejected_after_a_ui_pass_starts() {
        let (mut jump, scope) = setup(true);
        assert!(!jump.queue_focus_intent(
            scope.target_id("too-late"),
            egui::ViewportId::ROOT,
            jump.navigation_epoch(),
        ));
        assert!(jump.pending_focus_intent.is_none());
    }
}
